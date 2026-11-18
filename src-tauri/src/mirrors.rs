//! Where a file can be downloaded from when its own server cannot be reached, as it often
//! cannot be from mainland China. Every file the app downloads is checked against a SHA-256
//! it already knows (the update list against a signature), so a copy from elsewhere is used
//! only when it is the same file.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use futures_util::StreamExt;

/// Sent with every download. ModelScope refuses a request that names no client.
pub const AGENT: &str = concat!("Speech/", env!("CARGO_PKG_VERSION"));

/// How much of a file each source is asked for when they are timed, and how long they
/// get to send it.
const PROBE_BYTES: u64 = 512 * 1024;
#[cfg(not(test))]
const PROBE_TIME: Duration = Duration::from_secs(4);
#[cfg(test)]
const PROBE_TIME: Duration = Duration::from_millis(600);

/// The project's own copies on ModelScope, which is quick from mainland China and
/// reachable from elsewhere: a Huggingface file at `hf/<user>/<repo>/<revision>/<file>`,
/// a GitHub release file at `gh/<owner>/<repo>/<tag>/<file>`. Tried first.
pub const MODELSCOPE: Option<&str> =
    Some("https://www.modelscope.cn/models/penguinmiaou/speech-files/resolve/master/");

/// Public proxies that fetch a GitHub release file on the downloader's behalf: the address
/// of the file is put after the proxy's own. Tried last, in this order.
pub const GITHUB_PROXIES: [&str; 3] = [
    "https://ghfast.top/",
    "https://gh-proxy.com/",
    "https://ghproxy.net/",
];

const HUGGINGFACE: &str = "https://huggingface.co/";
const HUGGINGFACE_MIRROR: &str = "https://hf-mirror.com/";
const GITHUB: &str = "https://github.com/";
const RELEASE_FILE: &str = "/releases/download/";

/// The host of the source that delivered the last file, so the next file tries it first.
static LAST: Mutex<Option<String>> = Mutex::new(None);

/// Where `url` sits in the ModelScope copy, when it is a file kept there.
pub fn modelscope_path(url: &str) -> Option<String> {
    if let Some(path) = url.strip_prefix(HUGGINGFACE) {
        let (repo, rest) = path.split_once("/resolve/")?;
        return Some(format!("hf/{repo}/{rest}"));
    }
    let path = url.strip_prefix(GITHUB)?;
    let (repo, rest) = path.split_once(RELEASE_FILE)?;
    Some(format!("gh/{repo}/{rest}"))
}

/// The addresses to try for `url`, in order: the ModelScope copy, the file's own server,
/// then the Huggingface mirror or the GitHub proxies. The source that delivered the
/// last file goes first.
pub fn candidates(url: &str) -> Vec<String> {
    candidates_from(url, MODELSCOPE)
}

fn candidates_from(url: &str, modelscope: Option<&str>) -> Vec<String> {
    let mut found = sources(url, modelscope);
    let last = LAST.lock().unwrap_or_else(|p| p.into_inner()).clone();
    if let Some(index) = last.and_then(|last| found.iter().position(|url| host_of(url) == last)) {
        let first = found.remove(index);
        found.insert(0, first);
    }
    found
}

/// Every address `url` can be had from, the ModelScope copy first.
fn sources(url: &str, modelscope: Option<&str>) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    if let (Some(base), Some(path)) = (modelscope, modelscope_path(url)) {
        found.push(format!("{base}{path}"));
    }
    found.push(url.to_string());
    if let Some(path) = url.strip_prefix(HUGGINGFACE) {
        found.push(format!("{HUGGINGFACE_MIRROR}{path}"));
    } else if url.starts_with(GITHUB) && url.contains(RELEASE_FILE) {
        found.extend(GITHUB_PROXIES.iter().map(|proxy| format!("{proxy}{url}")));
    }
    found
}

