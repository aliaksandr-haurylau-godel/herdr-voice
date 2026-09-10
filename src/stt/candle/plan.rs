//! Every decision the decoder makes, with no tensor in any signature.
//!
//! Which frames go into which window, what tokens precede the audio, where a
//! window's text ends and where the next one starts. This is where a wrong
//! answer produces a wrong transcript, and none of it needs weights to test.
//! See `tasks/15/DESIGN_15.md`, sections 1 and 7.

use candle_transformers::models::whisper::{self as whisper, N_FRAMES};

/// A window of mel frames: where it starts, and how much of it is real audio
/// rather than the zero padding that fills a window out to 30 seconds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Window {
    pub start: usize,
    pub len: usize,
}

/// One second. A window with less real audio than this is not decoded: found by
/// running it, a window of 2 real frames and 2 998 of padding returned "[Music]".
pub const MIN_REAL_FRAMES: usize = 100;

/// Each timestamp token is 20 milliseconds and each mel frame is 10, so a
/// timestamp's index doubles into frames.
const FRAMES_PER_TIMESTAMP: usize = 2;

/// How many mel frames a take's samples actually make, before `pcm_to_mel` pads
/// them out.
///
/// This exists as its own function because getting it wrong is invisible:
/// `pcm_to_mel` rounds the frame count up to a multiple of 1 500 and then adds
/// 1 500 more, so a spectrogram is always 15 to 30 seconds longer than its
/// audio. Planning windows against the spectrogram's length instead of this put
/// a window of 2 real frames and 1 500 of silence through the model, and it came
/// back "[BLANK_AUDIO]".
pub fn real_frames(samples: usize) -> usize {
    samples / whisper::HOP_LENGTH
}

/// The next window, or `None` when what is left is padding.
pub fn next_window(frames: usize, seek: usize) -> Option<Window> {
    if seek >= frames {
        return None;
    }
    let len = (frames - seek).min(N_FRAMES);
    if len < MIN_REAL_FRAMES {
        return None;
    }
    Some(Window { start: seek, len })
}

/// How far to move after a window. A full window moves to the last timestamp it
/// produced, which is what keeps a word from being cut at the boundary; anything
/// else moves past its own end, because a timestamp inside padding is about
/// silence and a backward or zero advance would decode the same audio forever.
pub fn advance(window: &Window, last_timestamp: Option<u32>, ts_begin: u32) -> usize {
    if window.len < N_FRAMES {
        return window.len;
    }
    match last_timestamp {
        // `token > ts_begin` is what rules out a zero advance: the timestamp at
        // ts_begin itself is 0 s, and honouring it would decode the same window
        // forever. A separate `frames == 0` check was here and is not — it can
        // only be true when this guard is already false, and a mutation test
        // showed it made the guard untestable by covering for it.
        Some(token) if token > ts_begin => {
            let frames = (token - ts_begin) as usize * FRAMES_PER_TIMESTAMP;
            if frames > N_FRAMES {
                N_FRAMES
            } else {
                frames
            }
        }
        _ => N_FRAMES,
    }
}

/// The initial prompt: the marker, then the last `limit` tokens of the bias.
/// The end survives the cut because `bias::collect` puts the conversation there
/// and the conversation is what carries the technical terms.
pub fn prompt_tokens(start_of_prev: u32, bias: &[u32], limit: usize) -> Vec<u32> {
    if bias.is_empty() || limit == 0 {
        return Vec::new();
    }
    let keep = bias.len().min(limit);
    let mut out = Vec::with_capacity(keep + 1);
    out.push(start_of_prev);
    out.extend_from_slice(&bias[bias.len() - keep..]);
    out
}

/// The tokens that are words, with the timestamps taken out.
pub fn text_tokens(body: &[u32], ts_begin: u32) -> Vec<u32> {
    body.iter().copied().filter(|&t| t < ts_begin).collect()
}

