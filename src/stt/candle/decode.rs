//! The greedy decoding loop. Every decision it makes comes from `plan`; what is
//! left here is the encoder pass, an argmax and the tokenizer.
//!
//! Greedy at temperature zero, no beam search and no temperature fallback. What
//! the fallback buys — a retry when the output degenerates into repetition — is
//! not free to omit, and `tasks/15/DESIGN_15.md` section 10 records what that
//! leaves open. See section 7 for the rest.

use candle_core::{IndexOp, Tensor, D};
use candle_transformers::models::whisper::{self as whisper, model::Whisper, Config, N_FRAMES};
use tokenizers::Tokenizer;

use super::plan;

/// The 99 languages Whisper's multilingual models know, as the tokenizer spells
/// them. Used for `[stt] language = "auto"` and to refuse an unknown value by
/// name. Taken from the model card's own list.
pub const LANGUAGES: [&str; 99] = [
    "en", "zh", "de", "es", "ru", "ko", "fr", "ja", "pt", "tr", "pl", "ca", "nl", "ar", "sv", "it",
    "id", "hi", "fi", "vi", "he", "uk", "el", "ms", "cs", "ro", "da", "hu", "ta", "no", "th", "ur",
    "hr", "bg", "lt", "la", "mi", "ml", "cy", "sk", "te", "fa", "lv", "bn", "sr", "az", "sl", "kn",
    "et", "mk", "br", "eu", "is", "hy", "ne", "mn", "bs", "kk", "sq", "sw", "gl", "mr", "pa", "si",
    "km", "sn", "yo", "so", "af", "oc", "ka", "be", "tg", "sd", "gu", "am", "yi", "lo", "uz", "fo",
    "ht", "ps", "tk", "nn", "mt", "sa", "lb", "my", "bo", "tl", "mg", "as", "tt", "haw", "ln",
    "ha", "ba", "jw", "su",
];

/// The special tokens a decode needs, looked up once.
pub struct Tokens {
    pub sot: u32,
    pub eot: u32,
    pub transcribe: u32,
    pub start_of_prev: u32,
    /// The first timestamp token: everything at or above it is a time, not a
    /// word. Derived from `<|notimestamps|>`, which is not kept: nothing reads
    /// it, because this engine always decodes with timestamps on.
    pub ts_begin: u32,
}

impl Tokens {
    pub fn look_up(tokenizer: &Tokenizer) -> Result<Tokens, String> {
        let id = |name: &str| {
            tokenizer.token_to_id(name).ok_or_else(|| {
                format!("the tokenizer has no {name} token, so this is not a Whisper tokenizer")
            })
        };
        let no_timestamps = id(whisper::NO_TIMESTAMPS_TOKEN)?;
        Ok(Tokens {
            sot: id(whisper::SOT_TOKEN)?,
            eot: id(whisper::EOT_TOKEN)?,
            transcribe: id(whisper::TRANSCRIBE_TOKEN)?,
            start_of_prev: id("<|startofprev|>")?,
            ts_begin: no_timestamps + 1,
        })
    }
}

/// The language token for a configured value, or `None` for `auto`, or an error
/// naming a value the tokenizer does not know.
pub fn language_token(tokenizer: &Tokenizer, language: &str) -> Result<Option<u32>, String> {
    if language.eq_ignore_ascii_case("auto") {
        return Ok(None);
    }
    let name = format!("<|{}|>", language.to_lowercase());
    tokenizer.token_to_id(&name).map(Some).ok_or_else(|| {
        format!(
            "[stt] language is {language:?}, which this model does not know. Use a \
             two-letter code such as \"en\" or \"ru\", or \"auto\" to detect it"
        )
    })
}

/// Which language is being spoken, as a token. One decoder step from the start
/// token, an argmax over the language tokens only. Measured at 6 ms warm, so it
/// runs on the first window and the answer is reused for the rest of the take.
pub fn detect_language(
    model: &mut Whisper,
    tokenizer: &Tokenizer,
    tokens: &Tokens,
    features: &Tensor,
    device: &candle_core::Device,
) -> Result<u32, String> {
    // The 99 names become the ids index_select needs. A model that knows none of
    // them is not multilingual, and saying so beats detecting nothing silently.
    let mut ids = Vec::with_capacity(LANGUAGES.len());
    for code in LANGUAGES.iter() {
        if let Some(id) = tokenizer.token_to_id(&format!("<|{code}|>")) {
            ids.push(id);
        }
    }
    if ids.is_empty() {
        return Err(
            "this model has no language tokens, so [stt] language cannot be \"auto\"; \
             set it to the language you speak, such as \"en\""
                .to_string(),
        );
    }

    model.reset_kv_cache();
    let start = Tensor::new(&[[tokens.sot]], device).map_err(|e| e.to_string())?;
    let hidden = model
        .decoder
        .forward(&start, features, true)
        .map_err(|e| e.to_string())?;
    // The one row of logits for the one position fed in.
    let logits = model
        .decoder
        .final_linear(&hidden.i(..1).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?
        .i(0)
        .map_err(|e| e.to_string())?
        .i(0)
        .map_err(|e| e.to_string())?;
    let wanted = Tensor::new(ids.as_slice(), device).map_err(|e| e.to_string())?;
    let restricted = logits.index_select(&wanted, 0).map_err(|e| e.to_string())?;
    let probabilities = candle_nn::ops::softmax(&restricted, D::Minus1)
        .map_err(|e| e.to_string())?
        .to_vec1::<f32>()
        .map_err(|e| e.to_string())?;
    let best = probabilities
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.total_cmp(b.1))
        .map(|(at, _)| at)
        .unwrap_or(0);
    Ok(ids[best])
}

