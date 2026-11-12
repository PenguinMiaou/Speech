//! Tells who spoke when in a finished recording, so a meeting can be written out by speaker.

use ot_stt_protocol::{SpeakerTurn, SpeakerVoice};
use sherpa_onnx::{
    FastClusteringConfig, OfflineSpeakerDiarization, OfflineSpeakerDiarizationConfig,
    OfflineSpeakerSegmentationModelConfig, OfflineSpeakerSegmentationPyannoteModelConfig,
    SpeakerEmbeddingExtractorConfig,
};

/// Speaker turns in the order they start. Speakers are numbered from 1 by first appearance.
pub fn diarize(
    segmentation_model: &str,
    embedding_model: &str,
    pcm: &[f32],
    threads: i32,
) -> Result<(Vec<SpeakerTurn>, Vec<SpeakerVoice>), String> {
    if follows_speakers(segmentation_model) {
        return by_following(segmentation_model, embedding_model, pcm, threads);
    }
    let config = OfflineSpeakerDiarizationConfig {
        segmentation: OfflineSpeakerSegmentationModelConfig {
            pyannote: OfflineSpeakerSegmentationPyannoteModelConfig {
                model: Some(segmentation_model.to_string()),
                ..Default::default()
            },
            num_threads: threads,
            ..Default::default()
        },
        embedding: SpeakerEmbeddingExtractorConfig {
            model: Some(embedding_model.to_string()),
            num_threads: threads,
            ..Default::default()
        },
        clustering: FastClusteringConfig::default(),
        ..Default::default()
    };
    let diarizer = OfflineSpeakerDiarization::create(&config)
        .ok_or_else(|| "the speaker models could not be loaded".to_string())?;
    let result = diarizer
        .process(pcm)
        .ok_or_else(|| "the recording could not be split by speaker".to_string())?;
    let turns = number_by_first_appearance(
        result
            .sort_by_start_time()
            .into_iter()
            .map(|segment| (segment.start, segment.end, segment.speaker)),
    );
    let Some(extractor) = sherpa_onnx::SpeakerEmbeddingExtractor::create(&config.embedding) else {
        return Ok((turns, Vec::new()));
    };
    // The clustering often gives one person several numbers. Speakers whose voices are
    // alike are made one, and the voices are taken again from the joined speech.
    let voices = voices_of(&extractor, pcm, &turns);
    let spoke = |speaker: u32| -> u64 {
        turns
            .iter()
            .filter(|turn| turn.speaker == speaker)
            .map(|turn| turn.end_ms - turn.start_ms)
            .sum()
    };
    let mut by_length: Vec<&SpeakerVoice> = voices.iter().collect();
    by_length.sort_by_key(|voice| std::cmp::Reverse(spoke(voice.speaker)));
    let pairs = same_people(&by_length);
    let (turns, voices) = if pairs.is_empty() {
        (turns, voices)
    } else {
        let turns = joined(&turns, &pairs);
        let voices = voices_of(&extractor, pcm, &turns);
        (turns, voices)
    };
    // A long meeting leaves dozens of numbers that each said a few seconds: coughs,
    // laughter, two people at once. They are not people of their own.
    let whole = without_fragments(&turns, &voices);
    if whole == turns {
        return Ok((turns, voices));
    }
    let voices = voices_of(&extractor, pcm, &whole);
    Ok((whole, voices))
}

/// Whether the model follows each speaker through the whole recording by itself, rather
/// than cutting the sound up for voices to be compared afterwards.
pub fn follows_speakers(model: &str) -> bool {
    model.ends_with(".gguf")
}

/// The model that follows each speaker through a recording, with the voice model that
/// takes each speaker's voice. Kept loaded between recordings.
pub struct Follower {
    model: transcribe_cpp::Model,
    extractor: Option<sherpa_onnx::SpeakerEmbeddingExtractor>,
}

impl Follower {
    pub fn open(model: &str, embedding_model: &str, threads: i32) -> Result<Self, String> {
        let model = transcribe_cpp::Model::load(model).map_err(|error| error.to_string())?;
        let embedding = SpeakerEmbeddingExtractorConfig {
            model: Some(embedding_model.to_string()),
            num_threads: threads,
            ..Default::default()
        };
        Ok(Self {
            model,
            extractor: sherpa_onnx::SpeakerEmbeddingExtractor::create(&embedding),
        })
    }

    /// Who spoke when, and each speaker's voice. `as_heard` leaves the speakers as the
    /// model heard them; otherwise a speaker who said next to nothing is joined to another.
    pub fn follow(
        &self,
        pcm: &[f32],
        as_heard: bool,
    ) -> Result<(Vec<SpeakerTurn>, Vec<SpeakerVoice>), String> {
        let failed = |error: transcribe_cpp::Error| error.to_string();
        let mut session = self.model.diarize_session().map_err(failed)?;
        let mut segments = session.run(pcm, &Default::default()).map_err(failed)?;
        segments.sort_by_key(|segment| segment.t0_ms);
        let turns = number_by_first_appearance(segments.iter().map(|segment| {
            (
                segment.t0_ms as f32 / 1000.0,
                segment.t1_ms as f32 / 1000.0,
                segment.speaker_id,
            )
        }));
        let Some(extractor) = &self.extractor else {
            return Ok((turns, Vec::new()));
        };
        if as_heard {
            let voices = voices_from(extractor, pcm, &turns, QUICK_VOICE_SAMPLES);
            return Ok((turns, voices));
        }
        let voices = voices_of(extractor, pcm, &turns);
        let whole = without_fragments(&turns, &voices);
        if whole == turns {
            return Ok((turns, voices));
        }
        let voices = voices_of(extractor, pcm, &whole);
        Ok((whole, voices))
    }
}

