//! Keeps the user's own voice and drops other talkers before recognition, on this machine.
//! The user enrolls by speaking once; that sample stays in the app's data folder.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use crate::local_stt::supervisor::WorkerSupervisor;

const SAMPLE_RATE: usize = 16_000;
/// The model needs three seconds of the voice and uses at most the first thirty.
const MIN_SAMPLE_SECONDS: usize = 3;
const MAX_SAMPLE_SECONDS: usize = 30;
/// Below this loudness a recording is silence, not a voice.
const MIN_SAMPLE_RMS: f32 = 0.005;
/// Isolated audio that kept less than this share of the recording's loudness is not used.
/// Measured on 24 clips with synthetic noise: a second talker as loud as the user leaves
/// 0.52 to 0.66, steady noise as loud as the user (where the model also removes the user)
/// leaves a median of 0.34. Not yet checked against real rooms.
const KEEP_AT_LEAST: f32 = 0.5;
/// The model works on audio in pieces of this many samples.
const FEED_SAMPLES: usize = 5_120;

pub struct ModelFile {
    pub file_name: &'static str,
    pub url: &'static str,
    pub size_bytes: u64,
    pub sha256: &'static str,
}

/// VoxLock (Apache-2.0), in the form sherpa-onnx loads. The first file reads the voice
/// sample, the second processes the recording.
pub const MODEL_FILES: [ModelFile; 2] = [
    ModelFile {
        file_name: "voxlock_enroll.onnx",
        url: "https://github.com/arbazsiddiqui/voxlock/releases/download/sherpa-onnx-models/voxlock_enroll.onnx",
        size_bytes: 12_476_014,
        sha256: "5d06048c156bb8cd422d29848e6d38fb26a7b39f6f7fd92eccf2a6ed1ce003c6",
    },
    ModelFile {
        file_name: "voxlock_chunk_int8.onnx",
        url: "https://github.com/arbazsiddiqui/voxlock/releases/download/sherpa-onnx-models/voxlock_chunk_int8.onnx",
        size_bytes: 66_608_910,
        sha256: "df61a9d0062b0adbcf1182302b1eb8b815f15ac47ea4923e8e164c1aff64121d",
    },
];

/// How far the model download is, over both files.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadProgress {
    pub downloaded: u64,
    pub total: u64,
    pub bytes_per_sec: u64,
}

/// The progress of file `index` of [`MODEL_FILES`], as part of the whole download.
pub fn overall_download(index: usize, downloaded: u64, bytes_per_sec: u64) -> DownloadProgress {
    let before: u64 = MODEL_FILES[..index]
        .iter()
        .map(|file| file.size_bytes)
        .sum();
    DownloadProgress {
        downloaded: before + downloaded,
        total: MODEL_FILES.iter().map(|file| file.size_bytes).sum(),
        bytes_per_sec,
    }
}

static DATA_DIR: OnceLock<PathBuf> = OnceLock::new();
static ENABLED: AtomicBool = AtomicBool::new(false);
static ENROLL_NEXT: AtomicBool = AtomicBool::new(false);

pub fn init(data_dir: &Path) {
    let _ = DATA_DIR.set(data_dir.to_path_buf());
}

pub fn model_dir() -> Option<PathBuf> {
    DATA_DIR
        .get()
        .map(|dir| dir.join("models").join("voice-isolation"))
}

pub fn voice_sample_path() -> Option<PathBuf> {
    DATA_DIR.get().map(|dir| dir.join("voice-sample.wav"))
}

/// A model file is in place once it has its full size; it is only moved there after its
/// SHA-256 matched.
pub fn is_downloaded(dir: &Path, file: &ModelFile) -> bool {
    std::fs::metadata(dir.join(file.file_name)).is_ok_and(|meta| meta.len() == file.size_bytes)
}

/// The two model files, when both are fully downloaded.
pub fn installed_models() -> Option<(PathBuf, PathBuf)> {
    let dir = model_dir()?;
    let whole = |file: &ModelFile| is_downloaded(&dir, file).then(|| dir.join(file.file_name));
    Some((whole(&MODEL_FILES[0])?, whole(&MODEL_FILES[1])?))
}

