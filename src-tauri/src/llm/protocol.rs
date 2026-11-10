use reqwest::RequestBuilder;
use serde_json::{json, Value};
use std::collections::HashSet;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

const ANTHROPIC_API_HOST: &str = "api.anthropic.com";
const OPENAI_API_HOST: &str = "api.openai.com";
const ANTHROPIC_VERSION: &str = "2023-06-01";
const RESPONSES_MIN_OUTPUT_TOKENS: u32 = 16;
const REASONING_OFF_FIELDS: [&str; 5] = [
    "thinking",
    "reasoning",
    "reasoning_effort",
    "enable_thinking",
    "chat_template_kwargs",
];
const THINK_OPEN: &str = "<think>";
const THINK_CLOSE: &str = "</think>";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LlmApiKind {
    OpenAiCompatible,
    OpenAiResponses,
    AnthropicMessages,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct StreamEvent {
    pub text: Option<String>,
    pub reasoning: Option<String>,
    pub error: Option<String>,
    pub done: bool,
}

pub fn detect_api_kind(provider: &str, base_url: &str) -> LlmApiKind {
    let provider = provider.trim().to_ascii_lowercase();
    let host = url::Url::parse(base_url.trim())
        .ok()
        .and_then(|url| url.host_str().map(str::to_ascii_lowercase));

    if matches!(provider.as_str(), "claude" | "anthropic")
        && host.as_deref() == Some(ANTHROPIC_API_HOST)
    {
        LlmApiKind::AnthropicMessages
    } else {
        LlmApiKind::OpenAiCompatible
    }
}

/// `api_format` is the setting. A config saved before it existed has it empty.
pub fn resolve_api_kind(api_format: &str, provider: &str, base_url: &str) -> LlmApiKind {
    match api_format.trim() {
        "chat_completions" => LlmApiKind::OpenAiCompatible,
        "responses" => LlmApiKind::OpenAiResponses,
        "anthropic_messages" => LlmApiKind::AnthropicMessages,
        _ => detect_api_kind(provider, base_url),
    }
}

fn parse_http_url(base_url: &str) -> Result<url::Url, String> {
    let mut url = url::Url::parse(base_url.trim())
        .map_err(|error| format!("Invalid LLM base URL: {error}"))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err("LLM base URL must use http or https scheme".to_string());
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err("LLM base URL must not include credentials".to_string());
    }
    if url.fragment().is_some() {
        return Err("LLM base URL must not include a fragment".to_string());
    }
    url.set_fragment(None);
    Ok(url)
}

fn openai_api_root(path: &str) -> &str {
    let path = path.trim_end_matches('/');
    path.strip_suffix("/chat/completions")
        .or_else(|| path.strip_suffix("/responses"))
        .unwrap_or(path)
}

fn anthropic_api_root(path: &str) -> String {
    let path = path.trim_end_matches('/');
    if let Some(root) = path.strip_suffix("/messages") {
        return root.to_string();
    }
    if let Some(root) = path.strip_suffix("/models") {
        return root.to_string();
    }
    if path.is_empty() {
        "/v1".to_string()
    } else {
        path.to_string()
    }
}

pub fn chat_endpoint(kind: LlmApiKind, base_url: &str) -> Result<String, String> {
    let mut url = parse_http_url(base_url)?;
    let path = match kind {
        LlmApiKind::AnthropicMessages => format!("{}/messages", anthropic_api_root(url.path())),
        LlmApiKind::OpenAiCompatible => {
            format!("{}/chat/completions", openai_api_root(url.path()))
        }
        LlmApiKind::OpenAiResponses => format!("{}/responses", openai_api_root(url.path())),
    };
    url.set_path(&path);
    Ok(url.to_string())
}

pub fn models_endpoint(kind: LlmApiKind, base_url: &str) -> Result<String, String> {
    let mut url = parse_http_url(base_url)?;
    let path = match kind {
        LlmApiKind::AnthropicMessages => format!("{}/models", anthropic_api_root(url.path())),
        LlmApiKind::OpenAiCompatible | LlmApiKind::OpenAiResponses => {
            format!("{}/models", openai_api_root(url.path()))
        }
    };
    url.set_path(&path);
    Ok(url.to_string())
}

