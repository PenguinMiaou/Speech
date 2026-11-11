//! Capture the default output device as 16 kHz mono signed 16-bit PCM.

#[cfg(any(target_os = "macos", target_os = "windows"))]
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(any(target_os = "macos", target_os = "windows"))]
use std::sync::Arc;
use tokio::sync::mpsc;

/// Owns the capture worker. Dropping the handle also stops capture.
pub struct SystemAudioHandle {
    stop_tx: Option<std::sync::mpsc::Sender<()>>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl SystemAudioHandle {
    /// Stop capture and wait for the stream and its loopback resources to be released.
    /// Calling this more than once is harmless.
    pub fn stop(&mut self) {
        if let Some(sender) = self.stop_tx.take() {
            let _ = sender.send(());
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Drop for SystemAudioHandle {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Capture the output device in use, and whichever becomes it later, in 20 ms
/// little-endian PCM chunks.
/// Requires macOS 14.6 or later, or Windows 10 version 1703 or later.
/// On macOS, cpal cannot capture loopback from a device that also exposes input channels.
/// Returns after stream construction and the play request succeed.
/// After a later backend failure the capture is opened anew.
#[cfg(any(target_os = "macos", target_os = "windows"))]
pub fn start() -> Result<(SystemAudioHandle, mpsc::Receiver<Vec<u8>>), String> {
    #[cfg(target_os = "macos")]
    {
        let version = std::process::Command::new("/usr/bin/sw_vers")
            .arg("-productVersion")
            .output()
            .map_err(|_| "system_audio_unsupported".to_string())?;
        if !version.status.success()
            || !macos_version_supported(&String::from_utf8_lossy(&version.stdout))
        {
            return Err("system_audio_unsupported".to_string());
        }
    }

    // Keep up to a minute of complete chunks without blocking the audio callback.
    let (audio_tx, audio_rx) = mpsc::channel(3_000);
    let (stop_tx, stop_rx) = std::sync::mpsc::channel();
    let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel(1);
    let callback_stop = stop_tx.clone();
    let thread = std::thread::Builder::new()
        .name("system-audio".to_string())
        .spawn(move || {
            let lost = Arc::new(AtomicBool::new(false));
            let open = || {
                use cpal::traits::StreamTrait;
                let stream = open_stream(audio_tx.clone(), callback_stop.clone(), lost.clone())?;
                stream
                    .play()
                    .map_err(|error| format!("Failed to start system audio: {error}"))?;
                Ok::<_, String>(stream)
            };
            let mut output = default_output();
            let mut stream = match open() {
                Ok(stream) => Some(stream),
                Err(error) => {
                    let _ = ready_tx.send(Err(error));
                    return;
                }
            };
            if ready_tx.send(Ok(())).is_err() {
                return;
            }
            // The sound may move to another output while this runs, headphones put on
            // or taken off, and the output in use may go away: it is followed.
            while let Err(std::sync::mpsc::RecvTimeoutError::Timeout) = stop_rx.recv_timeout(LOOK) {
                let now = default_output();
                if !opens_anew(
                    stream.is_some(),
                    lost.load(Ordering::Relaxed),
                    &output,
                    &now,
                ) {
                    continue;
                }
                // The old tap has to be gone before a new one is made.
                let had = stream.take().is_some();
                lost.store(false, Ordering::Relaxed);
                output = now;
                stream = match open() {
                    Ok(stream) => {
                        tracing::info!("The computer's sound is recorded from another output");
                        Some(stream)
                    }
                    Err(error) => {
                        if had {
                            tracing::warn!("The computer's sound cannot be recorded now: {error}");
                        }
                        None
                    }
                };
            }
            // Dropping the stream also destroys cpal's tap and aggregate device on macOS.
            drop(stream);
        })
        .map_err(|error| format!("Failed to spawn system audio capture: {error}"))?;
    let handle = SystemAudioHandle {
        stop_tx: Some(stop_tx),
        thread: Some(thread),
    };
    ready_rx
        .recv()
        .map_err(|_| "System audio capture ended during startup".to_string())??;
    Ok((handle, audio_rx))
}

/// How often the output in use is looked at.
#[cfg(any(target_os = "macos", target_os = "windows"))]
const LOOK: std::time::Duration = std::time::Duration::from_secs(1);

#[cfg(any(target_os = "macos", target_os = "windows"))]
fn default_output() -> Option<String> {
    use cpal::traits::{DeviceTrait, HostTrait};
    let device = cpal::default_host().default_output_device()?;
    device.id().ok().map(|id| id.to_string())
}

/// Whether the computer's sound has to be opened anew: nothing is open, what was open
/// ended, or the sound now plays through another output.
#[cfg(any(target_os = "macos", target_os = "windows", test))]
fn opens_anew(open: bool, lost: bool, opened: &Option<String>, now: &Option<String>) -> bool {
    !open || lost || opened != now
}

/// System audio capture is unavailable on this platform.
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub fn start() -> Result<(SystemAudioHandle, mpsc::Receiver<Vec<u8>>), String> {
    Err("system_audio_unsupported".to_string())
}

#[cfg(any(target_os = "macos", test))]
fn macos_version_supported(version: &str) -> bool {
    let mut parts = version.trim().split('.');
    let major = parts.next().and_then(|part| part.parse::<u32>().ok());
    let minor = parts.next().and_then(|part| part.parse::<u32>().ok());
    matches!((major, minor), (Some(major), Some(minor)) if (major, minor) >= (14, 6))
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
fn open_stream(
    sender: mpsc::Sender<Vec<u8>>,
    stop: std::sync::mpsc::Sender<()>,
    lost: Arc<AtomicBool>,
) -> Result<cpal::Stream, String> {
    use cpal::traits::{DeviceTrait, HostTrait};

    let device = cpal::default_host()
        .default_output_device()
        .ok_or_else(|| "No default output device available".to_string())?;

    // cpal selects ordinary input on duplex devices, which would record their microphone.
    #[cfg(target_os = "macos")]
    if device.supports_input() {
        return Err("System audio loopback is unavailable for a default output device with input channels in cpal 0.18.2".to_string());
    }

    let config = device
        .default_output_config()
        .map_err(|error| format!("Failed to query system audio format: {error}"))?;
    if config.sample_rate() == 0 || config.channels() == 0 {
        return Err("Invalid default output audio format".to_string());
    }

    macro_rules! build {
        ($sample:ty) => {
            build_stream::<$sample>(&device, config.config(), sender, stop, lost)
        };
    }
    let stream = match config.sample_format() {
        cpal::SampleFormat::I8 => build!(i8),
        cpal::SampleFormat::I16 => build!(i16),
        cpal::SampleFormat::I24 => build!(cpal::I24),
        cpal::SampleFormat::I32 => build!(i32),
        cpal::SampleFormat::I64 => build!(i64),
        cpal::SampleFormat::U8 => build!(u8),
        cpal::SampleFormat::U16 => build!(u16),
        cpal::SampleFormat::U24 => build!(cpal::U24),
        cpal::SampleFormat::U32 => build!(u32),
        cpal::SampleFormat::U64 => build!(u64),
        cpal::SampleFormat::F32 => build!(f32),
        cpal::SampleFormat::F64 => build!(f64),
        format => return Err(format!("Unsupported system audio sample format: {format}")),
    };
    stream.map_err(|error| format!("Failed to open system audio capture: {error}"))
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
fn build_stream<T>(
    device: &cpal::Device,
    config: cpal::StreamConfig,
    sender: mpsc::Sender<Vec<u8>>,
    stop: std::sync::mpsc::Sender<()>,
    lost: Arc<AtomicBool>,
) -> Result<cpal::Stream, cpal::Error>
where
    T: cpal::SizedSample,
    f32: cpal::FromSample<T>,
{
    use cpal::traits::DeviceTrait;

    const CHUNK_BYTES: usize = 320 * 2;
    let mut converter = PcmConverter::new(config.sample_rate, config.channels);
    let mut pending = Vec::new();
    // An input stream on an output device enables Core Audio or WASAPI loopback.
    device.build_input_stream(
        config,
        move |data: &[T], _: &cpal::InputCallbackInfo| {
            pending.extend(converter.convert(data));
            while pending.len() >= CHUNK_BYTES {
                let bytes = pending.drain(..CHUNK_BYTES).collect();
                match sender.try_send(bytes) {
                    Ok(()) | Err(mpsc::error::TrySendError::Full(_)) => {}
                    Err(mpsc::error::TrySendError::Closed(_)) => {
                        let _ = stop.send(());
                        break;
                    }
                }
            }
        },
        move |error| {
            // The output moving elsewhere is seen by looking; anything else that is
            // not a late cycle ends this stream, and it is opened anew.
            if !matches!(
                error.kind(),
                cpal::ErrorKind::Xrun | cpal::ErrorKind::DeviceChanged
            ) {
                tracing::error!("System audio capture error: {error}");
                lost.store(true, Ordering::Relaxed);
            }
        },
        Some(std::time::Duration::from_secs(5)),
    )
}

/// Add both inputs with saturation, treating a missing sample as silence.
pub fn mix(microphone: &[i16], system: &[i16]) -> Vec<i16> {
    (0..microphone.len().max(system.len()))
        .map(|index| {
            microphone
                .get(index)
                .copied()
                .unwrap_or(0)
                .saturating_add(system.get(index).copied().unwrap_or(0))
        })
        .collect()
}

#[cfg(any(target_os = "macos", target_os = "windows", test))]
struct PcmConverter {
    channels: u16,
    resampler: Option<crate::audio::resample::Resampler>,
}

#[cfg(any(target_os = "macos", target_os = "windows", test))]
impl PcmConverter {
    fn new(sample_rate: u32, channels: u16) -> Self {
        Self {
            channels,
            resampler: (sample_rate != 16_000)
                .then(|| crate::audio::resample::Resampler::new(sample_rate, 16_000)),
        }
    }

    fn convert<T>(&mut self, samples: &[T]) -> Vec<u8>
    where
        T: cpal::SizedSample,
        f32: cpal::FromSample<T>,
    {
        use crate::audio::capture::{samples_to_f32, to_mono};

        let mono = to_mono(&samples_to_f32(samples), self.channels);
        let resampled = match &mut self.resampler {
            Some(resampler) => resampler.process(&mono),
            None => mono,
        };
        resampled
            .into_iter()
            .flat_map(|sample| ((sample * 32767.0).clamp(-32768.0, 32767.0) as i16).to_le_bytes())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sound_is_opened_anew_when_it_ended_or_plays_elsewhere() {
        let speakers = Some("speakers".to_string());
        let buds = Some("buds".to_string());
        assert!(!opens_anew(true, false, &speakers, &speakers));
        assert!(opens_anew(true, false, &speakers, &buds));
        assert!(opens_anew(true, true, &speakers, &speakers));
        assert!(opens_anew(false, false, &speakers, &speakers));
        assert!(opens_anew(true, false, &speakers, &None));
    }

    #[test]
    fn macos_loopback_requires_14_6_or_later() {
        for version in ["10.15.7", "12.7", "13.6", "14.1", "14.2", "14.5.9"] {
            assert!(!macos_version_supported(version), "{version}");
        }
        for version in ["14.6", "14.6.1", "14.10", "15.0", "27.2\n"] {
            assert!(macos_version_supported(version), "{version}");
        }
        for version in ["", "unknown", "14", "14.bad"] {
            assert!(!macos_version_supported(version), "{version}");
        }
    }

    #[test]
    fn stop_waits_for_worker_cleanup_and_can_be_called_twice() {
        let (stop_tx, stop_rx) = std::sync::mpsc::channel();
        let (audio_tx, mut audio_rx) = mpsc::channel::<Vec<u8>>(1);
        let thread = std::thread::spawn(move || {
            let _ = stop_rx.recv();
            drop(audio_tx);
        });
        let mut handle = SystemAudioHandle {
            stop_tx: Some(stop_tx),
            thread: Some(thread),
        };
        handle.stop();
        assert_eq!(
            audio_rx.try_recv(),
            Err(mpsc::error::TryRecvError::Disconnected)
        );
        handle.stop();
    }

    #[test]
    fn dropping_handle_waits_for_worker_cleanup() {
        let (stop_tx, stop_rx) = std::sync::mpsc::channel();
        let (audio_tx, mut audio_rx) = mpsc::channel::<Vec<u8>>(1);
        let thread = std::thread::spawn(move || {
            let _ = stop_rx.recv();
            std::thread::sleep(std::time::Duration::from_millis(20));
            drop(audio_tx);
        });
        drop(SystemAudioHandle {
            stop_tx: Some(stop_tx),
            thread: Some(thread),
        });
        assert_eq!(
            audio_rx.try_recv(),
            Err(mpsc::error::TryRecvError::Disconnected)
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    #[ignore = "captures three seconds of system audio and requires system audio recording permission"]
    fn records_three_seconds_of_system_audio() {
        let (mut handle, mut receiver) = start().expect("system audio capture failed");
        std::thread::sleep(std::time::Duration::from_secs(3));
        handle.stop();
        let mut sample_count = 0;
        let mut peak = 0_u16;
        while let Ok(bytes) = receiver.try_recv() {
            assert_eq!(bytes.len() % 2, 0);
            sample_count += bytes.len() / 2;
            for pair in bytes.chunks_exact(2) {
                peak = peak.max(i16::from_le_bytes([pair[0], pair[1]]).unsigned_abs());
            }
        }
        println!("sample_count={sample_count} peak={peak}");
        assert!(sample_count > 0, "no system audio samples were delivered");
    }

    #[test]
    fn mix_adds_samples_and_saturates_in_both_directions() {
        assert_eq!(
            mix(&[100, -100, i16::MAX, i16::MIN], &[200, -200, 1, -1]),
            vec![300, -300, i16::MAX, i16::MIN]
        );
    }

    #[test]
    fn mix_preserves_the_longer_inputs_tail() {
        assert_eq!(mix(&[1, 2, 3], &[4]), vec![5, 2, 3]);
        assert_eq!(mix(&[1], &[4, 5, 6]), vec![5, 5, 6]);
    }

    #[test]
    fn mix_accepts_empty_inputs() {
        assert_eq!(mix(&[], &[]), Vec::<i16>::new());
        assert_eq!(mix(&[i16::MIN, i16::MAX], &[]), vec![i16::MIN, i16::MAX]);
        assert_eq!(mix(&[], &[i16::MIN, i16::MAX]), vec![i16::MIN, i16::MAX]);
    }

    #[test]
    fn conversion_averages_channels_and_writes_little_endian_pcm() {
        let mut converter = PcmConverter::new(16_000, 2);
        assert_eq!(
            converter.convert(&[1.0_f32, 0.0, -1.0, 0.0, 0.75, -0.75]),
            vec![0xff, 0x3f, 0x01, 0xc0, 0x00, 0x00]
        );
        let mut surround = PcmConverter::new(16_000, 4);
        assert_eq!(
            surround.convert(&[1.0_f32, 1.0, 0.0, 0.0]),
            vec![0xff, 0x3f]
        );
    }

    #[test]
    fn conversion_clamps_out_of_range_samples() {
        let mut converter = PcmConverter::new(16_000, 1);
        assert_eq!(
            converter.convert(&[-2.0_f32, -1.0, 0.0, 1.0, 2.0]),
            vec![0x00, 0x80, 0x01, 0x80, 0x00, 0x00, 0xff, 0x7f, 0xff, 0x7f]
        );
    }

    #[test]
    fn conversion_normalizes_signed_unsigned_and_float_formats() {
        let mut converter = PcmConverter::new(16_000, 1);
        let expected = vec![0x01, 0x80, 0x00, 0x00, 0xff, 0x3f];
        assert_eq!(converter.convert(&[-1.0_f64, 0.0, 0.5]), expected);
        assert_eq!(converter.convert(&[i16::MIN, 0, 16384]), expected);
        assert_eq!(converter.convert(&[0_u16, 32768, 49152]), expected);
        assert_eq!(converter.convert(&[i32::MIN, 0, 1 << 30]), expected);
        assert_eq!(converter.convert(&[0_u8, 128, 192]), expected);
    }

    #[test]
    fn conversion_of_empty_input_is_empty() {
        for rate in [16_000, 48_000] {
            assert!(PcmConverter::new(rate, 2).convert::<f32>(&[]).is_empty());
        }
    }

    #[test]
    fn conversion_resamples_to_16khz_across_callback_boundaries() {
        for rate in [8_000, 44_100, 48_000, 96_000] {
            let input = vec![0.25_f32; rate as usize * 2];
            let whole = PcmConverter::new(rate, 2).convert(&input);
            let mut converter = PcmConverter::new(rate, 2);
            let split: Vec<u8> = input
                .chunks(514)
                .flat_map(|chunk| converter.convert(chunk))
                .collect();
            assert_eq!(
                split, whole,
                "callback boundaries changed conversion at {rate} Hz"
            );
            assert!((15_900..=16_000).contains(&(whole.len() / 2)));
            for bytes in whole[200..whole.len() - 200].chunks_exact(2) {
                let sample = i16::from_le_bytes([bytes[0], bytes[1]]);
                assert!((sample - 8191).abs() <= 1);
            }
        }
    }
}