fn download_lock() -> &'static tokio::sync::Mutex<()> {
    static LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
}

/// Waits until no other model download runs. Two downloads at once would write the same files.
pub async fn download_turn() -> tokio::sync::MutexGuard<'static, ()> {
    download_lock().lock().await
}

pub fn download_running() -> bool {
    download_lock().try_lock().is_err()
}

pub fn has_voice_sample() -> bool {
    voice_sample_path().is_some_and(|path| path.is_file())
}

pub fn set_enabled(enabled: bool) {
    ENABLED.store(enabled, Ordering::SeqCst);
}

/// Whether the user switched "only my voice" on.
pub fn enabled() -> bool {
    ENABLED.load(Ordering::SeqCst)
}

/// Everything a recording needs to be isolated, when the user switched it on and both the
/// model and a voice sample are there.
pub fn ready() -> Option<(PathBuf, PathBuf, PathBuf)> {
    if !ENABLED.load(Ordering::SeqCst) {
        return None;
    }
    let (enroll, chunk) = installed_models()?;
    let sample = voice_sample_path().filter(|path| path.is_file())?;
    Some((enroll, chunk, sample))
}

/// Asks that the next recording be kept as the user's voice sample.
pub fn request_enrollment() {
    ENROLL_NEXT.store(true, Ordering::SeqCst);
}

pub fn cancel_enrollment() {
    ENROLL_NEXT.store(false, Ordering::SeqCst);
}

pub fn take_enrollment_request() -> bool {
    ENROLL_NEXT.swap(false, Ordering::SeqCst)
}

/// How the last attempt to record a voice sample ended, for the settings page to show.
static LAST_ENROLLMENT: std::sync::Mutex<Option<Result<(), SampleError>>> =
    std::sync::Mutex::new(None);

/// Keeps `recorded` as the user's voice sample and remembers how that went.
pub fn enroll(recorded: &[f32]) {
    let outcome = match voice_sample_path() {
        Some(path) => save_voice_sample(&path, recorded),
        None => Err(SampleError::NotSaved),
    };
    tracing::info!("[Isolation] voice sample saved={}", outcome.is_ok());
    *LAST_ENROLLMENT.lock().unwrap_or_else(|e| e.into_inner()) = Some(outcome);
}

pub fn last_enrollment() -> Option<Result<(), SampleError>> {
    *LAST_ENROLLMENT.lock().unwrap_or_else(|e| e.into_inner())
}

pub fn enrollment_requested() -> bool {
    ENROLL_NEXT.load(Ordering::SeqCst)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SampleError {
    TooShort,
    TooQuiet,
    NotSaved,
}

impl SampleError {
    pub fn code(self) -> &'static str {
        match self {
            SampleError::TooShort => "too_short",
            SampleError::TooQuiet => "too_quiet",
            SampleError::NotSaved => "not_saved",
        }
    }
}

fn rms(audio: &[f32]) -> f32 {
    (audio.iter().map(|s| s * s).sum::<f32>() / audio.len().max(1) as f32).sqrt()
}

/// Writes `recorded` (16 kHz mono) as the voice sample at `path`.
pub fn save_voice_sample(path: &Path, recorded: &[f32]) -> Result<(), SampleError> {
    if recorded.len() < MIN_SAMPLE_SECONDS * SAMPLE_RATE {
        return Err(SampleError::TooShort);
    }
    if rms(recorded) < MIN_SAMPLE_RMS {
        return Err(SampleError::TooQuiet);
    }
    let kept = &recorded[..recorded.len().min(MAX_SAMPLE_SECONDS * SAMPLE_RATE)];
    let data_bytes = (kept.len() * 2) as u32;
    let mut wav = Vec::with_capacity(44 + data_bytes as usize);
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + data_bytes).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&(SAMPLE_RATE as u32).to_le_bytes());
    wav.extend_from_slice(&(SAMPLE_RATE as u32 * 2).to_le_bytes());
    wav.extend_from_slice(&2u16.to_le_bytes());
    wav.extend_from_slice(&16u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_bytes.to_le_bytes());
    for sample in kept {
        wav.extend_from_slice(&((sample.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes());
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|_| SampleError::NotSaved)?;
    }
    std::fs::write(path, wav).map_err(|_| SampleError::NotSaved)
}

