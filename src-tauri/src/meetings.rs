//! Past meetings: the text, the user's notes, the minutes and the speakers' names of each,
//! one file per meeting in the app's data folder.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

/// How much of a meeting's text the list shows.
const PREVIEW_CHARS: usize = 80;

/// A heading of a transcript: `<label> <number>  <time>` for a numbered speaker,
/// `<label>  <time>` for the first speaker, or the time alone where speakers are not told
/// apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Heading<'a> {
    /// What speakers are called; empty for a time alone.
    pub label: &'a str,
    /// The speaker's number; None where the label stands alone or there is no speaker.
    pub number: Option<u32>,
    pub time: &'a str,
}

/// Whether `text` is a time as a transcript writes it: minutes and seconds, with hours in
/// front where there are any.
fn is_time(text: &str) -> bool {
    let parts: Vec<&str> = text.split(':').collect();
    (2..=3).contains(&parts.len())
        && parts.iter().all(|part| {
            !part.is_empty() && part.len() <= 2 && part.chars().all(|c| c.is_ascii_digit())
        })
}

/// The heading `line` is; None for a line of what was said.
pub fn heading(line: &str) -> Option<Heading<'_>> {
    let line = line.trim_end();
    if is_time(line.trim_start()) {
        return Some(Heading {
            label: "",
            number: None,
            time: line.trim_start(),
        });
    }
    let (who, time) = line.rsplit_once("  ")?;
    let (who, time) = (who.trim(), time.trim());
    if !is_time(time) || who.is_empty() || who.chars().count() > 40 {
        return None;
    }
    match who.rsplit_once(' ') {
        Some((label, number)) if number.chars().all(|c| c.is_ascii_digit()) => {
            let label = label.trim_end();
            (!label.is_empty()).then_some(Heading {
                label,
                number: Some(number.parse().ok()?),
                time,
            })
        }
        _ => Some(Heading {
            label: who,
            number: None,
            time,
        }),
    }
}

/// The seconds from the start a transcript's time stands for.
pub fn seconds_of(time: &str) -> Option<u64> {
    if !is_time(time) {
        return None;
    }
    time.split(':').try_fold(0_u64, |total, part| {
        Some(total * 60 + part.parse::<u64>().ok()?)
    })
}

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
    /// The people the user said took part, as their names are to be written.
    #[serde(default)]
    pub people: Vec<String>,
    /// The questions asked of it and their answers, the newest first.
    #[serde(default)]
    pub answers: Vec<Answer>,
    /// Its chapters and what each speaker said, once they were asked for.
    #[serde(default)]
    pub overview: Option<crate::overview::Overview>,
}

/// A question asked of a meeting, with the answer it got and when, in milliseconds since
/// 1970.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Answer {
    pub question: String,
    pub answer: String,
    pub at: u64,
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
    /// Whether the words were only put right as the meeting went on, not tidied up.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub live: bool,
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

