//! Turns an audio or video file into text with the speech model on this machine.
//! The file is decoded, brought to the model's sample rate and cut at pauses into
//! pieces short enough to recognize one at a time.

use std::ops::Range;
use std::path::Path;

const RATE: u32 = 16_000;
/// Longest piece handed to the model, in seconds.
const PIECE_SECS: usize = 30;
/// How far back from the end of a piece a pause is looked for.
const SEARCH_SECS: usize = 6;
const FRAME: usize = RATE as usize / 5;

/// Longest file taken, so the decoded audio fits in memory.
const LONGEST_SECS: usize = 3 * 60 * 60;

/// The whole file as one channel at the model's sample rate.
pub fn decode(path: &Path) -> Result<Vec<f32>, String> {
    use symphonia::core::audio::SampleBuffer;
    use symphonia::core::codecs::{DecoderOptions, CODEC_TYPE_NULL};
    use symphonia::core::errors::Error;
    use symphonia::core::formats::FormatOptions;
    use symphonia::core::io::MediaSourceStream;
    use symphonia::core::meta::MetadataOptions;
    use symphonia::core::probe::Hint;

    let file = std::fs::File::open(path).map_err(|_| "file_unreadable".to_string())?;
    let stream = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(extension) = path.extension().and_then(|extension| extension.to_str()) {
        hint.with_extension(extension);
    }
    let mut format = symphonia::default::get_probe()
        .format(
            &hint,
            stream,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .map_err(|_| "file_not_audio".to_string())?
        .format;
    let track = format
        .tracks()
        .iter()
        .find(|track| track.codec_params.codec != CODEC_TYPE_NULL)
        .ok_or_else(|| "file_not_audio".to_string())?;
    let track_id = track.id;
    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .map_err(|_| "file_not_audio".to_string())?;

    let mut pcm = Vec::new();
    let mut resampler: Option<(u32, crate::audio::resample::Resampler)> = None;
    loop {
        let packet = match format.next_packet() {
            Ok(packet) => packet,
            // The end of the file arrives as an error.
            Err(Error::IoError(_)) | Err(Error::ResetRequired) => break,
            Err(_) => return Err("file_not_audio".to_string()),
        };
        if packet.track_id() != track_id {
            continue;
        }
        let decoded = match decoder.decode(&packet) {
            Ok(decoded) => decoded,
            // One damaged packet is skipped; the rest of the file still counts.
            Err(Error::DecodeError(_)) => continue,
            Err(_) => break,
        };
        let spec = *decoded.spec();
        let channels = spec.channels.count().max(1);
        let mut samples = SampleBuffer::<f32>::new(decoded.capacity() as u64, spec);
        samples.copy_interleaved_ref(decoded);
        let mono: Vec<f32> = samples
            .samples()
            .chunks(channels)
            .map(|frame| frame.iter().sum::<f32>() / frame.len() as f32)
            .collect();
        if resampler.as_ref().map(|(rate, _)| *rate) != Some(spec.rate) {
            resampler = Some((
                spec.rate,
                crate::audio::resample::Resampler::new(spec.rate, RATE),
            ));
        }
        if let Some((_, resampler)) = resampler.as_mut() {
            pcm.extend(
                resampler
                    .process(&mono)
                    .into_iter()
                    .map(|sample| sample.clamp(-1.0, 1.0)),
            );
        }
        if pcm.len() > LONGEST_SECS * RATE as usize {
            return Err("file_too_long".to_string());
        }
    }
    if pcm.is_empty() {
        return Err("file_not_audio".to_string());
    }
    Ok(pcm)
}

/// Where to cut the audio: every piece is short enough for the model, and each cut sits
/// in the quietest moment near the end of its piece.
pub fn pieces(pcm: &[f32]) -> Vec<Range<usize>> {
    let longest = PIECE_SECS * RATE as usize;
    let search = SEARCH_SECS * RATE as usize;
    let mut cut = Vec::new();
    let mut start = 0;
    while pcm.len() - start > longest {
        let end = start + longest;
        let quietest = (end - search..end - FRAME + 1)
            .step_by(FRAME / 2)
            .min_by(|a, b| {
                let energy =
                    |from: &usize| -> f32 { pcm[*from..*from + FRAME].iter().map(|s| s * s).sum() };
                energy(a).total_cmp(&energy(b))
            })
            .unwrap_or(end - FRAME);
        let at = quietest + FRAME / 2;
        cut.push(start..at);
        start = at;
    }
    if start < pcm.len() {
        cut.push(start..pcm.len());
    }
    cut
}

/// What the caller wants from a transcription.
#[derive(Debug, Clone, Default)]
pub struct Options {
    /// Write the text out by speaker, each named with this word and a number.
    pub speaker_label: Option<String>,
    /// The model to recognize with. The dictation model when absent or not installed.
    pub model_id: Option<String>,
    /// Words from the user's dictionary the model should expect.
    pub vocabulary: Vec<String>,
}

/// What a caller hears while a recording is being worked on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    /// Finding who spoke when. It has no steps to count.
    Speakers,
    Text {
        done: usize,
        total: usize,
    },
}

