//! Tells a recording that holds no sound at all from one where someone spoke. A cloud
//! recognizer given silence does not answer with nothing: it invents a short phrase.

/// Below this a recording holds only the noise of an idle input, about -50 dB of full scale.
/// Quiet speech into a low-gain microphone still peaks several times higher.
const SILENT_PEAK: u32 = 100;

/// The loudest sample in 16-bit little-endian audio.
pub fn peak(bytes: &[u8]) -> u32 {
    bytes
        .chunks_exact(2)
        .map(|pair| i16::from_le_bytes([pair[0], pair[1]]).unsigned_abs() as u32)
        .max()
        .unwrap_or(0)
}

pub fn is_silent(peak: u32) -> bool {
    peak < SILENT_PEAK
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bytes(samples: &[i16]) -> Vec<u8> {
        samples.iter().flat_map(|s| s.to_le_bytes()).collect()
    }

    #[test]
    fn the_peak_is_the_loudest_sample_either_way() {
        assert_eq!(peak(&bytes(&[3, -900, 40])), 900);
        assert_eq!(peak(&bytes(&[i16::MIN])), 32_768);
        assert_eq!(peak(&[]), 0);
    }

    #[test]
    fn a_blocked_or_muted_microphone_is_silent_and_quiet_speech_is_not() {
        assert!(is_silent(0));
        assert!(is_silent(60));
        assert!(!is_silent(400));
        assert!(!is_silent(12_000));
    }
}
