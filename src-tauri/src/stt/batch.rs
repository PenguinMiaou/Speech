//! The recognizer chosen in the settings, hearing a meeting a piece at a time.

use std::sync::{Arc, RwLock};
use std::time::Duration;

use super::whisper_compat::WhisperCompatConfig;
use super::SttConfig;

/// Whether sound given to this recognizer leaves the machine.
pub fn leaves_the_machine(provider: &str) -> bool {
    provider != super::config::LOCAL_QWEN3_PROVIDER
        && provider != super::config::APPLE_SPEECH_PROVIDER
}

/// How often a recognizer that asks for patience is asked again, and how long is waited
/// before the first of those; each further wait is as much longer again.
const ASKED_AGAIN: u32 = 6;
const WAIT: Duration = Duration::from_secs(10);

/// A recognizer away from this machine, as it is set up in the settings.
pub struct Chosen {
    provider: String,
    /// The endpoint and model of a recognizer the user set up themselves.
    custom: Option<(String, String)>,
    config: SttConfig,
    client: reqwest::Client,
    wait: Duration,
}

impl Chosen {
    /// The recognizer the settings name, or none where that one runs on this machine.
    pub fn of(
        settings: &crate::storage::AppConfig,
        api_key: String,
        client: reqwest::Client,
    ) -> Result<Option<Self>, String> {
        let provider = settings.stt_provider.clone();
        if !leaves_the_machine(&provider) {
            return Ok(None);
        }
        let custom = if provider == super::config::CUSTOM_WHISPER_PROVIDER {
            let built = super::config::build_custom_whisper_config(
                &settings.stt_custom_base_url,
                &settings.stt_custom_model,
            )?;
            Some((built.endpoint, built.model))
        } else {
            None
        };
        let config = SttConfig {
            api_key,
            language: (settings.stt_language != "multi").then(|| settings.stt_language.clone()),
            resource_id: (provider == super::volcengine::VOLCENGINE_DOUBAO_PROVIDER)
                .then(|| settings.stt_volcengine_resource_id.clone()),
            provider_region: (provider == super::aliyun_qwen3_asr::ALIYUN_QWEN3_ASR_PROVIDER)
                .then(|| settings.stt_aliyun_qwen_region.clone()),
            ..SttConfig::default()
        };
        Ok(Some(Self {
            provider,
            custom,
            config,
            client,
            wait: WAIT,
        }))
    }

    /// The words in a piece of sound. A recognizer that is busy is asked again after a
    /// while; one that refuses is not.
    pub fn hear(&self, pcm: &[f32], vocabulary: &[String]) -> Result<String, String> {
        let mut wait = self.wait;
        let mut asked = 0;
        loop {
            match tauri::async_runtime::block_on(self.heard(pcm, vocabulary)) {
                Err(crate::error::AppError::Api { status: 429, .. }) if asked < ASKED_AGAIN => {
                    std::thread::sleep(wait);
                    wait *= 2;
                    asked += 1;
                }
                other => return other.map_err(|error| error.to_user_error().code),
            }
        }
    }

