//! After a dictation is inserted, checks whether the text field it went into can be read
//! back, and whether the text was changed before the user moved on. This is the groundwork
//! for learning from corrections. It logs outcomes only: the text read from the field is
//! compared in memory and dropped.

pub mod corrections;
pub mod layout;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
use macos as platform;
#[cfg(not(target_os = "macos"))]
mod unsupported;
#[cfg(not(target_os = "macos"))]
use unsupported as platform;

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// Time the target application gets to show the inserted text before the first read.
const BASELINE_DELAY: Duration = Duration::from_millis(150);
/// How long edits are watched for after a dictation.
const OBSERVE_FOR: Duration = Duration::from_secs(60);
const TICK: Duration = Duration::from_millis(500);
/// Fields longer than this, in UTF-16 units, are read only at their end.
pub(crate) const WHOLE_FIELD_LIMIT: usize = 100_000;
/// Share of the inserted text that must remain for a change to count as an edit.
const EDITED_SHARE: f32 = 0.5;
const SHINGLE: usize = 4;

static OBSERVATION: AtomicU64 = AtomicU64::new(0);

/// What happened to the inserted text by the end of the observation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum After {
    Same,
    Edited,
    Gone,
}

impl After {
    fn as_str(self) -> &'static str {
        match self {
            After::Same => "same",
            After::Edited => "edited",
            After::Gone => "gone",
        }
    }
}

/// Terminal programs draw frames around their input and wrap long lines, and editors add
/// their own spacing, so text is compared without whitespace and frame characters.
pub fn comparable(text: &str) -> Vec<char> {
    text.chars()
        .filter(|c| !c.is_whitespace() && !('\u{2500}'..='\u{259f}').contains(c))
        .collect()
}

fn contains(field: &[char], wanted: &[char]) -> bool {
    wanted.is_empty() || field.windows(wanted.len()).any(|window| window == wanted)
}

pub fn holds_inserted_text(field: &str, inserted: &str) -> bool {
    contains(&comparable(field), &comparable(inserted))
}

pub fn compare(field: &str, inserted: &str) -> After {
    let field = comparable(field);
    let inserted = comparable(inserted);
    if contains(&field, &inserted) {
        return After::Same;
    }
    let pieces: Vec<&[char]> = inserted.windows(SHINGLE.min(inserted.len())).collect();
    let kept = pieces
        .iter()
        .filter(|piece| contains(&field, piece))
        .count();
    if kept as f32 >= pieces.len() as f32 * EDITED_SHARE {
        After::Edited
    } else {
        After::Gone
    }
}

/// The reading to learn from. Sending a message clears or replaces its field, so a reading
/// at the end that no longer holds the dictation gives way to the latest one that did.
fn settled(latest: Option<String>, last: Option<String>, inserted: &str) -> Option<String> {
    match last {
        Some(text) if compare(&text, inserted) != After::Gone => Some(text),
        last => latest.or(last),
    }
}

/// How many UTF-16 units to read from the end of a field too long to read whole.
fn tail_units(inserted: &str) -> usize {
    inserted.encode_utf16().count() * 2 + 4_000
}

/// Ends the observation in progress: a new dictation is starting.
pub fn recording_started() {
    OBSERVATION.fetch_add(1, Ordering::SeqCst);
}

/// The text just before the cursor in the focused field of application `pid`, at most
/// `max_units` UTF-16 units. None when there is no readable field, no cursor, or the field
/// holds a password.
pub fn before_cursor(pid: u32, max_units: usize) -> Option<String> {
    platform::before_cursor(pid, max_units)
}

/// What one dictation showed once the user was done with it.
#[derive(Default)]
pub struct Observed {
    /// Words the user corrected.
    pub corrections: Vec<corrections::Correction>,
    /// Layout the user changed or left alone.
    pub layout: Vec<layout::Seen>,
}

/// Receives what was observed about a dictation.
pub type Learn = Box<dyn FnOnce(Observed) + Send>;

