//! Finds the words a user corrected by hand in dictated text. A changed word counts as a
//! correction only when the two spellings sound alike; anything else is a change of mind.

use pinyin::{ToPinyin, ToPinyinMulti};

/// A word that was heard one way and corrected to another.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Correction {
    pub heard: String,
    pub corrected: String,
}

/// Longest dictation, in characters, that is compared.
const MAX_COMPARED: usize = 1_500;
/// Length of the pieces of the dictation searched for in the field to find where it sits.
const ANCHOR: usize = 4;
/// Field text kept on each side of the dictation, so a correction at its edge is still seen.
const CONTEXT: usize = 8;
const MAX_WORD: usize = 32;
const MAX_CORRECTIONS: usize = 3;
/// When more than this share of the dictation changed, the user rewrote it.
const REWRITE_SHARE: f32 = 0.5;

fn is_frame(c: char) -> bool {
    ('\u{2500}'..='\u{259f}').contains(&c)
}

fn is_han(c: char) -> bool {
    matches!(c as u32, 0x3400..=0x4dbf | 0x4e00..=0x9fff | 0xf900..=0xfaff | 0x20000..=0x2ffff)
}

/// Part of a word written in Latin letters, such as a product name.
fn is_token(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '-' || c == '_'
}

/// Text without frames and line wrapping. Spaces stay only between Latin words.
fn diffable(text: &str) -> Vec<char> {
    let mut out: Vec<char> = Vec::new();
    let mut space = false;
    for c in text.chars().filter(|c| !is_frame(*c)) {
        if c.is_whitespace() {
            space = true;
            continue;
        }
        if space && c.is_ascii_alphanumeric() && out.last().is_some_and(char::is_ascii_alphanumeric)
        {
            out.push(' ');
        }
        space = false;
        out.push(c);
    }
    out
}

fn last_position(haystack: &[char], needle: &[char], from: usize) -> Option<usize> {
    (from..=haystack.len().checked_sub(needle.len())?)
        .rev()
        .find(|start| &haystack[*start..start + needle.len()] == needle)
}

/// The part of the field that holds the dictation, with a little text around it.
fn window<'a>(field: &'a [char], inserted: &[char]) -> Option<&'a [char]> {
    let anchor = ANCHOR.min(inserted.len() / 2).max(1);
    let offsets = inserted.len().checked_sub(anchor)?;
    let (start, _) = (0..=offsets).find_map(|offset| {
        let position = last_position(field, &inserted[offset..offset + anchor], 0)?;
        Some((
            position.saturating_sub(offset + offset / 2 + CONTEXT),
            offset,
        ))
    })?;
    let end = (0..=offsets).find_map(|offset| {
        let piece = &inserted[offsets - offset..inserted.len() - offset];
        let position = last_position(field, piece, start)?;
        Some((position + anchor + offset + offset / 2 + CONTEXT).min(field.len()))
    })?;
    Some(&field[start..end])
}

/// A stretch that differs: `heard` is a range of the dictation, `corrected` of the field.
#[derive(Clone, Copy)]
struct Change {
    heard: (usize, usize),
    corrected: (usize, usize),
}

/// The stretches where `after` differs from `before`, by longest common subsequence.
fn changes(before: &[char], after: &[char]) -> Vec<Change> {
    let (n, m) = (before.len(), after.len());
    let width = m + 1;
    let mut common = vec![0u16; (n + 1) * width];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            common[i * width + j] = if before[i] == after[j] {
                common[(i + 1) * width + j + 1] + 1
            } else {
                common[(i + 1) * width + j].max(common[i * width + j + 1])
            };
        }
    }
    let mut found = Vec::new();
    let (mut i, mut j) = (0, 0);
    let mut open: Option<(usize, usize)> = None;
    while i < n || j < m {
        if i < n && j < m && before[i] == after[j] {
            if let Some((from_i, from_j)) = open.take() {
                found.push(Change {
                    heard: (from_i, i),
                    corrected: (from_j, j),
                });
            }
            i += 1;
            j += 1;
            continue;
        }
        open.get_or_insert((i, j));
        if j == m || (i < n && common[(i + 1) * width + j] >= common[i * width + j + 1]) {
            i += 1;
        } else {
            j += 1;
        }
    }
    if let Some((from_i, from_j)) = open {
        found.push(Change {
            heard: (from_i, n),
            corrected: (from_j, m),
        });
    }
    found
}

