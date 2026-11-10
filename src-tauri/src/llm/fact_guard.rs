//! Checks that organizing a long dictation did not lose what the speaker said.

use std::collections::HashSet;

use super::prompt::spoken_length;

const MIN_NAMES_TO_JUDGE: usize = 5;
const MIN_NAMES_KEPT_PERCENT: usize = 80;
const MIN_LENGTH_KEPT_PERCENT: usize = 40;

/// Latin words of three letters or more and numbers of two digits or more, lowercased. Shorter ones
/// are mostly recognition noise such as spelled-out letters.
fn names_and_numbers(text: &str) -> HashSet<String> {
    text.split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|token| {
            token.chars().filter(char::is_ascii_alphabetic).count() >= 3
                || token.chars().filter(char::is_ascii_digit).count() >= 2
        })
        .map(str::to_ascii_lowercase)
        .collect()
}

/// Whether the organized text still has the names and numbers of the transcript, and has not
/// shrunk into a summary.
pub fn kept_what_was_said(raw: &str, organized: &str) -> bool {
    if spoken_length(organized) * 100 < spoken_length(raw) * MIN_LENGTH_KEPT_PERCENT {
        return false;
    }
    let said = names_and_numbers(raw);
    if said.len() < MIN_NAMES_TO_JUDGE {
        return true;
    }
    let organized = organized.to_lowercase().replace(',', "");
    let kept = said
        .iter()
        .filter(|token| organized.contains(token.as_str()))
        .count();
    kept * 100 >= said.len() * MIN_NAMES_KEPT_PERCENT
}

#[cfg(test)]
mod tests {
    use super::*;

    const RAW: &str = "Alpha 项目 2026 年交给 Bravo，Charlie 负责 Delta 和 Echo，预算 300 万";

    #[test]
    fn organized_text_with_every_name_and_number_passes() {
        let organized =
            "1. 项目\n- Alpha 2026 年交给 Bravo\n- Charlie 负责 Delta 和 Echo\n- 预算 300 万";
        assert!(kept_what_was_said(RAW, organized));
    }

    #[test]
    fn losing_many_names_and_numbers_fails() {
        assert!(!kept_what_was_said(
            RAW,
            "Alpha 项目交给 Bravo，由负责人跟进，并确认预算和时间"
        ));
    }

    #[test]
    fn a_small_loss_is_tolerated_because_recognition_noise_can_be_cleaned_up() {
        let raw = "Alpha Bravo Charlie Delta Echo Foxtrot Golf Hotel India Juliet";
        let organized = "Alpha Bravo Charlie Delta Echo Foxtrot Golf Hotel India";
        assert!(kept_what_was_said(raw, organized));
    }

    #[test]
    fn letter_case_and_thousands_separators_do_not_count_as_loss() {
        let raw = "alpha bravo charlie delta echo 29800";
        assert!(kept_what_was_said(
            raw,
            "ALPHA Bravo CHARLIE Delta Echo 29,800"
        ));
    }

    #[test]
    fn stray_letters_and_single_digits_are_not_judged() {
        let speech = "明天开会".repeat(10);
        assert!(kept_what_was_said(
            &format!("a b c d e f 1 2 3 4 5 6 {speech}"),
            &speech
        ));
    }

    #[test]
    fn a_lost_name_counts_once_however_often_it_was_said() {
        let raw = "Zulu Zulu Zulu Zulu Zulu Alpha Bravo Charlie Delta Echo Foxtrot Golf";
        assert!(kept_what_was_said(
            raw,
            "Alpha Bravo Charlie Delta Echo Foxtrot Golf"
        ));
    }

    #[test]
    fn text_without_names_is_judged_by_length() {
        let raw = "这句话没有英文也没有数字".repeat(20);
        assert!(kept_what_was_said(&raw, &"这句话没有英文".repeat(20)));
        assert!(!kept_what_was_said(&raw, "这句话没有英文"));
    }
}
