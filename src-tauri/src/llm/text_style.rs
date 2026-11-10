//! Writing conventions for dictated text, enforced whatever the speech model or
//! the polish model produced.

use super::PolishRequest;

/// A user-chosen scene or custom prompt can ask for its own layout, so it is left alone.
pub fn applies_to(request: &PolishRequest) -> bool {
    super::case_guard::applies_to(request)
        && request.active_scene_prompt.trim().is_empty()
        && request.polish_custom_prompt.trim().is_empty()
}

/// Applies the conventions to text arriving in pieces. The pieces joined together
/// always equal [`tidy`] of the whole text.
#[derive(Default)]
pub struct StyleFilter {
    last: Option<char>,
    /// Whitespace seen since `last`. What happens to it depends on the next character.
    pending: String,
}

impl StyleFilter {
    pub fn push(&mut self, chunk: &str) -> String {
        let mut output = String::with_capacity(chunk.len());
        for character in chunk.chars() {
            if character.is_whitespace() {
                self.pending.push(character);
                continue;
            }
            self.resolve_pending(character, &mut output);
            output.push(character);
            self.last = Some(character);
        }
        output
    }

    fn resolve_pending(&mut self, next: char, output: &mut String) {
        let pending = std::mem::take(&mut self.pending);
        let Some(last) = self.last else {
            return;
        };
        match pending.rfind(['\n', '\r']) {
            Some(line_end) => {
                output.push('\n');
                output.push_str(&pending[line_end + 1..]);
            }
            None if closes_up(last, next) => {}
            None => output.push_str(&pending),
        }
    }
}

pub fn tidy(text: &str) -> String {
    StyleFilter::default().push(text)
}

enum Kind {
    Han,
    WidePunctuation,
    Latin,
    Other,
}

fn kind(character: char) -> Kind {
    match character {
        '\u{3040}'..='\u{30ff}'
        | '\u{3400}'..='\u{4dbf}'
        | '\u{4e00}'..='\u{9fff}'
        | '\u{f900}'..='\u{faff}'
        | '\u{20000}'..='\u{2ffff}' => Kind::Han,
        '\u{3000}'..='\u{303f}' | '\u{ff00}'..='\u{ffef}' => Kind::WidePunctuation,
        '%' | '+' | '#' | '\u{c0}'..='\u{24f}' => Kind::Latin,
        character if character.is_ascii_alphanumeric() => Kind::Latin,
        _ => Kind::Other,
    }
}

/// Spaces between two Chinese characters stay, since they can separate names in a list.
fn closes_up(before: char, after: char) -> bool {
    !matches!(
        (kind(before), kind(after)),
        (Kind::Other, _) | (_, Kind::Other) | (Kind::Latin, Kind::Latin) | (Kind::Han, Kind::Han)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removes_the_space_between_chinese_and_latin_words() {
        assert_eq!(tidy("我用 Python 写脚本"), "我用Python写脚本");
        assert_eq!(tidy("这是 Claude Code 的输出"), "这是Claude Code的输出");
    }

    #[test]
    fn removes_the_space_between_chinese_and_digits() {
        assert_eq!(tidy("下午 3 点开 2 个会"), "下午3点开2个会");
        assert_eq!(tidy("涨了 50% 的 C++ 代码"), "涨了50%的C++代码");
    }

    #[test]
    fn removes_the_space_next_to_chinese_punctuation() {
        assert_eq!(tidy("你好， world。 然后"), "你好，world。然后");
        assert_eq!(tidy("《 指南 》"), "《指南》");
    }

    #[test]
    fn keeps_spaces_that_separate_things() {
        for text in [
            "Send it to Mary, then call Ben.",
            "张三 李四 王五",
            "1. 报告",
            "- 张总主持",
            "Python 코드",
        ] {
            assert_eq!(tidy(text), text);
        }
    }

    #[test]
    fn blank_lines_become_one_line_break() {
        assert_eq!(tidy("第一段\n\n第二段"), "第一段\n第二段");
        assert_eq!(tidy("第一段\n \n\n第二段"), "第一段\n第二段");
        assert_eq!(tidy("第一段\r\n\r\n第二段"), "第一段\n第二段");
        assert_eq!(tidy("第一段  \n第二段"), "第一段\n第二段");
    }

    #[test]
    fn list_layout_survives() {
        let list = "下周的安排：\n1. 时间地点\n  - 张总主持\n2. 待确认";
        assert_eq!(tidy(list), list);
    }

    #[test]
    fn whitespace_around_the_text_is_dropped() {
        assert_eq!(tidy("\n\n 中文 \n"), "中文");
    }

    #[test]
    fn pieces_give_the_same_text_as_the_whole() {
        let text = "\n开头 Python 和 3 个，  world\n\n 第二段 A B\r\n\r\n  - 项目 \n";
        let whole = tidy(text);
        let chars: Vec<char> = text.chars().collect();
        for size in 1..=7 {
            let mut filter = StyleFilter::default();
            let pieces: String = chars
                .chunks(size)
                .map(|piece| filter.push(&piece.iter().collect::<String>()))
                .collect();
            assert_eq!(pieces, whole, "piece size {size}");
        }
    }

    #[test]
    fn only_plain_dictation_is_changed() {
        let dictation = crate::llm::case_guard::tests::dictation();
        assert!(applies_to(&dictation));

        let mut scene = dictation.clone();
        scene.active_scene_prompt = "Use two short paragraphs.".to_string();
        assert!(!applies_to(&scene));

        let mut custom = dictation.clone();
        custom.polish_custom_prompt = "Separate paragraphs with a blank line.".to_string();
        assert!(!applies_to(&custom));

        let mut selected = dictation;
        selected.selected_text = Some("selected".to_string());
        assert!(!applies_to(&selected));
    }
}
