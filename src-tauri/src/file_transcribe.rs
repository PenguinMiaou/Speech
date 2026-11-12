//! Turns an audio or video file into text with the speech model on this machine.
//! The file is decoded, brought to the model's sample rate and cut at pauses into
//! pieces short enough to recognize one at a time.

use std::ops::Range;
use std::path::Path;

const RATE: u32 = 16_000;
/// Longest piece handed to the model, in seconds.
const PIECE_SECS: usize = 30;
/// How far back from the end of a piece a pause is looked for.
const SEARCH_SECS: usize = 6;
const FRAME: usize = RATE as usize / 5;

/// Longest file taken, so the decoded audio fits in memory.
const LONGEST_SECS: usize = 3 * 60 * 60;

/// The whole file as one channel at the model's sample rate.
pub fn decode(path: &Path) -> Result<Vec<f32>, String> {
    use symphonia::core::audio::SampleBuffer;
    use symphonia::core::codecs::{DecoderOptions, CODEC_TYPE_NULL};
    use symphonia::core::errors::Error;
    use symphonia::core::formats::FormatOptions;
    use symphonia::core::io::MediaSourceStream;
    use symphonia::core::meta::MetadataOptions;
    use symphonia::core::probe::Hint;

    let file = std::fs::File::open(path).map_err(|_| "file_unreadable".to_string())?;
    let stream = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(extension) = path.extension().and_then(|extension| extension.to_str()) {
        hint.with_extension(extension);
    }
    let mut format = symphonia::default::get_probe()
        .format(
            &hint,
            stream,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .map_err(|_| "file_not_audio".to_string())?
        .format;
    let track = format
        .tracks()
        .iter()
        .find(|track| track.codec_params.codec != CODEC_TYPE_NULL)
        .ok_or_else(|| "file_not_audio".to_string())?;
    let track_id = track.id;
    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .map_err(|_| "file_not_audio".to_string())?;

    let mut pcm = Vec::new();
    let mut resampler: Option<(u32, crate::audio::resample::Resampler)> = None;
    loop {
        let packet = match format.next_packet() {
            Ok(packet) => packet,
            // The end of the file arrives as an error.
            Err(Error::IoError(_)) | Err(Error::ResetRequired) => break,
            Err(_) => return Err("file_not_audio".to_string()),
        };
        if packet.track_id() != track_id {
            continue;
        }
        let decoded = match decoder.decode(&packet) {
            Ok(decoded) => decoded,
            // One damaged packet is skipped; the rest of the file still counts.
            Err(Error::DecodeError(_)) => continue,
            Err(_) => break,
        };
        let spec = *decoded.spec();
        let channels = spec.channels.count().max(1);
        let mut samples = SampleBuffer::<f32>::new(decoded.capacity() as u64, spec);
        samples.copy_interleaved_ref(decoded);
        let mono: Vec<f32> = samples
            .samples()
            .chunks(channels)
            .map(|frame| frame.iter().sum::<f32>() / frame.len() as f32)
            .collect();
        if resampler.as_ref().map(|(rate, _)| *rate) != Some(spec.rate) {
            resampler = Some((
                spec.rate,
                crate::audio::resample::Resampler::new(spec.rate, RATE),
            ));
        }
        if let Some((_, resampler)) = resampler.as_mut() {
            pcm.extend(
                resampler
                    .process(&mono)
                    .into_iter()
                    .map(|sample| sample.clamp(-1.0, 1.0)),
            );
        }
        if pcm.len() > LONGEST_SECS * RATE as usize {
            return Err("file_too_long".to_string());
        }
    }
    if pcm.is_empty() {
        return Err("file_not_audio".to_string());
    }
    Ok(pcm)
}

/// Where to cut the audio: every piece is short enough for the model, and each cut sits
/// in the quietest moment near the end of its piece.
pub fn pieces(pcm: &[f32]) -> Vec<Range<usize>> {
    let longest = PIECE_SECS * RATE as usize;
    let search = SEARCH_SECS * RATE as usize;
    let mut cut = Vec::new();
    let mut start = 0;
    while pcm.len() - start > longest {
        let end = start + longest;
        let quietest = (end - search..end - FRAME + 1)
            .step_by(FRAME / 2)
            .min_by(|a, b| {
                let energy =
                    |from: &usize| -> f32 { pcm[*from..*from + FRAME].iter().map(|s| s * s).sum() };
                energy(a).total_cmp(&energy(b))
            })
            .unwrap_or(end - FRAME);
        let at = quietest + FRAME / 2;
        cut.push(start..at);
        start = at;
    }
    if start < pcm.len() {
        cut.push(start..pcm.len());
    }
    cut
}

/// The most languages a meeting can be said to be held in.
pub const MOST_LANGUAGES: usize = 3;

/// The languages the user named, tidied: no blanks, none twice, no more than three.
pub fn chosen_languages(languages: Vec<String>) -> Vec<String> {
    let mut chosen: Vec<String> = Vec::new();
    for code in languages {
        let code = code.trim().to_lowercase();
        if !code.is_empty() && !chosen.contains(&code) && chosen.len() < MOST_LANGUAGES {
            chosen.push(code);
        }
    }
    chosen
}

/// Whether the language the model heard is one of those the meeting is held in.
/// Cantonese counts as Chinese.
pub fn spoken_in(languages: &[String], heard: &str) -> bool {
    let plain = |code: &str| {
        let code = code.trim().to_lowercase();
        let code = code
            .split(['-', '_'])
            .next()
            .unwrap_or_default()
            .to_string();
        if code == "yue" {
            "zh".to_string()
        } else {
            code
        }
    };
    languages.iter().any(|code| plain(code) == plain(heard))
}

/// Recognizes speech in a meeting held in `languages`. One language is told to the model.
/// With several the model tells the language itself, and words it hears in a language
/// that is not among them are recognized again as the first one.
pub fn recognize_in(
    supervisor: &crate::local_stt::WorkerSupervisor,
    model: &Path,
    pcm: &[f32],
    vocabulary: &[String],
    before: Option<&str>,
    languages: &[String],
) -> Result<String, String> {
    // A recognizer away from this machine, where the settings name one, hears it all.
    if let Some(chosen) = crate::stt::batch::chosen() {
        return chosen
            .hear(pcm, vocabulary)
            .map(|text| without_loops(&text));
    }
    let ask = |language: Option<String>| {
        supervisor
            .transcribe_heard(
                model,
                pcm,
                vocabulary.to_vec(),
                before.map(str::to_string),
                language,
            )
            .map_err(|error| error.to_string())
    };
    let only = (languages.len() == 1).then(|| languages[0].clone());
    let (text, heard) = ask(only)?;
    let elsewhere = languages.len() > 1 && heard.is_some_and(|code| !spoken_in(languages, &code));
    if elsewhere && !text.trim().is_empty() {
        return ask(Some(languages[0].clone()))
            .map(|(text, _)| without_echo(&without_loops(&text), before));
    }
    Ok(without_echo(&without_loops(&text), before))
}

/// The longest run of characters looked at as one thing said over and over.
const LONGEST_LOOP: usize = 20;

/// How often a run of `length` characters must follow itself to count as the recognizer
/// being stuck. People do say a short word several times; they do not say a phrase
/// three times running, or one sound eight times.
fn stuck_after(length: usize) -> usize {
    match length {
        1 => 8,
        2 | 3 => 4,
        _ => 3,
    }
}

/// The text with words the recognizer got stuck on said once. On noise or silence a
/// recognizer sometimes writes the same words over and over; nobody said them that often.
pub fn without_loops(text: &str) -> String {
    let all: Vec<char> = text.chars().collect();
    let mut kept = String::with_capacity(text.len());
    let mut at = 0;
    let mut cut = false;
    'next: while at < all.len() {
        for length in 1..=LONGEST_LOOP.min((all.len() - at) / 2) {
            let unit = &all[at..at + length];
            // Figures and marks do repeat: a number with many noughts is not a loop.
            if !unit.iter().any(|character| character.is_alphabetic()) {
                continue;
            }
            let mut times = 1;
            while all[at + times * length..].starts_with(unit) {
                times += 1;
            }
            if times >= stuck_after(length) {
                kept.extend(unit);
                at += times * length;
                // The last time round may lack the space or mark the others end with.
                let bare: Vec<char> = unit.iter().collect::<String>().trim_end().chars().collect();
                if bare.len() < length && all[at..].starts_with(&bare) {
                    at += bare.len();
                }
                cut = true;
                continue 'next;
            }
        }
        kept.push(all[at]);
        at += 1;
    }
    if cut && text.trim_end().len() == text.len() {
        kept.truncate(kept.trim_end().len());
    }
    kept
}

