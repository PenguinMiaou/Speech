//! Commands behind the one-click model card in speech settings.

use crate::local_stt::catalog;
use crate::local_stt::install::{self, InstallError, InstallJob, Phase, Progress};
use serde::Serialize;
use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{Emitter, State};

pub const PROGRESS_EVENT: &str = "local-stt:progress";

#[derive(Default)]
pub struct InstallState {
    active: Mutex<HashMap<String, Arc<AtomicBool>>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LocalModelStatus {
    pub id: String,
    pub size_bytes: u64,
    pub recommended: bool,
    pub installed: bool,
    pub installing: bool,
    pub partial_bytes: u64,
}

fn free_space(dir: &Path) -> std::io::Result<u64> {
    fs2::available_space(dir)
}

fn emit(app: &tauri::AppHandle, model_id: &str, phase: Phase, error_code: Option<&str>) {
    emit_with_detail(app, model_id, phase, error_code, None)
}

fn emit_with_detail(
    app: &tauri::AppHandle,
    model_id: &str,
    phase: Phase,
    error_code: Option<&str>,
    error_detail: Option<String>,
) {
    let total = catalog::find(model_id).map_or(0, |entry| entry.size_bytes);
    let _ = app.emit(
        PROGRESS_EVENT,
        Progress {
            model_id: model_id.to_string(),
            phase,
            downloaded: if matches!(phase, Phase::Testing | Phase::Ready) {
                total
            } else {
                0
            },
            total,
            bytes_per_sec: 0,
            error_code: error_code.map(str::to_string),
            error_detail,
        },
    );
}

/// Loads the model and transcribes one second of silence, so "ready" means it really runs here.
async fn smoke_test(path: std::path::PathBuf) -> Result<(), crate::local_stt::LocalSttError> {
    use crate::local_stt::LocalSttError;
    let supervisor =
        crate::local_stt::global().ok_or_else(|| LocalSttError::Io("worker unavailable".into()))?;
    tokio::task::spawn_blocking(move || {
        supervisor.transcribe(&path, &[0.0f32; 16_000], Vec::new(), None)
    })
    .await
    .map_err(|e| LocalSttError::Io(e.to_string()))?
    .map(|_| ())
}

fn refuse_if_installing(state: &InstallState, model_id: &str) -> Result<(), String> {
    let active = state
        .active
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    if active.contains_key(model_id) {
        return Err(format!("{model_id} is being installed"));
    }
    Ok(())
}

#[tauri::command]
pub fn local_stt_list_models(state: State<'_, InstallState>) -> Vec<LocalModelStatus> {
    let active = state
        .active
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    catalog::entries()
        .iter()
        .map(|entry| LocalModelStatus {
            id: entry.id.to_string(),
            size_bytes: entry.size_bytes,
            recommended: entry.recommended,
            installed: catalog::installed_path(entry.id).is_some(),
            installing: active.contains_key(entry.id),
            partial_bytes: catalog::models_dir()
                .and_then(|dir| std::fs::metadata(install::part_path(&dir, entry.file_name)).ok())
                .map_or(0, |metadata| metadata.len()),
        })
        .collect()
}

/// Starts the install and returns at once. Progress arrives through `local-stt:progress`.
#[tauri::command]
pub fn local_stt_install_model(
    app: tauri::AppHandle,
    state: State<'_, InstallState>,
    model_id: String,
) -> Result<(), String> {
    let entry = catalog::find(&model_id).ok_or_else(|| format!("unknown model {model_id}"))?;
    let dir = catalog::models_dir().ok_or("models directory is not ready")?;
    let cancel = Arc::new(AtomicBool::new(false));
    {
        let mut active = state
            .active
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if active.contains_key(&model_id) {
            return Ok(());
        }
        active.insert(model_id.clone(), cancel.clone());
    }

    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(20))
        .read_timeout(Duration::from_secs(60))
        .build()
        .map_err(|e| e.to_string())?;
    let job = InstallJob {
        model_id: model_id.clone(),
        url: entry.url(),
        file_name: entry.file_name.to_string(),
        size_bytes: entry.size_bytes,
        sha256: entry.sha256.to_string(),
        dir,
        client,
        cancel,
        free_space,
    };

