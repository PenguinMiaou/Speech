//! Sample-rate conversion for the microphone stream.

use std::f64::consts::PI;

/// Zero crossings of the sinc kernel kept on each side.
const ZERO_CROSSINGS: f64 = 32.0;
/// Filter cutoff as a share of the lower Nyquist frequency.
const ROLLOFF: f64 = 0.94;
/// Kaiser window shape, about 90 dB of stopband attenuation.
const KAISER_BETA: f64 = 9.0;

/// Windowed-sinc polyphase converter. One instance lives for a whole recording,
/// so callback boundaries leave no trace in the output.
pub struct Resampler {
    up: usize,
    down: usize,
    half_width: usize,
    /// `up` rows of `2 * half_width` coefficients.
    kernel: Vec<f32>,
    history: Vec<f32>,
    /// Index in `history` of the input sample at or before the next output.
    read: usize,
    /// Position of the next output between `read` and `read + 1`, in `1 / up` steps.
    phase: usize,
}

impl Resampler {
    pub fn new(from_rate: u32, to_rate: u32) -> Self {
        let divisor = gcd(from_rate, to_rate);
        let up = (to_rate / divisor) as usize;
        let down = (from_rate / divisor) as usize;
        let cutoff = ROLLOFF * (up as f64 / down as f64).min(1.0);
        let half_width = (ZERO_CROSSINGS / cutoff).ceil() as usize;

        let mut kernel = Vec::with_capacity(up * 2 * half_width);
        for phase in 0..up {
            let fraction = phase as f64 / up as f64;
            let row: Vec<f64> = (0..2 * half_width)
                .map(|tap| {
                    let offset = tap as f64 + 1.0 - half_width as f64 - fraction;
                    cutoff * sinc(cutoff * offset) * kaiser(offset / half_width as f64)
                })
                .collect();
            let sum: f64 = row.iter().sum();
            kernel.extend(row.iter().map(|value| (value / sum) as f32));
        }

        // Silence before the first sample, so output starts at time zero.
        let lead = half_width - 1;
        Self {
            up,
            down,
            half_width,
            kernel,
            history: vec![0.0; lead],
            read: lead,
            phase: 0,
        }
    }

    /// Accepts the next callback's samples and returns the output they complete.
    pub fn process(&mut self, input: &[f32]) -> Vec<f32> {
        self.history.extend_from_slice(input);
        let width = 2 * self.half_width;
        let mut output = Vec::with_capacity(input.len() * self.up / self.down + 1);

        while self.read + self.half_width < self.history.len() {
            let start = self.read + 1 - self.half_width;
            let row = &self.kernel[self.phase * width..(self.phase + 1) * width];
            let sample: f32 = row
                .iter()
                .zip(&self.history[start..start + width])
                .map(|(coefficient, value)| coefficient * value)
                .sum();
            output.push(sample);

            self.phase += self.down;
            self.read += self.phase / self.up;
            self.phase %= self.up;
        }

        let consumed = (self.read + 1)
            .saturating_sub(self.half_width)
            .min(self.history.len());
        self.history.drain(..consumed);
        self.read -= consumed;
        output
    }
}

fn gcd(a: u32, b: u32) -> u32 {
    if b == 0 {
        a
    } else {
        gcd(b, a % b)
    }
}

fn sinc(x: f64) -> f64 {
    if x.abs() < 1e-9 {
        1.0
    } else {
        (PI * x).sin() / (PI * x)
    }
}

fn kaiser(x: f64) -> f64 {
    if x.abs() >= 1.0 {
        return 0.0;
    }
    bessel_i0(KAISER_BETA * (1.0 - x * x).sqrt()) / bessel_i0(KAISER_BETA)
}

