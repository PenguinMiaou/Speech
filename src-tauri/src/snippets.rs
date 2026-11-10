//! Spoken shortcuts: say a saved phrase by itself and its text is entered as written,
//! without going to the polish model.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Snippet {
    pub id: i64,
    pub phrase: String,
    pub text: String,
}

/// What is left of a phrase once case, spacing and punctuation are set aside.
pub fn normalized(phrase: &str) -> String {
    phrase
        .chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

/// The text of the snippet whose phrase is the whole of what was said.
pub fn matching<'a>(said: &str, snippets: &'a [Snippet]) -> Option<&'a str> {
    let said = normalized(said);
    if said.is_empty() {
        return None;
    }
    snippets
        .iter()
        .find(|snippet| normalized(&snippet.phrase) == said)
        .map(|snippet| snippet.text.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn saved() -> Vec<Snippet> {
        vec![
            Snippet {
                id: 1,
                phrase: "我的邮箱".to_string(),
                text: "someone@example.com".to_string(),
            },
            Snippet {
                id: 2,
                phrase: "Thanks note".to_string(),
                text: "Thank you, that helps.\nTalk soon.".to_string(),
            },
        ]
    }

    #[test]
    fn a_phrase_said_by_itself_gives_its_text_as_written() {
        let saved = saved();
        assert_eq!(matching("我的邮箱", &saved), Some("someone@example.com"));
        assert_eq!(
            matching("thanks note", &saved),
            Some("Thank you, that helps.\nTalk soon.")
        );
    }

    #[test]
    fn punctuation_spacing_and_case_from_recognition_do_not_matter() {
        let saved = saved();
        assert_eq!(matching(" 我的邮箱。", &saved), Some("someone@example.com"));
        assert_eq!(matching("我的 邮箱！", &saved), Some("someone@example.com"));
        assert!(matching("Thanks, note.", &saved).is_some());
    }

    #[test]
    fn a_phrase_inside_a_longer_sentence_is_ordinary_dictation() {
        let saved = saved();
        assert_eq!(matching("把我的邮箱发给他", &saved), None);
        assert_eq!(matching("我的邮箱是什么", &saved), None);
    }

    #[test]
    fn nothing_matches_silence_or_an_empty_phrase() {
        let blank = vec![Snippet {
            id: 1,
            phrase: "。".to_string(),
            text: "x".to_string(),
        }];
        assert_eq!(matching("", &saved()), None);
        assert_eq!(matching("。", &blank), None);
    }
}