    tauri::async_runtime::spawn(async move {
        let outcome = match catalog::installed_path(&job.model_id) {
            Some(path) => Ok(path),
            None => {
                let progress_app = app.clone();
                install::download_and_verify(&job, move |progress| {
                    let _ = progress_app.emit(PROGRESS_EVENT, progress);
                })
                .await
            }
        };
        let mut detail = None;
        let outcome = match outcome {
            Ok(path) => {
                emit(&app, &model_id, Phase::Testing, None);
                smoke_test(path).await.map_err(|error| {
                    tracing::warn!("local model failed its smoke test: {error:?}");
                    detail = Some(error.to_string());
                    crate::local_stt::supervisor::failure_code(&error)
                })
            }
            Err(InstallError::Cancelled) => Err("cancelled"),
            Err(error) => {
                tracing::warn!("local model install failed: {error:?}");
                Err(error.code())
            }
        };
        // Clear the active entry before announcing the result, so a retry from the UI is accepted.
        if let Some(state) = tauri::Manager::try_state::<InstallState>(&app) {
            state
                .active
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .remove(&model_id);
        }
        match outcome {
            Ok(()) => emit(&app, &model_id, Phase::Ready, None),
            Err("cancelled") => emit(&app, &model_id, Phase::Cancelled, None),
            Err(code) => emit_with_detail(&app, &model_id, Phase::Failed, Some(code), detail),
        }
    });
    Ok(())
}

#[tauri::command]
pub fn local_stt_cancel_install(state: State<'_, InstallState>, model_id: String) {
    let active = state
        .active
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    if let Some(cancel) = active.get(&model_id) {
        cancel.store(true, Ordering::SeqCst);
    }
}

/// Stops the worker first, since it may have the file mapped. Runs off the main thread because
/// stopping waits for any dictation in progress.
#[tauri::command]
pub async fn local_stt_delete_model(
    state: State<'_, InstallState>,
    model_id: String,
) -> Result<(), String> {
    refuse_if_installing(&state, &model_id)?;
    let entry = catalog::find(&model_id).ok_or_else(|| format!("unknown model {model_id}"))?;
    let dir = catalog::models_dir().ok_or("models directory is not ready")?;
    tauri::async_runtime::spawn_blocking(move || {
        if let Some(supervisor) = crate::local_stt::global() {
            supervisor.stop();
        }
        for path in [
            dir.join(entry.file_name),
            install::part_path(&dir, entry.file_name),
        ] {
            match std::fs::remove_file(&path) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.to_string()),
            }
        }
        Ok(())
    })
    .await
    .map_err(|error| error.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delete_is_refused_while_that_model_is_installing() {
        let state = InstallState::default();
        state
            .active
            .lock()
            .unwrap()
            .insert("busy".to_string(), Arc::new(AtomicBool::new(false)));
        assert!(refuse_if_installing(&state, "busy").is_err());
        assert!(refuse_if_installing(&state, "idle").is_ok());
    }
}

static TRANSCRIBING_FILE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

async fn file_options(
    dictionary: &crate::storage::DictionaryStore,
    speaker_label: Option<String>,
    model_id: Option<String>,
    languages: Vec<String>,
) -> crate::file_transcribe::Options {
    crate::file_transcribe::Options {
        speaker_label: speaker_label.filter(|label| !label.trim().is_empty()),
        model_id,
        vocabulary: crate::recent::engine_vocabulary(&dictionary.words().await),
        languages: crate::file_transcribe::chosen_languages(languages),
    }
}

fn report_stage(app: &tauri::AppHandle, stage: crate::file_transcribe::Stage) {
    use crate::file_transcribe::Stage;
    let payload = match stage {
        Stage::Written(text) => {
            let _ = app.emit("file-transcribe:written", text);
            return;
        }
        Stage::Speakers => serde_json::json!({ "stage": "speakers", "done": 0, "total": 0 }),
        Stage::Text { done, total } => {
            serde_json::json!({ "stage": "text", "done": done, "total": total })
        }
    };
    let _ = app.emit("file-transcribe:progress", payload);
}

