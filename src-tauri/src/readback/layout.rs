//! Notices what the user does to the layout of dictated text: whether they delete the
//! final period, and whether they join separate lines into one paragraph. Only the
//! conclusion leaves this module, never the text.

const ANCHOR: usize = 6;
const MIN_ANCHOR: usize = 3;
const AROUND_BREAK: usize = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Habit {
    NoFinalPeriod,
    OneParagraph,
}

impl Habit {
    pub fn as_str(self) -> &'static str {
        match self {
            Habit::NoFinalPeriod => "no_final_period",
            Habit::OneParagraph => "one_paragraph",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        [Habit::NoFinalPeriod, Habit::OneParagraph]
            .into_iter()
            .find(|habit| habit.as_str() == name)
    }

    /// What polish is told once the habit is established.
    pub fn instruction(self) -> &'static str {
        match self {
            Habit::NoFinalPeriod => "Do not put a period at the very end of the output.",
            Habit::OneParagraph => {
                "Output a single paragraph. No line breaks, no numbered list and no bullets anywhere, even for several separate points; join them into running sentences with punctuation."
            }
        }
    }
}

/// One dictation's evidence about a habit: the user changed the layout, or left it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Seen {
    pub habit: Habit,
    pub changed: bool,
}

fn is_period(c: char) -> bool {
    c == '。' || c == '.'
}

fn find_last(haystack: &[char], needle: &[char]) -> Option<usize> {
    if needle.is_empty() || needle.len() > haystack.len() {
        return None;
    }
    (0..=haystack.len() - needle.len())
        .rev()
        .find(|start| haystack[*start..*start + needle.len()] == *needle)
}

fn final_period(inserted: &[char], field: &[char]) -> Option<bool> {
    let end = inserted.iter().rposition(|c| !c.is_whitespace())?;
    if !is_period(inserted[end]) {
        return None;
    }
    let body = &inserted[..end];
    let anchor = &body[body.len().saturating_sub(ANCHOR)..];
    if anchor.len() < MIN_ANCHOR {
        return None;
    }
    let after = find_last(field, anchor)? + anchor.len();
    Some(!field.get(after).is_some_and(|c| is_period(*c)))
}

fn line_breaks(inserted: &[char], field: &[char]) -> Option<bool> {
    let (mut joined, mut kept) = (0usize, 0usize);
    for (at, _) in inserted.iter().enumerate().filter(|(_, c)| **c == '\n') {
        let before: Vec<char> = {
            let text: Vec<char> = inserted[..at]
                .iter()
                .rev()
                .skip_while(|c| c.is_whitespace())
                .copied()
                .collect();
            text.into_iter()
                .take(AROUND_BREAK)
                .take_while(|c| !c.is_whitespace())
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect()
        };
        let after: Vec<char> = inserted[at..]
            .iter()
            .skip_while(|c| c.is_whitespace())
            .take(AROUND_BREAK)
            .take_while(|c| !c.is_whitespace())
            .copied()
            .collect();
        if before.len() < 2 || after.len() < 2 {
            continue;
        }
        let Some(start) = find_last(field, &before) else {
            continue;
        };
        let rest = &field[start + before.len()..];
        let gap = rest
            .iter()
            .take_while(|c| c.is_whitespace() || is_frame(**c))
            .count();
        if !rest[gap..].starts_with(&after) {
            continue;
        }
        if rest[..gap].iter().any(|c| *c == '\n' || is_frame(*c)) {
            kept += 1;
        } else {
            joined += 1;
        }
    }
    match joined.cmp(&kept) {
        std::cmp::Ordering::Greater => Some(true),
        std::cmp::Ordering::Less => Some(false),
        std::cmp::Ordering::Equal => None,
    }
}

/// Box-drawing characters, which terminals put around and between lines.
fn is_frame(c: char) -> bool {
    ('\u{2500}'..='\u{259F}').contains(&c)
}

/// What `field_after` shows about the user's layout habits for the dictated `inserted`.
pub fn seen(inserted: &str, field_after: &str) -> Vec<Seen> {
    let inserted: Vec<char> = inserted.chars().collect();
    let field: Vec<char> = field_after.chars().collect();
    let mut seen = Vec::new();
    if let Some(changed) = final_period(&inserted, &field) {
        seen.push(Seen {
            habit: Habit::NoFinalPeriod,
            changed,
        });
    }
    if let Some(changed) = line_breaks(&inserted, &field) {
        seen.push(Seen {
            habit: Habit::OneParagraph,
            changed,
        });
    }
    seen
}

#[cfg(test)]
mod tests {
    use super::*;

    fn changed(habit: Habit) -> Vec<Seen> {
        vec![Seen {
            habit,
            changed: true,
        }]
    }

    fn kept(habit: Habit) -> Vec<Seen> {
        vec![Seen {
            habit,
            changed: false,
        }]
    }

    #[test]
    fn deleting_the_final_period_is_noticed() {
        assert_eq!(
            seen("今天下午三点开会。", "> 今天下午三点开会"),
            changed(Habit::NoFinalPeriod)
        );
        assert_eq!(
            seen("See you at three.", "See you at three\n"),
            changed(Habit::NoFinalPeriod)
        );
    }

    #[test]
    fn a_final_period_left_in_place_counts_as_kept() {
        assert_eq!(
            seen("今天下午三点开会。", "> 今天下午三点开会。"),
            kept(Habit::NoFinalPeriod)
        );
        assert_eq!(
            seen("今天下午三点开会。", "今天下午三点开会。记得带电脑"),
            kept(Habit::NoFinalPeriod)
        );
    }

    #[test]
    fn text_that_never_ended_in_a_period_says_nothing() {
        assert!(seen("今天下午三点开会", "今天下午三点开会").is_empty());
        assert!(seen("今天下午三点开会？", "今天下午三点开会").is_empty());
    }

    #[test]
    fn joining_the_lines_is_noticed() {
        assert_eq!(
            seen("先查日志\n再重启服务", "先查日志 再重启服务"),
            changed(Habit::OneParagraph)
        );
        assert_eq!(
            seen(
                "先查日志\n再重启服务\n最后发通知",
                "先查日志再重启服务最后发通知"
            ),
            changed(Habit::OneParagraph)
        );
    }

    #[test]
    fn lines_left_apart_count_as_kept() {
        assert_eq!(
            seen("先查日志\n再重启服务", "│ 先查日志 │\n│ 再重启服务 │"),
            kept(Habit::OneParagraph)
        );
        assert_eq!(
            seen("先查日志\n再重启服务", "先查日志\n  再重启服务"),
            kept(Habit::OneParagraph)
        );
    }

    #[test]
    fn text_the_user_rewrote_says_nothing() {
        assert!(seen("今天下午三点开会。", "明天再说").is_empty());
        assert!(seen("先查日志\n再重启服务", "改成别的内容").is_empty());
    }

    #[test]
    fn a_habit_is_found_again_by_its_name() {
        for habit in [Habit::NoFinalPeriod, Habit::OneParagraph] {
            assert_eq!(Habit::from_name(habit.as_str()), Some(habit));
        }
        assert_eq!(Habit::from_name("unknown"), None);
    }
}
