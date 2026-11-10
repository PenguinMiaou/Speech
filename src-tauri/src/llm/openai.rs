use async_trait::async_trait;
use futures_util::StreamExt;
use reqwest::Client;

use crate::error::AppError;

use super::{
    prompt, protocol, ChunkCallback, LlmConfig, LlmProvider, PolishRequest, PolishResponse,
};

pub struct OpenAiProvider {
    client: Client,
}

impl Default for OpenAiProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl OpenAiProvider {
    pub fn new() -> Self {
        Self {
            client: Client::new(),
        }
    }

    pub fn with_client(client: Client) -> Self {
        Self { client }
    }
}

#[async_trait]
impl LlmProvider for OpenAiProvider {
    async fn polish(
        &self,
        config: &LlmConfig,
        req: &PolishRequest,
        on_chunk: Option<&ChunkCallback>,
    ) -> Result<PolishResponse, AppError> {
        let has_selected_text = req
            .selected_text
            .as_ref()
            .is_some_and(|s| !s.trim().is_empty());

        let system_prompt = prompt::build_context_system_prompt(prompt::ContextPromptOptions {
            context: &req.context,
            dictionary: &req.dictionary,
            correction_rules: &req.correction_rules,
            polish_style: &req.polish_style,
            personal_style_prompt: "",
            mapped_scene_prompt: &req.mapped_scene_prompt,
            active_scene_prompt: &req.active_scene_prompt,
            polish_custom_prompt: &req.polish_custom_prompt,
            translate_enabled: req.translate_enabled,
            target_lang: &req.target_lang,
            has_selected_text,
            voice_intent: Some(&req.voice_intent),
            assistant_name: &req.assistant_name,
            long_speech: req.long_speech,
            layout_habits: &req.layout_habits,
        });

        let mut messages = vec![serde_json::json!({ "role": "system", "content": system_prompt })];
        if has_selected_text {
            messages.push(serde_json::json!({
                "role": "user",
                "content": format!("<selected_text>\n{}\n</selected_text>", req.selected_text.as_ref().unwrap())
            }));
        }
        messages.push(serde_json::json!({
            "role": "user",
            "content": format!("<transcription>\n{}\n</transcription>", req.raw_text)
        }));

        let api_kind =
            protocol::resolve_api_kind(&config.api_format, &config.provider, &config.base_url);
        let endpoint =
            protocol::chat_endpoint(api_kind, &config.base_url).map_err(AppError::Config)?;
        let mut body = protocol::build_chat_body(
            api_kind,
            &config.provider,
            &config.base_url,
            &config.model,
            messages,
            config.max_tokens,
            config.temperature,
            on_chunk.is_some(),
        );

        // Retry the initial connection (not once streaming starts)
        #[allow(unused_assignments)]
        let mut response = None;
        let mut last_error: Option<AppError> = None;
        let mut attempt = 0u32;

        loop {
            match protocol::send_with_reasoning_fallback(
                |body| {
                    let request = self
                        .client
                        .post(&endpoint)
                        .header("Content-Type", "application/json");
                    protocol::apply_auth_headers(
                        api_kind,
                        request,
                        &config.provider,
                        &config.api_key,
                    )
                    .json(body)
                    .timeout(protocol::request_timeout(api_kind, &config.model))
                },
                &mut body,
                &config.base_url,
                &config.model,
            )
            .await
            {
                Ok(resp) => {
                    let status = resp.status();
                    if status.is_success() {
                        response = Some(resp);
                        break;
                    } else if status.as_u16() >= 500 && attempt < 2 {
                        let body_text = resp.text().await.unwrap_or_default();
                        tracing::warn!(
                            "LLM server error {} (attempt {}/3), retrying",
                            status,
                            attempt + 1
                        );
                        last_error = Some(AppError::Api {
                            status: status.as_u16(),
                            body: body_text,
                        });
                        attempt += 1;
                        tokio::time::sleep(std::time::Duration::from_millis(
                            1000 * 2u64.pow(attempt - 1),
                        ))
                        .await;
                        continue;
                    } else {
                        let status = resp.status();
                        let text = resp.text().await.unwrap_or_default();
                        let sanitized = bounded_error_body(&text);
                        return Err(AppError::Api {
                            status: status.as_u16(),
                            body: sanitized.to_string(),
                        });
                    }
                }
                Err(e) if e.is_timeout() && attempt < 2 => {
                    tracing::warn!(
                        "LLM connection timeout (attempt {}/3), retrying",
                        attempt + 1
                    );
                    last_error = Some(e.into());
                    attempt += 1;
                    tokio::time::sleep(std::time::Duration::from_millis(
                        1000 * 2u64.pow(attempt - 1),
                    ))
                    .await;
                    continue;
                }
                Err(e) if e.is_connect() && attempt < 2 => {
                    tracing::warn!(
                        "LLM connection failed (attempt {}/3), retrying",
                        attempt + 1
                    );
                    last_error = Some(e.into());
                    attempt += 1;
                    tokio::time::sleep(std::time::Duration::from_millis(
                        1000 * 2u64.pow(attempt - 1),
                    ))
                    .await;
                    continue;
                }
                Err(e) => return Err(e.into()),
            }
        }

        let response = response.ok_or_else(|| last_error.unwrap())?;

        if let Some(callback) = on_chunk {
            // Streaming mode
            let mut full_text = String::new();
            let mut used = super::usage::TokenUsage::default();
            let mut reasoning_text = String::new();
            let mut think_filter = protocol::ThinkFilter::default();
            let mut stream = response.bytes_stream();

            let mut buffer = String::new();
            let mut stream_done = false;
            while !stream_done {
                let Some(chunk) = stream.next().await else {
                    break;
                };
                let chunk = chunk?;
                buffer.push_str(&String::from_utf8_lossy(&chunk));

                // Process SSE lines
                while let Some(line_end) = buffer.find('\n') {
                    let line = buffer[..line_end].trim().to_string();
                    buffer = buffer[line_end + 1..].to_string();

                    if let Some(data) = protocol::sse_data(&line) {
                        if data == "[DONE]" {
                            stream_done = true;
                            break;
                        }
                        if let Ok(v) = serde_json::from_str::<serde_json::Value>(data) {
                            used.absorb(&v);
                            let event = protocol::parse_stream_event(api_kind, &v);
                            if let Some(error) = event.error {
                                return Err(AppError::Config(error));
                            }
                            if let Some(content) = event.text {
                                let visible = think_filter.push(&content);
                                if !visible.is_empty() {
                                    full_text.push_str(&visible);
                                    callback(&visible);
                                }
                            }

                            // Collect reasoning_content as fallback for thinking-mode models
                            // where all output may land in this field instead of content
                            if let Some(rc) = event.reasoning {
                                if !rc.is_empty() {
                                    reasoning_text.push_str(&rc);
                                }
                            }
                            stream_done = event.done;
                        }
                    }
                }
            }

            let held_back = think_filter.finish();
            if !held_back.is_empty() {
                full_text.push_str(&held_back);
                callback(&held_back);
            }

            // If content was empty but reasoning_content had text, use it as output.
            // This handles GLM thinking-mode where the API puts all output in reasoning_content.
            if full_text.is_empty() && !reasoning_text.is_empty() {
                tracing::warn!(
                    "LLM content empty, using reasoning_content ({} chars) as output",
                    reasoning_text.len()
                );
                callback(&reasoning_text);
                full_text = reasoning_text;
            } else if full_text.is_empty() {
                tracing::error!("LLM streaming returned no content and no reasoning_content");
            }

            super::usage::report(used);
            Ok(PolishResponse {
                polished_text: full_text,
            })
        } else {
            // Non-streaming mode
            let v: serde_json::Value = response.json().await?;
            let text = protocol::strip_think_block(&protocol::response_text(api_kind, &v));

            if text.is_empty() {
                tracing::warn!(
                    "LLM non-streaming returned empty content, full response: {}",
                    v
                );
            }

            let mut used = super::usage::TokenUsage::default();
            used.absorb(&v);
            super::usage::report(used);
            Ok(PolishResponse {
                polished_text: text,
            })
        }
    }