/// Turns an audio or video file into text with the local model, one file at a time.
/// With `speaker_label` the text is written out by speaker. Progress goes out as
/// `file-transcribe:progress`.
#[tauri::command]
pub async fn transcribe_file(
    app: tauri::AppHandle,
    path: String,
    speaker_label: Option<String>,
    model_id: Option<String>,
    languages: Option<Vec<String>>,
    dictionary: tauri::State<'_, crate::storage::DictionaryStore>,
) -> Result<String, String> {
    let languages = languages.unwrap_or_default();
    let options = file_options(&dictionary, speaker_label, model_id, languages).await;
    if TRANSCRIBING_FILE.swap(true, Ordering::SeqCst) {
        return Err("busy".to_string());
    }
    let by_speaker = options.speaker_label.is_some();
    let names_to = app.clone();
    let result = tokio::task::spawn_blocking(move || {
        crate::file_transcribe::transcribe(std::path::Path::new(&path), &options, |stage| {
            report_stage(&app, stage)
        })
    })
    .await
    .map_err(|error| error.to_string())
    .and_then(|result| result);
    TRANSCRIBING_FILE.store(false, Ordering::SeqCst);
    if by_speaker && result.is_ok() {
        let _ = names_to.emit("meeting:names", crate::voiceprints::names());
    }
    result
}

/// Starts recording a meeting from the microphone. Each paragraph arrives as a
/// `meeting:segment` event while the recording runs, an error as `meeting:error`.
#[tauri::command]
pub async fn meeting_record_start(
    app: tauri::AppHandle,
    input_device: String,
    by_speaker: bool,
    system_sound: bool,
    model_id: Option<String>,
    languages: Option<Vec<String>>,
    dictionary: tauri::State<'_, crate::storage::DictionaryStore>,
) -> Result<(), String> {
    let vocabulary = crate::recent::engine_vocabulary(&dictionary.words().await);
    if TRANSCRIBING_FILE.load(Ordering::SeqCst) {
        return Err("busy".to_string());
    }
    crate::meeting_record::start(
        input_device,
        by_speaker,
        system_sound,
        model_id,
        vocabulary,
        languages.unwrap_or_default(),
        move |outcome| {
            let _ = match outcome {
                Ok(crate::meeting_record::Heard::Said(segment)) => {
                    let _ = app.emit("meeting:names", crate::voiceprints::names());
                    app.emit("meeting:segment", segment)
                }
                Ok(crate::meeting_record::Heard::Guess(text)) => app.emit("meeting:interim", text),
                Err(error) => app.emit("meeting:error", error),
            };
        },
    )
    .await
}

/// Pauses the meeting recording or goes on with it. What is said during a pause is not
/// recorded.
#[tauri::command]
pub fn meeting_record_pause(paused: bool) -> bool {
    crate::meeting_record::pause(paused)
}

/// Changes the languages the running meeting is held in: three at most, none to let the
/// model tell.
#[tauri::command]
pub fn meeting_languages(languages: Vec<String>) {
    crate::meeting_record::set_languages(languages);
}

#[derive(Serialize)]
pub struct MeetingLevel {
    pub seconds: u64,
    pub volume: f32,
    /// Seconds recorded and not yet recognized.
    pub waiting: u64,
}

/// The running meeting recording's length and microphone level, if one runs.
#[tauri::command]
pub fn meeting_record_level() -> Option<MeetingLevel> {
    crate::meeting_record::level().map(|(seconds, volume, waiting)| MeetingLevel {
        seconds,
        volume,
        waiting,
    })
}

/// Ends the meeting recording once the rest of it has been recognized. With `discard`
/// the audio still waiting is dropped.
/// The latest meeting's audio stays on disk; its path comes back unless it was discarded.
#[tauri::command]
pub async fn meeting_record_stop(discard: bool) -> Result<Option<String>, String> {
    crate::meeting_record::stop(discard).await?;
    Ok(crate::meeting_record::kept_recording().map(|path| path.to_string_lossy().into_owned()))
}

