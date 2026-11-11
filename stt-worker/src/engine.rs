//! The real speech engine, backed by transcribe.cpp.

use crate::isolate::{Isolator, VoxLock};
use crate::serve::{not_isolating, Engine, EngineError, EngineOutput};
use transcribe_cpp::{Feature, Model, RunOptions, Session};

#[derive(Default)]
pub struct RealEngine {
    session: Option<Session>,
    accepts: Hints,
    /// The language the next audio is in, when the caller knows it.
    language: Option<String>,
    /// The isolation model kept between recordings, with the files it was opened from.
    voice_model: Option<(String, VoxLock)>,
    isolating: Option<(String, Isolator<VoxLock>)>,
    /// The voices heard so far in a recording still running, with the model files.
    live_speakers: Option<(String, crate::diarize::LiveSpeakers)>,
    voices: Vec<ot_stt_protocol::SpeakerVoice>,
    /// The timing recognizer kept between requests, with the file it was opened from.
    timer: Option<(String, crate::timing::Timer)>,
}

/// Which optional hints the loaded model can use.
#[derive(Clone, Copy, Default)]
struct Hints {
    vocabulary: bool,
    prompt: bool,
}

fn run_options(vocabulary: Vec<String>, prompt: Option<String>, accepts: Hints) -> RunOptions {
    RunOptions {
        vocabulary: if accepts.vocabulary {
            vocabulary
        } else {
            Vec::new()
        },
        prompt: prompt.filter(|_| accepts.prompt),
        ..Default::default()
    }
}

fn engine_error(code: &str, error: impl std::fmt::Display) -> EngineError {
    EngineError {
        code: code.to_string(),
        message: error.to_string(),
    }
}

/// Two threads keep isolation well ahead of the recording without crowding recognition.
const ISOLATION_THREADS: i32 = 2;

impl Engine for RealEngine {
    fn isolate_begin(
        &mut self,
        enroll_model: &str,
        chunk_model: &str,
        voice_sample: &str,
    ) -> Result<(), EngineError> {
        self.isolating = None;
        let wanted = [enroll_model, chunk_model, voice_sample].join("\n");
        let model = match self.voice_model.take() {
            Some((opened, model)) if opened == wanted => model,
            _ => VoxLock::open(enroll_model, chunk_model, voice_sample, ISOLATION_THREADS)
                .map_err(|e| engine_error("isolation_failed", e))?,
        };
        self.isolating = Some((wanted, Isolator::new(model)));
        Ok(())
    }

    fn isolate_feed(&mut self, pcm: &[f32]) -> Result<(), EngineError> {
        let (_, isolator) = self.isolating.as_mut().ok_or_else(not_isolating)?;
        isolator.feed(pcm);
        Ok(())
    }

    fn last_voices(&mut self) -> Vec<ot_stt_protocol::SpeakerVoice> {
        std::mem::take(&mut self.voices)
    }

    fn speakers_window(
        &mut self,
        segmentation_model: &str,
        embedding_model: &str,
        pcm: &[f32],
        first: bool,
    ) -> Result<Vec<ot_stt_protocol::SpeakerTurn>, EngineError> {
        let failed = |message| EngineError {
            code: "diarize_failed".into(),
            message,
        };
        let wanted = [segmentation_model, embedding_model].join("\n");
        let keep = !first && matches!(&self.live_speakers, Some((opened, _)) if *opened == wanted);
        if !keep {
            let threads = std::thread::available_parallelism().map_or(2, |n| n.get().min(4)) as i32;
            let live =
                crate::diarize::LiveSpeakers::open(segmentation_model, embedding_model, threads)
                    .map_err(failed)?;
            self.live_speakers = Some((wanted, live));
        }
        let (_, live) = self.live_speakers.as_mut().expect("opened above");
        let (turns, voices) = live.window(pcm).map_err(failed)?;
        self.voices = voices;
        Ok(turns)
    }

    fn diarize(
        &mut self,
        segmentation_model: &str,
        embedding_model: &str,
        pcm: &[f32],
    ) -> Result<Vec<ot_stt_protocol::SpeakerTurn>, EngineError> {
        let threads = std::thread::available_parallelism().map_or(2, |n| n.get().min(4)) as i32;
        let (turns, voices) =
            crate::diarize::diarize(segmentation_model, embedding_model, pcm, threads).map_err(
                |message| EngineError {
                    code: "diarize_failed".into(),
                    message,
                },
            )?;
        self.voices = voices;
        Ok(turns)
    }

