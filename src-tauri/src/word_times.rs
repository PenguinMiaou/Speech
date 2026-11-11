//! When each word of a meeting was said. The recognizer that writes the words tells no
//! times, so a small second recognizer that does is run over the kept sound, and what it
//! heard is matched to the words that were written.

use std::path::{Path, PathBuf};

use crate::voice_isolation::ModelFile;

const RATE: u64 = 16_000;

/// The SenseVoice small model (FunAudioLLM, in the form sherpa-onnx loads) and its list
/// of tokens. It writes each token with the time it is said at.
pub const MODEL_FILES: [ModelFile; 2] = [
    ModelFile {
        file_name: "word-times.onnx",
        url: "https://huggingface.co/csukuangfj/sherpa-onnx-sense-voice-zh-en-ja-ko-yue-2024-07-17/resolve/2365baeacb507f821a0c8120fcee3d484dba7a07/model.int8.onnx",
        size_bytes: 239_233_841,
        sha256: "c71f0ce00bec95b07744e116345e33d8cbbe08cef896382cf907bf4b51a2cd51",
    },
    ModelFile {
        file_name: "word-times-tokens.txt",
        url: "https://huggingface.co/csukuangfj/sherpa-onnx-sense-voice-zh-en-ja-ko-yue-2024-07-17/resolve/2365baeacb507f821a0c8120fcee3d484dba7a07/tokens.txt",
        size_bytes: 315_894,
        sha256: "f449eb28dc567533d7fa59be34e2abca8784f771850c78a47fb731a31429a1dc",
    },
];

pub fn model_dir() -> Option<PathBuf> {
    crate::speakers::model_dir().map(|speakers| speakers.with_file_name("times"))
}

/// The model and its tokens, when both are fully downloaded.
pub fn installed_models() -> Option<(PathBuf, PathBuf)> {
    let dir = model_dir()?;
    let whole = |file: &ModelFile| {
        crate::voice_isolation::is_downloaded(&dir, file).then(|| dir.join(file.file_name))
    };
    Some((whole(&MODEL_FILES[0])?, whole(&MODEL_FILES[1])?))
}

/// The words under one heading of a meeting, with where their sound begins and ends.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct Span {
    pub start_ms: u64,
    pub end_ms: u64,
    pub text: String,
}

/// Sound shorter than this holds no whole word.
const SHORTEST_MS: u64 = 300;
/// A stretch longer than this is not heard in one piece.
const LONGEST_MS: u64 = 120_000;

/// Where the time is known in the words of each span, in order: `hear` is given the
/// sound of one span at a time and tells what it hears in it. A span whose sound cannot
/// be heard alone keeps no times.
fn timed(
    pcm: &[f32],
    spans: &[Span],
    mut hear: impl FnMut(&[f32]) -> Result<Vec<(String, u32)>, String>,
) -> Result<Vec<Vec<(u32, u64)>>, String> {
    let sample = |ms: u64| ((ms * RATE / 1000) as usize).min(pcm.len());
    spans
        .iter()
        .map(|span| {
            let (from, to) = (sample(span.start_ms), sample(span.end_ms));
            let ms = (to.saturating_sub(from) as u64) * 1000 / RATE;
            if !(SHORTEST_MS..=LONGEST_MS).contains(&ms) || span.text.trim().is_empty() {
                return Ok(Vec::new());
            }
            Ok(align(&span.text, &hear(&pcm[from..to])?, span.start_ms))
        })
        .collect()
}

/// Tells when the words of a kept recording were said, span by span. This runs in a
/// worker of its own, so that dictation is not kept waiting.
pub fn of(recording: &Path, spans: &[Span]) -> Result<Vec<Vec<(u32, u64)>>, String> {
    let missing = || "word_times_not_installed".to_string();
    let (model, tokens) = installed_models().ok_or_else(missing)?;
    let scratch = model_dir().ok_or_else(missing)?;
    let command = crate::local_stt::default_worker_command().map_err(|e| e.to_string())?;
    let pcm = crate::file_transcribe::decode(recording)?;
    let worker = crate::local_stt::WorkerSupervisor::new(
        command,
        crate::local_stt::REQUEST_TIMEOUT,
        std::time::Duration::from_secs(60),
    );
    let marks = timed(&pcm, spans, |sound| {
        worker
            .word_times(&model, &tokens, sound, &scratch)
            .map_err(|error| crate::local_stt::supervisor::failure_code(&error).to_string())
    });
    worker.stop();
    marks
}

/// Matching longer stretches than this would take more memory than it is worth.
const MOST_COMPARED: usize = 4_000_000;