/// Which audio goes to recognition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Audio {
    Recorded,
    Isolated,
}

/// Decides between the recording and its isolated version, and reports the share of
/// loudness the isolated version kept.
pub fn choose(recorded: &[f32], isolated: &[f32]) -> (Audio, f32) {
    let longer = recorded.len().max(isolated.len());
    let shorter = recorded.len().min(isolated.len());
    if shorter == 0 || (longer - shorter) * 10 > longer {
        return (Audio::Recorded, 0.0);
    }
    let before = rms(&recorded[..shorter]);
    if before <= f32::EPSILON {
        return (Audio::Recorded, 0.0);
    }
    let kept = rms(&isolated[..shorter]) / before;
    let audio = if kept >= KEEP_AT_LEAST {
        Audio::Isolated
    } else {
        Audio::Recorded
    };
    (audio, kept)
}

/// What the log says about one dictation: which sound went to the recognizer and why.
pub fn outcome(chosen: Option<(Audio, f32)>, waited_ms: u128) -> String {
    match chosen {
        Some((Audio::Isolated, kept)) => format!(
            "the isolated sound was used: it kept {kept:.2} of the loudness, waited {waited_ms} ms"
        ),
        Some((Audio::Recorded, kept)) => format!(
            "the recording was used: the isolated sound kept {kept:.2} of the loudness, waited {waited_ms} ms"
        ),
        None => format!(
            "the recording was used: the isolated sound was not ready after {waited_ms} ms"
        ),
    }
}

/// One recording being isolated while it is made.
pub struct Session {
    sender: Sender<Vec<f32>>,
    /// Behind a lock only so that the session can be shared between threads.
    done: std::sync::Mutex<Receiver<Option<Vec<f32>>>>,
    cancelled: Arc<AtomicBool>,
}

impl Session {
    pub fn start(
        supervisor: Arc<WorkerSupervisor>,
        enroll_model: PathBuf,
        chunk_model: PathBuf,
        voice_sample: PathBuf,
    ) -> Self {
        let (sender, pieces) = mpsc::channel::<Vec<f32>>();
        let (finished, done) = mpsc::channel();
        let cancelled = Arc::new(AtomicBool::new(false));
        let stop = cancelled.clone();
        std::thread::spawn(move || {
            let mut working = supervisor
                .isolate_begin(&enroll_model, &chunk_model, &voice_sample)
                .map_err(|error| tracing::warn!("Voice isolation could not start: {error}"))
                .is_ok();
            let began = working;
            let mut waiting: Vec<f32> = Vec::new();
            // Ends when `finish` drops the sender.
            for piece in pieces {
                if !working || stop.load(Ordering::SeqCst) {
                    continue;
                }
                waiting.extend(piece);
                if waiting.len() >= FEED_SAMPLES {
                    working = supervisor.isolate_feed(&waiting).is_ok();
                    waiting.clear();
                }
            }
            if working && !waiting.is_empty() && !stop.load(Ordering::SeqCst) {
                working = supervisor.isolate_feed(&waiting).is_ok();
            }
            // Always ended, so the worker is free for recognition.
            let isolated = if began {
                supervisor.isolate_end().ok()
            } else {
                None
            };
            let _ = finished.send(isolated.filter(|_| working && !stop.load(Ordering::SeqCst)));
        });
        Self {
            sender,
            done: std::sync::Mutex::new(done),
            cancelled,
        }
    }

    pub fn feed(&self, piece: Vec<f32>) {
        let _ = self.sender.send(piece);
    }