    fn name(&self) -> &str {
        "OpenAI"
    }
}

/// Long enough to keep the numbers at the end of a rate-limit message.
fn bounded_error_body(text: &str) -> &str {
    const LIMIT_BYTES: usize = 600;
    let end = text
        .char_indices()
        .take_while(|&(index, _)| index < LIMIT_BYTES)
        .last()
        .map(|(index, c)| index + c.len_utf8())
        .unwrap_or(text.len());
    &text[..end]
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::llm::test_dictation_request;
    use std::sync::{Arc, Mutex};
    use wiremock::matchers::{body_partial_json, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn config(server: &MockServer, model: &str, api_format: &str) -> LlmConfig {
        LlmConfig {
            provider: "custom".to_string(),
            api_key: String::new(),
            model: model.to_string(),
            base_url: format!("{}/v1", server.uri()),
            max_tokens: 256,
            temperature: 0.3,
            api_format: api_format.to_string(),
        }
    }

    fn chat_reply(text: &str) -> ResponseTemplate {
        ResponseTemplate::new(200)
            .set_body_json(serde_json::json!({"choices": [{"message": {"content": text}}]}))
    }

    #[tokio::test]
    async fn a_refused_reasoning_switch_is_retried_and_then_left_out() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .and(body_partial_json(
                serde_json::json!({"thinking": {"type": "disabled"}}),
            ))
            .respond_with(ResponseTemplate::new(400).set_body_string("unknown field"))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(chat_reply("clean text"))
            .expect(2)
            .mount(&server)
            .await;

        let config = config(&server, "deepseek-v4.1-flash", "chat_completions");
        let request = test_dictation_request("raw");
        for _ in 0..2 {
            let response = OpenAiProvider::new()
                .polish(&config, &request, None)
                .await
                .unwrap();
            assert_eq!(response.polished_text, "clean text");
        }
    }

    #[tokio::test]
    async fn a_think_block_in_a_full_reply_is_removed() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(chat_reply("<think>hm</think>\n\nclean text"))
            .mount(&server)
            .await;

        let response = OpenAiProvider::new()
            .polish(
                &config(&server, "some-model", "chat_completions"),
                &test_dictation_request("raw"),
                None,
            )
            .await
            .unwrap();
        assert_eq!(response.polished_text, "clean text");
    }

    #[tokio::test]
    async fn a_think_block_in_a_stream_is_never_passed_on() {
        let server = MockServer::start().await;
        let stream = [
            r#"data: {"choices":[{"delta":{"content":"<thi"}}]}"#,
            r#"data:{"choices":[{"delta":{"content":"nk>hm</think>"}}]}"#,
            r#"data: {"choices":[{"delta":{"content":"\n\nclean"}}]}"#,
            r#"data: {"choices":[{"delta":{"content":" text"}}]}"#,
            "data: [DONE]",
        ]
        .join("\n\n");
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_string(stream))
            .mount(&server)
            .await;

        let seen = Arc::new(Mutex::new(String::new()));
        let sink = seen.clone();
        let on_chunk: ChunkCallback =
            Box::new(move |chunk: &str| sink.lock().unwrap().push_str(chunk));
        let response = OpenAiProvider::new()
            .polish(
                &config(&server, "some-model", "chat_completions"),
                &test_dictation_request("raw"),
                Some(&on_chunk),
            )
            .await
            .unwrap();
        assert_eq!(response.polished_text, "clean text");
        assert_eq!(*seen.lock().unwrap(), "clean text");
    }

    #[tokio::test]
    async fn the_responses_format_streams_through_its_own_endpoint() {
        let server = MockServer::start().await;
        let stream = [
            "event: response.created\ndata: {\"type\":\"response.created\"}",
            "event: response.output_text.delta\ndata: {\"type\":\"response.output_text.delta\",\"delta\":\"clean\"}",
            "event: response.output_text.delta\ndata: {\"type\":\"response.output_text.delta\",\"delta\":\" text\"}",
            "event: response.completed\ndata: {\"type\":\"response.completed\",\"response\":{}}",
        ]
        .join("\n\n");
        Mock::given(method("POST"))
            .and(path("/v1/responses"))
            .and(body_partial_json(serde_json::json!({
                "model": "some-model",
                "store": false,
                "stream": true
            })))
            .respond_with(ResponseTemplate::new(200).set_body_string(stream))
            .expect(1)
            .mount(&server)
            .await;

        let on_chunk: ChunkCallback = Box::new(|_: &str| {});
        let response = OpenAiProvider::new()
            .polish(
                &config(&server, "some-model", "responses"),
                &test_dictation_request("raw"),
                Some(&on_chunk),
            )
            .await
            .unwrap();
        assert_eq!(response.polished_text, "clean text");
    }

    #[tokio::test]
    async fn the_anthropic_format_works_on_a_custom_gateway() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .and(body_partial_json(
                serde_json::json!({"thinking": {"type": "disabled"}}),
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "content": [{"type": "text", "text": "clean text"}]
            })))
            .expect(1)
            .mount(&server)
            .await;

        let response = OpenAiProvider::new()
            .polish(
                &config(&server, "some-model", "anthropic_messages"),
                &test_dictation_request("raw"),
                None,
            )
            .await
            .unwrap();
        assert_eq!(response.polished_text, "clean text");
    }

    #[test]
    fn a_rate_limit_body_keeps_its_requested_number() {
        let body = format!(
            "{{\"error\":{{\"message\":\"{} Limit 1000, Requested 4096, please reduce\"}}}}",
            "r".repeat(200)
        );
        assert!(bounded_error_body(&body).contains("Requested 4096"));
    }

    #[test]
    fn an_error_body_is_bounded_on_a_character_boundary() {
        let body = "润".repeat(1000);
        let bounded = bounded_error_body(&body);
        assert!(bounded.len() <= 603);
        assert!(bounded.chars().all(|c| c == '润'));
    }
}