    fn word_times(
        &mut self,
        model: &str,
        tokens: &str,
        pcm: &[f32],
    ) -> Result<Vec<(String, u32)>, EngineError> {
        if self
            .timer
            .as_ref()
            .is_none_or(|(opened, _)| opened != model)
        {
            let threads = std::thread::available_parallelism().map_or(2, |n| n.get().min(4)) as i32;
            let timer = crate::timing::Timer::load(model, tokens, threads).map_err(|message| {
                EngineError {
                    code: "timing_failed".into(),
                    message,
                }
            })?;
            self.timer = Some((model.to_string(), timer));
        }
        let (_, timer) = self.timer.as_ref().expect("loaded just above");
        Ok(timer
            .tokens(pcm)
            .into_iter()
            .map(|(token, seconds)| (token, (seconds.max(0.0) * 1000.0) as u32))
            .collect())
    }

    fn isolate_end(&mut self) -> Result<Vec<f32>, EngineError> {
        let (opened, isolator) = self.isolating.take().ok_or_else(not_isolating)?;
        let (isolated, model) = isolator.finish();
        self.voice_model = Some((opened, model));
        Ok(isolated)
    }

    fn load(&mut self, model_path: &str) -> Result<String, EngineError> {
        self.session = None;
        let model = Model::load(model_path).map_err(|e| engine_error("load_failed", e))?;
        let backend = model.backend();
        self.accepts = Hints {
            vocabulary: model.supports(Feature::Vocabulary),
            prompt: model.supports(Feature::ContextPrompt),
        };
        self.session = Some(
            model
                .session()
                .map_err(|e| engine_error("load_failed", e))?,
        );
        Ok(backend)
    }

    fn set_language(&mut self, language: Option<String>) {
        self.language = language.filter(|code| !code.trim().is_empty());
    }

    fn transcribe(
        &mut self,
        pcm: &[f32],
        vocabulary: Vec<String>,
        prompt: Option<String>,
    ) -> Result<EngineOutput, EngineError> {
        let session = self
            .session
            .as_mut()
            .ok_or_else(|| engine_error("not_loaded", "no model loaded"))?;
        let mut options = run_options(vocabulary, prompt, self.accepts);
        options.language = self.language.clone();
        heard(session.run(pcm, &options))
    }
}