/// How much said again counts as the recognizer writing down what it was given as having
/// come before: eight words, or sixteen characters of a writing without spaces.
const ECHO: usize = 16;
/// Once that is found, shorter pieces of what came before go as well.
const ECHO_REST: usize = 6;

/// One word, or one character of a writing without spaces: where it lies, how it reads
/// and how much it counts for.
struct Token {
    from: usize,
    reads: String,
    weight: usize,
}

fn tokens(text: &str) -> Vec<Token> {
    let mut found: Vec<Token> = Vec::new();
    let mut within = false;
    for (at, character) in text.char_indices() {
        if !character.is_alphanumeric() {
            within = false;
        } else if character as u32 >= 0x2E80 {
            within = false;
            found.push(Token {
                from: at,
                reads: character.to_string(),
                weight: 1,
            });
        } else {
            match found.last_mut() {
                Some(last) if within => last.reads.extend(character.to_lowercase()),
                _ => found.push(Token {
                    from: at,
                    reads: character.to_lowercase().collect(),
                    weight: 2,
                }),
            }
            within = true;
        }
    }
    found
}

/// The text without what the recognizer was given as having come before. Given the end
/// of what was said so far, a recognizer sometimes writes that down again in place of,
/// or in front of, what it hears.
pub fn without_echo(text: &str, before: Option<&str>) -> String {
    let Some(before) = before else {
        return text.to_string();
    };
    let given = tokens(before);
    let mut kept = text.to_string();
    let mut least = ECHO;
    loop {
        let heard = tokens(&kept);
        // The longest run of words that stand in both, by how much it counts for.
        let mut best = (0, 0, 0);
        let mut runs = vec![(0_usize, 0_usize); given.len() + 1];
        for (index, token) in heard.iter().enumerate() {
            let mut next = vec![(0, 0); given.len() + 1];
            for (other, was) in given.iter().enumerate() {
                if token.reads == was.reads {
                    let (length, weight) = runs[other];
                    next[other + 1] = (length + 1, weight + token.weight);
                    if weight + token.weight > best.2 {
                        best = (index + 1 - (length + 1), index + 1, weight + token.weight);
                    }
                }
            }
            runs = next;
        }
        let (from, to, weight) = best;
        if weight < least {
            break;
        }
        let end = heard.get(to).map_or(kept.len(), |token| token.from);
        kept.replace_range(heard[from].from..end, "");
        least = ECHO_REST;
    }
    if kept.len() == text.len() {
        return kept;
    }
    let kept = kept.trim();
    if kept.chars().any(char::is_alphanumeric) {
        kept.to_string()
    } else {
        String::new()
    }
}

