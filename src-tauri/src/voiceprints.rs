//! Voices the user has put a name to. When a later meeting has a speaker who sounds the
//! same, that speaker gets the name. Every meeting in which a name is confirmed or
//! corrected adds a sample of that voice, so the match gets better with use. The voices stay in one file in the app's data folder
//! and are only ever compared on this machine.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use ot_stt_protocol::SpeakerVoice;
use serde::{Deserialize, Serialize};

/// How alike a speaker must sound to a remembered voice to get its name. Measured on real
/// meetings: one person scores 0.7 to 0.94 against themselves, different people 0.37 to 0.6.
const SAME_PERSON: f32 = 0.68;

/// The most samples kept of one person's voice; the oldest goes first.
const MOST_SAMPLES: usize = 8;
/// Two samples this alike are the same stretch of speech.
const SAME_SAMPLE: f32 = 0.999;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Voiceprint {
    name: String,
    /// The voice as heard in different meetings, the oldest first.
    #[serde(default)]
    embeddings: Vec<Vec<f32>>,
    /// Where the first version of this file kept its single sample.
    #[serde(default, skip_serializing)]
    embedding: Vec<f32>,
}

impl Voiceprint {
    /// How alike `voice` is to the sample of this person it is most like.
    fn score(&self, voice: &[f32]) -> f32 {
        self.embeddings
            .iter()
            .map(|sample| alike(sample, voice))
            .fold(0.0, f32::max)
    }
}

static DATA_DIR: OnceLock<PathBuf> = OnceLock::new();
/// The voices of the speakers in the meeting in hand, by speaker number.
static HEARD: Mutex<Option<HashMap<u32, Vec<f32>>>> = Mutex::new(None);

pub fn init(data_dir: &Path) {
    let _ = DATA_DIR.set(data_dir.to_path_buf());
}

fn store() -> Option<PathBuf> {
    DATA_DIR.get().map(|dir| dir.join("voiceprints.json"))
}

fn read(store: &Path) -> Vec<Voiceprint> {
    let mut prints: Vec<Voiceprint> = std::fs::read(store)
        .ok()
        .and_then(|raw| serde_json::from_slice(&raw).ok())
        .unwrap_or_default();
    for print in &mut prints {
        if !print.embedding.is_empty() {
            print.embeddings.push(std::mem::take(&mut print.embedding));
        }
    }
    prints
}

fn write(store: &Path, prints: &[Voiceprint]) -> Result<(), String> {
    let raw = serde_json::to_vec(prints).map_err(|error| error.to_string())?;
    std::fs::write(store, raw).map_err(|error| error.to_string())
}

fn alike(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    let size = |v: &[f32]| v.iter().map(|x| x * x).sum::<f32>().sqrt();
    let sizes = size(a) * size(b);
    if sizes > 0.0 {
        dot / sizes
    } else {
        0.0
    }
}

/// The name for each speaker who sounds like a remembered voice. A name goes to one
/// speaker only, the one who sounds most like it.
fn match_names(prints: &[Voiceprint], heard: &HashMap<u32, Vec<f32>>) -> HashMap<u32, String> {
    let mut pairs: Vec<(f32, u32, &str)> = Vec::new();
    for (speaker, voice) in heard {
        for print in prints {
            let score = print.score(voice);
            if score >= SAME_PERSON {
                pairs.push((score, *speaker, &print.name));
            }
        }
    }
    pairs.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)));
    let mut names: HashMap<u32, String> = HashMap::new();
    for (_, speaker, name) in pairs {
        if !names.contains_key(&speaker) && !names.values().any(|taken| taken == name) {
            names.insert(speaker, name.to_string());
        }
    }
    names
}

/// Adds a sample of a voice under `name`. The same stretch of speech filed under another
/// name is taken away from there: that is the user correcting a wrong name.
fn remember_in(store: &Path, name: &str, embedding: Vec<f32>) -> Result<(), String> {
    let mut prints = read(store);
    for print in &mut prints {
        print
            .embeddings
            .retain(|sample| alike(sample, &embedding) < SAME_SAMPLE);
    }
    prints.retain(|print| print.name == name || !print.embeddings.is_empty());
    match prints.iter_mut().find(|print| print.name == name) {
        Some(print) => {
            print.embeddings.push(embedding);
            if print.embeddings.len() > MOST_SAMPLES {
                print.embeddings.remove(0);
            }
        }
        None => prints.push(Voiceprint {
            name: name.to_string(),
            embeddings: vec![embedding],
            embedding: Vec::new(),
        }),
    }
    write(store, &prints)
}

fn heard() -> std::sync::MutexGuard<'static, Option<HashMap<u32, Vec<f32>>>> {
    HEARD.lock().unwrap_or_else(|error| error.into_inner())
}

/// Takes in the voices a speaker request found. `fresh` starts a new meeting.
pub fn hear(voices: Vec<SpeakerVoice>, fresh: bool) {
    let mut heard = heard();
    if fresh {
        *heard = None;
    }
    let known = heard.get_or_insert_with(HashMap::new);
    for voice in voices {
        known.entry(voice.speaker).or_insert(voice.embedding);
    }
}

/// The remembered names of the speakers in the meeting in hand.
pub fn names() -> HashMap<u32, String> {
    let (Some(store), Some(heard)) = (store(), heard().clone()) else {
        return HashMap::new();
    };
    match_names(&read(&store), &heard)
}

