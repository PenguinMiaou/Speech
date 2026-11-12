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
    /// When the words under each heading of the transcript were said, in order. Empty
    /// where that was not written down.
    #[serde(default)]
    pub times: Vec<Times>,
    /// The terms a model was unsure how to spell, each the way it is now written, so
    /// that the user can be asked about them again. Empty where there were none.
    #[serde(default)]
    pub terms: Vec<crate::minutes::Term>,
    /// What the meeting said before it was tidied, so that it can still be compared
    /// with and gone back to. Absent where it was never tidied.
    #[serde(default)]
    pub unpolished: Option<Unpolished>,
    /// The folder the user keeps it in. Empty when it is in none.
    #[serde(default)]
    pub folder: String,
}

/// A meeting's text as it stood before it was tidied.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Unpolished {
    pub text: String,
    /// The same text cut into the paragraphs the tidied text has, where they differ.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shaped: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub times: Option<Vec<Times>>,
}

/// When the words under one heading were said.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Times {
    /// Where the last of them end, in milliseconds from the start.
    pub end_ms: u64,
    /// How long the text was when it was marked, in characters.
    pub chars: u32,
    /// Places in the text and when the words there began.
    pub marks: Vec<(u32, u64)>,
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
    pub folder: String,
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

/// Puts a meeting in a folder, or in none where `folder` is empty.
fn set_folder_in(dir: &Path, id: &str, folder: &str) -> Result<(), String> {
    let mut meeting = get_in(dir, id)?;
    meeting.folder = folder.trim().to_string();
    save_in(dir, &meeting)
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
            folder: meeting.folder.trim().to_string(),
            duration_s: meeting.duration_s,
            id: meeting.id,
            started_at: meeting.started_at,
        })
        .collect();
    all.sort_by(|a, b| b.started_at.cmp(&a.started_at).then(b.id.cmp(&a.id)));
    all
}

/// A meeting that holds what was looked for, with the place it was found in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Found {
    pub id: String,
    pub started_at: u64,
    pub title: String,
    pub duration_s: u64,
    /// The words around the first place found, on one line.
    pub found: String,
}

/// How much is shown on either side of a find, in characters.
const AROUND: usize = 40;

/// The text around the first `word` in `text`, on one line. Empty when it is not there.
fn around(text: &str, word: &str) -> String {
    let line: Vec<char> = text
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .collect();
    let lower: Vec<char> = line
        .iter()
        .flat_map(|each| each.to_lowercase().next())
        .collect();
    let word: Vec<char> = word
        .chars()
        .flat_map(|each| each.to_lowercase().next())
        .collect();
    if word.is_empty() || lower.len() != line.len() {
        return String::new();
    }
    let Some(at) = lower
        .windows(word.len())
        .position(|window| window == word.as_slice())
    else {
        return String::new();
    };
    let from = at.saturating_sub(AROUND);
    let to = (at + word.len() + AROUND).min(line.len());
    let mut shown: String = line[from..to].iter().collect();
    if from > 0 {
        shown.insert(0, '…');
    }
    if to < line.len() {
        shown.push('…');
    }
    shown
}

/// The meetings that hold every word of `query`, in their text, notes, minutes or title,
/// the newest first. Capital and small letters count as the same.
fn search_in(dir: &Path, query: &str) -> Vec<Found> {
    let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
    if words.is_empty() {
        return Vec::new();
    }
    let mut all: Vec<Found> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|kind| kind == "json"))
        .filter_map(|path| std::fs::read(path).ok())
        .filter_map(|raw| serde_json::from_slice::<Meeting>(&raw).ok())
        .filter_map(|meeting| {
            let parts = [
                &meeting.minutes,
                &meeting.notes,
                &meeting.transcript,
                &meeting.title,
            ];
            let whole = parts.map(|part| part.to_lowercase()).join("\n");
            if !words.iter().all(|word| whole.contains(word.as_str())) {
                return None;
            }
            let found = parts
                .iter()
                .map(|part| around(part, &words[0]))
                .find(|shown| !shown.is_empty())
                .unwrap_or_default();
            Some(Found {
                found,
                title: meeting.title.trim().to_string(),
                duration_s: meeting.duration_s,
                id: meeting.id,
                started_at: meeting.started_at,
            })
        })
        .collect();
    all.sort_by(|a, b| b.started_at.cmp(&a.started_at).then(b.id.cmp(&a.id)));
    all
}

/// How much of one meeting is read when a question is asked of many, in characters.
const MOST_READ: usize = 4_000;