/// Who spoke when, told by a model that keeps every speaker's number from the first
/// minute to the last. The voice model only takes each speaker's voice, for their name.
fn by_following(
    model: &str,
    embedding_model: &str,
    pcm: &[f32],
    threads: i32,
) -> Result<(Vec<SpeakerTurn>, Vec<SpeakerVoice>), String> {
    Follower::open(model, embedding_model, threads)?.follow(pcm, false)
}

/// A speaker who said less than this in all, and less than a twentieth of everything
/// said, is a fragment and not a person of their own.
const FRAGMENT_MS: u64 = 20_000;
const FRAGMENT_SHARE: u64 = 20;
/// A fragment whose voice is at least this alike to a person's is that person's;
/// otherwise it goes to whoever was speaking around it.
const LIKELY_ONE_PERSON: f32 = 0.5;

/// The turns with every fragment given to a person: the one it sounds most like, or else
/// the one speaking just before it. Numbered again in order of appearance.
fn without_fragments(turns: &[SpeakerTurn], voices: &[SpeakerVoice]) -> Vec<SpeakerTurn> {
    let mut spoke: std::collections::HashMap<u32, u64> = std::collections::HashMap::new();
    for turn in turns {
        *spoke.entry(turn.speaker).or_default() += turn.end_ms.saturating_sub(turn.start_ms);
    }
    let all: u64 = spoke.values().sum();
    let fragment =
        |speaker: u32| spoke[&speaker] < FRAGMENT_MS && spoke[&speaker] * FRAGMENT_SHARE < all;
    if !turns.iter().any(|turn| fragment(turn.speaker))
        || turns.iter().all(|turn| fragment(turn.speaker))
    {
        return turns.to_vec();
    }
    let voice = |speaker: u32| voices.iter().find(|voice| voice.speaker == speaker);
    let sounds_like = |speaker: u32| -> Option<u32> {
        let own = voice(speaker)?;
        voices
            .iter()
            .filter(|person| !fragment(person.speaker))
            .map(|person| (alike(&person.embedding, &own.embedding), person.speaker))
            .max_by(|a, b| a.0.total_cmp(&b.0))
            .filter(|(score, _)| *score >= LIKELY_ONE_PERSON)
            .map(|(_, person)| person)
    };
    let first_person = turns
        .iter()
        .map(|turn| turn.speaker)
        .find(|speaker| !fragment(*speaker));
    let mut speaking = first_person.unwrap_or(0);
    number_by_first_appearance(turns.iter().map(|turn| {
        let speaker = if fragment(turn.speaker) {
            sounds_like(turn.speaker).unwrap_or(speaking)
        } else {
            speaking = turn.speaker;
            turn.speaker
        };
        (
            turn.start_ms as f32 / 1000.0,
            turn.end_ms as f32 / 1000.0,
            speaker as i32,
        )
    }))
}

/// Two speakers whose voices are at least this alike are one person. Measured on real
/// meetings: one person's clusters score 0.7 to 0.94, different people 0.37 to 0.6.
const ONE_PERSON: f32 = 0.68;

fn alike(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    let size = |v: &[f32]| v.iter().map(|x| x * x).sum::<f32>().sqrt();
    let sizes = size(a) * size(b);
    if sizes > 0.0 {
        dot / sizes
    } else {
        0.0
    }
}

/// Which speakers are one person, as (the one who spoke more, the other). `voices` has
/// the one who spoke most first. A speaker is only ever compared with speakers kept as
/// people of their own, so a chain of near matches cannot join two different people.
fn same_people(voices: &[&SpeakerVoice]) -> Vec<(u32, u32)> {
    let mut people: Vec<&SpeakerVoice> = Vec::new();
    let mut pairs = Vec::new();
    for voice in voices {
        let closest = people
            .iter()
            .map(|person| (alike(&person.embedding, &voice.embedding), person.speaker))
            .max_by(|a, b| a.0.total_cmp(&b.0));
        match closest {
            Some((score, person)) if score >= ONE_PERSON => pairs.push((person, voice.speaker)),
            _ => people.push(voice),
        }
    }
    pairs
}

/// The turns with every pair of speakers made one, numbered again in order of appearance.
fn joined(turns: &[SpeakerTurn], pairs: &[(u32, u32)]) -> Vec<SpeakerTurn> {
    let mut stands_for: std::collections::HashMap<u32, u32> = std::collections::HashMap::new();
    let root = |map: &std::collections::HashMap<u32, u32>, mut speaker: u32| {
        while let Some(next) = map.get(&speaker) {
            speaker = *next;
        }
        speaker
    };
    for (a, b) in pairs {
        let (a, b) = (root(&stands_for, *a), root(&stands_for, *b));
        if a != b {
            stands_for.insert(a.max(b), a.min(b));
        }
    }
    number_by_first_appearance(turns.iter().map(|turn| {
        (
            turn.start_ms as f32 / 1000.0,
            turn.end_ms as f32 / 1000.0,
            root(&stands_for, turn.speaker) as i32,
        )
    }))
}

/// The most of one speaker's speech that goes into their voice.
const VOICE_SAMPLES: usize = 60 * 16_000;

fn embed(extractor: &sherpa_onnx::SpeakerEmbeddingExtractor, pcm: &[f32]) -> Option<Vec<f32>> {
    let stream = extractor.create_stream()?;
    stream.accept_waveform(16_000, pcm);
    stream.input_finished();
    extractor
        .is_ready(&stream)
        .then(|| extractor.compute(&stream))?
}

/// One voice per speaker, from up to a minute of what they said.
fn voices_of(
    extractor: &sherpa_onnx::SpeakerEmbeddingExtractor,
    pcm: &[f32],
    turns: &[SpeakerTurn],
) -> Vec<SpeakerVoice> {
    voices_from(extractor, pcm, turns, VOICE_SAMPLES)
}

