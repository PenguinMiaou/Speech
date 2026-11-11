//! Past meetings: the text, the user's notes, the minutes and the speakers' names of each,
//! one file per meeting in the app's data folder.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

/// How much of a meeting's text the list shows.
const PREVIEW_CHARS: usize = 80;

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Meeting {
    pub id: String,
    /// When it began, in milliseconds since 1970.
    pub started_at: u64,
    pub transcript: String,
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub minutes: String,
    #[serde(default)]
    pub names: HashMap<u32, String>,
    /// The name the user gave it. Empty until they do.
    #[serde(default)]
    pub title: String,
    /// How long the recording ran, in seconds. Zero when that is not known.
    #[serde(default)]
    pub duration_s: u64,
}

/// A meeting as the list shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Summary {
    pub id: String,
    pub started_at: u64,
    pub preview: String,
    pub has_minutes: bool,
    pub title: String,
    pub duration_s: u64,
    pub has_audio: bool,
}

static DATA_DIR: OnceLock<PathBuf> = OnceLock::new();

pub fn init(data_dir: &Path) {
    let _ = DATA_DIR.set(data_dir.to_path_buf());
}

fn dir() -> Result<PathBuf, String> {
    DATA_DIR
        .get()
        .map(|dir| dir.join("meetings"))
        .ok_or_else(|| "meeting_store_unavailable".to_string())
}

/// An id names a file, so it may hold nothing a path could be built from.
fn file_of(dir: &Path, id: &str) -> Result<PathBuf, String> {
    let plain = !id.is_empty()
        && id.len() <= 64
        && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-');
    if plain {
        Ok(dir.join(format!("{id}.json")))
    } else {
        Err("meeting_id_invalid".to_string())
    }
}

fn preview(transcript: &str) -> String {
    let line: String = transcript.split_whitespace().collect::<Vec<_>>().join(" ");
    line.chars().take(PREVIEW_CHARS).collect()
}

fn save_in(dir: &Path, meeting: &Meeting) -> Result<(), String> {
    let file = file_of(dir, &meeting.id)?;
    std::fs::create_dir_all(dir).map_err(|error| error.to_string())?;
    let raw = serde_json::to_vec(meeting).map_err(|error| error.to_string())?;
    std::fs::write(file, raw).map_err(|error| error.to_string())
}

fn get_in(dir: &Path, id: &str) -> Result<Meeting, String> {
    let raw = std::fs::read(file_of(dir, id)?).map_err(|_| "meeting_not_found".to_string())?;
    serde_json::from_slice(&raw).map_err(|_| "meeting_not_found".to_string())
}

/// Every meeting, the newest first.
fn list_in(dir: &Path) -> Vec<Summary> {
    let mut all: Vec<Summary> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        // A meeting's sound lies beside its text and is far too large to read here.
        .filter(|path| path.extension().is_some_and(|kind| kind == "json"))
        .filter_map(|path| std::fs::read(path).ok())
        .filter_map(|raw| serde_json::from_slice::<Meeting>(&raw).ok())
        .map(|meeting| Summary {
            preview: preview(&meeting.transcript),
            has_minutes: !meeting.minutes.trim().is_empty(),
            has_audio: audio_in(dir, &meeting.id).is_some(),
            title: meeting.title.trim().to_string(),
            duration_s: meeting.duration_s,
            id: meeting.id,
            started_at: meeting.started_at,
        })
        .collect();
    all.sort_by(|a, b| b.started_at.cmp(&a.started_at).then(b.id.cmp(&a.id)));
    all
}

/// Where a meeting's sound is kept, beside its text.
fn audio_file(dir: &Path, id: &str) -> Result<PathBuf, String> {
    Ok(file_of(dir, id)?.with_extension("wav"))
}

fn audio_in(dir: &Path, id: &str) -> Option<PathBuf> {
    audio_file(dir, id).ok().filter(|path| path.is_file())
}

/// Moves a finished recording to the meeting it belongs to.
fn keep_audio_in(dir: &Path, id: &str, recording: &Path) -> Result<PathBuf, String> {
    let kept = audio_file(dir, id)?;
    std::fs::create_dir_all(dir).map_err(|error| error.to_string())?;
    std::fs::rename(recording, &kept).map_err(|error| error.to_string())?;
    Ok(kept)
}

fn delete_audio_in(dir: &Path, id: &str) -> Result<(), String> {
    match std::fs::remove_file(audio_file(dir, id)?) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error.to_string()),
        _ => Ok(()),
    }
}

fn delete_in(dir: &Path, id: &str) -> Result<(), String> {
    delete_audio_in(dir, id)?;
    match std::fs::remove_file(file_of(dir, id)?) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error.to_string()),
        _ => Ok(()),
    }
}

pub fn save(meeting: &Meeting) -> Result<(), String> {
    save_in(&dir()?, meeting)
}

pub fn get(id: &str) -> Result<Meeting, String> {
    get_in(&dir()?, id)
}

pub fn list() -> Vec<Summary> {
    dir().map(|dir| list_in(&dir)).unwrap_or_default()
}

pub fn delete(id: &str) -> Result<(), String> {
    delete_in(&dir()?, id)
}

