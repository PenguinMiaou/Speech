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

/// Recognizes a whole file piece by piece. `progress` hears how many pieces are done.
pub fn transcribe(path: &Path, mut progress: impl FnMut(usize, usize)) -> Result<String, String> {
    use crate::local_stt::catalog;
    let model = catalog::installed_path(catalog::DEFAULT_LOCAL_MODEL_ID)
        .or_else(|| {
            catalog::entries()
                .iter()
                .find_map(|entry| catalog::installed_path(entry.id))
        })
        .ok_or_else(|| "local_model_not_installed".to_string())?;
    let supervisor =
        crate::local_stt::global().ok_or_else(|| "local_engine_unavailable".to_string())?;
    let pcm = decode(path)?;
    let cut = pieces(&pcm);
    let mut text = Vec::new();
    progress(0, cut.len());
    for (index, piece) in cut.iter().enumerate() {
        let said = supervisor
            .transcribe(&model, &pcm[piece.clone()], Vec::new(), None)
            .map_err(|error| error.to_string())?;
        let said = said.trim();
        if !said.is_empty() {
            text.push(said.to_string());
        }
        progress(index + 1, cut.len());
    }
    Ok(text.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

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
