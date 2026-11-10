//! Request loop of the worker. The engine sits behind a trait so the protocol can be tested without a model.

use ot_stt_protocol::{read_frame, write_frame, Request, Response, MAX_PCM_BYTES};
use std::io::{self, Read, Write};
use std::time::Instant;

/// 0.2 seconds at 16 kHz. Shorter audio is an accidental key press, not speech.
pub const MIN_SAMPLES: usize = 3_200;

pub struct EngineError {
    pub code: String,
    pub message: String,
}

pub struct EngineOutput {
    pub text: String,
    pub language: Option<String>,
}

pub trait Engine {
    /// Loads a model and returns the name of the backend it runs on.
    fn load(&mut self, model_path: &str) -> Result<String, EngineError>;
    fn transcribe(
        &mut self,
        pcm: &[f32],
        vocabulary: Vec<String>,
        prompt: Option<String>,
    ) -> Result<EngineOutput, EngineError>;
    /// Starts isolating the voice in `voice_sample` for a new recording.
    fn isolate_begin(
        &mut self,
        enroll_model: &str,
        chunk_model: &str,
        voice_sample: &str,
    ) -> Result<(), EngineError>;
    fn isolate_feed(&mut self, pcm: &[f32]) -> Result<(), EngineError>;
    /// Ends the recording and returns its isolated audio.
    fn isolate_end(&mut self) -> Result<Vec<f32>, EngineError>;
}

pub fn not_isolating() -> EngineError {
    EngineError {
        code: "not_isolating".into(),
        message: "no recording is being isolated".into(),
    }
}

fn read_pcm<R: Read>(reader: &mut R, bytes: u32) -> io::Result<Vec<f32>> {
    let mut raw = vec![0u8; bytes as usize];
    reader.read_exact(&mut raw)?;
    Ok(raw
        .chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect())
}

fn error(code: &str, message: impl Into<String>) -> Response {
    Response::Error {
        code: code.to_string(),
        message: message.into(),
    }
}

