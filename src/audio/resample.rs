//! From what the device gives to what recognition needs: 16 kHz, one channel.
//!
//! No input device on the development machine offers 16 kHz — they offer 48, and
//! the built-in microphone adds 44.1, 88.2 and 96 (`docs/evidence.md`). The
//! prototype never met this because `ffmpeg` resampled for it.
//!
//! Only whole ratios are handled, which turns a rational resampler into a filter
//! and a stride. A rate that is not a multiple of 16 kHz is refused by name rather
//! than resampled badly. See `tasks/8/DESIGN_8.md`, section 3.

use std::fmt;

/// What recognition reads.
pub const TARGET_RATE: u32 = 16_000;

/// Where the low-pass starts to cut, below the 8 kHz the target rate can carry.
const CUTOFF_HZ: f32 = 7_500.0;

/// Taps in the filter. Odd, so the delay is a whole number of samples.
const TAPS: usize = 127;

#[derive(Debug)]
pub enum ResampleError {
    UnsupportedRate(u32),
}

impl fmt::Display for ResampleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ResampleError::UnsupportedRate(rate) => write!(
                f,
                "the device offers {rate} Hz, which is not a whole multiple of {TARGET_RATE} Hz; \
                 this build records at 32000, 48000, 64000 or 96000 Hz"
            ),
        }
    }
}

impl std::error::Error for ResampleError {}

/// Interleaved channels averaged into one.
pub fn to_mono(samples: &[f32], channels: u16) -> Vec<f32> {
    if channels <= 1 {
        return samples.to_vec();
    }
    let channels = usize::from(channels);
    samples
        .chunks_exact(channels)
        .map(|frame| frame.iter().sum::<f32>() / channels as f32)
        .collect()
}

/// The whole ratio from a device rate to the target, if there is one.
pub fn ratio_for(rate: u32) -> Option<u32> {
    if rate < TARGET_RATE || rate % TARGET_RATE != 0 {
        return None;
    }
    Some(rate / TARGET_RATE)
}

/// A windowed-sinc low-pass, Hamming window, normalised to unit gain at zero.
fn low_pass(rate: u32) -> Vec<f32> {
    let middle = (TAPS - 1) as f32 / 2.0;
    let normalised_cutoff = CUTOFF_HZ / rate as f32; // cycles per sample
    let mut taps: Vec<f32> = (0..TAPS)
        .map(|i| {
            let offset = i as f32 - middle;
            let sinc = if offset == 0.0 {
                2.0 * normalised_cutoff
            } else {
                let x = std::f32::consts::TAU * normalised_cutoff * offset;
                (2.0 * normalised_cutoff) * (x.sin() / x)
            };
            let window = 0.54 - 0.46 * (std::f32::consts::TAU * i as f32 / (TAPS - 1) as f32).cos();
            sinc * window
        })
        .collect();
    let sum: f32 = taps.iter().sum();
    if sum != 0.0 {
        for tap in &mut taps {
            *tap /= sum;
        }
    }
    taps
}

/// Mono samples at a device rate, as mono samples at 16 kHz.
///
/// The filter is not optional. Without it everything above 8 kHz folds back into
/// the band — a 12 kHz tone at 48 kHz reappears at 4 kHz at full amplitude — and
/// that noise goes straight into recognition.
pub fn to_16k(samples: &[f32], rate: u32) -> Result<Vec<f32>, ResampleError> {
    let ratio = ratio_for(rate).ok_or(ResampleError::UnsupportedRate(rate))? as usize;
    if samples.is_empty() {
        return Ok(Vec::new());
    }
    if ratio == 1 {
        return Ok(samples.to_vec());
    }

    let taps = low_pass(rate);
    let middle = (TAPS - 1) / 2;
    let out_len = samples.len() / ratio;
    let mut out = Vec::with_capacity(out_len);
    for k in 0..out_len {
        let centre = k * ratio;
        let mut sum = 0.0;
        for (j, tap) in taps.iter().enumerate() {
            // Outside the take is silence, which is what a take is surrounded by.
            let index = centre + middle;
            if index >= j {
                if let Some(sample) = samples.get(index - j) {
                    sum += tap * sample;
                }
            }
        }
        out.push(sum);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(hz: f32, rate: u32, seconds: f32) -> Vec<f32> {
        let count = (rate as f32 * seconds) as usize;
        (0..count)
            .map(|i| (i as f32 * std::f32::consts::TAU * hz / rate as f32).sin())
            .collect()
    }

    fn rms(samples: &[f32]) -> f32 {
        if samples.is_empty() {
            return 0.0;
        }
        (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt()
    }

    #[test]
    fn two_channels_average_into_one() {
        let interleaved = [1.0, 0.0, 0.5, -0.5, -1.0, 1.0];
        assert_eq!(to_mono(&interleaved, 2), vec![0.5, 0.0, 0.0]);
    }

    #[test]
    fn one_channel_passes_through_untouched() {
        let samples = [0.1, -0.2, 0.3];
        assert_eq!(to_mono(&samples, 1), samples.to_vec());
    }

    #[test]
    fn only_whole_multiples_of_sixteen_kilohertz_are_accepted() {
        assert_eq!(ratio_for(16_000), Some(1));
        assert_eq!(ratio_for(32_000), Some(2));
        assert_eq!(ratio_for(48_000), Some(3));
        assert_eq!(ratio_for(64_000), Some(4));
        assert_eq!(ratio_for(96_000), Some(6));
        assert_eq!(ratio_for(44_100), None);
        assert_eq!(ratio_for(8_000), None);
    }

    #[test]
    fn a_rate_that_is_not_a_multiple_is_refused_by_name() {
        let error = to_16k(&tone(1_000.0, 44_100, 0.1), 44_100).expect_err("must refuse");
        let message = error.to_string();
        assert!(message.contains("44100"), "got {message}");
        assert!(message.contains("16000"), "got {message}");
    }

    #[test]
    fn speech_range_audio_survives_the_conversion() {
        let input = tone(1_000.0, 48_000, 1.0);
        let output = to_16k(&input, 48_000).expect("convert");
        assert_eq!(output.len(), 16_000, "one second stays one second");
        let ratio = rms(&output) / rms(&input);
        assert!(
            (ratio - 1.0).abs() < 0.1,
            "a 1 kHz tone must come through at about its own level, ratio was {ratio}"
        );
    }

    #[test]
    fn audio_above_half_the_target_rate_is_attenuated_rather_than_folded_down() {
        // This is the test the low-pass exists for. Without a filter a 12 kHz tone
        // at 48 kHz reappears at 4 kHz with its full amplitude, and recognition
        // then hears a whistle that nobody made.
        let input = tone(12_000.0, 48_000, 1.0);
        let output = to_16k(&input, 48_000).expect("convert");
        let ratio = rms(&output) / rms(&input);
        assert!(
            ratio < 0.1,
            "a 12 kHz tone must be attenuated, not folded down; ratio was {ratio}"
        );
    }

    #[test]
    fn a_take_of_nothing_converts_to_nothing() {
        assert_eq!(to_16k(&[], 48_000).expect("convert"), Vec::<f32>::new());
    }
}
