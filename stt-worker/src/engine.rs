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
        let transcript = session
            .run(pcm, &options)
            .map_err(|e| engine_error("transcribe_failed", e))?;
        Ok(EngineOutput {
            text: transcript.text.trim().to_string(),
            language: transcript.language,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with_vocabulary_and_prompt(accepts: Hints) -> RunOptions {
        run_options(vec!["TypeScript".into()], Some("上一段".into()), accepts)
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

    #[test]
    fn a_model_without_hint_support_is_never_sent_them() {
        let options = with_vocabulary_and_prompt(Hints::default());
        assert!(options.vocabulary.is_empty());
        assert_eq!(options.prompt, None);
    }
}
