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
/// A window is cut at the quietest moment between these two lengths.
const EARLIEST_CUT: usize = 20 * RATE;
const WINDOW: usize = 30 * RATE;
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

struct Recording {
    capture: AudioCaptureHandle,
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
    window: &[i16],
    offset_ms: u64,
    first: bool,
) -> Result<Vec<Segment>, String> {
    let pcm: Vec<f32> = window
        .iter()
        .map(|sample| *sample as f32 / 32_768.0)
        .collect();
    let turns = match speakers {
        Some(models) => Some(crate::speakers::merge_turns(
            &supervisor
                .speakers_window(
                    &models.segmentation,
                    &models.voice,
                    &pcm,
                    &models.scratch,
                    first,
                )
                .map_err(|error| error.to_string())?,
        )),
        None => None,
    };
    let mut said = Vec::new();
    for (turn, piece) in crate::file_transcribe::plan(&pcm, turns.as_deref()) {
        let text = supervisor
            .transcribe(model, &pcm[piece], Vec::new(), None)
            .map_err(|error| error.to_string())?;
        let text = text.trim();
        if !text.is_empty() {
            said.push((turn, text.to_string()));
        }
    }
    Ok(window_segments(offset_ms, turns.as_deref(), &said))
}

/// Recognizes window after window until the recording has ended and nothing waits.
fn work(
    waiting: Arc<Mutex<Waiting>>,
    supervisor: Arc<crate::local_stt::WorkerSupervisor>,
    model: std::path::PathBuf,
    speakers: Option<SpeakerModels>,
    mut report: impl FnMut(Result<Segment, String>),
) {
    let mut first = true;
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
                    drop(waiting);
                    std::thread::sleep(std::time::Duration::from_millis(300));
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
            &window,
            offset_ms,
            first,
        ) {
            Ok(segments) => segments.into_iter().for_each(|segment| report(Ok(segment))),
            Err(error) => {
                let mut waiting = locked(&waiting);
                waiting.dropped = true;
                waiting.pending = Vec::new();
                report(Err(error));
                return;
            }
        }
        first = false;
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
    model_id: Option<String>,
    report: impl FnMut(Result<Segment, String>) + Send + 'static,
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
    let worker = std::thread::spawn(move || work(queue, supervisor, model, speakers, report));
    *active() = Some(Recording {
        capture,
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
pub async fn stop(discard: bool) -> Result<(), String> {
    let Some(mut recording) = active().take() else {
        return Err("not_recording".to_string());
    };
    recording.capture.stop();
    // The capture thread closes the channel once it has handed over what it still held.
    let _ = tokio::time::timeout(std::time::Duration::from_secs(3), &mut recording.collector).await;
    recording.collector.abort();
    // An aborted task drops its writer, which hands the file what it still held.
    let _ = (&mut recording.collector).await;
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
            |outcome| {
                let segment = outcome.unwrap();
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

    fn tone(seconds: usize, loud: i16) -> Vec<i16> {
        (0..seconds * RATE)
            .map(|i| if i % 2 == 0 { loud } else { -loud })
            .collect()
    }

    #[test]
    fn nothing_is_cut_until_a_full_window_waits() {
        assert_eq!(cut_point(&tone(29, 5_000), false), None);
        assert_eq!(cut_point(&[], true), None);
    }

    #[test]
    fn a_window_is_cut_in_the_pause_nearest_its_end() {
        let mut samples = tone(24, 5_000);
        samples.extend(tone(1, 0));
        samples.extend(tone(10, 5_000));
        let cut = cut_point(&samples, false).unwrap();
        assert!((24 * RATE..25 * RATE).contains(&cut), "{cut}");
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
