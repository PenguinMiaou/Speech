//! Messages exchanged between the app and `ot-stt-worker` over stdin and stdout.

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::io::{self, Read, Write};

/// Largest JSON frame accepted. Audio travels outside frames, so this only bounds headers.
pub const MAX_FRAME_BYTES: u32 = 1024 * 1024;
/// Twelve minutes of 16 kHz mono f32 audio.
pub const MAX_PCM_BYTES: u32 = 46_080_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Request {
    Load {
        model_path: String,
    },
    Transcribe {
        vocabulary: Vec<String>,
        prompt: Option<String>,
        /// The language spoken (an ISO code), when the caller knows it. An older app sends
        /// none, and the model then tells the language itself.
        #[serde(default)]
        language: Option<String>,
        pcm_bytes: u32,
    },
    /// Starts keeping only the voice in `voice_sample` for the recording now beginning.
    IsolateBegin {
        enroll_model: String,
        chunk_model: String,
        voice_sample: String,
    },
    /// The next piece of the recording, followed by `pcm_bytes` of audio.
    IsolateFeed {
        pcm_bytes: u32,
    },
    /// Ends the recording. The answer carries the isolated audio.
    IsolateEnd,
    /// Tells who spoke when in the 16 kHz mono `f32` audio stored at `pcm_path`.
    Diarize {
        segmentation_model: String,
        embedding_model: String,
        pcm_path: String,
    },
    /// The same for the next window of a recording still running. Speakers keep their
    /// numbers from window to window; `first` starts a new recording.
    SpeakersWindow {
        segmentation_model: String,
        embedding_model: String,
        pcm_path: String,
        first: bool,
    },
    Shutdown,
}

/// A speaker's voice as the numbers the voice model gives it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpeakerVoice {
    pub speaker: u32,
    pub embedding: Vec<f32>,
}

impl Eq for SpeakerVoice {}

/// One stretch of speech by one speaker. Speakers are numbered from 1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpeakerTurn {
    pub start_ms: u64,
    pub end_ms: u64,
    pub speaker: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Response {
    Ready {
        backend: String,
        load_ms: u64,
    },
    Transcript {
        text: String,
        language: Option<String>,
        elapsed_ms: u64,
    },
    /// Who spoke when, in order of start time.
    Speakers {
        turns: Vec<SpeakerTurn>,
        /// What each speaker's voice sounds like, for telling known people by voice.
        #[serde(default)]
        voices: Vec<SpeakerVoice>,
    },
    /// The recording is being isolated; more audio may follow.
    Isolating,
    /// Followed by `pcm_bytes` of isolated audio.
    Isolated {
        pcm_bytes: u32,
    },
    Error {
        code: String,
        message: String,
    },
}

/// Writes a 4-byte little-endian length followed by the JSON body, then flushes.
pub fn write_frame<W: Write, T: Serialize>(w: &mut W, value: &T) -> io::Result<()> {
    let body = serde_json::to_vec(value)?;
    if body.len() > MAX_FRAME_BYTES as usize {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "frame too large",
        ));
    }
    w.write_all(&(body.len() as u32).to_le_bytes())?;
    w.write_all(&body)?;
    w.flush()
}

/// Returns `None` when the stream ends cleanly before a new frame starts.
pub fn read_frame<R: Read, T: DeserializeOwned>(r: &mut R) -> io::Result<Option<T>> {
    let mut length = [0u8; 4];
    match r.read_exact(&mut length) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(error) => return Err(error),
    }
    let length = u32::from_le_bytes(length);
    if length > MAX_FRAME_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "frame too large",
        ));
    }
    let mut body = vec![0u8; length as usize];
    r.read_exact(&mut body)?;
    Ok(Some(serde_json::from_slice(&body)?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn a_request_without_a_language_is_still_read() {
        let old = br#"{"Transcribe":{"vocabulary":[],"prompt":null,"pcm_bytes":8}}"#;
        assert_eq!(
            serde_json::from_slice::<Request>(old).unwrap(),
            Request::Transcribe {
                vocabulary: vec![],
                prompt: None,
                language: None,
                pcm_bytes: 8,
            }
        );
    }

    #[test]
    fn frame_round_trip() {
        let request = Request::Transcribe {
            vocabulary: vec!["TypeScript".into()],
            prompt: Some("前文".into()),
            language: Some("en".into()),
            pcm_bytes: 8,
        };
        let mut buffer = Vec::new();
        write_frame(&mut buffer, &request).unwrap();
        write_frame(&mut buffer, &Request::Shutdown).unwrap();

        let mut reader = Cursor::new(buffer);
        assert_eq!(
            read_frame::<_, Request>(&mut reader).unwrap(),
            Some(request)
        );
        assert_eq!(
            read_frame::<_, Request>(&mut reader).unwrap(),
            Some(Request::Shutdown)
        );
    }

    #[test]
    fn eof_returns_none() {
        let mut reader = Cursor::new(Vec::new());
        assert_eq!(read_frame::<_, Request>(&mut reader).unwrap(), None);
    }

    #[test]
    fn truncated_frame_is_an_error() {
        let mut buffer = Vec::new();
        write_frame(&mut buffer, &Request::Shutdown).unwrap();
        buffer.truncate(buffer.len() - 1);
        assert!(read_frame::<_, Request>(&mut Cursor::new(buffer)).is_err());
    }

    #[test]
    fn oversized_frame_rejected() {
        let buffer = (MAX_FRAME_BYTES + 1).to_le_bytes().to_vec();
        let error = read_frame::<_, Request>(&mut Cursor::new(buffer)).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    }

    #[test]
    fn pcm_limit_is_twelve_minutes_of_f32() {
        assert_eq!(MAX_PCM_BYTES, 12 * 60 * 16_000 * 4);
    }
}
