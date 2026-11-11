use crate::credentials::{resolve_llm_config_secret, SystemCredentialVault};
use crate::minutes::Template;
use crate::storage;

/// Writes the minutes of a meeting transcript after a template, with the language model
/// chosen in the settings.
#[tauri::command]
pub async fn meeting_minutes(
    transcript: String,
    template: String,
    notes: Option<String>,
    config_state: tauri::State<'_, storage::ConfigManager>,
    client: tauri::State<'_, reqwest::Client>,
) -> Result<String, String> {
    let config = config_state.load().await.map_err(|e| e.to_string())?;
    let api_key =
        resolve_llm_config_secret(&config, &SystemCredentialVault).map_err(|e| e.to_string())?;
    let configured = !config.llm_base_url.trim().is_empty()
        && !config.llm_model.trim().is_empty()
        && crate::llm::has_usable_provider_credentials(&config.llm_provider, &api_key);
    if !configured {
        return Err("llm_not_configured".to_string());
    }
    let notes = notes.unwrap_or_default();
    crate::minutes::minutes(&client, &config, &api_key, &transcript, &template, &notes).await
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
    let config = config_state.load().await.map_err(|e| e.to_string())?;
    let api_key =
        resolve_llm_config_secret(&config, &SystemCredentialVault).map_err(|e| e.to_string())?;
    let configured = !config.llm_base_url.trim().is_empty()
        && !config.llm_model.trim().is_empty()
        && crate::llm::has_usable_provider_credentials(&config.llm_provider, &api_key);
    if !configured {
        return Err("llm_not_configured".to_string());
    }
    crate::minutes::ask(&client, &config, &api_key, &transcript, &question).await
}

/// Names a meeting after what was said in it, with the language model chosen in the
/// settings. Empty when no model is set up, so that a meeting simply stays unnamed.
#[tauri::command]
pub async fn meeting_title(
    transcript: String,
    config_state: tauri::State<'_, storage::ConfigManager>,
    client: tauri::State<'_, reqwest::Client>,
) -> Result<String, String> {
    let config = config_state.load().await.map_err(|e| e.to_string())?;
    let api_key =
        resolve_llm_config_secret(&config, &SystemCredentialVault).map_err(|e| e.to_string())?;
    let configured = !config.llm_base_url.trim().is_empty()
        && !config.llm_model.trim().is_empty()
        && crate::llm::has_usable_provider_credentials(&config.llm_provider, &api_key);
    if !configured {
        return Ok(String::new());
    }
    crate::minutes::title(&client, &config, &api_key, &transcript).await
}

/// Puts the words of a whole meeting right with the language model chosen in the settings,
/// which is told the spellings in the user's dictionary. `pieces` is what each person said.
#[tauri::command]
pub async fn meeting_polish(
    pieces: Vec<String>,
    config_state: tauri::State<'_, storage::ConfigManager>,
    dictionary: tauri::State<'_, storage::DictionaryStore>,
    client: tauri::State<'_, reqwest::Client>,
) -> Result<Vec<String>, String> {
    let config = config_state.load().await.map_err(|e| e.to_string())?;
    let api_key =
        resolve_llm_config_secret(&config, &SystemCredentialVault).map_err(|e| e.to_string())?;
    let configured = !config.llm_base_url.trim().is_empty()
        && !config.llm_model.trim().is_empty()
        && crate::llm::has_usable_provider_credentials(&config.llm_provider, &api_key);
    if !configured {
        return Err("llm_not_configured".to_string());
    }
    let mut words = dictionary.words().await;
    words.reverse();
    words.truncate(80);
    crate::minutes::polish(&client, &config, &api_key, pieces, &words).await
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
    text: String,
    target: String,
    config_state: tauri::State<'_, storage::ConfigManager>,
    client: tauri::State<'_, reqwest::Client>,
) -> Result<String, String> {
    let config = config_state.load().await.map_err(|e| e.to_string())?;
    let api_key =
        resolve_llm_config_secret(&config, &SystemCredentialVault).map_err(|e| e.to_string())?;
    let configured = !config.llm_base_url.trim().is_empty()
        && !config.llm_model.trim().is_empty()
        && crate::llm::has_usable_provider_credentials(&config.llm_provider, &api_key);
    if !configured {
        return Err("llm_not_configured".to_string());
    }
    crate::minutes::translate(&client, &config, &api_key, &text, &target).await
}
