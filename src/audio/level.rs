//! How loud a take was.
//!
//! One number per take: the root mean square of the whole thing in decibels
//! relative to full scale, which is the measure the prototype used and the measure
//! the threshold in `docs/evidence.md` was set against. See `tasks/8/DESIGN_8.md`,
//! section 4.

/// What digital silence reports. A true zero would be minus infinity, which is
/// awkward to print, to compare and to put in a message.
pub const SILENCE_FLOOR_DBFS: f32 = -100.0;

/// The mean volume of a take, in decibels relative to full scale.
pub fn mean_dbfs(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return SILENCE_FLOOR_DBFS;
    }
    // Accumulated in f64: a long take is millions of squares, and f32 loses the
    // quiet ones against the loud.
    let sum: f64 = samples.iter().map(|s| f64::from(*s) * f64::from(*s)).sum();
    let rms = (sum / samples.len() as f64).sqrt();
    if rms <= 0.0 {
        return SILENCE_FLOOR_DBFS;
    }
    ((20.0 * rms.log10()) as f32).max(SILENCE_FLOOR_DBFS)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A sine of the given amplitude, one second at 16 kHz.
    fn tone(amplitude: f32) -> Vec<f32> {
        (0..16_000)
            .map(|i| amplitude * (i as f32 * std::f32::consts::TAU * 440.0 / 16_000.0).sin())
            .collect()
    }

    #[test]
    fn digital_silence_reports_the_floor_rather_than_an_infinity() {
        assert_eq!(mean_dbfs(&[0.0; 1000]), SILENCE_FLOOR_DBFS);
        assert_eq!(mean_dbfs(&[]), SILENCE_FLOOR_DBFS);
    }

    #[test]
    fn a_full_scale_tone_is_about_three_decibels_below_full_scale() {
        // The root mean square of a sine at amplitude 1 is 1/sqrt(2).
        let measured = mean_dbfs(&tone(1.0));
        assert!(
            (measured - -3.01).abs() < 0.1,
            "expected about -3.01 dBFS, got {measured}"
        );
    }

    #[test]
    fn a_quiet_tone_falls_below_the_default_threshold() {
        // The threshold this exists to serve is -60 dBFS.
        let measured = mean_dbfs(&tone(0.0005));
        assert!(measured < -60.0, "expected below -60 dBFS, got {measured}");
    }

    #[test]
    fn a_normal_tone_sits_above_the_default_threshold() {
        let measured = mean_dbfs(&tone(0.1));
        assert!(
            measured > -60.0 && measured < 0.0,
            "expected between -60 and 0 dBFS, got {measured}"
        );
    }
}
