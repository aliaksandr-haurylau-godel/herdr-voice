//! A take on disk, in the form the recognition stage reads.
//!
//! 16-bit PCM in a WAV container: what the prototype handed to a Whisper
//! command-line tool, and what those tools take without conversion.

use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

const HEADER_BYTES: u32 = 36;
const BITS_PER_SAMPLE: u16 = 16;
const CHANNELS: u16 = 1;

/// The bytes of a mono WAV file, header and all.
pub fn encode(samples: &[f32], rate: u32) -> Vec<u8> {
    let data_bytes = (samples.len() * 2) as u32;
    let mut out = Vec::with_capacity(44 + data_bytes as usize);

    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(HEADER_BYTES + data_bytes).to_le_bytes());
    out.extend_from_slice(b"WAVE");

    out.extend_from_slice(b"fmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&CHANNELS.to_le_bytes());
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(
        &(rate * u32::from(CHANNELS) * u32::from(BITS_PER_SAMPLE) / 8).to_le_bytes(),
    );
    out.extend_from_slice(&(CHANNELS * BITS_PER_SAMPLE / 8).to_le_bytes());
    out.extend_from_slice(&BITS_PER_SAMPLE.to_le_bytes());

    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_bytes.to_le_bytes());
    for sample in samples {
        // Clamped, not wrapped: a sample past full scale must come out loud, not
        // inverted.
        let scaled = (sample.clamp(-1.0, 1.0) * f32::from(i16::MAX)).round();
        out.extend_from_slice(&(scaled as i16).to_le_bytes());
    }
    out
}

/// Write a take where the next stage can find it.
pub fn write(path: &Path, samples: &[f32], rate: u32) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, encode(samples, rate))
}

// `read` and `ReadError` have no caller until the built-in engine lands
// (`tasks/15/PLAN_15.md`, Task 9), and this is a binary crate, so clippy's
// dead-code lint fires until then. The allowance is on these two items only and
// Task 9 removes it.
#[allow(dead_code)]
/// Why a take could not be read back. Each names what was found, because the
/// only useful thing to say about a file of the wrong shape is what shape it is.
#[derive(Debug)]
pub enum ReadError {
    NotAWav { path: PathBuf },
    NoChunk { path: PathBuf, chunk: &'static str },
    NotPcm { path: PathBuf, format: u16 },
    Channels { path: PathBuf, found: u16 },
    BitDepth { path: PathBuf, found: u16 },
    Io { path: PathBuf, why: String },
}

impl fmt::Display for ReadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ReadError::NotAWav { path } => write!(
                f,
                "{} is not a WAV file; the take is not readable and cannot be \
                 transcribed. Record it again",
                path.display()
            ),
            ReadError::NoChunk { path, chunk } => write!(
                f,
                "{} is a WAV file with no {chunk} chunk, so there is nothing to \
                 transcribe. Record it again",
                path.display()
            ),
            ReadError::NotPcm { path, format } => write!(
                f,
                "{} is WAV format {format}, not uncompressed PCM (1); this engine \
                 reads what this plugin records and converts nothing",
                path.display()
            ),
            ReadError::Channels { path, found } => write!(
                f,
                "{} has {found} channels; the engine reads mono, which is what this \
                 plugin records",
                path.display()
            ),
            ReadError::BitDepth { path, found } => write!(
                f,
                "{} is {found}-bit; the engine reads 16-bit, which is what this \
                 plugin records",
                path.display()
            ),
            ReadError::Io { path, why } => write!(f, "cannot read {}: {why}", path.display()),
        }
    }
}

impl std::error::Error for ReadError {}

