//! Who spoke when in a recording, for writing a meeting out by speaker. The models run in
//! the speech worker; this module holds their files and turns the worker's answer into
//! the stretches that are recognized one by one.

use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

pub use ot_stt_protocol::SpeakerTurn;

use crate::voice_isolation::ModelFile;

const RATE: u64 = 16_000;
/// Two turns of one speaker this close together are one paragraph.
const SAME_PARAGRAPH_GAP_MS: u64 = 2_000;
/// Shorter than this holds no whole word; the recognizer only invents text for it.
const SHORTEST_TURN_MS: u64 = 400;

/// The pyannote segmentation 3.0 model (MIT) and the 3D-Speaker CAM++ voice model
/// (Apache-2.0), in the form sherpa-onnx loads.
pub const MODEL_FILES: [ModelFile; 2] = [
    ModelFile {
        file_name: "speaker-segmentation.onnx",
        url: "https://huggingface.co/csukuangfj/sherpa-onnx-pyannote-segmentation-3-0/resolve/9403a6902bb58e3d5ae8c7e77c3422de279db2e0/model.onnx",
        size_bytes: 5_992_913,
        sha256: "220ad67ca923bef2fa91f2390c786097bf305bceb5e261d4af67b38e938e1079",
    },
    ModelFile {
        file_name: "speaker-voice.onnx",
        url: "https://github.com/k2-fsa/sherpa-onnx/releases/download/speaker-recongition-models/3dspeaker_speech_campplus_sv_zh_en_16k-common_advanced.onnx",
        size_bytes: 28_281_164,
        sha256: "aa3cfc16963a10586a9393f5035d6d6b57e98d358b347f80c2a30bf4f00ceba2",
    },
];

static DATA_DIR: OnceLock<PathBuf> = OnceLock::new();

pub fn init(data_dir: &Path) {
    let _ = DATA_DIR.set(data_dir.to_path_buf());
}

pub fn model_dir() -> Option<PathBuf> {
    DATA_DIR
        .get()
        .map(|dir| dir.join("models").join("speakers"))
}

/// The two model files, when both are fully downloaded.
pub fn installed_models() -> Option<(PathBuf, PathBuf)> {
    let dir = model_dir()?;
    let whole = |file: &ModelFile| {
        crate::voice_isolation::is_downloaded(&dir, file).then(|| dir.join(file.file_name))
    };
    Some((whole(&MODEL_FILES[0])?, whole(&MODEL_FILES[1])?))
}

/// Tells who spoke when in a whole recording kept on disk. This runs in a worker of its
/// own, so that dictation is not kept waiting for the minutes it can take.
pub fn again(recording: &Path) -> Result<Vec<SpeakerTurn>, String> {
    let missing = || "speaker_models_not_installed".to_string();
    let (segmentation, voice) = installed_models().ok_or_else(missing)?;
    let scratch = model_dir().ok_or_else(missing)?;
    let command = crate::local_stt::default_worker_command().map_err(|e| e.to_string())?;
    heard_whole(command, &segmentation, &voice, &scratch, recording)
}

fn heard_whole(
    command: crate::local_stt::WorkerCommand,
    segmentation: &Path,
    voice: &Path,
    scratch: &Path,
    recording: &Path,
) -> Result<Vec<SpeakerTurn>, String> {
    let pcm = crate::file_transcribe::decode(recording)?;
    let worker = crate::local_stt::WorkerSupervisor::new(
        command,
        crate::local_stt::REQUEST_TIMEOUT,
        std::time::Duration::from_secs(60),
    );
    let found = worker.diarize(segmentation, voice, &pcm, scratch);
    worker.stop();
    let (turns, _) = found.map_err(|error| crate::local_stt::supervisor::failure_code(&error))?;
    Ok(merge_turns(&turns))
}

/// Tidies the worker's turns into paragraphs: no audio twice, no blips, and one speaker's
/// neighbouring turns joined.
pub fn merge_turns(turns: &[SpeakerTurn]) -> Vec<SpeakerTurn> {
    let mut merged: Vec<SpeakerTurn> = Vec::new();
    for turn in turns {
        let mut turn = *turn;
        if let Some(last) = merged.last() {
            turn.start_ms = turn.start_ms.max(last.end_ms);
        }
        if turn.end_ms < turn.start_ms + SHORTEST_TURN_MS {
            continue;
        }
        match merged.last_mut() {
            Some(last)
                if last.speaker == turn.speaker
                    && turn.start_ms <= last.end_ms + SAME_PARAGRAPH_GAP_MS =>
            {
                last.end_ms = turn.end_ms;
            }
            _ => merged.push(turn),
        }
    }
    merged
}

