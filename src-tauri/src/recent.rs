//! What was dictated a moment ago, kept in memory so the next dictation in the same
//! application is recognized in context. Nothing here is written to disk.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

const KEPT_FOR: Duration = Duration::from_secs(600);
const MAX_ENTRIES: usize = 3;
const MAX_CHARS: usize = 400;
/// Words handed to the speech engine. More slow it down without raising accuracy.
const MAX_ENGINE_WORDS: usize = 30;
const MAX_WORD_CHARS: usize = 40;

#[derive(Default)]
pub struct RecentDictations {
    by_app: HashMap<String, Vec<(Instant, String)>>,
}

impl RecentDictations {
    pub fn remember(&mut self, app: &str, text: &str, now: Instant) {
        let text = text.trim();
        if app.is_empty() || text.is_empty() {
            return;
        }
        self.by_app.retain(|_, entries| {
            entries
                .iter()
                .any(|(at, _)| now.duration_since(*at) < KEPT_FOR)
        });
        let entries = self.by_app.entry(app.to_string()).or_default();
        entries.push((now, text.to_string()));
        if entries.len() > MAX_ENTRIES {
            entries.remove(0);
        }
    }

    /// The recent dictations in `app`, oldest first, cut to the most recent characters.
    pub fn context(&self, app: &str, now: Instant) -> Option<String> {
        let joined = self
            .by_app
            .get(app)?
            .iter()
            .filter(|(at, _)| now.duration_since(*at) < KEPT_FOR)
            .map(|(_, text)| text.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        let chars: Vec<char> = joined.chars().collect();
        let recent: String = chars[chars.len().saturating_sub(MAX_CHARS)..]
            .iter()
            .collect();
        (!recent.is_empty()).then_some(recent)
    }
}

/// Text before the cursor is at most this long when given to the engine.
const MAX_FIELD_CHARS: usize = 200;

/// What the speech engine is told came before this dictation: the text before the cursor
/// when it could be read, otherwise what was dictated a moment ago.
pub fn engine_prompt(field_before: Option<String>, recent: Option<String>) -> Option<String> {
    let before = field_before
        .map(|text| {
            let chars: Vec<char> = text.trim().chars().collect();
            chars[chars.len().saturating_sub(MAX_FIELD_CHARS)..]
                .iter()
                .collect::<String>()
        })
        .filter(|text| !text.is_empty());
    before.or(recent)
}

/// Holds the text read before the cursor for the recording in progress. The read runs
/// beside the recording, so a result that arrives for an earlier recording is dropped.
#[derive(Default)]
pub struct FieldContext {
    recording: u64,
    text: Option<String>,
}

impl FieldContext {
    /// Starts a recording and returns its number.
    pub fn begin(&mut self) -> u64 {
        self.recording += 1;
        self.text = None;
        self.recording
    }

    pub fn set(&mut self, recording: u64, text: String) {
        if recording == self.recording {
            self.text = Some(text);
        }
    }

    pub fn take(&mut self) -> Option<String> {
        self.text.take()
    }
}

fn field_context() -> &'static Mutex<FieldContext> {
    static SHARED: OnceLock<Mutex<FieldContext>> = OnceLock::new();
    SHARED.get_or_init(Mutex::default)
}

/// Reads the text before the cursor in application `pid` without holding up the recording.
/// The text stays in memory and is only ever given to the speech engine on this machine.
pub fn read_field_context(pid: u32) {
    let recording = field_context()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .begin();
    tauri::async_runtime::spawn_blocking(move || {
        if let Some(text) = crate::readback::before_cursor(pid, MAX_FIELD_CHARS * 2) {
            field_context()
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .set(recording, text);
        }
    });
}

pub fn take_field_context() -> Option<String> {
    field_context()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .take()
}

fn shared() -> &'static Mutex<RecentDictations> {
    static SHARED: OnceLock<Mutex<RecentDictations>> = OnceLock::new();
    SHARED.get_or_init(Mutex::default)
}

pub fn remember(app: &str, text: &str) {
    shared()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .remember(app, text, Instant::now());
}

