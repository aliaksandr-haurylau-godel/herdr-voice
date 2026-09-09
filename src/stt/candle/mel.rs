//! The log-mel spectrogram Whisper's encoder takes, and the filterbank it needs.
//!
//! `candle_transformers`'s `pcm_to_mel` takes the filterbank as a parameter and
//! does not compute one; candle's own example ships it as an opaque binary blob.
//! This computes it from the specification — the Slaney mel scale, triangular
//! filters, area-normalised — and the test compares against that blob, kept as a
//! fixture. See `tasks/15/DESIGN_15.md`, section 6.

use candle_transformers::models::whisper::{self as whisper, N_FFT, SAMPLE_RATE};

/// The Slaney mel scale: linear below 1 kHz, logarithmic above it. Not the HTK
/// formula, which is the other convention and produces a different bank.
fn hz_to_mel(hz: f64) -> f64 {
    let f_sp = 200.0 / 3.0;
    let min_log_hz = 1000.0;
    let min_log_mel = min_log_hz / f_sp;
    let logstep = (6.4f64).ln() / 27.0;
    if hz >= min_log_hz {
        min_log_mel + (hz / min_log_hz).ln() / logstep
    } else {
        hz / f_sp
    }
}

fn mel_to_hz(mel: f64) -> f64 {
    let f_sp = 200.0 / 3.0;
    let min_log_hz = 1000.0;
    let min_log_mel = min_log_hz / f_sp;
    let logstep = (6.4f64).ln() / 27.0;
    if mel >= min_log_mel {
        min_log_hz * (logstep * (mel - min_log_mel)).exp()
    } else {
        f_sp * mel
    }
}

/// `n_mels` triangular filters over the `N_FFT / 2 + 1` rFFT bins, row-major,
/// each normalised by the width of the band it covers.
pub fn filters(n_mels: usize) -> Vec<f32> {
    let rate = SAMPLE_RATE as f64;
    let bins = N_FFT / 2 + 1;
    let bin_hz: Vec<f64> = (0..bins).map(|i| i as f64 * rate / N_FFT as f64).collect();

    let low = hz_to_mel(0.0);
    let high = hz_to_mel(rate / 2.0);
    let edges: Vec<f64> = (0..n_mels + 2)
        .map(|i| mel_to_hz(low + (high - low) * i as f64 / (n_mels + 1) as f64))
        .collect();

    let mut bank = vec![0f32; n_mels * bins];
    for i in 0..n_mels {
        let (left, centre, right) = (edges[i], edges[i + 1], edges[i + 2]);
        let area = 2.0 / (right - left);
        for (j, &hz) in bin_hz.iter().enumerate() {
            let rising = (hz - left) / (centre - left);
            let falling = (right - hz) / (right - centre);
            bank[i * bins + j] = (rising.min(falling).max(0.0) * area) as f32;
        }
    }
    bank
}

/// The log-mel spectrogram of a take, row-major, `n_mels` rows.
pub fn spectrogram(config: &whisper::Config, pcm: &[f32], filters: &[f32]) -> Vec<f32> {
    whisper::audio::pcm_to_mel(config, pcm, filters)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reference(bytes: &[u8]) -> Vec<f32> {
        bytes
            .chunks_exact(4)
            .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect()
    }

    #[test]
    fn the_computed_filterbank_matches_the_reference_one() {
        // Without this, a hand-written filterbank is an assumption rather than a
        // fact, and a wrong one degrades every transcript quietly. Same standard
        // as the SHA-256 in src/stt/model.rs, which is checked against the
        // published vectors.
        for (n_mels, blob) in [
            (
                80usize,
                include_bytes!("testdata/melfilters80.bytes").as_slice(),
            ),
            (
                128usize,
                include_bytes!("testdata/melfilters128.bytes").as_slice(),
            ),
        ] {
            let theirs = reference(blob);
            let mine = filters(n_mels);
            assert_eq!(mine.len(), theirs.len(), "{n_mels} bins: wrong length");
            assert_eq!(mine.len(), n_mels * (N_FFT / 2 + 1));
            let worst = mine
                .iter()
                .zip(&theirs)
                .map(|(a, b)| (a - b).abs())
                .fold(0f32, f32::max);
            assert!(worst < 1e-6, "{n_mels} bins: worst difference {worst:e}");
        }
    }

    #[test]
    fn every_filter_has_weight_somewhere() {
        // A filterbank of zeros would pass a tolerance test against nothing and
        // silently produce a blank spectrogram.
        for n_mels in [80usize, 128] {
            let f = filters(n_mels);
            let bins = N_FFT / 2 + 1;
            for i in 0..n_mels {
                let row: f32 = f[i * bins..(i + 1) * bins].iter().sum();
                assert!(row > 0.0, "{n_mels} bins: filter {i} is empty");
            }
        }
    }

    #[test]
    fn the_scale_is_not_linear() {
        // Guards against hz_to_mel/mel_to_hz being replaced by an identity pair,
        // which would still produce a plausible-looking triangular bank.
        assert!(
            (hz_to_mel(1000.0) - 15.0).abs() < 1e-9,
            "the break point is 1000 Hz"
        );
        assert!(hz_to_mel(8000.0) < 4.0 * hz_to_mel(2000.0));
        assert!(
            (mel_to_hz(hz_to_mel(3000.0)) - 3000.0).abs() < 1e-6,
            "round trip"
        );
    }
}
