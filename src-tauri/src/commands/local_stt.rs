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
        },
    );
}

/// Loads the model and transcribes one second of silence, so "ready" means it really runs here.
async fn smoke_test(path: std::path::PathBuf) -> Result<(), InstallError> {
    let supervisor =
        crate::local_stt::global().ok_or_else(|| InstallError::Io("worker unavailable".into()))?;
    tokio::task::spawn_blocking(move || {
        supervisor.transcribe(&path, &[0.0f32; 16_000], Vec::new(), None)
    })
    .await
    .map_err(|e| InstallError::Io(e.to_string()))?
    .map(|_| ())
    .map_err(|e| InstallError::Io(e.to_string()))
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
        let outcome = match outcome {
            Ok(path) => {
                emit(&app, &model_id, Phase::Testing, None);
                smoke_test(path).await.map_err(|error| {
                    tracing::warn!("local model failed its smoke test: {error:?}");
                    "engine"
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
            Err(code) => emit(&app, &model_id, Phase::Failed, Some(code)),
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
