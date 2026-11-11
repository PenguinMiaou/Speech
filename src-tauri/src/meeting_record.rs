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
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Segment {
    pub speaker: Option<u32>,
    pub start_ms: u64,
    pub text: String,
}

#[derive(Default)]
struct Waiting {
    /// Recorded and not yet recognized.
    pending: Vec<i16>,
    /// Samples already handed to recognition.
    taken: usize,
    finishing: bool,
    dropped: bool,
}

static DATA_DIR: OnceLock<PathBuf> = OnceLock::new();

pub fn init(data_dir: &Path) {
    let _ = DATA_DIR.set(data_dir.to_path_buf());
}

/// Where the latest meeting's audio is kept.
fn recording_path() -> Option<PathBuf> {
    DATA_DIR.get().map(|dir| dir.join("meeting-recording.wav"))
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

/// The microphone's bytes with the computer's own sound added, sample by sample. The
/// microphone keeps the time: the computer's sound only arrives while something plays.
pub fn with_system_sound(
    microphone: &[u8],
    system: &mut std::collections::VecDeque<i16>,
) -> Vec<u8> {
    let mut mixed = Vec::with_capacity(microphone.len());
    for pair in microphone.chunks_exact(2) {
        let sample = i16::from_le_bytes([pair[0], pair[1]]);
        let sum = sample.saturating_add(system.pop_front().unwrap_or(0));
        mixed.extend_from_slice(&sum.to_le_bytes());
    }
    mixed
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
            }]
        };
    };
    turns
        .iter()
        .enumerate()
        .map(|(index, turn)| Segment {
            speaker: Some(turn.speaker),
            start_ms: offset_ms + turn.start_ms,
            text: text_of(Some(index)),
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

fn recognize_window(
    supervisor: &crate::local_stt::WorkerSupervisor,
    model: &std::path::Path,
    speakers: Option<&SpeakerModels>,
    told: &Told,
    window: &[i16],
    offset_ms: u64,
    first: bool,
) -> Result<Vec<Segment>, String> {
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
    Ok(window_segments(offset_ms, turns.as_deref(), &said))
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
        let (window, offset_ms) = {
            let mut waiting = locked(&waiting);
            match cut_point(&waiting.pending, waiting.finishing) {
                Some(cut) => {
                    let window: Vec<i16> = waiting.pending.drain(..cut).collect();
                    let offset_ms = (waiting.taken * 1000 / RATE) as u64;
                    waiting.taken += cut;
                    (window, offset_ms)
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
                    &window,
                    offset_ms,
                    first,
                )
            });
        match recognized {
            Ok(segments) => {
                lost = 0;
                segments.into_iter().for_each(|segment| {
                    said.push_str(&segment.text);
                    said.push(' ');
                    report(Ok(Heard::Said(segment)))
                })
            }
            // One window that cannot be recognized is passed over, and what follows is
            // still written out. Audio that piled up meanwhile is caught up on window
            // by window.
            Err(error) if !gives_up(&mut lost) => {
                tracing::warn!("[Meeting] a window at {offset_ms} ms was passed over: {error}");
                guessing.settled();
                continue;
            }
            Err(error) => {
                let mut waiting = locked(&waiting);
                waiting.dropped = true;
                waiting.pending = Vec::new();
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
    let collector = tokio::spawn(async move {
        while let Some(bytes) = audio.recv().await {
            let bytes = if system_sound {
                let mut heard = heard.lock().unwrap_or_else(|error| error.into_inner());
                with_system_sound(&bytes, &mut heard)
            } else {
                bytes
            };
            if let Some(writer) = file.as_mut() {
                let even = bytes.len() & !1;
                if writer.write_all(&bytes[..even]).is_err() {
                    file = None;
                }
            }
            let mut waiting = locked(&sink);
            if !waiting.dropped {
                collect(&mut waiting.pending, &bytes, LONGEST_SAMPLES);
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
        }
    }
    tokio::task::spawn_blocking(move || recording.worker.join())
        .await
        .map_err(|error| error.to_string())?
        .map_err(|_| "local_engine_unavailable".to_string())
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
        let mixed = with_system_sound(&bytes(&[1, 5, 7]), &mut system);
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
                text: "first second".into()
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
                    text: "你好".into()
                },
                Segment {
                    speaker: Some(2),
                    start_ms: 34_000,
                    text: "收到明白".into()
                },
            ]
        );
    }
}
