//! Keeps the enrolled speaker's voice and removes other talkers, while the recording is
//! still running, so that little work is left when it ends.

use sherpa_onnx::{
    OfflineSpeechDenoiserModelConfig, OfflineSpeechDenoiserVoxLockModelConfig,
    OnlineSpeechDenoiser, OnlineSpeechDenoiserConfig,
};

pub const SAMPLE_RATE: i32 = 16_000;
/// Audio is handed to the model in pieces of this length. Shorter pieces cost far more
/// processing per second of audio.
const FRAME_SAMPLES: usize = 5_120;

/// The model behind [`Isolator`], so the buffering can be tested without one.
pub trait Denoise {
    fn run(&mut self, frame: &[f32]) -> Vec<f32>;
    fn flush(&mut self) -> Vec<f32>;
    /// Forgets the recording just finished, keeping the enrolled voice.
    fn reset(&mut self);
}

pub struct VoxLock(OnlineSpeechDenoiser);

impl VoxLock {
    pub fn open(
        enroll_model: &str,
        chunk_model: &str,
        voice_sample: &str,
        threads: i32,
    ) -> Result<Self, String> {
        let config = OnlineSpeechDenoiserConfig {
            model: OfflineSpeechDenoiserModelConfig {
                voxlock: OfflineSpeechDenoiserVoxLockModelConfig {
                    enroll_model: Some(enroll_model.to_string()),
                    chunk_model: Some(chunk_model.to_string()),
                    voice_sample: Some(voice_sample.to_string()),
                },
                num_threads: threads,
                ..Default::default()
            },
        };
        OnlineSpeechDenoiser::create(&config)
            .map(Self)
            .ok_or_else(|| "the voice isolation model could not be loaded".to_string())
    }
}

impl Denoise for VoxLock {
    fn run(&mut self, frame: &[f32]) -> Vec<f32> {
        self.0.run(frame, SAMPLE_RATE).samples
    }

    fn flush(&mut self) -> Vec<f32> {
        self.0.flush().samples
    }

    fn reset(&mut self) {
        self.0.reset();
    }
}

/// Takes audio as it is recorded, in pieces of any size, and gives back the isolated voice.
pub struct Isolator<D: Denoise> {
    model: D,
    waiting: Vec<f32>,
    isolated: Vec<f32>,
}

impl<D: Denoise> Isolator<D> {
    pub fn new(model: D) -> Self {
        Self {
            model,
            waiting: Vec::new(),
            isolated: Vec::new(),
        }
    }

    pub fn feed(&mut self, samples: &[f32]) {
        self.waiting.extend_from_slice(samples);
        let whole = self.waiting.len() / FRAME_SAMPLES * FRAME_SAMPLES;
        for frame in self.waiting[..whole].chunks(FRAME_SAMPLES) {
            self.isolated.extend(self.model.run(frame));
        }
        self.waiting.drain(..whole);
    }

    /// Returns the isolated recording, and the model ready for the next one.
    pub fn finish(mut self) -> (Vec<f32>, D) {
        if !self.waiting.is_empty() {
            let rest = std::mem::take(&mut self.waiting);
            self.isolated.extend(self.model.run(&rest));
        }
        self.isolated.extend(self.model.flush());
        self.model.reset();
        (self.isolated, self.model)
    }
}

/// How much of the recording's loudness is left after isolation: about 1 when only the
/// enrolled voice was there, lower the more was removed.
pub fn energy_kept(recorded: &[f32], isolated: &[f32]) -> f32 {
    let rms = |audio: &[f32]| {
        (audio.iter().map(|s| s * s).sum::<f32>() / audio.len().max(1) as f32).sqrt()
    };
    let length = recorded.len().min(isolated.len());
    let before = rms(&recorded[..length]);
    if before <= f32::EPSILON {
        return 1.0;
    }
    rms(&isolated[..length]) / before
}