/// Watches the field of application `pid` that `inserted` was just written into, and hands
/// the words corrected in it to `learn`.
pub fn observe(pid: u32, app: String, inserted: String, learn: Option<Learn>) {
    let observation = OBSERVATION.fetch_add(1, Ordering::SeqCst) + 1;
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(BASELINE_DELAY).await;
        let tail = tail_units(&inserted);
        let started = Instant::now();
        let opened = tokio::task::spawn_blocking(move || platform::open(pid, tail)).await;
        let (mut field, text) = match opened {
            Ok(Ok(opened)) => opened,
            Ok(Err(reason)) => {
                tracing::info!("[Readback] app={app} readable=false reason={reason}");
                return;
            }
            Err(_) => return,
        };
        let located = holds_inserted_text(&text, &inserted);
        tracing::info!(
            "[Readback] app={app} readable=true route={} role={} units={} located={located} read_ms={}",
            field.route(),
            field.role(),
            field.units(),
            started.elapsed().as_millis(),
        );
        drop(text);
        if !located {
            field.close();
            return;
        }

        let watching = Instant::now();
        // The latest reading that still held the dictation, kept in memory only until the end.
        let mut latest: Option<String> = None;
        let ended_by = loop {
            tokio::time::sleep(TICK).await;
            if OBSERVATION.load(Ordering::SeqCst) != observation {
                break "next-dictation";
            }
            if watching.elapsed() >= OBSERVE_FOR {
                break "deadline";
            }
            let (still_focused, reading, returned) = match tokio::task::spawn_blocking(move || {
                let focused = field.is_focused();
                let reading = if focused { field.read(tail) } else { None };
                (focused, reading, field)
            })
            .await
            {
                Ok(result) => result,
                Err(_) => return,
            };
            field = returned;
            if !still_focused {
                break "left-field";
            }
            if let Some(text) = reading.filter(|text| compare(text, &inserted) != After::Gone) {
                latest = Some(text);
            }
        };
        let outcome = tokio::task::spawn_blocking(move || {
            let last = field.read(tail);
            field.close();
            settled(latest, last, &inserted).map(|text| {
                let after = compare(&text, &inserted);
                let observed = match after {
                    After::Edited => Observed {
                        corrections: corrections::corrections(&inserted, &text),
                        layout: layout::seen(&inserted, &text),
                    },
                    After::Same => Observed {
                        corrections: Vec::new(),
                        layout: layout::seen(&inserted, &text),
                    },
                    After::Gone => Observed::default(),
                };
                (after.as_str(), observed)
            })
        })
        .await
        .ok()
        .flatten();
        let (after, observed) = outcome.unwrap_or(("unreadable", Observed::default()));
        tracing::info!(
            "[Readback] app={app} after={after} ended_by={ended_by} watched_s={} corrections={} layout_changed={} layout_kept={}",
            watching.elapsed().as_secs(),
            observed.corrections.len(),
            observed.layout.iter().filter(|seen| seen.changed).count(),
            observed.layout.iter().filter(|seen| !seen.changed).count(),
        );
        if let Some(learn) = learn {
            if !observed.corrections.is_empty() || !observed.layout.is_empty() {
                learn(observed);
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    const INSERTED: &str = "明天下午三点开会记得带上报告";
    const EDITED: &str = "明天下午三点开会记得带上周报";

    #[test]
    fn a_sent_message_is_learned_from_the_last_reading_that_still_held_it() {
        // Sending clears the field or replaces it, so the reading at the end is empty or fails.
        assert_eq!(
            settled(Some(EDITED.into()), Some(String::new()), INSERTED).as_deref(),
            Some(EDITED)
        );
        assert_eq!(
            settled(Some(EDITED.into()), None, INSERTED).as_deref(),
            Some(EDITED)
        );
    }

    #[test]
    fn the_reading_at_the_end_wins_while_it_still_holds_the_dictation() {
        assert_eq!(
            settled(Some(INSERTED.into()), Some(EDITED.into()), INSERTED).as_deref(),
            Some(EDITED)
        );
    }

    #[test]
    fn without_an_earlier_reading_the_one_at_the_end_stands() {
        assert_eq!(
            settled(None, Some(String::new()), INSERTED).as_deref(),
            Some("")
        );
        assert_eq!(settled(None, None, INSERTED), None);
    }

    #[test]
    fn finds_text_that_a_terminal_wrapped_inside_a_frame() {
        let field = "╭────────╮\n│ > 明天下午三点开会，\n│   记得带上报告 │\n╰────────╯";
        assert!(holds_inserted_text(field, "明天下午三点开会，记得带上报告"));
    }

    #[test]
    fn finds_text_an_editor_surrounded_with_its_own_spacing() {
        assert!(holds_inserted_text(
            "Hi team,\n\n send the report today \n",
            "send the report today"
        ));
    }

    #[test]
    fn does_not_find_text_that_is_not_in_the_field() {
        assert!(!holds_inserted_text(
            "something else entirely",
            "send the report today"
        ));
    }

    #[test]
    fn unchanged_text_is_the_same() {
        assert_eq!(
            compare("> 明天下午三点开会", "明天下午三点开会"),
            After::Same
        );
    }

    #[test]
    fn a_corrected_word_is_an_edit() {
        assert_eq!(
            compare(
                "我在终端里的使用习惯不一定代表邮件",
                "我在中段里的使用习惯不一定代表邮件"
            ),
            After::Edited
        );
    }

    #[test]
    fn a_sent_or_cleared_message_is_gone() {
        assert_eq!(compare("", "明天下午三点开会，记得带上报告"), After::Gone);
        assert_eq!(
            compare("好的，收到", "明天下午三点开会，记得带上报告"),
            After::Gone
        );
    }

    #[test]
    fn a_short_insertion_is_compared_whole() {
        assert_eq!(compare("好的", "好"), After::Same);
        assert_eq!(compare("行", "好"), After::Gone);
    }

    #[test]
    fn a_long_field_is_read_far_enough_back_to_hold_the_insertion() {
        assert!(tail_units("好") >= 4_000);
        assert!(tail_units(&"字".repeat(5_000)) >= 10_000);
    }
}