/// A dictation waits for its voices, and taking a voice costs as much as the sound it is
/// taken from is long. Six seconds tell the user from someone else.
const QUICK_VOICE_SAMPLES: usize = 6 * 16_000;

/// One voice per speaker, from at most `most` samples of what they said.
fn voices_from(
    extractor: &sherpa_onnx::SpeakerEmbeddingExtractor,
    pcm: &[f32],
    turns: &[SpeakerTurn],
    most: usize,
) -> Vec<SpeakerVoice> {
    let mut speakers: Vec<u32> = turns.iter().map(|turn| turn.speaker).collect();
    speakers.sort_unstable();
    speakers.dedup();
    speakers
        .into_iter()
        .filter(|speaker| *speaker != 0)
        .filter_map(|speaker| {
            let mut said = Vec::new();
            for turn in turns.iter().filter(|turn| turn.speaker == speaker) {
                let at = |ms: u64| ((ms * 16) as usize).min(pcm.len());
                let room = most.saturating_sub(said.len());
                let (from, to) = (at(turn.start_ms), at(turn.end_ms));
                said.extend_from_slice(&pcm[from..to.min(from + room)]);
            }
            (said.len() >= SHORTEST_VOICE_SAMPLES)
                .then(|| embed(extractor, &said))
                .flatten()
                .map(|embedding| SpeakerVoice { speaker, embedding })
        })
        .collect()
}

/// Keeps speakers apart across the windows of a recording that is still running: each
/// window is split on its own, then every voice in it is matched against the voices
/// heard so far, so "speaker 2" stays the same person from one window to the next.
pub struct LiveSpeakers {
    diarizer: OfflineSpeakerDiarization,
    extractor: sherpa_onnx::SpeakerEmbeddingExtractor,
    known: sherpa_onnx::SpeakerEmbeddingManager,
}

/// How alike two voices must be to count as one person.
const SAME_VOICE: f32 = 0.45;
/// A voice heard for less than this is too short to recognize or remember.
const SHORTEST_VOICE_SAMPLES: usize = 16_000;
/// A voice that matches nobody is taken for a new person only after this much speech in
/// one window. Shorter stretches are mostly known people the model failed to recognize.
const NEW_VOICE_SAMPLES: usize = 4 * 16_000;

impl LiveSpeakers {
    pub fn open(
        segmentation_model: &str,
        embedding_model: &str,
        threads: i32,
    ) -> Result<Self, String> {
        let embedding = SpeakerEmbeddingExtractorConfig {
            model: Some(embedding_model.to_string()),
            num_threads: threads,
            ..Default::default()
        };
        let config = OfflineSpeakerDiarizationConfig {
            segmentation: OfflineSpeakerSegmentationModelConfig {
                pyannote: OfflineSpeakerSegmentationPyannoteModelConfig {
                    model: Some(segmentation_model.to_string()),
                    ..Default::default()
                },
                num_threads: threads,
                ..Default::default()
            },
            embedding: embedding.clone(),
            clustering: FastClusteringConfig::default(),
            ..Default::default()
        };
        let missing = || "the speaker models could not be loaded".to_string();
        let diarizer = OfflineSpeakerDiarization::create(&config).ok_or_else(missing)?;
        let extractor =
            sherpa_onnx::SpeakerEmbeddingExtractor::create(&embedding).ok_or_else(missing)?;
        let known =
            sherpa_onnx::SpeakerEmbeddingManager::create(extractor.dim()).ok_or_else(missing)?;
        Ok(Self {
            diarizer,
            extractor,
            known,
        })
    }

    fn voice(&self, pcm: &[f32]) -> Option<Vec<f32>> {
        embed(&self.extractor, pcm)
    }

    /// The turns of one window, with times counted from the window's start and speakers
    /// numbered across the whole recording. A voice too short to tell is speaker 0.
    pub fn window(&mut self, pcm: &[f32]) -> Result<(Vec<SpeakerTurn>, Vec<SpeakerVoice>), String> {
        let mut voices = Vec::new();
        let result = self
            .diarizer
            .process(pcm)
            .ok_or_else(|| "the recording could not be split by speaker".to_string())?;
        let segments = result.sort_by_start_time();
        let mut names: std::collections::HashMap<i32, u32> = std::collections::HashMap::new();
        let mut locals: Vec<i32> = segments.iter().map(|segment| segment.speaker).collect();
        locals.dedup();
        locals.sort_unstable();
        locals.dedup();
        for local in locals {
            let mut voice = Vec::new();
            for segment in segments.iter().filter(|segment| segment.speaker == local) {
                let at = |seconds: f32| ((seconds.max(0.0) * 16_000.0) as usize).min(pcm.len());
                voice.extend_from_slice(&pcm[at(segment.start)..at(segment.end)]);
            }
            let number = if voice.len() < SHORTEST_VOICE_SAMPLES {
                0
            } else {
                match self.voice(&voice) {
                    Some(embedding) => {
                        let number = match self.known.search(&embedding, SAME_VOICE) {
                            Some(name) => name.parse().unwrap_or(0),
                            None if voice.len() < NEW_VOICE_SAMPLES => 0,
                            None => {
                                let next = self.known.num_speakers() as u32 + 1;
                                self.known.add(&next.to_string(), &embedding);
                                next
                            }
                        };
                        if number != 0 {
                            voices.push(SpeakerVoice {
                                speaker: number,
                                embedding,
                            });
                        }
                        number
                    }
                    None => 0,
                }
            };
            names.insert(local, number);
        }
        let turns = segments
            .into_iter()
            .map(|segment| SpeakerTurn {
                start_ms: (segment.start.max(0.0) * 1000.0) as u64,
                end_ms: (segment.end.max(0.0) * 1000.0) as u64,
                speaker: names.get(&segment.speaker).copied().unwrap_or(0),
            })
            .collect();
        Ok((turns, voices))
    }
}

