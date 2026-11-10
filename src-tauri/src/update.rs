//! Updates from the project's releases: only the program files that changed are downloaded.

use std::collections::HashMap;
use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};

pub const RELEASES_URL: &str = "https://github.com/PenguinMiaou/opentypeless-local/releases";
pub const MANIFEST_NAME: &str = "update-manifest.json";

/// One program file of a release.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct FileEntry {
    /// The file's name next to the running program.
    pub name: String,
    /// The release asset that holds it.
    pub asset: String,
    pub sha256: String,
    pub size: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Manifest {
    pub version: String,
    /// Program files per platform, keyed like `windows-x64`.
    pub files: HashMap<String, Vec<FileEntry>>,
}

/// The platform this build can patch in place, if any.
pub fn platform_key() -> Option<&'static str> {
    // On macOS a changed program file loses the permissions the user granted, so a new release
    // is installed as a whole there.
    (cfg!(target_os = "windows") && cfg!(target_arch = "x86_64")).then_some("windows-x64")
}

/// Whether `candidate` is a later release than `current`. Anything unreadable is not newer.
pub fn is_newer(candidate: &str, current: &str) -> bool {
    fn numbers(version: &str) -> Option<[u64; 3]> {
        let mut parts = version.trim().trim_start_matches('v').split('.');
        let numbers = [
            parts.next()?.parse().ok()?,
            parts.next()?.parse().ok()?,
            parts.next()?.parse().ok()?,
        ];
        parts.next().is_none().then_some(numbers)
    }
    match (numbers(candidate), numbers(current)) {
        (Some(candidate), Some(current)) => candidate > current,
        _ => false,
    }
}

/// A name that stays inside the directory it is joined to.
pub fn is_plain_file_name(name: &str) -> bool {
    !name.is_empty()
        && name != "."
        && name != ".."
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
}

/// The entries whose file in `dir` is missing or differs.
pub fn changed_files(entries: &[FileEntry], dir: &Path) -> io::Result<Vec<FileEntry>> {
    let mut changed = Vec::new();
    for entry in entries {
        if !is_plain_file_name(&entry.name) || !is_plain_file_name(&entry.asset) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "the release names a file outside the program folder",
            ));
        }
        let path = dir.join(&entry.name);
        let same = path.is_file()
            && crate::local_stt::install::sha256_of(&path)?.eq_ignore_ascii_case(&entry.sha256);
        if !same {
            changed.push(entry.clone());
        }
    }
    Ok(changed)
}

pub fn staged_name(name: &str) -> String {
    format!("{name}.new")
}

fn replaced_name(name: &str) -> String {
    format!("{name}.old")
}

/// Puts every staged file in place. A running program cannot be overwritten, but it can be
/// renamed, so the current file steps aside and is removed at the next start.
pub fn swap_in(dir: &Path, names: &[String]) -> io::Result<()> {
    if let Some(missing) = names
        .iter()
        .find(|name| !dir.join(staged_name(name)).is_file())
    {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("{missing} was not downloaded"),
        ));
    }
    for name in names {
        let current = dir.join(name);
        if current.exists() {
            let aside = dir.join(replaced_name(name));
            let _ = std::fs::remove_file(&aside);
            std::fs::rename(&current, aside)?;
        }
        std::fs::rename(dir.join(staged_name(name)), current)?;
    }
    Ok(())
}