/// Grows a change inside a Latin word to the whole word, on both sides alike.
fn whole_words(change: Change, before: &[char], after: &[char]) -> Change {
    let (mut i1, mut i2) = change.heard;
    let (mut j1, mut j2) = change.corrected;
    let token_at = |text: &[char], index: usize| text.get(index).copied().is_some_and(is_token);
    while i1 > 0
        && j1 > 0
        && is_token(before[i1 - 1])
        && before[i1 - 1] == after[j1 - 1]
        && (token_at(before, i1) || token_at(after, j1))
    {
        i1 -= 1;
        j1 -= 1;
    }
    while i2 < before.len()
        && j2 < after.len()
        && is_token(before[i2])
        && before[i2] == after[j2]
        && (token_at(before, i2.wrapping_sub(1)) || token_at(after, j2.wrapping_sub(1)))
    {
        i2 += 1;
        j2 += 1;
    }
    Change {
        heard: (i1, i2),
        corrected: (j1, j2),
    }
}

fn merged(mut changes: Vec<Change>) -> Vec<Change> {
    changes.sort_by_key(|change| change.heard.0);
    let mut out: Vec<Change> = Vec::new();
    for change in changes {
        match out.last_mut() {
            Some(last)
                if change.heard.0 <= last.heard.1 && change.corrected.0 <= last.corrected.1 =>
            {
                last.heard.1 = last.heard.1.max(change.heard.1);
                last.corrected.1 = last.corrected.1.max(change.corrected.1);
            }
            _ => out.push(change),
        }
    }
    out
}

fn is_word_char(c: char) -> bool {
    is_han(c) || is_token(c) || c == ' '
}

/// Drops field text that sat next to the dictation and is not part of the corrected word.
fn without_context(corrected: &[char], heard: &[char], at_start: bool, at_end: bool) -> Vec<char> {
    let mut word = corrected;
    if at_start {
        if let Some(cut) = word.iter().rposition(|c| !is_word_char(*c)) {
            word = &word[cut + 1..];
        }
        if word.iter().all(|c| is_han(*c))
            && heard.iter().all(|c| is_han(*c))
            && word.len() > heard.len()
        {
            word = &word[word.len() - heard.len()..];
        }
    }
    if at_end {
        if let Some(cut) = word.iter().position(|c| !is_word_char(*c)) {
            word = &word[..cut];
        }
        if word.iter().all(|c| is_han(*c))
            && heard.iter().all(|c| is_han(*c))
            && word.len() > heard.len()
        {
            word = &word[..heard.len()];
        }
    }
    word.to_vec()
}

/// A reading without the differences speakers and recognizers commonly blur.
fn blurred(reading: &str) -> String {
    let reading = ["zh", "ch", "sh"]
        .iter()
        .find_map(|initial| {
            reading
                .strip_prefix(initial)
                .map(|rest| format!("{}{rest}", &initial[..1]))
        })
        .unwrap_or_else(|| reading.to_string());
    let reading = reading
        .strip_prefix('n')
        .map(|rest| format!("l{rest}"))
        .unwrap_or(reading);
    reading
        .strip_suffix("ng")
        .map(|rest| format!("{rest}n"))
        .unwrap_or(reading)
}

fn readings(c: char) -> Vec<String> {
    c.to_pinyin_multi()
        .map(|all| all.into_iter().map(|one| blurred(one.plain())).collect())
        .unwrap_or_default()
}

fn same_sound(left: char, right: char) -> bool {
    let right = readings(right);
    readings(left).iter().any(|reading| right.contains(reading))
}

/// Letters for how a text is said: readings for characters, lower case for Latin letters.
fn spoken(text: &[char]) -> String {
    text.iter()
        .map(|c| match c.to_pinyin() {
            Some(reading) => blurred(reading.plain()),
            None if c.is_ascii_alphabetic() => c.to_ascii_lowercase().to_string(),
            None => String::new(),
        })
        .collect()
}