/// The last timestamp a window produced, which is where the next one starts.
pub fn last_timestamp(body: &[u32], ts_begin: u32) -> Option<u32> {
    body.iter().rev().copied().find(|&t| t >= ts_begin)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_real_frame_count_is_the_audio_and_not_the_padding() {
        // pcm_to_mel rounds up to a multiple of 1500 frames and adds 1500 more,
        // so a 30-second take makes a 4500-frame spectrogram holding 3000 frames
        // of audio. Planning against 4500 decodes a window of pure silence, and
        // the model obligingly transcribes it.
        assert_eq!(real_frames(480_000), 3_000, "30 s at 16 kHz");
        assert_eq!(real_frames(176_000), 1_100, "11 s at 16 kHz");
        assert_eq!(real_frames(0), 0);

        // And the guard only means something when it is given this number: with
        // the padded 4500 a 30-second take yields a second window, with 3000 it
        // does not.
        assert!(
            next_window(4_500, 2_998).is_some(),
            "the padded count decodes silence"
        );
        assert!(next_window(3_000, 2_998).is_none(), "the real count stops");
    }

    #[test]
    fn a_take_shorter_than_a_window_is_one_window() {
        let w = next_window(1_500, 0).expect("some window");
        assert_eq!(w.start, 0);
        assert_eq!(w.len, 1_500, "the real audio, not the padded length");
    }

    #[test]
    fn a_long_take_is_cut_at_the_window_length() {
        let w = next_window(9_000, 0).expect("some window");
        assert_eq!(w.len, N_FRAMES);
        let w = next_window(9_000, 6_000).expect("some window");
        assert_eq!(w.start, 6_000);
        assert_eq!(w.len, N_FRAMES);
    }

    #[test]
    fn a_tail_that_is_almost_all_padding_is_not_a_window() {
        // Found by running it: a final window holding 2 frames of real audio and
        // 2998 of zeros returned "[Music]" — confident text transcribed from
        // silence. The take ends at the guard instead.
        assert!(
            next_window(9_000, 8_998).is_none(),
            "2 frames is not a window"
        );
        assert!(next_window(9_000, 9_000 - (MIN_REAL_FRAMES - 1)).is_none());
        assert!(
            next_window(9_000, 9_000 - MIN_REAL_FRAMES).is_some(),
            "exactly the guard is enough"
        );
    }

    #[test]
    fn a_seek_at_or_past_the_end_is_the_end() {
        assert!(next_window(3_000, 3_000).is_none());
        assert!(next_window(3_000, 4_000).is_none());
    }

    #[test]
    fn a_full_window_advances_to_its_last_timestamp() {
        // Measured on a 66-second sample: window 1 advanced 2998 frames, not
        // 3000, and the boundary read continuously.
        let w = Window {
            start: 0,
            len: N_FRAMES,
        };
        let ts_begin = 50_364u32;
        // Each timestamp token is 20 ms, and a frame is 10 ms.
        let at_29_98s = ts_begin + 1_499;
        assert_eq!(advance(&w, Some(at_29_98s), ts_begin), 2_998);
    }

    #[test]
    fn a_window_with_no_timestamp_advances_by_its_whole_length() {
        let w = Window {
            start: 0,
            len: N_FRAMES,
        };
        assert_eq!(advance(&w, None, 50_364), N_FRAMES);
    }

    #[test]
    fn a_final_short_window_advances_past_the_end_whatever_it_said() {
        // Otherwise a timestamp inside the padding sends the seek backwards and
        // the same audio is transcribed forever.
        let w = Window {
            start: 6_000,
            len: 900,
        };
        assert_eq!(advance(&w, Some(50_364 + 10), 50_364), 900);
    }

    #[test]
    fn a_zero_or_backward_timestamp_cannot_stall_the_loop() {
        let w = Window {
            start: 0,
            len: N_FRAMES,
        };
        let ts_begin = 50_364u32;
        assert_eq!(
            advance(&w, Some(ts_begin), ts_begin),
            N_FRAMES,
            "0 s would never advance"
        );
        assert_eq!(
            advance(&w, Some(ts_begin + 10_000), ts_begin),
            N_FRAMES,
            "past the window"
        );
    }

    #[test]
    fn the_prompt_is_the_marker_then_the_end_of_the_bias() {
        let ids: Vec<u32> = (1..=10).collect();
        assert_eq!(prompt_tokens(999, &ids, 4), vec![999, 7, 8, 9, 10]);
    }

    #[test]
    fn the_end_of_the_bias_is_what_survives_the_cut() {
        // bias::collect puts the conversation at the end, and the conversation is
        // what carries the terms. Cutting the head would drop exactly that.
        let ids: Vec<u32> = (1..=300).collect();
        let out = prompt_tokens(999, &ids, 224);
        assert_eq!(out.len(), 225);
        assert_eq!(out[0], 999);
        assert_eq!(*out.last().unwrap(), 300);
    }

    #[test]
    fn an_empty_bias_is_no_prompt_at_all() {
        // Not a bare marker: a <|startofprev|> with nothing after it is a prompt
        // the model has to make sense of.
        assert!(prompt_tokens(999, &[], 224).is_empty());
    }

    #[test]
    fn timestamps_are_stripped_from_the_text_and_read_for_the_advance() {
        let ts_begin = 50_364u32;
        let body = vec![ts_begin, 11, 12, ts_begin + 40, 13, ts_begin + 90];
        assert_eq!(text_tokens(&body, ts_begin), vec![11, 12, 13]);
        assert_eq!(last_timestamp(&body, ts_begin), Some(ts_begin + 90));
    }

    #[test]
    fn a_window_that_produced_no_timestamp_says_so() {
        assert_eq!(last_timestamp(&[11, 12, 13], 50_364), None);
        assert_eq!(last_timestamp(&[], 50_364), None);
    }
}