/// Removes what earlier updates left behind.
pub fn remove_leftovers(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.ends_with(".old") || name.ends_with(".new") || name.ends_with(".new.part") {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

/// What the settings page shows about updates.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub current: String,
    pub latest: String,
    pub available: bool,
    /// Bytes to download to update in place. `None` where the release is installed as a whole.
    pub patch_bytes: Option<u64>,
    pub release_url: String,
}

pub fn status(
    manifest: &Manifest,
    current: &str,
    dir: &Path,
    platform: Option<&str>,
) -> io::Result<Status> {
    let available = is_newer(&manifest.version, current);
    let entries = platform.and_then(|key| manifest.files.get(key));
    let patch_bytes = match entries {
        Some(entries) if available => Some(
            changed_files(entries, dir)?
                .iter()
                .map(|entry| entry.size)
                .sum(),
        ),
        _ => None,
    };
    Ok(Status {
        current: current.to_string(),
        latest: manifest.version.clone(),
        available,
        patch_bytes,
        release_url: format!("{RELEASES_URL}/tag/v{}", manifest.version),
    })
}

pub fn asset_url(version: &str, asset: &str) -> String {
    format!("{RELEASES_URL}/download/v{version}/{asset}")
}

pub fn manifest_url() -> String {
    format!("{RELEASES_URL}/latest/download/{MANIFEST_NAME}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};

    fn scratch(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("ot-update-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn entry(name: &str, content: &[u8]) -> FileEntry {
        FileEntry {
            name: name.to_string(),
            asset: format!("{name}-asset"),
            sha256: format!("{:x}", Sha256::digest(content)),
            size: content.len() as u64,
        }
    }

    #[test]
    fn a_later_release_is_newer_and_anything_else_is_not() {
        assert!(is_newer("1.2.1", "1.2.0"));
        assert!(is_newer("1.10.0", "1.9.9"));
        assert!(is_newer("v2.0.0", "1.9.9"));
        assert!(!is_newer("1.2.0", "1.2.0"));
        assert!(!is_newer("1.1.9", "1.2.0"));
        assert!(!is_newer("latest", "1.2.0"));
        assert!(!is_newer("1.2", "1.2.0"));
        assert!(!is_newer("1.2.x", "1.2.0"));
    }

    #[test]
    fn only_files_that_differ_or_are_missing_need_downloading() {
        let dir = scratch("changed");
        std::fs::write(dir.join("app.exe"), b"old app").unwrap();
        std::fs::write(dir.join("worker.exe"), b"same worker").unwrap();
        let entries = [
            entry("app.exe", b"new app"),
            entry("worker.exe", b"same worker"),
            entry("extra.dll", b"new file"),
        ];

        let changed = changed_files(&entries, &dir).unwrap();

        let names: Vec<_> = changed.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["app.exe", "extra.dll"]);
    }

    #[test]
    fn a_name_that_leaves_the_directory_is_refused() {
        assert!(is_plain_file_name("opentypeless.exe"));
        assert!(is_plain_file_name("ot-stt-worker"));
        for name in [
            "",
            ".",
            "..",
            "../evil.exe",
            "a/b.exe",
            "a\\b.exe",
            "C:evil.exe",
        ] {
            assert!(!is_plain_file_name(name), "{name}");
        }
        let dir = scratch("refused");
        let entries = [entry("../evil.exe", b"x")];
        assert!(changed_files(&entries, &dir).is_err());
    }

    #[test]
    fn staged_files_replace_the_current_ones_which_step_aside() {
        let dir = scratch("swap");
        std::fs::write(dir.join("app.exe"), b"old").unwrap();
        std::fs::write(dir.join("app.exe.new"), b"new").unwrap();
        std::fs::write(dir.join("extra.dll.new"), b"added").unwrap();

        swap_in(&dir, &["app.exe".to_string(), "extra.dll".to_string()]).unwrap();

        assert_eq!(std::fs::read(dir.join("app.exe")).unwrap(), b"new");
        assert_eq!(std::fs::read(dir.join("app.exe.old")).unwrap(), b"old");
        assert_eq!(std::fs::read(dir.join("extra.dll")).unwrap(), b"added");
        assert!(!dir.join("app.exe.new").exists());
    }

    #[test]
    fn nothing_is_replaced_when_a_staged_file_is_missing() {
        let dir = scratch("incomplete");
        std::fs::write(dir.join("app.exe"), b"old").unwrap();
        std::fs::write(dir.join("app.exe.new"), b"new").unwrap();
        std::fs::write(dir.join("worker.exe"), b"old worker").unwrap();

        let names = ["app.exe".to_string(), "worker.exe".to_string()];
        assert!(swap_in(&dir, &names).is_err());

        assert_eq!(std::fs::read(dir.join("app.exe")).unwrap(), b"old");
        assert_eq!(
            std::fs::read(dir.join("worker.exe")).unwrap(),
            b"old worker"
        );
    }

    #[test]
    fn leftovers_of_an_earlier_update_are_removed() {
        let dir = scratch("leftovers");
        for name in [
            "app.exe",
            "app.exe.old",
            "worker.exe.new",
            "worker.exe.new.part",
        ] {
            std::fs::write(dir.join(name), b"x").unwrap();
        }

        remove_leftovers(&dir);

        assert!(dir.join("app.exe").exists());
        assert!(!dir.join("app.exe.old").exists());
        assert!(!dir.join("worker.exe.new").exists());
        assert!(!dir.join("worker.exe.new.part").exists());
    }

    fn release(version: &str, entries: Vec<FileEntry>) -> Manifest {
        Manifest {
            version: version.to_string(),
            files: HashMap::from([("windows-x64".to_string(), entries)]),
        }
    }

    #[test]
    fn a_newer_release_costs_only_the_files_that_changed() {
        let dir = scratch("status");
        std::fs::write(dir.join("worker.exe"), b"same worker").unwrap();
        let manifest = release(
            "1.2.1",
            vec![
                entry("app.exe", b"new app"),
                entry("worker.exe", b"same worker"),
            ],
        );

        let status = status(&manifest, "1.2.0", &dir, Some("windows-x64")).unwrap();

        assert!(status.available);
        assert_eq!(status.patch_bytes, Some(7));
        assert_eq!(status.latest, "1.2.1");
        assert_eq!(status.release_url, format!("{RELEASES_URL}/tag/v1.2.1"));
    }

    #[test]
    fn a_platform_without_files_gets_the_release_page_instead() {
        let dir = scratch("no-platform");
        let manifest = release("1.2.1", vec![entry("app.exe", b"new app")]);

        assert_eq!(
            status(&manifest, "1.2.0", &dir, None).unwrap().patch_bytes,
            None
        );
        let other = status(&manifest, "1.2.0", &dir, Some("macos-arm64")).unwrap();
        assert!(other.available);
        assert_eq!(other.patch_bytes, None);
    }

    #[test]
    fn the_current_release_needs_nothing() {
        let dir = scratch("current");
        let manifest = release("1.2.0", vec![entry("app.exe", b"new app")]);

        let status = status(&manifest, "1.2.0", &dir, Some("windows-x64")).unwrap();

        assert!(!status.available);
        assert_eq!(status.patch_bytes, None);
    }

    #[test]
    fn reads_a_release_manifest() {
        let manifest: Manifest = serde_json::from_str(
            r#"{"version":"1.2.1","files":{"windows-x64":[
                {"name":"opentypeless.exe","asset":"opentypeless-windows-x64.exe","sha256":"ab","size":3}]}}"#,
        )
        .unwrap();
        assert_eq!(
            manifest.files["windows-x64"][0].asset,
            "opentypeless-windows-x64.exe"
        );
        assert_eq!(
            asset_url(&manifest.version, "opentypeless-windows-x64.exe"),
            format!("{RELEASES_URL}/download/v1.2.1/opentypeless-windows-x64.exe")
        );
    }
}
