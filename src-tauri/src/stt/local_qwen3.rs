//! Speech recognition on this machine through the bundled worker process.

use async_trait::async_trait;
use ot_stt_protocol::MAX_PCM_BYTES;

use super::{SttConfig, SttProvider, TranscriptEvent};
use crate::error::AppError;
use crate::local_stt::catalog::{self, DEFAULT_LOCAL_MODEL_ID};
use crate::local_stt::{self, LocalSttError};

pub use super::config::LOCAL_QWEN3_PROVIDER;

/// Incoming audio is 16-bit, the worker takes f32, so this is half the worker's byte limit.
const MAX_INPUT_BYTES: usize = (MAX_PCM_BYTES / 2) as usize;

#[derive(Default)]
pub struct LocalQwen3Provider {
    audio: Vec<u8>,
    model_id: Option<String>,
    vocabulary: Vec<String>,
    prompt: Option<String>,
    /// The recording being reduced to the user's own voice, when that is switched on.
    isolation: Option<crate::voice_isolation::Session>,
}

/// How long recognition waits for isolation to finish after the recording ends. It runs
/// ahead of the recording, so a longer wait means something is wrong and the recording is
/// used as it is.
const ISOLATION_WAIT: std::time::Duration = std::time::Duration::from_millis(1_500);

impl LocalQwen3Provider {
    pub fn new() -> Self {
        Self::default()
    }
}

/// Converts 16-bit little-endian PCM to the [-1, 1) samples the engine expects.
pub fn i16_le_to_f32(bytes: &[u8]) -> Vec<f32> {
    bytes
        .chunks_exact(2)
        .map(|c| i16::from_le_bytes([c[0], c[1]]) as f32 / 32768.0)
        .collect()
}

fn to_app_error(error: LocalSttError) -> AppError {
    AppError::Config(error.to_string())
}

#[async_trait]
impl SttProvider for LocalQwen3Provider {
    async fn connect(&mut self, config: &SttConfig) -> Result<(), AppError> {
        self.audio.clear();
        let model_id = config
            .resource_id
            .clone()
            .unwrap_or_else(|| DEFAULT_LOCAL_MODEL_ID.to_string());
        // Checked here so nobody speaks a whole dictation into a missing model.
        let path = catalog::installed_path(&model_id)
            .ok_or_else(|| to_app_error(LocalSttError::ModelMissing))?;
        // Warm up in the background so recording starts immediately.
        if let Some(supervisor) = local_stt::global() {
            tokio::task::spawn_blocking(move || {
                if let Err(error) = supervisor.ensure_loaded(&path) {
                    tracing::warn!("local speech warm-up failed: {error}");
                }
            });
        }
        self.model_id = Some(model_id);
        self.isolation = match (crate::voice_isolation::ready(), local_stt::global()) {
            (Some((enroll, chunk, sample)), Some(supervisor)) => Some(
                crate::voice_isolation::Session::start(supervisor, enroll, chunk, sample),
            ),
            _ => None,
        };
        self.vocabulary = config.vocabulary.clone();
        self.prompt = config.prompt.clone();
        Ok(())
    }

    async fn send_audio(&mut self, chunk: &[u8]) -> Result<(), AppError> {
        if self.audio.len() + chunk.len() > MAX_INPUT_BYTES {
            return Err(AppError::Config(
                "local speech: audio exceeds maximum length (12 min)".to_string(),
            ));
        }
        self.audio.extend_from_slice(chunk);
        if let Some(isolation) = &self.isolation {
            isolation.feed(i16_le_to_f32(chunk));
        }
        Ok(())
    }

    async fn recv_transcript(&mut self) -> Result<Option<TranscriptEvent>, AppError> {
        std::future::pending().await
    }

