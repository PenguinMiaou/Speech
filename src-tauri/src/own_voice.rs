//! Keeps only what the user said in a dictation: the speakers of the recording are told
//! apart, each voice is compared with the user's voice sample, and the stretches of the
//! speakers who are not the user never reach the recognizer.

use ot_stt_protocol::{SpeakerTurn, SpeakerVoice};

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::SystemTime;

/// A speaker at least this alike to the voice sample is the user. Measured with the
/// voice model on one user's recordings: other people in a meeting came to 0.20 at most,
/// and 75 of 76 of the user's own dictations to 0.30 or more (the middle one to 0.65).
pub const MINE: f32 = 0.30;
/// This much sound is kept before and after each stretch, so a word is not cut into.
const AROUND_MS: u64 = 200;

/// What was found out about a dictation.
#[derive(Debug, Clone, PartialEq)]
pub enum Verdict {
    /// No speaker said enough for a voice to be told, so nothing is known.
    Unknown,
    /// The stretches the user spoke, in milliseconds, and how alike each speaker with a
    /// voice is to the sample.
    Told {
        mine: Vec<(u64, u64)>,
        speakers: Vec<(u32, f32)>,
    },
}

fn alike(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    let size = |v: &[f32]| v.iter().map(|x| x * x).sum::<f32>().sqrt();
    let sizes = size(a) * size(b);
    if sizes > 0.0 {
        dot / sizes
    } else {
        0.0
    }
}

/// Which stretches of a recording the user spoke, given who spoke when, the voice of
/// each speaker who said enough to have one, and the voice of the user's sample.
pub fn judge(turns: &[SpeakerTurn], voices: &[SpeakerVoice], sample: &[f32]) -> Verdict {
    if voices.is_empty() {
        return Verdict::Unknown;
    }
    let speakers: Vec<(u32, f32)> = voices
        .iter()
        .map(|voice| (voice.speaker, alike(sample, &voice.embedding)))
        .collect();
    let is_mine = |speaker: u32| {
        speakers
            .iter()
            .any(|(known, score)| *known == speaker && *score >= MINE)
    };
    let mine = turns
        .iter()
        .filter(|turn| is_mine(turn.speaker))
        .map(|turn| (turn.start_ms, turn.end_ms))
        .collect();
    Verdict::Told { mine, speakers }
}

/// The stretches of `pcm` (16 kHz) given in milliseconds, one after another.
pub fn cut(pcm: &[f32], mine: &[(u64, u64)]) -> Vec<f32> {
    let whole_ms = (pcm.len() / 16) as u64;
    let mut stretches: Vec<(u64, u64)> = Vec::new();
    let mut sorted = mine.to_vec();
    sorted.sort_unstable();
    for (start, end) in sorted {
        let (start, end) = (
            start.saturating_sub(AROUND_MS),
            (end + AROUND_MS).min(whole_ms),
        );
        match stretches.last_mut() {
            Some(last) if start <= last.1 => last.1 = last.1.max(end),
            _ => stretches.push((start, end)),
        }
    }
    let mut kept = Vec::new();
    for (start, end) in stretches {
        let at = |ms: u64| ((ms * 16) as usize).min(pcm.len());
        kept.extend_from_slice(&pcm[at(start)..at(end)]);
    }
    kept
}

/// What the log says about one dictation. Never what was said.
pub fn outcome(verdict: &Verdict, whole_ms: u64, took_ms: u128) -> String {
    let seconds = |ms: u64| ms as f32 / 1000.0;
    match verdict {
        Verdict::Unknown => format!(
            "own voice: no voice could be told in {:.1} seconds, the recording was used, in {took_ms} ms",
            seconds(whole_ms)
        ),
        Verdict::Told { mine, speakers } => {
            let scores: Vec<String> = speakers
                .iter()
                .map(|(_, score)| format!("{score:.2}"))
                .collect();
            let kept: u64 = mine.iter().map(|(start, end)| end - start).sum();
            format!(
                "own voice: {} speakers, alike {}, kept {:.1} of {:.1} seconds, in {took_ms} ms",
                speakers.len(),
                scores.join(" "),
                seconds(kept),
                seconds(whole_ms)
            )
        }
    }
}

/// The voice sample as saved by the app: 16 kHz mono, 16 bits, after a 44-byte header.
pub fn read_sample(path: &Path) -> Option<Vec<f32>> {
    let bytes = std::fs::read(path).ok()?;
    let data = bytes.get(44..)?;
    Some(
        data.chunks_exact(2)
            .map(|pair| i16::from_le_bytes([pair[0], pair[1]]) as f32 / 32_767.0)
            .collect(),
    )
}

/// The files a dictation needs for only the user's words to be written.
#[derive(Debug, Clone)]
pub struct Models {
    turns: PathBuf,
    voice: PathBuf,
    sample: PathBuf,
    scratch: PathBuf,
}

/// The files, when the user switched "only my voice" on, recorded a voice sample and
/// the models that tell speakers apart are installed.
pub fn ready() -> Option<Models> {
    if !crate::voice_isolation::enabled() {
        return None;
    }
    let sample = crate::voice_isolation::voice_sample_path().filter(|path| path.is_file())?;
    let (turns, voice) = crate::speakers::models_for_a_file()?;
    Some(Models {
        turns,
        voice,
        sample,
        scratch: crate::speakers::model_dir()?,
    })
}

/// The voice of the sample, kept until the sample is recorded anew.
static SAMPLE: Mutex<Option<(SystemTime, Vec<f32>)>> = Mutex::new(None);