fn is_direct_openai(provider: &str, base_url: &str) -> bool {
    provider.trim().eq_ignore_ascii_case("openai")
        && url::Url::parse(base_url.trim())
            .ok()
            .and_then(|url| url.host_str().map(str::to_ascii_lowercase))
            .as_deref()
            == Some(OPENAI_API_HOST)
}

fn is_reasoning_model_without_sampling_controls(model: &str) -> bool {
    let model = model.trim().to_ascii_lowercase();
    model.starts_with("gpt-5")
        || model == "o1"
        || model.starts_with("o1-")
        || model == "o3"
        || model.starts_with("o3-")
        || model == "o4"
        || model.starts_with("o4-")
}

pub fn request_timeout(kind: LlmApiKind, model: &str) -> Duration {
    if kind == LlmApiKind::AnthropicMessages || is_reasoning_model_without_sampling_controls(model)
    {
        Duration::from_secs(60)
    } else {
        Duration::from_secs(30)
    }
}

fn normalize_anthropic_model(model: &str) -> String {
    let model = model
        .trim()
        .strip_prefix("anthropic/")
        .unwrap_or(model.trim());
    match model {
        "claude-sonnet-4" => "claude-sonnet-4-0".to_string(),
        "claude-opus-4" => "claude-opus-4-0".to_string(),
        _ => model.to_string(),
    }
}

fn split_system_messages(messages: Vec<Value>) -> (Vec<String>, Vec<Value>) {
    let mut system_parts = Vec::new();
    let mut others = Vec::new();
    for message in messages {
        if message["role"].as_str() == Some("system") {
            if let Some(content) = message["content"].as_str() {
                if !content.trim().is_empty() {
                    system_parts.push(content.to_string());
                }
            }
        } else {
            others.push(message);
        }
    }
    (system_parts, others)
}

fn openai_lowest_effort(model: &str) -> Option<&'static str> {
    if model.starts_with("gpt-5.") {
        Some("none")
    } else if model == "gpt-5" || model.starts_with("gpt-5-") {
        Some("minimal")
    } else if model.starts_with("gpt-oss")
        || ["o1", "o3", "o4"]
            .iter()
            .any(|name| model == *name || model.starts_with(&format!("{name}-")))
    {
        Some("low")
    } else {
        None
    }
}

/// Polish has to answer fast, so reasoning stays off wherever a model has a switch for it.
fn reasoning_off_fields(
    kind: LlmApiKind,
    provider: &str,
    model: &str,
) -> Vec<(&'static str, Value)> {
    let thinking_off = || ("thinking", json!({"type": "disabled"}));
    if kind == LlmApiKind::AnthropicMessages {
        return vec![thinking_off()];
    }

    let provider = provider.trim().to_ascii_lowercase();
    let model = model.trim().to_ascii_lowercase();
    if provider == "openrouter" {
        return vec![("reasoning", json!({"enabled": false}))];
    }
    let bare_model = model.rsplit('/').next().unwrap_or(&model);
    if let Some(effort) = openai_lowest_effort(bare_model) {
        return vec![match kind {
            LlmApiKind::OpenAiResponses => ("reasoning", json!({"effort": effort})),
            _ => ("reasoning_effort", json!(effort)),
        }];
    }
    if provider == "groq" {
        return if bare_model.starts_with("qwen") {
            vec![("reasoning_effort", json!("none"))]
        } else {
            Vec::new()
        };
    }

    let mut fields = if ["deepseek", "glm", "kimi", "doubao"]
        .iter()
        .any(|family| model.contains(family))
    {
        vec![thinking_off()]
    } else if model.contains("qwen") {
        vec![("enable_thinking", json!(false))]
    } else if model.contains("gemini") {
        return vec![("reasoning_effort", json!("none"))];
    } else {
        return Vec::new();
    };
    // Self-hosted servers usually take the switch through the chat template instead.
    if provider == "custom" {
        fields.push((
            "chat_template_kwargs",
            json!({"enable_thinking": false, "thinking": false}),
        ));
    }
    fields
}

fn rejected_reasoning_off() -> &'static Mutex<HashSet<String>> {
    static REJECTED: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    REJECTED.get_or_init(Default::default)
}

fn reasoning_off_key(base_url: &str, model: &str) -> String {
    format!("{}\n{}", base_url.trim(), model.trim())
}

fn reasoning_off_was_rejected(base_url: &str, model: &str) -> bool {
    rejected_reasoning_off()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .contains(&reasoning_off_key(base_url, model))
}

