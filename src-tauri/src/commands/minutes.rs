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
