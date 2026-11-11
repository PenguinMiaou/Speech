//! Tells who spoke when in a finished recording, so a meeting can be written out by speaker.

use ot_stt_protocol::{SpeakerTurn, SpeakerVoice};
use sherpa_onnx::{
    FastClusteringConfig, OfflineSpeakerDiarization, OfflineSpeakerDiarizationConfig,
    OfflineSpeakerSegmentationModelConfig, OfflineSpeakerSegmentationPyannoteModelConfig,
    SpeakerEmbeddingExtractorConfig,
};

/// Speaker turns in the order they start. Speakers are numbered from 1 by first appearance.
pub fn diarize(
    segmentation_model: &str,
    embedding_model: &str,
    pcm: &[f32],
    threads: i32,
) -> Result<(Vec<SpeakerTurn>, Vec<SpeakerVoice>), String> {
    let config = OfflineSpeakerDiarizationConfig {
        segmentation: OfflineSpeakerSegmentationModelConfig {
            pyannote: OfflineSpeakerSegmentationPyannoteModelConfig {
                model: Some(segmentation_model.to_string()),
                ..Default::default()
            },
            num_threads: threads,
            ..Default::default()
        },
        embedding: SpeakerEmbeddingExtractorConfig {
            model: Some(embedding_model.to_string()),
            num_threads: threads,
            ..Default::default()
        },
        clustering: FastClusteringConfig::default(),
        ..Default::default()
    };
    let diarizer = OfflineSpeakerDiarization::create(&config)
        .ok_or_else(|| "the speaker models could not be loaded".to_string())?;
    let result = diarizer
        .process(pcm)
        .ok_or_else(|| "the recording could not be split by speaker".to_string())?;
    let turns = number_by_first_appearance(
        result
            .sort_by_start_time()
            .into_iter()
            .map(|segment| (segment.start, segment.end, segment.speaker)),
    );
    let Some(extractor) = sherpa_onnx::SpeakerEmbeddingExtractor::create(&config.embedding) else {
        return Ok((turns, Vec::new()));
    };
    // The clustering often gives one person several numbers. Speakers whose voices are
    // alike are made one, and the voices are taken again from the joined speech.
    let voices = voices_of(&extractor, pcm, &turns);
    let spoke = |speaker: u32| -> u64 {
        turns
            .iter()
            .filter(|turn| turn.speaker == speaker)
            .map(|turn| turn.end_ms - turn.start_ms)
            .sum()
    };
    let mut by_length: Vec<&SpeakerVoice> = voices.iter().collect();
    by_length.sort_by_key(|voice| std::cmp::Reverse(spoke(voice.speaker)));
    let pairs = same_people(&by_length);
    if pairs.is_empty() {
        return Ok((turns, voices));
    }
    let turns = joined(&turns, &pairs);
    let voices = voices_of(&extractor, pcm, &turns);
    Ok((turns, voices))
}

/// Two speakers whose voices are at least this alike are one person. Measured on real
/// meetings: one person's clusters score 0.7 to 0.94, different people 0.37 to 0.6.
const ONE_PERSON: f32 = 0.68;

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

/// Which speakers are one person, as (the one who spoke more, the other). `voices` has
/// the one who spoke most first. A speaker is only ever compared with speakers kept as
/// people of their own, so a chain of near matches cannot join two different people.
fn same_people(voices: &[&SpeakerVoice]) -> Vec<(u32, u32)> {
    let mut people: Vec<&SpeakerVoice> = Vec::new();
    let mut pairs = Vec::new();
    for voice in voices {
        let closest = people
            .iter()
            .map(|person| (alike(&person.embedding, &voice.embedding), person.speaker))
            .max_by(|a, b| a.0.total_cmp(&b.0));
        match closest {
            Some((score, person)) if score >= ONE_PERSON => pairs.push((person, voice.speaker)),
            _ => people.push(voice),
        }
    }
    pairs
}