    /// The isolated recording, or None when it failed or is not ready within `wait`.
    pub fn finish(self, wait: Duration) -> Option<Vec<f32>> {
        let Session {
            sender,
            done,
            cancelled,
        } = self;
        drop(sender);
        let done = done.into_inner().unwrap_or_else(|e| e.into_inner());
        match done.recv_timeout(wait) {
            Ok(isolated) => isolated,
            Err(_) => {
                cancelled.store(true, Ordering::SeqCst);
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_log_says_whether_the_isolated_sound_was_used() {
        assert_eq!(
            outcome(Some((Audio::Isolated, 0.82)), 140),
            "the isolated sound was used: it kept 0.82 of the loudness, waited 140 ms"
        );
        assert_eq!(
            outcome(Some((Audio::Recorded, 0.31)), 90),
            "the recording was used: the isolated sound kept 0.31 of the loudness, waited 90 ms"
        );
        assert_eq!(
            outcome(None, 3000),
            "the recording was used: the isolated sound was not ready after 3000 ms"
        );
    }

    fn tone(seconds: f32, level: f32) -> Vec<f32> {
        (0..(seconds * 16_000.0) as usize)
            .map(|n| level * (n as f32 * 0.05).sin())
            .collect()
    }

    fn temp_file(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "ot-voice-{}-{}-{name}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    #[test]
    fn isolated_audio_is_used_unless_most_of_the_recording_was_removed() {
        let recorded = tone(1.0, 0.5);
        let scaled = |by: f32| -> Vec<f32> { recorded.iter().map(|s| s * by).collect() };
        assert_eq!(choose(&recorded, &scaled(1.0)).0, Audio::Isolated);
        assert_eq!(choose(&recorded, &scaled(0.6)).0, Audio::Isolated);
        // Losing more than half the loudness is what steady loud noise looks like, where the
        // model also removes the speaker.
        assert_eq!(choose(&recorded, &scaled(0.4)).0, Audio::Recorded);
    }

    #[test]
    fn isolated_audio_of_another_length_is_not_trusted() {
        let recorded = tone(1.0, 0.5);
        assert_eq!(choose(&recorded, &recorded[..8_000]).0, Audio::Recorded);
        assert_eq!(choose(&recorded, &[]).0, Audio::Recorded);
    }

    #[test]
    fn a_voice_sample_needs_three_seconds_of_sound() {
        let path = temp_file("short.wav");
        assert_eq!(
            save_voice_sample(&path, &tone(2.9, 0.3)),
            Err(SampleError::TooShort)
        );
        assert_eq!(
            save_voice_sample(&path, &tone(4.0, 0.0001)),
            Err(SampleError::TooQuiet)
        );
        assert!(!path.exists());
    }

    #[test]
    fn a_voice_sample_is_saved_as_sixteen_bit_mono_and_cut_to_thirty_seconds() {
        let path = temp_file("voice.wav");
        save_voice_sample(&path, &tone(40.0, 0.3)).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(&bytes[..4], b"RIFF");
        assert_eq!(&bytes[8..16], b"WAVEfmt ");
        assert_eq!(u16::from_le_bytes([bytes[22], bytes[23]]), 1);
        assert_eq!(
            u32::from_le_bytes([bytes[24], bytes[25], bytes[26], bytes[27]]),
            16_000
        );
        assert_eq!(u16::from_le_bytes([bytes[34], bytes[35]]), 16);
        assert_eq!(bytes.len(), 44 + 30 * 16_000 * 2);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn a_model_file_counts_as_downloaded_only_at_its_full_size() {
        let dir = temp_file("models");
        std::fs::create_dir_all(&dir).unwrap();
        let file = &MODEL_FILES[0];
        assert!(!is_downloaded(&dir, file));
        std::fs::write(dir.join(file.file_name), vec![0u8; 10]).unwrap();
        assert!(!is_downloaded(&dir, file));
        std::fs::write(
            dir.join(file.file_name),
            vec![0u8; file.size_bytes as usize],
        )
        .unwrap();
        assert!(is_downloaded(&dir, file));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[tokio::test]
    async fn only_one_model_download_runs_and_a_second_waits_for_it() {
        let first = download_turn().await;
        assert!(download_running());
        let waiting = tokio::spawn(async {
            let _turn = download_turn().await;
        });
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(!waiting.is_finished());
        drop(first);
        waiting.await.unwrap();
        assert!(!download_running());
    }

    #[test]
    fn download_progress_counts_both_model_files_together() {
        let total: u64 = MODEL_FILES.iter().map(|file| file.size_bytes).sum();
        let first = overall_download(0, 6_000_000, 2_000_000);
        assert_eq!(
            first,
            DownloadProgress {
                downloaded: 6_000_000,
                total,
                bytes_per_sec: 2_000_000
            }
        );
        let second = overall_download(1, 10_000_000, 3_000_000);
        assert_eq!(second.downloaded, MODEL_FILES[0].size_bytes + 10_000_000);
        assert_eq!(second.total, total);
    }

    #[test]
    fn a_request_to_enroll_is_used_by_one_recording_only() {
        request_enrollment();
        assert!(take_enrollment_request());
        assert!(!take_enrollment_request());
        request_enrollment();
        cancel_enrollment();
        assert!(!take_enrollment_request());
    }

    #[cfg(unix)]
    #[test]
    fn a_recording_sent_while_it_happens_comes_back_isolated() {
        use crate::local_stt::supervisor::tests::{model_file, supervisor};
        let worker = Arc::new(supervisor("ok", "", 5_000, 60_000));
        let file = model_file();
        let session = Session::start(worker, file.clone(), file.clone(), file);
        session.feed(vec![0.5, 0.25]);
        session.feed(vec![1.0]);
        assert_eq!(
            session.finish(Duration::from_secs(5)),
            Some(vec![0.25, 0.125, 0.5])
        );
    }

    /// Runs the built worker on a real recording, fed at the speed it would be recorded.
    /// `ISOLATE_WORKER`, `ISOLATE_ENROLL`, `ISOLATE_CHUNK`, `ISOLATE_VOICE` and `ISOLATE_IN`
    /// are paths; the last two are 16 kHz mono 16-bit wav files.
    #[test]
    #[ignore = "needs the built worker, the model files and recordings"]
    fn the_built_worker_isolates_a_recording_as_fast_as_it_is_spoken() {
        use crate::local_stt::supervisor::WorkerCommand;
        let path = |name: &str| PathBuf::from(std::env::var(name).expect(name));
        let worker = Arc::new(WorkerSupervisor::new(
            WorkerCommand {
                program: path("ISOLATE_WORKER"),
                args: vec!["serve".to_string()],
            },
            Duration::from_secs(30),
            Duration::from_secs(60),
        ));
        let wav = std::fs::read(path("ISOLATE_IN")).unwrap();
        let recorded: Vec<f32> = wav[44..]
            .chunks_exact(2)
            .map(|b| i16::from_le_bytes([b[0], b[1]]) as f32 / 32768.0)
            .collect();
        for round in 0..2 {
            let session = Session::start(
                worker.clone(),
                path("ISOLATE_ENROLL"),
                path("ISOLATE_CHUNK"),
                path("ISOLATE_VOICE"),
            );
            for piece in recorded.chunks(1_600) {
                session.feed(piece.to_vec());
                std::thread::sleep(Duration::from_millis(100));
            }
            let started = std::time::Instant::now();
            let isolated = session
                .finish(Duration::from_millis(1_500))
                .expect("in time");
            let (audio, kept) = choose(&recorded, &isolated);
            println!(
                "round={round} seconds={:.1} wait_ms={} kept={kept:.2} used={audio:?}",
                recorded.len() as f32 / 16_000.0,
                started.elapsed().as_millis()
            );
            assert_eq!(isolated.len(), recorded.len());
        }
    }

    #[cfg(unix)]
    #[test]
    fn a_recording_that_cannot_be_isolated_in_time_is_given_up() {
        use crate::local_stt::supervisor::tests::{model_file, supervisor};
        let worker = Arc::new(supervisor("ok", "", 5_000, 60_000));
        let file = model_file();
        let session = Session::start(worker, file.clone(), file.clone(), file);
        session.feed(vec![0.5; 16]);
        assert_eq!(session.finish(Duration::ZERO), None);
    }
}
