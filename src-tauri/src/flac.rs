//! A meeting's sound kept as FLAC: the same samples as the recording, in about half the
//! space. Written a block at a time, so that a long meeting needs no more memory than a
//! short one, and read back through the decoder before the recording it was made from is
//! let go.

use flacenc::bitsink::MemSink;
use flacenc::component::{BitRepr, Stream};
use flacenc::error::Verify;
use flacenc::source::{Context, Fill, FrameBuf};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;

/// Samples in a block; 4096 is what the reference encoder uses at this rate.
const BLOCK: usize = 4096;
const RATE: usize = 16_000;

/// Writes 16 kHz, one-channel, 16-bit samples to a FLAC file as they are given.
pub struct Encoder {
    file: std::fs::File,
    config: flacenc::error::Verified<flacenc::config::Encoder>,
    stream: Stream,
    buffer: (FrameBuf, Context),
    /// Samples given and not yet written, fewer than a block.
    waiting: Vec<i16>,
    bytes: Vec<u8>,
    sink: MemSink<u8>,
}

fn failed(error: impl std::fmt::Debug) -> String {
    format!("flac: {error:?}")
}

/// The file's start: its mark and the description of the stream, always 42 bytes, written
/// again with the true length and digest once every sample is in.
fn head(stream: &Stream) -> Result<Vec<u8>, String> {
    let mut sink = MemSink::<u8>::new();
    stream.write(&mut sink).map_err(failed)?;
    Ok(sink.as_slice().to_vec())
}

impl Encoder {
    pub fn create(path: &Path) -> Result<Self, String> {
        let config = flacenc::config::Encoder::default()
            .into_verified()
            .map_err(failed)?;
        let mut stream = Stream::new(RATE, 1, 16).map_err(failed)?;
        stream
            .stream_info_mut()
            .set_block_sizes(BLOCK, BLOCK)
            .map_err(failed)?;
        let mut file = std::fs::File::create(path).map_err(|error| error.to_string())?;
        file.write_all(&head(&stream)?)
            .map_err(|error| error.to_string())?;
        Ok(Self {
            file,
            config,
            stream,
            buffer: (
                FrameBuf::with_size(1, BLOCK).map_err(failed)?,
                Context::new(16, 1),
            ),
            waiting: Vec::with_capacity(BLOCK),
            bytes: Vec::with_capacity(BLOCK * 2),
            sink: MemSink::new(),
        })
    }

    pub fn push(&mut self, mut samples: &[i16]) -> Result<(), String> {
        while !samples.is_empty() {
            let room = BLOCK - self.waiting.len();
            let (now, later) = samples.split_at(room.min(samples.len()));
            self.waiting.extend_from_slice(now);
            samples = later;
            if self.waiting.len() == BLOCK {
                self.block()?;
            }
        }
        Ok(())
    }

    fn block(&mut self) -> Result<(), String> {
        if self.waiting.is_empty() {
            return Ok(());
        }
        self.bytes.clear();
        for sample in &self.waiting {
            self.bytes.extend_from_slice(&sample.to_le_bytes());
        }
        self.waiting.clear();
        self.buffer.fill_le_bytes(&self.bytes, 2).map_err(failed)?;
        let number = self
            .buffer
            .1
            .current_frame_number()
            .ok_or("flac: no frame")?;
        let frame = flacenc::encode_fixed_size_frame(
            &self.config,
            &self.buffer.0,
            number,
            self.stream.stream_info(),
        )
        .map_err(failed)?;
        self.stream.stream_info_mut().update_frame_info(&frame);
        self.sink.clear();
        frame.write(&mut self.sink).map_err(failed)?;
        self.file
            .write_all(self.sink.as_slice())
            .map_err(|error| error.to_string())
    }

    /// Writes what is left and the stream's true length and digest. Returns the number of
    /// samples written.
    pub fn finish(mut self) -> Result<u64, String> {
        self.block()?;
        let total = self.buffer.1.total_samples();
        let info = self.stream.stream_info_mut();
        info.set_md5_digest(&self.buffer.1.md5_digest());
        info.set_total_samples(total);
        // The last block may be shorter; like the reference encoder, the stream still
        // names its one block size.
        info.set_block_sizes(BLOCK, BLOCK).map_err(failed)?;
        let start = head(&self.stream)?;
        self.file
            .seek(SeekFrom::Start(0))
            .and_then(|_| self.file.write_all(&start))
            .and_then(|_| self.file.sync_all())
            .map_err(|error| error.to_string())?;
        Ok(total as u64)
    }
}