/// The characters of `text` that count as words, lower-cased, each with what `tag` gives.
fn letters<T: Copy>(text: &str, mut tag: impl FnMut(usize) -> T) -> Vec<(char, T)> {
    let mut all = Vec::new();
    let mut place = 0;
    for character in text.chars() {
        if character.is_alphanumeric() {
            let tagged = tag(place);
            all.extend(character.to_lowercase().map(|lower| (lower, tagged)));
        }
        place += character.len_utf16();
    }
    all
}

/// Where in `text` the time is known: places counted the way the page counts them, each
/// with the millisecond the word there begins at. `tokens` are what the timing recognizer
/// heard, with the millisecond each begins at, counted from `offset_ms`.
pub fn align(text: &str, tokens: &[(String, u32)], offset_ms: u64) -> Vec<(u32, u64)> {
    let written = letters(text, |place| place as u32);
    let heard: Vec<(char, u64)> = tokens
        .iter()
        // The recognizer's own marks, such as the language, are not words.
        .filter(|(token, _)| !token.starts_with("<|"))
        .flat_map(|(token, ms)| letters(token, |_| offset_ms + *ms as u64))
        .collect();
    let (rows, columns) = (written.len(), heard.len());
    if rows == 0 || columns == 0 || rows.saturating_mul(columns) > MOST_COMPARED {
        return Vec::new();
    }
    // The longest run of characters both have in common, in order.
    let width = columns + 1;
    let mut common = vec![0u16; (rows + 1) * width];
    for row in (0..rows).rev() {
        for column in (0..columns).rev() {
            common[row * width + column] = if written[row].0 == heard[column].0 {
                common[(row + 1) * width + column + 1].saturating_add(1)
            } else {
                common[(row + 1) * width + column].max(common[row * width + column + 1])
            };
        }
    }
    let mut marks: Vec<(u32, u64)> = Vec::new();
    let (mut row, mut column) = (0, 0);
    while row < rows && column < columns {
        if written[row].0 == heard[column].0 {
            let (place, ms) = (written[row].1, heard[column].1);
            // The characters of one piece share its time; the first of them carries it.
            if marks
                .last()
                .is_none_or(|last| ms > last.1 && place > last.0)
            {
                marks.push((place, ms));
            }
            row += 1;
            column += 1;
        } else if common[(row + 1) * width + column] >= common[row * width + column + 1] {
            row += 1;
        } else {
            column += 1;
        }
    }
    marks
}

#[cfg(test)]
mod tests {
    use super::*;

    fn heard(tokens: &[(&str, u32)]) -> Vec<(String, u32)> {
        tokens
            .iter()
            .map(|(token, ms)| (token.to_string(), *ms))
            .collect()
    }

    /// Needs the worker, the timing model and a recording. Run with
    /// `OT_WORKER=<ot-stt-worker> OT_SV=<dir> OT_WAV=<wav> cargo test hears_a_kept -- --ignored --nocapture`.
    /// Prints counts only.
    #[test]
    #[ignore]
    fn hears_a_kept_recording() {
        let dir = PathBuf::from(std::env::var("OT_SV").unwrap());
        let pcm =
            crate::file_transcribe::decode(Path::new(&std::env::var("OT_WAV").unwrap())).unwrap();
        let worker = crate::local_stt::WorkerSupervisor::new(
            crate::local_stt::WorkerCommand {
                program: std::env::var("OT_WORKER").unwrap().into(),
                args: vec!["serve".to_string()],
            },
            crate::local_stt::REQUEST_TIMEOUT,
            std::time::Duration::from_secs(60),
        );
        let started = std::time::Instant::now();
        let mut tokens = 0;
        for piece in pcm.chunks(16_000 * 25).take(8) {
            tokens += worker
                .word_times(
                    &dir.join("model.int8.onnx"),
                    &dir.join("tokens.txt"),
                    piece,
                    &std::env::temp_dir(),
                )
                .unwrap()
                .len();
        }
        worker.stop();
        println!(
            "8 pieces of 25 s: {tokens} tokens in {:.1}s",
            started.elapsed().as_secs_f32()
        );
        assert!(tokens > 0);
    }

