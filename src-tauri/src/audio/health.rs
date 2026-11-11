//! Tells whether a recording lost audio. The system reports a late audio cycle whenever
//! the device starts, which loses nothing that was said; this measures what was really
//! captured so that a real gap during speech shows up in the log.

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// A late cycle this soon after the stream is built belongs to the device starting up.
const STARTUP: Duration = Duration::from_millis(500);

pub fn is_startup(since_built: Duration) -> bool {
    since_built < STARTUP
}

/// Audio missing between the first and the last callback, in milliseconds.
pub fn missing_ms(span: Duration, frames_before_last_callback: u64, rate: u32) -> u64 {
    let captured_ms = frames_before_last_callback * 1_000 / u64::from(rate.max(1));
    (span.as_millis() as u64).saturating_sub(captured_ms)
}

pub struct CaptureHealth {
    built: Instant,
    rate: u32,
    frames: AtomicU64,
    last_callback_frames: AtomicU64,
    first_ns: AtomicU64,
    last_ns: AtomicU64,
    late_cycles: AtomicU32,
}

impl CaptureHealth {
    pub fn new(rate: u32) -> Self {
        Self {
            built: Instant::now(),
            rate,
            frames: AtomicU64::new(0),
            last_callback_frames: AtomicU64::new(0),
            first_ns: AtomicU64::new(0),
            last_ns: AtomicU64::new(0),
            late_cycles: AtomicU32::new(0),
        }
    }

    /// Called from the audio callback with the number of frames it delivered.
    pub fn delivered(&self, frames: usize) {
        let now = (self.built.elapsed().as_nanos() as u64).max(1);
        let _ = self
            .first_ns
            .compare_exchange(0, now, Ordering::Relaxed, Ordering::Relaxed);
        self.last_ns.store(now, Ordering::Relaxed);
        self.frames.fetch_add(frames as u64, Ordering::Relaxed);
        self.last_callback_frames
            .store(frames as u64, Ordering::Relaxed);
    }

    /// How long ago the stream last delivered audio, or was built when it never did.
    pub fn nothing_for(&self) -> Duration {
        self.built
            .elapsed()
            .saturating_sub(Duration::from_nanos(self.last_ns.load(Ordering::Relaxed)))
    }

    /// Records a late audio cycle. Returns how far into the recording it happened when it
    /// was not part of the device starting up.
    pub fn late_cycle(&self) -> Option<Duration> {
        let since_built = self.built.elapsed();
        if is_startup(since_built) {
            return None;
        }
        self.late_cycles.fetch_add(1, Ordering::Relaxed);
        Some(since_built)
    }

    pub fn late_cycles(&self) -> u32 {
        self.late_cycles.load(Ordering::Relaxed)
    }

    pub fn missing_ms(&self) -> u64 {
        let first = self.first_ns.load(Ordering::Relaxed);
        let last = self.last_ns.load(Ordering::Relaxed);
        let before_last = self
            .frames
            .load(Ordering::Relaxed)
            .saturating_sub(self.last_callback_frames.load(Ordering::Relaxed));
        missing_ms(
            Duration::from_nanos(last.saturating_sub(first)),
            before_last,
            self.rate,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_late_cycle_while_the_device_starts_is_not_a_gap() {
        assert!(is_startup(Duration::from_millis(100)));
        assert!(!is_startup(Duration::from_secs(2)));
    }

    #[test]
    fn a_recording_that_kept_every_cycle_misses_nothing() {
        // 100 callbacks of 480 frames at 48 kHz: 990 ms between the first and the last.
        assert_eq!(missing_ms(Duration::from_millis(990), 99 * 480, 48_000), 0);
    }

    #[test]
    fn a_dropped_cycle_shows_as_missing_audio() {
        assert_eq!(
            missing_ms(Duration::from_millis(1_000), 99 * 480, 48_000),
            10
        );
    }

    #[test]
    fn tells_how_long_ago_audio_last_came() {
        let health = CaptureHealth::new(48_000);
        std::thread::sleep(Duration::from_millis(30));
        assert!(health.nothing_for() >= Duration::from_millis(30));
        health.delivered(480);
        assert!(health.nothing_for() < Duration::from_millis(20));
    }

    #[test]
    fn counts_frames_and_late_cycles() {
        let health = CaptureHealth::new(48_000);
        health.delivered(480);
        health.delivered(480);
        assert!(health.missing_ms() < 5);
        assert_eq!(health.late_cycle(), None);
        assert_eq!(health.late_cycles(), 0);
    }
}