/// Runs `attempt` again after each of the `waits` until it succeeds. A recognizer that
/// crashed or timed out is started afresh by the next request, so trying again often works.
pub fn patiently<T>(
    waits: &[std::time::Duration],
    mut attempt: impl FnMut() -> Result<T, String>,
) -> Result<T, String> {
    let mut outcome = attempt();
    for wait in waits {
        if outcome.is_ok() {
            break;
        }
        std::thread::sleep(*wait);
        outcome = attempt();
    }
    outcome
}

/// How long to wait before each further try at a piece that could not be recognized.
pub const TRY_AGAIN_AFTER: [std::time::Duration; 2] = [
    std::time::Duration::from_secs(1),
    std::time::Duration::from_secs(4),
];

/// What the caller wants from a transcription.
#[derive(Debug, Clone, Default)]
pub struct Options {
    /// Write the text out by speaker, each named with this word and a number.
    pub speaker_label: Option<String>,
    /// The model to recognize with. The dictation model when absent or not installed.
    pub model_id: Option<String>,
    /// Words from the user's dictionary the model should expect.
    pub vocabulary: Vec<String>,
    /// The languages spoken, when the user named them: three at most.
    pub languages: Vec<String>,
}

/// What a caller hears while a recording is being worked on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stage {
    /// Finding who spoke when. It has no steps to count.
    Speakers,
    Text {
        done: usize,
        total: usize,
    },
    /// The text written out so far, for the caller to show while the rest is worked on.
    Written(String),
}

/// The pieces to recognize, each with the index of the turn it belongs to.
pub fn plan(
    pcm: &[f32],
    turns: Option<&[crate::speakers::SpeakerTurn]>,
) -> Vec<(Option<usize>, Range<usize>)> {
    let Some(turns) = turns else {
        return pieces(pcm).into_iter().map(|piece| (None, piece)).collect();
    };
    turns
        .iter()
        .enumerate()
        .flat_map(|(index, turn)| {
            let span = crate::speakers::samples(turn, pcm.len());
            let from = span.start;
            pieces(&pcm[span])
                .into_iter()
                .map(move |piece| (Some(index), from + piece.start..from + piece.end))
        })
        .collect()
}

pub(crate) fn joined(parts: &[&str]) -> String {
    let mut text = String::new();
    for part in parts {
        let latin = |c: Option<char>| c.is_some_and(|c| c.is_ascii() && !c.is_ascii_whitespace());
        if latin(text.chars().last()) && latin(part.chars().next()) {
            text.push(' ');
        }
        text.push_str(part);
    }
    text
}

/// The recognized pieces as the final text.
pub fn assemble(
    said: &[(Option<usize>, String)],
    turns: Option<&[crate::speakers::SpeakerTurn]>,
    speaker_label: &str,
) -> String {
    let Some(turns) = turns else {
        return said
            .iter()
            .map(|(_, text)| text.as_str())
            .collect::<Vec<_>>()
            .join("\n");
    };
    let by_turn: Vec<_> = turns
        .iter()
        .enumerate()
        .map(|(index, turn)| {
            let parts: Vec<&str> = said
                .iter()
                .filter(|(of, _)| *of == Some(index))
                .map(|(_, text)| text.as_str())
                .collect();
            (*turn, joined(&parts))
        })
        .collect();
    crate::speakers::transcript(&by_turn, speaker_label)
}

pub(crate) fn model_path(wanted: Option<&str>) -> Option<std::path::PathBuf> {
    use crate::local_stt::catalog;
    wanted
        .and_then(catalog::installed_path)
        .or_else(|| catalog::installed_path(catalog::DEFAULT_LOCAL_MODEL_ID))
        .or_else(|| {
            catalog::entries()
                .iter()
                .find_map(|entry| catalog::installed_path(entry.id))
        })
}

/// Set when the user asks for the file in hand to be let be.
static STOP: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// What a transcription that was stopped ends with.
pub const STOPPED: &str = "stopped";

/// Asks for the transcription in hand to end before its next piece, or takes that back.
pub fn stop(asked: bool) {
    STOP.store(asked, std::sync::atomic::Ordering::SeqCst);
}

fn stopped() -> Result<(), String> {
    if STOP.load(std::sync::atomic::Ordering::SeqCst) {
        Err(STOPPED.to_string())
    } else {
        Ok(())
    }
}

/// How many pieces one after another may fail before the file is given up.
const FAILED_IN_A_ROW: u32 = 3;

