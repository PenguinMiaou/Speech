//! Records a meeting from the microphone and writes it out while it runs: the audio is
//! cut into windows at pauses, each window is recognized and then dropped, so only the
//! part not yet recognized is held in memory. The whole recording goes to one file on
//! disk, kept until the next recording starts or the user deletes it.

use std::io::{Seek, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use crate::audio::capture::{AudioCaptureHandle, AudioConfig};

use crate::speakers::SpeakerTurn;

const RATE: usize = 16_000;
/// The most audio that may wait to be recognized, should recognition fall behind.
const LONGEST_SAMPLES: usize = 3 * 60 * 60 * RATE;
/// A window is cut at the first pause after the shorter length, so that words settle
/// soon after they are said, and at the quietest moment before the longer one when the
/// talk never pauses.
const EARLIEST_CUT: usize = 6 * RATE;
const WINDOW: usize = 20 * RATE;
/// A pause is this many quiet steps in a row.
const PAUSE_STEPS: usize = 2;
/// A step is quiet when it carries less than this share of a loud step's energy.
const QUIET_SHARE: u64 = 16;
/// After this many windows in a row that could not be recognized even when tried again,
/// recognition gives up. The sound is still kept and can be written out afterwards.
const MOST_LOST_WINDOWS: usize = 3;
/// How much of what was just said is given to the recognizer as what came before.
const CONTEXT_CHARS: usize = 200;
/// The most words the recognizer is given.
const MOST_WORDS: usize = 30;
/// The quietest moment is looked for in steps of a fifth of a second.
const STEP: usize = RATE / 5;
/// A last window shorter than this holds no word.
const SHORTEST_WINDOW: usize = RATE / 2;
/// A window whose loudest sample is below this is silence.
const SILENT_PEAK: u16 = 100;

/// One paragraph of the meeting. `speaker` is absent when speakers are not told apart,
/// and 0 for a voice too short to tell.
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct Segment {
    pub speaker: Option<u32>,
    pub start_ms: u64,
    pub text: String,
    /// Where the words end. Zero when that is not known.
    #[serde(default)]
    pub end_ms: u64,
    /// Whether the words came in by the microphone and not out of the computer: the
    /// user's own side of a call. Absent when the computer's sound is not recorded, or
    /// when it cannot be told.
    #[serde(default)]
    pub mine: Option<bool>,
}

#[derive(Default)]
struct Waiting {
    /// Recorded and not yet recognized.
    pending: Vec<i16>,
    /// The computer's own sound in what waits, sample for sample; empty when it is not
    /// recorded. What waits less this is what the microphone alone heard.
    theirs: Vec<i16>,
    /// Samples already handed to recognition.
    taken: usize,
    finishing: bool,
    dropped: bool,
    /// While set, what the microphone hears is neither kept nor written out.
    paused: bool,
    /// Set when the recording pauses: what waits is recognized without waiting for more.
    settle: bool,
}

static DATA_DIR: OnceLock<PathBuf> = OnceLock::new();

pub fn init(data_dir: &Path) {
    let _ = DATA_DIR.set(data_dir.to_path_buf());
}

const RECORDING: &str = "meeting-recording.wav";
/// What was said in the recording still running, one paragraph to a line. It is there
/// only while a recording runs, so that finding it at startup means one was cut short.
const JOURNAL: &str = "meeting-recording.jsonl";
/// A recording cut short before this many seconds is not worth bringing back.
const SHORTEST_RECOVERED: u64 = 2;

/// Where the latest meeting's audio is kept.
fn recording_path() -> Option<PathBuf> {
    DATA_DIR.get().map(|dir| dir.join(RECORDING))
}

/// Writes one more paragraph of the running recording down on disk.
fn write_down(dir: &Path, segment: &Segment) {
    let Ok(mut line) = serde_json::to_string(segment) else {
        return;
    };
    line.push('\n');
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join(JOURNAL))
    {
        let _ = file.write_all(line.as_bytes());
    }
}

/// A recording that the app did not live to end: what was written down of it, and how
/// long its sound is.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Recovered {
    pub segments: Vec<Segment>,
    pub seconds: u64,
}

/// Finds a recording that was cut short, for instance by a crash or a power cut: its
/// sound is made a whole file, and what was written down of it is handed back, once.
pub fn recover() -> Option<Recovered> {
    if is_recording() {
        return None;
    }
    let found = recover_in(DATA_DIR.get()?)?;
    crate::applog::note(
        "meeting",
        &format!(
            "a recording cut short was found again: {} s, {} paragraphs",
            found.seconds,
            found.segments.len()
        ),
    );
    Some(found)
}

fn recover_in(dir: &Path) -> Option<Recovered> {
    let journal = dir.join(JOURNAL);
    let written = std::fs::read_to_string(&journal).ok()?;
    let _ = std::fs::remove_file(&journal);
    let recording = dir.join(RECORDING);
    finish_wav(&recording).ok()?;
    let seconds = std::fs::metadata(&recording).ok()?.len().saturating_sub(44) / (RATE as u64 * 2);
    if seconds < SHORTEST_RECOVERED {
        return None;
    }
    // A last line the app did not finish writing is left out.
    let segments = written
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect();
    Some(Recovered { segments, seconds })
}

/// The latest meeting's audio, if it is still on disk.
pub fn kept_recording() -> Option<PathBuf> {
    recording_path().filter(|path| path.is_file())
}

pub fn delete_recording() {
    if let Some(path) = recording_path() {
        let _ = std::fs::remove_file(path);
    }
}

/// The header of a 16 kHz, one-channel, 16-bit WAV file holding `data_bytes` of audio.
pub fn wav_header(data_bytes: u32) -> [u8; 44] {
    let mut header = [0u8; 44];
    header[0..4].copy_from_slice(b"RIFF");
    header[4..8].copy_from_slice(&(36 + data_bytes).to_le_bytes());
    header[8..16].copy_from_slice(b"WAVEfmt ");
    header[16..20].copy_from_slice(&16u32.to_le_bytes());
    header[20..22].copy_from_slice(&1u16.to_le_bytes());
    header[22..24].copy_from_slice(&1u16.to_le_bytes());
    header[24..28].copy_from_slice(&(RATE as u32).to_le_bytes());
    header[28..32].copy_from_slice(&(RATE as u32 * 2).to_le_bytes());
    header[32..34].copy_from_slice(&2u16.to_le_bytes());
    header[34..36].copy_from_slice(&16u16.to_le_bytes());
    header[36..40].copy_from_slice(b"data");
    header[40..44].copy_from_slice(&data_bytes.to_le_bytes());
    header
}

/// Writes the real length into the header once the recording has ended.
fn finish_wav(path: &Path) -> std::io::Result<()> {
    let mut file = std::fs::OpenOptions::new().write(true).open(path)?;
    let data = file
        .metadata()?
        .len()
        .saturating_sub(44)
        .min(u32::MAX as u64 - 36);
    file.seek(std::io::SeekFrom::Start(0))?;
    file.write_all(&wav_header(data as u32))
}

/// The most of the computer's own sound that waits for the microphone to catch up.
const SYSTEM_BACKLOG: usize = 2 * RATE;