#[allow(dead_code)]
/// The samples and the rate a take was written at. The inverse of `encode`, and
/// deliberately no more general than that: `audio::resample` serves capture, and a
/// take that is not the shape this crate writes is a fault to name rather than a
/// conversion to perform.
pub fn read(path: &Path) -> Result<(Vec<f32>, u32), ReadError> {
    let bytes = std::fs::read(path).map_err(|e| ReadError::Io {
        path: path.to_path_buf(),
        why: e.to_string(),
    })?;
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err(ReadError::NotAWav {
            path: path.to_path_buf(),
        });
    }

    let u16_at = |at: usize| u16::from_le_bytes([bytes[at], bytes[at + 1]]);
    let u32_at =
        |at: usize| u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]]);

    let (mut fmt_at, mut data) = (None, None);
    let mut at = 12usize;
    while at + 8 <= bytes.len() {
        let id = &bytes[at..at + 4];
        let size = u32_at(at + 4) as usize;
        let body = at + 8;
        if body + size > bytes.len() {
            // A chunk claiming more than the file holds: truncated. Keep what is
            // actually there rather than refusing a take that is mostly readable.
            if id == b"data" && body <= bytes.len() {
                data = Some((body, bytes.len() - body));
            }
            break;
        }
        if id == b"fmt " && size >= 16 {
            fmt_at = Some(body);
        } else if id == b"data" {
            data = Some((body, size));
        }
        at = body + size + (size & 1);
    }

    let fmt_at = fmt_at.ok_or(ReadError::NoChunk {
        path: path.to_path_buf(),
        chunk: "fmt ",
    })?;
    let format = u16_at(fmt_at);
    if format != 1 {
        return Err(ReadError::NotPcm {
            path: path.to_path_buf(),
            format,
        });
    }
    let channels = u16_at(fmt_at + 2);
    if channels != 1 {
        return Err(ReadError::Channels {
            path: path.to_path_buf(),
            found: channels,
        });
    }
    let rate = u32_at(fmt_at + 4);
    let bits = u16_at(fmt_at + 14);
    if bits != 16 {
        return Err(ReadError::BitDepth {
            path: path.to_path_buf(),
            found: bits,
        });
    }

    let (start, size) = data.ok_or(ReadError::NoChunk {
        path: path.to_path_buf(),
        chunk: "data",
    })?;
    let samples = bytes[start..start + size]
        .chunks_exact(2)
        .map(|c| i16::from_le_bytes([c[0], c[1]]) as f32 / 32_768.0)
        .collect();
    Ok((samples, rate))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read_u32(bytes: &[u8], at: usize) -> u32 {
        u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
    }

    fn read_u16(bytes: &[u8], at: usize) -> u16 {
        u16::from_le_bytes(bytes[at..at + 2].try_into().unwrap())
    }

    #[test]
    fn the_header_says_what_was_asked_for() {
        let bytes = encode(&[0.0, 0.5, -0.5], 16_000);
        assert_eq!(&bytes[0..4], b"RIFF");
        assert_eq!(&bytes[8..12], b"WAVE");
        assert_eq!(&bytes[12..16], b"fmt ");
        assert_eq!(read_u32(&bytes, 16), 16, "a PCM fmt chunk is 16 bytes");
        assert_eq!(read_u16(&bytes, 20), 1, "format 1 is PCM");
        assert_eq!(read_u16(&bytes, 22), 1, "one channel");
        assert_eq!(read_u32(&bytes, 24), 16_000, "the sample rate asked for");
        assert_eq!(read_u32(&bytes, 28), 32_000, "byte rate is rate times two");
        assert_eq!(read_u16(&bytes, 32), 2, "block align is two bytes");
        assert_eq!(read_u16(&bytes, 34), 16, "sixteen bits per sample");
        assert_eq!(&bytes[36..40], b"data");
        assert_eq!(read_u32(&bytes, 40), 6, "three samples of two bytes");
        assert_eq!(read_u32(&bytes, 4), 36 + 6, "the RIFF size covers the rest");
        assert_eq!(bytes.len(), 44 + 6);
    }

    #[test]
    fn samples_outside_the_range_saturate_rather_than_wrap() {
        // Scaling is symmetric, by i16::MAX in both directions, so full scale down
        // is -32767 rather than i16::MIN. The alternative gives the two halves of
        // the scale different factors for the sake of one extra step.
        let bytes = encode(&[2.0, -2.0], 16_000);
        assert_eq!(read_u16(&bytes, 44) as i16, i16::MAX);
        assert_eq!(read_u16(&bytes, 46) as i16, -i16::MAX);
    }

    #[test]
    fn a_written_file_reads_back_as_what_was_written() {
        let path = std::env::temp_dir().join(format!("wav-test-{}.wav", std::process::id()));
        write(&path, &[0.25; 8], 16_000).expect("write");
        let bytes = std::fs::read(&path).expect("read");
        assert_eq!(&bytes[0..4], b"RIFF");
        assert_eq!(read_u32(&bytes, 24), 16_000);
        assert_eq!(read_u32(&bytes, 40), 16);
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn what_encode_writes_read_reads_back() {
        let samples: Vec<f32> = (0..8000).map(|i| ((i as f32) / 40.0).sin() * 0.5).collect();
        let dir = std::env::temp_dir().join(format!("wav-rt-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("take.wav");
        write(&path, &samples, 16_000).unwrap();

        let (back, rate) = read(&path).expect("must read what we wrote");
        assert_eq!(rate, 16_000);
        assert_eq!(back.len(), samples.len());
        // 16-bit quantisation is the only loss.
        for (a, b) in samples.iter().zip(&back) {
            assert!((a - b).abs() < 1.0 / 32_767.0, "{a} vs {b}");
        }
    }

    #[test]
    fn a_file_that_is_not_a_wav_is_named_as_one() {
        let dir = std::env::temp_dir().join(format!("wav-bad-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("nope.wav");
        std::fs::write(&path, b"<html>not audio at all, really not").unwrap();
        let error = read(&path).expect_err("must refuse");
        let message = error.to_string();
        assert!(message.contains("not a WAV"), "got {message}");
    }

    #[test]
    fn stereo_and_the_wrong_width_are_refused_by_name_not_converted() {
        let dir = std::env::temp_dir().join(format!("wav-shape-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();

        let path = dir.join("stereo.wav");
        std::fs::write(&path, fake_wav(2, 16, 16_000)).unwrap();
        let message = read(&path).expect_err("stereo must be refused").to_string();
        assert!(message.contains("mono"), "got {message}");
        assert!(
            message.contains('2'),
            "it must say what it found: {message}"
        );

        let path = dir.join("eight.wav");
        std::fs::write(&path, fake_wav(1, 8, 16_000)).unwrap();
        let message = read(&path).expect_err("8-bit must be refused").to_string();
        assert!(message.contains("16-bit"), "got {message}");
    }

    #[test]
    fn the_rate_is_reported_rather_than_resampled() {
        // read() reports the rate it found; refusing a wrong one is the caller's
        // job, because only the caller knows what it needs.
        let dir = std::env::temp_dir().join(format!("wav-rate-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("r.wav");
        write(&path, &[0.0f32; 100], 44_100).unwrap();
        let (_, rate) = read(&path).expect("a readable file at another rate");
        assert_eq!(rate, 44_100);
    }

    /// A WAV header with the given shape and an empty `data` chunk.
    fn fake_wav(channels: u16, bits: u16, rate: u32) -> Vec<u8> {
        let block_align = channels * bits / 8;
        let byte_rate = rate * block_align as u32;
        let mut v = Vec::new();
        v.extend_from_slice(b"RIFF");
        v.extend_from_slice(&36u32.to_le_bytes());
        v.extend_from_slice(b"WAVEfmt ");
        v.extend_from_slice(&16u32.to_le_bytes());
        v.extend_from_slice(&1u16.to_le_bytes());
        v.extend_from_slice(&channels.to_le_bytes());
        v.extend_from_slice(&rate.to_le_bytes());
        v.extend_from_slice(&byte_rate.to_le_bytes());
        v.extend_from_slice(&block_align.to_le_bytes());
        v.extend_from_slice(&bits.to_le_bytes());
        v.extend_from_slice(b"data");
        v.extend_from_slice(&0u32.to_le_bytes());
        v
    }
}