/// What a piece is taken to hold. A piece that could not be heard is left out, so that
/// one bad stretch does not cost the whole file; `failed` counts such pieces in a row,
/// and the file ends when there are too many, when it was let be, or when no model is
/// there to hear with.
fn forgiving(
    failed: &std::cell::Cell<u32>,
    outcome: Result<String, String>,
) -> Result<String, String> {
    match outcome {
        Ok(text) => {
            failed.set(0);
            Ok(text)
        }
        Err(error) => {
            let hopeless = error == STOPPED || error.contains("model") || error.contains("stt_");
            failed.set(failed.get() + 1);
            if hopeless || failed.get() >= FAILED_IN_A_ROW {
                return Err(error);
            }
            crate::applog::note("file", "a piece could not be heard and is left out");
            Ok(String::new())
        }
    }
}

/// Recognizes a whole file. See [`transcribe_pcm`].
pub fn transcribe(
    path: &Path,
    options: &Options,
    progress: impl FnMut(Stage),
) -> Result<String, String> {
    transcribe_pcm(&decode(path)?, options, progress)
}

/// The model on this machine to hear with. None is needed, and an empty path stands for
/// it, where the settings name a recognizer away from this machine.
pub fn model_for(model_id: Option<&str>) -> Result<std::path::PathBuf, String> {
    if crate::stt::batch::chosen().is_some() {
        return Ok(std::path::PathBuf::new());
    }
    model_path(model_id).ok_or_else(|| "local_model_not_installed".to_string())
}

/// Recognizes a recording piece by piece, by speaker when asked and the speaker models
/// are installed.
pub fn transcribe_pcm(
    pcm: &[f32],
    options: &Options,
    progress: impl FnMut(Stage),
) -> Result<String, String> {
    let model = model_for(options.model_id.as_deref())?;
    let supervisor =
        crate::local_stt::global().ok_or_else(|| "local_engine_unavailable".to_string())?;
    let speakers = match &options.speaker_label {
        Some(label) => {
            let missing = || "speaker_models_not_installed".to_string();
            let (segmentation, voice) = crate::speakers::models_for_a_file().ok_or_else(missing)?;
            Some(SpeakerModels {
                segmentation,
                voice,
                scratch: crate::speakers::model_dir().ok_or_else(missing)?,
                label: label.clone(),
                command: crate::local_stt::default_worker_command()
                    .map_err(|error| error.to_string())?,
            })
        }
        None => None,
    };
    recognize(
        &supervisor,
        &model,
        speakers.as_ref(),
        &options.vocabulary,
        &options.languages,
        pcm,
        progress,
    )
}

struct SpeakerModels {
    segmentation: std::path::PathBuf,
    voice: std::path::PathBuf,
    scratch: std::path::PathBuf,
    label: String,
    /// Starts the worker that tells the speakers apart while the other one recognizes.
    command: crate::local_stt::WorkerCommand,
}

/// How much of a recording has its speakers told apart at a time. A part is heard while
/// the one before it is recognized, so the text of a long recording begins to show after
/// the first part and not after the whole.
const PART_SECS: usize = 8 * 60;

/// A recording of `length` samples in parts of `part`; a last part under half that long
/// goes with the one before it.
fn parts_of(length: usize, part: usize) -> Vec<Range<usize>> {
    let mut parts: Vec<Range<usize>> = (0..length)
        .step_by(part.max(1))
        .map(|from| from..(from + part).min(length))
        .collect();
    if parts.len() > 1 && parts[parts.len() - 1].len() < part / 2 {
        parts.pop();
        let last = parts.len() - 1;
        parts[last].end = length;
    }
    parts
}

/// Tells who spoke when in a part of a recording; true for its first part. The speakers
/// keep their numbers from part to part.
type Tell<'a> =
    &'a mut (dyn FnMut(&[f32], bool) -> Result<Vec<crate::speakers::SpeakerTurn>, String> + Send);