/// The speakers of a recording still running, told by the model that follows each of
/// them through all of it: every window is handed on as it comes, and a speaker keeps
/// one number from the first window to the last without voices being compared.
pub struct LiveFollowing {
    session: transcribe_cpp::DiarizeSession,
    extractor: Option<sherpa_onnx::SpeakerEmbeddingExtractor>,
    /// Samples handed to the model so far.
    fed: usize,
    /// The model's own speaker numbers, in the order they were first heard.
    seen: Vec<i32>,
}

impl LiveFollowing {
    pub fn open(model: &str, embedding_model: &str, threads: i32) -> Result<Self, String> {
        let failed = |error: transcribe_cpp::Error| error.to_string();
        let model = transcribe_cpp::Model::load(model).map_err(failed)?;
        let mut session = model.diarize_session().map_err(failed)?;
        session.stream_begin(&Default::default()).map_err(failed)?;
        let embedding = SpeakerEmbeddingExtractorConfig {
            model: Some(embedding_model.to_string()),
            num_threads: threads,
            ..Default::default()
        };
        Ok(Self {
            session,
            extractor: sherpa_onnx::SpeakerEmbeddingExtractor::create(&embedding),
            fed: 0,
            seen: Vec::new(),
        })
    }

    /// The turns of one window, with times counted from the window's start, and the
    /// voice of each speaker heard in it.
    pub fn window(&mut self, pcm: &[f32]) -> Result<(Vec<SpeakerTurn>, Vec<SpeakerVoice>), String> {
        let from_ms = (self.fed / 16) as u64;
        let failed = |error: transcribe_cpp::Error| error.to_string();
        self.session.stream_feed(pcm).map_err(failed)?;
        // The model decides in long steps, up to half a minute behind the sound. What it
        // would say of the rest were the recording to end here is asked for as well, so
        // a window has its speakers by the time its words are written.
        let mut segments = self.session.stream_peek().map_err(failed)?;
        self.fed += pcm.len();
        segments.sort_by_key(|segment| segment.t0_ms);
        let heard: Vec<(i64, i64, i32)> = segments
            .iter()
            .map(|segment| (segment.t0_ms, segment.t1_ms, segment.speaker_id))
            .collect();
        let to_ms = (self.fed / 16) as u64;
        let turns = window_turns(&heard, from_ms, to_ms, to_ms, &mut self.seen);
        let voices = match &self.extractor {
            Some(extractor) => voices_of(extractor, pcm, &turns),
            None => Vec::new(),
        };
        Ok((turns, voices))
    }
}

/// A turn that ends this close to where the model has got to is still being spoken.
const STILL_SPEAKING_MS: u64 = 200;

/// The turns of the window from `from_ms` to `to_ms`, out of everything the model has
/// decided of the recording so far (start, end and the model's speaker number, in the
/// order they start). Times are counted from the window's start. `seen` keeps the
/// speakers' numbers from one window to the next.
fn window_turns(
    heard: &[(i64, i64, i32)],
    from_ms: u64,
    to_ms: u64,
    decided_ms: u64,
    seen: &mut Vec<i32>,
) -> Vec<SpeakerTurn> {
    let mut turns = Vec::new();
    let mut last: Option<(u64, u32)> = None;
    for (start, end, speaker) in heard {
        let (start, end) = ((*start).max(0) as u64, (*end).max(0) as u64);
        let position = seen.iter().position(|known| known == speaker);
        let number = position.unwrap_or_else(|| {
            seen.push(*speaker);
            seen.len() - 1
        }) as u32
            + 1;
        if last.is_none_or(|(latest, _)| end >= latest) {
            last = Some((end, number));
        }
        let (start, end) = (start.max(from_ms), end.min(to_ms));
        if start < end {
            turns.push(SpeakerTurn {
                start_ms: start - from_ms,
                end_ms: end - from_ms,
                speaker: number,
            });
        }
    }
    // The model has not got to the end of the window yet. Whoever is speaking where it
    // has got to is taken to speak on to the end.
    let undecided_from = decided_ms.max(from_ms);
    if let Some((end, speaker)) = last.filter(|_| undecided_from < to_ms) {
        if end + STILL_SPEAKING_MS >= decided_ms {
            let rest_from = end.clamp(from_ms, undecided_from) - from_ms;
            match turns
                .iter_mut()
                .rev()
                .find(|turn| turn.speaker == speaker && turn.end_ms == rest_from)
            {
                Some(turn) => turn.end_ms = to_ms - from_ms,
                None => turns.push(SpeakerTurn {
                    start_ms: undecided_from - from_ms,
                    end_ms: to_ms - from_ms,
                    speaker,
                }),
            }
        }
    }
    turns
}

/// The speakers of a recording still running, by whichever model is installed.
pub enum Live {
    ByWindow(Box<LiveSpeakers>),
    Following(Box<LiveFollowing>),
}

impl Live {
    pub fn open(
        segmentation_model: &str,
        embedding_model: &str,
        threads: i32,
    ) -> Result<Self, String> {
        if follows_speakers(segmentation_model) {
            let live = LiveFollowing::open(segmentation_model, embedding_model, threads)?;
            return Ok(Self::Following(Box::new(live)));
        }
        let live = LiveSpeakers::open(segmentation_model, embedding_model, threads)?;
        Ok(Self::ByWindow(Box::new(live)))
    }

