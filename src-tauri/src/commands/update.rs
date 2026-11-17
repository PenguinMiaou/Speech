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

/// How long the list of the latest release may take from one source. The list is small;
/// a server that cannot send it in this time is not reachable.
const LIST_TIMEOUT: Duration = Duration::from_secs(10);

async fn fetch(client: &reqwest::Client, url: &str) -> Option<Vec<u8>> {
    let response = client
        .get(url)
        .header(reqwest::header::USER_AGENT, crate::mirrors::AGENT)
        .timeout(LIST_TIMEOUT)
        .send()
        .await
        .ok()?
        .error_for_status()
        .ok()?;
    response.bytes().await.ok().map(|bytes| bytes.to_vec())
}

/// The newest release manifest carrying the release key's signature. Every source is
/// asked at once: a copy may still hold the list of an earlier release, which passes its
/// signature check, so the newest one any source gives is the latest.
async fn latest_manifest(client: &reqwest::Client) -> Result<Manifest, String> {
    let key = {
        use base64::Engine;
        base64::engine::general_purpose::STANDARD
            .decode(update::PUBLIC_KEY)
            .map_err(|_| "signature".to_string())?
    };
    let manifests = crate::mirrors::candidates(&update::manifest_url());
    let signatures = crate::mirrors::candidates(&update::signature_url());
    let answers = futures_util::future::join_all(manifests.iter().zip(&signatures).map(
        |(manifest_url, signature_url)| async move {
            let (body, signature) =
                tokio::join!(fetch(client, manifest_url), fetch(client, signature_url));
            (manifest_url, body.zip(signature))
        },
    ))
    .await;
    newest(answers, &key)
}

/// A source's address, and the manifest and signature it gave, when it gave both.
type Answer<'a> = (&'a String, Option<(Vec<u8>, Vec<u8>)>);

/// The newest of the manifests that came with a valid signature, or why there is none.
fn newest(answers: Vec<Answer>, key: &[u8]) -> Result<Manifest, String> {
    let mut failure = "network".to_string();
    let mut best: Option<Manifest> = None;
    for (url, answer) in answers {
        let Some((body, signature)) = answer else {
            continue;
        };
        match update::read_signed(&body, &String::from_utf8_lossy(&signature), key) {
            Ok(manifest) => {
                if best
                    .as_ref()
                    .is_none_or(|best| update::is_newer(&manifest.version, &best.version))
                {
                    best = Some(manifest);
                }
            }
            Err(error) => {
                crate::applog::note(
                    "update",
                    &format!(
                        "the manifest from {} was refused: {error}",
                        crate::mirrors::host_of(url)
                    ),
                );
                failure = error;
            }
        }
    }
    best.ok_or(failure)
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

#[cfg(test)]
mod tests {
    use super::*;
    use ring::signature::{Ed25519KeyPair, KeyPair};

    fn signer() -> Ed25519KeyPair {
        let rng = ring::rand::SystemRandom::new();
        let pkcs8 = Ed25519KeyPair::generate_pkcs8(&rng).unwrap();
        Ed25519KeyPair::from_pkcs8(pkcs8.as_ref()).unwrap()
    }

    fn answer(key: &Ed25519KeyPair, version: &str) -> Option<(Vec<u8>, Vec<u8>)> {
        use base64::Engine;
        let body = format!(r#"{{"version":"{version}","files":{{}}}}"#).into_bytes();
        let signature = base64::engine::general_purpose::STANDARD.encode(key.sign(&body).as_ref());
        Some((body, signature.into_bytes()))
    }

    #[test]
    fn the_newest_signed_manifest_wins_over_an_older_copy() {
        let key = signer();
        let urls = ["a".to_string(), "b".to_string(), "c".to_string()];
        let answers = vec![
            (&urls[0], answer(&key, "1.2.10")),
            (&urls[1], None),
            (&urls[2], answer(&key, "1.2.11")),
        ];
        let manifest = newest(answers, key.public_key().as_ref()).unwrap();
        assert_eq!(manifest.version, "1.2.11");
    }

    #[test]
    fn a_newer_manifest_with_another_signature_is_ignored() {
        let key = signer();
        let other = signer();
        let urls = ["a".to_string(), "b".to_string()];
        let answers = vec![
            (&urls[0], answer(&key, "1.2.10")),
            (&urls[1], answer(&other, "9.9.9")),
        ];
        let manifest = newest(answers, key.public_key().as_ref()).unwrap();
        assert_eq!(manifest.version, "1.2.10");
    }

    #[test]
    fn no_answer_is_a_network_failure() {
        let key = signer();
        let url = "a".to_string();
        assert_eq!(
            newest(vec![(&url, None)], key.public_key().as_ref()).unwrap_err(),
            "network"
        );
    }
}