pub(crate) fn get_in(dir: &Path, id: &str) -> Result<Meeting, String> {
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
pub(crate) fn list_in(dir: &Path) -> Vec<Summary> {
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
pub(crate) fn search_in(dir: &Path, query: &str) -> Vec<Found> {
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

/// Held while a meeting's sound file is written, moved or taken away, so that keeping,
/// packing, cutting and deleting it never meet halfway.
static AUDIO: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn audio_lock() -> std::sync::MutexGuard<'static, ()> {
    AUDIO
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Where a meeting's sound is kept, beside its text, as the recording was made.
fn audio_file(dir: &Path, id: &str) -> Result<PathBuf, String> {
    Ok(file_of(dir, id)?.with_extension("wav"))
}

/// Where a meeting's sound is kept once packed: the same samples in about half the room.
fn packed_file(dir: &Path, id: &str) -> Result<PathBuf, String> {
    Ok(file_of(dir, id)?.with_extension("flac"))
}

/// A packed file still being written, taken away if it is ever left behind.
fn packing_file(dir: &Path, id: &str) -> Result<PathBuf, String> {
    Ok(file_of(dir, id)?.with_extension("flac.part"))
}

/// The sound as it was before its last cut, as `ext` (wav or flac), kept aside until the
/// cut can no longer be undone.
fn before_cut_file(dir: &Path, id: &str, ext: &str) -> Result<PathBuf, String> {
    Ok(file_of(dir, id)?.with_extension(format!("{ext}.before-cut")))
}

fn drop_before_cut(dir: &Path, id: &str) -> Result<(), String> {
    remove_if_there(&before_cut_file(dir, id, "wav")?)?;
    remove_if_there(&before_cut_file(dir, id, "flac")?)
}

/// Puts back the sound a meeting had before its last cut. False where none is kept.
fn undo_cut_in(dir: &Path, id: &str) -> Result<bool, String> {
    let _held = audio_lock();
    for ext in ["flac", "wav"] {
        let before = before_cut_file(dir, id, ext)?;
        if !before.is_file() {
            continue;
        }
        // The cut sound goes, in whatever form it was packed into since.
        remove_if_there(&packing_file(dir, id)?)?;
        remove_if_there(&packed_file(dir, id)?)?;
        remove_if_there(&audio_file(dir, id)?)?;
        let back = file_of(dir, id)?.with_extension(ext);
        std::fs::rename(&before, &back).map_err(|error| error.to_string())?;
        drop_before_cut(dir, id)?;
        return Ok(true);
    }
    Ok(false)
}

/// Lets the sound from before a meeting's last cut go: the cut stands.
fn keep_cut_in(dir: &Path, id: &str) -> Result<(), String> {
    let _held = audio_lock();
    drop_before_cut(dir, id)
}

/// Lets go of every sound kept aside for an undo that can no longer be asked for, as when
/// the app last ended within the time to undo.
fn drop_cuts_in(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let _held = audio_lock();
    for entry in entries.flatten() {
        if entry.file_name().to_string_lossy().ends_with(".before-cut") {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

fn audio_in(dir: &Path, id: &str) -> Option<PathBuf> {
    let packed = packed_file(dir, id).ok().filter(|path| path.is_file());
    packed.or_else(|| audio_file(dir, id).ok().filter(|path| path.is_file()))
}

/// Moves a finished recording to the meeting it belongs to.
fn keep_audio_in(dir: &Path, id: &str, recording: &Path) -> Result<PathBuf, String> {
    let _held = audio_lock();
    let kept = audio_file(dir, id)?;
    std::fs::create_dir_all(dir).map_err(|error| error.to_string())?;
    // A packed sound from before is not this recording's; it goes first, so that a WAV
    // beside a packed file is only ever one whose packing was not finished.
    remove_if_there(&packed_file(dir, id)?)?;
    std::fs::rename(recording, &kept).map_err(|error| error.to_string())?;
    Ok(kept)
}

fn remove_if_there(path: &Path) -> Result<(), String> {
    match std::fs::remove_file(path) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error.to_string()),
        _ => Ok(()),
    }
}

fn delete_audio_in(dir: &Path, id: &str) -> Result<(), String> {
    let _held = audio_lock();
    drop_before_cut(dir, id)?;
    remove_if_there(&packing_file(dir, id)?)?;
    remove_if_there(&packed_file(dir, id)?)?;
    remove_if_there(&audio_file(dir, id)?)
}

/// Opens a meeting's WAV sound past its header, checking that it is the app's own
/// recording: 16 kHz, one channel, 16 bits.
fn open_wav(path: &Path, write: bool) -> Result<std::fs::File, String> {
    use std::io::Read;
    let mut file = std::fs::OpenOptions::new()
        .read(true)
        .write(write)
        .open(path)
        .map_err(|error| error.to_string())?;
    let mut header = [0_u8; 44];
    file.read_exact(&mut header)
        .map_err(|_| "audio_unknown".to_string())?;
    let ours = crate::meeting_record::wav_header(0);
    // Bytes 4 to 8 hold the file's length, which differs from file to file.
    if header[..4] != ours[..4] || header[8..40] != ours[8..40] {
        return Err("audio_unknown".to_string());
    }
    Ok(file)
}

/// Packs a meeting's WAV sound as FLAC, reads the packed file back whole, and only then
/// lets the WAV go. Returns the packed file, or None where there was no WAV to pack. A WAV
/// that is not the app's own recording, or holds no sound, is left as it is.
fn pack_audio_in(dir: &Path, id: &str) -> Result<Option<PathBuf>, String> {
    let _held = audio_lock();
    let wav = audio_file(dir, id)?;
    let packed = packed_file(dir, id)?;
    if !wav.is_file() {
        return Ok(packed.is_file().then_some(packed));
    }
    // The WAV of a packed file, left when letting it go failed: the packed file may since
    // have been cut, and stays as it is.
    if packed.is_file() {
        remove_if_there(&wav)?;
        return Ok(Some(packed));
    }
    let packing = packing_file(dir, id)?;
    let mut body = std::io::BufReader::new(open_wav(&wav, false)?);
    if let Err(error) = crate::flac::from_wav(&mut body, &packing) {
        let _ = std::fs::remove_file(&packing);
        return Err(error);
    }
    drop(body);
    std::fs::rename(&packing, &packed).map_err(|error| error.to_string())?;
    remove_if_there(&wav)?;
    Ok(Some(packed))
}

/// Packs every meeting's sound still kept as WAV, one at a time, and tells `packed` of
/// each. Waits while a meeting is being recorded, and leaves the rest for another time
/// where it goes on.
fn pack_all_in(dir: &Path, mut packed: impl FnMut(&str, &Path)) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut names: Vec<String> = entries
        .filter_map(|entry| entry.ok()?.file_name().into_string().ok())
        .collect();
    names.sort();
    for name in &names {
        // A packed file left half written when the app last ended.
        if let Some(id) = name.strip_suffix(".flac.part") {
            if let Ok(path) = packing_file(dir, id) {
                let _held = audio_lock();
                let _ = std::fs::remove_file(path);
            }
        }
    }
    for name in &names {
        let Some(id) = name.strip_suffix(".wav") else {
            continue;
        };
        if !file_of(dir, id).is_ok_and(|text| text.is_file()) {
            continue;
        }
        if crate::meeting_record::is_recording() {
            return;
        }
        match pack_audio_in(dir, id) {
            Ok(Some(path)) => packed(id, &path),
            Ok(None) => {}
            Err(error) => crate::applog::note(
                "meeting",
                &format!("the sound of a meeting could not be packed: {error}"),
            ),
        }
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

pub fn trim_audio(id: &str, end_ms: u64) -> Result<(), String> {
    trim_audio_in(&dir()?, id, end_ms)
}

pub fn undo_cut(id: &str) -> Result<bool, String> {
    undo_cut_in(&dir()?, id)
}

pub fn keep_cut(id: &str) -> Result<(), String> {
    keep_cut_in(&dir()?, id)
}

pub fn drop_cuts() {
    if let Ok(dir) = dir() {
        drop_cuts_in(&dir);
    }
}

pub fn pack_audio(id: &str) -> Result<Option<PathBuf>, String> {
    pack_audio_in(&dir()?, id)
}

pub fn pack_all(packed: impl FnMut(&str, &Path)) {
    if let Ok(dir) = dir() {
        pack_all_in(&dir, packed);
    }
}

/// Cuts a meeting's sound off at `end_ms` from its start. The sound is the app's own
/// recording, as WAV or packed; any other file is left as it is. The sound from before is
/// kept aside, for the cut to be undone, until `keep_cut` or the next cut.
fn trim_audio_in(dir: &Path, id: &str, end_ms: u64) -> Result<(), String> {
    use std::io::{Seek, SeekFrom, Write};
    let _held = audio_lock();
    // Sixteen samples a millisecond.
    let keep_samples = end_ms.saturating_mul(16);
    let packed = packed_file(dir, id)?;
    if packed.is_file() {
        let packing = packing_file(dir, id)?;
        let cut = (|| {
            let mut encoder = crate::flac::Encoder::create(&packing)?;
            let mut left = keep_samples;
            crate::flac::read(&packed, |block| {
                let take = block.len().min(usize::try_from(left).unwrap_or(usize::MAX));
                encoder.push(&block[..take])?;
                left -= take as u64;
                Ok(left > 0)
            })?;
            let written = encoder.finish()?;
            if written == 0 {
                return Err("audio_empty".to_string());
            }
            let back = crate::flac::read(&packing, |_| Ok(true))?;
            if back != written {
                return Err("audio_unreadable".to_string());
            }
            drop_before_cut(dir, id)?;
            let before = before_cut_file(dir, id, "flac")?;
            std::fs::rename(&packed, &before).map_err(|error| error.to_string())?;
            std::fs::rename(&packing, &packed).map_err(|error| {
                // The sound stays as it was where the cut one cannot take its place.
                let _ = std::fs::rename(&before, &packed);
                error.to_string()
            })
        })();
        if cut.is_err() {
            let _ = std::fs::remove_file(&packing);
        }
        return cut;
    }
    let path = audio_file(dir, id)?;
    let mut file = open_wav(&path, true)?;
    drop_before_cut(dir, id)?;
    std::fs::copy(&path, before_cut_file(dir, id, "wav")?).map_err(|error| error.to_string())?;
    let length = file.metadata().map_err(|error| error.to_string())?.len();
    let held = length.saturating_sub(44);
    let kept = keep_samples.saturating_mul(2).min(held) & !1;
    let kept_u32 = u32::try_from(kept).map_err(|_| "audio_too_long".to_string())?;
    file.set_len(44 + kept).map_err(|error| error.to_string())?;
    file.seek(SeekFrom::Start(0))
        .and_then(|_| file.write_all(&crate::meeting_record::wav_header(kept_u32)))
        .and_then(|_| file.sync_all())
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cut_can_be_undone_until_it_is_kept_packed_or_not() {
        let dir = std::env::temp_dir().join(format!("undo-cut-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(file_of(&dir, "m-1").unwrap(), "{}").unwrap();

        // Cut as WAV, packed in the meantime, then undone: the whole WAV is back.
        let sound = recording(&dir, "m-1", 2 * 16_000);
        let whole = std::fs::read(audio_file(&dir, "m-1").unwrap()).unwrap();
        trim_audio_in(&dir, "m-1", 500).unwrap();
        pack_audio_in(&dir, "m-1").unwrap();
        assert!(undo_cut_in(&dir, "m-1").unwrap());
        assert_eq!(
            std::fs::read(audio_file(&dir, "m-1").unwrap()).unwrap(),
            whole
        );
        assert!(!packed_file(&dir, "m-1").unwrap().exists());
        assert!(!undo_cut_in(&dir, "m-1").unwrap());

        // Cut when packed, then undone: every sample is back.
        let packed = pack_audio_in(&dir, "m-1").unwrap().unwrap();
        trim_audio_in(&dir, "m-1", 500).unwrap();
        assert_eq!(samples_of(&packed).len(), 8_000);
        assert!(undo_cut_in(&dir, "m-1").unwrap());
        assert_eq!(samples_of(&packed), sound);

        // Kept: nothing is left aside, and there is nothing to undo.
        trim_audio_in(&dir, "m-1", 500).unwrap();
        keep_cut_in(&dir, "m-1").unwrap();
        assert!(!undo_cut_in(&dir, "m-1").unwrap());
        assert_eq!(samples_of(&packed).len(), 8_000);

        // Left aside when the app ended, or when the meeting goes: let go.
        trim_audio_in(&dir, "m-1", 250).unwrap();
        assert!(before_cut_file(&dir, "m-1", "flac").unwrap().exists());
        drop_cuts_in(&dir);
        assert!(!before_cut_file(&dir, "m-1", "flac").unwrap().exists());
        trim_audio_in(&dir, "m-1", 100).unwrap();
        delete_in(&dir, "m-1").unwrap();
        let left: Vec<_> = std::fs::read_dir(&dir).unwrap().flatten().collect();
        assert!(left.is_empty(), "{left:?}");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    fn recording(dir: &Path, id: &str, samples: usize) -> Vec<i16> {
        let sound: Vec<i16> = (0..samples)
            .map(|at| ((at * 37) % 2000) as i16 - 1000)
            .collect();
        let body: Vec<u8> = sound.iter().flat_map(|s| s.to_le_bytes()).collect();
        let mut bytes = crate::meeting_record::wav_header(body.len() as u32).to_vec();
        bytes.extend(&body);
        std::fs::write(audio_file(dir, id).unwrap(), bytes).unwrap();
        sound
    }

    fn samples_of(path: &Path) -> Vec<i16> {
        let mut all = Vec::new();
        crate::flac::read(path, |block| {
            all.extend_from_slice(block);
            Ok(true)
        })
        .unwrap();
        all
    }

    #[test]
    fn a_meetings_sound_is_packed_without_losing_a_sample_and_cut_when_packed() {
        let dir = std::env::temp_dir().join(format!("pack-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let sound = recording(&dir, "m-1", 3 * 16_000 + 5);

        let packed = pack_audio_in(&dir, "m-1").unwrap().unwrap();
        assert_eq!(packed, dir.join("m-1.flac"));
        assert!(!audio_file(&dir, "m-1").unwrap().exists());
        assert_eq!(audio_in(&dir, "m-1"), Some(packed.clone()));
        assert_eq!(samples_of(&packed), sound);
        // Packing again finds nothing more to do.
        assert_eq!(pack_audio_in(&dir, "m-1").unwrap(), Some(packed.clone()));

        trim_audio_in(&dir, "m-1", 1_250).unwrap();
        assert_eq!(samples_of(&packed), sound[..20_000]);
        // A WAV left beside the packed file does not undo the cut.
        recording(&dir, "m-1", 3 * 16_000);
        assert_eq!(pack_audio_in(&dir, "m-1").unwrap(), Some(packed.clone()));
        assert!(!audio_file(&dir, "m-1").unwrap().exists());
        assert_eq!(samples_of(&packed), sound[..20_000]);
        assert!(!packing_file(&dir, "m-1").unwrap().exists());
        // Nothing would be left.
        assert!(trim_audio_in(&dir, "m-1", 0).is_err());
        assert_eq!(samples_of(&packed).len(), 20_000);

        // A new recording kept for the meeting replaces the packed sound.
        let fresh = dir.join("fresh.wav");
        std::fs::write(&fresh, crate::meeting_record::wav_header(0)).unwrap();
        keep_audio_in(&dir, "m-1", &fresh).unwrap();
        assert!(!packed.exists());
        // One with no sound in it stays as it was made.
        assert!(pack_audio_in(&dir, "m-1").is_err());
        assert_eq!(
            audio_in(&dir, "m-1"),
            Some(audio_file(&dir, "m-1").unwrap())
        );

        delete_audio_in(&dir, "m-1").unwrap();
        assert_eq!(audio_in(&dir, "m-1"), None);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_sound_of_every_meeting_kept_as_wav_is_packed_once() {
        let dir = std::env::temp_dir().join(format!("pack-all-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let first = recording(&dir, "m-1", 16_000);
        std::fs::write(file_of(&dir, "m-1").unwrap(), "{}").unwrap();
        recording(&dir, "m-2", 16_000);
        std::fs::write(file_of(&dir, "m-2").unwrap(), "{}").unwrap();
        // A sound with no meeting is not the app's to touch.
        recording(&dir, "stray", 16_000);
        // A packed file left half written.
        std::fs::write(packing_file(&dir, "m-9").unwrap(), b"half").unwrap();

        let mut told = Vec::new();
        pack_all_in(&dir, |id, path| {
            told.push((id.to_string(), path.to_path_buf()))
        });
        assert_eq!(
            told,
            vec![
                ("m-1".to_string(), dir.join("m-1.flac")),
                ("m-2".to_string(), dir.join("m-2.flac")),
            ]
        );
        assert_eq!(samples_of(&dir.join("m-1.flac")), first);
        assert!(dir.join("stray.wav").exists());
        assert!(!packing_file(&dir, "m-9").unwrap().exists());
        told.clear();
        pack_all_in(&dir, |id, _| told.push((id.to_string(), PathBuf::new())));
        assert!(told.is_empty());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_meetings_sound_is_cut_off_at_a_time() {
        let dir = std::env::temp_dir().join(format!("trim-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = audio_file(&dir, "m-1").unwrap();
        // Three seconds of sound.
        let data = vec![7_u8; 3 * 32_000];
        let mut bytes = crate::meeting_record::wav_header(data.len() as u32).to_vec();
        bytes.extend(&data);
        std::fs::write(&path, &bytes).unwrap();

        trim_audio_in(&dir, "m-1", 1_250).unwrap();
        let cut = std::fs::read(&path).unwrap();
        assert_eq!(cut.len(), 44 + 40_000);
        assert_eq!(&cut[..44], &crate::meeting_record::wav_header(40_000));
        // A time past the end keeps it all.
        trim_audio_in(&dir, "m-1", 60_000).unwrap();
        assert_eq!(std::fs::read(&path).unwrap().len(), 44 + 40_000);

        // A file that is not the app's own recording is left alone.
        std::fs::write(&path, b"not a recording at all, just some text here....").unwrap();
        assert_eq!(
            trim_audio_in(&dir, "m-1", 10),
            Err("audio_unknown".to_string())
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn headings_come_in_three_shapes() {
        let numbered = super::heading("Speaker 2  01:05").unwrap();
        assert_eq!(
            (numbered.label, numbered.number, numbered.time),
            ("Speaker", Some(2), "01:05")
        );
        let first = super::heading("说话人  1:02:03").unwrap();
        assert_eq!(
            (first.label, first.number, first.time),
            ("说话人", None, "1:02:03")
        );
        let alone = super::heading("00:42").unwrap();
        assert_eq!((alone.label, alone.number), ("", None));
        assert!(super::heading("We met at 10  10:30 sharp").is_none());
        assert!(super::heading("The roses need water.").is_none());
        assert_eq!(super::seconds_of("1:02:03"), Some(3723));
        assert_eq!(super::seconds_of("05:55"), Some(355));
        assert_eq!(super::seconds_of("5"), None);
    }

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