/// The turns with every pair of speakers made one, numbered again in order of appearance.
fn joined(turns: &[SpeakerTurn], pairs: &[(u32, u32)]) -> Vec<SpeakerTurn> {
    let mut stands_for: std::collections::HashMap<u32, u32> = std::collections::HashMap::new();
    let root = |map: &std::collections::HashMap<u32, u32>, mut speaker: u32| {
        while let Some(next) = map.get(&speaker) {
            speaker = *next;
        }
        speaker
    };
    for (a, b) in pairs {
        let (a, b) = (root(&stands_for, *a), root(&stands_for, *b));
        if a != b {
            stands_for.insert(a.max(b), a.min(b));
        }
    }
    number_by_first_appearance(turns.iter().map(|turn| {
        (
            turn.start_ms as f32 / 1000.0,
            turn.end_ms as f32 / 1000.0,
            root(&stands_for, turn.speaker) as i32,
        )
    }))
}

/// The most of one speaker's speech that goes into their voice.
const VOICE_SAMPLES: usize = 60 * 16_000;

fn embed(extractor: &sherpa_onnx::SpeakerEmbeddingExtractor, pcm: &[f32]) -> Option<Vec<f32>> {
    let stream = extractor.create_stream()?;
    stream.accept_waveform(16_000, pcm);
    stream.input_finished();
    extractor
        .is_ready(&stream)
        .then(|| extractor.compute(&stream))?
}

/// One voice per speaker, from up to a minute of what they said.
fn voices_of(
    extractor: &sherpa_onnx::SpeakerEmbeddingExtractor,
    pcm: &[f32],
    turns: &[SpeakerTurn],
) -> Vec<SpeakerVoice> {
    let mut speakers: Vec<u32> = turns.iter().map(|turn| turn.speaker).collect();
    speakers.sort_unstable();
    speakers.dedup();
    speakers
        .into_iter()
        .filter(|speaker| *speaker != 0)
        .filter_map(|speaker| {
            let mut said = Vec::new();
            for turn in turns.iter().filter(|turn| turn.speaker == speaker) {
                let at = |ms: u64| ((ms * 16) as usize).min(pcm.len());
                let room = VOICE_SAMPLES.saturating_sub(said.len());
                let (from, to) = (at(turn.start_ms), at(turn.end_ms));
                said.extend_from_slice(&pcm[from..to.min(from + room)]);
            }
            (said.len() >= SHORTEST_VOICE_SAMPLES)
                .then(|| embed(extractor, &said))
                .flatten()
                .map(|embedding| SpeakerVoice { speaker, embedding })
        })
        .collect()
}

/// Keeps speakers apart across the windows of a recording that is still running: each
/// window is split on its own, then every voice in it is matched against the voices
/// heard so far, so "speaker 2" stays the same person from one window to the next.
pub struct LiveSpeakers {
    diarizer: OfflineSpeakerDiarization,
    extractor: sherpa_onnx::SpeakerEmbeddingExtractor,
    known: sherpa_onnx::SpeakerEmbeddingManager,
}

/// How alike two voices must be to count as one person.
const SAME_VOICE: f32 = 0.45;
/// A voice heard for less than this is too short to recognize or remember.
const SHORTEST_VOICE_SAMPLES: usize = 16_000;
/// A voice that matches nobody is taken for a new person only after this much speech in
/// one window. Shorter stretches are mostly known people the model failed to recognize.
const NEW_VOICE_SAMPLES: usize = 4 * 16_000;

impl LiveSpeakers {
    pub fn open(
        segmentation_model: &str,
        embedding_model: &str,
        threads: i32,
    ) -> Result<Self, String> {
        let embedding = SpeakerEmbeddingExtractorConfig {
            model: Some(embedding_model.to_string()),
            num_threads: threads,
            ..Default::default()
        };
        let config = OfflineSpeakerDiarizationConfig {
            segmentation: OfflineSpeakerSegmentationModelConfig {
                pyannote: OfflineSpeakerSegmentationPyannoteModelConfig {
                    model: Some(segmentation_model.to_string()),
                    ..Default::default()
                },
                num_threads: threads,
                ..Default::default()
            },
            embedding: embedding.clone(),
            clustering: FastClusteringConfig::default(),
            ..Default::default()
        };
        let missing = || "the speaker models could not be loaded".to_string();
        let diarizer = OfflineSpeakerDiarization::create(&config).ok_or_else(missing)?;
        let extractor =
            sherpa_onnx::SpeakerEmbeddingExtractor::create(&embedding).ok_or_else(missing)?;
        let known =
            sherpa_onnx::SpeakerEmbeddingManager::create(extractor.dim()).ok_or_else(missing)?;
        Ok(Self {
            diarizer,
            extractor,
            known,
        })
    }

