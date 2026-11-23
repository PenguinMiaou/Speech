//! The questions the user keeps to ask of any meeting, such as a follow-up email or the
//! risks raised, stored on this machine.

use std::path::PathBuf;

/// The most questions kept, and the longest one, in characters.
const MOST_QUESTIONS: usize = 20;
const LONGEST_QUESTION: usize = 300;

fn store() -> Option<PathBuf> {
    crate::minutes::data_dir().map(|dir| dir.join("meeting-questions.json"))
}

/// The questions kept, in the order the user keeps them.
pub fn list() -> Vec<String> {
    store()
        .and_then(|path| std::fs::read(path).ok())
        .and_then(|raw| serde_json::from_slice::<Vec<String>>(&raw).ok())
        .map(|questions| tidy(&questions))
        .unwrap_or_default()
}

/// Keeps `questions` as the user left them, tidied; returns what is now kept.
pub fn save(questions: &[String]) -> Result<Vec<String>, String> {
    let kept = tidy(questions);
    let path = store().ok_or_else(|| "questions_store_unavailable".to_string())?;
    let raw = serde_json::to_vec_pretty(&kept).map_err(|error| error.to_string())?;
    std::fs::write(path, raw).map_err(|error| error.to_string())?;
    Ok(kept)
}

/// The questions without blanks, repeats or overlong ones, at most `MOST_QUESTIONS`.
fn tidy(questions: &[String]) -> Vec<String> {
    let mut kept: Vec<String> = Vec::new();
    for question in questions {
        let question = question.trim();
        if question.is_empty()
            || question.chars().count() > LONGEST_QUESTION
            || kept.iter().any(|was| was == question)
        {
            continue;
        }
        kept.push(question.to_string());
        if kept.len() == MOST_QUESTIONS {
            break;
        }
    }
    kept
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn questions_are_kept_without_blanks_repeats_or_overlong_ones() {
        let many: Vec<String> = (0..30).map(|n| format!("question {n}")).collect();
        assert_eq!(tidy(&many).len(), MOST_QUESTIONS);
        let asked = vec![
            " Write a follow-up email ".to_string(),
            String::new(),
            "Write a follow-up email".to_string(),
            "x".repeat(LONGEST_QUESTION + 1),
            "List the risks".to_string(),
        ];
        assert_eq!(
            tidy(&asked),
            vec!["Write a follow-up email", "List the risks"]
        );
    }
}
