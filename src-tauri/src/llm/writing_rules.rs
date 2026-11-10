//! Instructions that follow from two settings: which Chinese script to write in, and how
//! much punctuation polish may add.

/// The instructions for the chosen script and punctuation mode. The defaults add none.
pub fn writing_rules(chinese_script: &str, punctuation_mode: &str) -> Vec<String> {
    let mut rules = Vec::new();
    match chinese_script {
        "simplified" => rules.push(
            "Write all Chinese in Simplified Chinese characters, converting any Traditional characters."
                .to_string(),
        ),
        "traditional" => rules.push(
            "Write all Chinese in Traditional Chinese characters, converting any Simplified characters."
                .to_string(),
        ),
        _ => {}
    }
    match punctuation_mode {
        "spoken" => rules.push(
            "Add no punctuation of your own. Only where the speaker says the name of a mark (comma, period, question mark, exclamation mark, colon, new line, 逗号, 句号, 问号, 感叹号, 冒号, 换行), put that mark there and drop the spoken name."
                .to_string(),
        ),
        "none" => rules.push(
            "Output no punctuation marks at all. Separate clauses with a single space."
                .to_string(),
        ),
        _ => {}
    }
    rules
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_defaults_add_no_instruction() {
        assert!(writing_rules("preserve", "auto").is_empty());
        assert!(writing_rules("", "").is_empty());
        assert!(writing_rules("something else", "unknown").is_empty());
    }

    #[test]
    fn a_chosen_script_is_asked_for_by_name() {
        let simplified = writing_rules("simplified", "auto");
        assert_eq!(simplified.len(), 1);
        assert!(simplified[0].contains("Simplified Chinese"));
        assert!(writing_rules("traditional", "auto")[0].contains("Traditional Chinese"));
    }

    #[test]
    fn punctuation_only_on_command_names_the_spoken_marks() {
        let rules = writing_rules("preserve", "spoken");
        assert_eq!(rules.len(), 1);
        assert!(rules[0].contains("no punctuation of your own"));
        assert!(rules[0].contains("逗号"));
        assert!(rules[0].contains("new line"));
    }

    #[test]
    fn punctuation_can_be_left_out_altogether() {
        let rules = writing_rules("traditional", "none");
        assert_eq!(rules.len(), 2);
        assert!(rules[1].contains("no punctuation marks"));
    }
}
