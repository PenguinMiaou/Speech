use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Duration;

use serde::Serialize;
use tauri::Emitter;

use crate::local_stt::install::{download_and_verify, InstallJob, Phase};
use crate::update::{self, Manifest, Status};

pub const PROGRESS_EVENT: &str = "update:progress";

/// Bytes downloaded so far out of all the files this update needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct DownloadProgress {
    pub downloaded: u64,
    pub total: u64,
}

fn free_space(dir: &Path) -> std::io::Result<u64> {
    fs2::available_space(dir)
}

fn program_dir() -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|error| error.to_string())?;
    exe.parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| "the program has no folder".to_string())
}

fn client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(15))
        .read_timeout(Duration::from_secs(60))
        .build()
        .map_err(|error| error.to_string())
}

async fn latest_manifest(client: &reqwest::Client) -> Result<Manifest, String> {
    client
        .get(update::manifest_url())
        .send()
        .await
        .and_then(|response| response.error_for_status())
        .map_err(|_| "network".to_string())?
        .json()
        .await
        .map_err(|_| "manifest".to_string())
}

/// Asks the project's releases whether a newer version exists.
#[tauri::command]
pub async fn update_check() -> Result<Status, String> {
    let manifest = latest_manifest(&client()?).await?;
    let dir = program_dir()?;
    update::status(
        &manifest,
        env!("CARGO_PKG_VERSION"),
        &dir,
        update::platform_key(),
    )
    .map_err(|error| error.to_string())
}

/// Downloads the program files that changed, checks each against the release and puts them in
/// place, reporting progress as [`PROGRESS_EVENT`]. Nothing is replaced unless every file
/// arrived intact. The running app is untouched until [`update_restart`].
#[tauri::command]
pub async fn update_download(app: tauri::AppHandle) -> Result<(), String> {
    let client = client()?;
    let manifest = latest_manifest(&client).await?;
    let dir = program_dir()?;
    let entries = update::platform_key()
        .and_then(|key| manifest.files.get(key))
        .ok_or("unsupported")?;
    if !update::is_newer(&manifest.version, env!("CARGO_PKG_VERSION")) {
        return Err("current".to_string());
    }
    let changed = update::changed_files(entries, &dir).map_err(|error| error.to_string())?;
    let total: u64 = changed.iter().map(|entry| entry.size).sum();
    let mut finished = 0;
    for entry in &changed {
        let job = InstallJob {
            model_id: entry.name.clone(),
            url: update::asset_url(&manifest.version, &entry.asset),
            file_name: update::staged_name(&entry.name),
            size_bytes: entry.size,
            sha256: entry.sha256.to_lowercase(),
            dir: dir.clone(),
            client: client.clone(),
            cancel: Arc::new(AtomicBool::new(false)),
            free_space,
        };
        let progress_app = app.clone();
        download_and_verify(&job, move |progress| {
            if progress.phase == Phase::Downloading {
                let _ = progress_app.emit(
                    PROGRESS_EVENT,
                    DownloadProgress {
                        downloaded: finished + progress.downloaded,
                        total,
                    },
                );
            }
        })
        .await
        .map_err(|error| error.code().to_string())?;
        finished += entry.size;
    }
    let names: Vec<String> = changed.into_iter().map(|entry| entry.name).collect();
    update::swap_in(&dir, &names).map_err(|error| error.to_string())
}

/// Restarts into the downloaded version. Called only when the user asks for it.
#[tauri::command]
pub fn update_restart(app: tauri::AppHandle) {
    app.restart()
}
