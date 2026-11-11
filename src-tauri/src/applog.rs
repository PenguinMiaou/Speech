//! A plain record of what the app did and what went wrong, kept in a file on this machine
//! so that a fault can be looked into afterwards. It holds states, counts and error
//! messages, and never what was said, typed or read.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

/// A log that has grown past this is put aside as the old one, and a new one begun.
const LARGEST: u64 = 1_000_000;
/// The longest a single entry may be, in characters.
const LONGEST_ENTRY: usize = 400;

static FILE: OnceLock<PathBuf> = OnceLock::new();
static WRITING: Mutex<()> = Mutex::new(());

pub fn init(data_dir: &Path) {
    let _ = FILE.set(data_dir.join("speech.log"));
    note(
        "app",
        &format!(
            "started {} on {} {}{}",
            env!("CARGO_PKG_VERSION"),
            std::env::consts::OS,
            std::env::consts::ARCH,
            processor()
        ),
    );
}

/// What the processor can do, where the speech models depend on it.
fn processor() -> String {
    #[cfg(target_arch = "x86_64")]
    {
        let has = |name: &str, there: bool| {
            if there {
                format!(" {name}")
            } else {
                String::new()
            }
        };
        format!(
            ",{}{}{}{}",
            has("avx", std::is_x86_feature_detected!("avx")),
            has("avx2", std::is_x86_feature_detected!("avx2")),
            has("fma", std::is_x86_feature_detected!("fma")),
            has("avx512f", std::is_x86_feature_detected!("avx512f")),
        )
    }
    #[cfg(not(target_arch = "x86_64"))]
    String::new()
}

/// Where the log is kept, once the app has started.
pub fn path() -> Option<PathBuf> {
    FILE.get().cloned()
}

/// One entry as it is written: the time, the part of the app, and what happened on one
/// line of bounded length.
pub fn entry(when: &str, area: &str, what: &str) -> String {
    let what: String = what
        .chars()
        .map(|each| if each.is_control() { ' ' } else { each })
        .take(LONGEST_ENTRY)
        .collect();
    format!("{when} {area} {}\n", what.trim())
}

/// Adds an entry to the file at `path`, putting a file that has grown too large aside.
pub fn write(path: &Path, line: &str) -> std::io::Result<()> {
    if std::fs::metadata(path).is_ok_and(|file| file.len() > LARGEST) {
        let _ = std::fs::rename(path, path.with_extension("old.log"));
    }
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?
        .write_all(line.as_bytes())
}

/// Records that something happened. `what` must not hold anything said, typed or read.
pub fn note(area: &str, what: &str) {
    let Some(path) = FILE.get() else { return };
    let when = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let _held = WRITING
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let _ = write(path, &entry(&when, area, what));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_entry_is_one_line_of_bounded_length() {
        let line = entry("2026-01-01 10:00:00", "meeting", " failed:\nengine\r\n ");
        assert_eq!(line, "2026-01-01 10:00:00 meeting failed: engine\n");
        let long = entry("t", "a", &"x".repeat(5_000));
        assert_eq!(long.chars().count(), "t a \n".len() + LONGEST_ENTRY);
    }

    #[test]
    fn entries_follow_one_another_and_a_large_log_is_put_aside() {
        let dir = std::env::temp_dir().join(format!("ot-applog-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("speech.log");
        let _ = std::fs::remove_file(&path);
        write(&path, "one\n").unwrap();
        write(&path, "two\n").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "one\ntwo\n");

        std::fs::write(&path, vec![b'x'; LARGEST as usize + 1]).unwrap();
        write(&path, "three\n").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "three\n");
        assert!(dir.join("speech.old.log").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
