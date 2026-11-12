//! Updates from the project's releases: only the program files that changed are downloaded.

use std::collections::HashMap;
use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};

pub const RELEASES_URL: &str = "https://github.com/PenguinMiaou/Speech/releases";
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

/// The platform this build can update in place, if any.
pub fn platform_key() -> Option<&'static str> {
    if cfg!(target_os = "windows") && cfg!(target_arch = "x86_64") {
        Some("windows-x64")
    } else if cfg!(target_os = "macos") && cfg!(target_arch = "aarch64") {
        Some(MACOS_KEY)
    } else {
        None
    }
}

/// On macOS a changed program file breaks the app's signature, and with it the
/// permissions the user granted. So the whole signed app is downloaded there and takes
/// the place of the one in use.
pub const MACOS_KEY: &str = "macos-arm64";

/// The app a program runs from: the folder ending in `.app` two levels above it.
pub fn bundle_of(exe: &Path) -> Option<std::path::PathBuf> {
    let macos = exe.parent()?;
    let contents = macos.parent()?;
    let bundle = contents.parent()?;
    (macos.file_name()? == "MacOS"
        && contents.file_name()? == "Contents"
        && bundle.extension()? == "app")
        .then(|| bundle.to_path_buf())
}

/// Where a new app is downloaded and unpacked: next to the one in use, so that taking
/// its place is a rename and cannot stop half way.
pub fn staging_dir(bundle: &Path) -> std::path::PathBuf {
    bundle.with_file_name(".speech-update")
}

/// Unpacks a downloaded app into `staging` and gives the one app it holds.
pub fn unpack_app(archive: &Path, staging: &Path) -> io::Result<std::path::PathBuf> {
    let refused = |why: &str| io::Error::new(io::ErrorKind::InvalidData, why.to_string());
    let into = staging.join("unpacked");
    let _ = std::fs::remove_dir_all(&into);
    std::fs::create_dir_all(&into)?;
    let done = std::process::Command::new("/usr/bin/tar")
        .arg("-xzf")
        .arg(archive)
        .arg("-C")
        .arg(&into)
        .status()?;
    if !done.success() {
        return Err(refused("the download could not be unpacked"));
    }
    let mut held = std::fs::read_dir(&into)?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            !path
                .file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with("._"))
        });
    match (held.next(), held.next()) {
        (Some(app), None)
            if app.extension().is_some_and(|kind| kind == "app")
                && app.symlink_metadata()?.is_dir() =>
        {
            Ok(app)
        }
        _ => Err(refused("the download does not hold one app")),
    }
}

/// Whether Apple's tools take `app` for intact and signed by the same team as `like`.
#[cfg(target_os = "macos")]
pub fn signed_like(app: &Path, like: &Path) -> io::Result<()> {
    let refused = |why: &str| io::Error::new(io::ErrorKind::InvalidData, why.to_string());
    let intact = std::process::Command::new("/usr/bin/codesign")
        .args(["--verify", "--deep", "--strict"])
        .arg(app)
        .output()?;
    if !intact.status.success() {
        return Err(refused("the downloaded app is not intact"));
    }
    let team = |bundle: &Path| -> io::Result<Option<String>> {
        let told = std::process::Command::new("/usr/bin/codesign")
            .args(["-dv", "--verbose=2"])
            .arg(bundle)
            .output()?;
        Ok(String::from_utf8_lossy(&told.stderr)
            .lines()
            .find_map(|line| line.strip_prefix("TeamIdentifier="))
            .map(str::to_string))
    };
    match (team(app)?, team(like)?) {
        (Some(new), Some(now)) if new == now && new != "not set" => Ok(()),
        _ => Err(refused("the downloaded app is not signed as this one is")),
    }
}

/// Puts `new` where `bundle` is. The app in use steps aside into `staging`, where the
/// next start clears it away, and comes back if the new one cannot take its place.
pub fn swap_bundle(new: &Path, bundle: &Path, staging: &Path) -> io::Result<()> {
    if !new.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "the new app was not unpacked",
        ));
    }
    let aside = staging.join("replaced.app");
    let _ = std::fs::remove_dir_all(&aside);
    std::fs::rename(bundle, &aside)?;
    if let Err(error) = std::fs::rename(new, bundle) {
        let _ = std::fs::rename(&aside, bundle);
        return Err(error);
    }
    Ok(())
}

/// Whether a new app already stands in the place of the one in use, waiting for a
/// restart.
pub fn bundle_swapped(bundle: &Path) -> bool {
    staging_dir(bundle).join("replaced.app").is_dir()
}