/// The microphone's bytes with the computer's own sound added, sample by sample, and the
/// computer's part of each sample beside the sum. The microphone keeps the time: the
/// computer's sound only arrives while something plays. With `echo`, a stretch in which
/// the microphone heard nothing but the loudspeakers is turned down first.
fn mixed_with(
    microphone: &[u8],
    system: &mut std::collections::VecDeque<i16>,
    echo: Option<&mut Echo>,
) -> (Vec<u8>, Vec<i16>) {
    let mut own: Vec<i16> = microphone
        .chunks_exact(2)
        .map(|pair| i16::from_le_bytes([pair[0], pair[1]]))
        .collect();
    let theirs: Vec<i16> = own
        .iter()
        .map(|_| system.pop_front().unwrap_or(0))
        .collect();
    if let Some(echo) = echo {
        echo.quieten(&mut own, &theirs);
    }
    let mixed = own
        .iter()
        .zip(&theirs)
        .flat_map(|(own, other)| own.saturating_add(*other).to_le_bytes())
        .collect();
    (mixed, theirs)
}

/// How many pieces of the microphone are looked back on: half a second.
const ECHO_SPAN: usize = 25;
/// How many more pieces of the computer's sound are looked back on, for the time its
/// sound takes out of the loudspeakers and into the microphone.
const ECHO_LATE: usize = 10;
/// How many pieces of the others talking it takes to learn how loud they reach the
/// microphone, and over how many of the latest that is kept up.
const ECHO_LEARNED: usize = 50;
const ECHO_REMEMBERED: usize = 200;
/// The microphone must be this many times louder than the loudspeakers make it at
/// their loudest for the user to be speaking too.
const OWN_BY: f64 = 2.5;
/// How many pieces the user counts as still speaking after the last loud one, so that
/// the quiet ends of words are kept.
const OWN_HOLD: usize = 20;

/// Tells when the microphone hears nothing but the computer's own sound coming out of
/// the loudspeakers, so that the others are not recorded twice. It learns how loud the
/// loudspeakers reach the microphone, and takes whatever is clearly louder for the user.
#[derive(Default)]
pub struct Echo {
    far: std::collections::VecDeque<f64>,
    near: std::collections::VecDeque<f64>,
    /// The microphone's loudest piece of late over the loudspeakers' loudest, for the
    /// latest pieces in which it heard the loudspeakers.
    heard: std::collections::VecDeque<f64>,
    /// What that comes to as a rule: how much of the loudspeakers reaches the microphone.
    reach: Option<f64>,
    held: usize,
    /// How far the microphone is turned down at the end of the last piece.
    down: Option<f32>,
}

/// What is left of the microphone where it hears only the loudspeakers. It is turned
/// down and not off, so that words of the user's mistaken for the loudspeakers are
/// still there to be recognized.
const ECHO_LEFT: f32 = 0.25;

fn mean_square(samples: &[i16]) -> f64 {
    if samples.is_empty() {
        return 0.0;
    }
    samples
        .iter()
        .map(|sample| f64::from(*sample) * f64::from(*sample))
        .sum::<f64>()
        / samples.len() as f64
}

/// How alike two runs of loudness rise and fall: 1 when one is the other made louder
/// or quieter, 0 when they have nothing to do with each other.
fn alike(one: &[f64], other: &[f64]) -> f64 {
    let count = one.len().min(other.len()) as f64;
    if count == 0.0 {
        return 0.0;
    }
    let mean = |values: &[f64]| values.iter().sum::<f64>() / count;
    let (mean_one, mean_other) = (mean(one), mean(other));
    let (mut both, mut spread_one, mut spread_other) = (0.0, 0.0, 0.0);
    for (a, b) in one.iter().zip(other) {
        both += (a - mean_one) * (b - mean_other);
        spread_one += (a - mean_one) * (a - mean_one);
        spread_other += (b - mean_other) * (b - mean_other);
    }
    if spread_one <= 0.0 || spread_other <= 0.0 {
        return 0.0;
    }
    both / (spread_one * spread_other).sqrt()
}

/// How alike the microphone and the computer's sound must rise and fall for the
/// microphone to be hearing the loudspeakers.
const FOLLOWS: f64 = 0.7;

impl Echo {
    /// Whether the microphone has of late risen and fallen as the computer's sound did,
    /// at whatever delay the loudspeakers' sound reaches it.
    fn follows(&self) -> bool {
        if self.near.len() < ECHO_SPAN || self.far.len() < ECHO_SPAN + ECHO_LATE {
            return false;
        }
        let near: Vec<f64> = self.near.iter().map(|loud| loud.sqrt()).collect();
        let far: Vec<f64> = self.far.iter().map(|loud| loud.sqrt()).collect();
        (0..=ECHO_LATE).any(|late| {
            let from = ECHO_LATE - late;
            alike(&near, &far[from..from + ECHO_SPAN]) >= FOLLOWS
        })
    }

    /// Turns the microphone down where it holds nothing but the computer's sound,
    /// gliding from one loudness to the other within the piece.
    pub fn quieten(&mut self, microphone: &mut [i16], theirs: &[i16]) {
        let to = if self.only_echo(microphone, theirs) {
            ECHO_LEFT
        } else {
            1.0
        };
        let from = self.down.replace(to).unwrap_or(1.0);
        if from == 1.0 && to == 1.0 {
            return;
        }
        let count = microphone.len().max(1) as f32;
        for (index, sample) in microphone.iter_mut().enumerate() {
            let by = from + (to - from) * (index as f32 + 1.0) / count;
            *sample = (f32::from(*sample) * by) as i16;
        }
    }

    /// Whether this piece of the microphone holds nothing but the computer's sound.
    fn only_echo(&mut self, microphone: &[i16], theirs: &[i16]) -> bool {
        let now = mean_square(microphone);
        self.near.push_back(now);
        if self.near.len() > ECHO_SPAN {
            self.near.pop_front();
        }
        self.far.push_back(mean_square(theirs));
        if self.far.len() > ECHO_SPAN + ECHO_LATE {
            self.far.pop_front();
        }
        let loudest =
            |pieces: &std::collections::VecDeque<f64>| pieces.iter().copied().fold(0.0, f64::max);
        // Whatever the microphone has of the loudspeakers was played within this span,
        // so it is never louder than their loudest piece in it reaches the microphone.
        let played = loudest(&self.far);
        if played < TOO_QUIET {
            self.held = self.held.saturating_sub(1);
            return false;
        }
        // Learned only where the microphone rises and falls as the computer's sound
        // does: there it hears the loudspeakers and little else. The user's own words
        // do not follow the others', so they never pass for the loudspeakers.
        if self.follows() {
            self.heard.push_back(loudest(&self.near) / played);
            if self.heard.len() > ECHO_REMEMBERED {
                self.heard.pop_front();
            }
            if self.heard.len() >= ECHO_LEARNED && self.heard.len() % 10 == 0 {
                let mut sorted: Vec<f64> = self.heard.iter().copied().collect();
                sorted.sort_by(f64::total_cmp);
                self.reach = Some(sorted[sorted.len() / 2]);
            }
        }
        let Some(reach) = self.reach else {
            return false;
        };
        if now > played * reach * OWN_BY {
            self.held = OWN_HOLD;
            return false;
        }
        if self.held > 0 {
            self.held -= 1;
            return false;
        }
        true
    }
}

/// The computer's sound must be this many times weaker than the microphone's for words
/// to count as the user's own: with loudspeakers the microphone hears the others too,
/// but never louder than itself by this much.
const MINE_BY: f64 = 8.0;
/// Below this mean square neither side said anything to tell by.
const TOO_QUIET: f64 = 100.0 * 100.0;