/// The consonants of a spoken text, with similar ones made equal.
fn consonants(spoken: &str) -> Vec<char> {
    let mut key: Vec<char> = Vec::new();
    for c in spoken.chars() {
        let mapped: &[char] = match c {
            'a' | 'e' | 'i' | 'o' | 'u' | 'y' | 'h' | 'w' => &[],
            'b' | 'p' => &['p'],
            'd' | 't' => &['t'],
            'g' | 'k' | 'q' | 'c' => &['k'],
            'v' | 'f' => &['f'],
            'z' | 's' => &['s'],
            'r' | 'n' | 'l' => &['l'],
            'x' => &['k', 's'],
            'j' => &['j'],
            'm' => &['m'],
            _ => &[],
        };
        for c in mapped {
            if key.last() != Some(c) {
                key.push(*c);
            }
        }
    }
    key
}

fn distance(a: &[char], b: &[char]) -> usize {
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, x) in a.iter().enumerate() {
        let mut diagonal = row[0];
        row[0] = i + 1;
        for (j, y) in b.iter().enumerate() {
            let above = row[j + 1];
            row[j + 1] = if x == y {
                diagonal
            } else {
                1 + diagonal.min(above).min(row[j])
            };
            diagonal = above;
        }
    }
    row[b.len()]
}

fn sounds_alike(heard: &[char], corrected: &[char]) -> bool {
    let all_han = |text: &[char]| text.iter().all(|c| is_han(*c));
    if all_han(heard) && all_han(corrected) && heard.len() == corrected.len() {
        return heard
            .iter()
            .zip(corrected)
            .all(|(a, b)| a == b || same_sound(*a, *b));
    }
    let (a, b) = (consonants(&spoken(heard)), consonants(&spoken(corrected)));
    let shorter = a.len().min(b.len());
    if shorter == 0 {
        return false;
    }
    let allowed = match shorter {
        0..=3 => 0,
        4..=7 => 1,
        _ => 2,
    };
    distance(&a, &b) <= allowed
}

fn same_letters(heard: &[char], corrected: &[char]) -> bool {
    let letters = |text: &[char]| -> Vec<char> {
        text.iter()
            .filter(|c| **c != ' ')
            .map(char::to_ascii_lowercase)
            .collect()
    };
    letters(heard) == letters(corrected)
}

fn is_a_word(text: &[char]) -> bool {
    !text.is_empty()
        && text.len() <= MAX_WORD
        && text.iter().all(|c| is_word_char(*c) && !c.is_ascii_digit())
        && text.iter().any(|c| is_han(*c) || c.is_ascii_alphabetic())
}

