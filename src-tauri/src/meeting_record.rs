//! Records a meeting from the microphone so it can be written out like a dropped file.

use std::sync::{Arc, Mutex};

use crate::audio::capture::{AudioCaptureHandle, AudioConfig};

/// Three hours at 16 kHz, the same limit a dropped file has.
const LONGEST_SAMPLES: usize = 3 * 60 * 60 * 16_000;

struct Recording {
    capture: AudioCaptureHandle,
    samples: Arc<Mutex<Vec<i16>>>,
    collector: tokio::task::JoinHandle<()>,
}

static ACTIVE: Mutex<Option<Recording>> = Mutex::new(None);

fn active() -> std::sync::MutexGuard<'static, Option<Recording>> {
    ACTIVE.lock().unwrap_or_else(|error| error.into_inner())
}

/// Adds the capture's 16-bit little-endian bytes to the recording, up to the limit.
fn collect(samples: &mut Vec<i16>, bytes: &[u8], limit: usize) {
    let room = limit.saturating_sub(samples.len());
    samples.extend(
        bytes
            .chunks_exact(2)
            .take(room)
            .map(|pair| i16::from_le_bytes([pair[0], pair[1]])),
    );
}

pub fn is_recording() -> bool {
    active().is_some()
}

/// Starts recording from `input_device` (empty for the system default).
pub async fn start(input_device: String) -> Result<(), String> {
    if is_recording() {
        return Err("busy".to_string());
    }
    let config = AudioConfig {
        input_device,
        ..AudioConfig::default()
    };
    let (mut capture, mut audio) =
        AudioCaptureHandle::start(config).map_err(|error| error.to_string())?;
    capture
        .wait_until_ready()
        .await
        .map_err(|error| error.to_string())?;
    let samples = Arc::new(Mutex::new(Vec::new()));
    let sink = samples.clone();
    let collector = tokio::spawn(async move {
        while let Some(bytes) = audio.recv().await {
            let mut samples = sink.lock().unwrap_or_else(|error| error.into_inner());
            collect(&mut samples, &bytes, LONGEST_SAMPLES);
        }
    });
    *active() = Some(Recording {
        capture,
        samples,
        collector,
    });
    Ok(())
}

/// How long the recording is so far and how loud the microphone is now.
pub fn level() -> Option<(u64, f32)> {
    let active = active();
    let recording = active.as_ref()?;
    let samples = recording
        .samples
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .len();
    Some((samples as u64 / 16_000, recording.capture.get_volume()))
}

/// Ends the recording and returns its audio.
pub async fn stop() -> Result<Vec<f32>, String> {
    let Some(mut recording) = active().take() else {
        return Err("not_recording".to_string());
    };
    recording.capture.stop();
    // The capture thread closes the channel once it has handed over what it still held.
    let _ = tokio::time::timeout(std::time::Duration::from_secs(3), &mut recording.collector).await;
    recording.collector.abort();
    let samples = std::mem::take(
        &mut *recording
            .samples
            .lock()
            .unwrap_or_else(|error| error.into_inner()),
    );
    Ok(samples
        .iter()
        .map(|sample| *sample as f32 / 32_768.0)
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bytes_become_samples_and_stop_at_the_limit() {
        let mut samples = Vec::new();
        collect(&mut samples, &[0x00, 0x40, 0xff, 0xff, 0x01], 10);
        assert_eq!(samples, vec![16_384, -1]);
        collect(&mut samples, &[1, 0, 2, 0, 3, 0], 4);
        assert_eq!(samples, vec![16_384, -1, 1, 2]);
    }
}