/// Whether a stretch of the recording is the user's own side of a call: said into the
/// microphone while the computer played next to nothing. `mixed` is the recording and
/// `theirs` the computer's part of it. None when the computer's sound is not recorded
/// or the stretch is too quiet to tell.
pub fn is_mine(mixed: &[i16], theirs: &[i16]) -> Option<bool> {
    if theirs.len() != mixed.len() || mixed.is_empty() {
        return None;
    }
    let mut own = 0f64;
    let mut other = 0f64;
    for (sum, theirs) in mixed.iter().zip(theirs) {
        let mine = *sum as f64 - *theirs as f64;
        own += mine * mine;
        other += (*theirs as f64) * (*theirs as f64);
    }
    let count = mixed.len() as f64;
    if own / count < TOO_QUIET && other / count < TOO_QUIET {
        return None;
    }
    Some(own > other * MINE_BY)
}

struct Recording {
    /// The words the recognizer is given; corrections made during the meeting join them.
    vocabulary: Arc<Mutex<Vec<String>>>,
    /// The languages the meeting is held in, which the user may change while it runs.
    languages: Arc<Mutex<Vec<String>>>,
    capture: AudioCaptureHandle,
    system: Option<crate::system_audio::SystemAudioHandle>,
    waiting: Arc<Mutex<Waiting>>,
    collector: tokio::task::JoinHandle<()>,
    worker: std::thread::JoinHandle<()>,
}

struct SpeakerModels {
    segmentation: std::path::PathBuf,
    voice: std::path::PathBuf,
    scratch: std::path::PathBuf,
}

fn locked(waiting: &Mutex<Waiting>) -> std::sync::MutexGuard<'_, Waiting> {
    waiting.lock().unwrap_or_else(|error| error.into_inner())
}

/// Where to cut the next window off the waiting audio, if there is enough of it: at the
/// first pause once enough has been said, at the quietest moment of a full window that
/// has no pause, or everything left once the recording has ended.
pub fn cut_point(samples: &[i16], finishing: bool) -> Option<usize> {
    let whole = (finishing && !samples.is_empty()).then_some(samples.len());
    if samples.len() < EARLIEST_CUT + PAUSE_STEPS * STEP {
        return whole;
    }
    let within = samples.len().min(WINDOW);
    let energies: Vec<u64> = samples[..within]
        .chunks_exact(STEP)
        .map(|step| {
            step.iter()
                .map(|sample| (*sample as i64 * *sample as i64) as u64)
                .sum()
        })
        .collect();
    let mut sorted = energies.clone();
    sorted.sort_unstable();
    let loud = sorted[sorted.len() * 4 / 5];
    let quiet = |energy: u64| energy < loud / QUIET_SHARE;
    let first = EARLIEST_CUT / STEP;
    let pause = (first..energies.len().saturating_sub(PAUSE_STEPS - 1))
        .find(|at| energies[*at..*at + PAUSE_STEPS].iter().all(|e| quiet(*e)));
    if let Some(at) = pause {
        return Some(at * STEP + PAUSE_STEPS * STEP / 2);
    }
    if samples.len() < WINDOW {
        return whole;
    }
    let quietest = (first..energies.len())
        .min_by_key(|at| energies[*at])
        .unwrap_or(energies.len() - 1);
    Some(quietest * STEP + STEP / 2)
}

/// The end of what was said so far, for the recognizer to go on from.
fn context(said: &str) -> Option<String> {
    let skip = said.chars().count().saturating_sub(CONTEXT_CHARS);
    let tail: String = said.chars().skip(skip).collect();
    let tail = tail.trim();
    (!tail.is_empty()).then(|| tail.to_string())
}

/// True for words in a script the meeting's language is not written in: a meeting held
/// in a language of Latin letters that comes out mostly in Chinese, Japanese or Korean
/// characters was misheard.
pub fn out_of_language(text: &str, languages: &[String]) -> bool {
    const LATIN: [&str; 10] = ["en", "fr", "de", "es", "pt", "it", "vi", "id", "nl", "tr"];
    if languages.is_empty() || !languages.iter().all(|code| LATIN.contains(&code.as_str())) {
        return false;
    }
    let east = |c: char| matches!(c, '\u{3040}'..='\u{30ff}' | '\u{4e00}'..='\u{9fff}' | '\u{ac00}'..='\u{d7af}');
    let letters = text.chars().filter(|c| c.is_alphabetic()).count();
    let foreign = text.chars().filter(|c| east(*c)).count();
    letters > 0 && foreign * 2 > letters
}

/// The recognizer's words with `words` put first; the newest words count most.
pub fn with_words(known: &[String], words: &[String]) -> Vec<String> {
    let mut all: Vec<String> = Vec::new();
    for word in words.iter().chain(known) {
        let word = word.trim();
        if !word.is_empty() && !all.iter().any(|kept| kept.eq_ignore_ascii_case(word)) {
            all.push(word.to_string());
        }
    }
    all.truncate(MOST_WORDS);
    all
}

/// A window's recognized pieces as paragraphs, timed from the start of the meeting.
pub fn window_segments(
    offset_ms: u64,
    turns: Option<&[SpeakerTurn]>,
    said: &[(Option<usize>, String)],
) -> Vec<Segment> {
    let text_of = |turn: Option<usize>| {
        let parts: Vec<&str> = said
            .iter()
            .filter(|(of, _)| *of == turn)
            .map(|(_, text)| text.as_str())
            .collect();
        crate::file_transcribe::joined(&parts)
    };
    let Some(turns) = turns else {
        let text = text_of(None);
        return if text.is_empty() {
            Vec::new()
        } else {
            vec![Segment {
                speaker: None,
                start_ms: offset_ms,
                text,
                ..Segment::default()
            }]
        };
    };
    turns
        .iter()
        .enumerate()
        .map(|(index, turn)| Segment {
            speaker: Some(turn.speaker),
            start_ms: offset_ms + turn.start_ms,
            end_ms: offset_ms + turn.end_ms,
            text: text_of(Some(index)),
            ..Segment::default()
        })
        .filter(|segment| !segment.text.is_empty())
        .collect()
}

/// What the recognizer is told about the audio it gets.
#[derive(Clone, Default)]
struct Told {
    vocabulary: Vec<String>,
    /// What was said just before.
    before: Option<String>,
    /// The languages the meeting is held in; empty when the model is to tell.
    languages: Vec<String>,
}

/// A stretch of the recording to be recognized.
struct Window<'a> {
    sound: &'a [i16],
    /// The computer's part of the sound, sample for sample; empty when it is not recorded.
    theirs: &'a [i16],
    /// Where the stretch begins in the recording.
    offset_ms: u64,
    /// True for the first stretch of a recording.
    first: bool,
}