/// `sources` ordered by how fast each sends the start of the file, all of them asked at
/// once. Sources that sent nothing keep their order after the rest.
pub async fn fastest_first(client: &reqwest::Client, sources: Vec<String>) -> Vec<String> {
    let speeds =
        futures_util::future::join_all(sources.iter().map(|url| speed_of(client, url))).await;
    crate::applog::note(
        "download",
        &format!(
            "sources timed: {}",
            sources
                .iter()
                .zip(&speeds)
                .map(|(url, speed)| match speed {
                    Some(speed) => format!("{} {} KB/s", host_of(url), speed / 1024),
                    None => format!("{} none", host_of(url)),
                })
                .collect::<Vec<_>>()
                .join(", ")
        ),
    );
    let mut ranked: Vec<(u64, usize, String)> = sources
        .into_iter()
        .zip(speeds)
        .enumerate()
        .map(|(index, (url, speed))| (speed.unwrap_or(0), index, url))
        .collect();
    ranked.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    ranked.into_iter().map(|(_, _, url)| url).collect()
}

/// Bytes per second `url` sends of the start of its file, counted from the request, or
/// `None` when it sends nothing in [`PROBE_TIME`].
async fn speed_of(client: &reqwest::Client, url: &str) -> Option<u64> {
    let started = Instant::now();
    let deadline = tokio::time::Instant::now() + PROBE_TIME;
    let request = client
        .get(url)
        .header(reqwest::header::USER_AGENT, AGENT)
        .header(
            reqwest::header::RANGE,
            format!("bytes=0-{}", PROBE_BYTES - 1),
        )
        .send();
    let response = tokio::time::timeout_at(deadline, request)
        .await
        .ok()?
        .ok()?;
    if !response.status().is_success() {
        return None;
    }
    let mut stream = response.bytes_stream();
    let mut received = 0u64;
    while received < PROBE_BYTES {
        match tokio::time::timeout_at(deadline, stream.next()).await {
            Ok(Some(Ok(chunk))) => received += chunk.len() as u64,
            _ => break,
        }
    }
    let seconds = started.elapsed().as_secs_f64().max(0.001);
    (received > 0).then(|| (received as f64 / seconds) as u64)
}

/// Whether `url` goes through a public proxy or mirror rather than to the file's own
/// server or the project's ModelScope copy.
pub fn relayed(url: &str) -> bool {
    url.starts_with(HUGGINGFACE_MIRROR) || GITHUB_PROXIES.iter().any(|proxy| url.starts_with(proxy))
}

/// Remembers which source delivered a file.
pub fn worked(used: &str) {
    *LAST.lock().unwrap_or_else(|p| p.into_inner()) = Some(host_of(used).to_string());
}

#[cfg(test)]
fn forget() {
    *LAST.lock().unwrap() = None;
}

/// The host of an address, for the log.
pub fn host_of(url: &str) -> &str {
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    rest.split('/').next().unwrap_or(rest)
}

#[cfg(test)]
mod tests {
    use super::*;

    const HF: &str = "https://huggingface.co/user/repo/resolve/abc123/model.onnx";
    const GH: &str = "https://github.com/owner/repo/releases/download/tag/file.gguf";

    #[test]
    fn a_huggingface_file_can_come_from_the_mirror_in_china() {
        let _guard = LOCK.lock().unwrap();
        assert_eq!(
            candidates_from(HF, None),
            vec![
                HF.to_string(),
                "https://hf-mirror.com/user/repo/resolve/abc123/model.onnx".to_string()
            ]
        );
    }

    #[test]
    fn a_github_release_file_can_come_through_the_proxies() {
        let _guard = LOCK.lock().unwrap();
        let found = candidates_from(GH, None);
        assert_eq!(found[0], GH);
        assert_eq!(found.len(), 1 + GITHUB_PROXIES.len());
        assert!(found[1..].iter().all(|url| url.ends_with(GH)));
    }

    #[test]
    fn other_addresses_have_no_other_source() {
        assert_eq!(
            candidates_from("http://127.0.0.1:9/model.gguf", None),
            vec!["http://127.0.0.1:9/model.gguf".to_string()]
        );
        let page = "https://github.com/owner/repo/archive/main.zip";
        assert_eq!(candidates_from(page, None), vec![page.to_string()]);
    }

