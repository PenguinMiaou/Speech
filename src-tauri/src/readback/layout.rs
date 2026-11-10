//! Notices what the user does to dictated text after it lands: whether they delete the
//! final period, join separate lines, swap the polite and plain "you", take out
//! exclamation marks, or open or close the space between Chinese and Latin text.
//! Only the conclusion leaves this module, never the text.

const ANCHOR: usize = 6;
const MIN_ANCHOR: usize = 3;
const AROUND_BREAK: usize = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Habit {
    NoFinalPeriod,
    OneParagraph,
    PlainYou,
    PoliteYou,
    NoExclamation,
    NoCjkLatinSpace,
    CjkLatinSpace,
}

impl Habit {
    pub const ALL: [Habit; 7] = [
        Habit::NoFinalPeriod,
        Habit::OneParagraph,
        Habit::PlainYou,
        Habit::PoliteYou,
        Habit::NoExclamation,
        Habit::NoCjkLatinSpace,
        Habit::CjkLatinSpace,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Habit::NoFinalPeriod => "no_final_period",
            Habit::OneParagraph => "one_paragraph",
            Habit::PlainYou => "plain_you",
            Habit::PoliteYou => "polite_you",
            Habit::NoExclamation => "no_exclamation",
            Habit::NoCjkLatinSpace => "no_cjk_latin_space",
            Habit::CjkLatinSpace => "cjk_latin_space",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|habit| habit.as_str() == name)
    }

    /// What polish is told once the habit is established.
    pub fn instruction(self) -> &'static str {
        match self {
            Habit::NoFinalPeriod => "Do not put a period at the very end of the output.",
            Habit::OneParagraph => {
                "Output a single paragraph. No line breaks, no numbered list and no bullets anywhere, even for several separate points; join them into running sentences with punctuation."
            }
            Habit::PlainYou => "In Chinese, address the reader as 你, never 您.",
            Habit::PoliteYou => "In Chinese, address the reader as 您, never 你.",
            Habit::NoExclamation => {
                "Do not use exclamation marks; end those sentences with a period."
            }
            Habit::NoCjkLatinSpace => {
                "Do not put a space between Chinese characters and Latin letters or digits."
            }
            Habit::CjkLatinSpace => {
                "Put one space between Chinese characters and Latin letters or digits."
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

/// What the field now holds in place of `inserted[from..to]`, located by the unchanged
/// text on both sides. `None` when that text can no longer be found.
fn replaced(inserted: &[char], from: usize, to: usize, field: &[char]) -> Option<Vec<char>> {
    let before: Vec<char> = {
        let mut text: Vec<char> = inserted[..from]
            .iter()
            .rev()
            .take(AROUND_BREAK)
            .take_while(|c| !c.is_whitespace())
            .copied()
            .collect();
        text.reverse();
        text
    };
    let after: Vec<char> = inserted[to..]
        .iter()
        .take(AROUND_BREAK)
        .take_while(|c| !c.is_whitespace())
        .copied()
        .collect();
    if before.len() < 2 {
        return None;
    }
    let rest = &field[find_last(field, &before)? + before.len()..];
    if after.is_empty() {
        // The end of what was dictated: whatever follows directly is the replacement.
        let at_end = inserted[to..].iter().all(|c| c.is_whitespace());
        return at_end.then(|| {
            rest.iter()
                .take(1)
                .take_while(|c| !c.is_whitespace())
                .copied()
                .collect()
        });
    }
    if after.len() < 2 {
        return None;
    }
    (0..=3.min(rest.len()))
        .find(|gap| rest[*gap..].starts_with(&after))
        .map(|gap| rest[..gap].to_vec())
}

fn vote(changed: usize, kept: usize) -> Option<bool> {
    match changed.cmp(&kept) {
        std::cmp::Ordering::Greater => Some(true),
        std::cmp::Ordering::Less => Some(false),
        std::cmp::Ordering::Equal => None,
    }
}

/// Whether the user swapped every `from` they were given for `to`.
fn swapped(inserted: &[char], field: &[char], from: char, to: char) -> Option<bool> {
    let (mut changed, mut kept) = (0, 0);
    for (at, _) in inserted.iter().enumerate().filter(|(_, c)| **c == from) {
        match replaced(inserted, at, at + 1, field).as_deref() {
            Some([now]) if *now == to => changed += 1,
            Some([now]) if *now == from => kept += 1,
            _ => {}
        }
    }
    vote(changed, kept)
}

fn is_exclamation(c: char) -> bool {
    c == '！' || c == '!'
}

fn exclamations(inserted: &[char], field: &[char]) -> Option<bool> {
    let (mut changed, mut kept) = (0, 0);
    for (at, _) in inserted
        .iter()
        .enumerate()
        .filter(|(_, c)| is_exclamation(**c))
    {
        match replaced(inserted, at, at + 1, field) {
            Some(now) if now.iter().any(|c| is_exclamation(*c)) => kept += 1,
            Some(_) => changed += 1,
            None => {}
        }
    }
    vote(changed, kept)
}

fn is_cjk(c: char) -> bool {
    ('\u{4e00}'..='\u{9fff}').contains(&c)
}

fn is_latin(c: char) -> bool {
    c.is_ascii_alphanumeric()
}

fn meets(a: char, b: char) -> bool {
    (is_cjk(a) && is_latin(b)) || (is_latin(a) && is_cjk(b))
}

/// The user's verdict on the space where Chinese meets Latin text. `spaced` picks the
/// places that were written with a space, or the places written without one.
fn cjk_latin_space(inserted: &[char], field: &[char], spaced: bool) -> Option<bool> {
    let (mut changed, mut kept) = (0, 0);
    for at in 1..inserted.len() {
        let (from, to) = if spaced {
            if at + 1 >= inserted.len()
                || inserted[at] != ' '
                || !meets(inserted[at - 1], inserted[at + 1])
            {
                continue;
            }
            (at, at + 1)
        } else {
            if !meets(inserted[at - 1], inserted[at]) {
                continue;
            }
            (at, at)
        };
        let Some(now) = replaced(inserted, from, to, field) else {
            continue;
        };
        let has_space = now.as_slice() == [' '];
        if !has_space && !now.is_empty() {
            continue;
        }
        if has_space == spaced {
            kept += 1;
        } else {
            changed += 1;
        }
    }
    vote(changed, kept)
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
    let style = [
        (Habit::PlainYou, swapped(&inserted, &field, '您', '你')),
        (Habit::PoliteYou, swapped(&inserted, &field, '你', '您')),
        (Habit::NoExclamation, exclamations(&inserted, &field)),
        (
            Habit::NoCjkLatinSpace,
            cjk_latin_space(&inserted, &field, true),
        ),
        (
            Habit::CjkLatinSpace,
            cjk_latin_space(&inserted, &field, false),
        ),
    ];
    for (habit, verdict) in style {
        if let Some(changed) = verdict {
            seen.push(Seen { habit, changed });
        }
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

    fn only(list: Vec<Seen>, habit: Habit) -> Option<bool> {
        list.into_iter()
            .find(|seen| seen.habit == habit)
            .map(|seen| seen.changed)
    }

    #[test]
    fn changing_the_polite_you_to_the_plain_one_is_noticed() {
        let inserted = "明天您方便的话我们见一面";
        assert_eq!(
            only(seen(inserted, "明天你方便的话我们见一面"), Habit::PlainYou),
            Some(true)
        );
        assert_eq!(only(seen(inserted, inserted), Habit::PlainYou), Some(false));
    }

    #[test]
    fn changing_the_plain_you_to_the_polite_one_is_noticed() {
        let inserted = "麻烦你看一下附件";
        assert_eq!(
            only(seen(inserted, "王总，麻烦您看一下附件"), Habit::PoliteYou),
            Some(true)
        );
        assert_eq!(
            only(seen(inserted, inserted), Habit::PoliteYou),
            Some(false)
        );
    }

    #[test]
    fn removing_exclamation_marks_is_noticed() {
        let inserted = "太好了！我们周五出发！";
        assert_eq!(
            only(
                seen(inserted, "太好了。我们周五出发。"),
                Habit::NoExclamation
            ),
            Some(true)
        );
        assert_eq!(
            only(seen(inserted, "太好了，我们周五出发"), Habit::NoExclamation),
            Some(true)
        );
        assert_eq!(
            only(seen(inserted, inserted), Habit::NoExclamation),
            Some(false)
        );
    }

    #[test]
    fn closing_up_the_space_between_chinese_and_latin_is_noticed() {
        let inserted = "我们用 GitHub 发布新版本";
        assert_eq!(
            only(
                seen(inserted, "我们用GitHub发布新版本"),
                Habit::NoCjkLatinSpace
            ),
            Some(true)
        );
        assert_eq!(
            only(seen(inserted, inserted), Habit::NoCjkLatinSpace),
            Some(false)
        );
        assert_eq!(only(seen(inserted, inserted), Habit::CjkLatinSpace), None);
    }

    #[test]
    fn opening_a_space_between_chinese_and_latin_is_noticed() {
        let inserted = "我们用GitHub发布新版本";
        assert_eq!(
            only(
                seen(inserted, "我们用 GitHub 发布新版本"),
                Habit::CjkLatinSpace
            ),
            Some(true)
        );
        assert_eq!(
            only(seen(inserted, inserted), Habit::CjkLatinSpace),
            Some(false)
        );
    }

    #[test]
    fn text_that_gives_no_sign_of_a_style_says_nothing_about_it() {
        let quiet = seen("今天下午三点开会", "今天下午三点开会");
        for habit in [
            Habit::PlainYou,
            Habit::PoliteYou,
            Habit::NoExclamation,
            Habit::NoCjkLatinSpace,
            Habit::CjkLatinSpace,
        ] {
            assert_eq!(only(quiet.clone(), habit), None, "{habit:?}");
        }
        // The field no longer holds the dictated text at all.
        assert!(seen("麻烦您看一下！", "完全不同的内容").is_empty());
    }

    #[test]
    fn every_habit_has_a_name_that_reads_back_and_an_instruction() {
        for habit in Habit::ALL {
            assert_eq!(Habit::from_name(habit.as_str()), Some(habit));
            assert!(!habit.instruction().is_empty());
        }
        assert_eq!(Habit::ALL.len(), 7);
    }
}