/// Greedy decoding: the argmax token, appended, until the end token or the cap.
/// Returns only what was generated after `prefix`.
///
/// Temperature zero and no fallback ladder — `tasks/15/DESIGN_15.md` section 10
/// records what that leaves open. The cap is what bounds a degenerate repetition.
fn greedy(
    model: &mut Whisper,
    features: &Tensor,
    prefix: &[u32],
    limit: usize,
    eot: u32,
    device: &candle_core::Device,
) -> Result<Vec<u32>, String> {
    let mut tokens = prefix.to_vec();
    model.reset_kv_cache();
    for step in 0..limit {
        let input = Tensor::new(tokens.as_slice(), device)
            .and_then(|t| t.unsqueeze(0))
            .map_err(|e| e.to_string())?;
        // Flush on the first step only: the cache is being filled from empty.
        let hidden = model
            .decoder
            .forward(&input, features, step == 0)
            .map_err(|e| e.to_string())?;
        let last = hidden.dim(1).map_err(|e| e.to_string())? - 1;
        let logits = model
            .decoder
            .final_linear(&hidden.i((..1, last..)).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?
            .i(0)
            .map_err(|e| e.to_string())?
            .i(0)
            .map_err(|e| e.to_string())?
            .to_vec1::<f32>()
            .map_err(|e| e.to_string())?;
        let next = logits
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.total_cmp(b.1))
            .map(|(at, _)| at as u32)
            .ok_or_else(|| "the model produced no logits".to_string())?;
        if next == eot {
            break;
        }
        tokens.push(next);
    }
    Ok(tokens[prefix.len().min(tokens.len())..].to_vec())
}

/// A whole take: window by window, each conditioned on the bias string and on
/// what the previous window said. Every decision here comes from `plan`.
#[allow(clippy::too_many_arguments)]
pub fn run(
    model: &mut Whisper,
    tokenizer: &Tokenizer,
    tokens: &Tokens,
    mel: &Tensor,
    real_frames: usize,
    bias: &[u32],
    configured_language: Option<u32>,
    config: &Config,
    device: &candle_core::Device,
) -> Result<String, String> {
    let mut language = configured_language;
    let mut seek = 0usize;
    let mut text = String::new();
    let mut previous: Vec<u32> = Vec::new();

    // The spectrogram is longer than the audio: `pcm_to_mel` rounds up to a
    // multiple of 1 500 frames and then adds 1 500 more. Windows are planned
    // against `real_frames` and sliced out of the padded tensor, which is what
    // makes the one-second guard mean what it says. Planning against the padded
    // length instead put a window of 2 real frames and 1 500 of silence through
    // the model, and it came back "[BLANK_AUDIO]" — found by running a take of
    // exactly 30 seconds.
    let padded_frames = mel.dim(2).map_err(|e| e.to_string())?;

    while let Some(window) = plan::next_window(real_frames, seek) {
        // Whisper's encoder takes exactly 30 seconds. Where the spectrogram's own
        // padding reaches that far, use it rather than building more.
        let available = (padded_frames - window.start).min(N_FRAMES);
        let slice = mel
            .narrow(2, window.start, available)
            .map_err(|e| e.to_string())?;
        let padded = if available < N_FRAMES {
            let pad = Tensor::zeros(
                (1, config.num_mel_bins, N_FRAMES - available),
                whisper::DTYPE,
                device,
            )
            .map_err(|e| e.to_string())?;
            Tensor::cat(&[&slice, &pad], 2).map_err(|e| e.to_string())?
        } else {
            slice
        };
        let features = model
            .encoder
            .forward(&padded, true)
            .map_err(|e| e.to_string())?;

        let language_token = match language {
            Some(token) => token,
            None => {
                let detected = detect_language(model, tokenizer, tokens, &features, device)?;
                language = Some(detected);
                detected
            }
        };

        // The bias string first, then what the last window said: the prompt is
        // cut from the front, and the newest context is what must survive.
        let mut carry = bias.to_vec();
        carry.extend_from_slice(&previous);
        // saturating: `max_target_positions` comes from the model's config.json,
        // which is digest-verified for a pinned model and only checked to be a
        // Whisper config for a hand-placed one. A 0 or 1 there would underflow in
        // a debug build, which `CLAUDE.md` counts as a panic path.
        let prompt_cap = (config.max_target_positions / 2).saturating_sub(1);
        let mut prefix = plan::prompt_tokens(tokens.start_of_prev, &carry, prompt_cap);
        // Timestamps on: the advance depends on them. No no_timestamps token.
        prefix.extend_from_slice(&[tokens.sot, language_token, tokens.transcribe]);

        let limit = config.max_target_positions.saturating_sub(prefix.len() + 8);
        let body = greedy(model, &features, &prefix, limit, tokens.eot, device)?;

        let words = plan::text_tokens(&body, tokens.ts_begin);
        let piece = tokenizer.decode(&words, true).map_err(|e| e.to_string())?;
        let piece = piece.trim();
        if !piece.is_empty() {
            if !text.is_empty() {
                text.push(' ');
            }
            text.push_str(piece);
        }
        previous = words;

        seek += plan::advance(
            &window,
            plan::last_timestamp(&body, tokens.ts_begin),
            tokens.ts_begin,
        );
    }
    Ok(text)
}