    pub fn window(&mut self, pcm: &[f32]) -> Result<(Vec<SpeakerTurn>, Vec<SpeakerVoice>), String> {
        match self {
            Self::ByWindow(live) => live.window(pcm),
            Self::Following(live) => live.window(pcm),
        }
    }
}

/// The library numbers speakers arbitrarily. A reader expects the first voice to be 1.
fn number_by_first_appearance(segments: impl Iterator<Item = (f32, f32, i32)>) -> Vec<SpeakerTurn> {
    let mut seen: Vec<i32> = Vec::new();
    segments
        .map(|(start, end, speaker)| {
            let position = seen.iter().position(|known| *known == speaker);
            let number = position.unwrap_or_else(|| {
                seen.push(speaker);
                seen.len() - 1
            }) + 1;
            SpeakerTurn {
                start_ms: (start.max(0.0) * 1000.0) as u64,
                end_ms: (end.max(0.0) * 1000.0) as u64,
                speaker: number as u32,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn said(start_s: u64, end_s: u64, speaker: u32) -> SpeakerTurn {
        SpeakerTurn {
            start_ms: start_s * 1000,
            end_ms: end_s * 1000,
            speaker,
        }
    }

    fn sounding(speaker: u32, embedding: [f32; 2]) -> SpeakerVoice {
        SpeakerVoice {
            speaker,
            embedding: embedding.to_vec(),
        }
    }

    #[test]
    fn a_few_seconds_of_someone_are_not_a_person_of_their_own() {
        let turns = [
            said(0, 300, 1),
            said(300, 303, 2),
            said(303, 600, 3),
            said(600, 602, 4),
            said(602, 604, 5),
            said(604, 900, 1),
        ];
        let voices = [
            sounding(1, [1.0, 0.0]),
            sounding(3, [0.0, 1.0]),
            // Sounds like the first person though it comes while the other one speaks.
            sounding(4, [0.9, 0.1]),
        ];
        let speakers: Vec<u32> = without_fragments(&turns, &voices)
            .iter()
            .map(|turn| turn.speaker)
            .collect();
        // 2 has no voice and goes to who spoke before it; 4 sounds like 1; 5 follows 3.
        assert_eq!(speakers, vec![1, 1, 2, 1, 2, 1]);
    }

    #[test]
    fn people_in_a_short_talk_are_kept_though_each_said_little() {
        let turns = [said(0, 10, 1), said(10, 18, 2), said(18, 30, 1)];
        assert_eq!(without_fragments(&turns, &[]), turns.to_vec());
    }

    #[test]
    fn a_model_that_follows_speakers_is_known_by_its_file() {
        assert!(follows_speakers("models/speakers/speaker-turns.gguf"));
        assert!(!follows_speakers(
            "models/speakers/speaker-segmentation.onnx"
        ));
    }

    #[test]
    fn the_first_voice_heard_is_speaker_one() {
        let turns = number_by_first_appearance(
            [(0.0, 1.5, 7), (1.5, 3.0, 2), (3.0, 4.0, 7), (4.0, 5.0, 0)].into_iter(),
        );
        assert_eq!(
            turns.iter().map(|turn| turn.speaker).collect::<Vec<_>>(),
            vec![1, 2, 1, 3]
        );
        assert_eq!((turns[1].start_ms, turns[1].end_ms), (1500, 3000));
    }

    /// Prints how alike the voices of two recordings are, `OT_WAV` and `OT_WAV2`, with
    /// the seconds each speaker has: the numbers a name-by-voice threshold is set from.
    #[test]
    fn speakers_who_sound_alike_become_one() {
        let turn = |start_ms, speaker| SpeakerTurn {
            start_ms,
            end_ms: start_ms + 1000,
            speaker,
        };
        let turns = [
            turn(0, 1),
            turn(1000, 2),
            turn(2000, 3),
            turn(3000, 4),
            turn(4000, 2),
        ];
        let numbers: Vec<u32> = joined(&turns, &[(1, 3), (3, 4)])
            .iter()
            .map(|turn| turn.speaker)
            .collect();
        assert_eq!(numbers, [1, 2, 1, 1, 2]);
        let voice = |speaker, embedding: [f32; 2]| SpeakerVoice {
            speaker,
            embedding: embedding.to_vec(),
        };
        // 2 is close to 1, and 3 is close to 2 but not to 1: 3 stays a person of its own.
        let voices = [
            voice(1, [1.0, 0.0]),
            voice(2, [0.8, 0.6]),
            voice(3, [0.3, 0.95]),
        ];
        let order: Vec<&SpeakerVoice> = voices.iter().collect();
        assert_eq!(same_people(&order), [(1, 2)]);
        assert!((alike(&[1.0, 0.0], &[3.0, 0.0]) - 1.0).abs() < 1e-6);
        assert_eq!(alike(&[0.0, 0.0], &[1.0, 0.0]), 0.0);
    }

    #[test]
    #[ignore]
    fn compares_the_voices_of_two_recordings() {
        let voices = |name: &str| {
            let wav = std::fs::read(std::env::var(name).unwrap()).unwrap();
            let pcm = crate::eval::decode_wav(&wav).unwrap();
            let (turns, voices) = diarize(
                &std::env::var("OT_SEG").unwrap(),
                &std::env::var("OT_EMB").unwrap(),
                &pcm,
                4,
            )
            .unwrap();
            let seconds = |speaker: u32| {
                turns
                    .iter()
                    .filter(|turn| turn.speaker == speaker)
                    .map(|turn| turn.end_ms - turn.start_ms)
                    .sum::<u64>()
                    / 1000
            };
            voices
                .into_iter()
                .map(|voice| (seconds(voice.speaker), voice.embedding))
                .filter(|(seconds, _)| *seconds >= 20)
                .collect::<Vec<_>>()
        };
        let alike = |a: &[f32], b: &[f32]| {
            let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
            let size = |v: &[f32]| v.iter().map(|x| x * x).sum::<f32>().sqrt();
            dot / (size(a) * size(b))
        };
        let (first, second) = (voices("OT_WAV"), voices("OT_WAV2"));
        for (seconds, voice) in &first {
            let row: Vec<String> = second
                .iter()
                .map(|(other, theirs)| format!("{:.2} ({other}s)", alike(voice, theirs)))
                .collect();
            println!("voice with {seconds}s: {}", row.join("  "));
        }
    }

    #[test]
    fn a_window_of_a_running_recording_keeps_each_speakers_number() {
        let mut seen = Vec::new();
        let first = window_turns(
            &[(0, 4000, 7), (4000, 9000, 3)],
            0,
            10_000,
            10_000,
            &mut seen,
        );
        assert_eq!(first, vec![said(0, 4, 1), said(4, 9, 2)]);
        let heard = [(0, 4000, 7), (4000, 9000, 3), (10_000, 15_000, 7)];
        let second = window_turns(&heard, 10_000, 20_000, 20_000, &mut seen);
        assert_eq!(second, vec![said(0, 5, 1)]);
    }

    #[test]
    fn a_turn_begun_before_the_window_is_cut_at_its_start() {
        let mut seen = Vec::new();
        let turns = window_turns(&[(8000, 12_000, 3)], 10_000, 20_000, 20_000, &mut seen);
        assert_eq!(turns, vec![said(0, 2, 1)]);
    }

    #[test]
    fn whoever_speaks_where_the_model_has_got_to_has_the_rest_of_the_window() {
        let mut seen = Vec::new();
        let turns = window_turns(&[(10_000, 19_200, 3)], 10_000, 20_000, 19_200, &mut seen);
        assert_eq!(turns, vec![said(0, 10, 1)]);
        let stopped = window_turns(&[(10_000, 15_000, 3)], 10_000, 20_000, 19_200, &mut seen);
        assert_eq!(stopped, vec![said(0, 5, 1)]);
    }

    #[test]
    fn a_window_the_model_has_not_reached_goes_to_whoever_was_speaking() {
        let mut seen = Vec::new();
        let turns = window_turns(&[(0, 9500, 5)], 10_000, 10_800, 9500, &mut seen);
        assert_eq!(
            turns,
            vec![SpeakerTurn {
                start_ms: 0,
                end_ms: 800,
                speaker: 1
            }]
        );
        assert!(window_turns(&[(0, 5000, 5)], 10_000, 10_800, 9500, &mut seen).is_empty());
    }

    /// Needs the following model, the voice model and a recording. Run with
    /// `OT_SEG=<gguf> OT_EMB=<onnx> OT_WAV=<wav> OT_WINDOW=<seconds> cargo test follows_a_running -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn follows_a_running_recording_as_it_follows_the_whole() {
        let wav = std::fs::read(std::env::var("OT_WAV").unwrap()).unwrap();
        let pcm = crate::eval::decode_wav(&wav).unwrap();
        let window: usize = std::env::var("OT_WINDOW").unwrap().parse().unwrap();
        let (seg, emb) = (
            std::env::var("OT_SEG").unwrap(),
            std::env::var("OT_EMB").unwrap(),
        );
        let mut live = Live::open(&seg, &emb, 4).unwrap();
        let started = std::time::Instant::now();
        let (mut slowest, mut turns, mut speakers) = (0.0f32, 0, std::collections::BTreeMap::new());
        for piece in pcm.chunks(window * 16_000) {
            let began = std::time::Instant::now();
            let (found, _) = live.window(piece).unwrap();
            slowest = slowest.max(began.elapsed().as_secs_f32());
            turns += found.len();
            for turn in found {
                *speakers.entry(turn.speaker).or_insert(0u64) += turn.end_ms - turn.start_ms;
            }
        }
        println!(
            "live: {turns} turns, seconds by speaker {:?}, took {:.1}s, slowest window {slowest:.2}s",
            speakers.iter().map(|(who, ms)| (*who, ms / 1000)).collect::<Vec<_>>(),
            started.elapsed().as_secs_f32()
        );
        let (whole, _) = diarize(&seg, &emb, &pcm, 4).unwrap();
        let mut all = std::collections::BTreeMap::new();
        for turn in &whole {
            *all.entry(turn.speaker).or_insert(0u64) += turn.end_ms - turn.start_ms;
        }
        println!(
            "whole: {} turns, seconds by speaker {:?}",
            whole.len(),
            all.iter()
                .map(|(who, ms)| (*who, ms / 1000))
                .collect::<Vec<_>>()
        );
    }

    /// How alike each speaker of a recording is to an enrolled voice, over all they said
    /// and over single turns of three seconds or more. Run with `OT_SEG=<gguf>
    /// OT_EMB=<onnx> OT_WAV=<wav> OT_SAMPLE=<wav> cargo test enrolled_voice -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn tells_the_enrolled_voice_from_the_others() {
        let read = |name: &str| {
            crate::eval::decode_wav(&std::fs::read(std::env::var(name).unwrap()).unwrap()).unwrap()
        };
        let (pcm, sample) = (read("OT_WAV"), read("OT_SAMPLE"));
        let (seg, emb) = (
            std::env::var("OT_SEG").unwrap(),
            std::env::var("OT_EMB").unwrap(),
        );
        let (turns, voices) = diarize(&seg, &emb, &pcm, 4).unwrap();
        let config = SpeakerEmbeddingExtractorConfig {
            model: Some(emb),
            num_threads: 4,
            ..Default::default()
        };
        let extractor = sherpa_onnx::SpeakerEmbeddingExtractor::create(&config).unwrap();
        let mine = embed(&extractor, &sample).unwrap();
        println!("sample {:.1}s", sample.len() as f32 / 16_000.0);
        for voice in &voices {
            let mut single: Vec<f32> = turns
                .iter()
                .filter(|turn| turn.speaker == voice.speaker && turn.end_ms - turn.start_ms >= 3000)
                .take(40)
                .filter_map(|turn| {
                    let at = |ms: u64| ((ms * 16) as usize).min(pcm.len());
                    embed(&extractor, &pcm[at(turn.start_ms)..at(turn.end_ms)])
                })
                .map(|theirs| alike(&mine, &theirs))
                .collect();
            single.sort_by(|a, b| a.total_cmp(b));
            let middle = single.get(single.len() / 2).copied().unwrap_or(0.0);
            println!(
                "speaker {}: all {:.2}; {} turns: lowest {:.2} middle {:.2} highest {:.2}",
                voice.speaker,
                alike(&mine, &voice.embedding),
                single.len(),
                single.first().copied().unwrap_or(0.0),
                middle,
                single.last().copied().unwrap_or(0.0)
            );
        }
    }

    /// How alike each recording in a folder is to an enrolled voice. Run with
    /// `OT_EMB=<onnx> OT_DIR=<folder of wavs> OT_SAMPLE=<wav> cargo test own_recordings -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn knows_the_enrolled_voice_in_its_own_recordings() {
        let read =
            |path: &std::path::Path| crate::eval::decode_wav(&std::fs::read(path).ok()?).ok();
        let config = SpeakerEmbeddingExtractorConfig {
            model: Some(std::env::var("OT_EMB").unwrap()),
            num_threads: 4,
            ..Default::default()
        };
        let extractor = sherpa_onnx::SpeakerEmbeddingExtractor::create(&config).unwrap();
        let sample = read(std::path::Path::new(&std::env::var("OT_SAMPLE").unwrap())).unwrap();
        let mine = embed(&extractor, &sample).unwrap();
        let mut scores: Vec<(f32, f32)> = std::fs::read_dir(std::env::var("OT_DIR").unwrap())
            .unwrap()
            .filter_map(|entry| read(&entry.ok()?.path()))
            .filter_map(|pcm| {
                Some((
                    alike(&mine, &embed(&extractor, &pcm)?),
                    pcm.len() as f32 / 16_000.0,
                ))
            })
            .collect();
        scores.sort_by(|a, b| a.0.total_cmp(&b.0));
        let at = |share: f32| scores[((scores.len() - 1) as f32 * share) as usize];
        println!(
            "{} recordings: lowest {:.2} ({:.0}s) 5% {:.2} 25% {:.2} middle {:.2} highest {:.2}; below 0.30: {}, below 0.45: {}",
            scores.len(), at(0.0).0, at(0.0).1, at(0.05).0, at(0.25).0, at(0.5).0, at(1.0).0,
            scores.iter().filter(|score| score.0 < 0.30).count(),
            scores.iter().filter(|score| score.0 < 0.45).count()
        );
        let short: Vec<f32> = scores
            .iter()
            .filter(|score| score.1 < 4.0)
            .map(|score| score.0)
            .collect();
        println!(
            "under 4s: {} recordings, lowest {:.2}",
            short.len(),
            short.first().copied().unwrap_or(0.0)
        );
    }

    /// Dictations with someone else speaking before or after: how much of the user's
    /// speech and of the other's is kept when only speakers alike to the sample count.
    /// `OT_SEG=<gguf> OT_EMB=<onnx> OT_SAMPLE=<wav> OT_DIR=<the user's wavs> OT_WAV=<others'
    /// wav> cargo test only_the_enrolled -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn keeps_only_the_enrolled_voice_of_a_dictation() {
        let read =
            |path: &std::path::Path| crate::eval::decode_wav(&std::fs::read(path).ok()?).ok();
        let env = |name: &str| std::env::var(name).unwrap();
        let follower = Follower::open(&env("OT_SEG"), &env("OT_EMB"), 4).unwrap();
        let sample = read(std::path::Path::new(&env("OT_SAMPLE"))).unwrap();
        let mine = follower
            .follow(&sample, true)
            .unwrap()
            .1
            .remove(0)
            .embedding;
        let meeting = read(std::path::Path::new(&env("OT_WAV"))).unwrap();
        let others: Vec<Vec<f32>> = follower
            .follow(&meeting, true)
            .unwrap()
            .0
            .iter()
            .filter(|turn| turn.end_ms - turn.start_ms >= 5000)
            .map(|turn| meeting[(turn.start_ms * 16) as usize..][..5 * 16_000].to_vec())
            .collect();
        let own: Vec<Vec<f32>> = std::fs::read_dir(env("OT_DIR"))
            .unwrap()
            .filter_map(|entry| read(&entry.ok()?.path()))
            .filter(|pcm| pcm.len() >= 2 * 16_000)
            .take(40)
            .collect();
        // Milliseconds kept of the stretch from..to of a recording.
        let kept = |pcm: &[f32], from: usize, to: usize| -> (u64, u128) {
            let began = std::time::Instant::now();
            let (turns, voices) = follower.follow(pcm, true).unwrap();
            let took = began.elapsed().as_millis();
            let is_mine = |speaker: u32| {
                voices
                    .iter()
                    .any(|voice| voice.speaker == speaker && alike(&mine, &voice.embedding) >= 0.30)
            };
            let (from, to) = ((from / 16) as u64, (to / 16) as u64);
            let ms = turns
                .iter()
                .filter(|turn| is_mine(turn.speaker))
                .map(|turn| turn.end_ms.min(to).saturating_sub(turn.start_ms.max(from)))
                .sum();
            (ms, took)
        };
        let gap = vec![0.0f32; 4800];
        let (mut of_mine, mut of_theirs, mut said_mine, mut said_theirs) = (0u64, 0u64, 0u64, 0u64);
        let (mut leaked, mut lost, mut times) = (0, 0, Vec::new());
        for (index, clip) in own.iter().enumerate() {
            let other = &others[index % others.len()];
            let (first, second) = if index % 2 == 0 {
                (clip, other)
            } else {
                (other, clip)
            };
            let both = [first.as_slice(), &gap, second.as_slice()].concat();
            let cut = first.len() + gap.len();
            let (a, took) = kept(&both, 0, first.len());
            let (b, _) = kept(&both, cut, both.len());
            times.push(took);
            let (m, t) = if index % 2 == 0 { (a, b) } else { (b, a) };
            of_mine += m;
            of_theirs += t;
            said_mine += (clip.len() / 16) as u64;
            said_theirs += (other.len() / 16) as u64;
            leaked += usize::from(t > 500);
            lost += usize::from(m * 2 < (clip.len() / 16) as u64);
        }
        times.sort_unstable();
        println!(
            "{} dictations with another speaker: kept {:.0}% of the user's speech and {:.1}% of the other's; the other got through in {leaked}, the user was mostly lost in {lost}; took middle {} ms, slowest {} ms",
            own.len(),
            of_mine as f32 * 100.0 / said_mine as f32,
            of_theirs as f32 * 100.0 / said_theirs as f32,
            times[times.len() / 2],
            times[times.len() - 1]
        );
        let alone = others
            .iter()
            .take(20)
            .filter(|other| kept(other, 0, other.len()).0 > 500)
            .count();
        println!(
            "the other speaker alone got through in {alone} of {}",
            others.len().min(20)
        );
    }

    /// Where the time of telling a short recording's speakers goes.
    #[test]
    #[ignore]
    fn times_the_steps_of_a_short_recording() {
        let read =
            |path: &std::path::Path| crate::eval::decode_wav(&std::fs::read(path).ok()?).ok();
        let env = |name: &str| std::env::var(name).unwrap();
        let follower = Follower::open(&env("OT_SEG"), &env("OT_EMB"), 4).unwrap();
        let clips: Vec<Vec<f32>> = std::fs::read_dir(env("OT_DIR"))
            .unwrap()
            .filter_map(|entry| read(&entry.ok()?.path()))
            .take(12)
            .collect();
        let mut session = follower.model.diarize_session().unwrap();
        for clip in &clips {
            let a = std::time::Instant::now();
            let fresh = follower.model.diarize_session().unwrap();
            let opened = a.elapsed().as_millis();
            drop(fresh);
            let b = std::time::Instant::now();
            let segments = session.run(clip, &Default::default()).unwrap();
            let ran = b.elapsed().as_millis();
            let turns = number_by_first_appearance(segments.iter().map(|segment| {
                (
                    segment.t0_ms as f32 / 1000.0,
                    segment.t1_ms as f32 / 1000.0,
                    segment.speaker_id,
                )
            }));
            let c = std::time::Instant::now();
            let voices = voices_of(follower.extractor.as_ref().unwrap(), clip, &turns);
            println!(
                "{:.1}s clip: session {opened} ms, run {ran} ms, {} voices {} ms",
                clip.len() as f32 / 16_000.0,
                voices.len(),
                c.elapsed().as_millis()
            );
        }
    }

    /// Same needs as the test below, plus `OT_WINDOW` seconds per window.
    #[test]
    #[ignore]
    fn keeps_speakers_apart_window_by_window() {
        let wav = std::fs::read(std::env::var("OT_WAV").unwrap()).unwrap();
        let pcm = crate::eval::decode_wav(&wav).unwrap();
        let window: usize = std::env::var("OT_WINDOW").unwrap().parse().unwrap();
        let mut live = LiveSpeakers::open(
            &std::env::var("OT_SEG").unwrap(),
            &std::env::var("OT_EMB").unwrap(),
            4,
        )
        .unwrap();
        let started = std::time::Instant::now();
        for (index, piece) in pcm.chunks(window * 16_000).enumerate() {
            for turn in live.window(piece).unwrap().0 {
                let offset = (index * window * 1000) as u64;
                println!(
                    "{:>6} - {:>6} speaker {}",
                    offset + turn.start_ms,
                    offset + turn.end_ms,
                    turn.speaker
                );
            }
        }
        println!("took {:.1}s", started.elapsed().as_secs_f32());
    }

    /// Needs the two models and a recording. Run with
    /// `OT_SEG=<onnx> OT_EMB=<onnx> OT_WAV=<16 kHz mono 16-bit wav> cargo test real_recording -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn splits_a_real_recording_by_speaker() {
        let wav = std::fs::read(std::env::var("OT_WAV").unwrap()).unwrap();
        let pcm = crate::eval::decode_wav(&wav).unwrap();
        let started = std::time::Instant::now();
        let turns = diarize(
            &std::env::var("OT_SEG").unwrap(),
            &std::env::var("OT_EMB").unwrap(),
            &pcm,
            4,
        )
        .unwrap()
        .0;
        println!(
            "audio {:.1}s took {:.1}s",
            pcm.len() as f32 / 16_000.0,
            started.elapsed().as_secs_f32()
        );
        for turn in &turns {
            println!(
                "{:>6} - {:>6} speaker {}",
                turn.start_ms, turn.end_ms, turn.speaker
            );
        }
        let mut said: std::collections::BTreeMap<u32, u64> = Default::default();
        for turn in &turns {
            *said.entry(turn.speaker).or_default() += (turn.end_ms - turn.start_ms) / 1000;
        }
        println!("seconds by speaker {said:?}");
    }
}