fn bessel_i0(x: f64) -> f64 {
    let mut sum = 1.0;
    let mut term = 1.0;
    for k in 1..64 {
        term *= (x / (2.0 * k as f64)).powi(2);
        sum += term;
        if term < 1e-12 * sum {
            break;
        }
    }
    sum
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::TAU;

    fn tone(frequency: f64, rate: u32, seconds: f64) -> Vec<f32> {
        (0..(rate as f64 * seconds) as usize)
            .map(|i| (0.5 * (TAU * frequency * i as f64 / rate as f64).sin()) as f32)
            .collect()
    }

    fn rms(samples: &[f32]) -> f64 {
        (samples.iter().map(|s| (*s as f64).powi(2)).sum::<f64>() / samples.len() as f64).sqrt()
    }

    /// Level of a converted tone against its input, ignoring the settling edges.
    fn gain_db(from: u32, to: u32, frequency: f64) -> f64 {
        let input = tone(frequency, from, 1.0);
        let output = Resampler::new(from, to).process(&input);
        let edge = output.len() / 10;
        20.0 * (rms(&output[edge..output.len() - edge]) / rms(&input)).log10()
    }

    #[test]
    fn speech_frequencies_keep_their_level() {
        for (from, to) in [(48_000, 16_000), (44_100, 16_000), (96_000, 16_000)] {
            for (frequency, tolerance) in
                [(300.0, 0.1), (1_000.0, 0.1), (4_000.0, 0.1), (6_500.0, 0.5)]
            {
                let gain = gain_db(from, to, frequency);
                assert!(
                    gain.abs() < tolerance,
                    "{from}->{to} at {frequency} Hz changed by {gain:.2} dB"
                );
            }
        }
    }

    #[test]
    fn sound_above_the_new_limit_does_not_fold_into_speech() {
        for (from, to) in [(48_000, 16_000), (44_100, 16_000), (96_000, 16_000)] {
            for frequency in [9_000.0, 12_000.0, 20_000.0] {
                let gain = gain_db(from, to, frequency);
                assert!(
                    gain < -60.0,
                    "{from}->{to} at {frequency} Hz is only {gain:.1} dB"
                );
            }
        }
    }

    #[test]
    fn callback_sizes_do_not_change_the_result() {
        let input = tone(1_000.0, 48_000, 1.0);
        let whole = Resampler::new(48_000, 16_000).process(&input);
        for size in [1, 480, 512, 1_024, 4_099] {
            let mut resampler = Resampler::new(48_000, 16_000);
            let split: Vec<f32> = input
                .chunks(size)
                .flat_map(|c| resampler.process(c))
                .collect();
            assert_eq!(split, whole, "callback size {size} changed the audio");
        }
    }

    #[test]
    fn duration_is_kept_at_every_common_rate() {
        for from in [
            8_000, 16_000, 22_050, 32_000, 44_100, 48_000, 96_000, 192_000,
        ] {
            let input = vec![0.0_f32; from as usize];
            let mut resampler = Resampler::new(from, 16_000);
            let output: Vec<f32> = input
                .chunks(512)
                .flat_map(|c| resampler.process(c))
                .collect();
            assert!(
                (15_900..=16_000).contains(&output.len()),
                "{from} Hz produced {} samples",
                output.len()
            );
        }
    }

    #[test]
    fn the_signal_is_not_shifted_in_time() {
        let input = tone(1_000.0, 48_000, 1.0);
        let output = Resampler::new(48_000, 16_000).process(&input);
        let expected = tone(1_000.0, 16_000, 1.0);
        let edge = output.len() / 10;
        let error: Vec<f32> = output[edge..output.len() - edge]
            .iter()
            .zip(&expected[edge..])
            .map(|(a, b)| a - b)
            .collect();
        assert!(rms(&error) < 0.001, "error {:.5}", rms(&error));
    }

    #[test]
    fn a_constant_level_stays_constant() {
        let output = Resampler::new(44_100, 16_000).process(&vec![0.25_f32; 44_100]);
        let edge = output.len() / 10;
        for sample in &output[edge..output.len() - edge] {
            assert!((sample - 0.25).abs() < 1e-4, "got {sample}");
        }
    }

    #[test]
    fn low_rate_microphones_are_raised_to_the_target() {
        let gain = gain_db(8_000, 16_000, 1_000.0);
        assert!(gain.abs() < 0.1, "changed by {gain:.2} dB");
    }
}
