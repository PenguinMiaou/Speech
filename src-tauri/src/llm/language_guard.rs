//! Tells whether a translation came out in the language asked for, by the letters it is
//! written in. A model sometimes gives the cleaned text back in the speaker's language
//! instead of translating it.

/// Whether `text` is written in the language `target` names. A language this cannot tell
/// by its letters, or a text without letters, passes.
pub fn reads_as(text: &str, target: &str) -> bool {
    let (mut han, mut kana, mut hangul, mut cyrillic, mut other) = (0, 0, 0, 0, 0);
    for character in text.chars().filter(|c| c.is_alphabetic()) {
        match character as u32 {
            0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xF900..=0xFAFF => han += 1,
            0x3040..=0x30FF => kana += 1,
            0xAC00..=0xD7AF | 0x1100..=0x11FF => hangul += 1,
            0x0400..=0x04FF => cyrillic += 1,
            _ => other += 1,
        }
    }
    let letters = han + kana + hangul + cyrillic + other;
    if letters == 0 {
        return true;
    }
    let foreign = han + kana + hangul + cyrillic;
    match target.trim() {
        "zh" => han > 0 && kana == 0 && hangul == 0,
        "ja" => kana > 0,
        "ko" => hangul > 0,
        "ru" | "uk" => cyrillic > 0,
        "en" | "fr" | "de" | "es" | "pt" | "it" | "nl" | "tr" | "pl" | "id" | "ms" | "vi" => {
            foreign * 5 < letters
        }
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::reads_as;

    #[test]
    fn a_translation_left_in_the_speakers_language_is_told() {
        assert!(reads_as("Let's go hiking this weekend.", "en"));
        assert!(!reads_as("周末天气好的话我们去爬山吧", "en"));
        // A name or a term kept as it was is still a translation.
        assert!(reads_as(
            "Please send the 报价单 to Ann before Friday.",
            "en"
        ));
        assert!(reads_as("明日の会議は午後4時です", "ja"));
        assert!(!reads_as("明天的会议改到下午四点", "ja"));
        assert!(reads_as("明天的会议改到下午四点，记得带 laptop", "zh"));
        assert!(!reads_as("The meeting is moved to 4 PM.", "zh"));
        assert!(reads_as("회의는 오후 4시입니다", "ko"));
        assert!(!reads_as("Встреча в четыре", "en"));
        assert!(reads_as("Встреча в четыре", "ru"));
        assert!(reads_as("", "en"));
        assert!(reads_as("สวัสดี", "th"));
    }
}