    async fn disconnect(&mut self) -> Result<Option<String>, AppError> {
        if self.audio.is_empty() {
            return Ok(None);
        }
        let model_id = self
            .model_id
            .clone()
            .unwrap_or_else(|| DEFAULT_LOCAL_MODEL_ID.to_string());
        let path = catalog::installed_path(&model_id)
            .ok_or_else(|| to_app_error(LocalSttError::ModelMissing))?;
        let supervisor = local_stt::global()
            .ok_or_else(|| to_app_error(LocalSttError::Io("worker unavailable".into())))?;
        let mut pcm = i16_le_to_f32(&std::mem::take(&mut self.audio));
        if crate::voice_isolation::take_enrollment_request() {
            crate::voice_isolation::enroll(&pcm);
        }
        if let Some(isolation) = self.isolation.take() {
            let started = std::time::Instant::now();
            let isolated = tokio::task::spawn_blocking(move || isolation.finish(ISOLATION_WAIT))
                .await
                .ok()
                .flatten();
            match isolated {
                Some(isolated) => {
                    let (audio, kept) = crate::voice_isolation::choose(&pcm, &isolated);
                    let used_isolated = audio == crate::voice_isolation::Audio::Isolated;
                    crate::applog::note(
                        "voice",
                        &crate::voice_isolation::outcome(
                            Some((audio, kept)),
                            started.elapsed().as_millis(),
                        ),
                    );
                    tracing::info!(
                        "[Isolation] kept={kept:.2} used={} wait_ms={}",
                        if used_isolated {
                            "isolated"
                        } else {
                            "recording"
                        },
                        started.elapsed().as_millis()
                    );
                    if used_isolated {
                        pcm = isolated;
                    }
                }
                None => {
                    crate::applog::note(
                        "voice",
                        &crate::voice_isolation::outcome(None, started.elapsed().as_millis()),
                    );
                    tracing::info!(
                        "[Isolation] not ready, used=recording wait_ms={}",
                        started.elapsed().as_millis()
                    )
                }
            }
        }
        let vocabulary = std::mem::take(&mut self.vocabulary);
        // Text before the cursor never leaves this machine: only this provider reads it.
        let prompt =
            crate::recent::engine_prompt(crate::recent::take_field_context(), self.prompt.take());
        let text = tokio::task::spawn_blocking(move || {
            supervisor.transcribe(&path, &pcm, vocabulary, prompt)
        })
        .await
        .map_err(|e| AppError::Config(e.to_string()))?
        .map_err(to_app_error)?;
        Ok((!text.trim().is_empty()).then_some(text))
    }

    fn name(&self) -> &str {
        LOCAL_QWEN3_PROVIDER
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stt::config::stt_provider_requires_api_key;
    use crate::stt::create_provider;

    #[test]
    fn pcm_i16_converted_to_f32() {
        assert_eq!(
            i16_le_to_f32(&[0x00, 0x40, 0x00, 0x80, 0xff]),
            vec![0.5, -1.0]
        );
    }

    #[test]
    fn create_provider_returns_local_qwen3() {
        let provider = create_provider(LOCAL_QWEN3_PROVIDER, None, None).unwrap();
        assert_eq!(provider.name(), LOCAL_QWEN3_PROVIDER);
    }

    #[test]
    fn does_not_require_api_key() {
        assert!(!stt_provider_requires_api_key(LOCAL_QWEN3_PROVIDER));
    }

    #[tokio::test]
    async fn disconnect_with_empty_buffer_returns_none() {
        let mut provider = LocalQwen3Provider::new();
        assert_eq!(provider.disconnect().await.unwrap(), None);
    }

    #[tokio::test]
    async fn missing_model_fails_before_recording_starts() {
        let mut provider = LocalQwen3Provider::new();
        let config = SttConfig {
            resource_id: Some("no-such-model".to_string()),
            ..Default::default()
        };
        let error = provider.connect(&config).await.unwrap_err();
        assert!(error.to_string().contains("local_model_not_installed"));
    }

    #[tokio::test]
    async fn model_removed_during_recording_has_its_own_error_code() {
        let mut provider = LocalQwen3Provider::new();
        provider.model_id = Some("no-such-model".to_string());
        provider.send_audio(&[0, 0, 0, 0]).await.unwrap();
        let error = provider.disconnect().await.unwrap_err();
        assert_eq!(error.to_user_error().code, "stt_local_model_missing");
    }

    #[tokio::test]
    async fn audio_over_the_limit_is_rejected() {
        let mut provider = LocalQwen3Provider::new();
        provider.audio = vec![0; MAX_INPUT_BYTES];
        assert!(provider.send_audio(&[0, 0]).await.is_err());
    }
}