pub fn mark_reasoning_off_rejected(base_url: &str, model: &str) {
    rejected_reasoning_off()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .insert(reasoning_off_key(base_url, model));
}

pub fn strip_reasoning_off(body: &mut Value) -> bool {
    let Some(object) = body.as_object_mut() else {
        return false;
    };
    let mut removed = false;
    for field in REASONING_OFF_FIELDS {
        removed |= object.remove(field).is_some();
    }
    removed
}

/// A strict server answers 400 or 422 to a field it does not know. The request then goes out once
/// more without the switch, and later requests to that endpoint leave it out.
pub async fn send_with_reasoning_fallback<F>(
    build: F,
    body: &mut Value,
    base_url: &str,
    model: &str,
) -> reqwest::Result<reqwest::Response>
where
    F: Fn(&Value) -> RequestBuilder,
{
    let response = build(body).send().await?;
    if !matches!(response.status().as_u16(), 400 | 422) || !strip_reasoning_off(body) {
        return Ok(response);
    }
    tracing::warn!("LLM endpoint refused the reasoning switch, retrying without it");
    let retry = build(body).send().await?;
    if retry.status().is_success() {
        mark_reasoning_off_rejected(base_url, model);
    }
    Ok(retry)
}

#[allow(clippy::too_many_arguments)]
pub fn build_chat_body(
    kind: LlmApiKind,
    provider: &str,
    base_url: &str,
    model: &str,
    messages: Vec<Value>,
    max_tokens: u32,
    temperature: f64,
    stream: bool,
) -> Value {
    let mut body = match kind {
        LlmApiKind::AnthropicMessages => {
            let (system_parts, anthropic_messages) = split_system_messages(messages);
            let mut body = json!({
                "model": normalize_anthropic_model(model),
                "messages": anthropic_messages,
                "max_tokens": max_tokens,
                "temperature": temperature.clamp(0.0, 1.0),
                "stream": stream
            });
            if !system_parts.is_empty() {
                body["system"] = Value::String(system_parts.join("\n\n"));
            }
            body
        }
        LlmApiKind::OpenAiResponses => {
            let (system_parts, input) = split_system_messages(messages);
            let mut body = json!({
                "model": model,
                "input": input,
                "max_output_tokens": max_tokens.max(RESPONSES_MIN_OUTPUT_TOKENS),
                "store": false,
                "stream": stream
            });
            if !system_parts.is_empty() {
                body["instructions"] = Value::String(system_parts.join("\n\n"));
            }
            if !is_reasoning_model_without_sampling_controls(model) {
                body["temperature"] = json!(temperature);
            }
            body
        }
        LlmApiKind::OpenAiCompatible => {
            let mut body = json!({
                "model": model,
                "messages": messages,
                "stream": stream
            });
            let object = body.as_object_mut().unwrap();
            if is_direct_openai(provider, base_url) {
                object.insert("max_completion_tokens".to_string(), json!(max_tokens));
                if !is_reasoning_model_without_sampling_controls(model) {
                    object.insert("temperature".to_string(), json!(temperature));
                }
            } else {
                object.insert("max_tokens".to_string(), json!(max_tokens));
                object.insert("temperature".to_string(), json!(temperature));
            }
            body
        }
    };
    if !reasoning_off_was_rejected(base_url, model) {
        for (field, value) in reasoning_off_fields(kind, provider, model) {
            body[field] = value;
        }
    }
    body
}

pub fn apply_auth_headers(
    kind: LlmApiKind,
    request: RequestBuilder,
    provider: &str,
    api_key: &str,
) -> RequestBuilder {
    let api_key = api_key.trim();
    match kind {
        LlmApiKind::AnthropicMessages => {
            let request = request.header("anthropic-version", ANTHROPIC_VERSION);
            if api_key.is_empty() {
                request
            } else {
                request.header("x-api-key", api_key)
            }
        }
        LlmApiKind::OpenAiCompatible | LlmApiKind::OpenAiResponses => {
            if super::provider_requires_api_key(provider) || !api_key.is_empty() {
                request.header("Authorization", format!("Bearer {api_key}"))
            } else {
                request
            }
        }
    }
}