/// Remembers the voice of a speaker of the meeting in hand under a name, so that the
/// same person gets the name in later meetings. Returns the remembered names.
#[tauri::command]
pub fn speaker_remember(speaker: u32, name: String) -> Result<Vec<String>, String> {
    crate::voiceprints::remember(speaker, &name)
}

/// Learns from the names the meeting in hand ended with: each named speaker's voice
/// becomes one more sample of that person.
#[tauri::command]
pub fn speaker_confirm(
    names: std::collections::HashMap<u32, String>,
) -> Result<Vec<String>, String> {
    crate::voiceprints::confirm(&names)
}

#[tauri::command]
pub fn speaker_voices() -> Vec<String> {
    crate::voiceprints::remembered()
}

#[tauri::command]
pub fn speaker_forget(name: String) -> Result<Vec<String>, String> {
    crate::voiceprints::forget(&name)
}

/// Copies the latest meeting's audio to where the user asked.
#[tauri::command]
pub fn meeting_recording_save(path: String) -> Result<(), String> {
    let kept = crate::meeting_record::kept_recording().ok_or("no_recording")?;
    std::fs::copy(kept, path)
        .map(|_| ())
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn meeting_recording_delete() {
    if !crate::meeting_record::is_recording() {
        crate::meeting_record::delete_recording();
    }
}

#[derive(Serialize)]
pub struct SpeakerModelsStatus {
    pub installed: bool,
    pub downloading: bool,
    pub total_bytes: u64,
}

#[tauri::command]
pub fn speaker_models_status() -> SpeakerModelsStatus {
    SpeakerModelsStatus {
        installed: crate::speakers::installed_models().is_some(),
        downloading: SPEAKER_DOWNLOAD.load(Ordering::SeqCst),
        total_bytes: crate::speakers::MODEL_FILES
            .iter()
            .map(|file| file.size_bytes)
            .sum(),
    }
}

static SPEAKER_DOWNLOAD: AtomicBool = AtomicBool::new(false);

/// Downloads and verifies the two speaker models, reporting progress over both as
/// `speakers:download-progress`.
#[tauri::command]
pub async fn speaker_models_install(app: tauri::AppHandle) -> Result<(), String> {
    let dir = crate::speakers::model_dir().ok_or("models directory is not ready")?;
    if SPEAKER_DOWNLOAD.swap(true, Ordering::SeqCst) {
        return Err("busy".to_string());
    }
    let result = install_speaker_models(&app, &dir).await;
    SPEAKER_DOWNLOAD.store(false, Ordering::SeqCst);
    result
}

async fn install_speaker_models(app: &tauri::AppHandle, dir: &Path) -> Result<(), String> {
    let files = &crate::speakers::MODEL_FILES;
    let total: u64 = files.iter().map(|file| file.size_bytes).sum();
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(20))
        .read_timeout(Duration::from_secs(60))
        .build()
        .map_err(|error| error.to_string())?;
    let mut before = 0;
    for file in files {
        if !crate::voice_isolation::is_downloaded(dir, file) {
            let job = InstallJob {
                model_id: file.file_name.to_string(),
                url: file.url.to_string(),
                file_name: file.file_name.to_string(),
                size_bytes: file.size_bytes,
                sha256: file.sha256.to_string(),
                dir: dir.to_path_buf(),
                client: client.clone(),
                cancel: Arc::new(AtomicBool::new(false)),
                free_space,
            };
            let progress_app = app.clone();
            install::download_and_verify(&job, move |progress| {
                if progress.phase == Phase::Downloading {
                    let _ = progress_app.emit(
                        "speakers:download-progress",
                        serde_json::json!({ "downloaded": before + progress.downloaded, "total": total }),
                    );
                }
            })
            .await
            .map_err(|error| error.code().to_string())?;
        }
        before += file.size_bytes;
    }
    Ok(())
}

/// Writes text to the place the user just chose in the save dialog.
#[tauri::command]
pub fn save_text_file(path: String, text: String) -> Result<(), String> {
    let bytes = if path.to_lowercase().ends_with(".docx") {
        crate::docx::docx(&text)
    } else {
        text.into_bytes()
    };
    std::fs::write(path, bytes).map_err(|error| error.to_string())
}
