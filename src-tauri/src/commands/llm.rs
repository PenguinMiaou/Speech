use crate::credentials::{resolve_config_secret, SystemCredentialVault};
use crate::llm::protocol::LlmApiKind;

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LlmBenchResult {
    latency_ms: u32,
    reasoning_active: bool,
}

fn build_fetch_models_request(
    client: &reqwest::Client,
    kind: LlmApiKind,
    provider: &str,
    api_key: &str,
    url: &str,
) -> reqwest::RequestBuilder {
    crate::llm::protocol::apply_auth_headers(kind, client.get(url), provider, api_key)
}

fn models_http_error(status: u16, body: &str) -> String {
    let snippet: String = body.trim().chars().take(300).collect();
    format!("HTTP {status}: {snippet}")
}

#[tauri::command]
pub async fn fetch_llm_models(
    api_key: String,
    provider: String,
    base_url: String,
    api_format: String,
    client: tauri::State<'_, reqwest::Client>,
) -> Result<Vec<String>, String> {
    if base_url.is_empty() {
        return Ok(vec![]);
    }
    if !crate::llm::has_usable_provider_credentials(&provider, &api_key) {
        return Ok(vec![]);
    }

    // Validate base_url is a proper HTTP(S) URL
    let parsed = url::Url::parse(&base_url).map_err(|e| format!("Invalid base URL: {e}"))?;
    if parsed.scheme() != "https" && parsed.scheme() != "http" {
        return Err("Base URL must use http or https scheme".to_string());
    }

    let kind = crate::llm::protocol::resolve_api_kind(&api_format, &provider, &base_url);
    let url = crate::llm::protocol::models_endpoint(kind, &base_url)?;

    let resp = build_fetch_models_request(&client, kind, &provider, &api_key, &url)
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await
        .map_err(|e| e.to_string())?;

    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(models_http_error(status.as_u16(), &body));
    }

    let body: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;

    // OpenAI-compatible: { data: [{ id: "model-name" }] }
    // Ollama-compatible: { models: [{ name: "model-name" }] }
    let mut models: Vec<String> = Vec::new();

    if let Some(data) = body.get("data").and_then(|d| d.as_array()) {
        for item in data {
            if let Some(id) = item.get("id").and_then(|v| v.as_str()) {
                models.push(id.to_string());
            }
        }
    } else if let Some(data) = body.get("models").and_then(|d| d.as_array()) {
        for item in data {
            if let Some(name) = item.get("name").and_then(|v| v.as_str()) {
                models.push(name.to_string());
            }
        }
    }

    models.sort();
    Ok(models)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_failed_model_list_reports_the_status_and_a_bounded_body() {
        let message = models_http_error(401, &"x".repeat(1000));
        assert!(message.starts_with("HTTP 401: x"));
        assert!(message.chars().count() < 320);
    }

    #[test]
    fn model_request_omits_authorization_for_keyless_ollama() {
        let request = build_fetch_models_request(
            &reqwest::Client::new(),
            LlmApiKind::OpenAiCompatible,
            "ollama",
            "",
            "http://localhost:11434/v1/models",
        )
        .build()
        .unwrap();

        assert!(request.headers().get("Authorization").is_none());
    }

    #[test]
    fn model_request_keeps_authorization_for_keyed_providers() {
        let request = build_fetch_models_request(
            &reqwest::Client::new(),
            LlmApiKind::OpenAiCompatible,
            "openai",
            "sk-test",
            "https://api.openai.com/v1/models",
        )
        .build()
        .unwrap();

        assert_eq!(
            request.headers().get("Authorization").unwrap(),
            "Bearer sk-test"
        );
    }
}

#[tauri::command]
pub async fn bench_llm_connection(
    api_key: String,
    provider: String,
    base_url: String,
    model: String,
    api_format: String,
    client: tauri::State<'_, reqwest::Client>,
) -> Result<LlmBenchResult, String> {
    if provider.is_empty() {
        return Err("No provider specified".to_string());
    }

    let api_key = resolve_config_secret(&api_key, "llm", &provider, &SystemCredentialVault)
        .map_err(|e| e.to_string())?;

    if base_url.is_empty() || !crate::llm::has_usable_provider_credentials(&provider, &api_key) {
        return Err("API key or base URL is empty".to_string());
    }

    let parsed = url::Url::parse(&base_url).map_err(|e| format!("Invalid base URL: {e}"))?;
    if parsed.scheme() != "https" && parsed.scheme() != "http" {
        return Err("Base URL must use http or https scheme".to_string());
    }

    let kind = crate::llm::protocol::resolve_api_kind(&api_format, &provider, &base_url);
    let url = crate::llm::protocol::chat_endpoint(kind, &base_url)?;
    let mut body = crate::llm::protocol::build_chat_body(
        kind,
        &provider,
        &base_url,
        &model,
        vec![serde_json::json!({"role": "user", "content": "hi"})],
        1,
        0.3,
        false,
    );

    let t0 = std::time::Instant::now();
    let resp = crate::llm::protocol::send_with_reasoning_fallback(
        |body| {
            let request = client.post(&url).header("Content-Type", "application/json");
            crate::llm::protocol::apply_auth_headers(kind, request, &provider, &api_key)
                .json(body)
                .timeout(crate::llm::protocol::request_timeout(kind, &model))
        },
        &mut body,
        &base_url,
        &model,
    )
    .await
    .map_err(|e| e.to_string())?;
    let latency_ms = t0.elapsed().as_millis() as u32;

    if !resp.status().is_success() {
        let status = resp.status();
        let details: String = resp
            .text()
            .await
            .unwrap_or_default()
            .chars()
            .take(200)
            .collect();
        return Err(if details.is_empty() {
            format!("HTTP {status}")
        } else {
            format!("HTTP {status}: {details}")
        });
    }

    let reasoning_active = resp
        .json::<serde_json::Value>()
        .await
        .is_ok_and(|reply| crate::llm::protocol::response_shows_reasoning(kind, &reply));
    Ok(LlmBenchResult {
        latency_ms,
        reasoning_active,
    })
}
