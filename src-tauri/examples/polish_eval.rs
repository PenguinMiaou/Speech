//! Runs recognized text from `results/<label>.jsonl` through the real polish provider and writes
//! `results/<label>-polished.jsonl`. Sends text only, never audio. Prints counts, never text.
//!
//! `cargo run --release --example polish_eval -- --dir <dir> --label <label>`
//! The API key comes from `POLISH_API_KEY`, or `GROQ_API_KEY`.

use opentypeless_lib::app_detector::types::{
    BrowserAccessStatus, ContextFamily, ContextProfileSummary,
};
use opentypeless_lib::llm::{self, LlmConfig, PolishRequest};
use opentypeless_lib::voice_intent::{VoiceIntent, VoiceIntentKind, VoiceOutputPlacement};
use serde_json::{json, Value};
use std::io::Write;
use std::time::{Duration, Instant};

const MODEL: &str = "qwen/qwen3.8-27b";
const BASE_URL: &str = "https://api.groq.com/openai/v1";
const PROVIDER: &str = "groq";
const ATTEMPTS: u32 = 3;
const RETRY_WAIT: Duration = Duration::from_secs(20);

fn arg(name: &str) -> String {
    let args: Vec<String> = std::env::args().collect();
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1).cloned())
        .unwrap_or_else(|| panic!("missing {name}"))
}

fn request(raw_text: String, previous: Option<String>, assistant_name: String) -> PolishRequest {
    PolishRequest {
        raw_text,
        context: ContextProfileSummary {
            profile_id: "eval".to_string(),
            family: ContextFamily::General,
            app_label: "Eval".to_string(),
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
        selected_text: previous,
        operation_id: None,
        voice_intent: VoiceIntent {
            kind: VoiceIntentKind::DictateInsert,
            placement: VoiceOutputPlacement::InsertAtCursor,
            confidence: 1.0,
            search_provider: None,
            payload: None,
            grammar_locale: None,
            fallback_reason: None,
        },
        assistant_name,
        long_speech: false,
        layout_habits: Vec::new(),
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let dir = arg("--dir");
    let label = arg("--label");
    let api_key = std::env::var("POLISH_API_KEY")
        .or_else(|_| std::env::var("GROQ_API_KEY"))
        .expect("set POLISH_API_KEY or GROQ_API_KEY");
    let setting =
        |name: &str, default: &str| std::env::var(name).unwrap_or_else(|_| default.to_string());
    let provider_name = setting("POLISH_PROVIDER", PROVIDER);
    let config = LlmConfig {
        provider: provider_name.clone(),
        api_key,
        model: setting("POLISH_MODEL", MODEL),
        base_url: setting("POLISH_BASE_URL", BASE_URL),
        max_tokens: 4096,
        temperature: 0.3,
        api_format: setting("POLISH_API_FORMAT", ""),
    };
    let provider = llm::create_provider(&provider_name, None);

    let input = std::fs::read_to_string(format!("{dir}/results/{label}.jsonl")).unwrap();
    let mut output =
        std::fs::File::create(format!("{dir}/results/{label}-polished.jsonl")).unwrap();
    let (mut done, mut failed) = (0u32, 0u32);

    for line in input.lines() {
        let row: Value = serde_json::from_str(line).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(
            setting("POLISH_GAP_MS", "0").parse().unwrap_or(0),
        ));
        let raw = row["text"].as_str().unwrap_or_default().to_string();
        let started = Instant::now();
        let mut text = raw.clone();
        let mut error = Value::Null;
        if !raw.trim().is_empty() {
            for attempt in 1..=ATTEMPTS {
                let mut req = request(
                    raw.clone(),
                    row["previous"].as_str().map(str::to_string),
                    row["name"].as_str().unwrap_or_default().to_string(),
                );
                let strings = |field: &str| -> Vec<String> {
                    row[field]
                        .as_array()
                        .map(|items| {
                            items
                                .iter()
                                .filter_map(|item| item.as_str().map(str::to_string))
                                .collect()
                        })
                        .unwrap_or_default()
                };
                // Optional per-row dictionary words and layout habits, as the app supplies them.
                req.dictionary = strings("words");
                req.layout_habits = strings("habits");
                req.long_speech = req.is_long_dictation();
                match llm::polish_keeping_facts(provider.as_ref(), &config, &req, None).await {
                    Ok(response) => {
                        text = response.polished_text;
                        error = Value::Null;
                        break;
                    }
                    Err(failure) => {
                        // Only the error class is kept, since a provider error can quote the input.
                        error = json!(format!("{failure:?}").chars().take(40).collect::<String>());
                        if attempt < ATTEMPTS {
                            tokio::time::sleep(RETRY_WAIT).await;
                        }
                    }
                }
            }
        }
        failed += u32::from(!error.is_null());
        done += 1;
        let result = json!({
            "idx": row["idx"],
            "text": text,
            "lang": row["lang"],
            "err": error,
            "secs": started.elapsed().as_secs_f64(),
        });
        writeln!(output, "{result}").unwrap();
        if done % 10 == 0 {
            println!("polished {done} failed {failed}");
        }
    }
    println!("polished {done} failed {failed}");
}