fn recognize_window(
    supervisor: &crate::local_stt::WorkerSupervisor,
    model: &std::path::Path,
    speakers: Option<&SpeakerModels>,
    told: &Told,
    heard: Window,
) -> Result<Vec<Segment>, String> {
    let Window {
        sound: window,
        theirs,
        offset_ms,
        first,
    } = heard;
    let pcm: Vec<f32> = window
        .iter()
        .map(|sample| *sample as f32 / 32_768.0)
        .collect();
    let turns = match speakers {
        Some(models) => {
            let (found, voices) = supervisor
                .speakers_window(
                    &models.segmentation,
                    &models.voice,
                    &pcm,
                    &models.scratch,
                    first,
                )
                .map_err(|error| error.to_string())?;
            crate::voiceprints::hear(voices, first);
            Some(crate::speakers::merge_turns(&found))
        }
        None => None,
    };
    let mut said = Vec::new();
    for (turn, piece) in crate::file_transcribe::plan(&pcm, turns.as_deref()) {
        let text = crate::file_transcribe::recognize_in(
            supervisor,
            model,
            &pcm[piece],
            &told.vocabulary,
            told.before.as_deref(),
            &told.languages,
        )?;
        let text = text.trim();
        if !text.is_empty() && !out_of_language(text, &told.languages) {
            said.push((turn, text.to_string()));
        }
    }
    let mut segments = window_segments(offset_ms, turns.as_deref(), &said);
    let whole_ms = (window.len() * 1000 / RATE) as u64;
    for segment in &mut segments {
        if segment.end_ms == 0 {
            segment.end_ms = offset_ms + whole_ms;
        }
        let sample =
            |ms: u64| ((ms.saturating_sub(offset_ms)) as usize * RATE / 1000).min(window.len());
        let (from, to) = (sample(segment.start_ms), sample(segment.end_ms));
        if theirs.len() == window.len() && from < to {
            segment.mine = is_mine(&window[from..to], &theirs[from..to]);
        }
    }
    Ok(segments)
}

/// Recognizes window after window until the recording has ended and nothing waits.
/// How much new audio there must be before the unsettled words are guessed at again.
const GUESS_EVERY: usize = RATE;
/// A guess that takes longer than this is no use; the machine then waits for settled words.
const SLOWEST_GUESS: std::time::Duration = std::time::Duration::from_secs(6);

/// What the recorder reports: words that are settled, or a guess at the words still coming.
pub enum Heard {
    Said(Segment),
    Guess(String),
}

/// When to guess at the words that are not settled yet. A machine that needs long for a
/// guess makes fewer of them, and one that needs very long makes none.
struct Guessing {
    guessed_at: usize,
    every: usize,
    off: bool,
}

impl Default for Guessing {
    fn default() -> Self {
        Self {
            guessed_at: 0,
            every: GUESS_EVERY,
            off: false,
        }
    }
}

impl Guessing {
    fn due(&self, unsettled: usize) -> bool {
        !self.off && unsettled >= RATE && unsettled >= self.guessed_at + self.every
    }

    fn took(&mut self, unsettled: usize, elapsed: std::time::Duration) {
        self.guessed_at = unsettled;
        self.off = elapsed > SLOWEST_GUESS;
        let twice = (elapsed.as_secs_f64() * 2.0 * RATE as f64) as usize;
        self.every = twice.max(GUESS_EVERY);
    }

    fn settled(&mut self) {
        self.guessed_at = 0;
    }
}

/// Counts one more window lost; true once too many were lost in a row.
fn gives_up(lost: &mut usize) -> bool {
    *lost += 1;
    *lost >= MOST_LOST_WINDOWS
}

fn work(
    waiting: Arc<Mutex<Waiting>>,
    supervisor: Arc<crate::local_stt::WorkerSupervisor>,
    model: std::path::PathBuf,
    speakers: Option<SpeakerModels>,
    vocabulary: Arc<Mutex<Vec<String>>>,
    languages: Arc<Mutex<Vec<String>>>,
    mut report: impl FnMut(Result<Heard, String>),
) {
    let mut first = true;
    let mut guessing = Guessing::default();
    let mut said = String::new();
    let mut lost = 0;
    loop {
        let told = Told {
            vocabulary: vocabulary
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .clone(),
            before: context(&said),
            languages: languages
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .clone(),
        };
        let (window, theirs, offset_ms) = {
            let mut waiting = locked(&waiting);
            let closing = waiting.finishing || std::mem::take(&mut waiting.settle);
            match cut_point(&waiting.pending, closing) {
                Some(cut) => {
                    // The computer's part is cut along with the whole, or is not there.
                    let theirs: Vec<i16> = if waiting.theirs.len() == waiting.pending.len() {
                        waiting.theirs.drain(..cut).collect()
                    } else {
                        waiting.theirs.clear();
                        Vec::new()
                    };
                    let window: Vec<i16> = waiting.pending.drain(..cut).collect();
                    let offset_ms = (waiting.taken * 1000 / RATE) as u64;
                    waiting.taken += cut;
                    (window, theirs, offset_ms)
                }
                None if waiting.finishing => return,
                None => {
                    let unsettled = waiting.pending.len();
                    let heard = guessing.due(unsettled).then(|| waiting.pending.clone());
                    drop(waiting);
                    let Some(heard) = heard else {
                        std::thread::sleep(std::time::Duration::from_millis(300));
                        continue;
                    };
                    let started = std::time::Instant::now();
                    let loud = heard
                        .iter()
                        .any(|sample| sample.unsigned_abs() >= SILENT_PEAK);
                    if loud {
                        let pcm: Vec<f32> = heard.iter().map(|s| *s as f32 / 32_768.0).collect();
                        let guessed = crate::file_transcribe::recognize_in(
                            &supervisor,
                            &model,
                            &pcm,
                            &told.vocabulary,
                            told.before.as_deref(),
                            &told.languages,
                        );
                        match guessed {
                            Ok(text) if out_of_language(&text, &told.languages) => {}
                            Ok(text) => report(Ok(Heard::Guess(text.trim().to_string()))),
                            // Settled words still come; only the guesses stop.
                            Err(_) => guessing.off = true,
                        }
                    }
                    guessing.took(unsettled, started.elapsed());
                    continue;
                }
            }
        };
        let peak = window.iter().map(|sample| sample.unsigned_abs()).max();
        if window.len() < SHORTEST_WINDOW || peak.unwrap_or(0) < SILENT_PEAK {
            continue;
        }
        let recognized =
            crate::file_transcribe::patiently(&crate::file_transcribe::TRY_AGAIN_AFTER, || {
                recognize_window(
                    &supervisor,
                    &model,
                    speakers.as_ref(),
                    &told,
                    Window {
                        sound: &window,
                        theirs: &theirs,
                        offset_ms,
                        first,
                    },
                )
            });
        match recognized {
            Ok(segments) => {
                lost = 0;
                segments.into_iter().for_each(|segment| {
                    said.push_str(&segment.text);
                    said.push(' ');
                    if let Some(dir) = DATA_DIR.get() {
                        write_down(dir, &segment);
                    }
                    report(Ok(Heard::Said(segment)))
                })
            }
            // One window that cannot be recognized is passed over, and what follows is
            // still written out. Audio that piled up meanwhile is caught up on window
            // by window.
            Err(error) if !gives_up(&mut lost) => {
                tracing::warn!("[Meeting] a window at {offset_ms} ms was passed over: {error}");
                crate::applog::note(
                    "meeting",
                    &format!("the window at {offset_ms} ms was passed over ({lost} lost in a row)"),
                );
                guessing.settled();
                continue;
            }
            Err(error) => {
                let mut waiting = locked(&waiting);
                waiting.dropped = true;
                waiting.pending = Vec::new();
                waiting.theirs = Vec::new();
                crate::applog::note("meeting", &format!("recognition gave up at {offset_ms} ms"));
                report(Err(error));
                return;
            }
        }
        first = false;
        guessing.settled();
    }
}

static ACTIVE: Mutex<Option<Recording>> = Mutex::new(None);

fn active() -> std::sync::MutexGuard<'static, Option<Recording>> {
    ACTIVE.lock().unwrap_or_else(|error| error.into_inner())
}

/// Adds the capture's 16-bit little-endian bytes to the recording, up to the limit.
fn collect(samples: &mut Vec<i16>, bytes: &[u8], limit: usize) {
    let room = limit.saturating_sub(samples.len());
    samples.extend(
        bytes
            .chunks_exact(2)
            .take(room)
            .map(|pair| i16::from_le_bytes([pair[0], pair[1]])),
    );
}