/// Corrections the user made to `inserted`, judging by the field text read afterwards.
pub fn corrections(inserted: &str, field_after: &str) -> Vec<Correction> {
    let before = diffable(inserted);
    let field = diffable(field_after);
    if before.len() < 2 || before.len() > MAX_COMPARED {
        return Vec::new();
    }
    let Some(after) = window(&field, &before) else {
        return Vec::new();
    };
    if after.len() > MAX_COMPARED * 2 {
        return Vec::new();
    }
    let found = changes(&before, after);
    let changed: usize = found
        .iter()
        .map(|change| change.heard.1 - change.heard.0)
        .sum();
    if changed as f32 > before.len() as f32 * REWRITE_SHARE {
        return Vec::new();
    }
    let found = merged(
        found
            .into_iter()
            .map(|change| whole_words(change, &before, after))
            .collect(),
    );

    let mut out: Vec<Correction> = Vec::new();
    for change in found {
        let heard: Vec<char> = before[change.heard.0..change.heard.1].to_vec();
        let corrected = without_context(
            &after[change.corrected.0..change.corrected.1],
            &heard,
            change.heard.0 == 0,
            change.heard.1 == before.len(),
        );
        let trim = |text: Vec<char>| {
            text.iter()
                .collect::<String>()
                .trim()
                .chars()
                .collect::<Vec<char>>()
        };
        let (heard, corrected) = (trim(heard), trim(corrected));
        if !is_a_word(&heard) || !is_a_word(&corrected) || corrected.len() < 2 || heard == corrected
        {
            continue;
        }
        if !same_letters(&heard, &corrected) && !sounds_alike(&heard, &corrected) {
            continue;
        }
        let correction = Correction {
            heard: heard.into_iter().collect(),
            corrected: corrected.into_iter().collect(),
        };
        if !out.contains(&correction) {
            out.push(correction);
        }
        if out.len() == MAX_CORRECTIONS {
            break;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pairs(inserted: &str, field: &str) -> Vec<(String, String)> {
        corrections(inserted, field)
            .into_iter()
            .map(|c| (c.heard, c.corrected))
            .collect()
    }

    fn pair(heard: &str, corrected: &str) -> Vec<(String, String)> {
        vec![(heard.to_string(), corrected.to_string())]
    }

    #[test]
    fn a_word_replaced_by_one_that_sounds_the_same_is_a_correction() {
        assert_eq!(
            pairs("我在中段里输入命令", "╭──╮\n│ > 我在终端里输入命令 │\n╰──╯"),
            pair("中段", "终端")
        );
    }

    #[test]
    fn near_sounds_count_as_the_same() {
        assert_eq!(
            pairs("这边需要更多支援", "这边需要更多资源"),
            pair("支援", "资源")
        );
    }

    #[test]
    fn a_misspelled_name_is_corrected_as_a_whole_word() {
        assert_eq!(
            pairs("I use Gosdi every day", "I use Ghostty every day"),
            pair("Gosdi", "Ghostty")
        );
    }

    #[test]
    fn a_term_heard_as_similar_sounding_characters_is_a_correction() {
        assert_eq!(
            pairs("我们用酷伯内提斯部署服务", "我们用Kubernetes部署服务"),
            pair("酷伯内提斯", "Kubernetes")
        );
    }

    #[test]
    fn a_change_of_capital_letters_is_a_correction() {
        assert_eq!(
            pairs("代码放在github上面", "代码放在GitHub上面"),
            pair("github", "GitHub")
        );
    }

    #[test]
    fn a_change_of_mind_is_not_a_correction() {
        assert!(pairs("我们明天下午开会", "我们后天下午开会").is_empty());
        assert!(pairs("send it on Monday please", "send it on Friday please").is_empty());
    }

    #[test]
    fn changed_numbers_are_not_corrections() {
        assert!(pairs("会议在十三号举行", "会议在十四号举行").is_empty());
        assert!(pairs("一共 30 个人参加", "一共 38 个人参加").is_empty());
    }

    #[test]
    fn added_removed_and_repunctuated_text_is_not_a_correction() {
        assert!(pairs("明天开会", "明天上午开会").is_empty());
        assert!(pairs("那个明天开会", "明天开会").is_empty());
        assert!(pairs("明天开会，记得带报告", "明天开会。记得带报告").is_empty());
    }

    #[test]
    fn unchanged_text_has_no_corrections() {
        assert!(pairs("明天下午三点开会", "> 明天下午三点开会").is_empty());
    }

    #[test]
    fn a_rewrite_is_not_a_correction() {
        assert!(pairs(
            "明天下午三点开会记得带报告",
            "算了这件事下周再说吧你先忙别的"
        )
        .is_empty());
        assert!(pairs("明天下午三点开会记得带报告", "").is_empty());
    }

    #[test]
    fn text_around_the_dictation_is_ignored() {
        assert_eq!(
            pairs(
                "我在中段里输入命令",
                "之前写的一句话。我在终端里输入命令，后面接着打的字"
            ),
            pair("中段", "终端")
        );
    }

    #[test]
    fn several_words_can_be_corrected_at_once() {
        assert_eq!(
            pairs("我在中段里用Gosdi输入命令", "我在终端里用Ghostty输入命令"),
            vec![
                ("中段".to_string(), "终端".to_string()),
                ("Gosdi".to_string(), "Ghostty".to_string())
            ]
        );
    }
}
