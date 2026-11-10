use enigo::Keyboard as _;
use unicode_segmentation::UnicodeSegmentation;

const MAX_BACKWARD_SELECTION: usize = 2000;
const SELECTION_SETTLE_MS: u64 = 30;

#[cfg(not(target_os = "macos"))]
use enigo::{Direction, Enigo, Key, Keyboard, Settings as EnigoSettings};

const CLIPBOARD_COPY_SETTLE_MS: u64 = 100;

pub fn selected_text_from_clipboard_result(
    selected: Option<String>,
    sentinel: &str,
) -> Option<String> {
    match selected {
        Some(text) if !text.trim().is_empty() && text != sentinel => Some(text),
        _ => None,
    }
}

fn clipboard_copy_sentinel() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    format!(
        "__opentypeless_copy_sentinel_{}_{}__",
        std::process::id(),
        nanos
    )
}

#[cfg(target_os = "macos")]
fn copy_selected_text_to_clipboard() -> bool {
    match std::process::Command::new("/usr/bin/osascript")
        .args([
            "-e",
            r#"tell application "System Events" to keystroke "c" using command down"#,
        ])
        .status()
    {
        Ok(status) if status.success() => true,
        Ok(status) => {
            tracing::warn!(
                "macOS selected-text copy failed with exit code: {:?}",
                status.code()
            );
            false
        }
        Err(e) => {
            tracing::warn!("Failed to run osascript for selected-text copy: {}", e);
            false
        }
    }
}

#[cfg(not(target_os = "macos"))]
fn copy_selected_text_to_clipboard() -> bool {
    let Ok(mut enigo) = Enigo::new(&EnigoSettings::default()) else {
        return false;
    };

    let pressed = enigo.key(Key::Control, Direction::Press).is_ok();
    if pressed {
        let _ = enigo.key(Key::Unicode('c'), Direction::Click);
        let _ = enigo.key(Key::Control, Direction::Release);
    }
    pressed
}

pub fn capture_selected_text() -> Option<String> {
    let mut clipboard = arboard::Clipboard::new().ok()?;
    let backup = clipboard.get_text().ok();
    let sentinel = clipboard_copy_sentinel();
    let _ = clipboard.set_text(&sentinel);

    if !copy_selected_text_to_clipboard() {
        tracing::debug!("Selected text copy shortcut could not be sent");
    }

    std::thread::sleep(std::time::Duration::from_millis(CLIPBOARD_COPY_SETTLE_MS));

    let selected = clipboard.get_text().ok();

    if let Some(ref b) = backup {
        let _ = clipboard.set_text(b);
    } else {
        let _ = clipboard.set_text("");
    }

    tracing::info!(
        "Selected text capture: backup_len={}, selected_len={}",
        backup.as_deref().map(|s| s.len()).unwrap_or(0),
        selected.as_deref().map(|s| s.len()).unwrap_or(0)
    );

    let result = selected_text_from_clipboard_result(selected, &sentinel);
    if result.is_none() {
        tracing::debug!("Selected text capture did not produce fresh clipboard text");
    }
    result
}

/// Left-arrow presses needed to select `text` backwards from the end of it.
pub fn backward_selection_steps(text: &str) -> Option<usize> {
    let steps = text.graphemes(true).count();
    (steps > 0 && steps <= MAX_BACKWARD_SELECTION).then_some(steps)
}

/// Apps may change line endings and spacing, so those differences do not count.
pub fn is_same_text(left: &str, right: &str) -> bool {
    !left.trim().is_empty() && left.split_whitespace().eq(right.split_whitespace())
}

/// The default macOS delay is 20 ms before each press and each release.
fn selection_key_settings() -> enigo::Settings {
    enigo::Settings {
        mac_delay: 1,
        ..Default::default()
    }
}

pub fn select_backwards(steps: usize) -> bool {
    let Ok(mut keyboard) = enigo::Enigo::new(&selection_key_settings()) else {
        return false;
    };
    if keyboard
        .key(enigo::Key::Shift, enigo::Direction::Press)
        .is_err()
    {
        return false;
    }
    let selected = (0..steps).all(|_| {
        keyboard
            .key(enigo::Key::LeftArrow, enigo::Direction::Click)
            .is_ok()
    });
    let _ = keyboard.key(enigo::Key::Shift, enigo::Direction::Release);
    std::thread::sleep(std::time::Duration::from_millis(SELECTION_SETTLE_MS));
    selected
}

/// Puts the cursor back at the end of the text a failed check left selected.
pub fn collapse_selection() {
    if let Ok(mut keyboard) = enigo::Enigo::new(&enigo::Settings::default()) {
        let _ = keyboard.key(enigo::Key::RightArrow, enigo::Direction::Click);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selection_counts_what_the_cursor_steps_over() {
        assert_eq!(backward_selection_steps("abc"), Some(3));
        assert_eq!(backward_selection_steps("小克"), Some(2));
        assert_eq!(backward_selection_steps("ok👍🏽"), Some(3));
        assert_eq!(backward_selection_steps("a\r\nb"), Some(3));
    }

    #[test]
    fn selection_refuses_empty_or_very_long_text() {
        assert_eq!(backward_selection_steps(""), None);
        assert_eq!(backward_selection_steps(&"字".repeat(2001)), None);
        assert_eq!(backward_selection_steps(&"字".repeat(2000)), Some(2000));
    }

    #[test]
    fn selecting_the_longest_text_stays_under_five_seconds() {
        let press_and_release_ms = 2 * u64::from(selection_key_settings().mac_delay);
        assert!(press_and_release_ms * MAX_BACKWARD_SELECTION as u64 <= 5000);
    }

    #[test]
    fn same_text_ignores_only_spacing_differences() {
        assert!(is_same_text("第一行\n第二行", " 第一行\r\n第二行\n"));
        assert!(is_same_text("a  b", "a b"));
        assert!(!is_same_text("小克", "小可"));
        assert!(!is_same_text("abc", "abcd"));
        assert!(!is_same_text("", ""));
    }

    #[test]
    fn selected_text_rejects_copy_sentinel_when_clipboard_was_unchanged() {
        assert_eq!(
            selected_text_from_clipboard_result(Some("__sentinel__".to_string()), "__sentinel__"),
            None
        );
    }

    #[test]
    fn selected_text_accepts_text_that_matches_previous_clipboard_backup() {
        assert_eq!(
            selected_text_from_clipboard_result(Some("selected text".to_string()), "__sentinel__"),
            Some("selected text".to_string())
        );
    }

    #[test]
    fn selected_text_rejects_whitespace_only_clipboard() {
        assert_eq!(
            selected_text_from_clipboard_result(Some(" \n\t ".to_string()), "__sentinel__"),
            None
        );
    }
}