pub fn is_recording() -> bool {
    active().is_some()
}

/// Pauses the running recording, or goes on with it. Nothing said during a pause is
/// kept. Returns whether a recording was there to pause.
pub fn pause(paused: bool) -> bool {
    let active = active();
    let Some(recording) = active.as_ref() else {
        return false;
    };
    let mut waiting = locked(&recording.waiting);
    waiting.paused = paused;
    waiting.settle = paused;
    true
}

/// Changes the languages the running recording is held in.
pub fn set_languages(languages: Vec<String>) {
    if let Some(recording) = active().as_ref() {
        *recording
            .languages
            .lock()
            .unwrap_or_else(|error| error.into_inner()) =
            crate::file_transcribe::chosen_languages(languages);
    }
}

/// Gives the recognizer of the running recording more words to listen for.
pub fn add_words(words: &[String]) {
    if let Some(recording) = active().as_ref() {
        let mut known = recording
            .vocabulary
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        *known = with_words(&known, words);
    }
}

/// Starts recording from `input_device` (empty for the system default). Every paragraph
/// recognized, and an error that ends recognition, goes to `report`.
pub async fn start(
    input_device: String,
    by_speaker: bool,
    system_sound: bool,
    model_id: Option<String>,
    vocabulary: Vec<String>,
    languages: Vec<String>,
    report: impl FnMut(Result<Heard, String>) + Send + 'static,
) -> Result<(), String> {
    if is_recording() {
        return Err("busy".to_string());
    }
    delete_recording();
    let model = crate::file_transcribe::model_path(model_id.as_deref())
        .ok_or_else(|| "local_model_not_installed".to_string())?;
    let supervisor =
        crate::local_stt::global().ok_or_else(|| "local_engine_unavailable".to_string())?;
    let speakers = if by_speaker {
        let missing = || "speaker_models_not_installed".to_string();
        let (segmentation, voice) = crate::speakers::installed_models().ok_or_else(missing)?;
        Some(SpeakerModels {
            segmentation,
            voice,
            scratch: crate::speakers::model_dir().ok_or_else(missing)?,
        })
    } else {
        None
    };
    let config = AudioConfig {
        input_device,
        follow_devices: true,
        ..AudioConfig::default()
    };
    let (mut capture, mut audio) =
        AudioCaptureHandle::start(config).map_err(|error| error.to_string())?;
    capture
        .wait_until_ready()
        .await
        .map_err(|error| error.to_string())?;
    // The computer's own sound, for the far side of an online meeting.
    let heard: Arc<Mutex<std::collections::VecDeque<i16>>> = Arc::default();
    let system = if system_sound {
        let (handle, mut sound) = crate::system_audio::start()?;
        let heard = heard.clone();
        tokio::spawn(async move {
            while let Some(bytes) = sound.recv().await {
                let mut heard = heard.lock().unwrap_or_else(|error| error.into_inner());
                heard.extend(
                    bytes
                        .chunks_exact(2)
                        .map(|pair| i16::from_le_bytes([pair[0], pair[1]])),
                );
                let extra = heard.len().saturating_sub(SYSTEM_BACKLOG);
                heard.drain(..extra);
            }
        });
        Some(handle)
    } else {
        None
    };
    let waiting = Arc::new(Mutex::new(Waiting::default()));
    let sink = waiting.clone();
    // A recording that cannot be kept on disk is still written out as text.
    let mut file = recording_path().and_then(|path| {
        let mut file = std::io::BufWriter::new(std::fs::File::create(path).ok()?);
        file.write_all(&wav_header(0)).ok()?;
        Some(file)
    });
    // From here until the recording ends in order, it can be found again if it is cut short.
    if let Some(dir) = DATA_DIR.get() {
        let _ = std::fs::write(dir.join(JOURNAL), "");
    }
    // With headphones the microphone hears nothing of the computer's sound. They may be
    // put on or taken off while the meeting runs, so this is looked at again and again.
    let worn = Arc::new(std::sync::atomic::AtomicBool::new(
        system_sound && crate::audio::output::headphones(),
    ));
    if system_sound {
        let worn = Arc::downgrade(&worn);
        std::thread::spawn(move || {
            while let Some(worn) = worn.upgrade() {
                worn.store(
                    crate::audio::output::headphones(),
                    std::sync::atomic::Ordering::Relaxed,
                );
                drop(worn);
                std::thread::sleep(std::time::Duration::from_secs(2));
            }
        });
    }
    let mut echo = Echo::default();
    let collector = tokio::spawn(async move {
        while let Some(bytes) = audio.recv().await {
            let (bytes, theirs) = if system_sound {
                let mut heard = heard.lock().unwrap_or_else(|error| error.into_inner());
                let loudspeakers = !worn.load(std::sync::atomic::Ordering::Relaxed);
                mixed_with(&bytes, &mut heard, loudspeakers.then_some(&mut echo))
            } else {
                (bytes, Vec::new())
            };
            // The lock is held while the file is written, so that a pause leaves the same
            // audio out of the file and out of the text, and the times in the text stay
            // the times in the file.
            let mut waiting = locked(&sink);
            if waiting.paused {
                continue;
            }
            if let Some(writer) = file.as_mut() {
                let even = bytes.len() & !1;
                if writer.write_all(&bytes[..even]).is_err() {
                    file = None;
                }
            }
            if !waiting.dropped {
                let room = LONGEST_SAMPLES.saturating_sub(waiting.pending.len());
                collect(&mut waiting.pending, &bytes, LONGEST_SAMPLES);
                waiting.theirs.extend(theirs.into_iter().take(room));
            }
        }
    });
    let queue = waiting.clone();
    let vocabulary = Arc::new(Mutex::new(with_words(&vocabulary, &[])));
    let words = vocabulary.clone();
    let languages = Arc::new(Mutex::new(crate::file_transcribe::chosen_languages(
        languages,
    )));
    let held_in = languages.clone();
    let worker = std::thread::spawn(move || {
        work(queue, supervisor, model, speakers, words, held_in, report)
    });
    *active() = Some(Recording {
        vocabulary,
        languages,
        capture,
        system,
        waiting,
        collector,
        worker,
    });
    Ok(())
}

/// How long the recording is so far, how loud the microphone is now, and how many
/// seconds of it still wait to be recognized.
pub fn level() -> Option<(u64, f32, u64)> {
    let active = active();
    let recording = active.as_ref()?;
    let waiting = locked(&recording.waiting);
    let pending = waiting.pending.len();
    Some((
        ((waiting.taken + pending) / RATE) as u64,
        recording.capture.get_volume(),
        (pending / RATE) as u64,
    ))
}

/// Ends the recording and waits until what was recorded has been recognized. With
/// `discard` the audio still waiting is dropped instead, and so is the file on disk.
/// Waits for the task that takes in the audio to end, and ends it when it does not.
async fn settled(task: &mut tokio::task::JoinHandle<()>, patience: std::time::Duration) {
    // The capture thread closes the channel once it has handed over what it still held.
    if tokio::time::timeout(patience, &mut *task).await.is_ok() {
        // It has ended, and a task that has ended must not be waited for again.
        return;
    }
    task.abort();
    // An aborted task drops its writer, which hands the file what it still held.
    let _ = task.await;
}

