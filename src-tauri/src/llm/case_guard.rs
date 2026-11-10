use std::collections::{HashMap, HashSet};

use super::PolishRequest;
use crate::voice_intent::VoiceIntentKind;

/// Acronyms are short, so capitals on a word this long point at the model rather than the term.
const LONG_WORD_CHARS: usize = 5;
const SHOUTING_EVIDENCE: usize = 2;

#[derive(Default)]
struct SpokenForms {
    /// Lowercased word to its first form that is not all capitals.
    ordinary: HashMap<String, String>,
    in_capitals: HashSet<String>,
}

impl SpokenForms {
    fn from_text(text: &str) -> Self {
        let mut forms = Self::default();
        for word in latin_words(text) {
            let key = word.to_ascii_lowercase();
            if is_all_capitals(word) {
                forms.in_capitals.insert(key);
            } else {
                forms
                    .ordinary
                    .entry(key)
                    .or_insert_with(|| word.to_string());
            }
        }
        forms
    }
}

/// Rewrites, answers and translations may use capitals the transcript does not have.
pub fn applies_to(request: &PolishRequest) -> bool {
    request.voice_intent.kind == VoiceIntentKind::DictateInsert
        && !request.translate_enabled
        && request.selected_text.is_none()
        && request.assistant_name.is_empty()
}

/// A polish model sometimes turns ordinary words into capitals, more often in long mixed text.
/// When at least two long words were changed that way, every shouted word goes back to the form
/// in the transcript. Capitals on a few short words are left alone, since those are usually an
/// acronym the model corrected.
pub fn restore_shouted_words(raw: &str, polished: &str, terms: &[String]) -> String {
    let spoken = SpokenForms::from_text(raw);
    let protected = SpokenForms::from_text(&terms.join(" ")).in_capitals;
    let is_shouted = |word: &str| {
        let key = word.to_ascii_lowercase();
        is_all_capitals(word)
            && spoken.ordinary.contains_key(&key)
            && !spoken.in_capitals.contains(&key)
            && !protected.contains(&key)
    };

    let evidence = latin_words(polished)
        .filter(|word| word.len() >= LONG_WORD_CHARS && is_shouted(word))
        .count();
    if evidence < SHOUTING_EVIDENCE {
        return polished.to_string();
    }

    let mut restored = String::with_capacity(polished.len());
    let mut copied_to = 0;
    for word in latin_words(polished) {
        let start = word.as_ptr() as usize - polished.as_ptr() as usize;
        restored.push_str(&polished[copied_to..start]);
        let key = word.to_ascii_lowercase();
        if is_shouted(word) {
            restored.push_str(&spoken.ordinary[&key]);
        } else if word.len() >= LONG_WORD_CHARS
            && word.chars().all(|c| c.is_ascii_uppercase())
            && !spoken.in_capitals.contains(&key)
            && !protected.contains(&key)
        {
            restored.push_str(&key);
        } else {
            restored.push_str(word);
        }
        copied_to = start + word.len();
    }
    restored.push_str(&polished[copied_to..]);
    restored
}

fn latin_words(text: &str) -> impl Iterator<Item = &str> {
    text.split(|c: char| !(c.is_ascii_alphanumeric() || c == '\''))
        .filter(|word| word.starts_with(|c: char| c.is_ascii_alphabetic()))
}

fn is_all_capitals(word: &str) -> bool {
    word.len() >= 2
        && word.chars().any(|c| c.is_ascii_uppercase())
        && !word.chars().any(|c| c.is_ascii_lowercase())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::{applies_to, restore_shouted_words};
    use crate::llm::{test_dictation_request, PolishRequest};
    use crate::voice_intent::VoiceIntentKind;

    const NO_TERMS: &[String] = &[];

    pub(crate) fn dictation() -> PolishRequest {
        test_dictation_request("")
    }

    #[test]
    fn checks_plain_dictation_only() {
        assert!(applies_to(&dictation()));

        let mut rewrite = dictation();
        rewrite.selected_text = Some("selected".to_string());
        assert!(!applies_to(&rewrite));

        let mut translate = dictation();
        translate.translate_enabled = true;
        assert!(!applies_to(&translate));

        let mut addressed = dictation();
        addressed.assistant_name = "name".to_string();
        assert!(!applies_to(&addressed));

        let mut question = dictation();
        question.voice_intent.kind = VoiceIntentKind::OpenQuestion;
        assert!(!applies_to(&question));
    }

    #[test]
    fn restores_ordinary_words_a_model_turned_into_capitals() {
        let raw = "SQL 上面要做 transformer 和 document retrieval 的 pipeline 测试";
        let polished = "SQL 上面要做 TRANSFORMER 和 DOCUMENT RETRIEVAL 的 PIPELINE 测试";
        assert_eq!(restore_shouted_words(raw, polished, NO_TERMS), raw);
    }

    #[test]
    fn keeps_an_acronym_the_model_corrected() {
        let raw = "把 sql 和 vpn 和 ci 的方案发过来";
        let polished = "把 SQL 和 VPN 和 CI 的方案发过来";
        assert_eq!(restore_shouted_words(raw, polished, NO_TERMS), polished);
    }

    #[test]
    fn one_capitalized_long_word_is_not_enough_evidence() {
        let raw = "这个 kubernetes 集群";
        let polished = "这个 KUBERNETES 集群";
        assert_eq!(restore_shouted_words(raw, polished, NO_TERMS), polished);
    }

    #[test]
    fn restores_short_words_once_the_output_is_shouting() {
        let raw = "the document retrieval is on hold";
        let polished = "THE DOCUMENT RETRIEVAL IS ON HOLD";
        assert_eq!(restore_shouted_words(raw, polished, NO_TERMS), raw);
    }

    #[test]
    fn keeps_a_word_the_speaker_also_said_in_capitals() {
        let raw = "document retrieval 走 SQL，sql 已经部署";
        let polished = "DOCUMENT RETRIEVAL 走 SQL，SQL 已经部署";
        assert_eq!(
            restore_shouted_words(raw, polished, NO_TERMS),
            "document retrieval 走 SQL，SQL 已经部署"
        );
    }

    #[test]
    fn keeps_dictionary_terms_in_capitals() {
        let raw = "document retrieval 和 acme 模型";
        let polished = "DOCUMENT RETRIEVAL 和 ACME 模型";
        let terms = vec!["ACME".to_string()];
        assert_eq!(
            restore_shouted_words(raw, polished, &terms),
            "document retrieval 和 ACME 模型"
        );
    }

    #[test]
    fn lowers_long_shouted_words_the_transcript_does_not_contain() {
        let raw = "document retrieval 的 pipelines";
        let polished = "DOCUMENT RETRIEVAL 的 PIPELINE，VPN";
        assert_eq!(
            restore_shouted_words(raw, polished, NO_TERMS),
            "document retrieval 的 pipeline，VPN"
        );
    }

    #[test]
    fn leaves_text_alone_when_nothing_was_shouted() {
        let raw = "润色 document retrieval";
        let polished = "润色 Document Retrieval，以及 SQL。";
        assert_eq!(restore_shouted_words(raw, polished, NO_TERMS), polished);
    }

    #[test]
    fn keeps_the_original_title_case_form() {
        let raw = "Claude and Document retrieval";
        let polished = "CLAUDE AND DOCUMENT RETRIEVAL";
        assert_eq!(restore_shouted_words(raw, polished, NO_TERMS), raw);
    }
}
