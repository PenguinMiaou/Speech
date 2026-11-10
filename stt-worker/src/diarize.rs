//! Tells who spoke when in a finished recording, so a meeting can be written out by speaker.

use ot_stt_protocol::SpeakerTurn;
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
) -> Result<Vec<SpeakerTurn>, String> {
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
    Ok(number_by_first_appearance(
        result
            .sort_by_start_time()
            .into_iter()
            .map(|segment| (segment.start, segment.end, segment.speaker)),
    ))
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
        let stream = self.extractor.create_stream()?;
        stream.accept_waveform(16_000, pcm);
        stream.input_finished();
        self.extractor
            .is_ready(&stream)
            .then(|| self.extractor.compute(&stream))?
    }

    /// The turns of one window, with times counted from the window's start and speakers
    /// numbered across the whole recording. A voice too short to tell is speaker 0.
    pub fn window(&mut self, pcm: &[f32]) -> Result<Vec<SpeakerTurn>, String> {
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
                    Some(embedding) => match self.known.search(&embedding, SAME_VOICE) {
                        Some(name) => name.parse().unwrap_or(0),
                        None => {
                            let next = self.known.num_speakers() as u32 + 1;
                            self.known.add(&next.to_string(), &embedding);
                            next
                        }
                    },
                    None => 0,
                }
            };
            names.insert(local, number);
        }
        Ok(segments
            .into_iter()
            .map(|segment| SpeakerTurn {
                start_ms: (segment.start.max(0.0) * 1000.0) as u64,
                end_ms: (segment.end.max(0.0) * 1000.0) as u64,
                speaker: names.get(&segment.speaker).copied().unwrap_or(0),
            })
            .collect())
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
            for turn in live.window(piece).unwrap() {
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
        .unwrap();
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
