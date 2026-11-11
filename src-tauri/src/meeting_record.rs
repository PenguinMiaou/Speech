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
/// A window is cut at the quietest moment between these two lengths. On real meetings,
/// speakers are told apart as steadily in 20-second windows as in 30-second ones, and the
/// text arrives sooner.
const EARLIEST_CUT: usize = 12 * RATE;
const WINDOW: usize = 20 * RATE;
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
/// quietest moment of a full window, or everything left once the recording has ended.
pub fn cut_point(samples: &[i16], finishing: bool) -> Option<usize> {
    if samples.len() < WINDOW {
        return (finishing && !samples.is_empty()).then_some(samples.len());
    }
    let energy = |from: usize| -> u64 {
        samples[from..from + STEP]
            .iter()
            .map(|sample| (*sample as i64 * *sample as i64) as u64)
            .sum()
    };
    let quietest = (EARLIEST_CUT..=WINDOW - STEP)
        .step_by(STEP)
        .min_by_key(|from| energy(*from))
        .unwrap_or(WINDOW - STEP);
    Some(quietest + STEP / 2)
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

fn recognize_window(
    supervisor: &crate::local_stt::WorkerSupervisor,
    model: &std::path::Path,
    speakers: Option<&SpeakerModels>,
    vocabulary: &[String],
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
        let text = supervisor
            .transcribe(model, &pcm[piece], vocabulary.to_vec(), None)
            .map_err(|error| error.to_string())?;
        let text = text.trim();
        if !text.is_empty() {
            said.push((turn, text.to_string()));
        }
    }
    Ok(window_segments(offset_ms, turns.as_deref(), &said))
}

/// Recognizes window after window until the recording has ended and nothing waits.
/// How much new audio there must be before the unsettled words are guessed at again.
const GUESS_EVERY: usize = 5 * RATE / 2;
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

fn work(
    waiting: Arc<Mutex<Waiting>>,
    supervisor: Arc<crate::local_stt::WorkerSupervisor>,
    model: std::path::PathBuf,
    speakers: Option<SpeakerModels>,
    vocabulary: Vec<String>,
    mut report: impl FnMut(Result<Heard, String>),
) {
    let mut first = true;
    let mut guessing = Guessing::default();
    loop {
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
                        match supervisor.transcribe(&model, &pcm, vocabulary.clone(), None) {
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
        match recognize_window(
            &supervisor,
            &model,
            speakers.as_ref(),
            &vocabulary,
            &window,
            offset_ms,
            first,
        ) {
            Ok(segments) => segments
                .into_iter()
                .for_each(|segment| report(Ok(Heard::Said(segment)))),
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

/// Starts recording from `input_device` (empty for the system default). Every paragraph
/// recognized, and an error that ends recognition, goes to `report`.
pub async fn start(
    input_device: String,
    by_speaker: bool,
    system_sound: bool,
    model_id: Option<String>,
    vocabulary: Vec<String>,
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
    let worker =
        std::thread::spawn(move || work(queue, supervisor, model, speakers, vocabulary, report));
    *active() = Some(Recording {
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
        assert!(!guessing.due(4 * RATE), "nothing much new yet");
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
            Vec::new(),
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
    fn nothing_is_cut_until_a_full_window_waits() {
        assert_eq!(cut_point(&tone(19, 5_000), false), None);
        assert_eq!(cut_point(&[], true), None);
    }

    #[test]
    fn a_window_is_cut_in_the_pause_nearest_its_end() {
        let mut samples = tone(15, 5_000);
        samples.extend(tone(1, 0));
        samples.extend(tone(10, 5_000));
        let cut = cut_point(&samples, false).unwrap();
        assert!((15 * RATE..16 * RATE).contains(&cut), "{cut}");
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