/// The newest `most` meetings laid out to be read together, each under a heading with its
/// name and day: its minutes where it has them, and otherwise the beginning of what was said.
fn reading_in(dir: &Path, most: usize) -> String {
    let mut all: Vec<Meeting> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|kind| kind == "json"))
        .filter_map(|path| std::fs::read(path).ok())
        .filter_map(|raw| serde_json::from_slice::<Meeting>(&raw).ok())
        .collect();
    all.sort_by(|a, b| b.started_at.cmp(&a.started_at).then(b.id.cmp(&a.id)));
    all.iter()
        .take(most)
        .map(|meeting| {
            let day = chrono::DateTime::from_timestamp_millis(meeting.started_at as i64)
                .map(|when| {
                    when.with_timezone(&chrono::Local)
                        .format("%Y-%m-%d")
                        .to_string()
                })
                .unwrap_or_default();
            let name = match meeting.title.trim() {
                "" => "Meeting",
                title => title,
            };
            let source = if meeting.minutes.trim().is_empty() {
                &meeting.transcript
            } else {
                &meeting.minutes
            };
            // On one paragraph, so that a meeting is not cut away from its heading.
            let said: String = source
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
                .chars()
                .take(MOST_READ)
                .collect();
            format!("=== {name}, {day} ===\n{said}")
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

pub fn reading(most: usize) -> String {
    dir().map(|dir| reading_in(&dir, most)).unwrap_or_default()
}

pub fn search(query: &str) -> Vec<Found> {
    dir().map(|dir| search_in(&dir, query)).unwrap_or_default()
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

pub fn set_folder(id: &str, folder: &str) -> Result<(), String> {
    set_folder_in(&dir()?, id, folder)
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

    #[test]
    fn a_meeting_keeps_the_terms_it_was_unsure_of_and_reads_older_ones_without_them() {
        let older: Meeting =
            serde_json::from_str(r#"{"id":"m-1","started_at":1,"transcript":"a"}"#).unwrap();
        assert!(older.terms.is_empty());
        let meeting = Meeting {
            terms: vec![crate::minutes::Term {
                spelling: "Tauri".into(),
                others: vec!["Tori".into()],
            }],
            unpolished: Some(Unpolished {
                text: "we use tori".into(),
                ..Default::default()
            }),
            ..older
        };
        let again: Meeting =
            serde_json::from_str(&serde_json::to_string(&meeting).unwrap()).unwrap();
        assert_eq!(again, meeting);
    }

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
    fn a_meeting_keeps_when_its_words_were_said_and_one_without_that_still_opens() {
        let dir = scratch("times");
        let mut timed = meeting("m1", 10, "Speaker 1  00:00\nwe ship on Friday");
        timed.times = vec![Times {
            end_ms: 8000,
            chars: 17,
            marks: vec![(0, 0), (8, 5000)],
        }];
        save_in(&dir, &timed).unwrap();
        assert_eq!(get_in(&dir, "m1").unwrap().times, timed.times);

        std::fs::write(
            dir.join("m0.json"),
            r#"{"id":"m0","started_at":5,"transcript":"older"}"#,
        )
        .unwrap();
        assert!(get_in(&dir, "m0").unwrap().times.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn meetings_are_found_by_words_in_their_text_notes_minutes_or_title() {
        let dir = scratch("search");
        save_in(
            &dir,
            &meeting("m1", 10, "说话人 1  00:00\n我们讨论了预算，要砍掉百分之十"),
        )
        .unwrap();
        let mut second = meeting("m2", 20, "nothing of the kind");
        second.minutes = "## Decisions\nThe BUDGET is cut".into();
        second.title = "Weekly".into();
        save_in(&dir, &second).unwrap();
        save_in(&dir, &meeting("m3", 30, "something else")).unwrap();

        let found = search_in(&dir, " budget ");
        assert_eq!(found.len(), 1);
        assert_eq!(
            (found[0].id.as_str(), found[0].title.as_str()),
            ("m2", "Weekly")
        );
        assert_eq!(found[0].found, "## Decisions The BUDGET is cut");

        let found = search_in(&dir, "预算");
        assert_eq!(found[0].id, "m1");
        assert!(found[0].found.contains("讨论了预算，要砍掉"));

        // Every word asked for must be there, wherever in the meeting.
        assert_eq!(search_in(&dir, "weekly budget").len(), 1);
        assert!(search_in(&dir, "weekly 预算").is_empty());
        assert!(search_in(&dir, "  ").is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_newest_meetings_are_laid_out_to_be_read_together() {
        let dir = scratch("reading");
        let mut first = meeting("m1", 1_700_000_000_000, &"长".repeat(9_000));
        first.title = "预算".into();
        save_in(&dir, &first).unwrap();
        let mut second = meeting("m2", 1_700_086_400_000, "what was said");
        second.minutes = "## Decisions\nShip on Friday".into();
        save_in(&dir, &second).unwrap();
        save_in(&dir, &meeting("m0", 1_600_000_000_000, "long ago")).unwrap();

        let read = reading_in(&dir, 2);
        let (newest, older) = read.split_once("\n\n=== ").unwrap();
        assert!(newest.starts_with("=== Meeting, 2023-11-"));
        // Minutes say more in less than the words they were written from.
        assert!(newest.ends_with("===\n## Decisions Ship on Friday"));
        assert!(older.starts_with("预算, 2023-11-"));
        assert!(older.chars().count() < MOST_READ + 100);
        assert!(!read.contains("long ago"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn what_is_shown_of_a_find_is_cut_between_characters() {
        let text = format!("{}预算{}", "开".repeat(200), "会".repeat(200));
        let shown = around(&text, "预算");
        assert!(shown.starts_with('…') && shown.ends_with('…'));
        assert!(shown.contains("预算"));
        assert!(shown.chars().count() <= 2 * AROUND + 4);
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
    fn a_meeting_is_put_in_a_folder_and_the_list_says_which() {
        let dir = std::env::temp_dir().join(format!("ot-meetings-folder-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let meeting = Meeting {
            id: "m-1".into(),
            started_at: 1,
            transcript: "hello".into(),
            ..Default::default()
        };
        save_in(&dir, &meeting).unwrap();
        assert_eq!(list_in(&dir)[0].folder, "");
        set_folder_in(&dir, "m-1", "  Clients ").unwrap();
        assert_eq!(list_in(&dir)[0].folder, "Clients");
        assert_eq!(get_in(&dir, "m-1").unwrap().transcript, "hello");
        set_folder_in(&dir, "m-1", "").unwrap();
        assert_eq!(list_in(&dir)[0].folder, "");
        assert!(set_folder_in(&dir, "m-none", "x").is_err());
        let _ = std::fs::remove_dir_all(&dir);
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
