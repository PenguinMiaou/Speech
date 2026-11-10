//! Places an app that is not in the built-in list, from what the app says about itself:
//! a word in its name or identifier, or the category it declares to the system.
//! Nothing is looked up online.

use super::types::ContextFamily;

const WORDS: &[(&str, ContextFamily)] = &[
    ("mail", ContextFamily::Email),
    ("email", ContextFamily::Email),
    ("terminal", ContextFamily::PromptOrCode),
    ("console", ContextFamily::PromptOrCode),
    ("shell", ContextFamily::PromptOrCode),
    ("ide", ContextFamily::PromptOrCode),
    ("code", ContextFamily::PromptOrCode),
    ("coder", ContextFamily::PromptOrCode),
    ("vim", ContextFamily::PromptOrCode),
    ("emacs", ContextFamily::PromptOrCode),
    ("chat", ContextFamily::PersonalChat),
    ("messenger", ContextFamily::PersonalChat),
    ("messages", ContextFamily::PersonalChat),
    ("notes", ContextFamily::Document),
    ("note", ContextFamily::Document),
    ("docs", ContextFamily::Document),
    ("writer", ContextFamily::Document),
    ("markdown", ContextFamily::Document),
];

/// macOS app categories that say how text is written there. Most categories do not.
const CATEGORIES: &[(&str, ContextFamily)] = &[
    (
        "public.app-category.developer-tools",
        ContextFamily::PromptOrCode,
    ),
    (
        "public.app-category.social-networking",
        ContextFamily::PersonalChat,
    ),
];

/// A whole word in the name or identifier decides first, then the declared category.
pub(crate) fn infer_family(
    name: Option<&str>,
    identity: Option<&str>,
    category: Option<&str>,
) -> Option<ContextFamily> {
    let by_word = [name, identity].into_iter().flatten().find_map(|text| {
        text.split(|c: char| !c.is_alphanumeric())
            .map(str::to_lowercase)
            .find_map(|word| {
                WORDS
                    .iter()
                    .find(|(known, _)| *known == word)
                    .map(|(_, family)| *family)
            })
    });
    by_word.or_else(|| {
        let category = category?.trim();
        CATEGORIES
            .iter()
            .find(|(known, _)| known.eq_ignore_ascii_case(category))
            .map(|(_, family)| *family)
    })
}
