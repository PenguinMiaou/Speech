//! When each word of a stretch of sound is said, heard by a small recognizer that tells
//! the time of every token it writes.

use sherpa_onnx::{OfflineRecognizer, OfflineRecognizerConfig, OfflineSenseVoiceModelConfig};

/// A recognizer that tells when each of its tokens is said.
pub struct Timer {
    recognizer: OfflineRecognizer,
}

impl Timer {
    pub fn load(model: &str, tokens: &str, threads: i32) -> Result<Self, String> {
        let mut config = OfflineRecognizerConfig::default();
        config.model_config.sense_voice = OfflineSenseVoiceModelConfig {
            model: Some(model.into()),
            language: Some("auto".into()),
            use_itn: false,
        };
        config.model_config.tokens = Some(tokens.into());
        config.model_config.num_threads = threads;
        let recognizer =
            OfflineRecognizer::create(&config).ok_or("the timing model could not be loaded")?;
        Ok(Self { recognizer })
    }

    /// The tokens heard in `pcm` (16 kHz mono) with the second each begins at.
    pub fn tokens(&self, pcm: &[f32]) -> Vec<(String, f32)> {
        let stream = self.recognizer.create_stream();
        stream.accept_waveform(16_000, pcm);
        self.recognizer.decode(&stream);
        let Some(result) = stream.get_result() else {
            return Vec::new();
        };
        let times = result.timestamps.unwrap_or_default();
        result.tokens.into_iter().zip(times).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `OT_SV=<dir with model.int8.onnx and tokens.txt> OT_WAV=<16 kHz mono wav>
    /// [OT_SHOW=1] cargo test --release times_a_real -- --ignored --nocapture`.
    /// Prints the words only when OT_SHOW is set.
    #[test]
    #[ignore]
    fn times_a_real_recording() {
        let dir = std::env::var("OT_SV").unwrap();
        let wav = std::fs::read(std::env::var("OT_WAV").unwrap()).unwrap();
        let pcm = crate::eval::decode_wav(&wav).unwrap();
        let started = std::time::Instant::now();
        let timer = Timer::load(
            &format!("{dir}/model.int8.onnx"),
            &format!("{dir}/tokens.txt"),
            4,
        )
        .unwrap();
        println!("loaded in {:.1}s", started.elapsed().as_secs_f32());
        let piece = 30 * 16_000;
        let started = std::time::Instant::now();
        let mut all = 0;
        let mut backwards = 0;
        for (index, chunk) in pcm.chunks(piece).take(20).enumerate() {
            let tokens = timer.tokens(chunk);
            all += tokens.len();
            backwards += tokens
                .windows(2)
                .filter(|pair| pair[1].1 < pair[0].1)
                .count();
            let last = tokens.last().map(|token| token.1).unwrap_or(0.0);
            println!("piece {index}: {} tokens, last at {last:.2}s", tokens.len());
            if std::env::var("OT_SHOW").is_ok() {
                for (token, at) in &tokens {
                    print!("{token}@{at:.2} ");
                }
                println!();
            }
        }
        let heard = pcm.len().min(20 * piece) as f32 / 16_000.0;
        println!(
            "audio {heard:.0}s took {:.1}s, {all} tokens, {backwards} out of order",
            started.elapsed().as_secs_f32()
        );
    }
}
