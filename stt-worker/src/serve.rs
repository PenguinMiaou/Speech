//! Request loop of the worker. The engine sits behind a trait so the protocol can be tested without a model.

use ot_stt_protocol::{read_frame, write_frame, Request, Response, MAX_PCM_BYTES};
use std::io::{self, Read, Write};
use std::time::Instant;

/// 0.2 seconds at 16 kHz. Shorter audio is an accidental key press, not speech.
pub const MIN_SAMPLES: usize = 3_200;

#[derive(Debug)]
pub struct EngineError {
    pub code: String,
    pub message: String,
}

#[derive(Debug)]
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
    /// Names the language of the audio that follows, or leaves it to the model with `None`.
    fn set_language(&mut self, _language: Option<String>) {}
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
    /// Tells who spoke when in a whole recording.
    fn diarize(
        &mut self,
        segmentation_model: &str,
        embedding_model: &str,
        pcm: &[f32],
        as_heard: bool,
    ) -> Result<Vec<ot_stt_protocol::SpeakerTurn>, EngineError>;
    /// The voices of the speakers the last speaker request found.
    fn last_voices(&mut self) -> Vec<ot_stt_protocol::SpeakerVoice> {
        Vec::new()
    }
    /// Tells who spoke when in the next window of a running recording.
    fn speakers_window(
        &mut self,
        segmentation_model: &str,
        embedding_model: &str,
        pcm: &[f32],
        first: bool,
    ) -> Result<Vec<ot_stt_protocol::SpeakerTurn>, EngineError>;
    /// Tells what is heard in `pcm` piece by piece, with the millisecond each begins at.
    fn word_times(
        &mut self,
        model: &str,
        tokens: &str,
        pcm: &[f32],
    ) -> Result<Vec<(String, u32)>, EngineError>;
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