    #[test]
    fn the_source_that_worked_last_is_tried_first() {
        let _guard = LOCK.lock().unwrap();
        forget();
        let proxied = candidates_from(GH, None)[2].clone();
        worked(&proxied);
        let found = candidates_from(GH, None);
        assert_eq!(found[0], proxied);
        assert_eq!(found.len(), 1 + GITHUB_PROXIES.len());
        assert!(found.contains(&GH.to_string()));
        assert_eq!(candidates_from(HF, None)[0], HF);
        worked(GH);
        assert_eq!(candidates_from(GH, None)[0], GH);
        forget();
    }

    const COPY: &str = "https://www.modelscope.cn/models/owner/files/resolve/master/";

    #[test]
    fn the_modelscope_copy_is_tried_first() {
        let _guard = LOCK.lock().unwrap();
        forget();
        let found = candidates_from(GH, Some(COPY));
        assert_eq!(found[0], format!("{COPY}gh/owner/repo/tag/file.gguf"));
        assert_eq!(found[1], GH);
        let found = candidates_from(HF, Some(COPY));
        assert_eq!(found[0], format!("{COPY}hf/user/repo/abc123/model.onnx"));
        assert_eq!(&found[1..], candidates_from(HF, None).as_slice());
        let local = "http://127.0.0.1:9/model.gguf";
        assert_eq!(candidates_from(local, Some(COPY)), vec![local.to_string()]);
    }

    #[test]
    fn after_the_copy_failed_the_source_that_worked_goes_first() {
        let _guard = LOCK.lock().unwrap();
        forget();
        worked(GH);
        assert_eq!(candidates_from(GH, Some(COPY))[0], GH);
        worked(&format!("{COPY}gh/x"));
        assert!(candidates_from(HF, Some(COPY))[0].starts_with(COPY));
        forget();
    }

    /// A server that answers after `wait` and then sends `body` at once.
    async fn answers_after(wait: Duration, body: &'static [u8]) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move {
            while let Ok((mut socket, _)) = listener.accept().await {
                tokio::spawn(async move {
                    use tokio::io::{AsyncReadExt, AsyncWriteExt};
                    let mut request = [0u8; 1024];
                    let _ = socket.read(&mut request).await;
                    tokio::time::sleep(wait).await;
                    let head = format!("HTTP/1.1 200 OK\r\ncontent-length: {}\r\n\r\n", body.len());
                    let _ = socket.write_all(head.as_bytes()).await;
                    let _ = socket.write_all(body).await;
                });
            }
        });
        format!("http://{address}/model.gguf")
    }

    #[tokio::test]
    async fn the_quickest_source_is_tried_first_and_a_silent_one_last() {
        static BODY: [u8; 64 * 1024] = [7; 64 * 1024];
        let silent = answers_after(Duration::from_secs(5), &BODY).await;
        let slow = answers_after(Duration::from_millis(300), &BODY).await;
        let quick = answers_after(Duration::ZERO, &BODY).await;
        let refused = "http://127.0.0.1:9/model.gguf".to_string();
        let order = fastest_first(
            &reqwest::Client::new(),
            vec![silent.clone(), refused.clone(), slow.clone(), quick.clone()],
        )
        .await;
        assert_eq!(order, vec![quick, slow, silent, refused]);
    }

    #[test]
    fn only_the_mirror_and_the_proxies_are_relayed() {
        let found = candidates_from(GH, Some(COPY));
        let through: Vec<bool> = found.iter().map(|url| relayed(url)).collect();
        assert_eq!(through[..2], [false, false]);
        assert!(through[2..].iter().all(|through| *through));
        assert!(relayed(&candidates_from(HF, None)[1]));
    }

    #[test]
    fn the_log_names_only_the_host_a_file_came_from() {
        assert_eq!(host_of("https://hf-mirror.com/a/b"), "hf-mirror.com");
        assert_eq!(
            host_of(&format!("{}{GH}", GITHUB_PROXIES[0])),
            host_of(GITHUB_PROXIES[0])
        );
    }

    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
}