pub fn response_text(kind: LlmApiKind, body: &Value) -> String {
    match kind {
        LlmApiKind::AnthropicMessages => body["content"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|block| block["type"].as_str() == Some("text"))
            .filter_map(|block| block["text"].as_str())
            .collect::<Vec<_>>()
            .join(""),
        LlmApiKind::OpenAiResponses => body["output"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|item| item["type"].as_str() == Some("message"))
            .flat_map(|item| item["content"].as_array().into_iter().flatten())
            .filter(|block| block["type"].as_str() == Some("output_text"))
            .filter_map(|block| block["text"].as_str())
            .collect::<Vec<_>>()
            .join(""),
        LlmApiKind::OpenAiCompatible => {
            let message = &body["choices"][0]["message"];
            message["content"]
                .as_str()
                .filter(|content| !content.is_empty())
                .or_else(|| message["reasoning_content"].as_str())
                .unwrap_or("")
                .to_string()
        }
    }
}

/// Whether a finished reply shows the model reasoned, so a test can warn about it.
pub fn response_shows_reasoning(kind: LlmApiKind, body: &Value) -> bool {
    let has_item = |list: &Value, item_type: &str| {
        list.as_array()
            .into_iter()
            .flatten()
            .any(|item| item["type"].as_str() == Some(item_type))
    };
    let counted = |tokens: &Value| tokens.as_u64().unwrap_or(0) > 0;
    let has_text = |text: &Value| text.as_str().is_some_and(|text| !text.trim().is_empty());
    match kind {
        LlmApiKind::AnthropicMessages => has_item(&body["content"], "thinking"),
        LlmApiKind::OpenAiResponses => {
            has_item(&body["output"], "reasoning")
                || counted(&body["usage"]["output_tokens_details"]["reasoning_tokens"])
        }
        LlmApiKind::OpenAiCompatible => {
            let message = &body["choices"][0]["message"];
            has_text(&message["reasoning_content"])
                || has_text(&message["reasoning"])
                || message["content"]
                    .as_str()
                    .is_some_and(|content| content.trim_start().starts_with(THINK_OPEN))
                || counted(&body["usage"]["completion_tokens_details"]["reasoning_tokens"])
        }
    }
}

pub fn sse_data(line: &str) -> Option<&str> {
    line.strip_prefix("data:").map(str::trim_start)
}

pub fn parse_stream_event(kind: LlmApiKind, body: &Value) -> StreamEvent {
    match kind {
        LlmApiKind::AnthropicMessages => {
            if body["type"].as_str() == Some("error") {
                return StreamEvent {
                    error: body["error"]["message"].as_str().map(str::to_string),
                    ..StreamEvent::default()
                };
            }
            if body["type"].as_str() == Some("message_stop") {
                return StreamEvent {
                    done: true,
                    ..StreamEvent::default()
                };
            }
            if body["type"].as_str() == Some("content_block_delta")
                && body["delta"]["type"].as_str() == Some("text_delta")
            {
                return StreamEvent {
                    text: body["delta"]["text"].as_str().map(str::to_string),
                    ..StreamEvent::default()
                };
            }
            StreamEvent::default()
        }
        LlmApiKind::OpenAiResponses => {
            let delta = || body["delta"].as_str().map(str::to_string);
            let event = StreamEvent::default();
            match body["type"].as_str() {
                Some("response.output_text.delta") => StreamEvent {
                    text: delta(),
                    ..event
                },
                Some("response.reasoning_text.delta")
                | Some("response.reasoning_summary_text.delta") => StreamEvent {
                    reasoning: delta(),
                    ..event
                },
                Some("response.completed") | Some("response.incomplete") => StreamEvent {
                    done: true,
                    ..event
                },
                Some("response.failed") => StreamEvent {
                    error: Some(
                        body["response"]["error"]["message"]
                            .as_str()
                            .unwrap_or("The model service reported a failure")
                            .to_string(),
                    ),
                    ..event
                },
                Some("error") => StreamEvent {
                    error: Some(
                        body["message"]
                            .as_str()
                            .or_else(|| body["error"]["message"].as_str())
                            .unwrap_or("The model service reported an error")
                            .to_string(),
                    ),
                    ..event
                },
                _ => event,
            }
        }
        LlmApiKind::OpenAiCompatible => {
            let delta = &body["choices"][0]["delta"];
            StreamEvent {
                text: delta["content"].as_str().map(str::to_string),
                reasoning: delta["reasoning_content"]
                    .as_str()
                    .or_else(|| delta["reasoning"].as_str())
                    .map(str::to_string),
                ..StreamEvent::default()
            }
        }
    }
}

