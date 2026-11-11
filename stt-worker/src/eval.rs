//! Runs the fixed test set through the same engine the app uses. Text goes to a local file, never to stdout.

use crate::serve::Engine;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Instant;

pub struct EvalArgs {
    pub model: String,
    pub dir: PathBuf,
    pub label: String,
    pub vocabulary: Vec<String>,
    pub use_context: bool,
    /// The language every clip is in, when the run names one.
    pub language: Option<String>,
}

pub fn parse_args(args: &[String]) -> Result<EvalArgs, String> {
    let (mut model, mut dir, mut label, mut vocab_file, mut use_context) =
        (None, None, None, None, false);
    let mut language = None;
    let mut iter = args.iter();
    while let Some(flag) = iter.next() {
        let mut value = || {
            iter.next()
                .cloned()
                .ok_or_else(|| format!("{flag} needs a value"))
        };
        match flag.as_str() {
            "--model" => model = Some(value()?),
            "--dir" => dir = Some(value()?),
            "--label" => label = Some(value()?),
            "--vocab-file" => vocab_file = Some(value()?),
            "--context" => use_context = true,
            "--language" => language = Some(value()?),
            other => return Err(format!("unknown flag {other}")),
        }
    }
    let vocabulary = match vocab_file {
        Some(path) => {
            let text = fs::read_to_string(&path).map_err(|e| format!("{path}: {e}"))?;
            serde_json::from_str(&text).map_err(|e| format!("{path}: {e}"))?
        }
        None => Vec::new(),
    };
    Ok(EvalArgs {
        model: model.ok_or("--model is required")?,
        dir: PathBuf::from(dir.ok_or("--dir is required")?),
        label: label.ok_or("--label is required")?,
        vocabulary,
        use_context,
        language,
    })
}

/// Reads 16-bit PCM from a WAV file, skipping any chunks before the audio data.
pub fn decode_wav(bytes: &[u8]) -> Result<Vec<f32>, String> {
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err("not a WAV file".into());
    }
    let mut position = 12;
    while position + 8 <= bytes.len() {
        let id = &bytes[position..position + 4];
        let size =
            u32::from_le_bytes(bytes[position + 4..position + 8].try_into().unwrap()) as usize;
        let start = position + 8;
        if id == b"data" {
            let end = (start + size).min(bytes.len());
            return Ok(bytes[start..end]
                .chunks_exact(2)
                .map(|c| i16::from_le_bytes([c[0], c[1]]) as f32 / 32768.0)
                .collect());
        }
        position = start + size + (size & 1);
    }
    Err("WAV file has no data chunk".into())
}

fn resolve(dir: &Path, wav: &str) -> PathBuf {
    let path = PathBuf::from(wav);
    if path.is_absolute() {
        path
    } else {
        dir.join(path)
    }
}

pub fn run<E: Engine>(args: &EvalArgs, engine: &mut E) -> Result<(), String> {
    let read_json = |path: PathBuf| -> Result<Value, String> {
        let text = fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))
    };
    let manifest = read_json(args.dir.join("manifest.json"))?;
    let items = manifest["items"]
        .as_array()
        .ok_or("manifest has no items")?;
    let contexts: HashMap<String, String> = if args.use_context {
        serde_json::from_value(read_json(args.dir.join("contexts.json"))?)
            .map_err(|e| e.to_string())?
    } else {
        HashMap::new()
    };

    let backend = engine
        .load(&args.model)
        .map_err(|e| format!("{}: {}", e.code, e.message))?;
    println!("backend {backend}");
    engine.set_language(args.language.clone());

    let results = args.dir.join("results");
    fs::create_dir_all(&results).map_err(|e| e.to_string())?;
    let target = results.join(format!("{}.jsonl", args.label));
    let temporary = target.with_extension("jsonl.tmp");
    let mut file = fs::File::create(&temporary).map_err(|e| e.to_string())?;

    let started = Instant::now();
    for item in items {
        let idx = item["idx"].as_i64().ok_or("item has no idx")?;
        let wav = resolve(&args.dir, item["wav"].as_str().ok_or("item has no wav")?);
        let bytes = fs::read(&wav).map_err(|e| format!("{}: {e}", wav.display()))?;
        let pcm = decode_wav(&bytes)?;
        let prompt = contexts.get(&idx.to_string()).cloned();
        let clip_started = Instant::now();
        let (text, language, error) = match engine.transcribe(&pcm, args.vocabulary.clone(), prompt)
        {
            Ok(output) => (output.text, output.language, None),
            Err(e) => (String::new(), None, Some(e.code)),
        };
        let line = json!({
            "idx": idx,
            "text": text,
            "lang": language,
            "err": error,
            "secs": clip_started.elapsed().as_secs_f64(),
        });
        writeln!(file, "{line}").map_err(|e| e.to_string())?;
    }
    file.flush().map_err(|e| e.to_string())?;
    fs::rename(&temporary, &target).map_err(|e| e.to_string())?;
    println!(
        "{} done in {:.0}s",
        args.label,
        started.elapsed().as_secs_f64()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wav_with_list_chunk(samples: &[i16]) -> Vec<u8> {
        let mut bytes = b"RIFF\0\0\0\0WAVE".to_vec();
        bytes.extend_from_slice(b"LIST");
        bytes.extend_from_slice(&3u32.to_le_bytes());
        bytes.extend_from_slice(b"abc\0");
        bytes.extend_from_slice(b"data");
        bytes.extend_from_slice(&((samples.len() * 2) as u32).to_le_bytes());
        for sample in samples {
            bytes.extend_from_slice(&sample.to_le_bytes());
        }
        bytes
    }

    #[test]
    fn decodes_audio_after_other_chunks() {
        let pcm = decode_wav(&wav_with_list_chunk(&[16384, -32768])).unwrap();
        assert_eq!(pcm, vec![0.5, -1.0]);
    }

    #[test]
    fn rejects_files_that_are_not_wav() {
        assert!(decode_wav(b"not audio at all").is_err());
    }

    #[test]
    fn parse_args_requires_model() {
        let args: Vec<String> = ["--dir", "d", "--label", "l"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert!(parse_args(&args).is_err());
    }
}