    async fn heard(
        &self,
        pcm: &[f32],
        vocabulary: &[String],
    ) -> Result<String, crate::error::AppError> {
        let custom = self
            .custom
            .as_ref()
            .map(|(endpoint, model)| WhisperCompatConfig {
                provider_name: self.provider.clone(),
                endpoint: endpoint.clone(),
                model: model.clone(),
                extra_fields: Vec::new(),
                api_key_required: false,
            });
        let mut provider =
            super::create_provider(&self.provider, custom, Some(self.client.clone()))?;
        let config = SttConfig {
            vocabulary: vocabulary.to_vec(),
            ..self.config.clone()
        };
        provider.connect(&config).await?;
        let bytes: Vec<u8> = pcm
            .iter()
            .flat_map(|sample| ((sample.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes())
            .collect();
        // Sent in pieces of a tenth of a second, as a microphone would give them.
        for piece in bytes.chunks(3_200) {
            provider.send_audio(piece).await?;
        }
        Ok(provider
            .disconnect()
            .await?
            .unwrap_or_default()
            .trim()
            .to_string())
    }
}

static CHOSEN: RwLock<Option<Arc<Chosen>>> = RwLock::new(None);

/// Names the recognizer that meetings are heard with from now on; none is the one on
/// this machine.
pub fn choose(chosen: Option<Chosen>) {
    *CHOSEN.write().unwrap_or_else(|error| error.into_inner()) = chosen.map(Arc::new);
}

/// The recognizer away from this machine that meetings are heard with, if one is named.
pub fn chosen() -> Option<Arc<Chosen>> {
    CHOSEN
        .read()
        .unwrap_or_else(|error| error.into_inner())
        .clone()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};

    /// A server that answers each transcription request with what `answers` holds next.
    fn server(answers: Vec<(u16, &'static str)>) -> (String, std::thread::JoinHandle<usize>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = format!(
            "http://{}/v1/audio/transcriptions",
            listener.local_addr().unwrap()
        );
        let served = std::thread::spawn(move || {
            let mut bytes = 0;
            for (status, body) in answers {
                let (mut stream, _) = listener.accept().unwrap();
                let mut head = Vec::new();
                let mut byte = [0u8; 1];
                while !head.ends_with(b"\r\n\r\n") {
                    stream.read_exact(&mut byte).unwrap();
                    head.push(byte[0]);
                }
                let head = String::from_utf8_lossy(&head).to_lowercase();
                let length: usize = head
                    .lines()
                    .find_map(|line| line.strip_prefix("content-length:"))
                    .map(|value| value.trim().parse().unwrap())
                    .unwrap_or(0);
                let mut body_in = vec![0u8; length];
                stream.read_exact(&mut body_in).unwrap();
                bytes += length;
                write!(
                    stream,
                    "HTTP/1.1 {status} X\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                    body.len()
                )
                .unwrap();
            }
            bytes
        });
        (address, served)
    }

    fn chosen(endpoint: String) -> Chosen {
        Chosen {
            provider: crate::stt::config::CUSTOM_WHISPER_PROVIDER.to_string(),
            custom: Some((endpoint, "a-model".to_string())),
            config: crate::stt::SttConfig::default(),
            client: reqwest::Client::new(),
            wait: std::time::Duration::from_millis(10),
        }
    }

    #[test]
    fn a_piece_of_sound_comes_back_as_the_words_the_chosen_recognizer_heard() {
        let (endpoint, served) = server(vec![(200, r#"{"text":" we ship on Friday "}"#)]);
        let text = chosen(endpoint).hear(&[0.25; 16_000], &[]).unwrap();
        assert_eq!(text, "we ship on Friday");
        // A second of sound went there: 16,000 samples of two bytes each, and a little more.
        assert!(served.join().unwrap() > 32_000);
    }

    #[test]
    fn a_recognizer_that_asks_for_patience_is_asked_again() {
        let (endpoint, served) = server(vec![(429, "{}"), (200, r#"{"text":"again"}"#)]);
        assert_eq!(chosen(endpoint).hear(&[0.25; 1_600], &[]).unwrap(), "again");
        served.join().unwrap();
    }

    #[test]
    fn a_recognizer_that_refuses_is_not_asked_again() {
        let (endpoint, served) = server(vec![(401, "{}")]);
        assert!(chosen(endpoint).hear(&[0.25; 1_600], &[]).is_err());
        served.join().unwrap();
    }

    #[test]
    fn sound_without_words_is_no_failure() {
        let (endpoint, served) = server(vec![(200, r#"{"text":""}"#)]);
        assert_eq!(chosen(endpoint).hear(&[0.0; 1_600], &[]).unwrap(), "");
        served.join().unwrap();
    }

    #[test]
    fn the_recognizers_on_this_machine_are_none_of_the_chosen() {
        assert!(!leaves_the_machine(
            crate::stt::config::LOCAL_QWEN3_PROVIDER
        ));
        assert!(!leaves_the_machine(
            crate::stt::config::APPLE_SPEECH_PROVIDER
        ));
        assert!(leaves_the_machine("groq-whisper"));
        assert!(leaves_the_machine(
            crate::stt::config::CUSTOM_WHISPER_PROVIDER
        ));
    }
}
