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
    let mut status = update::status(
        &manifest,
        env!("CARGO_PKG_VERSION"),
        &dir,
        update::platform_key(),
    )
    .map_err(|error| error.to_string())?;
    // Where the whole app is replaced, nothing is left to download once the new one
    // stands in its place.
    let swapped = std::env::current_exe()
        .ok()
        .and_then(|exe| update::bundle_of(&exe))
        .is_some_and(|bundle| update::bundle_swapped(&bundle));
    if swapped && status.patch_bytes.is_some() {
        status.patch_bytes = Some(0);
    }
    Ok(status)
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
    #[cfg(target_os = "macos")]
    if update::platform_key() == Some(update::MACOS_KEY) {
        return download_app(&app, &client, entries).await;
    }
    let changed = update::changed_files(entries, &dir).map_err(|error| error.to_string())?;
    let total: u64 = changed.iter().map(|entry| entry.size).sum();
    let mut finished = 0;
    for entry in &changed {
        let job = InstallJob {
            model_id: entry.name.clone(),
            url: update::asset_url(&entry.asset),
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

/// Downloads the whole signed app, checks it against the release and against the
/// signature of the app in use, and puts it in that app's place.
#[cfg(target_os = "macos")]
async fn download_app(
    app: &tauri::AppHandle,
    client: &reqwest::Client,
    entries: &[update::FileEntry],
) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|error| error.to_string())?;
    let bundle = update::bundle_of(&exe).ok_or("unsupported")?;
    let entry = entries.first().ok_or("unsupported")?;
    if !update::is_plain_file_name(&entry.asset) {
        return Err("manifest".to_string());
    }
    let staging = update::staging_dir(&bundle);
    // An app run from the disk image, or from a folder this user may not write to,
    // cannot be replaced.
    std::fs::create_dir_all(&staging).map_err(|_| "unwritable".to_string())?;
    let job = InstallJob {
        model_id: entry.name.clone(),
        url: update::asset_url(&entry.asset),
        file_name: entry.asset.clone(),
        size_bytes: entry.size,
        sha256: entry.sha256.to_lowercase(),
        dir: staging.clone(),
        client: client.clone(),
        cancel: Arc::new(AtomicBool::new(false)),
        free_space,
    };
    let total = entry.size;
    let progress_app = app.clone();
    let archive = download_and_verify(&job, move |progress| {
        if progress.phase == Phase::Downloading {
            let _ = progress_app.emit(
                PROGRESS_EVENT,
                DownloadProgress {
                    downloaded: progress.downloaded,
                    total,
                },
            );
        }
    })
    .await
    .map_err(|error| error.code().to_string())?;
    let placed = tokio::task::spawn_blocking(move || -> std::io::Result<()> {
        let unpacked = update::unpack_app(&archive, &staging)?;
        update::signed_like(&unpacked, &bundle)?;
        update::swap_bundle(&unpacked, &bundle, &staging)
    })
    .await
    .map_err(|error| error.to_string())?;
    crate::applog::note(
        "update",
        &match &placed {
            Ok(()) => "the new app is in place".to_string(),
            Err(error) => format!("the new app could not be put in place: {error}"),
        },
    );
    placed.map_err(|error| error.to_string())
}

/// Restarts into the downloaded version. Called only when the user asks for it.
#[tauri::command]
pub fn update_restart(app: tauri::AppHandle) {
    app.restart()
}

/// Tokens the polish model reported using: today, the last 30 days and in all.
#[tauri::command]
pub fn token_usage_summary() -> crate::llm::usage::Summary {
    crate::llm::usage::summary()
}
