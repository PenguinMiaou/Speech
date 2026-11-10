//! Counts the tokens the polish model reports using, by day, in a file on this machine.
//! Only the numbers a provider sends back are counted; nothing is estimated.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use serde_json::Value;

const FILE_NAME: &str = "token-usage.json";
const DAYS_KEPT: usize = 400;

/// What one reply cost, as the provider reported it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenUsage {
    pub input: u64,
    pub output: u64,
}

impl TokenUsage {
    pub fn is_empty(&self) -> bool {
        self.input == 0 && self.output == 0
    }
}

impl TokenUsage {
    /// Takes in whatever usage one reply body or stream event carries. Providers put it in
    /// different places and some repeat a running total, so the largest figure seen wins.
    pub fn absorb(&mut self, body: &Value) {
        for usage in [
            &body["usage"],
            &body["x_groq"]["usage"],
            &body["message"]["usage"],
            &body["response"]["usage"],
        ] {
            let read =
                |names: [&str; 2]| names.iter().filter_map(|name| usage[*name].as_u64()).max();
            if let Some(input) = read(["prompt_tokens", "input_tokens"]) {
                self.input = self.input.max(input);
            }
            if let Some(output) = read(["completion_tokens", "output_tokens"]) {
                self.output = self.output.max(output);
            }
        }
    }
}

/// Tokens and replies over a stretch of days.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Span {
    pub input: u64,
    pub output: u64,
    pub requests: u64,
}

impl Span {
    fn add(&mut self, other: Span) {
        self.input += other.input;
        self.output += other.output;
        self.requests += other.requests;
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub today: Span,
    pub last_30_days: Span,
    pub total: Span,
}

/// Usage keyed by local date, written as `YYYY-MM-DD` so that text order is date order.
#[derive(Debug, Default, Serialize, Deserialize)]
struct Days(BTreeMap<String, Span>);

impl Days {
    fn add(&mut self, day: &str, usage: TokenUsage) {
        if usage.is_empty() {
            return;
        }
        self.0.entry(day.to_string()).or_default().add(Span {
            input: usage.input,
            output: usage.output,
            requests: 1,
        });
        while self.0.len() > DAYS_KEPT {
            self.0.pop_first();
        }
    }

    fn summary(&self, today: &str, thirty_days_ago: &str) -> Summary {
        let mut summary = Summary::default();
        for (day, span) in &self.0 {
            summary.total.add(*span);
            if day.as_str() > thirty_days_ago {
                summary.last_30_days.add(*span);
            }
            if day == today {
                summary.today.add(*span);
            }
        }
        summary
    }