pub async fn stop(discard: bool) -> Result<(), String> {
    let Some(mut recording) = active().take() else {
        return Err("not_recording".to_string());
    };
    recording.capture.stop();
    if let Some(mut system) = recording.system.take() {
        // Ending it waits for the audio system, which must not hold up this thread.
        let _ = tokio::task::spawn_blocking(move || system.stop()).await;
    }
    settled(&mut recording.collector, std::time::Duration::from_secs(3)).await;
    match recording_path() {
        Some(_) if discard => delete_recording(),
        Some(path) => {
            let _ = finish_wav(&path);
        }
        None => {}
    }
    {
        let mut waiting = locked(&recording.waiting);
        waiting.finishing = true;
        if discard {
            waiting.dropped = true;
            waiting.pending = Vec::new();
            waiting.theirs = Vec::new();
        }
    }
    let ended = tokio::task::spawn_blocking(move || recording.worker.join())
        .await
        .map_err(|error| error.to_string())
        .and_then(|joined| joined.map_err(|_| "local_engine_unavailable".to_string()));
    // Everything said has been handed over by now, so nothing is left to find again.
    if let Some(dir) = DATA_DIR.get() {
        let _ = std::fs::remove_file(dir.join(JOURNAL));
    }
    ended
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guesses_come_as_the_unsettled_audio_grows() {
        let mut guessing = Guessing::default();
        assert!(!guessing.due(RATE / 2), "too little to guess at");
        assert!(guessing.due(3 * RATE));
        guessing.took(3 * RATE, std::time::Duration::from_millis(400));
        assert!(!guessing.due(3 * RATE + RATE / 2), "nothing much new yet");
        assert!(guessing.due(3 * RATE + GUESS_EVERY));
    }

    #[test]
    fn a_slow_machine_guesses_less_often_and_a_very_slow_one_not_at_all() {
        let mut guessing = Guessing::default();
        guessing.took(3 * RATE, std::time::Duration::from_secs(3));
        assert!(!guessing.due(3 * RATE + GUESS_EVERY));
        assert!(guessing.due(3 * RATE + 6 * RATE));
        guessing.took(9 * RATE, std::time::Duration::from_secs(7));
        assert!(!guessing.due(19 * RATE));
    }

    #[test]
    fn guessing_starts_over_once_words_are_settled() {
        let mut guessing = Guessing::default();
        guessing.took(10 * RATE, std::time::Duration::from_millis(400));
        guessing.settled();
        assert!(guessing.due(GUESS_EVERY));
    }

    #[tokio::test]
    async fn waiting_for_a_task_that_has_already_ended_does_not_panic() {
        let mut ended = tokio::spawn(async {});
        settled(&mut ended, std::time::Duration::from_secs(1)).await;
    }

    #[tokio::test]
    async fn a_task_that_does_not_end_is_ended() {
        let mut stuck = tokio::spawn(std::future::pending::<()>());
        settled(&mut stuck, std::time::Duration::from_millis(10)).await;
        assert!(stuck.is_finished());
    }

    #[test]
    fn bytes_become_samples_and_stop_at_the_limit() {
        let mut samples = Vec::new();
        collect(&mut samples, &[0x00, 0x40, 0xff, 0xff, 0x01], 10);
        assert_eq!(samples, vec![16_384, -1]);
        collect(&mut samples, &[1, 0, 2, 0, 3, 0], 4);
        assert_eq!(samples, vec![16_384, -1, 1, 2]);
    }

    #[test]
    fn a_recording_that_was_cut_short_is_found_again_with_what_was_said() {
        let dir = std::env::temp_dir().join(format!("ot-recover-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        assert!(recover_in(&dir).is_none(), "nothing was being recorded");

        // Five seconds of sound whose header was never given its length, and two
        // paragraphs written down before the app went away.
        let mut raw = wav_header(0).to_vec();
        raw.extend(std::iter::repeat_n([0u8, 16], 5 * RATE).flatten());
        std::fs::write(dir.join(RECORDING), raw).unwrap();
        let said = |start_ms, text: &str| Segment {
            speaker: Some(1),
            start_ms,
            text: text.to_string(),
            ..Segment::default()
        };
        write_down(&dir, &said(0, "one"));
        write_down(&dir, &said(3_000, "two"));
        std::fs::OpenOptions::new()
            .append(true)
            .open(dir.join(JOURNAL))
            .unwrap()
            .write_all(b"{\"speaker\":1,\"start_")
            .unwrap();

        let found = recover_in(&dir).unwrap();
        assert_eq!(found.seconds, 5);
        assert_eq!(found.segments, vec![said(0, "one"), said(3_000, "two")]);
        let header = std::fs::read(dir.join(RECORDING)).unwrap();
        let length = u32::from_le_bytes(header[40..44].try_into().unwrap());
        assert_eq!(length as usize, 5 * RATE * 2);
        assert!(
            recover_in(&dir).is_none(),
            "a meeting is found again only once"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_recording_too_short_to_matter_is_not_brought_back() {
        let dir = std::env::temp_dir().join(format!("ot-recover-short-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut raw = wav_header(0).to_vec();
        raw.extend(std::iter::repeat_n([0u8, 16], RATE).flatten());
        std::fs::write(dir.join(RECORDING), raw).unwrap();
        std::fs::write(dir.join(JOURNAL), "").unwrap();
        assert!(recover_in(&dir).is_none());
        assert!(!dir.join(JOURNAL).exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn words_said_into_the_microphone_while_the_computer_is_quiet_are_the_users_own() {
        let loud: Vec<i16> = (0..RATE)
            .map(|at| if at % 2 == 0 { 6_000 } else { -6_000 })
            .collect();
        let quiet = vec![0i16; RATE];
        // The user speaks, and the computer plays nothing.
        assert_eq!(is_mine(&loud, &quiet), Some(true));
        // The others speak through headphones: the recording is the computer's sound.
        assert_eq!(is_mine(&loud, &loud), Some(false));
        // The others speak through loudspeakers, and the microphone hears them as well.
        let both: Vec<i16> = loud.iter().map(|sample| sample + sample / 2).collect();
        assert_eq!(is_mine(&both, &loud), Some(false));
        // Nobody speaks, or the computer's sound is not recorded.
        assert_eq!(is_mine(&quiet, &quiet), None);
        assert_eq!(is_mine(&loud, &[]), None);
    }

    /// A voice-like sound that is the same every time: noise whose loudness changes
    /// from piece to piece, with pauses, each voice in its own way.
    fn voice(pieces: usize, loud: f64, seed: u32) -> Vec<i16> {
        let mut state = seed;
        let mut next = move || {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            f64::from(state >> 16) / 65_536.0
        };
        let mut samples = Vec::with_capacity(pieces * 320);
        for _ in 0..pieces {
            let level = next();
            let level = if level < 0.25 { 0.0 } else { 0.2 + level };
            for _ in 0..320 {
                samples.push(((next() * 2.0 - 1.0) * loud * level) as i16);
            }
        }
        samples
    }

    /// What the microphone hears of `theirs` out of the loudspeakers: later and weaker.
    fn from_the_loudspeakers(theirs: &[i16]) -> Vec<i16> {
        let late = 1_600;
        (0..theirs.len())
            .map(|index| {
                if index < late {
                    0
                } else {
                    (f64::from(theirs[index - late]) * 0.3) as i16
                }
            })
            .collect()
    }

    /// How many of the pieces from `from` on were taken for the loudspeakers alone.
    fn taken_out(
        echo: &mut Echo,
        microphone: &[i16],
        theirs: &[i16],
        from: usize,
    ) -> (usize, usize) {
        let mut out = 0;
        let mut all = 0;
        for (index, (own, other)) in microphone.chunks(320).zip(theirs.chunks(320)).enumerate() {
            let only = echo.only_echo(own, other);
            if index >= from {
                all += 1;
                out += usize::from(only);
            }
        }
        (out, all)
    }

    #[test]
    fn what_the_microphone_hears_only_from_the_loudspeakers_is_taken_out() {
        let theirs = voice(500, 8_000.0, 1);
        let microphone = from_the_loudspeakers(&theirs);
        let (out, all) = taken_out(&mut Echo::default(), &microphone, &theirs, 150);
        assert!(out * 10 >= all * 9, "{out} of {all}");
    }

    #[test]
    fn nothing_is_taken_out_before_the_loudspeakers_are_known() {
        let theirs = voice(40, 8_000.0, 1);
        let microphone = from_the_loudspeakers(&theirs);
        assert_eq!(
            taken_out(&mut Echo::default(), &microphone, &theirs, 0).0,
            0
        );
    }

    #[test]
    fn the_user_talking_over_the_others_is_kept() {
        let theirs = voice(500, 8_000.0, 1);
        let mut microphone = from_the_loudspeakers(&theirs);
        // From the fourth second on the user talks too.
        for (sample, own) in microphone
            .iter_mut()
            .zip(voice(500, 6_000.0, 7))
            .skip(200 * 320)
        {
            *sample = sample.saturating_add(own);
        }
        let (out, all) = taken_out(&mut Echo::default(), &microphone, &theirs, 215);
        assert_eq!(out, 0, "{out} of {all}");
    }

    #[test]
    fn the_user_talking_while_the_computer_is_quiet_is_kept() {
        let microphone = voice(300, 6_000.0, 7);
        let theirs = vec![0; microphone.len()];
        assert_eq!(
            taken_out(&mut Echo::default(), &microphone, &theirs, 0).0,
            0
        );
    }

    #[test]
    fn with_headphones_nobody_told_of_the_user_is_still_kept() {
        let theirs = voice(500, 8_000.0, 1);
        // The microphone hears nothing of the others, then the user over them.
        let mut microphone = vec![0_i16; theirs.len()];
        for (sample, own) in microphone
            .iter_mut()
            .zip(voice(500, 3_000.0, 7))
            .skip(200 * 320)
        {
            *sample = own;
        }
        let (out, all) = taken_out(&mut Echo::default(), &microphone, &theirs, 215);
        assert_eq!(out, 0, "{out} of {all}");
    }

    #[test]
    fn a_stretch_that_is_only_the_loudspeakers_is_recorded_with_the_microphone_turned_down() {
        let theirs = voice(400, 8_000.0, 1);
        let microphone = from_the_loudspeakers(&theirs);
        let mut echo = Echo::default();
        let mut system: std::collections::VecDeque<i16> = theirs.iter().copied().collect();
        let (mut heard, mut kept) = (0.0, 0.0);
        for piece in microphone.chunks(320) {
            let bytes: Vec<u8> = piece
                .iter()
                .flat_map(|sample| sample.to_le_bytes())
                .collect();
            let (mixed, part) = mixed_with(&bytes, &mut system, Some(&mut echo));
            let own: Vec<i16> = mixed
                .chunks_exact(2)
                .zip(&part)
                .map(|(pair, part)| i16::from_le_bytes([pair[0], pair[1]]) - part)
                .collect();
            if system.len() < 200 * 320 {
                heard += mean_square(piece);
                kept += mean_square(&own);
            }
        }
        assert!(kept < heard * 0.1, "{kept} of {heard}");
    }

    #[test]
    fn the_microphone_is_turned_down_without_a_jump() {
        let mut echo = Echo {
            reach: Some(1.0),
            ..Echo::default()
        };
        let theirs = vec![8_000_i16; 320];
        let mut microphone = vec![1_000_i16; 320];
        echo.quieten(&mut microphone, &theirs);
        assert!(microphone[0] > 990, "{}", microphone[0]);
        assert_eq!(microphone[319], 250);
        let mut next = vec![1_000_i16; 320];
        echo.quieten(&mut next, &theirs);
        assert!(next.iter().all(|sample| *sample == 250));
    }

    #[test]
    fn mixing_keeps_the_computers_part_beside_the_sum() {
        let mut system = std::collections::VecDeque::from(vec![100i16, 200]);
        let microphone = [10i16, 20, 30]
            .iter()
            .flat_map(|s| s.to_le_bytes())
            .collect::<Vec<_>>();
        let (mixed, theirs) = mixed_with(&microphone, &mut system, None);
        assert_eq!(theirs, vec![100, 200, 0]);
        assert_eq!(
            mixed,
            [110i16, 220, 30]
                .iter()
                .flat_map(|s| s.to_le_bytes())
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn the_kept_recording_is_a_file_other_programs_can_open() {
        let path = std::env::temp_dir().join(format!("ot-meeting-{}.wav", std::process::id()));
        let mut raw = wav_header(0).to_vec();
        raw.extend(std::iter::repeat_n([0u8, 16], RATE).flatten());
        std::fs::write(&path, raw).unwrap();
        finish_wav(&path).unwrap();
        let pcm = crate::file_transcribe::decode(&path);
        let _ = std::fs::remove_file(&path);
        let written = wav_header(6);
        // Decoding trims a few samples at the edges.
        assert!(pcm.is_ok_and(|pcm| pcm.len() > RATE - 100 && pcm.len() <= RATE));
        assert_eq!(&written[0..4], b"RIFF");
        assert_eq!(u32::from_le_bytes(written[4..8].try_into().unwrap()), 42);
        assert_eq!(u32::from_le_bytes(written[40..44].try_into().unwrap()), 6);
    }

    /// Needs the built worker, a speech model, the two speaker models and a recording:
    /// `OT_FAR OT_OWN`: two real recordings, one played by the loudspeakers and one said
    /// by the user from the second minute on. Prints how much was taken out of each part.
    #[test]
    #[ignore]
    fn takes_a_real_voice_out_of_the_loudspeakers() {
        let read = |name: &str| -> Vec<i16> {
            crate::file_transcribe::decode(&PathBuf::from(std::env::var(name).unwrap()))
                .unwrap()
                .iter()
                .take(180 * RATE)
                .map(|sample| (sample * 32_767.0) as i16)
                .collect()
        };
        let theirs = read("OT_FAR");
        let own = read("OT_OWN");
        let mut microphone = from_the_loudspeakers(&theirs);
        let from = 60 * RATE;
        for (sample, own) in microphone.iter_mut().zip(&own).skip(from) {
            *sample = sample.saturating_add(*own);
        }
        let mut echo = Echo::default();
        let (mut alone, mut alone_out, mut over, mut over_out) = (0, 0, 0, 0);
        let (mut said, mut said_down) = (0.0, 0.0);
        for (index, (heard, other)) in microphone.chunks(320).zip(theirs.chunks(320)).enumerate() {
            let only = echo.only_echo(heard, other);
            let spoken = own
                .get(index * 320..(index + 1) * 320)
                .map(mean_square)
                .unwrap_or(0.0);
            if index * 320 < from {
                // Counted only where the loudspeakers really reach the microphone.
                if mean_square(heard) >= TOO_QUIET {
                    alone += 1;
                    alone_out += usize::from(only);
                }
            } else if spoken >= TOO_QUIET {
                over += 1;
                over_out += usize::from(only);
                said += spoken;
                if only {
                    said_down += spoken;
                }
            }
        }
        println!("loudspeakers alone: {alone_out} of {alone} pieces turned down");
        println!(
            "user talking over them: {over_out} of {over} pieces turned down, {:.1}% of what the user's voice carried",
            said_down / said * 100.0
        );
    }

    /// `OT_WORKER OT_MODEL OT_SEG OT_EMB OT_FILE`. Prints what a meeting would show.
    #[test]
    #[ignore]
    fn writes_a_real_meeting_out_window_by_window() {
        let var = |name: &str| PathBuf::from(std::env::var(name).unwrap());
        let supervisor = Arc::new(crate::local_stt::WorkerSupervisor::new(
            crate::local_stt::WorkerCommand {
                program: var("OT_WORKER"),
                args: vec!["serve".to_string()],
            },
            std::time::Duration::from_secs(120),
            std::time::Duration::from_secs(60),
        ));
        let speakers = SpeakerModels {
            segmentation: var("OT_SEG"),
            voice: var("OT_EMB"),
            scratch: std::env::temp_dir(),
        };
        let pcm = crate::file_transcribe::decode(&var("OT_FILE")).unwrap();
        let seconds = pcm.len() / RATE;
        let waiting = Arc::new(Mutex::new(Waiting {
            pending: pcm
                .iter()
                .map(|sample| (sample * 32_767.0) as i16)
                .collect(),
            finishing: true,
            ..Waiting::default()
        }));
        let started = std::time::Instant::now();
        work(
            waiting,
            supervisor,
            var("OT_MODEL"),
            Some(speakers),
            Arc::default(),
            Arc::new(Mutex::new(
                std::env::var("OT_LANGUAGES")
                    .map(|all| all.split(',').map(str::to_string).collect())
                    .unwrap_or_default(),
            )),
            |outcome| {
                let Heard::Said(segment) = outcome.unwrap() else {
                    return;
                };
                println!(
                    "[{:>5.1}s] speaker {:?} at {:>3}s: {}",
                    started.elapsed().as_secs_f32(),
                    segment.speaker.unwrap_or(0),
                    segment.start_ms / 1000,
                    segment.text
                );
            },
        );
        println!(
            "audio {seconds}s took {:.1}s",
            started.elapsed().as_secs_f32()
        );
    }

    #[test]
    fn the_computers_sound_is_added_to_the_microphones_and_silence_where_it_has_none() {
        let bytes = |samples: &[i16]| -> Vec<u8> {
            samples
                .iter()
                .flat_map(|sample| sample.to_le_bytes())
                .collect()
        };
        let mut system = std::collections::VecDeque::from([100, i16::MAX]);
        let mixed = mixed_with(&bytes(&[1, 5, 7]), &mut system, None).0;
        assert_eq!(mixed, bytes(&[101, i16::MAX, 7]));
        assert!(system.is_empty());
    }

    fn tone(seconds: usize, loud: i16) -> Vec<i16> {
        (0..seconds * RATE)
            .map(|i| if i % 2 == 0 { loud } else { -loud })
            .collect()
    }

    #[test]
    fn talk_without_a_pause_is_not_cut_until_a_full_window_waits() {
        assert_eq!(cut_point(&tone(19, 5_000), false), None);
        assert_eq!(cut_point(&[], true), None);
        let cut = cut_point(&tone(21, 5_000), false).unwrap();
        assert!((EARLIEST_CUT..=WINDOW).contains(&cut), "{cut}");
    }

    #[test]
    fn words_settle_at_the_first_pause_once_enough_was_said() {
        let mut samples = tone(8, 5_000);
        samples.extend(tone(1, 40));
        samples.extend(tone(2, 5_000));
        let cut = cut_point(&samples, false).unwrap();
        assert!((8 * RATE..9 * RATE).contains(&cut), "{cut}");
    }

    #[test]
    fn a_pause_too_early_does_not_cut() {
        let mut samples = tone(2, 5_000);
        samples.extend(tone(1, 0));
        samples.extend(tone(5, 5_000));
        assert_eq!(cut_point(&samples, false), None);
    }

    #[test]
    fn the_recognizer_goes_on_from_the_end_of_what_was_said() {
        assert_eq!(context(""), None);
        assert_eq!(context("so far "), Some("so far".to_string()));
        let long = "字".repeat(CONTEXT_CHARS + 50);
        assert_eq!(context(&long).unwrap().chars().count(), CONTEXT_CHARS);
    }

    #[test]
    fn chinese_heard_in_an_english_meeting_is_a_mishearing() {
        let held_in = |codes: &[&str]| {
            codes
                .iter()
                .map(|code| code.to_string())
                .collect::<Vec<_>>()
        };
        assert!(out_of_language("谢谢大家", &held_in(&["en"])));
        assert!(out_of_language("谢谢大家", &held_in(&["en", "fr"])));
        assert!(!out_of_language("Thanks everyone", &held_in(&["en"])));
        assert!(!out_of_language("We met 王艳 today", &held_in(&["en"])));
        assert!(!out_of_language("谢谢大家", &held_in(&["en", "zh"])));
        assert!(!out_of_language("谢谢大家", &[]));
    }

    #[test]
    fn recognition_goes_on_past_a_lost_window_and_gives_up_after_several() {
        let mut lost = 0;
        assert!(!gives_up(&mut lost));
        assert!(!gives_up(&mut lost));
        assert!(gives_up(&mut lost));
    }

    #[test]
    fn corrected_words_come_first_and_none_twice() {
        let known = vec!["Tauri".to_string(), "Rust".to_string()];
        let all = with_words(&known, &["rust".to_string(), "Qwen".to_string()]);
        assert_eq!(all, vec!["rust", "Qwen", "Tauri"]);
        let many: Vec<String> = (0..40).map(|n| format!("w{n}")).collect();
        assert_eq!(with_words(&many, &[]).len(), MOST_WORDS);
    }

    #[test]
    fn what_is_left_goes_whole_once_the_recording_has_ended() {
        assert_eq!(cut_point(&tone(3, 5_000), true), Some(3 * RATE));
    }

    fn turn(start_ms: u64, end_ms: u64, speaker: u32) -> SpeakerTurn {
        SpeakerTurn {
            start_ms,
            end_ms,
            speaker,
        }
    }

    #[test]
    fn a_window_without_speakers_is_one_paragraph_at_its_own_time() {
        let said = vec![(None, "first".to_string()), (None, "second".to_string())];
        assert_eq!(
            window_segments(60_000, None, &said),
            vec![Segment {
                speaker: None,
                start_ms: 60_000,
                text: "first second".into(),
                ..Segment::default()
            }]
        );
        assert!(window_segments(0, None, &[]).is_empty());
    }

    #[test]
    fn each_turn_is_a_paragraph_timed_from_the_start_of_the_meeting() {
        let turns = [
            turn(0, 4_000, 1),
            turn(4_000, 9_000, 2),
            turn(9_000, 12_000, 1),
        ];
        let said = vec![
            (Some(0), "你好".to_string()),
            (Some(1), "收到".to_string()),
            (Some(1), "明白".to_string()),
        ];
        assert_eq!(
            window_segments(30_000, Some(&turns), &said),
            vec![
                Segment {
                    speaker: Some(1),
                    start_ms: 30_000,
                    text: "你好".into(),
                    end_ms: 34_000,
                    ..Segment::default()
                },
                Segment {
                    speaker: Some(2),
                    start_ms: 34_000,
                    text: "收到明白".into(),
                    end_ms: 39_000,
                    ..Segment::default()
                },
            ]
        );
    }
}