/// Reads a FLAC file block by block, handing each block's samples to `take` until it
/// returns false, and checks the samples against the digest the file carries. Returns the
/// number of samples read, or an error where the file is not whole.
pub fn read(
    path: &Path,
    mut take: impl FnMut(&[i16]) -> Result<bool, String>,
) -> Result<u64, String> {
    use symphonia::core::audio::SampleBuffer;
    use symphonia::core::codecs::DecoderOptions;
    use symphonia::core::errors::Error;
    use symphonia::core::io::MediaSourceStream;
    use symphonia::core::probe::Hint;

    let file = std::fs::File::open(path).map_err(|error| error.to_string())?;
    let source = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    hint.with_extension("flac");
    let mut format = symphonia::default::get_probe()
        .format(&hint, source, &Default::default(), &Default::default())
        .map_err(|error| error.to_string())?
        .format;
    let track = format.default_track().ok_or("flac: no sound")?;
    let id = track.id;
    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions { verify: true })
        .map_err(|error| error.to_string())?;
    let mut buffer: Option<SampleBuffer<i16>> = None;
    let mut count = 0u64;
    let mut whole = true;
    loop {
        let packet = match format.next_packet() {
            Ok(packet) => packet,
            Err(Error::IoError(error)) if error.kind() == std::io::ErrorKind::UnexpectedEof => {
                break
            }
            Err(error) => return Err(error.to_string()),
        };
        if packet.track_id() != id {
            continue;
        }
        let decoded = decoder.decode(&packet).map_err(|error| error.to_string())?;
        if buffer
            .as_ref()
            .is_none_or(|buffer| buffer.capacity() < decoded.capacity())
        {
            buffer = Some(SampleBuffer::new(
                decoded.capacity() as u64,
                *decoded.spec(),
            ));
        }
        let buffer = buffer.as_mut().expect("made above");
        buffer.copy_interleaved_ref(decoded);
        count += buffer.samples().len() as u64;
        if !take(buffer.samples())? {
            whole = false;
            break;
        }
    }
    // Only a file read to its end can be checked against its digest.
    if whole && decoder.finalize().verify_ok != Some(true) {
        return Err("flac: the sound does not match its digest".to_string());
    }
    Ok(count)
}

/// Writes the samples of a 16 kHz, one-channel, 16-bit WAV body read from `wav` to `out`
/// as FLAC, then reads `out` back whole. Returns the number of samples.
pub fn from_wav(wav: &mut impl Read, out: &Path) -> Result<u64, String> {
    let mut encoder = Encoder::create(out)?;
    let mut bytes = vec![0u8; BLOCK * 2 * 16];
    let mut samples = Vec::with_capacity(BLOCK * 16);
    let mut odd: Option<u8> = None;
    loop {
        let read = wav.read(&mut bytes).map_err(|error| error.to_string())?;
        if read == 0 {
            break;
        }
        samples.clear();
        let mut at = 0;
        if let Some(low) = odd.take() {
            samples.push(i16::from_le_bytes([low, bytes[0]]));
            at = 1;
        }
        let pairs = bytes[at..read].chunks_exact(2);
        odd = pairs.remainder().first().copied();
        samples.extend(pairs.map(|pair| i16::from_le_bytes([pair[0], pair[1]])));
        encoder.push(&samples)?;
    }
    let written = encoder.finish()?;
    // A recording with no sound in it stays as it is: there is nothing to save.
    if written == 0 {
        let _ = std::fs::remove_file(out);
        return Err("flac: no sound".to_string());
    }
    let read = read(out, |_| Ok(true))?;
    if read != written {
        return Err(format!("flac: wrote {written} samples, read back {read}"));
    }
    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("ot-flac-{}-{name}", std::process::id()))
    }

    /// Speech-like sound that does not repeat: a few tones with noise on them.
    fn sound(length: usize) -> Vec<i16> {
        let mut seed = 7u32;
        (0..length)
            .map(|at| {
                seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                let t = at as f32 / RATE as f32;
                let tone = (t * 220.0 * std::f32::consts::TAU).sin() * 6000.0
                    + (t * 1250.0 * std::f32::consts::TAU).sin() * 2000.0;
                (tone + (seed >> 20) as f32 - 2048.0) as i16
            })
            .collect()
    }

    #[test]
    fn gives_back_every_sample_as_it_was_and_takes_less_room() {
        // Not a whole number of blocks, and an odd number of bytes per read.
        let samples = sound(RATE * 7 + 123);
        let body: Vec<u8> = samples.iter().flat_map(|s| s.to_le_bytes()).collect();
        let out = temp("round.flac");
        let written = from_wav(&mut std::io::Cursor::new(&body), &out).unwrap();
        assert_eq!(written, samples.len() as u64);
        let mut back = Vec::new();
        read(&out, |block| {
            back.extend_from_slice(block);
            Ok(true)
        })
        .unwrap();
        assert_eq!(back, samples);
        assert!(std::fs::metadata(&out).unwrap().len() < body.len() as u64);

        // A file whose sound was changed after it was written is refused.
        let mut bytes = std::fs::read(&out).unwrap();
        let middle = bytes.len() / 2;
        bytes[middle] ^= 0x55;
        std::fs::write(&out, &bytes).unwrap();
        assert!(read(&out, |_| Ok(true)).is_err());
        std::fs::remove_file(&out).unwrap();
    }

    #[test]
    fn a_packed_file_is_heard_again_as_the_recording_was() {
        let samples = sound(RATE * 3 + 77);
        let body: Vec<u8> = samples.iter().flat_map(|s| s.to_le_bytes()).collect();
        let out = temp("again.flac");
        from_wav(&mut std::io::Cursor::new(&body), &out).unwrap();
        let wav = temp("again.wav");
        let mut file = crate::meeting_record::wav_header(body.len() as u32).to_vec();
        file.extend(&body);
        std::fs::write(&wav, file).unwrap();
        // What "transcribe again" and the word times read is the same from either file.
        let packed = crate::file_transcribe::decode(&out).unwrap();
        assert_eq!(packed, crate::file_transcribe::decode(&wav).unwrap());
        assert!(packed.len() > samples.len() - 100);
        std::fs::remove_file(&out).unwrap();
        std::fs::remove_file(&wav).unwrap();
    }

    #[test]
    fn an_empty_recording_is_not_made_into_a_file() {
        let out = temp("empty.flac");
        assert!(from_wav(&mut std::io::empty(), &out).is_err());
        assert!(!out.exists());
    }
}
