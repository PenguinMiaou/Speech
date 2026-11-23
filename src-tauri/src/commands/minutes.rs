use crate::credentials::{resolve_meeting_llm, SystemCredentialVault};
use crate::minutes::Template;
use crate::storage;

/// Writes the minutes of a meeting transcript after a template, with the language model
/// chosen in the settings.
/// The settings and key meetings use: the meeting service where one is set, else the main one.
async fn meeting_llm(
    config_state: &storage::ConfigManager,
) -> Result<(storage::AppConfig, String), String> {
    let config = config_state.load().await.map_err(|e| e.to_string())?;
    resolve_meeting_llm(config, &SystemCredentialVault).map_err(|e| e.to_string())
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn meeting_minutes(
    transcript: String,
    template: String,
    notes: Option<String>,
    full: Option<bool>,
    language: Option<String>,
    config_state: tauri::State<'_, storage::ConfigManager>,
    dictionary: tauri::State<'_, storage::DictionaryStore>,
    client: tauri::State<'_, reqwest::Client>,
) -> Result<String, String> {
    let (config, api_key) = meeting_llm(&config_state).await?;
    let configured = !config.llm_base_url.trim().is_empty()
        && !config.llm_model.trim().is_empty()
        && crate::llm::has_usable_provider_credentials(&config.llm_provider, &api_key);
    if !configured {
        return Err("llm_not_configured".to_string());
    }
    let notes = notes.unwrap_or_default();
    // The spellings the user taught the dictionary, newest first.
    let mut words = dictionary.words().await;
    words.reverse();
    words.truncate(80);
    crate::minutes::minutes(
        &client,
        &config,
        &api_key,
        &transcript,
        &template,
        &notes,
        &words,
        crate::minutes::Form {
            full: full.unwrap_or(false),
            language: language.unwrap_or_default(),
            ..Default::default()
        },
    )
    .await
}

/// The rules learned for writing minutes after the template `text` names, or that the
/// minutes in `text` follow.
#[tauri::command]
pub fn minutes_style(text: String) -> Vec<String> {
    crate::minutes_style::rules(&text)
}

/// Keeps the rules for the template `text` names as the user left them.
#[tauri::command]
pub fn minutes_style_save(text: String, rules: Vec<String>) -> Result<Vec<String>, String> {
    crate::minutes_style::save(&text, &rules)
}

/// The questions the user keeps to ask of any meeting.
#[tauri::command]
pub fn meeting_questions() -> Vec<String> {
    crate::meeting_questions::list()
}

/// Keeps the questions to ask of any meeting as the user left them.
#[tauri::command]
pub fn meeting_questions_save(questions: Vec<String>) -> Result<Vec<String>, String> {
    crate::meeting_questions::save(&questions)
}

/// Learns from how the user changed minutes how they want them written. Returns the rules
/// now kept for their template.
#[tauri::command]
pub async fn minutes_learn(
    before: String,
    after: String,
    config_state: tauri::State<'_, storage::ConfigManager>,
    client: tauri::State<'_, reqwest::Client>,
) -> Result<Vec<String>, String> {
    let (config, api_key) = meeting_llm(&config_state).await?;
    let configured = !config.llm_base_url.trim().is_empty()
        && !config.llm_model.trim().is_empty()
        && crate::llm::has_usable_provider_credentials(&config.llm_provider, &api_key);
    if !configured {
        return Err("llm_not_configured".to_string());
    }
    crate::minutes_style::learn(&client, &config, &api_key, &before, &after).await
}

#[tauri::command]
pub fn minutes_templates() -> Vec<Template> {
    crate::minutes::templates()
}

/// Adds a text file as a minutes template and returns all templates.
#[tauri::command]
pub fn minutes_template_import(path: String) -> Result<Vec<Template>, String> {
    crate::minutes::import(std::path::Path::new(&path))
}

/// Keeps a template written in the app and returns all templates.
#[tauri::command]
pub fn minutes_template_save(name: String, text: String) -> Result<Vec<Template>, String> {
    crate::minutes::save(&name, &text)
}

#[tauri::command]
pub fn minutes_template_remove(name: String) -> Result<Vec<Template>, String> {
    crate::minutes::remove(&name)
}

/// Answers a question about a meeting transcript with the language model chosen in the
/// settings.
#[tauri::command]
pub async fn meeting_ask(
    transcript: String,
    question: String,
    config_state: tauri::State<'_, storage::ConfigManager>,
    client: tauri::State<'_, reqwest::Client>,
) -> Result<String, String> {
    let (config, api_key) = meeting_llm(&config_state).await?;
    let configured = !config.llm_base_url.trim().is_empty()
        && !config.llm_model.trim().is_empty()
        && crate::llm::has_usable_provider_credentials(&config.llm_provider, &api_key);
    if !configured {
        return Err("llm_not_configured".to_string());
    }
    crate::minutes::ask(&client, &config, &api_key, &transcript, &question).await
}

/// How many of the newest meetings a question asked of all of them reads.
const MOST_MEETINGS_ASKED: usize = 20;

/// Answers a question from the newest meetings together: from the minutes of those that
/// have them, and the beginning of what was said in the others.
#[tauri::command]
pub async fn meetings_ask(
    question: String,
    config_state: tauri::State<'_, storage::ConfigManager>,
    client: tauri::State<'_, reqwest::Client>,
) -> Result<String, String> {
    let (config, api_key) = meeting_llm(&config_state).await?;
    let configured = !config.llm_base_url.trim().is_empty()
        && !config.llm_model.trim().is_empty()
        && crate::llm::has_usable_provider_credentials(&config.llm_provider, &api_key);
    if !configured {
        return Err("llm_not_configured".to_string());
    }
    let read = tokio::task::spawn_blocking(|| crate::meetings::reading(MOST_MEETINGS_ASKED))
        .await
        .map_err(|error| error.to_string())?;
    crate::minutes::ask(&client, &config, &api_key, &read, &question).await
}

/// Names a meeting after what was said in it, with the language model chosen in the
/// settings. Empty when no model is set up, so that a meeting simply stays unnamed.
#[tauri::command]
pub async fn meeting_title(
    transcript: String,
    config_state: tauri::State<'_, storage::ConfigManager>,
    client: tauri::State<'_, reqwest::Client>,
) -> Result<String, String> {
    let (config, api_key) = meeting_llm(&config_state).await?;
    let configured = !config.llm_base_url.trim().is_empty()
        && !config.llm_model.trim().is_empty()
        && crate::llm::has_usable_provider_credentials(&config.llm_provider, &api_key);
    if !configured {
        return Ok(String::new());
    }
    crate::minutes::title(&client, &config, &api_key, &transcript).await
}

/// Puts the words of a whole meeting right with the language model chosen in the settings,
/// which is told the spellings in the user's dictionary. `pieces` is what each person said,
/// `calendar` the day of the meeting and the days around it.
#[tauri::command]
pub async fn meeting_polish(
    app: tauri::AppHandle,
    pieces: Vec<String>,
    calendar: Option<String>,
    config_state: tauri::State<'_, storage::ConfigManager>,
    dictionary: tauri::State<'_, storage::DictionaryStore>,
    client: tauri::State<'_, reqwest::Client>,
) -> Result<Vec<String>, String> {
    let (config, api_key) = meeting_llm(&config_state).await?;
    let configured = !config.llm_base_url.trim().is_empty()
        && !config.llm_model.trim().is_empty()
        && crate::llm::has_usable_provider_credentials(&config.llm_provider, &api_key);
    if !configured {
        return Err("llm_not_configured".to_string());
    }
    let mut words = dictionary.words().await;
    words.reverse();
    use tauri::Emitter;
    let (pieces, terms) = crate::minutes::polish(
        &client,
        &config,
        &api_key,
        pieces,
        &words,
        calendar.as_deref().unwrap_or(""),
        |done, total| {
            let _ = app.emit(
                "meeting:polishing",
                serde_json::json!({ "step": "words", "done": done, "total": total }),
            );
        },
    )
    .await?;
    // The spellings the model could not be sure of, for the user to settle once.
    let _ = app.emit("meeting:terms", terms);
    Ok(pieces)
}

/// Checks who said what in a meeting by the sense of it, with the language model chosen
/// in the settings. As many pieces come back, in the same order, with every word kept.
#[tauri::command]
pub async fn meeting_speakers_check(
    app: tauri::AppHandle,
    pieces: Vec<crate::minutes::Said>,
    config_state: tauri::State<'_, storage::ConfigManager>,
    client: tauri::State<'_, reqwest::Client>,
) -> Result<Vec<crate::minutes::Said>, String> {
    use tauri::Emitter;
    let (config, api_key) = meeting_llm(&config_state).await?;
    let configured = !config.llm_base_url.trim().is_empty()
        && !config.llm_model.trim().is_empty()
        && crate::llm::has_usable_provider_credentials(&config.llm_provider, &api_key);
    if !configured {
        return Err("llm_not_configured".to_string());
    }
    crate::minutes::check_speakers(&client, &config, &api_key, pieces, |done, total| {
        let _ = app.emit(
            "meeting:polishing",
            serde_json::json!({ "step": "speakers", "done": done, "total": total }),
        );
    })
    .await
}

/// The names the transcript itself gives its speakers, each with the words that give it,
/// for the user to take or leave. Nothing is named here.
#[tauri::command]
pub async fn meeting_speaker_names(
    pieces: Vec<crate::minutes::Said>,
    config_state: tauri::State<'_, storage::ConfigManager>,
    client: tauri::State<'_, reqwest::Client>,
) -> Result<Vec<crate::minutes::NameFound>, String> {
    let (config, api_key) = meeting_llm(&config_state).await?;
    let configured = !config.llm_base_url.trim().is_empty()
        && !config.llm_model.trim().is_empty()
        && crate::llm::has_usable_provider_credentials(&config.llm_provider, &api_key);
    if !configured {
        return Err("llm_not_configured".to_string());
    }
    crate::minutes::find_names(&client, &config, &api_key, &pieces).await
}

/// Learns from a correction the user made to the meeting's words: a word put right goes
/// into the dictionary, and the recognizer of a running recording listens for it from
/// then on. Returns the words learned.
#[tauri::command]
pub async fn meeting_learn(
    app: tauri::AppHandle,
    before: String,
    after: String,
    dictionary: tauri::State<'_, storage::DictionaryStore>,
) -> Result<Vec<String>, String> {
    use tauri::Emitter;
    let mut words = Vec::new();
    let mut fresh = Vec::new();
    for correction in crate::readback::corrections::corrections(&before, &after) {
        if let Ok(true) = dictionary
            .learn(&correction.heard, &correction.corrected)
            .await
        {
            fresh.push(correction.corrected.clone());
        }
        words.push(correction.corrected);
    }
    crate::meeting_record::add_words(&words);
    if !fresh.is_empty() {
        let _ = app.emit("dictionary:learned", fresh);
    }
    Ok(words)
}

/// Translates one piece of a meeting with the language model chosen in the settings.
#[tauri::command]
pub async fn meeting_translate(
    app: tauri::AppHandle,
    text: String,
    target: String,
    config_state: tauri::State<'_, storage::ConfigManager>,
    client: tauri::State<'_, reqwest::Client>,
) -> Result<String, String> {
    let (config, api_key) = meeting_llm(&config_state).await?;
    let configured = !config.llm_base_url.trim().is_empty()
        && !config.llm_model.trim().is_empty()
        && crate::llm::has_usable_provider_credentials(&config.llm_provider, &api_key);
    if !configured {
        return Err("llm_not_configured".to_string());
    }
    use tauri::Emitter;
    // The translation is shown as it is written: each event carries all of it so far.
    let so_far = std::sync::Mutex::new(String::new());
    let piece = move |piece: &str| {
        let mut so_far = so_far
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        so_far.push_str(piece);
        let _ = app.emit("meeting:translating", so_far.as_str());
    };
    crate::minutes::translate(&client, &config, &api_key, &text, &target, &piece).await
}

/// The chapters of a meeting and what each of its speakers said, with the language model
/// chosen for meetings. `transcript` keeps the speakers' numbers in its headings, which
/// `label` begins; `names` are the names the user gave them; `language` is the code of the
/// language to write in, or empty for the meeting's own.
#[tauri::command]
pub async fn meeting_overview(
    transcript: String,
    label: String,
    names: Option<std::collections::HashMap<u32, String>>,
    language: Option<String>,
    config_state: tauri::State<'_, storage::ConfigManager>,
    client: tauri::State<'_, reqwest::Client>,
) -> Result<crate::overview::Overview, String> {
    let (config, api_key) = meeting_llm(&config_state).await?;
    let configured = !config.llm_base_url.trim().is_empty()
        && !config.llm_model.trim().is_empty()
        && crate::llm::has_usable_provider_credentials(&config.llm_provider, &api_key);
    if !configured {
        return Err("llm_not_configured".to_string());
    }
    crate::overview::overview(
        &client,
        &config,
        &api_key,
        &transcript,
        &label,
        &names.unwrap_or_default(),
        &language.unwrap_or_default(),
    )
    .await
}
