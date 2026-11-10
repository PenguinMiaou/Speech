//! Settings for keeping only the user's own voice in local recognition.

use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Duration;

use crate::local_stt::install::{download_and_verify, InstallJob, Phase};
use crate::voice_isolation::{self, MODEL_FILES};
use tauri::Emitter;

pub const PROGRESS_EVENT: &str = "voice-isolation:progress";

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceIsolationStatus {
    model_installed: bool,
    /// A model download is running, perhaps started before the settings page was reopened.
    downloading: bool,
    has_sample: bool,
    /// The next recording will be kept as the voice sample.
    enrollment_pending: bool,
    /// Why the last recording could not be kept: `too_short`, `too_quiet` or `not_saved`.
    enrollment_error: Option<&'static str>,
    model_bytes: u64,
}

fn free_space(dir: &Path) -> std::io::Result<u64> {
    fs2::available_space(dir)
}

#[tauri::command]
pub fn voice_isolation_status() -> VoiceIsolationStatus {
    VoiceIsolationStatus {
        model_installed: voice_isolation::installed_models().is_some(),
        downloading: voice_isolation::download_running(),
        has_sample: voice_isolation::has_voice_sample(),
        enrollment_pending: voice_isolation::enrollment_requested(),
        enrollment_error: voice_isolation::last_enrollment()
            .and_then(|outcome| outcome.err())
            .map(|error| error.code()),
        model_bytes: MODEL_FILES.iter().map(|file| file.size_bytes).sum(),
    }
}

/// Downloads and verifies the two model files, reporting progress over both as
/// [`PROGRESS_EVENT`]. Returns once both are in place. A call made while a download runs
/// waits for it, then finds the files in place.
#[tauri::command]
pub async fn voice_isolation_install(app: tauri::AppHandle) -> Result<(), String> {
    let dir = voice_isolation::model_dir().ok_or("models directory is not ready")?;
    let _turn = voice_isolation::download_turn().await;
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(20))
        .read_timeout(Duration::from_secs(60))
        .build()
        .map_err(|error| error.to_string())?;
    for (index, file) in MODEL_FILES.iter().enumerate() {
        if voice_isolation::is_downloaded(&dir, file) {
            continue;
        }
        let job = InstallJob {
            model_id: file.file_name.to_string(),
            url: file.url.to_string(),
            file_name: file.file_name.to_string(),
            size_bytes: file.size_bytes,
            sha256: file.sha256.to_string(),
            dir: dir.clone(),
            client: client.clone(),
            cancel: Arc::new(AtomicBool::new(false)),
            free_space,
        };
        let progress_app = app.clone();
        download_and_verify(&job, move |progress| {
            if progress.phase == Phase::Downloading {
                let overall = voice_isolation::overall_download(
                    index,
                    progress.downloaded,
                    progress.bytes_per_sec,
                );
                let _ = progress_app.emit(PROGRESS_EVENT, overall);
            }
        })
        .await
        .map_err(|error| error.code().to_string())?;
    }
    Ok(())
}

/// Asks that the next recording become the user's voice sample, or withdraws that.
#[tauri::command]
pub fn voice_isolation_enroll_next(wanted: bool) {
    if wanted {
        voice_isolation::request_enrollment();
    } else {
        voice_isolation::cancel_enrollment();
    }
}

#[tauri::command]
pub fn voice_isolation_delete_sample() -> Result<(), String> {
    match voice_isolation::voice_sample_path() {
        Some(path) if path.is_file() => std::fs::remove_file(path).map_err(|e| e.to_string()),
        _ => Ok(()),
    }
}
