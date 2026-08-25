//! A take on disk, in the form the recognition stage reads.
//!
//! 16-bit PCM in a WAV container: what the prototype handed to a Whisper
//! command-line tool, and what those tools take without conversion.

use std::io;
use std::path::Path;

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
}
