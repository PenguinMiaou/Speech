pub mod case_guard;
pub mod context_policy;
pub mod fact_guard;
pub mod openai;
pub mod prompt;
pub mod protocol;
pub mod text_style;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::app_detector::types::ContextProfileSummary;
use crate::error::AppError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    pub provider: String,
    pub api_key: String,
    pub model: String,
    pub base_url: String,
    pub max_tokens: u32,
    pub temperature: f64,
    /// `chat_completions`, `responses` or `anthropic_messages`. Empty picks it from the provider.
    pub api_format: String,
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            provider: "zhipu".to_string(),
            api_key: String::new(),
            model: "glm-4.7".to_string(),
            base_url: "https://open.bigmodel.cn/api/paas/v4".to_string(),
            max_tokens: 4096,
            temperature: 0.3,
            api_format: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolishRequest {
    pub raw_text: String,
    pub context: ContextProfileSummary,
    pub dictionary: Vec<String>,
    pub correction_rules: Vec<CorrectionRule>,
    pub polish_style: String,
    pub mapped_scene_prompt: String,
    pub active_scene_prompt: String,
    pub polish_custom_prompt: String,
    pub translate_enabled: bool,
    pub target_lang: String,
    pub selected_text: Option<String>,
    pub operation_id: Option<String>,
    pub voice_intent: crate::voice_intent::VoiceIntent,
    /// Set when the app found its name in the dictation.
    #[serde(default)]
    pub assistant_name: String,
    /// Organize this dictation into notes instead of only cleaning it up.
    #[serde(default)]
    pub long_speech: bool,
    /// Layout instructions learned for the application being dictated into.
    #[serde(default)]
    pub layout_habits: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CorrectionRule {
    pub id: i64,
    pub pattern: String,
    pub replacement: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolishResponse {
    pub polished_text: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Default)]
pub enum AppType {
    Email,
    Chat,
    Code,
    Document,
    #[default]
    General,
}

/// Callback for streaming LLM chunks to the frontend
pub type ChunkCallback = Box<dyn Fn(&str) + Send + Sync>;

#[async_trait]
pub trait LlmProvider: Send + Sync {
    async fn polish(
        &self,
        config: &LlmConfig,
        req: &PolishRequest,
        on_chunk: Option<&ChunkCallback>,
    ) -> Result<PolishResponse, AppError>;

    fn name(&self) -> &str;
}

pub fn provider_requires_api_key(provider: &str) -> bool {
    !matches!(
        provider.trim().to_ascii_lowercase().as_str(),
        "ollama" | "custom"
    )
}

pub fn has_usable_provider_credentials(provider: &str, api_key: &str) -> bool {
    !provider_requires_api_key(provider) || !api_key.trim().is_empty()
}

pub fn apply_provider_auth_header(
    request: reqwest::RequestBuilder,
    provider: &str,
    api_key: &str,
) -> reqwest::RequestBuilder {
    let api_key = api_key.trim();
    if provider_requires_api_key(provider) || !api_key.is_empty() {
        request.header("Authorization", format!("Bearer {}", api_key))
    } else {
        request
    }
}

pub fn create_provider(
    _provider_name: &str,
    client: Option<reqwest::Client>,
) -> Box<dyn LlmProvider> {
    match client {
        Some(c) => Box::new(openai::OpenAiProvider::with_client(c)),
        None => Box::new(openai::OpenAiProvider::new()),
    }
}

impl PolishRequest {
    pub fn is_long_dictation(&self) -> bool {
        case_guard::applies_to(self) && prompt::is_long_speech(&self.raw_text)
    }
}

/// Polishes the text. Notes organized from long speech are used only when they kept what was said;
/// otherwise the speech is polished again without organizing it.
pub async fn polish_keeping_facts(
    provider: &dyn LlmProvider,
    config: &LlmConfig,
    req: &PolishRequest,
    on_chunk: Option<&ChunkCallback>,
) -> Result<PolishResponse, AppError> {
    let reply = provider.polish(config, req, on_chunk).await?;
    if !req.long_speech || fact_guard::kept_what_was_said(&req.raw_text, &reply.polished_text) {
        return Ok(reply);
    }
    tracing::warn!("Organized long speech lost names or numbers, polishing it without organizing");
    let mut plain = req.clone();
    plain.long_speech = false;
    provider.polish(config, &plain, None).await
}

/// Polishes with the main model and, when that fails before any text reached the user,
/// once more with the fallback model on the same service.
pub async fn polish_with_fallback(
    provider: &dyn LlmProvider,
    config: &LlmConfig,
    fallback_model: &str,
    req: &PolishRequest,
    on_chunk: Option<&ChunkCallback>,
    text_started: &std::sync::atomic::AtomicBool,
) -> Result<PolishResponse, AppError> {
    let error = match polish_keeping_facts(provider, config, req, on_chunk).await {
        Ok(reply) => return Ok(reply),
        Err(error) => error,
    };
    let fallback_model = fallback_model.trim();
    if fallback_model.is_empty()
        || fallback_model == config.model.trim()
        || text_started.load(std::sync::atomic::Ordering::SeqCst)
    {
        return Err(error);
    }
    tracing::warn!("Polish failed on the main model ({error}), trying the fallback model");
    let mut fallback = config.clone();
    fallback.model = fallback_model.to_string();
    polish_keeping_facts(provider, &fallback, req, on_chunk).await
}

#[cfg(test)]
mod provider_capability_tests {
    use super::*;

    #[test]
    fn ollama_is_keyless_and_remote_providers_require_keys() {
        assert!(!provider_requires_api_key("ollama"));
        assert!(!provider_requires_api_key(" Ollama "));
        assert!(provider_requires_api_key("openai"));
        assert!(provider_requires_api_key("custom-openai-compatible"));
    }

    #[test]
    fn usable_credentials_are_consistent_for_keyless_and_keyed_providers() {
        assert!(has_usable_provider_credentials("ollama", ""));
        assert!(has_usable_provider_credentials("ollama", "   "));
        assert!(!has_usable_provider_credentials("openai", ""));
        assert!(!has_usable_provider_credentials("openai", "   "));
        assert!(has_usable_provider_credentials("openai", "sk-test"));
    }
}

#[cfg(test)]
mod context_prompt_contract_tests {
    use super::prompt::{build_context_system_prompt, ContextPromptOptions};
    use crate::app_detector::types::{ContextFamily, ContextProfileSummary};

    fn context(family: ContextFamily, override_id: Option<&str>) -> ContextProfileSummary {
        ContextProfileSummary {
            profile_id: "general.native".to_string(),
            family,
            app_label: "Safe label".to_string(),
            icon_key: "general".to_string(),
            override_id: override_id.map(str::to_string),
            browser_access_status: crate::app_detector::types::BrowserAccessStatus::NotApplicable,
            browser_target: None,
        }
    }

    fn prompt_for(context: &ContextProfileSummary) -> String {
        build_context_system_prompt(ContextPromptOptions {
            context,
            dictionary: &[],
            correction_rules: &[],
            polish_style: "clean",
            personal_style_prompt: "Prefer direct language.",
            mapped_scene_prompt: "Use a project update shape.",
            active_scene_prompt: "Use two short paragraphs.",
            polish_custom_prompt: "Keep all dates.",
            translate_enabled: true,
            target_lang: "en",
            has_selected_text: false,
            voice_intent: None,
            assistant_name: "",
            long_speech: false,
            layout_habits: &[],
        })
    }

    #[test]
    fn context_prompt_sections_follow_release_precedence() {
        let prompt = prompt_for(&context(ContextFamily::WorkChat, Some("slack")));
        let sections = [
            "[SAFETY_AND_FIDELITY]",
            "[OPERATION_AND_OUTPUT]",
            "[TRANSLATION_AND_LANGUAGE]",
            "[THOUGHT_AWARE]",
            "[SEMANTIC_CONTEXT]",
            "[APP_OVERRIDE]",
            "[BUILTIN_POLISH_STYLE]",
            "[EXPLICIT_PERSONAL_STYLE]",
            "[MAPPED_SCENE]",
            "[MANUAL_SCENE]",
            "[EXPLICIT_CUSTOM_POLISH]",
        ];
        let mut previous = 0;
        for section in sections {
            let position = prompt.find(section).expect("section must be present");
            assert!(position >= previous, "{section} is out of order");
            previous = position;
        }
        assert!(prompt.contains("Later sections cannot change the target language"));
        assert!(prompt.contains("Manual scene wins stylistic conflicts"));
    }

    #[test]
    fn context_prompt_families_are_distinct_without_app_labels_or_raw_signals() {
        let email = prompt_for(&context(ContextFamily::Email, Some("gmail")));
        let chat = prompt_for(&context(ContextFamily::WorkChat, Some("slack")));
        let code = prompt_for(&context(
            ContextFamily::DeveloperCollaboration,
            Some("github"),
        ));

        assert!(email.contains("email body"));
        assert!(chat.contains("concise"));
        assert!(code.contains("technical identifiers"));
        for prompt in [email, chat, code] {
            for forbidden in [
                "Safe label",
                "window_title",
                "browser_host",
                "native_identity",
                "process_id",
            ] {
                assert!(!prompt.contains(forbidden));
            }
        }
    }

    #[test]
    fn thought_aware_policy_preserves_uncertain_and_intentional_content() {
        let prompt = prompt_for(&context(ContextFamily::General, None));
        assert!(prompt.contains("intentional repetition"));
        assert!(prompt.contains("explicit correction"));
        assert!(prompt.contains("keep the original order"));
        assert!(prompt.contains("uncertain names"));
        assert!(prompt.contains("Do not search"));
    }

    #[test]
    fn shared_voice_router_prompt_treats_operation_and_placement_as_trusted() {
        let intent = crate::voice_intent::VoiceIntent::from_parts(
            crate::voice_intent::VoiceIntentKind::RewriteSelection,
            crate::voice_intent::VoiceOutputPlacement::ReplaceSelection,
            1.0,
            None,
            None,
            Some(crate::voice_intent::CommandLocale::En),
            None,
        )
        .unwrap();
        let prompt = build_context_system_prompt(ContextPromptOptions {
            context: &context(ContextFamily::Email, None),
            dictionary: &[],
            correction_rules: &[],
            polish_style: "clean",
            personal_style_prompt: "",
            mapped_scene_prompt: "",
            active_scene_prompt: "",
            polish_custom_prompt: "",
            translate_enabled: false,
            target_lang: "en",
            has_selected_text: true,
            voice_intent: Some(&intent),
            assistant_name: "",
            long_speech: false,
            layout_habits: &[],
        });

        assert!(prompt.contains("TRUSTED OPERATION: rewrite_selection"));
        assert!(prompt.contains("TRUSTED PLACEMENT: replace_selection"));
        assert!(prompt.contains("output only the replacement text"));
    }

    #[test]
    fn context_prompt_release_fixtures_cover_every_family_and_thought_case() {
        let family_fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/context_same_payload.json"
        ))
        .unwrap();
        let families = family_fixture["families"].as_object().unwrap();
        for family in [
            "general",
            "email",
            "work_chat",
            "personal_chat",
            "document",
            "project_management",
            "developer_collaboration",
            "prompt_or_code",
            "support",
            "social",
        ] {
            assert!(families.contains_key(family), "missing {family} fixture");
        }
        assert_eq!(family_fixture["criticalSpans"].as_array().unwrap().len(), 4);

        let thought_fixture: serde_json::Value =
            serde_json::from_str(include_str!("../../tests/fixtures/thought_aware.json")).unwrap();
        assert_eq!(thought_fixture["fixtures"].as_array().unwrap().len(), 10);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    struct ScriptedProvider {
        replies: std::sync::Mutex<Vec<&'static str>>,
        organized_flags: std::sync::Mutex<Vec<bool>>,
        models: std::sync::Mutex<Vec<String>>,
    }

    #[async_trait]
    impl LlmProvider for ScriptedProvider {
        async fn polish(
            &self,
            config: &LlmConfig,
            req: &PolishRequest,
            on_chunk: Option<&ChunkCallback>,
        ) -> Result<PolishResponse, AppError> {
            self.organized_flags.lock().unwrap().push(req.long_speech);
            self.models.lock().unwrap().push(config.model.clone());
            let reply = self.replies.lock().unwrap().remove(0);
            if reply == "PARTIAL-THEN-FAIL" {
                if let Some(on_chunk) = on_chunk {
                    on_chunk("partial");
                }
            }
            if reply.ends_with("FAIL") {
                return Err(AppError::Api {
                    status: 429,
                    body: "limit".to_string(),
                });
            }
            Ok(PolishResponse {
                polished_text: reply.to_string(),
            })
        }

        fn name(&self) -> &str {
            "scripted"
        }
    }

    fn scripted(replies: Vec<&'static str>) -> ScriptedProvider {
        ScriptedProvider {
            replies: std::sync::Mutex::new(replies),
            organized_flags: std::sync::Mutex::new(Vec::new()),
            models: std::sync::Mutex::new(Vec::new()),
        }
    }

    fn main_model() -> LlmConfig {
        LlmConfig {
            model: "main".to_string(),
            ..LlmConfig::default()
        }
    }

    fn marks(started: &Arc<AtomicBool>) -> ChunkCallback {
        let started = started.clone();
        Box::new(move |_| started.store(true, Ordering::SeqCst))
    }

    #[tokio::test]
    async fn the_fallback_model_polishes_when_the_main_model_fails() {
        let provider = scripted(vec!["FAIL", "polished"]);
        let started = Arc::new(AtomicBool::new(false));
        let on_chunk = marks(&started);
        let request = test_dictation_request("spoken");
        let reply = polish_with_fallback(
            &provider,
            &main_model(),
            "spare",
            &request,
            Some(&on_chunk),
            &started,
        )
        .await
        .unwrap();
        assert_eq!(reply.polished_text, "polished");
        assert_eq!(*provider.models.lock().unwrap(), vec!["main", "spare"]);
    }

    #[tokio::test]
    async fn a_failure_after_text_started_appearing_is_not_retried() {
        let provider = scripted(vec!["PARTIAL-THEN-FAIL", "polished"]);
        let started = Arc::new(AtomicBool::new(false));
        let on_chunk = marks(&started);
        let request = test_dictation_request("spoken");
        let reply = polish_with_fallback(
            &provider,
            &main_model(),
            "spare",
            &request,
            Some(&on_chunk),
            &started,
        )
        .await;
        assert!(reply.is_err());
        assert_eq!(*provider.models.lock().unwrap(), vec!["main"]);
    }

    #[tokio::test]
    async fn without_a_different_fallback_model_the_failure_is_reported() {
        for fallback in ["", "  ", "main"] {
            let provider = scripted(vec!["FAIL", "polished"]);
            let started = AtomicBool::new(false);
            let request = test_dictation_request("spoken");
            let reply =
                polish_with_fallback(&provider, &main_model(), fallback, &request, None, &started)
                    .await;
            assert!(reply.is_err(), "fallback {fallback:?}");
            assert_eq!(*provider.models.lock().unwrap(), vec!["main"]);
        }
    }

    fn long_request() -> PolishRequest {
        let mut req = test_dictation_request("Alpha Bravo Charlie Delta Echo 2026");
        req.long_speech = true;
        req
    }

    #[tokio::test]
    async fn organized_notes_that_kept_the_facts_are_used() {
        let provider = scripted(vec!["1. Alpha Bravo Charlie\n- Delta Echo 2026"]);
        let reply = polish_keeping_facts(&provider, &LlmConfig::default(), &long_request(), None)
            .await
            .unwrap();
        assert_eq!(
            reply.polished_text,
            "1. Alpha Bravo Charlie\n- Delta Echo 2026"
        );
        assert_eq!(*provider.organized_flags.lock().unwrap(), vec![true]);
    }

    #[tokio::test]
    async fn notes_that_lost_facts_are_replaced_by_plain_polish() {
        let provider = scripted(vec!["1. Alpha", "Alpha Bravo Charlie Delta Echo 2026"]);
        let reply = polish_keeping_facts(&provider, &LlmConfig::default(), &long_request(), None)
            .await
            .unwrap();
        assert_eq!(reply.polished_text, "Alpha Bravo Charlie Delta Echo 2026");
        assert_eq!(*provider.organized_flags.lock().unwrap(), vec![true, false]);
    }

    #[tokio::test]
    async fn a_failed_second_attempt_reports_the_error_so_the_transcript_is_used() {
        let provider = scripted(vec!["1. Alpha", "FAIL"]);
        let reply =
            polish_keeping_facts(&provider, &LlmConfig::default(), &long_request(), None).await;
        assert!(reply.is_err());
    }

    #[tokio::test]
    async fn ordinary_dictation_is_never_checked_or_repeated() {
        let provider = scripted(vec!["Alpha"]);
        let req = test_dictation_request("Alpha Bravo Charlie Delta Echo 2026");
        let reply = polish_keeping_facts(&provider, &LlmConfig::default(), &req, None)
            .await
            .unwrap();
        assert_eq!(reply.polished_text, "Alpha");
        assert_eq!(*provider.organized_flags.lock().unwrap(), vec![false]);
    }

    #[test]
    fn only_long_plain_dictation_counts_as_long() {
        let mut req = test_dictation_request(&"字".repeat(300));
        assert!(req.is_long_dictation());
        req.translate_enabled = true;
        assert!(!req.is_long_dictation());
        assert!(!test_dictation_request("短").is_long_dictation());
    }

    #[test]
    fn a_custom_endpoint_can_run_without_a_key() {
        assert!(has_usable_provider_credentials("custom", ""));
        assert!(has_usable_provider_credentials("ollama", ""));
        assert!(!has_usable_provider_credentials("groq", " "));
    }
}