pub fn context(app: &str) -> Option<String> {
    shared()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .context(app, Instant::now())
}

/// The dictionary words given to the speech engine: the newest ones, since those are the
/// words the user most recently needed. `words` is in the order they were added.
pub fn engine_vocabulary(words: &[String]) -> Vec<String> {
    let mut chosen: Vec<String> = Vec::new();
    for word in words.iter().rev() {
        let word = word.trim();
        if word.is_empty() || word.chars().count() > MAX_WORD_CHARS {
            continue;
        }
        if !chosen.iter().any(|kept| kept.eq_ignore_ascii_case(word)) {
            chosen.push(word.to_string());
        }
        if chosen.len() == MAX_ENGINE_WORDS {
            break;
        }
    }
    chosen
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_next_dictation_in_the_same_app_gets_the_earlier_ones() {
        let mut recent = RecentDictations::default();
        let now = Instant::now();
        recent.remember("mail", "第一句", now);
        recent.remember("mail", "第二句", now);
        assert_eq!(
            recent.context("mail", now).as_deref(),
            Some("第一句\n第二句")
        );
        assert_eq!(recent.context("chat", now), None);
    }

    #[test]
    fn only_the_last_three_dictations_are_kept() {
        let mut recent = RecentDictations::default();
        let now = Instant::now();
        for text in ["一", "二", "三", "四"] {
            recent.remember("mail", text, now);
        }
        assert_eq!(recent.context("mail", now).as_deref(), Some("二\n三\n四"));
    }

    #[test]
    fn dictations_older_than_ten_minutes_are_forgotten() {
        let mut recent = RecentDictations::default();
        let start = Instant::now();
        recent.remember("mail", "旧的", start);
        let later = start + Duration::from_secs(601);
        assert_eq!(recent.context("mail", later), None);
        recent.remember("chat", "新的", later);
        assert!(!recent.by_app.contains_key("mail"));
    }

    #[test]
    fn a_long_context_keeps_its_most_recent_characters() {
        let mut recent = RecentDictations::default();
        let now = Instant::now();
        recent.remember("mail", &"前".repeat(500), now);
        recent.remember("mail", "结尾", now);
        let context = recent.context("mail", now).unwrap();
        assert_eq!(context.chars().count(), 400);
        assert!(context.ends_with("结尾"));
    }

    #[test]
    fn text_before_the_cursor_is_preferred_over_earlier_dictations() {
        let before = Some(format!("{}光标前", "字".repeat(300)));
        let prompt = engine_prompt(before, Some("前一句".to_string())).unwrap();
        assert_eq!(prompt.chars().count(), 200);
        assert!(prompt.ends_with("光标前"));
    }

    #[test]
    fn an_empty_field_falls_back_to_earlier_dictations() {
        assert_eq!(
            engine_prompt(Some("  \n".to_string()), Some("前一句".to_string())).as_deref(),
            Some("前一句")
        );
        assert_eq!(engine_prompt(None, None), None);
    }

    #[test]
    fn text_read_for_one_recording_is_not_used_by_the_next() {
        let mut slot = FieldContext::default();
        let first = slot.begin();
        slot.set(first, "第一次".to_string());
        assert_eq!(slot.take().as_deref(), Some("第一次"));
        assert_eq!(slot.take(), None);

        let stale = slot.begin();
        let current = slot.begin();
        slot.set(stale, "迟到的".to_string());
        assert_eq!(slot.take(), None);
        slot.set(current, "这一次".to_string());
        assert_eq!(slot.take().as_deref(), Some("这一次"));
    }

    #[test]
    fn the_engine_gets_the_newest_thirty_words_once_each() {
        let words: Vec<String> = (0..40).map(|n| format!("Word{n}")).collect();
        let chosen = engine_vocabulary(&words);
        assert_eq!(chosen.len(), 30);
        assert_eq!(chosen[0], "Word39");
        assert!(!chosen.contains(&"Word9".to_string()));

        let repeated = vec!["GitHub".to_string(), " github ".to_string(), String::new()];
        assert_eq!(engine_vocabulary(&repeated), vec!["github".to_string()]);
    }
}
