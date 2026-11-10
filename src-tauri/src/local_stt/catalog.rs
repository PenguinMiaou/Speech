//! The models the app can install. Every entry is pinned to a revision and a checksum.

use std::path::PathBuf;
use std::sync::OnceLock;

pub const DEFAULT_LOCAL_MODEL_ID: &str = "qwen3-asr-1.7b-q8";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModelEntry {
    pub id: &'static str,
    pub file_name: &'static str,
    pub repo: &'static str,
    pub revision: &'static str,
    pub size_bytes: u64,
    pub sha256: &'static str,
    pub recommended: bool,
}

impl ModelEntry {
    pub fn url(&self) -> String {
        format!(
            "https://huggingface.co/{}/resolve/{}/{}",
            self.repo, self.revision, self.file_name
        )
    }
}

const ENTRIES: [ModelEntry; 7] = [
    ModelEntry {
        id: DEFAULT_LOCAL_MODEL_ID,
        file_name: "Qwen3-ASR-1.7B-Q8_0.gguf",
        repo: "handy-computer/Qwen3-ASR-1.7B-gguf",
        revision: "92282af1610a2db19d66f2bef1e260f5deca782d",
        size_bytes: 2_185_030_624,
        sha256: "9a0d81792dfea2d5f278b8a63deb3ea6e02139ce42c2301f32ea19c4f77526b7",
        recommended: true,
    },
    ModelEntry {
        id: "qwen3-asr-0.6b-q8",
        file_name: "Qwen3-ASR-0.6B-Q8_0.gguf",
        repo: "handy-computer/Qwen3-ASR-0.6B-gguf",
        revision: "e4e16599b900eb0cb36e524514756bb92eb092b7",
        size_bytes: 850_423_456,
        sha256: "f081b2d5e23bd669d92cc331d722a8a0681943b8e6f34b48996fd5c319b5acd8",
        recommended: false,
    },
    ModelEntry {
        id: "qwen3-asr-1.7b-f16",
        file_name: "Qwen3-ASR-1.7B-F16.gguf",
        repo: "handy-computer/Qwen3-ASR-1.7B-gguf",
        revision: "0e0a596e3959c3f3f2ab7d443a5b6f0f4a4bba6f",
        size_bytes: 4_091_390_944,
        sha256: "edb09c29b8f73822c639168d5ef72aa2dccdf8b4e48fc4b8518885352ff62c71",
        recommended: false,
    },
    ModelEntry {
        id: "moss-transcribe-diarize-q8",
        file_name: "MOSS-Transcribe-Diarize-Q8_0.gguf",
        repo: "handy-computer/moss-transcribe-diarize-gguf",
        revision: "5780e665a3eb60694e0402ed25b6775ee587681b",
        size_bytes: 986_900_160,
        sha256: "6bc3afe18c18ceec90f8c7e22300942db27ff0549c08f8e864f7168866445944",
        recommended: false,
    },
    ModelEntry {
        id: "cohere-transcribe-q8",
        file_name: "cohere-transcribe-03-2026-Q8_0.gguf",
        repo: "handy-computer/cohere-transcribe-03-2026-gguf",
        revision: "7353b6040bdc1be9e5a3823d99d5fe12afe88264",
        size_bytes: 2_410_655_232,
        sha256: "931916663432fd895423a4291a8400221802b288967ca2d435fc5e3141c9e71e",
        recommended: false,
    },
    ModelEntry {
        id: "voxtral-mini-4b-q8",
        file_name: "Voxtral-Mini-4B-Realtime-2602-Q8_0.gguf",
        repo: "handy-computer/Voxtral-Mini-4B-Realtime-2602-gguf",
        revision: "7fd3d424fc558edca2c53808578b0541ef52a81a",
        size_bytes: 4_731_791_648,
        sha256: "6fb22249463c1e7a600d920ad732e2dd0a231510e9db59fb8061d3d1d48d0c67",
        recommended: false,
    },
    ModelEntry {
        id: "nemotron-3.5-0.6b-q8",
        file_name: "nemotron-3.5-asr-streaming-0.6b-Q8_0.gguf",
        repo: "handy-computer/nemotron-3.5-asr-streaming-0.6b-gguf",
        revision: "53ce288f69326b415aeef030cfcb02a08a8096dd",
        size_bytes: 751_094_240,
        sha256: "b94545b313b3223fda7b2857a52681da813935c2127643d1e9ff0c23d988089c",
        recommended: false,
    },
];

static MODELS_DIR: OnceLock<PathBuf> = OnceLock::new();

pub fn entries() -> &'static [ModelEntry] {
    &ENTRIES
}

pub fn find(id: &str) -> Option<&'static ModelEntry> {
    ENTRIES.iter().find(|entry| entry.id == id)
}

pub fn set_models_dir(dir: PathBuf) {
    let _ = MODELS_DIR.set(dir);
}

pub fn models_dir() -> Option<PathBuf> {
    MODELS_DIR.get().cloned()
}

/// A model counts as installed only when the final file exists at its exact size.
pub fn installed_path(id: &str) -> Option<PathBuf> {
    let entry = find(id)?;
    let path = models_dir()?.join(entry.file_name);
    let size = std::fs::metadata(&path).ok()?.len();
    (size == entry.size_bytes).then_some(path)
}

pub fn any_installed() -> bool {
    ENTRIES
        .iter()
        .any(|entry| installed_path(entry.id).is_some())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_entries_are_pinned() {
        for entry in entries() {
            assert!(entry.url().contains(entry.revision));
            assert_eq!(entry.revision.len(), 40);
            assert_eq!(entry.sha256.len(), 64);
            assert!(entry.size_bytes > 0);
        }
    }

    #[test]
    fn every_model_has_its_own_id_and_file() {
        let ids: std::collections::HashSet<_> = entries().iter().map(|entry| entry.id).collect();
        let files: std::collections::HashSet<_> =
            entries().iter().map(|entry| entry.file_name).collect();
        assert_eq!(ids.len(), entries().len());
        assert_eq!(files.len(), entries().len());
    }

    #[test]
    fn the_catalog_offers_the_models_from_the_survey() {
        for id in [
            "qwen3-asr-1.7b-f16",
            "moss-transcribe-diarize-q8",
            "cohere-transcribe-q8",
            "voxtral-mini-4b-q8",
            "nemotron-3.5-0.6b-q8",
        ] {
            assert!(find(id).is_some(), "{id} is missing");
        }
    }

    #[test]
    fn exactly_one_model_is_recommended_and_it_is_the_default() {
        let recommended: Vec<_> = entries().iter().filter(|entry| entry.recommended).collect();
        assert_eq!(recommended.len(), 1);
        assert_eq!(recommended[0].id, DEFAULT_LOCAL_MODEL_ID);
    }

    #[test]
    fn unknown_model_is_not_installed() {
        assert_eq!(installed_path("no-such-model"), None);
    }
}
