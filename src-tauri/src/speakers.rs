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
/// (Apache-2.0), in the form sherpa-onnx loads, and NVIDIA's Nemotron-3-Diarization
/// (OpenMDW-1.1), which follows up to eight speakers through a whole recording.
pub const MODEL_FILES: [ModelFile; 3] = [
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
    ModelFile {
        file_name: "speaker-turns.gguf",
        url: "https://github.com/PenguinMiaou/Speech/releases/download/models-1/Nemotron-3-Diarization-Q8_0.gguf",
        size_bytes: 108_664_384,
        sha256: "8bb6dbb3de10c94f2b413e83e0219f6a7c4a86e3a9be3074e2b46e17b9770028",
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

fn downloaded(index: usize) -> Option<PathBuf> {
    let dir = model_dir()?;
    let file = &MODEL_FILES[index];
    crate::voice_isolation::is_downloaded(&dir, file).then(|| dir.join(file.file_name))
}

/// The models for a recording still running, and the voice model: the model that
/// follows the speakers hears the recording as it comes in, so a speaker keeps one
/// number throughout. Only when every file is downloaded.
pub fn installed_models() -> Option<(PathBuf, PathBuf)> {
    models_for_a_file()
}

/// What the log says when telling the speakers of one window of a running recording
/// took more than half as long as the window lasts, which holds the words back.
pub fn slow_window(window_ms: u64, took: std::time::Duration) -> Option<String> {
    (took.as_millis() as u64 * 2 > window_ms).then(|| {
        format!(
            "speakers of a {:.1} second window took {:.1} seconds",
            window_ms as f32 / 1000.0,
            took.as_secs_f32()
        )
    })
}

/// The models for a finished recording: the one that follows the speakers through all
/// of it, and the voice model.
pub fn models_for_a_file() -> Option<(PathBuf, PathBuf)> {
    downloaded(0)?;
    Some((downloaded(2)?, downloaded(1)?))
}

/// Whether the model tells the speakers of a whole recording in one go.
pub fn follows(model: &Path) -> bool {
    model
        .extension()
        .is_some_and(|extension| extension == "gguf")
}

/// What the log says once a model has followed the speakers through a whole recording.
pub fn followed(model: &Path, turns: &[SpeakerTurn], took: std::time::Duration) -> String {
    let speakers: std::collections::BTreeSet<u32> = turns.iter().map(|turn| turn.speaker).collect();
    format!(
        "speakers followed by {}: {} speakers, {} turns, in {:.1} seconds",
        model
            .file_name()
            .map(|name| name.to_string_lossy())
            .unwrap_or_default(),
        speakers.len(),
        turns.len(),
        took.as_secs_f32()
    )
}

/// The turns of a whole recording that fall between two moments, counted from the first.
pub fn within(whole: &[SpeakerTurn], from_ms: u64, to_ms: u64) -> Vec<SpeakerTurn> {
    whole
        .iter()
        .filter(|turn| turn.start_ms < to_ms && turn.end_ms > from_ms)
        .map(|turn| SpeakerTurn {
            start_ms: turn.start_ms.max(from_ms) - from_ms,
            end_ms: turn.end_ms.min(to_ms) - from_ms,
            speaker: turn.speaker,
        })
        .collect()
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

    #[test]
    fn the_log_says_which_model_followed_the_speakers_and_what_it_found() {
        let turn = |start_ms, end_ms, speaker| SpeakerTurn {
            start_ms,
            end_ms,
            speaker,
        };
        let turns = [turn(0, 900, 1), turn(900, 2_000, 2), turn(2_000, 2_500, 1)];
        assert_eq!(
            followed(
                Path::new("/models/speaker-turns.gguf"),
                &turns,
                std::time::Duration::from_millis(21_300)
            ),
            "speakers followed by speaker-turns.gguf: 2 speakers, 3 turns, in 21.3 seconds"
        );
    }

    #[test]
    fn the_log_says_when_the_speakers_of_a_window_took_too_long() {
        let took = std::time::Duration::from_millis;
        assert_eq!(
            slow_window(10_000, took(6_200)).as_deref(),
            Some("speakers of a 10.0 second window took 6.2 seconds")
        );
        assert_eq!(slow_window(10_000, took(4_900)), None);
    }

    fn turn(start_ms: u64, end_ms: u64, speaker: u32) -> SpeakerTurn {
        SpeakerTurn {
            start_ms,
            end_ms,
            speaker,
        }
    }

    #[test]
    fn a_part_of_a_recording_gets_the_turns_that_fall_in_it_counted_from_its_start() {
        let whole = [
            turn(0, 4_000, 1),
            turn(4_000, 12_000, 2),
            turn(12_000, 20_000, 1),
        ];
        assert_eq!(
            within(&whole, 5_000, 15_000),
            vec![turn(0, 7_000, 2), turn(7_000, 10_000, 1)]
        );
        assert_eq!(within(&whole, 20_000, 30_000), vec![]);
    }

    #[test]
    fn the_model_that_follows_speakers_is_downloaded_with_the_others() {
        assert_eq!(MODEL_FILES.len(), 3);
        assert!(follows(Path::new(MODEL_FILES[2].file_name)));
        assert!(!follows(Path::new(MODEL_FILES[0].file_name)));
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