/// The sound of a meeting, if it is still kept.
pub fn audio(id: &str) -> Option<PathBuf> {
    audio_in(&dir().ok()?, id)
}

pub fn keep_audio(id: &str, recording: &Path) -> Result<PathBuf, String> {
    keep_audio_in(&dir()?, id, recording)
}

pub fn delete_audio(id: &str) -> Result<(), String> {
    delete_audio_in(&dir()?, id)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ot-meetings-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    fn meeting(id: &str, started_at: u64, transcript: &str) -> Meeting {
        Meeting {
            id: id.into(),
            started_at,
            transcript: transcript.into(),
            ..Meeting::default()
        }
    }

    #[test]
    fn a_meeting_takes_over_the_recording_and_loses_it_when_deleted() {
        let dir = scratch("audio");
        save_in(&dir, &meeting("m1", 10, "hello")).unwrap();
        let recording = dir.join("fresh.wav");
        std::fs::write(&recording, b"RIFF").unwrap();
        assert_eq!(audio_in(&dir, "m1"), None);
        let kept = keep_audio_in(&dir, "m1", &recording).unwrap();
        assert_eq!(kept, dir.join("m1.wav"));
        assert!(!recording.exists());
        assert_eq!(audio_in(&dir, "m1"), Some(kept.clone()));
        // The sound file is not taken for a meeting, and the list says which meeting has sound.
        let list = list_in(&dir);
        assert_eq!(list.len(), 1);
        assert!(list[0].has_audio);
        delete_in(&dir, "m1").unwrap();
        assert!(!kept.exists());
    }

    #[test]
    fn the_sound_of_a_meeting_can_go_while_its_text_stays() {
        let dir = scratch("audio-only");
        save_in(&dir, &meeting("m1", 10, "hello")).unwrap();
        let recording = dir.join("fresh.wav");
        std::fs::write(&recording, b"RIFF").unwrap();
        keep_audio_in(&dir, "m1", &recording).unwrap();
        delete_audio_in(&dir, "m1").unwrap();
        assert_eq!(audio_in(&dir, "m1"), None);
        assert_eq!(get_in(&dir, "m1").unwrap().transcript, "hello");
        assert!(keep_audio_in(&dir, "../x", &recording).is_err());
    }

    #[test]
    fn the_list_shows_a_title_and_how_long_the_meeting_was() {
        let dir = scratch("title");
        let mut named = meeting("m1", 10, "hello");
        named.title = "Weekly sync".into();
        named.duration_s = 754;
        save_in(&dir, &named).unwrap();
        let list = list_in(&dir);
        assert_eq!(list[0].title, "Weekly sync");
        assert_eq!(list[0].duration_s, 754);
        // A meeting saved before titles existed still opens.
        let old = br#"{"id":"m0","started_at":1,"transcript":"x"}"#;
        std::fs::write(dir.join("m0.json"), old).unwrap();
        assert_eq!(get_in(&dir, "m0").unwrap().title, "");
    }

    #[test]
    fn a_saved_meeting_comes_back_whole_and_saving_again_replaces_it() {
        let dir = scratch("save");
        let mut first = meeting("m1", 10, "说话人 1  00:00\n你好");
        first.names.insert(1, "Ann".into());
        save_in(&dir, &first).unwrap();
        assert_eq!(get_in(&dir, "m1").unwrap(), first);
        first.minutes = "## 结论".into();
        save_in(&dir, &first).unwrap();
        assert_eq!(get_in(&dir, "m1").unwrap().minutes, "## 结论");
        assert_eq!(list_in(&dir).len(), 1);
    }

    #[test]
    fn the_list_has_the_newest_first_with_a_one_line_preview() {
        let dir = scratch("list");
        save_in(&dir, &meeting("a", 10, "old")).unwrap();
        let mut new = meeting("b", 20, &format!("first line\n\n{}", "字".repeat(200)));
        new.minutes = "done".into();
        save_in(&dir, &new).unwrap();
        let list = list_in(&dir);
        assert_eq!(
            list.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(),
            ["b", "a"]
        );
        assert!(list[0].preview.starts_with("first line 字"));
        assert_eq!(list[0].preview.chars().count(), PREVIEW_CHARS);
        assert_eq!((list[0].has_minutes, list[1].has_minutes), (true, false));
        assert!(list_in(&scratch("none")).is_empty());
    }

    #[test]
    fn a_deleted_meeting_is_gone_and_deleting_it_again_is_fine() {
        let dir = scratch("delete");
        save_in(&dir, &meeting("a", 10, "text")).unwrap();
        delete_in(&dir, "a").unwrap();
        delete_in(&dir, "a").unwrap();
        assert_eq!(get_in(&dir, "a").unwrap_err(), "meeting_not_found");
    }

    #[test]
    fn an_id_that_could_name_another_file_is_refused() {
        let dir = scratch("ids");
        for id in ["", "../config", "a/b", "a.json", "a b"] {
            assert_eq!(
                save_in(&dir, &meeting(id, 1, "x")).unwrap_err(),
                "meeting_id_invalid"
            );
            assert_eq!(get_in(&dir, id).unwrap_err(), "meeting_id_invalid");
            assert_eq!(delete_in(&dir, id).unwrap_err(), "meeting_id_invalid");
        }
    }
}
