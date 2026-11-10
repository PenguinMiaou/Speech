//! Finds where a dictation addresses the app by the name the user gave it.

use pinyin::ToPinyinMulti;

use crate::app_detector::types::TargetAppGuard;

#[derive(Debug, PartialEq, Eq)]
pub struct Addressed<'a> {
    /// Dictated text before the name. Empty means the instruction is about earlier output.
    pub before: &'a str,
    pub after: &'a str,
}

const FILLERS: [&str; 10] = [
    "嗯", "呃", "啊", "哦", "那个", "um", "uh", "hey", "okay", "ok",
];
const CLAUSE_PUNCTUATION: &str = "，。！？；：、,.!?;:\n\r";
const TERMINAL_IDENTITIES: [&str; 13] = [
    "ghostty",
    "iterm",
    "com.apple.terminal",
    "warp",
    "kitty",
    "alacritty",
    "wezterm",
    "hyper",
    "windowsterminal",
    "conhost",
    "cmd.exe",
    "powershell",
    "pwsh",
];

fn is_only_fillers(text: &str) -> bool {
    let mut rest = text
        .trim_matches(|c: char| c.is_whitespace() || CLAUSE_PUNCTUATION.contains(c))
        .to_lowercase();
    while !rest.is_empty() {
        let Some(filler) = FILLERS.iter().find(|filler| rest.starts_with(**filler)) else {
            return false;
        };
        rest = rest[filler.len()..]
            .trim_start_matches(|c: char| c.is_whitespace() || CLAUSE_PUNCTUATION.contains(c))
            .to_string();
    }
    true
}

fn starts_a_clause(prefix: &str) -> bool {
    is_only_fillers(prefix)
        || prefix
            .trim_end()
            .ends_with(|c: char| CLAUSE_PUNCTUATION.contains(c))
}

fn sounds_the_same(left: char, right: char) -> bool {
    if left.eq_ignore_ascii_case(&right) {
        return true;
    }
    match (left.to_pinyin_multi(), right.to_pinyin_multi()) {
        (Some(left), Some(right)) => left
            .into_iter()
            .any(|one| right.into_iter().any(|other| one.plain() == other.plain())),
        _ => false,
    }
}

/// Byte length of the name at the start of `text`, if it is there.
fn name_length(text: &str, name: &str) -> Option<usize> {
    let mut heard = text.char_indices();
    let mut length = 0;
    for expected in name.chars() {
        let (index, actual) = heard.next()?;
        if !sounds_the_same(actual, expected) {
            return None;
        }
        length = index + actual.len_utf8();
    }
    Some(length)
}

pub fn find_addressed<'a>(text: &'a str, name: &str) -> Option<Addressed<'a>> {
    let name = name.trim();
    if name.is_empty() {
        return None;
    }
    for (start, _) in text.char_indices() {
        let prefix = &text[..start];
        if !starts_a_clause(prefix) {
            continue;
        }
        let Some(length) = name_length(&text[start..], name) else {
            continue;
        };
        let after = &text[start + length..];
        if after.starts_with(|c: char| c.is_ascii_alphanumeric())
            && name.ends_with(|c: char| c.is_ascii_alphanumeric())
        {
            continue;
        }
        let instruction =
            after.trim_start_matches(|c: char| c.is_whitespace() || CLAUSE_PUNCTUATION.contains(c));
        if instruction.is_empty() || instruction.starts_with('的') {
            continue;
        }
        let before = if is_only_fillers(prefix) {
            ""
        } else {
            prefix.trim_end()
        };
        return Some(Addressed {
            before,
            after: instruction.trim_end(),
        });
    }
    None
}

/// In a terminal, Shift+Left sends an escape sequence to the shell instead of selecting text.
pub fn is_terminal_target(guard: &TargetAppGuard) -> bool {
    guard.native_identity.as_deref().is_some_and(|identity| {
        let identity = identity.to_ascii_lowercase();
        TERMINAL_IDENTITIES
            .iter()
            .any(|terminal| identity.contains(terminal))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn find<'a>(text: &'a str, name: &str) -> Option<(&'a str, &'a str)> {
        find_addressed(text, name).map(|found| (found.before, found.after))
    }

    #[test]
    fn finds_the_name_at_the_start() {
        assert_eq!(
            find("小克，把前面翻译成英文", "小克"),
            Some(("", "把前面翻译成英文"))
        );
    }

    #[test]
    fn accepts_a_homophone_of_the_name() {
        for heard in ["小可", "小科", "晓克"] {
            let text = format!("{heard}，把前面翻译成英文");
            assert_eq!(find(&text, "小克").unwrap().1, "把前面翻译成英文");
        }
    }

    #[test]
    fn rejects_a_name_that_sounds_different() {
        assert_eq!(find("小张，把前面翻译成英文", "小克"), None);
    }

    #[test]
    fn splits_a_dictation_that_addresses_the_app_midway() {
        assert_eq!(
            find("今天开会讨论预算。小克，把前面翻译成英文", "小克"),
            Some(("今天开会讨论预算。", "把前面翻译成英文"))
        );
    }

    #[test]
    fn ignores_the_name_inside_a_clause() {
        assert_eq!(find("我去找小克问一下", "小克"), None);
    }

    #[test]
    fn ignores_a_possessive_mention() {
        assert_eq!(find("小克的报告发我一下", "小克"), None);
    }

    #[test]
    fn needs_words_after_the_name() {
        assert_eq!(find("小克。", "小克"), None);
    }

    #[test]
    fn treats_fillers_before_the_name_as_nothing() {
        assert_eq!(
            find("嗯，那个，小克，把前面翻译成英文", "小克"),
            Some(("", "把前面翻译成英文"))
        );
    }

    #[test]
    fn matches_a_latin_name_on_word_boundaries() {
        assert_eq!(
            find("jarvis, translate that", "Jarvis"),
            Some(("", "translate that"))
        );
        assert_eq!(
            find("Hello there. Jarvis translate that", "Jarvis"),
            Some(("Hello there.", "translate that"))
        );
        assert_eq!(find("Jarvisson is here", "Jarvis"), None);
    }

    #[test]
    fn stays_off_without_a_name() {
        assert_eq!(find("小克，把前面翻译成英文", "  "), None);
    }

    #[test]
    fn recognizes_terminal_apps() {
        let guard = |identity: &str| TargetAppGuard {
            process_id: Some(1),
            native_identity: Some(identity.to_string()),
        };
        assert!(is_terminal_target(&guard("com.mitchellh.ghostty")));
        assert!(is_terminal_target(&guard("WindowsTerminal.exe")));
        assert!(!is_terminal_target(&guard("com.apple.TextEdit")));
        assert!(!is_terminal_target(&TargetAppGuard::default()));
    }
}