    fn load(path: &Path) -> Self {
        std::fs::read(path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    fn save(&self, path: &Path) {
        if let Ok(bytes) = serde_json::to_vec(self) {
            let _ = std::fs::write(path, bytes);
        }
    }
}

static STORE: Mutex<Option<(PathBuf, Days)>> = Mutex::new(None);

fn day(offset_days: i64) -> String {
    (chrono::Local::now() - chrono::Duration::days(offset_days))
        .format("%Y-%m-%d")
        .to_string()
}

/// Reads earlier counts from the app's data folder. Until this runs nothing is counted.
pub fn init(data_dir: &Path) {
    let path = data_dir.join(FILE_NAME);
    let days = Days::load(&path);
    *STORE.lock().unwrap_or_else(|e| e.into_inner()) = Some((path, days));
}

/// Adds what one reply reported to today's count.
pub fn report(usage: TokenUsage) {
    if usage.is_empty() {
        return;
    }
    let mut store = STORE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((path, days)) = store.as_mut() {
        days.add(&day(0), usage);
        days.save(path);
    }
}

pub fn summary() -> Summary {
    let store = STORE.lock().unwrap_or_else(|e| e.into_inner());
    store
        .as_ref()
        .map(|(_, days)| days.summary(&day(0), &day(30)))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn seen(bodies: &[Value]) -> TokenUsage {
        let mut usage = TokenUsage::default();
        for body in bodies {
            usage.absorb(body);
        }
        usage
    }

    #[test]
    fn reads_usage_from_a_chat_completions_reply() {
        let usage = seen(&[json!({"usage": {"prompt_tokens": 120, "completion_tokens": 30}})]);
        assert_eq!(
            usage,
            TokenUsage {
                input: 120,
                output: 30
            }
        );
    }

    #[test]
    fn reads_usage_a_stream_carries_in_its_last_chunk() {
        let usage = seen(&[
            json!({"choices": [{"delta": {"content": "hi"}}]}),
            json!({"x_groq": {"usage": {"prompt_tokens": 80, "completion_tokens": 12}}}),
        ]);
        assert_eq!(
            usage,
            TokenUsage {
                input: 80,
                output: 12
            }
        );
    }

    #[test]
    fn reads_usage_split_across_the_events_of_a_messages_stream() {
        let usage = seen(&[
            json!({"type": "message_start", "message": {"usage": {"input_tokens": 200, "output_tokens": 1}}}),
            json!({"type": "message_delta", "usage": {"output_tokens": 9}}),
            json!({"type": "message_delta", "usage": {"output_tokens": 41}}),
        ]);
        assert_eq!(
            usage,
            TokenUsage {
                input: 200,
                output: 41
            }
        );
    }

    #[test]
    fn reads_usage_from_a_responses_reply() {
        let usage = seen(&[json!({
            "type": "response.completed",
            "response": {"usage": {"input_tokens": 55, "output_tokens": 7}}
        })]);
        assert_eq!(
            usage,
            TokenUsage {
                input: 55,
                output: 7
            }
        );
    }

    #[test]
    fn a_reply_without_usage_counts_nothing() {
        assert!(seen(&[json!({"choices": []}), json!("text")]).is_empty());
    }

    #[test]
    fn adds_up_by_day_and_sums_today_the_last_thirty_days_and_everything() {
        let mut days = Days::default();
        days.add(
            "2026-01-01",
            TokenUsage {
                input: 1000,
                output: 100,
            },
        );
        days.add(
            "2026-11-01",
            TokenUsage {
                input: 300,
                output: 30,
            },
        );
        days.add(
            "2026-11-10",
            TokenUsage {
                input: 20,
                output: 5,
            },
        );
        days.add(
            "2026-11-10",
            TokenUsage {
                input: 10,
                output: 5,
            },
        );

        let summary = days.summary("2026-11-10", "2026-10-12");
        assert_eq!(
            summary.today,
            Span {
                input: 30,
                output: 10,
                requests: 2
            }
        );
        assert_eq!(
            summary.last_30_days,
            Span {
                input: 330,
                output: 40,
                requests: 3
            }
        );
        assert_eq!(
            summary.total,
            Span {
                input: 1330,
                output: 140,
                requests: 4
            }
        );
    }

    #[test]
    fn a_reply_that_reported_nothing_is_not_counted_as_a_request() {
        let mut days = Days::default();
        days.add("2026-11-10", TokenUsage::default());
        assert_eq!(
            days.summary("2026-11-10", "2026-10-12").total,
            Span::default()
        );
    }

    #[test]
    fn keeps_only_the_most_recent_days() {
        let mut days = Days::default();
        for day in 0..(DAYS_KEPT + 5) {
            days.add(
                &format!("d{day:05}"),
                TokenUsage {
                    input: 1,
                    output: 1,
                },
            );
        }
        assert_eq!(days.0.len(), DAYS_KEPT);
        assert!(!days.0.contains_key("d00000"));
    }

    #[test]
    fn survives_being_written_and_read_back() {
        let dir = std::env::temp_dir().join(format!("ot-usage-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut days = Days::default();
        days.add(
            "2026-11-10",
            TokenUsage {
                input: 7,
                output: 3,
            },
        );
        days.save(&dir.join(FILE_NAME));
        let read = Days::load(&dir.join(FILE_NAME));
        assert_eq!(read.summary("2026-11-10", "2026-10-12").total.input, 7);
        assert!(Days::load(&dir.join("missing.json")).0.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