/// Writes a recording out part by part: `tell` goes ahead through the parts while `hear`
/// recognizes the pieces of the parts already told. Without `tell` the speakers are not
/// told apart. The count in `progress` is in seconds of the recording.
fn in_parts(
    pcm: &[f32],
    part: usize,
    label: &str,
    tell: Option<Tell>,
    mut hear: impl FnMut(&[f32]) -> Result<String, String>,
    mut progress: impl FnMut(Stage),
) -> Result<String, String> {
    let total = pcm.len() / RATE as usize;
    let Some(tell) = tell else {
        let mut said = Vec::new();
        progress(Stage::Text { done: 0, total });
        for piece in pieces(pcm) {
            let text = hear(&pcm[piece.clone()])?;
            let text = text.trim();
            if !text.is_empty() {
                said.push((None, text.to_string()));
                progress(Stage::Written(assemble(&said, None, label)));
            }
            progress(Stage::Text {
                done: (piece.end / RATE as usize).min(total),
                total,
            });
        }
        return Ok(assemble(&said, None, label));
    };
    progress(Stage::Speakers);
    let parts = parts_of(pcm.len(), part);
    std::thread::scope(|scope| {
        let (found, told) = std::sync::mpsc::channel();
        let ahead = parts.clone();
        scope.spawn(move || {
            for (index, range) in ahead.into_iter().enumerate() {
                let turns = tell(&pcm[range], index == 0);
                let failed = turns.is_err();
                // Nobody waits any more once the writing has ended.
                if found.send(turns).is_err() || failed {
                    break;
                }
            }
        });
        let mut turns: Vec<crate::speakers::SpeakerTurn> = Vec::new();
        let mut said = Vec::new();
        for range in &parts {
            let here = told
                .recv()
                .map_err(|_| "local_engine_unavailable".to_string())??;
            let here = crate::speakers::merge_turns(&here);
            let sound = &pcm[range.clone()];
            let before = turns.len();
            let offset_ms = (range.start * 1000 / RATE as usize) as u64;
            turns.extend(here.iter().map(|turn| crate::speakers::SpeakerTurn {
                start_ms: turn.start_ms + offset_ms,
                end_ms: turn.end_ms + offset_ms,
                speaker: turn.speaker,
            }));
            for (turn, piece) in plan(sound, Some(&here)) {
                let text = hear(&sound[piece.clone()])?;
                let text = text.trim();
                if !text.is_empty() {
                    said.push((turn.map(|index| before + index), text.to_string()));
                    progress(Stage::Written(assemble(&said, Some(&turns), label)));
                }
                progress(Stage::Text {
                    done: ((range.start + piece.end) / RATE as usize).min(total),
                    total,
                });
            }
            progress(Stage::Text {
                done: (range.end / RATE as usize).min(total),
                total,
            });
        }
        Ok(assemble(&said, Some(&turns), label))
    })
}