    fn voice(&self, pcm: &[f32]) -> Option<Vec<f32>> {
        embed(&self.extractor, pcm)
    }

    /// The turns of one window, with times counted from the window's start and speakers
    /// numbered across the whole recording. A voice too short to tell is speaker 0.
    pub fn window(&mut self, pcm: &[f32]) -> Result<(Vec<SpeakerTurn>, Vec<SpeakerVoice>), String> {
        let mut voices = Vec::new();
        let result = self
            .diarizer
            .process(pcm)
            .ok_or_else(|| "the recording could not be split by speaker".to_string())?;
        let segments = result.sort_by_start_time();
        let mut names: std::collections::HashMap<i32, u32> = std::collections::HashMap::new();
        let mut locals: Vec<i32> = segments.iter().map(|segment| segment.speaker).collect();
        locals.dedup();
        locals.sort_unstable();
        locals.dedup();
        for local in locals {
            let mut voice = Vec::new();
            for segment in segments.iter().filter(|segment| segment.speaker == local) {
                let at = |seconds: f32| ((seconds.max(0.0) * 16_000.0) as usize).min(pcm.len());
                voice.extend_from_slice(&pcm[at(segment.start)..at(segment.end)]);
            }
            let number = if voice.len() < SHORTEST_VOICE_SAMPLES {
                0
            } else {
                match self.voice(&voice) {
                    Some(embedding) => {
                        let number = match self.known.search(&embedding, SAME_VOICE) {
                            Some(name) => name.parse().unwrap_or(0),
                            None if voice.len() < NEW_VOICE_SAMPLES => 0,
                            None => {
                                let next = self.known.num_speakers() as u32 + 1;
                                self.known.add(&next.to_string(), &embedding);
                                next
                            }
                        };
                        if number != 0 {
                            voices.push(SpeakerVoice {
                                speaker: number,
                                embedding,
                            });
                        }
                        number
                    }
                    None => 0,
                }
            };
            names.insert(local, number);
        }
        let turns = segments
            .into_iter()
            .map(|segment| SpeakerTurn {
                start_ms: (segment.start.max(0.0) * 1000.0) as u64,
                end_ms: (segment.end.max(0.0) * 1000.0) as u64,
                speaker: names.get(&segment.speaker).copied().unwrap_or(0),
            })
            .collect();
        Ok((turns, voices))
    }
}