    /// How much of what the main recognizer writes gets a time. Needs both models:
    /// `OT_WORKER OT_SV OT_MODEL=<gguf> OT_WAV`, `--ignored --nocapture`. Prints counts only.
    #[test]
    #[ignore]
    fn times_most_of_the_words_written_from_a_real_recording() {
        let dir = PathBuf::from(std::env::var("OT_SV").unwrap());
        let model = PathBuf::from(std::env::var("OT_MODEL").unwrap());
        let pcm =
            crate::file_transcribe::decode(Path::new(&std::env::var("OT_WAV").unwrap())).unwrap();
        let worker = crate::local_stt::WorkerSupervisor::new(
            crate::local_stt::WorkerCommand {
                program: std::env::var("OT_WORKER").unwrap().into(),
                args: vec!["serve".to_string()],
            },
            crate::local_stt::REQUEST_TIMEOUT,
            std::time::Duration::from_secs(60),
        );
        let (mut letters_written, mut timed_places, mut widest_gap) = (0, 0, 0u64);
        for piece in pcm.chunks(16_000 * 25).skip(4).take(8) {
            let text = crate::file_transcribe::recognize_in(&worker, &model, piece, &[], None, &[])
                .unwrap();
            let tokens = worker
                .word_times(
                    &dir.join("model.int8.onnx"),
                    &dir.join("tokens.txt"),
                    piece,
                    &std::env::temp_dir(),
                )
                .unwrap();
            let marks = align(&text, &tokens, 0);
            letters_written += letters(&text, |_| ()).len();
            timed_places += marks.len();
            for pair in marks.windows(2) {
                widest_gap = widest_gap.max(pair[1].1 - pair[0].1);
            }
        }
        worker.stop();
        println!(
            "{letters_written} letters written, {timed_places} places timed, widest gap {widest_gap} ms"
        );
    }

    #[test]
    fn each_paragraph_is_heard_alone_and_timed_from_where_it_begins() {
        let pcm = vec![0.0f32; 16_000 * 10];
        let spans = [
            Span {
                start_ms: 2_000,
                end_ms: 4_000,
                text: "周五".into(),
            },
            Span {
                start_ms: 6_000,
                end_ms: 60_000,
                text: "上线".into(),
            },
        ];
        let mut lengths = Vec::new();
        let marks = timed(&pcm, &spans, |sound| {
            lengths.push(sound.len());
            Ok(heard(&[("周", 100), ("五", 300), ("上", 500), ("线", 700)]))
        })
        .unwrap();
        // The second paragraph ends where the sound does.
        assert_eq!(lengths, vec![32_000, 64_000]);
        assert_eq!(
            marks,
            vec![vec![(0, 2_100), (1, 2_300)], vec![(0, 6_500), (1, 6_700)]]
        );
    }

    #[test]
    fn a_paragraph_too_short_or_too_long_to_be_heard_alone_keeps_no_times() {
        let pcm = vec![0.0f32; 16_000 * 200];
        let spans = [
            Span {
                start_ms: 1_000,
                end_ms: 1_100,
                text: "好".into(),
            },
            Span {
                start_ms: 2_000,
                end_ms: 190_000,
                text: "好".into(),
            },
            Span {
                start_ms: 900_000,
                end_ms: 901_000,
                text: "好".into(),
            },
        ];
        let marks = timed(&pcm, &spans, |_| panic!("nothing is to be heard")).unwrap();
        assert_eq!(marks, vec![vec![], vec![], vec![]]);
    }

    #[test]
    fn each_written_character_takes_the_time_of_the_same_character_heard() {
        let tokens = heard(&[
            ("<|zh|>", 0),
            ("开", 720),
            ("饭", 960),
            ("时", 1260),
            ("间", 1500),
            ("早", 1920),
            ("上", 2100),
        ]);
        assert_eq!(
            align("开饭时间：早上", &tokens, 10_000),
            vec![
                (0, 10_720),
                (1, 10_960),
                (2, 11_260),
                (3, 11_500),
                (5, 11_920),
                (6, 12_100)
            ]
        );
    }

    #[test]
    fn words_written_in_letters_are_matched_through_the_pieces_they_were_heard_in() {
        let tokens = heard(&[
            ("▁we", 100),
            ("▁sh", 400),
            ("ip", 550),
            ("▁on", 800),
            ("▁fri", 1000),
            ("day", 1300),
        ]);
        assert_eq!(
            align("We ship on Friday.", &tokens, 0),
            vec![
                (0, 100),
                (3, 400),
                (5, 550),
                (8, 800),
                (11, 1000),
                (14, 1300)
            ]
        );
    }

    #[test]
    fn a_word_heard_differently_is_passed_over() {
        let tokens = heard(&[("周", 0), ("无", 200), ("上", 400), ("线", 600)]);
        assert_eq!(
            align("周五上线", &tokens, 0),
            vec![(0, 0), (2, 400), (3, 600)]
        );
    }

    #[test]
    fn places_are_counted_the_way_the_page_counts_them() {
        assert_eq!(align("😀好", &heard(&[("好", 500)]), 0), vec![(2, 500)]);
    }

    #[test]
    fn nothing_is_told_where_nothing_matches() {
        assert!(align("hello", &heard(&[("你", 0), ("好", 200)]), 0).is_empty());
        assert!(align("", &heard(&[("好", 200)]), 0).is_empty());
    }
}
