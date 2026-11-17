//! Downloads a model with resume support and verifies its checksum before it can be used.

use futures_util::StreamExt;
use reqwest::header::RANGE;
use reqwest::StatusCode;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

const PROGRESS_INTERVAL: Duration = Duration::from_millis(250);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Phase {
    Downloading,
    Verifying,
    Testing,
    Ready,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Progress {
    pub model_id: String,
    pub phase: Phase,
    pub downloaded: u64,
    pub total: u64,
    pub bytes_per_sec: u64,
    pub error_code: Option<String>,
    /// What exactly went wrong, as the program reported it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_detail: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstallError {
    DiskFull,
    Network(String),
    Checksum,
    Cancelled,
    Io(String),
}

impl InstallError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::DiskFull => "disk_full",
            Self::Network(_) => "network",
            Self::Checksum => "checksum",
            Self::Cancelled => "cancelled",
            Self::Io(_) => "io",
        }
    }
}

impl From<io::Error> for InstallError {
    fn from(error: io::Error) -> Self {
        Self::Io(error.to_string())
    }
}

pub struct InstallJob {
    pub model_id: String,
    pub url: String,
    pub file_name: String,
    pub size_bytes: u64,
    pub sha256: String,
    pub dir: PathBuf,
    pub client: reqwest::Client,
    pub cancel: Arc<AtomicBool>,
    pub free_space: fn(&Path) -> io::Result<u64>,
}

pub fn part_path(dir: &Path, file_name: &str) -> PathBuf {
    dir.join(format!("{file_name}.part"))
}

pub(crate) fn sha256_of(path: &Path) -> io::Result<String> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 1024 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

/// Space needed for what is still missing, plus 10% headroom.
fn required_space(size_bytes: u64, have: u64) -> u64 {
    let missing = size_bytes.saturating_sub(have);
    missing + missing / 10
}

/// A source is given up for the next one when, this long into a download, it has sent
/// less than [`SLOWEST_BYTES_PER_SEC`] on average.
#[cfg(not(test))]
const SLOW_AFTER: Duration = Duration::from_secs(20);
#[cfg(test)]
const SLOW_AFTER: Duration = Duration::from_millis(300);
#[cfg(not(test))]
const SLOWEST_BYTES_PER_SEC: u64 = 64 * 1024;
#[cfg(test)]
const SLOWEST_BYTES_PER_SEC: u64 = 40;

async fn download<F: FnMut(Progress)>(
    job: &InstallJob,
    url: &str,
    give_up_when_slow: bool,
    part: &Path,
    have: u64,
    on_progress: &mut F,
) -> Result<(), InstallError> {
    let mut request = job
        .client
        .get(url)
        .header(reqwest::header::USER_AGENT, crate::mirrors::AGENT);
    if have > 0 {
        request = request.header(RANGE, format!("bytes={have}-"));
    }
    let response = request
        .send()
        .await
        .map_err(|e| InstallError::Network(e.to_string()))?;
    let mut written = match response.status() {
        StatusCode::PARTIAL_CONTENT => have,
        StatusCode::OK => 0,
        status => return Err(InstallError::Network(format!("HTTP {status}"))),
    };

    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false)
        .open(part)?;
    file.set_len(written)?;
    file.seek(SeekFrom::Start(written))?;

    let mut stream = response.bytes_stream();
    let (began, began_at) = (Instant::now(), written);
    let mut reported_at = Instant::now();
    let mut reported_bytes = written;
    while let Some(chunk) = stream.next().await {
        if job.cancel.load(Ordering::SeqCst) {
            file.flush()?;
            return Err(InstallError::Cancelled);
        }
        let chunk = chunk.map_err(|e| InstallError::Network(e.to_string()))?;
        file.write_all(&chunk)?;
        written += chunk.len() as u64;
        let spent = began.elapsed();
        if give_up_when_slow
            && spent >= SLOW_AFTER
            && (written - began_at) * 1000 < SLOWEST_BYTES_PER_SEC * spent.as_millis() as u64
        {
            file.flush()?;
            return Err(InstallError::Network("too slow".to_string()));
        }
        let elapsed = reported_at.elapsed();
        if elapsed >= PROGRESS_INTERVAL {
            on_progress(Progress {
                model_id: job.model_id.clone(),
                phase: Phase::Downloading,
                downloaded: written,
                total: job.size_bytes,
                bytes_per_sec: ((written - reported_bytes) as f64 / elapsed.as_secs_f64()) as u64,
                error_code: None,
                error_detail: None,
            });
            reported_at = Instant::now();
            reported_bytes = written;
        }
    }
    file.flush()?;
    if written < job.size_bytes {
        return Err(InstallError::Network(format!(
            "connection ended at {written} bytes"
        )));
    }
    Ok(())
}