pub fn serve<R: Read, W: Write, E: Engine>(
    reader: &mut R,
    writer: &mut W,
    engine: &mut E,
) -> io::Result<()> {
    let mut loaded = false;
    while let Some(request) = read_frame::<_, Request>(reader)? {
        let response = match request {
            Request::Shutdown => return Ok(()),
            Request::IsolateBegin {
                enroll_model,
                chunk_model,
                voice_sample,
            } => match engine.isolate_begin(&enroll_model, &chunk_model, &voice_sample) {
                Ok(()) => Response::Isolating,
                Err(e) => error(&e.code, e.message),
            },
            Request::IsolateFeed { pcm_bytes } => {
                if pcm_bytes > MAX_PCM_BYTES {
                    return Err(io::Error::new(io::ErrorKind::InvalidData, "audio too long"));
                }
                let pcm = read_pcm(reader, pcm_bytes)?;
                match engine.isolate_feed(&pcm) {
                    Ok(()) => Response::Isolating,
                    Err(e) => error(&e.code, e.message),
                }
            }
            Request::IsolateEnd => match engine.isolate_end() {
                Ok(isolated) => {
                    write_frame(
                        writer,
                        &Response::Isolated {
                            pcm_bytes: (isolated.len() * 4) as u32,
                        },
                    )?;
                    for sample in &isolated {
                        writer.write_all(&sample.to_le_bytes())?;
                    }
                    writer.flush()?;
                    continue;
                }
                Err(e) => error(&e.code, e.message),
            },
            Request::Load { model_path } => {
                let started = Instant::now();
                match engine.load(&model_path) {
                    Ok(backend) => {
                        loaded = true;
                        Response::Ready {
                            backend,
                            load_ms: started.elapsed().as_millis() as u64,
                        }
                    }
                    Err(e) => {
                        loaded = false;
                        error(&e.code, e.message)
                    }
                }
            }
            Request::Transcribe {
                vocabulary,
                prompt,
                pcm_bytes,
            } => {
                if pcm_bytes > MAX_PCM_BYTES {
                    return Err(io::Error::new(io::ErrorKind::InvalidData, "audio too long"));
                }
                let pcm = read_pcm(reader, pcm_bytes)?;
                let started = Instant::now();
                if !loaded {
                    error("not_loaded", "no model loaded")
                } else if pcm.len() < MIN_SAMPLES {
                    Response::Transcript {
                        text: String::new(),
                        language: None,
                        elapsed_ms: 0,
                    }
                } else {
                    match engine.transcribe(&pcm, vocabulary, prompt) {
                        Ok(output) => Response::Transcript {
                            text: output.text,
                            language: output.language,
                            elapsed_ms: started.elapsed().as_millis() as u64,
                        },
                        Err(e) => error(&e.code, e.message),
                    }
                }
            }
        };
        write_frame(writer, &response)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[derive(Default)]
    struct FakeEngine {
        fail_load: bool,
        calls: Vec<(usize, Vec<String>, Option<String>)>,
        isolating: Option<Vec<f32>>,
    }

    impl Engine for FakeEngine {
        fn load(&mut self, _model_path: &str) -> Result<String, EngineError> {
            if self.fail_load {
                return Err(EngineError {
                    code: "load_failed".into(),
                    message: "bad file".into(),
                });
            }
            Ok("MTL0".into())
        }
        fn transcribe(
            &mut self,
            pcm: &[f32],
            vocabulary: Vec<String>,
            prompt: Option<String>,
        ) -> Result<EngineOutput, EngineError> {
            self.calls.push((pcm.len(), vocabulary, prompt));
            Ok(EngineOutput {
                text: "你好".into(),
                language: Some("zh".into()),
            })
        }
        fn isolate_begin(&mut self, _: &str, _: &str, _: &str) -> Result<(), EngineError> {
            self.isolating = Some(Vec::new());
            Ok(())
        }
        fn isolate_feed(&mut self, pcm: &[f32]) -> Result<(), EngineError> {
            match &mut self.isolating {
                Some(audio) => {
                    audio.extend_from_slice(pcm);
                    Ok(())
                }
                None => Err(not_isolating()),
            }
        }
        fn isolate_end(&mut self) -> Result<Vec<f32>, EngineError> {
            self.isolating.take().ok_or_else(not_isolating)
        }
    }

    fn feed_frame(samples: &[f32]) -> (Request, Vec<f32>) {
        (
            Request::IsolateFeed {
                pcm_bytes: (samples.len() * 4) as u32,
            },
            samples.to_vec(),
        )
    }

    fn begin_frame() -> (Request, Vec<f32>) {
        (
            Request::IsolateBegin {
                enroll_model: "enroll.onnx".into(),
                chunk_model: "chunk.onnx".into(),
                voice_sample: "voice.wav".into(),
            },
            vec![],
        )
    }

    #[test]
    fn audio_fed_during_a_recording_comes_back_isolated_at_its_end() {
        let mut output = Vec::new();
        let frames = [
            begin_frame(),
            feed_frame(&[0.5, 0.25]),
            feed_frame(&[0.125]),
            (Request::IsolateEnd, vec![]),
        ];
        serve(&mut input(&frames), &mut output, &mut FakeEngine::default()).unwrap();

        let mut reader = Cursor::new(output);
        for _ in 0..3 {
            assert_eq!(
                read_frame::<_, Response>(&mut reader).unwrap(),
                Some(Response::Isolating)
            );
        }
        assert_eq!(
            read_frame::<_, Response>(&mut reader).unwrap(),
            Some(Response::Isolated { pcm_bytes: 12 })
        );
        assert_eq!(read_pcm(&mut reader, 12).unwrap(), vec![0.5, 0.25, 0.125]);
        assert_eq!(read_frame::<_, Response>(&mut reader).unwrap(), None);
    }

    #[test]
    fn audio_fed_without_a_recording_in_progress_is_an_error_and_is_skipped() {
        let frames = [
            feed_frame(&[0.5, 0.25]),
            (Request::IsolateEnd, vec![]),
            load_frame(),
        ];
        let responses = run(&frames, &mut FakeEngine::default());
        assert!(matches!(&responses[0], Response::Error { code, .. } if code == "not_isolating"));
        assert!(matches!(&responses[1], Response::Error { code, .. } if code == "not_isolating"));
        assert!(matches!(&responses[2], Response::Ready { .. }));
    }

    fn input(frames: &[(Request, Vec<f32>)]) -> Cursor<Vec<u8>> {
        let mut buffer = Vec::new();
        for (request, pcm) in frames {
            write_frame(&mut buffer, request).unwrap();
            for sample in pcm {
                buffer.extend_from_slice(&sample.to_le_bytes());
            }
        }
        Cursor::new(buffer)
    }

    fn transcribe_frame(samples: usize) -> (Request, Vec<f32>) {
        (
            Request::Transcribe {
                vocabulary: vec!["TypeScript".into()],
                prompt: Some("前文".into()),
                pcm_bytes: (samples * 4) as u32,
            },
            vec![0.25; samples],
        )
    }

    fn load_frame() -> (Request, Vec<f32>) {
        (
            Request::Load {
                model_path: "m.gguf".into(),
            },
            vec![],
        )
    }

    fn run(frames: &[(Request, Vec<f32>)], engine: &mut FakeEngine) -> Vec<Response> {
        let mut output = Vec::new();
        serve(&mut input(frames), &mut output, engine).unwrap();
        let mut reader = Cursor::new(output);
        let mut responses = Vec::new();
        while let Some(response) = read_frame::<_, Response>(&mut reader).unwrap() {
            responses.push(response);
        }
        responses
    }

    #[test]
    fn transcribe_before_load_returns_not_loaded() {
        let responses = run(&[transcribe_frame(MIN_SAMPLES)], &mut FakeEngine::default());
        assert!(matches!(&responses[0], Response::Error { code, .. } if code == "not_loaded"));
    }

    #[test]
    fn load_reports_backend() {
        let responses = run(&[load_frame()], &mut FakeEngine::default());
        assert!(matches!(&responses[0], Response::Ready { backend, .. } if backend == "MTL0"));
    }

    #[test]
    fn failed_load_keeps_worker_unloaded() {
        let mut engine = FakeEngine {
            fail_load: true,
            ..Default::default()
        };
        let responses = run(&[load_frame(), transcribe_frame(MIN_SAMPLES)], &mut engine);
        assert!(matches!(&responses[0], Response::Error { code, .. } if code == "load_failed"));
        assert!(matches!(&responses[1], Response::Error { code, .. } if code == "not_loaded"));
    }

    #[test]
    fn transcribe_passes_audio_vocabulary_and_prompt() {
        let mut engine = FakeEngine::default();
        let responses = run(&[load_frame(), transcribe_frame(MIN_SAMPLES)], &mut engine);
        assert!(matches!(&responses[1], Response::Transcript { text, .. } if text == "你好"));
        assert_eq!(
            engine.calls,
            vec![(
                MIN_SAMPLES,
                vec!["TypeScript".to_string()],
                Some("前文".to_string())
            )]
        );
    }

    #[test]
    fn very_short_audio_returns_empty_without_calling_engine() {
        let mut engine = FakeEngine::default();
        let responses = run(
            &[load_frame(), transcribe_frame(MIN_SAMPLES - 1)],
            &mut engine,
        );
        assert!(matches!(&responses[1], Response::Transcript { text, .. } if text.is_empty()));
        assert!(engine.calls.is_empty());
    }

    #[test]
    fn shutdown_stops_the_loop() {
        let responses = run(
            &[(Request::Shutdown, vec![]), load_frame()],
            &mut FakeEngine::default(),
        );
        assert!(responses.is_empty());
    }

    #[test]
    fn oversized_audio_is_rejected() {
        let mut output = Vec::new();
        let frame = (
            Request::Transcribe {
                vocabulary: vec![],
                prompt: None,
                pcm_bytes: MAX_PCM_BYTES + 4,
            },
            vec![],
        );
        assert!(serve(
            &mut input(&[frame]),
            &mut output,
            &mut FakeEngine::default()
        )
        .is_err());
    }
}