/// The pieces to recognize, each with the index of the turn it belongs to.
pub fn plan(
    pcm: &[f32],
    turns: Option<&[crate::speakers::SpeakerTurn]>,
) -> Vec<(Option<usize>, Range<usize>)> {
    let Some(turns) = turns else {
        return pieces(pcm).into_iter().map(|piece| (None, piece)).collect();
    };
    turns
        .iter()
        .enumerate()
        .flat_map(|(index, turn)| {
            let span = crate::speakers::samples(turn, pcm.len());
            let from = span.start;
            pieces(&pcm[span])
                .into_iter()
                .map(move |piece| (Some(index), from + piece.start..from + piece.end))
        })
        .collect()
}

pub(crate) fn joined(parts: &[&str]) -> String {
    let mut text = String::new();
    for part in parts {
        let latin = |c: Option<char>| c.is_some_and(|c| c.is_ascii() && !c.is_ascii_whitespace());
        if latin(text.chars().last()) && latin(part.chars().next()) {
            text.push(' ');
        }
        text.push_str(part);
    }
    text
}

/// The recognized pieces as the final text.
pub fn assemble(
    said: &[(Option<usize>, String)],
    turns: Option<&[crate::speakers::SpeakerTurn]>,
    speaker_label: &str,
) -> String {
    let Some(turns) = turns else {
        return said
            .iter()
            .map(|(_, text)| text.as_str())
            .collect::<Vec<_>>()
            .join("\n");
    };
    let by_turn: Vec<_> = turns
        .iter()
        .enumerate()
        .map(|(index, turn)| {
            let parts: Vec<&str> = said
                .iter()
                .filter(|(of, _)| *of == Some(index))
                .map(|(_, text)| text.as_str())
                .collect();
            (*turn, joined(&parts))
        })
        .collect();
    crate::speakers::transcript(&by_turn, speaker_label)
}

pub(crate) fn model_path(wanted: Option<&str>) -> Option<std::path::PathBuf> {
    use crate::local_stt::catalog;
    wanted
        .and_then(catalog::installed_path)
        .or_else(|| catalog::installed_path(catalog::DEFAULT_LOCAL_MODEL_ID))
        .or_else(|| {
            catalog::entries()
                .iter()
                .find_map(|entry| catalog::installed_path(entry.id))
        })
}

/// Recognizes a whole file. See [`transcribe_pcm`].
pub fn transcribe(
    path: &Path,
    options: &Options,
    progress: impl FnMut(Stage),
) -> Result<String, String> {
    transcribe_pcm(&decode(path)?, options, progress)
}

/// Recognizes a recording piece by piece, by speaker when asked and the speaker models
/// are installed.
pub fn transcribe_pcm(
    pcm: &[f32],
    options: &Options,
    progress: impl FnMut(Stage),
) -> Result<String, String> {
    let model = model_path(options.model_id.as_deref())
        .ok_or_else(|| "local_model_not_installed".to_string())?;
    let supervisor =
        crate::local_stt::global().ok_or_else(|| "local_engine_unavailable".to_string())?;
    let speakers = match &options.speaker_label {
        Some(label) => {
            let missing = || "speaker_models_not_installed".to_string();
            let (segmentation, voice) = crate::speakers::installed_models().ok_or_else(missing)?;
            Some(SpeakerModels {
                segmentation,
                voice,
                scratch: crate::speakers::model_dir().ok_or_else(missing)?,
                label: label.clone(),
            })
        }
        None => None,
    };
    recognize(
        &supervisor,
        &model,
        speakers.as_ref(),
        &options.vocabulary,
        pcm,
        progress,
    )
}

struct SpeakerModels {
    segmentation: std::path::PathBuf,
    voice: std::path::PathBuf,
    scratch: std::path::PathBuf,
    label: String,
}