#[derive(Default)]
enum ThinkState {
    #[default]
    Start,
    Inside,
    AfterClose,
    Passed,
}

/// A server without a reasoning parser sends `<think>` blocks as ordinary text, and a model with
/// reasoning off may still send an empty one. Neither may be typed into the user's document.
#[derive(Default)]
pub struct ThinkFilter {
    state: ThinkState,
    pending: String,
}

impl ThinkFilter {
    pub fn push(&mut self, chunk: &str) -> String {
        match self.state {
            ThinkState::Passed => return chunk.to_string(),
            ThinkState::AfterClose => return self.after_close(chunk),
            ThinkState::Start | ThinkState::Inside => self.pending.push_str(chunk),
        }
        if matches!(self.state, ThinkState::Start) {
            let text = self.pending.trim_start();
            if let Some(rest) = text.strip_prefix(THINK_OPEN) {
                self.pending = rest.to_string();
                self.state = ThinkState::Inside;
            } else if THINK_OPEN.starts_with(text) {
                return String::new();
            } else {
                self.state = ThinkState::Passed;
                return std::mem::take(&mut self.pending);
            }
        }
        match self.pending.find(THINK_CLOSE) {
            Some(end) => {
                let rest = self.pending[end + THINK_CLOSE.len()..].to_string();
                self.pending.clear();
                self.after_close(&rest)
            }
            None => String::new(),
        }
    }

    fn after_close(&mut self, chunk: &str) -> String {
        let text = chunk.trim_start();
        self.state = if text.is_empty() {
            ThinkState::AfterClose
        } else {
            ThinkState::Passed
        };
        text.to_string()
    }

    pub fn finish(&mut self) -> String {
        match self.state {
            ThinkState::Start => std::mem::take(&mut self.pending),
            _ => String::new(),
        }
    }
}