/// Files at least this large come from the source that is quickest when timed.
const TIME_SOURCES_FROM: u64 = 8 * 1024 * 1024;

/// Returns the path of the verified model. A partial download stays on disk for the next attempt,
/// except after a checksum failure, when it cannot be trusted.
pub async fn download_and_verify<F: FnMut(Progress)>(
    job: &InstallJob,
    on_progress: F,
) -> Result<PathBuf, InstallError> {
    let mut sources = crate::mirrors::candidates(&job.url);
    // For a large file the sources are timed first, so it comes from the quickest one
    // where the user is; a small one is fetched before timing would finish.
    if job.size_bytes >= TIME_SOURCES_FROM && sources.len() > 1 {
        sources = crate::mirrors::fastest_first(&job.client, sources).await;
    }
    download_and_verify_from(job, &sources, on_progress).await
}

/// The same from the first of `sources` that delivers the file intact. A source that
/// fails, sends another file or only trickles gives way to the next; what arrived of the
/// file is kept for it unless it was found to be another file.
pub async fn download_and_verify_from<F: FnMut(Progress)>(
    job: &InstallJob,
    sources: &[String],
    mut on_progress: F,
) -> Result<PathBuf, InstallError> {
    let mut failure = InstallError::Network("no source".to_string());
    for (index, url) in sources.iter().enumerate() {
        let last = index + 1 == sources.len();
        match download_and_verify_once(job, url, !last, &mut on_progress).await {
            Ok(path) => {
                crate::mirrors::worked(url);
                if index > 0 {
                    crate::applog::note(
                        "download",
                        &format!(
                            "{} came from {}",
                            job.file_name,
                            crate::mirrors::host_of(url)
                        ),
                    );
                }
                return Ok(path);
            }
            Err(
                error @ (InstallError::DiskFull | InstallError::Cancelled | InstallError::Io(_)),
            ) => return Err(error),
            Err(error) => {
                crate::applog::note(
                    "download",
                    &format!(
                        "{} could not come from {}: {}",
                        job.file_name,
                        crate::mirrors::host_of(url),
                        error.code()
                    ),
                );
                failure = error;
            }
        }
    }
    Err(failure)
}