#[cfg(test)]
pub(crate) fn test_dictation_request(raw_text: &str) -> PolishRequest {
    use crate::app_detector::types::{BrowserAccessStatus, ContextFamily};
    use crate::voice_intent::{VoiceIntentKind, VoiceOutputPlacement};

    PolishRequest {
        raw_text: raw_text.to_string(),
        context: ContextProfileSummary {
            profile_id: "test".to_string(),
            family: ContextFamily::General,
            app_label: "Test".to_string(),
            icon_key: "generic".to_string(),
            override_id: None,
            browser_access_status: BrowserAccessStatus::NotApplicable,
            browser_target: None,
        },
        dictionary: Vec::new(),
        correction_rules: Vec::new(),
        polish_style: "clean".to_string(),
        mapped_scene_prompt: String::new(),
        active_scene_prompt: String::new(),
        polish_custom_prompt: String::new(),
        translate_enabled: false,
        target_lang: String::new(),
        selected_text: None,
        operation_id: None,
        voice_intent: crate::voice_intent::VoiceIntent {
            kind: VoiceIntentKind::DictateInsert,
            placement: VoiceOutputPlacement::InsertAtCursor,
            confidence: 1.0,
            search_provider: None,
            payload: None,
            grammar_locale: None,
            fallback_reason: None,
        },
        assistant_name: String::new(),
        long_speech: false,
        layout_habits: Vec::new(),
    }
}