/// `isolate --enroll-model <onnx> --chunk-model <onnx> --voice <wav> --in <wav> --out <wav>`:
/// runs one file through the isolator the way a recording is, and prints the timing.
pub fn run_file(args: &[String]) -> Result<(), String> {
    let value = |name: &str| {
        args.iter()
            .position(|arg| arg == name)
            .and_then(|at| args.get(at + 1))
            .cloned()
            .ok_or_else(|| format!("missing {name}"))
    };
    let input = sherpa_onnx::Wave::read(&value("--in")?).ok_or("the input could not be read")?;
    if input.sample_rate() != SAMPLE_RATE {
        return Err("the input must be 16 kHz".into());
    }
    let threads = value("--threads")
        .ok()
        .and_then(|n| n.parse().ok())
        .unwrap_or(1);
    let started = std::time::Instant::now();
    let model = VoxLock::open(
        &value("--enroll-model")?,
        &value("--chunk-model")?,
        &value("--voice")?,
        threads,
    )?;
    let loaded = started.elapsed();
    let mut isolator = Isolator::new(model);
    let mut slowest = std::time::Duration::ZERO;
    // 512 samples at a time, the size a microphone callback delivers after resampling.
    for piece in input.samples().chunks(512) {
        let piece_started = std::time::Instant::now();
        isolator.feed(piece);
        slowest = slowest.max(piece_started.elapsed());
    }
    let finish_started = std::time::Instant::now();
    let (isolated, model) = isolator.finish();
    let finish = finish_started.elapsed();
    // The app keeps the model between recordings, so a second pass must give the same audio.
    let mut again = Isolator::new(model);
    again.feed(input.samples());
    let same_again = again.finish().0 == isolated;
    let audio = input.samples().len() as f64 / SAMPLE_RATE as f64;
    println!(
        "audio={audio:.1}s load={:.2}s processing={:.2}s ({:.2}x) slowest_frame={}ms finish={}ms out_samples={} in_samples={} kept={:.2} same_on_reuse={same_again}",
        loaded.as_secs_f64(),
        (started.elapsed() - loaded).as_secs_f64(),
        (started.elapsed() - loaded).as_secs_f64() / audio,
        slowest.as_millis(),
        finish.as_millis(),
        isolated.len(),
        input.samples().len(),
        energy_kept(input.samples(), &isolated),
    );
    if !sherpa_onnx::write(&value("--out")?, &isolated, SAMPLE_RATE) {
        return Err("the output could not be written".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Passes audio through and records the size of every piece it was given.
    #[derive(Default)]
    struct Recording {
        frames: Vec<usize>,
        resets: usize,
    }

    impl Denoise for Recording {
        fn run(&mut self, frame: &[f32]) -> Vec<f32> {
            self.frames.push(frame.len());
            frame.to_vec()
        }

        fn flush(&mut self) -> Vec<f32> {
            vec![9.0]
        }

        fn reset(&mut self) {
            self.resets += 1;
        }
    }

    #[test]
    fn the_share_of_loudness_kept_is_one_for_untouched_audio_and_falls_as_more_is_removed() {
        let recorded = vec![0.5f32; 1_000];
        assert!((energy_kept(&recorded, &recorded) - 1.0).abs() < 1e-6);
        let halved: Vec<f32> = recorded.iter().map(|s| s * 0.5).collect();
        assert!((energy_kept(&recorded, &halved) - 0.5).abs() < 1e-6);
        assert_eq!(energy_kept(&[0.0; 10], &[0.0; 10]), 1.0);
    }

    #[test]
    fn audio_arriving_in_small_pieces_reaches_the_model_in_whole_frames() {
        let mut isolator = Isolator::new(Recording::default());
        let audio: Vec<f32> = (0..12_000).map(|n| n as f32).collect();
        for piece in audio.chunks(512) {
            isolator.feed(piece);
        }
        let (isolated, model) = isolator.finish();
        assert_eq!(model.frames, vec![5_120, 5_120, 1_760]);
        assert_eq!(model.resets, 1);
        assert_eq!(isolated[..12_000], audio[..]);
        assert_eq!(isolated[12_000..], [9.0]);
    }

    #[test]
    fn a_recording_shorter_than_one_frame_is_still_processed() {
        let mut isolator = Isolator::new(Recording::default());
        isolator.feed(&[0.5; 100]);
        let (isolated, model) = isolator.finish();
        assert_eq!(isolated.len(), 101);
        assert_eq!(model.frames, vec![100]);
    }
}