async fn download_and_verify_once<F: FnMut(Progress)>(
    job: &InstallJob,
    url: &str,
    give_up_when_slow: bool,
    on_progress: &mut F,
) -> Result<PathBuf, InstallError> {
    std::fs::create_dir_all(&job.dir)?;
    let part = part_path(&job.dir, &job.file_name);
    let mut have = std::fs::metadata(&part).map(|m| m.len()).unwrap_or(0);
    if have > job.size_bytes {
        std::fs::remove_file(&part)?;
        have = 0;
    }
    if (job.free_space)(&job.dir)? < required_space(job.size_bytes, have) {
        return Err(InstallError::DiskFull);
    }
    if job.cancel.load(Ordering::SeqCst) {
        return Err(InstallError::Cancelled);
    }

    on_progress(Progress {
        model_id: job.model_id.clone(),
        phase: Phase::Downloading,
        downloaded: have,
        total: job.size_bytes,
        bytes_per_sec: 0,
        error_code: None,
        error_detail: None,
    });
    if have < job.size_bytes {
        download(job, url, give_up_when_slow, &part, have, on_progress).await?;
    }

    on_progress(Progress {
        model_id: job.model_id.clone(),
        phase: Phase::Verifying,
        downloaded: job.size_bytes,
        total: job.size_bytes,
        bytes_per_sec: 0,
        error_code: None,
        error_detail: None,
    });
    let to_hash = part.clone();
    let digest = tokio::task::spawn_blocking(move || sha256_of(&to_hash))
        .await
        .map_err(|e| InstallError::Io(e.to_string()))??;
    if digest != job.sha256 {
        std::fs::remove_file(&part)?;
        return Err(InstallError::Checksum);
    }
    let target = job.dir.join(&job.file_name);
    std::fs::rename(&part, &target)?;
    Ok(target)
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{header, method};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    const BODY: &[u8] = b"pretend this is a model file";

    fn plenty(_: &Path) -> io::Result<u64> {
        Ok(u64::MAX)
    }

    fn none(_: &Path) -> io::Result<u64> {
        Ok(0)
    }

    fn temp_dir() -> PathBuf {
        use std::sync::atomic::AtomicU32;
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let dir = std::env::temp_dir().join(format!(
            "ot-install-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::SeqCst)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn digest(bytes: &[u8]) -> String {
        Sha256::digest(bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }

    fn job(server: &MockServer, dir: &Path) -> InstallJob {
        InstallJob {
            model_id: "m".into(),
            url: format!("{}/model.gguf", server.uri()),
            file_name: "model.gguf".into(),
            size_bytes: BODY.len() as u64,
            sha256: digest(BODY),
            dir: dir.to_path_buf(),
            client: reqwest::Client::new(),
            cancel: Arc::new(AtomicBool::new(false)),
            free_space: plenty,
        }
    }

    async fn serve(status: u16, body: &[u8]) -> MockServer {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(status).set_body_bytes(body.to_vec()))
            .mount(&server)
            .await;
        server
    }

    /// Installs the catalog model `INSTALL_MODEL_ID` into `INSTALL_DIR` from the real server,
    /// the way the settings page does.
    #[tokio::test]
    #[ignore = "downloads a model of several hundred megabytes"]
    async fn installs_a_catalog_model_from_the_real_server() {
        let id = std::env::var("INSTALL_MODEL_ID").expect("INSTALL_MODEL_ID");
        let dir = PathBuf::from(std::env::var("INSTALL_DIR").expect("INSTALL_DIR"));
        let entry = crate::local_stt::catalog::find(&id).expect("a catalog model");
        let job = InstallJob {
            model_id: id.clone(),
            url: entry.url(),
            file_name: entry.file_name.to_string(),
            size_bytes: entry.size_bytes,
            sha256: entry.sha256.to_string(),
            dir,
            client: reqwest::Client::new(),
            cancel: Arc::new(AtomicBool::new(false)),
            free_space: plenty,
        };
        let started = std::time::Instant::now();
        let mut phases = Vec::new();
        let path = download_and_verify(&job, |progress| {
            if phases.last() != Some(&progress.phase) {
                phases.push(progress.phase);
            }
        })
        .await
        .expect("an installed model");
        println!(
            "installed {} bytes in {}s, phases {phases:?}",
            std::fs::metadata(&path).unwrap().len(),
            started.elapsed().as_secs()
        );
        assert_eq!(std::fs::metadata(&path).unwrap().len(), entry.size_bytes);
    }

    /// Downloads the models that tell speakers apart into `INSTALL_DIR` from the real
    /// servers, the way the meeting card does.
    #[tokio::test]
    #[ignore = "downloads about 140 megabytes"]
    async fn installs_the_speaker_models_from_the_real_servers() {
        let dir = PathBuf::from(std::env::var("INSTALL_DIR").expect("INSTALL_DIR"));
        for file in &crate::speakers::MODEL_FILES {
            let job = InstallJob {
                model_id: file.file_name.to_string(),
                url: file.url.to_string(),
                file_name: file.file_name.to_string(),
                size_bytes: file.size_bytes,
                sha256: file.sha256.to_string(),
                dir: dir.clone(),
                client: reqwest::Client::new(),
                cancel: Arc::new(AtomicBool::new(false)),
                free_space: plenty,
            };
            let started = std::time::Instant::now();
            let mut steps = 0;
            let path = download_and_verify(&job, |_| steps += 1)
                .await
                .expect("an installed model");
            println!(
                "{} is {} bytes after {}s and {steps} progress reports",
                file.file_name,
                std::fs::metadata(&path).unwrap().len(),
                started.elapsed().as_secs()
            );
            assert_eq!(std::fs::metadata(&path).unwrap().len(), file.size_bytes);
        }
    }

    #[tokio::test]
    async fn downloads_and_verifies() {
        let (server, dir) = (serve(200, BODY).await, temp_dir());
        let mut phases = Vec::new();
        let path = download_and_verify(&job(&server, &dir), |p| phases.push(p.phase))
            .await
            .unwrap();
        assert_eq!(std::fs::read(path).unwrap(), BODY);
        assert!(!part_path(&dir, "model.gguf").exists());
        assert_eq!(phases.first(), Some(&Phase::Downloading));
        assert_eq!(phases.last(), Some(&Phase::Verifying));
    }

    #[tokio::test]
    async fn resumes_with_range_header() {
        let (server, dir) = (MockServer::start().await, temp_dir());
        std::fs::write(part_path(&dir, "model.gguf"), &BODY[..7]).unwrap();
        Mock::given(method("GET"))
            .and(header("range", "bytes=7-"))
            .respond_with(ResponseTemplate::new(206).set_body_bytes(BODY[7..].to_vec()))
            .expect(1)
            .mount(&server)
            .await;
        let path = download_and_verify(&job(&server, &dir), |_| {})
            .await
            .unwrap();
        assert_eq!(std::fs::read(path).unwrap(), BODY);
    }

    #[tokio::test]
    async fn server_ignoring_range_restarts() {
        let (server, dir) = (serve(200, BODY).await, temp_dir());
        std::fs::write(part_path(&dir, "model.gguf"), b"stale bytes").unwrap();
        let path = download_and_verify(&job(&server, &dir), |_| {})
            .await
            .unwrap();
        assert_eq!(std::fs::read(path).unwrap(), BODY);
    }

    #[tokio::test]
    async fn complete_part_skips_download() {
        let (server, dir) = (MockServer::start().await, temp_dir());
        std::fs::write(part_path(&dir, "model.gguf"), BODY).unwrap();
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(500))
            .expect(0)
            .mount(&server)
            .await;
        assert!(download_and_verify(&job(&server, &dir), |_| {})
            .await
            .is_ok());
    }

    #[tokio::test]
    async fn checksum_mismatch_deletes_part() {
        let (server, dir) = (serve(200, BODY).await, temp_dir());
        let mut bad = job(&server, &dir);
        bad.sha256 = digest(b"something else");
        assert_eq!(
            download_and_verify(&bad, |_| {}).await,
            Err(InstallError::Checksum)
        );
        assert!(!part_path(&dir, "model.gguf").exists());
        assert!(!dir.join("model.gguf").exists());
    }

    #[tokio::test]
    async fn network_drop_keeps_part() {
        let (server, dir) = (serve(200, &BODY[..5]).await, temp_dir());
        let error = download_and_verify(&job(&server, &dir), |_| {})
            .await
            .unwrap_err();
        assert_eq!(error.code(), "network");
        assert_eq!(
            std::fs::read(part_path(&dir, "model.gguf")).unwrap(),
            &BODY[..5]
        );
        assert!(!dir.join("model.gguf").exists());
    }

    #[tokio::test]
    async fn server_error_keeps_part() {
        let (server, dir) = (serve(503, b"").await, temp_dir());
        std::fs::write(part_path(&dir, "model.gguf"), &BODY[..5]).unwrap();
        let error = download_and_verify(&job(&server, &dir), |_| {})
            .await
            .unwrap_err();
        assert_eq!(error.code(), "network");
        assert_eq!(
            std::fs::read(part_path(&dir, "model.gguf")).unwrap(),
            &BODY[..5]
        );
    }

    #[tokio::test]
    async fn cancel_keeps_part_and_reports_cancelled() {
        let (server, dir) = (serve(200, BODY).await, temp_dir());
        std::fs::write(part_path(&dir, "model.gguf"), &BODY[..5]).unwrap();
        let cancelled = job(&server, &dir);
        cancelled.cancel.store(true, Ordering::SeqCst);
        assert_eq!(
            download_and_verify(&cancelled, |_| {}).await,
            Err(InstallError::Cancelled)
        );
        assert!(part_path(&dir, "model.gguf").exists());
    }

    #[tokio::test]
    async fn insufficient_disk_fails_before_download() {
        let (server, dir) = (MockServer::start().await, temp_dir());
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200))
            .expect(0)
            .mount(&server)
            .await;
        let mut full = job(&server, &dir);
        full.free_space = none;
        assert_eq!(
            download_and_verify(&full, |_| {}).await,
            Err(InstallError::DiskFull)
        );
    }

    #[test]
    fn required_space_counts_only_what_is_missing() {
        assert_eq!(required_space(1000, 0), 1100);
        assert_eq!(required_space(1000, 900), 110);
        assert_eq!(required_space(1000, 1000), 0);
    }
    #[tokio::test]
    async fn a_source_that_fails_gives_way_to_the_next() {
        let broken = serve(500, b"").await;
        let good = serve(200, BODY).await;
        let dir = temp_dir();
        let job = job(&broken, &dir);
        let sources = [job.url.clone(), format!("{}/model.gguf", good.uri())];
        let path = download_and_verify_from(&job, &sources, |_| {})
            .await
            .unwrap();
        assert_eq!(std::fs::read(path).unwrap(), BODY);
    }

    #[tokio::test]
    async fn a_source_that_sends_another_file_gives_way_to_the_next() {
        let tampered = serve(200, b"pretend this is a model fil!").await;
        let good = serve(200, BODY).await;
        let dir = temp_dir();
        let job = job(&tampered, &dir);
        let sources = [job.url.clone(), format!("{}/model.gguf", good.uri())];
        let path = download_and_verify_from(&job, &sources, |_| {})
            .await
            .unwrap();
        assert_eq!(std::fs::read(path).unwrap(), BODY);
    }

    #[tokio::test]
    async fn when_every_source_fails_the_last_failure_is_told() {
        let broken = serve(500, b"").await;
        let dir = temp_dir();
        let job = job(&broken, &dir);
        let sources = [job.url.clone(), job.url.clone()];
        let error = download_and_verify_from(&job, &sources, |_| {})
            .await
            .unwrap_err();
        assert_eq!(error.code(), "network");
    }

    /// A server that sends the headers of `body` and then one byte every 50 ms.
    async fn trickle(body: &'static [u8]) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move {
            while let Ok((mut socket, _)) = listener.accept().await {
                tokio::spawn(async move {
                    use tokio::io::{AsyncReadExt, AsyncWriteExt};
                    let mut request = [0u8; 1024];
                    let _ = socket.read(&mut request).await;
                    let head = format!("HTTP/1.1 200 OK\r\ncontent-length: {}\r\n\r\n", body.len());
                    let _ = socket.write_all(head.as_bytes()).await;
                    for byte in body {
                        if socket.write_all(&[*byte]).await.is_err() {
                            return;
                        }
                        tokio::time::sleep(Duration::from_millis(50)).await;
                    }
                });
            }
        });
        format!("http://{address}/model.gguf")
    }

    #[tokio::test]
    async fn a_source_that_only_trickles_gives_way_to_the_next() {
        let slow = trickle(BODY).await;
        let good = serve(200, BODY).await;
        let dir = temp_dir();
        let job = job(&good, &dir);
        let sources = [slow, job.url.clone()];
        let started = Instant::now();
        let path = download_and_verify_from(&job, &sources, |_| {})
            .await
            .unwrap();
        assert_eq!(std::fs::read(path).unwrap(), BODY);
        assert!(started.elapsed() < Duration::from_millis(BODY.len() as u64 * 50));
    }

    #[tokio::test]
    async fn the_last_source_is_waited_for_however_slow() {
        let slow = trickle(BODY).await;
        let dir = temp_dir();
        let mut job = job(&serve(500, b"").await, &dir);
        job.url = slow.clone();
        let path = download_and_verify_from(&job, &[slow], |_| {})
            .await
            .unwrap();
        assert_eq!(std::fs::read(path).unwrap(), BODY);
    }
    /// With the file's own server out of reach, the speaker models arrive through the
    /// Huggingface mirror and a GitHub proxy and pass their checksums.
    #[tokio::test]
    #[ignore]
    async fn the_speaker_models_arrive_from_the_mirrors() {
        let dir = temp_dir();
        for file in &crate::speakers::MODEL_FILES[..2] {
            let job = InstallJob {
                model_id: file.file_name.into(),
                url: file.url.into(),
                file_name: file.file_name.into(),
                size_bytes: file.size_bytes,
                sha256: file.sha256.into(),
                dir: dir.clone(),
                client: reqwest::Client::new(),
                cancel: Arc::new(AtomicBool::new(false)),
                free_space: plenty,
            };
            // The file's own address stands in for an unreachable one; the rest are tried
            // as the app tries them.
            let sources: Vec<String> = crate::mirrors::candidates(file.url)
                .into_iter()
                .map(|url| {
                    if url == file.url {
                        "http://127.0.0.1:9/unreachable".to_string()
                    } else {
                        url
                    }
                })
                .collect();
            let path = download_and_verify_from(&job, &sources, |_| {})
                .await
                .unwrap();
            assert!(path.exists());
        }
    }

    /// Times every real source of a file large enough to be timed and downloads it the way
    /// the app does. Run with `--ignored --nocapture`; needs the network.
    #[tokio::test]
    #[ignore]
    async fn a_large_file_comes_from_the_quickest_real_source() {
        let file = &crate::speakers::MODEL_FILES[1];
        assert!(file.size_bytes >= TIME_SOURCES_FROM);
        let client = reqwest::Client::new();
        let order =
            crate::mirrors::fastest_first(&client, crate::mirrors::candidates(file.url)).await;
        let hosts: Vec<&str> = order
            .iter()
            .map(|url| crate::mirrors::host_of(url))
            .collect();
        println!("quickest first: {hosts:?}");
        let dir = temp_dir();
        let job = InstallJob {
            model_id: file.file_name.into(),
            url: file.url.into(),
            file_name: file.file_name.into(),
            size_bytes: file.size_bytes,
            sha256: file.sha256.into(),
            dir: dir.clone(),
            client,
            cancel: Arc::new(AtomicBool::new(false)),
            free_space: plenty,
        };
        let started = Instant::now();
        let path = download_and_verify(&job, |_| {}).await.unwrap();
        println!(
            "{} MB in {:?}",
            file.size_bytes / 1_000_000,
            started.elapsed()
        );
        assert!(path.exists());
    }

    /// Resumes a part-downloaded file from the project's ModelScope copy, as a download cut
    /// off in mainland China carries on. Run with `--ignored`; needs the network.
    #[tokio::test]
    #[ignore]
    async fn a_download_from_the_modelscope_copy_carries_on_where_it_stopped() {
        let file = &crate::word_times::MODEL_FILES[1];
        let copy = crate::mirrors::candidates(file.url)[0].clone();
        assert!(copy.starts_with(crate::mirrors::MODELSCOPE.unwrap()));
        let dir = temp_dir();
        let job = InstallJob {
            model_id: file.file_name.into(),
            url: file.url.into(),
            file_name: file.file_name.into(),
            size_bytes: file.size_bytes,
            sha256: file.sha256.into(),
            dir: dir.clone(),
            client: reqwest::Client::new(),
            cancel: Arc::new(AtomicBool::new(false)),
            free_space: plenty,
        };
        let part = part_path(&dir, file.file_name);
        const HAVE: usize = 100_000;

        // A part that is not the file's beginning stays as it is: the copy sends only the
        // rest, so the whole fails its check.
        std::fs::write(&part, vec![0u8; HAVE]).unwrap();
        let wrong = download_and_verify_from(&job, std::slice::from_ref(&copy), |_| {}).await;
        assert!(matches!(wrong, Err(InstallError::Checksum)), "{wrong:?}");

        // The real beginning, and the copy sends the rest.
        let start = reqwest::Client::new()
            .get(&copy)
            .header(reqwest::header::USER_AGENT, crate::mirrors::AGENT)
            .header(RANGE, format!("bytes=0-{}", HAVE - 1))
            .send()
            .await
            .unwrap()
            .bytes()
            .await
            .unwrap();
        assert_eq!(start.len(), HAVE);
        std::fs::write(&part, &start).unwrap();
        let mut first_seen = None;
        let path = download_and_verify_from(&job, std::slice::from_ref(&copy), |progress| {
            first_seen.get_or_insert(progress.downloaded);
        })
        .await
        .unwrap();
        assert_eq!(first_seen, Some(HAVE as u64));
        assert_eq!(sha256_of(&path).unwrap(), file.sha256);
    }
}