fn recognize(
    supervisor: &crate::local_stt::WorkerSupervisor,
    model: &Path,
    speakers: Option<&SpeakerModels>,
    vocabulary: &[String],
    pcm: &[f32],
    mut progress: impl FnMut(Stage),
) -> Result<String, String> {
    let turns = match speakers {
        Some(models) => {
            progress(Stage::Speakers);
            let (found, voices) = supervisor
                .diarize(&models.segmentation, &models.voice, pcm, &models.scratch)
                .map_err(|error| error.to_string())?;
            crate::voiceprints::hear(voices, true);
            Some(crate::speakers::merge_turns(&found))
        }
        None => None,
    };
    let planned = plan(pcm, turns.as_deref());
    let mut said = Vec::new();
    progress(Stage::Text {
        done: 0,
        total: planned.len(),
    });
    for (index, (turn, piece)) in planned.iter().enumerate() {
        let text = supervisor
            .transcribe(model, &pcm[piece.clone()], vocabulary.to_vec(), None)
            .map_err(|error| error.to_string())?;
        let text = text.trim();
        if !text.is_empty() {
            said.push((*turn, text.to_string()));
        }
        progress(Stage::Text {
            done: index + 1,
            total: planned.len(),
        });
    }
    Ok(assemble(
        &said,
        turns.as_deref(),
        speakers
            .map(|models| models.label.as_str())
            .unwrap_or_default(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn without_speakers_the_plan_is_the_plain_cut() {
        let pcm = tone(70.0);
        let planned = plan(&pcm, None);
        assert_eq!(planned.len(), pieces(&pcm).len());
        assert!(planned.iter().all(|(turn, _)| turn.is_none()));
    }

    #[test]
    fn with_speakers_every_piece_belongs_to_a_turn_and_stays_inside_it() {
        let pcm = tone(100.0);
        let turn = |start_ms, end_ms, speaker| crate::speakers::SpeakerTurn {
            start_ms,
            end_ms,
            speaker,
        };
        let turns = [turn(0, 10_000, 1), turn(10_000, 80_000, 2)];
        let planned = plan(&pcm, Some(&turns));
        assert_eq!(planned[0], (Some(0), 0..160_000));
        let second: Vec<_> = planned
            .iter()
            .filter(|(turn, _)| *turn == Some(1))
            .collect();
        assert!(second.len() >= 3);
        assert_eq!(second[0].1.start, 160_000);
        assert_eq!(second.last().unwrap().1.end, 1_280_000);
        assert!(second
            .iter()
            .all(|(_, piece)| piece.len() <= PIECE_SECS * RATE as usize));
    }

    #[test]
    fn pieces_join_into_one_paragraph_per_turn() {
        let turn = |start_ms, speaker| crate::speakers::SpeakerTurn {
            start_ms,
            end_ms: start_ms + 1_000,
            speaker,
        };
        let turns = [turn(0, 1), turn(5_000, 2)];
        let said = vec![
            (Some(0), "你好。".to_string()),
            (Some(1), "First part".to_string()),
            (Some(1), "second part.".to_string()),
        ];
        assert_eq!(
            assemble(&said, Some(&turns), "Speaker"),
            "Speaker 1  00:00\n你好。\n\nSpeaker 2  00:05\nFirst part second part."
        );
        let plain = vec![(None, "one".to_string()), (None, "two".to_string())];
        assert_eq!(assemble(&plain, None, "Speaker"), "one\ntwo");
    }

    /// Needs the built worker, a speech model, the two speaker models and a recording:
    /// `OT_WORKER=<bin> OT_MODEL=<gguf> OT_SEG=<onnx> OT_EMB=<onnx> OT_FILE=<audio>
    /// cargo test --lib writes_a_real_meeting -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn writes_a_real_meeting_out_by_speaker() {
        let var = |name: &str| std::path::PathBuf::from(std::env::var(name).unwrap());
        let supervisor = crate::local_stt::WorkerSupervisor::new(
            crate::local_stt::WorkerCommand {
                program: var("OT_WORKER"),
                args: vec!["serve".to_string()],
            },
            std::time::Duration::from_secs(120),
            std::time::Duration::from_secs(60),
        );
        let speakers = SpeakerModels {
            segmentation: var("OT_SEG"),
            voice: var("OT_EMB"),
            scratch: std::env::temp_dir(),
            label: "说话人".to_string(),
        };
        let pcm = decode(&var("OT_FILE")).unwrap();
        let started = std::time::Instant::now();
        let text = recognize(
            &supervisor,
            &var("OT_MODEL"),
            Some(&speakers),
            &[],
            &pcm,
            |_| {},
        )
        .unwrap();
        println!(
            "audio {:.0}s took {:.1}s\n{text}",
            pcm.len() as f32 / RATE as f32,
            started.elapsed().as_secs_f32()
        );
    }

    fn tone(seconds: f32) -> Vec<f32> {
        (0..(seconds * RATE as f32) as usize)
            .map(|i| (i as f32 * 0.05).sin() * 0.5)
            .collect()
    }

    #[test]
    fn a_short_recording_is_one_piece() {
        let pcm = tone(12.0);
        assert_eq!(pieces(&pcm), vec![0..pcm.len()]);
        assert!(pieces(&[]).is_empty());
    }

    #[test]
    fn a_long_recording_is_cut_into_pieces_that_cover_all_of_it() {
        let pcm = tone(100.0);
        let cut = pieces(&pcm);
        assert!(cut.len() >= 4);
        assert_eq!(cut[0].start, 0);
        assert_eq!(cut.last().unwrap().end, pcm.len());
        for pair in cut.windows(2) {
            assert_eq!(pair[0].end, pair[1].start);
        }
        for piece in &cut {
            assert!(piece.len() <= PIECE_SECS * RATE as usize);
            assert!(!piece.is_empty());
        }
    }

    #[test]
    fn a_cut_falls_in_a_pause_rather_than_mid_word() {
        let mut pcm = tone(50.0);
        let pause = 27 * RATE as usize..28 * RATE as usize;
        pcm[pause.clone()].fill(0.0);
        let cut = pieces(&pcm);
        assert!(pause.contains(&cut[0].end), "cut at {}", cut[0].end);
    }

    /// A 16-bit WAV, written by hand so the test needs no fixture file.
    fn wav(rate: u32, channels: u16, frames: usize) -> Vec<u8> {
        let data_len = (frames * channels as usize * 2) as u32;
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&(36 + data_len).to_le_bytes());
        bytes.extend_from_slice(b"WAVEfmt ");
        bytes.extend_from_slice(&16u32.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&channels.to_le_bytes());
        bytes.extend_from_slice(&rate.to_le_bytes());
        bytes.extend_from_slice(&(rate * channels as u32 * 2).to_le_bytes());
        bytes.extend_from_slice(&(channels * 2).to_le_bytes());
        bytes.extend_from_slice(&16u16.to_le_bytes());
        bytes.extend_from_slice(b"data");
        bytes.extend_from_slice(&data_len.to_le_bytes());
        for frame in 0..frames {
            let sample = ((frame as f32 * 0.03).sin() * 12_000.0) as i16;
            for _ in 0..channels {
                bytes.extend_from_slice(&sample.to_le_bytes());
            }
        }
        bytes
    }

    #[test]
    fn decodes_a_stereo_file_to_one_channel_at_the_model_rate() {
        let path = std::env::temp_dir().join(format!("ot-decode-{}.wav", std::process::id()));
        std::fs::write(&path, wav(44_100, 2, 44_100 * 2)).unwrap();
        let pcm = decode(&path).unwrap();
        let _ = std::fs::remove_file(&path);

        let two_seconds = 2 * RATE as usize;
        assert!(
            pcm.len().abs_diff(two_seconds) < 400,
            "{} samples",
            pcm.len()
        );
        assert!(pcm.iter().any(|sample| sample.abs() > 0.1));
        assert!(pcm.iter().all(|sample| sample.abs() <= 1.0));
    }

    #[test]
    fn says_so_for_a_file_that_holds_no_audio() {
        let path = std::env::temp_dir().join(format!("ot-decode-{}.txt", std::process::id()));
        std::fs::write(&path, b"not audio at all").unwrap();
        let result = decode(&path);
        let _ = std::fs::remove_file(&path);
        assert!(result.is_err());
        assert!(decode(Path::new("/nonexistent/file.mp3")).is_err());
    }

    /// Run with `OT_DECODE_FILE=<path> cargo test decodes_the_named_file -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn decodes_the_named_file() {
        let path = std::env::var("OT_DECODE_FILE").unwrap();
        let pcm = decode(Path::new(&path)).unwrap();
        println!(
            "seconds: {:.1} pieces: {}",
            pcm.len() as f32 / RATE as f32,
            pieces(&pcm).len()
        );
    }
}