/// Remembers the voice of a speaker of the meeting in hand under `name`.
pub fn remember(speaker: u32, name: &str) -> Result<Vec<String>, String> {
    let name = name.trim();
    let store = store().ok_or("voice_store_unavailable")?;
    let voice = heard()
        .as_ref()
        .and_then(|heard| heard.get(&speaker).cloned());
    if let (false, Some(voice)) = (name.is_empty(), voice) {
        remember_in(&store, name, voice)?;
    }
    Ok(remembered())
}

/// Takes the names the speakers of the meeting in hand ended up with as one more sample
/// of each of those voices.
pub fn confirm(names: &HashMap<u32, String>) -> Result<Vec<String>, String> {
    for (speaker, name) in names {
        remember(*speaker, name)?;
    }
    Ok(remembered())
}

/// The names of the remembered voices.
pub fn remembered() -> Vec<String> {
    store()
        .map(|store| read(&store).into_iter().map(|print| print.name).collect())
        .unwrap_or_default()
}

pub fn forget(name: &str) -> Result<Vec<String>, String> {
    let store = store().ok_or("voice_store_unavailable")?;
    let mut prints = read(&store);
    prints.retain(|print| print.name != name);
    write(&store, &prints)?;
    Ok(remembered())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn print(name: &str, embedding: &[f32]) -> Voiceprint {
        Voiceprint {
            name: name.into(),
            embeddings: vec![embedding.to_vec()],
            embedding: Vec::new(),
        }
    }

    fn scratch(name: &str) -> PathBuf {
        let store =
            std::env::temp_dir().join(format!("ot-voices-{}-{name}.json", std::process::id()));
        let _ = std::fs::remove_file(&store);
        store
    }

    #[test]
    fn a_speaker_who_sounds_like_a_remembered_voice_gets_its_name() {
        let prints = [
            print("Ann", &[1.0, 0.0, 0.0]),
            print("Bo", &[0.0, 1.0, 0.0]),
        ];
        let heard = HashMap::from([
            (1, vec![0.1, 0.9, 0.0]),
            (2, vec![0.0, 0.0, 1.0]),
            (3, vec![0.9, 0.2, 0.0]),
        ]);
        assert_eq!(
            match_names(&prints, &heard),
            HashMap::from([(1, "Bo".to_string()), (3, "Ann".to_string())])
        );
    }

    #[test]
    fn a_name_goes_only_to_the_speaker_who_sounds_most_like_it() {
        let prints = [print("Ann", &[1.0, 0.0])];
        let heard = HashMap::from([(1, vec![0.8, 0.5]), (2, vec![1.0, 0.1])]);
        assert_eq!(
            match_names(&prints, &heard),
            HashMap::from([(2, "Ann".to_string())])
        );
    }

    #[test]
    fn voices_of_another_size_or_no_size_match_nobody() {
        assert_eq!(alike(&[1.0, 0.0], &[1.0]), 0.0);
        assert_eq!(alike(&[0.0, 0.0], &[1.0, 0.0]), 0.0);
        assert!((alike(&[2.0, 0.0], &[5.0, 0.0]) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn each_meeting_adds_a_sample_and_a_speaker_matches_the_closest_one() {
        let store = scratch("learn");
        remember_in(&store, "Ann", vec![1.0, 0.0, 0.0]).unwrap();
        // Ann on another microphone sounds different; alone, the first sample misses her.
        let heard = HashMap::from([(1, vec![0.5, 0.85, 0.0])]);
        assert!(match_names(&read(&store), &heard).is_empty());
        remember_in(&store, "Ann", vec![0.4, 0.9, 0.0]).unwrap();
        assert_eq!(read(&store)[0].embeddings.len(), 2);
        assert_eq!(
            match_names(&read(&store), &heard),
            HashMap::from([(1, "Ann".to_string())])
        );
    }

    #[test]
    fn the_same_speech_under_a_new_name_leaves_the_old_name() {
        let store = scratch("correct");
        remember_in(&store, "Ann", vec![1.0, 0.0]).unwrap();
        remember_in(&store, "Ann", vec![0.0, 1.0]).unwrap();
        remember_in(&store, "Bo", vec![0.0, 1.0]).unwrap();
        let prints = read(&store);
        assert_eq!(prints[0].embeddings, vec![vec![1.0, 0.0]]);
        assert_eq!(prints[1], print("Bo", &[0.0, 1.0]));
        // A name whose only sample moved away is gone.
        remember_in(&store, "Bo", vec![1.0, 0.0]).unwrap();
        assert_eq!(read(&store).len(), 1);
        // Saying the same thing twice does not count twice.
        remember_in(&store, "Bo", vec![1.0, 0.0]).unwrap();
        assert_eq!(read(&store)[0].embeddings.len(), 2);
    }

    #[test]
    fn only_the_newest_samples_of_a_voice_are_kept() {
        let store = scratch("cap");
        let sample = |index: usize| {
            let mut voice = vec![0.0; MOST_SAMPLES + 2];
            voice[index] = 1.0;
            voice
        };
        for index in 0..MOST_SAMPLES + 2 {
            remember_in(&store, "Ann", sample(index)).unwrap();
        }
        let samples = &read(&store)[0].embeddings;
        assert_eq!(samples.len(), MOST_SAMPLES);
        assert_eq!(samples[0], sample(2));
    }

    #[test]
    fn a_file_from_the_first_version_is_still_read() {
        let store = scratch("old");
        std::fs::write(&store, r#"[{"name":"Ann","embedding":[1.0,0.0]}]"#).unwrap();
        assert_eq!(read(&store), vec![print("Ann", &[1.0, 0.0])]);
    }
}