/// Reads raw 32-bit samples without holding the file's bytes and the samples at once: a
/// three-hour recording is 690 MB either way.
fn read_pcm_file(path: &str) -> io::Result<Vec<f32>> {
    let file = std::fs::File::open(path)?;
    let mut pcm = Vec::with_capacity((file.metadata()?.len() / 4) as usize);
    let mut reader = io::BufReader::with_capacity(1 << 16, file);
    let mut sample = [0u8; 4];
    loop {
        match reader.read_exact(&mut sample) {
            Ok(()) => pcm.push(f32::from_le_bytes(sample)),
            Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => return Ok(pcm),
            Err(error) => return Err(error),
        }
    }
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
            Request::SpeakersWindow {
                segmentation_model,
                embedding_model,
                pcm_path,
                first,
            } => match read_pcm_file(&pcm_path) {
                Ok(pcm) => {
                    match engine.speakers_window(&segmentation_model, &embedding_model, &pcm, first)
                    {
                        Ok(turns) => Response::Speakers {
                            turns,
                            voices: engine.last_voices(),
                        },
                        Err(e) => error(&e.code, e.message),
                    }
                }
                Err(e) => error("audio_unreadable", e.to_string()),
            },
            Request::Diarize {
                segmentation_model,
                embedding_model,
                pcm_path,
                as_heard,
            } => match read_pcm_file(&pcm_path) {
                Ok(pcm) => {
                    match engine.diarize(&segmentation_model, &embedding_model, &pcm, as_heard) {
                        Ok(turns) => Response::Speakers {
                            turns,
                            voices: engine.last_voices(),
                        },
                        Err(e) => error(&e.code, e.message),
                    }
                }
                Err(e) => error("audio_unreadable", e.to_string()),
            },
            Request::WordTimes {
                model,
                tokens,
                pcm_path,
            } => match read_pcm_file(&pcm_path) {
                Ok(pcm) => match engine.word_times(&model, &tokens, &pcm) {
                    Ok(tokens) => Response::WordTimes { tokens },
                    Err(e) => error(&e.code, e.message),
                },
                Err(e) => error("audio_unreadable", e.to_string()),
            },
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
                language,
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
                    engine.set_language(language);
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
        languages: Vec<Option<String>>,
        isolating: Option<Vec<f32>>,
    }

    impl Engine for FakeEngine {
        fn word_times(
            &mut self,
            _model: &str,
            _tokens: &str,
            pcm: &[f32],
        ) -> Result<Vec<(String, u32)>, EngineError> {
            Ok(vec![("heard".into(), pcm.len() as u32)])
        }

        fn speakers_window(
            &mut self,
            _segmentation_model: &str,
            _embedding_model: &str,
            pcm: &[f32],
            first: bool,
        ) -> Result<Vec<ot_stt_protocol::SpeakerTurn>, EngineError> {
            Ok(vec![ot_stt_protocol::SpeakerTurn {
                start_ms: 0,
                end_ms: pcm.len() as u64,
                speaker: if first { 1 } else { 2 },
            }])
        }

        fn diarize(
            &mut self,
            _segmentation_model: &str,
            _embedding_model: &str,
            pcm: &[f32],
            as_heard: bool,
        ) -> Result<Vec<ot_stt_protocol::SpeakerTurn>, EngineError> {
            Ok(vec![ot_stt_protocol::SpeakerTurn {
                start_ms: 0,
                end_ms: pcm.len() as u64,
                speaker: if as_heard { 7 } else { 1 },
            }])
        }

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
        fn set_language(&mut self, language: Option<String>) {
            self.languages.push(language);
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

    #[test]
    fn a_window_of_a_running_recording_comes_back_as_speaker_turns() {
        let path = std::env::temp_dir().join(format!("ot-window-{}.pcm", std::process::id()));
        let raw: Vec<u8> = [0.5f32, 0.25]
            .iter()
            .flat_map(|s| s.to_le_bytes())
            .collect();
        std::fs::write(&path, raw).unwrap();
        let request = |first| Request::SpeakersWindow {
            segmentation_model: "seg.onnx".into(),
            embedding_model: "emb.onnx".into(),
            pcm_path: path.to_string_lossy().into_owned(),
            first,
        };
        let frames = [(request(true), vec![]), (request(false), vec![])];
        let mut output = Vec::new();
        serve(&mut input(&frames), &mut output, &mut FakeEngine::default()).unwrap();
        let _ = std::fs::remove_file(&path);

        let mut reader = Cursor::new(output);
        let speaker = |response| match response {
            Some(Response::Speakers { turns, .. }) => (turns[0].speaker, turns[0].end_ms),
            other => panic!("{other:?}"),
        };
        assert_eq!(
            speaker(read_frame::<_, Response>(&mut reader).unwrap()),
            (1, 2)
        );
        assert_eq!(
            speaker(read_frame::<_, Response>(&mut reader).unwrap()),
            (2, 2)
        );
    }

    #[test]
    fn a_recording_on_disk_comes_back_as_speaker_turns() {
        let path = std::env::temp_dir().join(format!("ot-diarize-{}.pcm", std::process::id()));
        let samples = [0.5f32, 0.25, 0.125];
        let raw: Vec<u8> = samples.iter().flat_map(|s| s.to_le_bytes()).collect();
        std::fs::write(&path, raw).unwrap();
        let request = |pcm_path: String| Request::Diarize {
            segmentation_model: "seg.onnx".into(),
            embedding_model: "emb.onnx".into(),
            pcm_path,
            as_heard: false,
        };
        let frames = [
            (request(path.to_string_lossy().into_owned()), vec![]),
            (request("/nonexistent/audio.pcm".into()), vec![]),
        ];
        let mut output = Vec::new();
        serve(&mut input(&frames), &mut output, &mut FakeEngine::default()).unwrap();
        let _ = std::fs::remove_file(&path);

        let mut reader = Cursor::new(output);
        assert_eq!(
            read_frame::<_, Response>(&mut reader).unwrap(),
            Some(Response::Speakers {
                turns: vec![ot_stt_protocol::SpeakerTurn {
                    start_ms: 0,
                    end_ms: 3,
                    speaker: 1
                }],
                voices: vec![]
            })
        );
        assert!(matches!(
            read_frame::<_, Response>(&mut reader).unwrap(),
            Some(Response::Error { code, .. }) if code == "audio_unreadable"
        ));
    }

    #[test]
    fn a_recording_can_be_asked_for_as_the_model_heard_it() {
        let path = std::env::temp_dir().join(format!("ot-heard-{}.pcm", std::process::id()));
        std::fs::write(&path, 0.5f32.to_le_bytes()).unwrap();
        let request = Request::Diarize {
            segmentation_model: "turns.gguf".into(),
            embedding_model: "emb.onnx".into(),
            pcm_path: path.to_string_lossy().into_owned(),
            as_heard: true,
        };
        let mut output = Vec::new();
        serve(
            &mut input(&[(request, vec![])]),
            &mut output,
            &mut FakeEngine::default(),
        )
        .unwrap();
        let _ = std::fs::remove_file(&path);
        let answer = read_frame::<_, Response>(&mut Cursor::new(output)).unwrap();
        assert!(matches!(answer, Some(Response::Speakers { turns, .. }) if turns[0].speaker == 7));
    }

    #[test]
    fn the_times_of_the_words_in_a_file_of_sound_are_told() {
        let path = std::env::temp_dir().join(format!("ot-times-{}.pcm", std::process::id()));
        let raw: Vec<u8> = [0.1f32, 0.2].iter().flat_map(|s| s.to_le_bytes()).collect();
        std::fs::write(&path, raw).unwrap();
        let request = |pcm_path: String| Request::WordTimes {
            model: "timing.onnx".into(),
            tokens: "tokens.txt".into(),
            pcm_path,
        };
        let frames = [
            (request(path.to_string_lossy().into_owned()), vec![]),
            (request("/nonexistent/audio.pcm".into()), vec![]),
        ];
        let mut output = Vec::new();
        serve(&mut input(&frames), &mut output, &mut FakeEngine::default()).unwrap();
        let _ = std::fs::remove_file(&path);

        let mut reader = Cursor::new(output);
        assert_eq!(
            read_frame::<_, Response>(&mut reader).unwrap(),
            Some(Response::WordTimes {
                tokens: vec![("heard".into(), 2)]
            })
        );
        assert!(matches!(
            read_frame::<_, Response>(&mut reader).unwrap(),
            Some(Response::Error { code, .. }) if code == "audio_unreadable"
        ));
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
                language: Some("en".into()),
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
    fn transcribe_tells_the_engine_the_language() {
        let mut engine = FakeEngine::default();
        run(&[load_frame(), transcribe_frame(MIN_SAMPLES)], &mut engine);
        assert_eq!(engine.languages, vec![Some("en".to_string())]);
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
                language: None,
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