pub fn strip_think_block(text: &str) -> String {
    let mut filter = ThinkFilter::default();
    let mut output = filter.push(text);
    output.push_str(&filter.finish());
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    fn messages() -> Vec<Value> {
        vec![
            json!({"role": "system", "content": "Be concise."}),
            json!({"role": "user", "content": "Hello"}),
        ]
    }

    #[test]
    fn native_anthropic_uses_messages_endpoint_without_double_appending() {
        for base_url in [
            "https://api.anthropic.com",
            "https://api.anthropic.com/v1",
            "https://api.anthropic.com/v1/messages",
        ] {
            assert_eq!(
                chat_endpoint(LlmApiKind::AnthropicMessages, base_url).unwrap(),
                "https://api.anthropic.com/v1/messages"
            );
        }
        assert_eq!(
            models_endpoint(
                LlmApiKind::AnthropicMessages,
                "https://api.anthropic.com/v1/messages"
            )
            .unwrap(),
            "https://api.anthropic.com/v1/models"
        );
    }

    #[test]
    fn claude_on_openrouter_stays_openai_compatible() {
        assert_eq!(
            detect_api_kind("claude", "https://openrouter.ai/api/v1"),
            LlmApiKind::OpenAiCompatible
        );
        assert_eq!(
            chat_endpoint(
                detect_api_kind("claude", "https://openrouter.ai/api/v1"),
                "https://openrouter.ai/api/v1"
            )
            .unwrap(),
            "https://openrouter.ai/api/v1/chat/completions"
        );
    }

    #[test]
    fn native_anthropic_body_moves_system_prompt_and_normalizes_old_default_model() {
        let body = build_chat_body(
            LlmApiKind::AnthropicMessages,
            "claude",
            "https://api.anthropic.com/v1/messages",
            "anthropic/claude-sonnet-4",
            messages(),
            256,
            0.3,
            true,
        );

        assert_eq!(body["model"], "claude-sonnet-4-0");
        assert_eq!(body["system"], "Be concise.");
        assert_eq!(body["messages"].as_array().unwrap().len(), 1);
        assert_eq!(body["max_tokens"], 256);
        assert!(body.get("max_completion_tokens").is_none());
    }

    #[test]
    fn direct_openai_gpt5_uses_compatible_token_field_and_omits_temperature() {
        let body = build_chat_body(
            LlmApiKind::OpenAiCompatible,
            "openai",
            "https://api.openai.com/v1",
            "gpt-5",
            messages(),
            4096,
            0.3,
            false,
        );

        assert_eq!(body["max_completion_tokens"], 4096);
        assert!(body.get("max_tokens").is_none());
        assert!(body.get("temperature").is_none());
    }

    #[test]
    fn openai_compatible_proxies_keep_legacy_fields_for_compatibility() {
        let body = build_chat_body(
            LlmApiKind::OpenAiCompatible,
            "openrouter",
            "https://openrouter.ai/api/v1",
            "openai/gpt-5",
            messages(),
            4096,
            0.3,
            false,
        );

        assert_eq!(body["max_tokens"], 4096);
        assert_eq!(body["temperature"], 0.3);
        assert!(body.get("max_completion_tokens").is_none());
    }

    #[test]
    fn slow_reasoning_apis_get_a_longer_timeout_without_a_new_setting() {
        assert_eq!(
            request_timeout(LlmApiKind::OpenAiCompatible, "gpt-5"),
            Duration::from_secs(60)
        );
        assert_eq!(
            request_timeout(LlmApiKind::AnthropicMessages, "claude-sonnet-4-0"),
            Duration::from_secs(60)
        );
        assert_eq!(
            request_timeout(LlmApiKind::OpenAiCompatible, "gemini-2.5-flash"),
            Duration::from_secs(30)
        );
    }

    #[test]
    fn auth_headers_match_native_anthropic_and_openai_protocols() {
        let anthropic = apply_auth_headers(
            LlmApiKind::AnthropicMessages,
            reqwest::Client::new().post("https://api.anthropic.com/v1/messages"),
            "claude",
            "anthropic-key",
        )
        .build()
        .unwrap();
        assert_eq!(anthropic.headers()["x-api-key"], "anthropic-key");
        assert_eq!(anthropic.headers()["anthropic-version"], ANTHROPIC_VERSION);
        assert!(anthropic.headers().get("Authorization").is_none());

        let openai = apply_auth_headers(
            LlmApiKind::OpenAiCompatible,
            reqwest::Client::new().post("https://api.openai.com/v1/chat/completions"),
            "openai",
            "openai-key",
        )
        .build()
        .unwrap();
        assert_eq!(openai.headers()["Authorization"], "Bearer openai-key");
    }

    #[test]
    fn response_parsers_support_anthropic_json_and_streaming_events() {
        let response = json!({
            "content": [
                {"type": "text", "text": "Hello"},
                {"type": "text", "text": " world"}
            ]
        });
        assert_eq!(
            response_text(LlmApiKind::AnthropicMessages, &response),
            "Hello world"
        );

        let event = parse_stream_event(
            LlmApiKind::AnthropicMessages,
            &json!({
                "type": "content_block_delta",
                "delta": {"type": "text_delta", "text": "Hello"}
            }),
        );
        assert_eq!(event.text.as_deref(), Some("Hello"));
        assert!(!event.done);
    }

    const GATEWAY: &str = "http://gateway.example.com/v1";
    const R: LlmApiKind = LlmApiKind::OpenAiResponses;
    const C: LlmApiKind = LlmApiKind::OpenAiCompatible;
    const A: LlmApiKind = LlmApiKind::AnthropicMessages;

    fn body_for(kind: LlmApiKind, provider: &str, model: &str) -> Value {
        build_chat_body(kind, provider, GATEWAY, model, messages(), 256, 0.3, false)
    }

    #[test]
    fn an_explicit_format_overrides_detection() {
        let claude = "https://api.anthropic.com/v1";
        assert_eq!(resolve_api_kind("responses", "claude", claude), R);
        assert_eq!(resolve_api_kind("chat_completions", "claude", claude), C);
        assert_eq!(resolve_api_kind("anthropic_messages", "custom", GATEWAY), A);
        assert_eq!(resolve_api_kind("", "claude", claude), A);
        assert_eq!(resolve_api_kind("", "custom", GATEWAY), C);
    }

    #[test]
    fn each_format_has_its_own_paths_on_a_custom_gateway() {
        assert_eq!(
            chat_endpoint(C, GATEWAY).unwrap(),
            "http://gateway.example.com/v1/chat/completions"
        );
        assert_eq!(
            chat_endpoint(R, GATEWAY).unwrap(),
            "http://gateway.example.com/v1/responses"
        );
        assert_eq!(
            chat_endpoint(R, "http://gateway.example.com/v1/responses").unwrap(),
            "http://gateway.example.com/v1/responses"
        );
        assert_eq!(
            chat_endpoint(A, GATEWAY).unwrap(),
            "http://gateway.example.com/v1/messages"
        );
        for kind in [C, R, A] {
            assert_eq!(
                models_endpoint(kind, GATEWAY).unwrap(),
                "http://gateway.example.com/v1/models"
            );
        }
        assert_eq!(
            models_endpoint(R, "http://gateway.example.com/v1/responses").unwrap(),
            "http://gateway.example.com/v1/models"
        );
    }

    #[test]
    fn responses_body_uses_instructions_and_input() {
        let body = body_for(R, "custom", "some-model");
        assert_eq!(body["instructions"], "Be concise.");
        assert_eq!(body["input"].as_array().unwrap().len(), 1);
        assert_eq!(body["max_output_tokens"], 256);
        assert_eq!(body["temperature"], 0.3);
        assert_eq!(body["store"], false);
        assert!(body.get("messages").is_none());
        assert!(body.get("max_tokens").is_none());
    }

    #[test]
    fn responses_body_respects_the_minimum_output_limit() {
        let body = build_chat_body(R, "custom", GATEWAY, "m", messages(), 1, 0.3, false);
        assert_eq!(body["max_output_tokens"], 16);
    }

    #[test]
    fn responses_body_omits_temperature_for_reasoning_models() {
        assert!(body_for(R, "custom", "gpt-5.1")
            .get("temperature")
            .is_none());
    }

    #[test]
    fn responses_parsers_read_text_and_stream_events() {
        let response = json!({"output": [
            {"type": "reasoning", "summary": []},
            {"type": "message", "content": [
                {"type": "output_text", "text": "Hello"},
                {"type": "output_text", "text": " world"}
            ]}
        ]});
        assert_eq!(response_text(R, &response), "Hello world");

        let delta = json!({"type": "response.output_text.delta", "delta": "Hi"});
        assert_eq!(parse_stream_event(R, &delta).text.as_deref(), Some("Hi"));
        let done = json!({"type": "response.completed", "response": {}});
        assert!(parse_stream_event(R, &done).done);
        let failed = json!({"type": "response.failed", "response": {"error": {"message": "no"}}});
        assert_eq!(parse_stream_event(R, &failed).error.as_deref(), Some("no"));
        let error = json!({"type": "error", "message": "bad"});
        assert_eq!(parse_stream_event(R, &error).error.as_deref(), Some("bad"));
    }

    #[test]
    fn deepseek_turns_thinking_off_without_an_effort_field() {
        let body = body_for(C, "deepseek", "deepseek-v4.1-flash");
        assert_eq!(body["thinking"]["type"], "disabled");
        assert!(body.get("reasoning_effort").is_none());
        assert!(body.get("chat_template_kwargs").is_none());
    }

    #[test]
    fn a_custom_gateway_also_gets_the_template_switch() {
        for kind in [C, R] {
            let body = body_for(kind, "custom", "deepseek-v4.1-flash");
            assert_eq!(body["thinking"]["type"], "disabled");
            assert_eq!(body["chat_template_kwargs"]["enable_thinking"], false);
        }
    }

    #[test]
    fn glm_runs_with_thinking_off_at_the_requested_temperature() {
        let body = body_for(C, "zhipu", "glm-4-flash");
        assert_eq!(body["thinking"]["type"], "disabled");
        assert_eq!(body["temperature"], 0.3);
    }

    #[test]
    fn each_family_gets_its_own_reasoning_switch() {
        assert_eq!(body_for(C, "qwen", "qwen-turbo")["enable_thinking"], false);
        let groq = body_for(C, "groq", "qwen/qwen3.8-27b");
        assert_eq!(groq["reasoning_effort"], "none");
        assert!(groq.get("enable_thinking").is_none());
        assert_eq!(
            body_for(C, "openai", "gpt-5")["reasoning_effort"],
            "minimal"
        );
        assert_eq!(body_for(C, "openai", "gpt-5.1")["reasoning_effort"], "none");
        assert_eq!(
            body_for(R, "openai", "gpt-5.1")["reasoning"]["effort"],
            "none"
        );
        assert_eq!(
            body_for(A, "custom", "claude-x")["thinking"]["type"],
            "disabled"
        );
        assert_eq!(
            body_for(C, "openrouter", "any/model")["reasoning"]["enabled"],
            false
        );
    }

    #[test]
    fn a_model_without_a_known_switch_gets_none() {
        for (provider, model) in [("custom", "ep-20250101"), ("openai", "gpt-4o-mini")] {
            let mut body = body_for(C, provider, model);
            assert!(!strip_reasoning_off(&mut body));
        }
    }

    #[test]
    fn stripping_removes_every_switch_once() {
        let mut body = body_for(C, "custom", "deepseek-v4.1-flash");
        assert!(strip_reasoning_off(&mut body));
        assert!(body.get("thinking").is_none());
        assert!(body.get("chat_template_kwargs").is_none());
        assert!(!strip_reasoning_off(&mut body));
    }

    #[test]
    fn an_endpoint_that_rejected_the_switch_is_not_sent_it_again() {
        let url = "http://rejects.example.com/v1";
        mark_reasoning_off_rejected(url, "deepseek-chat");
        let mut body =
            build_chat_body(C, "custom", url, "deepseek-chat", messages(), 8, 0.3, false);
        assert!(!strip_reasoning_off(&mut body));
    }

    #[test]
    fn reasoning_is_detected_in_each_format() {
        let chat = json!({"choices": [{"message": {"content": "", "reasoning_content": "hm"}}]});
        assert!(response_shows_reasoning(C, &chat));
        let tagged = json!({"choices": [{"message": {"content": "<think>hm"}}]});
        assert!(response_shows_reasoning(C, &tagged));
        let counted = json!({
            "choices": [{"message": {"content": "hi"}}],
            "usage": {"completion_tokens_details": {"reasoning_tokens": 12}}
        });
        assert!(response_shows_reasoning(C, &counted));
        let clean = json!({
            "choices": [{"message": {"content": "hi"}}],
            "usage": {"completion_tokens_details": {"reasoning_tokens": 0}}
        });
        assert!(!response_shows_reasoning(C, &clean));

        assert!(response_shows_reasoning(
            R,
            &json!({"output": [{"type": "reasoning"}]})
        ));
        assert!(!response_shows_reasoning(
            R,
            &json!({"output": [{"type": "message"}]})
        ));
        assert!(response_shows_reasoning(
            A,
            &json!({"content": [{"type": "thinking"}]})
        ));
        assert!(!response_shows_reasoning(
            A,
            &json!({"content": [{"type": "text"}]})
        ));
    }

    #[test]
    fn stream_data_is_read_with_or_without_the_space() {
        assert_eq!(sse_data("data: {\"a\":1}"), Some("{\"a\":1}"));
        assert_eq!(sse_data("data:{\"a\":1}"), Some("{\"a\":1}"));
        assert_eq!(sse_data("event: response.completed"), None);
    }

    #[test]
    fn keyless_anthropic_gateways_get_no_key_header() {
        let request = apply_auth_headers(
            A,
            reqwest::Client::new().post("http://gateway.example.com/v1/messages"),
            "custom",
            "",
        )
        .build()
        .unwrap();
        assert!(request.headers().get("x-api-key").is_none());
        assert_eq!(request.headers()["anthropic-version"], ANTHROPIC_VERSION);
    }

    #[test]
    fn a_think_block_never_reaches_the_output() {
        assert_eq!(strip_think_block("<think>hm</think>\n\nhello"), "hello");
        assert_eq!(strip_think_block("  <think>\n\n</think>hello"), "hello");
        assert_eq!(
            strip_think_block("hello <think> stays"),
            "hello <think> stays"
        );
        assert_eq!(strip_think_block("<think>never closed"), "");
        assert_eq!(strip_think_block("<thin"), "<thin");
    }

    #[test]
    fn a_think_block_split_across_chunks_is_removed() {
        let mut filter = ThinkFilter::default();
        let mut output = String::new();
        for chunk in ["<th", "ink>reason", "ing</thi", "nk>\n", "hel", "lo"] {
            output.push_str(&filter.push(chunk));
        }
        output.push_str(&filter.finish());
        assert_eq!(output, "hello");
    }
}
