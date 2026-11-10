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