/// The library numbers speakers arbitrarily. A reader expects the first voice to be 1.
fn number_by_first_appearance(segments: impl Iterator<Item = (f32, f32, i32)>) -> Vec<SpeakerTurn> {
    let mut seen: Vec<i32> = Vec::new();
    segments
        .map(|(start, end, speaker)| {
            let position = seen.iter().position(|known| *known == speaker);
            let number = position.unwrap_or_else(|| {
                seen.push(speaker);
                seen.len() - 1
            }) + 1;
            SpeakerTurn {
                start_ms: (start.max(0.0) * 1000.0) as u64,
                end_ms: (end.max(0.0) * 1000.0) as u64,
                speaker: number as u32,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_voice_heard_is_speaker_one() {
        let turns = number_by_first_appearance(
            [(0.0, 1.5, 7), (1.5, 3.0, 2), (3.0, 4.0, 7), (4.0, 5.0, 0)].into_iter(),
        );
        assert_eq!(
            turns.iter().map(|turn| turn.speaker).collect::<Vec<_>>(),
            vec![1, 2, 1, 3]
        );
        assert_eq!((turns[1].start_ms, turns[1].end_ms), (1500, 3000));
    }

    /// Prints how alike the voices of two recordings are, `OT_WAV` and `OT_WAV2`, with
    /// the seconds each speaker has: the numbers a name-by-voice threshold is set from.
    #[test]
    fn speakers_who_sound_alike_become_one() {
        let turn = |start_ms, speaker| SpeakerTurn {
            start_ms,
            end_ms: start_ms + 1000,
            speaker,
        };
        let turns = [
            turn(0, 1),
            turn(1000, 2),
            turn(2000, 3),
            turn(3000, 4),
            turn(4000, 2),
        ];
        let numbers: Vec<u32> = joined(&turns, &[(1, 3), (3, 4)])
            .iter()
            .map(|turn| turn.speaker)
            .collect();
        assert_eq!(numbers, [1, 2, 1, 1, 2]);
        let voice = |speaker, embedding: [f32; 2]| SpeakerVoice {
            speaker,
            embedding: embedding.to_vec(),
        };
        // 2 is close to 1, and 3 is close to 2 but not to 1: 3 stays a person of its own.
        let voices = [
            voice(1, [1.0, 0.0]),
            voice(2, [0.8, 0.6]),
            voice(3, [0.3, 0.95]),
        ];
        let order: Vec<&SpeakerVoice> = voices.iter().collect();
        assert_eq!(same_people(&order), [(1, 2)]);
        assert!((alike(&[1.0, 0.0], &[3.0, 0.0]) - 1.0).abs() < 1e-6);
        assert_eq!(alike(&[0.0, 0.0], &[1.0, 0.0]), 0.0);
    }

    #[test]
    #[ignore]
    fn compares_the_voices_of_two_recordings() {
        let voices = |name: &str| {
            let wav = std::fs::read(std::env::var(name).unwrap()).unwrap();
            let pcm = crate::eval::decode_wav(&wav).unwrap();
            let (turns, voices) = diarize(
                &std::env::var("OT_SEG").unwrap(),
                &std::env::var("OT_EMB").unwrap(),
                &pcm,
                4,
            )
            .unwrap();
            let seconds = |speaker: u32| {
                turns
                    .iter()
                    .filter(|turn| turn.speaker == speaker)
                    .map(|turn| turn.end_ms - turn.start_ms)
                    .sum::<u64>()
                    / 1000
            };
            voices
                .into_iter()
                .map(|voice| (seconds(voice.speaker), voice.embedding))
                .filter(|(seconds, _)| *seconds >= 20)
                .collect::<Vec<_>>()
        };
        let alike = |a: &[f32], b: &[f32]| {
            let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
            let size = |v: &[f32]| v.iter().map(|x| x * x).sum::<f32>().sqrt();
            dot / (size(a) * size(b))
        };
        let (first, second) = (voices("OT_WAV"), voices("OT_WAV2"));
        for (seconds, voice) in &first {
            let row: Vec<String> = second
                .iter()
                .map(|(other, theirs)| format!("{:.2} ({other}s)", alike(voice, theirs)))
                .collect();
            println!("voice with {seconds}s: {}", row.join("  "));
        }
    }

    /// Same needs as the test below, plus `OT_WINDOW` seconds per window.
    #[test]
    #[ignore]
    fn keeps_speakers_apart_window_by_window() {
        let wav = std::fs::read(std::env::var("OT_WAV").unwrap()).unwrap();
        let pcm = crate::eval::decode_wav(&wav).unwrap();
        let window: usize = std::env::var("OT_WINDOW").unwrap().parse().unwrap();
        let mut live = LiveSpeakers::open(
            &std::env::var("OT_SEG").unwrap(),
            &std::env::var("OT_EMB").unwrap(),
            4,
        )
        .unwrap();
        let started = std::time::Instant::now();
        for (index, piece) in pcm.chunks(window * 16_000).enumerate() {
            for turn in live.window(piece).unwrap().0 {
                let offset = (index * window * 1000) as u64;
                println!(
                    "{:>6} - {:>6} speaker {}",
                    offset + turn.start_ms,
                    offset + turn.end_ms,
                    turn.speaker
                );
            }
        }
        println!("took {:.1}s", started.elapsed().as_secs_f32());
    }

    /// Needs the two models and a recording. Run with
    /// `OT_SEG=<onnx> OT_EMB=<onnx> OT_WAV=<16 kHz mono 16-bit wav> cargo test real_recording -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn splits_a_real_recording_by_speaker() {
        let wav = std::fs::read(std::env::var("OT_WAV").unwrap()).unwrap();
        let pcm = crate::eval::decode_wav(&wav).unwrap();
        let started = std::time::Instant::now();
        let turns = diarize(
            &std::env::var("OT_SEG").unwrap(),
            &std::env::var("OT_EMB").unwrap(),
            &pcm,
            4,
        )
        .unwrap()
        .0;
        println!(
            "audio {:.1}s took {:.1}s",
            pcm.len() as f32 / 16_000.0,
            started.elapsed().as_secs_f32()
        );
        for turn in &turns {
            println!(
                "{:>6} - {:>6} speaker {}",
                turn.start_ms, turn.end_ms, turn.speaker
            );
        }
    }
}