/// The samples of a turn, kept inside a recording of `len` samples.
pub fn samples(turn: &SpeakerTurn, len: usize) -> Range<usize> {
    let at = |ms: u64| ((ms * RATE / 1000) as usize).min(len);
    at(turn.start_ms)..at(turn.end_ms)
}

fn clock(ms: u64) -> String {
    let seconds = ms / 1000;
    let (hours, minutes, seconds) = (seconds / 3600, seconds / 60 % 60, seconds % 60);
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes:02}:{seconds:02}")
    }
}

/// The recognized turns as text: a line naming the speaker and the time, then what they said.
pub fn transcript(turns: &[(SpeakerTurn, String)], speaker_label: &str) -> String {
    turns
        .iter()
        .filter(|(_, text)| !text.trim().is_empty())
        .map(|(turn, text)| {
            format!(
                "{speaker_label} {}  {}\n{}",
                turn.speaker,
                clock(turn.start_ms),
                text.trim()
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Run by hand on a real recording: `OT_WORKER OT_SEG OT_EMB OT_WAV`.
    #[test]
    #[ignore]
    fn tells_the_speakers_of_a_kept_recording_again() {
        let at = |name: &str| std::path::PathBuf::from(std::env::var(name).unwrap());
        let command = crate::local_stt::WorkerCommand {
            program: at("OT_WORKER"),
            args: vec!["serve".to_string()],
        };
        let began = std::time::Instant::now();
        let turns = heard_whole(
            command,
            &at("OT_SEG"),
            &at("OT_EMB"),
            &std::env::temp_dir(),
            &at("OT_WAV"),
        )
        .unwrap();
        let mut spoke = std::collections::BTreeMap::new();
        for turn in &turns {
            *spoke.entry(turn.speaker).or_insert(0) += (turn.end_ms - turn.start_ms) / 1000;
        }
        println!(
            "{} turns in {:?}, seconds each: {spoke:?}",
            turns.len(),
            began.elapsed()
        );
    }

    fn turn(start_ms: u64, end_ms: u64, speaker: u32) -> SpeakerTurn {
        SpeakerTurn {
            start_ms,
            end_ms,
            speaker,
        }
    }

    #[test]
    fn neighbouring_turns_of_one_speaker_become_one() {
        let merged = merge_turns(&[
            turn(0, 2_000, 1),
            turn(2_400, 5_000, 1),
            turn(5_100, 9_000, 2),
            turn(9_200, 12_000, 1),
        ]);
        assert_eq!(
            merged,
            vec![
                turn(0, 5_000, 1),
                turn(5_100, 9_000, 2),
                turn(9_200, 12_000, 1)
            ]
        );
    }

    #[test]
    fn a_long_pause_keeps_one_speaker_in_two_paragraphs() {
        let merged = merge_turns(&[turn(0, 2_000, 1), turn(8_000, 9_000, 1)]);
        assert_eq!(merged.len(), 2);
    }

    #[test]
    fn overlapping_turns_do_not_repeat_audio() {
        let merged = merge_turns(&[turn(0, 5_800, 1), turn(5_500, 9_000, 2)]);
        assert_eq!(merged, vec![turn(0, 5_800, 1), turn(5_800, 9_000, 2)]);
    }

    #[test]
    fn a_blip_too_short_to_hold_a_word_is_dropped() {
        let merged = merge_turns(&[
            turn(0, 3_000, 1),
            turn(3_000, 3_200, 2),
            turn(3_300, 6_000, 1),
        ]);
        assert_eq!(merged, vec![turn(0, 6_000, 1)]);
    }

    #[test]
    fn a_turn_becomes_a_sample_range_inside_the_recording() {
        assert_eq!(samples(&turn(1_000, 2_000, 1), 100_000), 16_000..32_000);
        assert_eq!(samples(&turn(1_000, 9_000, 1), 20_000), 16_000..20_000);
    }

    #[test]
    fn the_transcript_names_each_speaker_with_the_time_they_start() {
        let text = transcript(
            &[
                (turn(0, 5_000, 1), "大家好。".to_string()),
                (turn(65_000, 70_000, 2), "Thanks.".to_string()),
                (turn(71_000, 72_000, 1), "  ".to_string()),
                (turn(3_725_000, 3_730_000, 1), "好的。".to_string()),
            ],
            "说话人",
        );
        assert_eq!(
            text,
            "说话人 1  00:00\n大家好。\n\n说话人 2  01:05\nThanks.\n\n说话人 1  1:02:05\n好的。"
        );
    }
}