fn sample_voice(
    supervisor: &crate::local_stt::WorkerSupervisor,
    models: &Models,
) -> Option<Vec<f32>> {
    let saved = std::fs::metadata(&models.sample).ok()?.modified().ok()?;
    let mut kept = SAMPLE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some((of, voice)) = kept.as_ref() {
        if *of == saved {
            return Some(voice.clone());
        }
    }
    let pcm = read_sample(&models.sample)?;
    let (turns, voices) = supervisor
        .heard(&models.turns, &models.voice, &pcm, &models.scratch)
        .ok()?;
    let said = |speaker: u32| -> u64 {
        turns
            .iter()
            .filter(|turn| turn.speaker == speaker)
            .map(|turn| turn.end_ms - turn.start_ms)
            .sum()
    };
    let voice = voices
        .into_iter()
        .max_by_key(|voice| said(voice.speaker))?
        .embedding;
    *kept = Some((saved, voice.clone()));
    Some(voice)
}

/// Has the models loaded and the sample's voice taken before a dictation ends, so the
/// answer does not wait for them.
pub fn warm(supervisor: &crate::local_stt::WorkerSupervisor, models: &Models) {
    let _ = sample_voice(supervisor, models);
}

/// Tells which stretches of a dictation the user spoke. `Unknown` as well when the
/// models could not be asked.
pub fn told(
    supervisor: &crate::local_stt::WorkerSupervisor,
    models: &Models,
    pcm: &[f32],
) -> Verdict {
    let Some(sample) = sample_voice(supervisor, models) else {
        return Verdict::Unknown;
    };
    match supervisor.heard(&models.turns, &models.voice, pcm, &models.scratch) {
        Ok((turns, voices)) => judge(&turns, &voices, &sample),
        Err(_) => Verdict::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn turn(start_ms: u64, end_ms: u64, speaker: u32) -> SpeakerTurn {
        SpeakerTurn {
            start_ms,
            end_ms,
            speaker,
        }
    }

    fn voice(speaker: u32, embedding: [f32; 2]) -> SpeakerVoice {
        SpeakerVoice {
            speaker,
            embedding: embedding.to_vec(),
        }
    }

    const ME: [f32; 2] = [1.0, 0.0];
    const LIKE_ME: [f32; 2] = [0.8, 0.6];
    const SOMEONE: [f32; 2] = [0.1, 1.0];

    #[test]
    fn only_the_stretches_of_the_speaker_who_sounds_like_the_sample_are_kept() {
        let turns = [turn(0, 4000, 1), turn(4000, 7000, 2), turn(7000, 9000, 1)];
        let voices = [voice(1, LIKE_ME), voice(2, SOMEONE)];
        let Verdict::Told { mine, speakers } = judge(&turns, &voices, &ME) else {
            panic!("the speakers have voices");
        };
        assert_eq!(mine, vec![(0, 4000), (7000, 9000)]);
        assert_eq!(speakers.len(), 2);
        assert!(speakers[0].1 > MINE && speakers[1].1 < MINE);
    }

    #[test]
    fn nothing_is_kept_when_nobody_sounds_like_the_sample() {
        let verdict = judge(&[turn(0, 4000, 1)], &[voice(1, SOMEONE)], &ME);
        assert!(matches!(verdict, Verdict::Told { mine, .. } if mine.is_empty()));
    }

    #[test]
    fn a_speaker_too_brief_to_have_a_voice_is_left_out_once_someone_is_told() {
        let turns = [turn(0, 4000, 1), turn(4000, 4600, 2)];
        let Verdict::Told { mine, .. } = judge(&turns, &[voice(1, LIKE_ME)], &ME) else {
            panic!("speaker 1 has a voice");
        };
        assert_eq!(mine, vec![(0, 4000)]);
    }

    #[test]
    fn a_recording_in_which_no_voice_can_be_told_is_not_judged() {
        assert_eq!(judge(&[turn(0, 600, 1)], &[], &ME), Verdict::Unknown);
        assert_eq!(judge(&[], &[], &ME), Verdict::Unknown);
    }

    #[test]
    fn what_is_kept_is_cut_out_with_a_little_before_and_after() {
        let pcm: Vec<f32> = (0..160_000).map(|index| index as f32).collect();
        let kept = cut(&pcm, &[(1000, 2000), (2100, 3000), (9900, 10_000)]);
        // 1000–3000 as one stretch from 800 to 3200, then 9700 to the end.
        assert_eq!(kept.len(), (2400 + 300) * 16);
        assert_eq!(kept[0], 12_800.0);
        assert_eq!(kept[2400 * 16], 155_200.0);
    }

    #[test]
    fn the_log_says_how_alike_each_speaker_was_and_how_much_was_kept() {
        let told = Verdict::Told {
            mine: vec![(0, 4000)],
            speakers: vec![(1, 0.62), (2, 0.08)],
        };
        assert_eq!(
            outcome(&told, 9000, 310),
            "own voice: 2 speakers, alike 0.62 0.08, kept 4.0 of 9.0 seconds, in 310 ms"
        );
        assert_eq!(
            outcome(&Verdict::Unknown, 900, 120),
            "own voice: no voice could be told in 0.9 seconds, the recording was used, in 120 ms"
        );
    }

    #[test]
    fn the_voice_sample_is_read_back_as_it_was_saved() {
        let dir = std::env::temp_dir().join(format!("own-voice-{}", std::process::id()));
        let path = dir.join("voice-sample.wav");
        let recorded: Vec<f32> = (0..64_000)
            .map(|index| ((index % 100) as f32 - 50.0) / 100.0)
            .collect();
        crate::voice_isolation::save_voice_sample(&path, &recorded).unwrap();
        let read = read_sample(&path).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(read.len(), recorded.len());
        assert!((read[10] - recorded[10]).abs() < 0.001);
    }
}