/// Removes what an earlier update of the app left next to it.
pub fn remove_bundle_leftovers(bundle: &Path) {
    let _ = std::fs::remove_dir_all(staging_dir(bundle));
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

/// The release entry that holds the files of the newest version, kept apart from the
/// release pages so those list only what a person installs.
const UPDATE_FILES_TAG: &str = "update-files";

pub fn asset_url(asset: &str) -> String {
    format!("{RELEASES_URL}/download/{UPDATE_FILES_TAG}/{asset}")
}

pub fn manifest_url() -> String {
    asset_url(MANIFEST_NAME)
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
            asset_url("opentypeless-windows-x64.exe"),
            format!("{RELEASES_URL}/download/update-files/opentypeless-windows-x64.exe")
        );
        assert_eq!(
            manifest_url(),
            format!("{RELEASES_URL}/download/update-files/update-manifest.json")
        );
    }

    #[test]
    fn the_app_a_program_runs_from_is_the_folder_that_ends_in_app() {
        assert_eq!(
            bundle_of(Path::new(
                "/Applications/Speech.app/Contents/MacOS/opentypeless"
            )),
            Some(std::path::PathBuf::from("/Applications/Speech.app"))
        );
        assert_eq!(bundle_of(Path::new("/usr/local/bin/opentypeless")), None);
        assert_eq!(bundle_of(Path::new("/x/Speech.app/opentypeless")), None);
    }

    #[cfg(unix)]
    #[test]
    fn a_downloaded_app_is_unpacked_and_takes_the_place_of_the_one_in_use() {
        let dir = scratch("app-swap");
        let made = dir.join("made/Speech.app/Contents");
        std::fs::create_dir_all(&made).unwrap();
        std::fs::write(made.join("version"), "new").unwrap();
        let archive = dir.join("Speech.app.tar.gz");
        assert!(std::process::Command::new("tar")
            .arg("-czf")
            .arg(&archive)
            .arg("-C")
            .arg(dir.join("made"))
            .arg("Speech.app")
            .status()
            .unwrap()
            .success());
        let bundle = dir.join("Speech.app");
        std::fs::create_dir_all(bundle.join("Contents")).unwrap();
        std::fs::write(bundle.join("Contents/version"), "old").unwrap();

        let staging = staging_dir(&bundle);
        let unpacked = unpack_app(&archive, &staging).unwrap();
        swap_bundle(&unpacked, &bundle, &staging).unwrap();

        assert_eq!(
            std::fs::read_to_string(bundle.join("Contents/version")).unwrap(),
            "new"
        );
        // The app that was in use stands aside until the next start clears it away.
        assert_eq!(
            std::fs::read_to_string(staging.join("replaced.app/Contents/version")).unwrap(),
            "old"
        );
        assert!(bundle_swapped(&bundle));
        remove_bundle_leftovers(&bundle);
        assert!(!staging.exists());
        assert!(!bundle_swapped(&bundle));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn an_archive_that_holds_no_app_or_reaches_outside_is_refused() {
        let dir = scratch("app-refuse");
        std::fs::create_dir_all(dir.join("made")).unwrap();
        std::fs::write(dir.join("made/notes.txt"), "x").unwrap();
        let archive = dir.join("a.tar.gz");
        assert!(std::process::Command::new("tar")
            .arg("-czf")
            .arg(&archive)
            .arg("-C")
            .arg(dir.join("made"))
            .arg("notes.txt")
            .status()
            .unwrap()
            .success());
        assert!(unpack_app(&archive, &dir.join("staging")).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_app_in_use_stays_where_it_is_when_the_new_one_cannot_take_its_place() {
        let dir = scratch("app-keep");
        let bundle = dir.join("Speech.app");
        std::fs::create_dir_all(&bundle).unwrap();
        std::fs::write(bundle.join("version"), "old").unwrap();
        let staging = staging_dir(&bundle);
        std::fs::create_dir_all(&staging).unwrap();
        assert!(swap_bundle(&staging.join("missing.app"), &bundle, &staging).is_err());
        assert_eq!(
            std::fs::read_to_string(bundle.join("version")).unwrap(),
            "old"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Packs the signed app at `SIGNED_APP` as a release does, unpacks it and checks it
    /// the way an update does.
    #[cfg(target_os = "macos")]
    #[test]
    #[ignore = "needs a signed app on this machine"]
    fn a_signed_app_packed_and_unpacked_is_still_taken_for_intact() {
        let app = std::path::PathBuf::from(std::env::var("SIGNED_APP").expect("SIGNED_APP"));
        let dir = scratch("app-signed");
        let archive = dir.join("app.tar.gz");
        assert!(std::process::Command::new("tar")
            .arg("-czf")
            .arg(&archive)
            .arg("-C")
            .arg(app.parent().unwrap())
            .arg(app.file_name().unwrap())
            .status()
            .unwrap()
            .success());
        let unpacked = unpack_app(&archive, &dir.join("staging")).unwrap();
        signed_like(&unpacked, &app).unwrap();
        // An app that was changed after it was signed is refused.
        std::fs::write(unpacked.join("Contents/Info.plist"), "changed").unwrap();
        assert!(signed_like(&unpacked, &app).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