fn recognize(
    supervisor: &crate::local_stt::WorkerSupervisor,
    model: &Path,
    speakers: Option<&SpeakerModels>,
    vocabulary: &[String],
    languages: &[String],
    pcm: &[f32],
    progress: impl FnMut(Stage),
) -> Result<String, String> {
    let failed = std::cell::Cell::new(0);
    let hear = |sound: &[f32]| {
        stopped()?;
        forgiving(
            &failed,
            patiently(&TRY_AGAIN_AFTER, || {
                recognize_in(supervisor, model, sound, vocabulary, None, languages)
            }),
        )
    };
    let Some(models) = speakers else {
        return in_parts(pcm, PART_SECS * RATE as usize, "", None, hear, progress);
    };
    let worker = crate::local_stt::WorkerSupervisor::new(
        models.command.clone(),
        crate::local_stt::REQUEST_TIMEOUT,
        std::time::Duration::from_secs(60),
    );
    // A model that follows the speakers is given the whole recording once; each part
    // then takes its own stretch of that answer.
    let follows = crate::speakers::follows(&models.segmentation);
    let mut whole = Vec::new();
    let mut at = 0;
    let mut tell = |sound: &[f32], first: bool| {
        stopped()?;
        if follows {
            if first {
                let started = std::time::Instant::now();
                let followed = worker.diarize_unless(
                    &models.segmentation,
                    &models.voice,
                    pcm,
                    &models.scratch,
                    &|| stopped().is_err(),
                );
                // Asked to stop while the speakers were followed, the file is let be.
                stopped()?;
                let (found, voices) = followed.map_err(|error| {
                    crate::applog::note(
                        "file",
                        &format!("the speakers could not be followed: {error}"),
                    );
                    error.to_string()
                })?;
                crate::applog::note(
                    "file",
                    &crate::speakers::followed(&models.segmentation, &found, started.elapsed()),
                );
                crate::voiceprints::hear(voices, true);
                whole = found;
                at = 0;
            }
            let ms = |samples: usize| (samples as u64) * 1000 / u64::from(RATE);
            let here = crate::speakers::within(&whole, ms(at), ms(at + sound.len()));
            at += sound.len();
            return Ok(here);
        }
        let (found, voices) = worker
            .speakers_window(
                &models.segmentation,
                &models.voice,
                sound,
                &models.scratch,
                first,
            )
            .map_err(|error| error.to_string())?;
        crate::voiceprints::hear(voices, first);
        Ok(found)
    };
    let written = in_parts(
        pcm,
        PART_SECS * RATE as usize,
        &models.label,
        Some(&mut tell),
        hear,
        progress,
    );
    worker.stop();
    written
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_piece_that_cannot_be_heard_is_left_out_and_the_rest_goes_on() {
        let answers = std::cell::RefCell::new(vec![
            Ok("one".to_string()),
            Err("engine_slow".to_string()),
            Ok("three".to_string()),
        ]);
        let failed = std::cell::Cell::new(0);
        let mut said = Vec::new();
        for _ in 0..3 {
            said.push(forgiving(&failed, answers.borrow_mut().remove(0)).unwrap());
        }
        assert_eq!(said, vec!["one", "", "three"]);
    }

    #[test]
    fn pieces_that_fail_one_after_another_end_the_file() {
        let failed = std::cell::Cell::new(0);
        let fail = || forgiving(&failed, Err("engine_slow".to_string()));
        assert_eq!(fail().unwrap(), "");
        assert_eq!(fail().unwrap(), "");
        assert_eq!(fail().unwrap_err(), "engine_slow");
    }

    #[test]
    fn a_file_that_was_let_be_or_has_no_model_ends_at_once() {
        let failed = std::cell::Cell::new(0);
        assert!(forgiving(&failed, Err(STOPPED.to_string())).is_err());
        let failed = std::cell::Cell::new(0);
        assert!(forgiving(&failed, Err("model_missing".to_string())).is_err());
    }

    #[test]
    fn a_file_that_is_let_be_ends_before_its_next_piece() {
        assert_eq!(stopped(), Ok(()));
        stop(true);
        assert_eq!(stopped(), Err(STOPPED.to_string()));
        stop(false);
        assert_eq!(stopped(), Ok(()));
    }

    #[test]
    fn words_the_recognizer_got_stuck_on_are_said_once() {
        assert_eq!(without_loops("好的好的好的好的好的"), "好的");
        assert_eq!(without_loops("嗯嗯嗯嗯嗯嗯嗯嗯嗯嗯"), "嗯");
        assert_eq!(
            without_loops("Thank you. Thank you. Thank you. Thank you."),
            "Thank you."
        );
        assert_eq!(
            without_loops("下周再说吧，再见再见再见再见再见。"),
            "下周再说吧，再见。"
        );
    }

    #[test]
    fn words_said_twice_or_for_stress_are_kept() {
        for said in [
            "哈哈哈哈",
            "很好很好很好",
            "我们周五上线，我们周五测试",
            "no no no, not that one",
            "一二三四一二三四",
            "一共 1000000000 元",
            "然后…………………………",
        ] {
            assert_eq!(without_loops(said), said);
        }
    }

    #[test]
    fn a_meeting_is_held_in_three_languages_at_most() {
        let named = ["en", " ZH ", "en", "", "ja", "ko"]
            .map(str::to_string)
            .to_vec();
        assert_eq!(chosen_languages(named), vec!["en", "zh", "ja"]);
    }

    #[test]
    fn the_language_heard_is_checked_against_those_of_the_meeting() {
        let held_in = ["en".to_string(), "zh".to_string()];
        assert!(spoken_in(&held_in, "EN"));
        assert!(spoken_in(&held_in, "zh-CN"));
        assert!(spoken_in(&held_in, "yue"));
        assert!(!spoken_in(&held_in, "ja"));
    }

    #[test]
    fn a_piece_that_fails_is_tried_again_until_it_works() {
        let none = [std::time::Duration::ZERO; 2];
        let mut tries = 0;
        let outcome = patiently(&none, || {
            tries += 1;
            if tries < 3 {
                Err("worker crashed".to_string())
            } else {
                Ok("words")
            }
        });
        assert_eq!((outcome, tries), (Ok("words"), 3));
        let mut tries = 0;
        let outcome: Result<(), String> = patiently(&none, || {
            tries += 1;
            Err("still down".to_string())
        });
        assert_eq!((outcome, tries), (Err("still down".to_string()), 3));
        let mut tries = 0;
        let _ = patiently(&none, || {
            tries += 1;
            Ok::<_, String>(())
        });
        assert_eq!(tries, 1);
    }

    #[test]
    fn without_speakers_the_plan_is_the_plain_cut() {
        let pcm = tone(70.0);
        let planned = plan(&pcm, None);
        assert_eq!(planned.len(), pieces(&pcm).len());
        assert!(planned.iter().all(|(turn, _)| turn.is_none()));
    }

    #[test]
    fn with_speakers_every_piece_belongs_to_a_turn_and_stays_inside_it() {
        let pcm = tone(100.0);
        let turn = |start_ms, end_ms, speaker| crate::speakers::SpeakerTurn {
            start_ms,
            end_ms,
            speaker,
        };
        let turns = [turn(0, 10_000, 1), turn(10_000, 80_000, 2)];
        let planned = plan(&pcm, Some(&turns));
        assert_eq!(planned[0], (Some(0), 0..160_000));
        let second: Vec<_> = planned
            .iter()
            .filter(|(turn, _)| *turn == Some(1))
            .collect();
        assert!(second.len() >= 3);
        assert_eq!(second[0].1.start, 160_000);
        assert_eq!(second.last().unwrap().1.end, 1_280_000);
        assert!(second
            .iter()
            .all(|(_, piece)| piece.len() <= PIECE_SECS * RATE as usize));
    }

    #[test]
    fn pieces_join_into_one_paragraph_per_turn() {
        let turn = |start_ms, speaker| crate::speakers::SpeakerTurn {
            start_ms,
            end_ms: start_ms + 1_000,
            speaker,
        };
        let turns = [turn(0, 1), turn(5_000, 2)];
        let said = vec![
            (Some(0), "你好。".to_string()),
            (Some(1), "First part".to_string()),
            (Some(1), "second part.".to_string()),
        ];
        assert_eq!(
            assemble(&said, Some(&turns), "Speaker"),
            "Speaker 1  00:00\n你好。\n\nSpeaker 2  00:05\nFirst part second part."
        );
        let plain = vec![(None, "one".to_string()), (None, "two".to_string())];
        assert_eq!(assemble(&plain, None, "Speaker"), "one\ntwo");
    }

    /// Needs the built worker, a speech model, the two speaker models and a recording:
    /// `OT_WORKER=<bin> OT_MODEL=<gguf> OT_SEG=<onnx> OT_EMB=<onnx> OT_FILE=<audio>
    /// cargo test --lib writes_a_real_meeting -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn writes_a_real_meeting_out_by_speaker() {
        let var = |name: &str| std::path::PathBuf::from(std::env::var(name).unwrap());
        let supervisor = crate::local_stt::WorkerSupervisor::new(
            crate::local_stt::WorkerCommand {
                program: var("OT_WORKER"),
                args: vec!["serve".to_string()],
            },
            std::time::Duration::from_secs(120),
            std::time::Duration::from_secs(60),
        );
        let speakers = SpeakerModels {
            segmentation: var("OT_SEG"),
            voice: var("OT_EMB"),
            scratch: std::env::temp_dir(),
            label: "说话人".to_string(),
            command: crate::local_stt::WorkerCommand {
                program: var("OT_WORKER"),
                args: vec!["serve".to_string()],
            },
        };
        let pcm = decode(&var("OT_FILE")).unwrap();
        let started = std::time::Instant::now();
        let mut shown = false;
        let text = recognize(
            &supervisor,
            &var("OT_MODEL"),
            Some(&speakers),
            &[],
            &[],
            &pcm,
            |stage| {
                if let Stage::Written(text) = &stage {
                    if !shown {
                        shown = true;
                        println!(
                            "the first {} characters show after {:.1}s",
                            text.chars().count(),
                            started.elapsed().as_secs_f32()
                        );
                    }
                }
            },
        )
        .unwrap();
        println!(
            "audio {:.0}s took {:.1}s",
            pcm.len() as f32 / RATE as f32,
            started.elapsed().as_secs_f32()
        );
        // How much each numbered speaker said, and who is taken for whom: the ghosts, and
        // with `OT_PEOPLE` the speakers beyond that many.
        let mut said: std::collections::HashMap<u32, usize> = std::collections::HashMap::new();
        let mut speaker = None;
        for line in text.lines() {
            match line.strip_prefix("说话人 ") {
                Some(rest) if rest.contains("  ") => {
                    speaker = rest.split(' ').next().and_then(|n| n.parse::<u32>().ok());
                }
                _ if line.starts_with("说话人  ") => speaker = None,
                _ => {
                    if let Some(speaker) = speaker {
                        *said.entry(speaker).or_default() +=
                            line.chars().filter(|c| !c.is_whitespace()).count();
                    }
                }
            }
        }
        let mut each: Vec<_> = said.iter().collect();
        each.sort();
        println!("said by speaker {each:?}");
        println!("ghosts {:?}", crate::voiceprints::merged(&said, 0));
        println!("alike {:?}", crate::voiceprints::likenesses());
        if let Ok(people) = std::env::var("OT_PEOPLE") {
            let most = people.parse().unwrap();
            println!(
                "with {most} people {:?}",
                crate::voiceprints::merged(&said, most)
            );
        }
        // The words go to a file when one is named, so that they are not shown.
        match std::env::var("OT_OUT") {
            Ok(out) => std::fs::write(out, text).unwrap(),
            Err(_) => println!("{text}"),
        }
    }

    #[test]
    fn a_long_recording_is_written_out_part_by_part() {
        let pcm = tone(30.0);
        let part = 10 * RATE as usize;
        let firsts = std::sync::Mutex::new(Vec::new());
        let mut tell = |sound: &[f32], first: bool| {
            let mut firsts = firsts.lock().unwrap();
            firsts.push(first);
            Ok(vec![crate::speakers::SpeakerTurn {
                start_ms: 0,
                end_ms: (sound.len() * 1000 / RATE as usize) as u64,
                speaker: firsts.len() as u32,
            }])
        };
        let mut stages = Vec::new();

        let text = in_parts(
            &pcm,
            part,
            "S",
            Some(&mut tell),
            |_| Ok("words".to_string()),
            |stage| stages.push(stage),
        )
        .unwrap();

        assert_eq!(
            text,
            "S 1  00:00\nwords\n\nS 2  00:10\nwords\n\nS 3  00:20\nwords"
        );
        assert_eq!(*firsts.lock().unwrap(), vec![true, false, false]);
        assert_eq!(stages[0], Stage::Speakers);
        // The text shows after the first part, and the count ends at the whole length.
        assert!(stages.contains(&Stage::Written("S 1  00:00\nwords".to_string())));
        assert_eq!(
            stages.last(),
            Some(&Stage::Text {
                done: 30,
                total: 30
            })
        );
    }

    #[test]
    fn a_part_whose_speakers_cannot_be_told_ends_the_writing() {
        let pcm = tone(30.0);
        let mut tell = |_: &[f32], first: bool| {
            if first {
                Ok(Vec::new())
            } else {
                Err("lost".to_string())
            }
        };
        let out = in_parts(
            &pcm,
            10 * RATE as usize,
            "S",
            Some(&mut tell),
            |_| Ok("words".to_string()),
            |_| {},
        );
        assert_eq!(out, Err("lost".to_string()));
    }

    #[test]
    fn the_parts_cover_the_recording_and_none_is_a_scrap() {
        assert_eq!(parts_of(24, 10), vec![0..10, 10..24]);
        assert_eq!(parts_of(30, 10), vec![0..10, 10..20, 20..30]);
        assert_eq!(parts_of(4, 10), vec![0..4]);
        assert!(parts_of(0, 10).is_empty());
    }

    #[test]
    fn what_came_before_is_not_written_down_again() {
        let before = "so we start with the basic services and then we add more of them later";
        let heard = "So we start with the basic services, and then we add more of them later. Any questions?";
        assert_eq!(without_echo(heard, Some(before)), "Any questions?");
        // Said back with a word dropped in the middle, both halves go.
        let before = "and then to the beginning of the. The the bank over there, we have been talking about it before";
        let heard = "And then to the beginning of the bank over there. We have been talking about it before.";
        assert_eq!(without_echo(heard, Some(before)), "");
    }

    #[test]
    fn a_few_words_said_again_stay() {
        let before = "we can do that for you next week";
        assert_eq!(
            without_echo("Yes, we can do that for you.", Some(before)),
            "Yes, we can do that for you."
        );
        assert_eq!(without_echo("Anything at all.", None), "Anything at all.");
    }

    #[test]
    fn what_came_before_is_told_in_writing_without_spaces() {
        let before = "我们先从基础的服务开始然后再慢慢增加其他的服务";
        let heard = "我们先从基础的服务开始，然后再慢慢增加其他的服务。还有问题吗？";
        assert_eq!(without_echo(heard, Some(before)), "还有问题吗？");
        assert_eq!(
            without_echo("我们先从基础开始。", Some(before)),
            "我们先从基础开始。"
        );
    }

    fn tone(seconds: f32) -> Vec<f32> {
        (0..(seconds * RATE as f32) as usize)
            .map(|i| (i as f32 * 0.05).sin() * 0.5)
            .collect()
    }

    #[test]
    fn a_short_recording_is_one_piece() {
        let pcm = tone(12.0);
        assert_eq!(pieces(&pcm), vec![0..pcm.len()]);
        assert!(pieces(&[]).is_empty());
    }

    #[test]
    fn a_long_recording_is_cut_into_pieces_that_cover_all_of_it() {
        let pcm = tone(100.0);
        let cut = pieces(&pcm);
        assert!(cut.len() >= 4);
        assert_eq!(cut[0].start, 0);
        assert_eq!(cut.last().unwrap().end, pcm.len());
        for pair in cut.windows(2) {
            assert_eq!(pair[0].end, pair[1].start);
        }
        for piece in &cut {
            assert!(piece.len() <= PIECE_SECS * RATE as usize);
            assert!(!piece.is_empty());
        }
    }

    #[test]
    fn a_cut_falls_in_a_pause_rather_than_mid_word() {
        let mut pcm = tone(50.0);
        let pause = 27 * RATE as usize..28 * RATE as usize;
        pcm[pause.clone()].fill(0.0);
        let cut = pieces(&pcm);
        assert!(pause.contains(&cut[0].end), "cut at {}", cut[0].end);
    }

    /// A 16-bit WAV, written by hand so the test needs no fixture file.
    fn wav(rate: u32, channels: u16, frames: usize) -> Vec<u8> {
        let data_len = (frames * channels as usize * 2) as u32;
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&(36 + data_len).to_le_bytes());
        bytes.extend_from_slice(b"WAVEfmt ");
        bytes.extend_from_slice(&16u32.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&channels.to_le_bytes());
        bytes.extend_from_slice(&rate.to_le_bytes());
        bytes.extend_from_slice(&(rate * channels as u32 * 2).to_le_bytes());
        bytes.extend_from_slice(&(channels * 2).to_le_bytes());
        bytes.extend_from_slice(&16u16.to_le_bytes());
        bytes.extend_from_slice(b"data");
        bytes.extend_from_slice(&data_len.to_le_bytes());
        for frame in 0..frames {
            let sample = ((frame as f32 * 0.03).sin() * 12_000.0) as i16;
            for _ in 0..channels {
                bytes.extend_from_slice(&sample.to_le_bytes());
            }
        }
        bytes
    }

    #[test]
    fn decodes_a_stereo_file_to_one_channel_at_the_model_rate() {
        let path = std::env::temp_dir().join(format!("ot-decode-{}.wav", std::process::id()));
        std::fs::write(&path, wav(44_100, 2, 44_100 * 2)).unwrap();
        let pcm = decode(&path).unwrap();
        let _ = std::fs::remove_file(&path);

        let two_seconds = 2 * RATE as usize;
        assert!(
            pcm.len().abs_diff(two_seconds) < 400,
            "{} samples",
            pcm.len()
        );
        assert!(pcm.iter().any(|sample| sample.abs() > 0.1));
        assert!(pcm.iter().all(|sample| sample.abs() <= 1.0));
    }

    #[test]
    fn says_so_for_a_file_that_holds_no_audio() {
        let path = std::env::temp_dir().join(format!("ot-decode-{}.txt", std::process::id()));
        std::fs::write(&path, b"not audio at all").unwrap();
        let result = decode(&path);
        let _ = std::fs::remove_file(&path);
        assert!(result.is_err());
        assert!(decode(Path::new("/nonexistent/file.mp3")).is_err());
    }

    /// Run with `OT_DECODE_FILE=<path> cargo test decodes_the_named_file -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn decodes_the_named_file() {
        let path = std::env::var("OT_DECODE_FILE").unwrap();
        let pcm = decode(Path::new(&path)).unwrap();
        println!(
            "seconds: {:.1} pieces: {}",
            pcm.len() as f32 / RATE as f32,
            pieces(&pcm).len()
        );
    }
}