/// What a run heard. A decode that was stopped because its words began to repeat, or
/// because it ran out of room, still heard what came before: that is what it gives,
/// and it is no failure.
fn heard(
    run: Result<transcribe_cpp::Transcript, transcribe_cpp::Error>,
) -> Result<EngineOutput, EngineError> {
    let transcript = match run {
        Ok(transcript) => transcript,
        Err(
            error @ (transcribe_cpp::Error::OutputRepetition { .. }
            | transcribe_cpp::Error::OutputTruncated { .. }),
        ) => error.partial().cloned().unwrap_or_default(),
        Err(error) => return Err(engine_error("transcribe_failed", error)),
    };
    Ok(EngineOutput {
        text: transcript.text.trim().to_string(),
        language: transcript.language,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with_vocabulary_and_prompt(accepts: Hints) -> RunOptions {
        run_options(vec!["TypeScript".into()], Some("上一段".into()), accepts)
    }

    #[test]
    fn what_was_heard_before_the_words_began_to_repeat_is_kept() {
        let partial = transcribe_cpp::Transcript {
            text: " we ship on Friday ".into(),
            ..Default::default()
        };
        let looped = transcribe_cpp::Error::OutputRepetition {
            message: "decode stopped".into(),
            partial: Some(Box::new(partial.clone())),
        };
        assert_eq!(heard(Err(looped)).unwrap().text, "we ship on Friday");
        let cut = transcribe_cpp::Error::OutputTruncated {
            message: "budget".into(),
            partial: Some(Box::new(partial)),
        };
        assert_eq!(heard(Err(cut)).unwrap().text, "we ship on Friday");
        let nothing = transcribe_cpp::Error::OutputRepetition {
            message: "decode stopped".into(),
            partial: None,
        };
        assert_eq!(heard(Err(nothing)).unwrap().text, "");
    }

    #[test]
    fn a_run_that_failed_otherwise_is_a_failure() {
        let failed = transcribe_cpp::Error::UnsupportedRole("x".into());
        assert_eq!(heard(Err(failed)).unwrap_err().code, "transcribe_failed");
    }

    #[test]
    fn a_model_that_takes_hints_receives_them() {
        let options = with_vocabulary_and_prompt(Hints {
            vocabulary: true,
            prompt: true,
        });
        assert_eq!(options.vocabulary, vec!["TypeScript".to_string()]);
        assert_eq!(options.prompt.as_deref(), Some("上一段"));
    }

    /// Tries a model that only tells speakers apart, in one pass over the whole recording:
    /// `OT_MODEL=<gguf> OT_WAV=<16 kHz mono 16-bit wav> cargo test --release
    /// tells_who_spoke_when -- --ignored --nocapture`. Prints numbers only.
    #[test]
    #[ignore]
    fn tells_who_spoke_when() {
        let wav = std::fs::read(std::env::var("OT_WAV").unwrap()).unwrap();
        let pcm = crate::eval::decode_wav(&wav).unwrap();
        let model = Model::load(std::env::var("OT_MODEL").unwrap()).unwrap();
        let mut session = model.diarize_session().unwrap();
        let started = std::time::Instant::now();
        let segments = session.run(&pcm, &Default::default()).unwrap();
        println!(
            "audio {:.0}s took {:.1}s on {}, {} segments",
            pcm.len() as f32 / 16_000.0,
            started.elapsed().as_secs_f32(),
            model.backend(),
            segments.len()
        );
        let mut seconds: std::collections::BTreeMap<i32, i64> = Default::default();
        for segment in &segments {
            *seconds.entry(segment.speaker_id).or_default() += segment.t1_ms - segment.t0_ms;
            println!(
                "{:>7} - {:>7} speaker {}",
                segment.t0_ms, segment.t1_ms, segment.speaker_id
            );
        }
        println!("milliseconds by speaker {seconds:?}");
    }

    #[test]
    fn a_model_without_hint_support_is_never_sent_them() {
        let options = with_vocabulary_and_prompt(Hints::default());
        assert!(options.vocabulary.is_empty());
        assert_eq!(options.prompt, None);
    }
    /// Tries a model that tells speakers apart by itself, on a real recording, and says
    /// how much each speaker has and when each turn falls; none of the words. Run with
    /// `OT_MODEL=<gguf> OT_WAV=<16 kHz mono 16-bit wav> cargo test --release
    /// tells_speakers_with_the_model_itself -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn tells_speakers_with_the_model_itself() {
        let wav = std::fs::read(std::env::var("OT_WAV").unwrap()).unwrap();
        let pcm = crate::eval::decode_wav(&wav).unwrap();
        let model = Model::load(std::env::var("OT_MODEL").unwrap()).unwrap();
        let mut session = model.session().unwrap();
        let options = RunOptions {
            diarize: transcribe_cpp::Diarize::On,
            ..Default::default()
        };
        let started = std::time::Instant::now();
        let transcript = session.run(&pcm, &options).unwrap();
        println!(
            "audio {:.0}s took {:.1}s, {} characters, {} segments, {} speaker turns",
            pcm.len() as f32 / 16_000.0,
            started.elapsed().as_secs_f32(),
            transcript.text.chars().count(),
            transcript.segments.len(),
            transcript.speaker_segments.len()
        );
        let mut said: std::collections::BTreeMap<i32, (usize, usize)> = Default::default();
        for segment in &transcript.segments {
            let entry = said.entry(segment.speaker_id).or_default();
            entry.0 += 1;
            entry.1 += segment.text.chars().count();
        }
        println!("segments and characters by speaker {said:?}");
        // The words go to a file when one is named, so that they are not shown.
        if let Ok(out) = std::env::var("OT_OUT") {
            std::fs::write(out, &transcript.text).unwrap();
        }
        for segment in &transcript.segments {
            println!(
                "turn {:>7} - {:>7} speaker {} characters {}",
                segment.t0_ms,
                segment.t1_ms,
                segment.speaker_id,
                segment.text.chars().count()
            );
        }
    }
}
