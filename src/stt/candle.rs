//! The built-in engine: a Whisper model running in this process.
//!
//! `candle-transformers` supplies the network. Everything around it lives here,
//! split so that the tensor half is thin and every decision that can make a
//! transcript wrong lives in `plan`, which no test needs weights to reach.
//! See `tasks/15/DESIGN_15.md`, section 1.

pub mod decode;
pub mod device;
pub mod mel;
pub mod plan;
pub mod store;

use std::path::Path;
use std::sync::Mutex;

use candle_core::Tensor;
use candle_nn::VarBuilder;
use candle_transformers::models::whisper::{self as whisper, model::Whisper, Config, N_FRAMES};
use tokenizers::Tokenizer;

use crate::stt::{Engine, EngineError};

/// A Whisper model, resident. The `Mutex` is not for contention — a take is one
/// at a time — but because decoding mutates the model's key-value cache and
/// `Engine::transcribe` takes `&self`.
pub struct CandleEngine {
    model: Mutex<Whisper>,
    tokenizer: Tokenizer,
    tokens: decode::Tokens,
    config: Config,
    filters: Vec<f32>,
    device: candle_core::Device,
    language: Option<u32>,
}

/// Whisper is 80 mel bins, or 128 for the large-v3 models. Anything else is a
/// model this engine cannot run, and saying so beats a filterbank of the wrong
/// shape and a blank transcript.
fn check_mel_bins(bins: usize) -> Result<(), EngineError> {
    if bins == 80 || bins == 128 {
        Ok(())
    } else {
        Err(EngineError::Candle(format!(
            "this model wants {bins} mel bins and this engine builds 80 or 128; it \
             is not a Whisper model this plugin can run. Run `herdr-voice model \
             --choose` to install one that is"
        )))
    }
}

/// Everything about a candle model that can be judged without reading weights:
/// the mel-bin count from `config.json` (two kilobytes) and the form of
/// `[stt] language`.
///
/// It exists so `doctor` and the daemon agree. Both checks used to live only in
/// `CandleEngine::new`, so `doctor` reported `engine ok` for a configuration the
/// daemon then refused — a typo in `[stt] language` was enough. Found by an S4
/// review. What still cannot be judged here is whether `tokenizer.json` is a
/// Whisper tokenizer, which needs the file loaded; `new` checks that, and
/// `doctor`'s comment says as much rather than claiming otherwise.
pub fn precheck(found: &store::Found, language: &str) -> Result<(), EngineError> {
    let config_path = found.dir().config();
    let text = std::fs::read_to_string(&config_path).map_err(|e| {
        EngineError::Candle(format!(
            "cannot read {}: {e}. Run `herdr-voice model --choose` to install the \
             model again",
            config_path.display()
        ))
    })?;
    let config: Config = serde_json::from_str(&text).map_err(|e| {
        EngineError::Candle(format!(
            "{} is not readable JSON ({e}); delete it and run \
             `herdr-voice model --choose` again",
            config_path.display()
        ))
    })?;
    check_mel_bins(config.num_mel_bins)?;
    check_language(language)
}

/// `auto`, or one of the languages Whisper knows. Checked without a tokenizer so
/// `doctor` can check it too.
fn check_language(language: &str) -> Result<(), EngineError> {
    if language.eq_ignore_ascii_case("auto") {
        return Ok(());
    }
    let lowered = language.to_lowercase();
    if decode::LANGUAGES.contains(&lowered.as_str()) {
        return Ok(());
    }
    Err(EngineError::Candle(format!(
        "[stt] language is {language:?}, which Whisper does not know. Use a \
         two-letter code such as \"en\" or \"ru\", or \"auto\" to detect it"
    )))
}

/// A take, as samples, refusing anything that is not what the recorder writes.
fn read_take(path: &Path) -> Result<Vec<f32>, EngineError> {
    let (pcm, rate) =
        crate::audio::wav::read(path).map_err(|e| EngineError::Candle(e.to_string()))?;
    if rate != whisper::SAMPLE_RATE as u32 {
        return Err(EngineError::Candle(format!(
            "the take at {} is {rate} Hz and this engine reads 16000 Hz, which is \
             what this plugin records. The file was not produced by this plugin",
            path.display()
        )));
    }
    Ok(pcm)
}

impl CandleEngine {
    pub fn new(
        found: &store::Found,
        language: &str,
        selection: &device::Selection,
    ) -> Result<CandleEngine, EngineError> {
        // Counted so that `doctor_reads_no_weights` means what it says: this is
        // the one place weights are read.
        #[cfg(test)]
        store::weight_reads::increment();

        let dir = found.dir();
        // Every failure below names the file and what to do, which is the whole of
        // `CLAUDE.md`'s rule applied to a directory of three files.
        let named = |path: &Path, why: String| {
            EngineError::Candle(format!(
                "cannot use the speech model at {}: {why}. Run `herdr-voice model \
                 --choose` to install one again",
                path.display()
            ))
        };

        let config_path = dir.config();
        let config_text = std::fs::read_to_string(&config_path)
            .map_err(|e| named(&config_path, e.to_string()))?;
        let config: Config =
            serde_json::from_str(&config_text).map_err(|e| named(&config_path, e.to_string()))?;
        check_mel_bins(config.num_mel_bins)?;

        let tokenizer_path = dir.tokenizer();
        let tokenizer = Tokenizer::from_file(&tokenizer_path)
            .map_err(|e| named(&tokenizer_path, e.to_string()))?;
        let tokens = decode::Tokens::look_up(&tokenizer).map_err(EngineError::Candle)?;
        let language = decode::language_token(&tokenizer, language).map_err(EngineError::Candle)?;

        let filters = mel::filters(config.num_mel_bins);
        let device = device::device_for(selection);

        let weights_path = dir.weights();
        // Safe here in the sense that matters: the file was verified by
        // `store::locate` before this was called, so it is not a mapping of
        // something arbitrary. A mapped file changed underneath is the residual
        // risk, and it is the same one every mmap-based loader carries.
        let builder = unsafe {
            VarBuilder::from_mmaped_safetensors(
                std::slice::from_ref(&weights_path),
                whisper::DTYPE,
                &device,
            )
        }
        .map_err(|e| named(&weights_path, e.to_string()))?;
        let model = Whisper::load(&builder, config.clone())
            .map_err(|e| named(&weights_path, e.to_string()))?;

        let engine = CandleEngine {
            model: Mutex::new(model),
            tokenizer,
            tokens,
            config,
            filters,
            device,
            language,
        };
        engine.warm();
        Ok(engine)
    }

    /// One encoder pass over 30 seconds of silence, so the first real take does
    /// not pay for it — measured at 0.77-0.84 s for the default model
    /// (`tasks/15/DESIGN_15.md`, section 8).
    ///
    /// Not fatal if it fails: a warm-up is an optimisation, and the take path
    /// reports its own failures with a take in hand to name. Nothing here
    /// unwraps, so a poisoned lock or a device hiccup costs the warm-up and
    /// nothing else.
    fn warm(&self) {
        if let Ok(silence) = Tensor::zeros(
            (1, self.config.num_mel_bins, N_FRAMES),
            whisper::DTYPE,
            &self.device,
        ) {
            if let Ok(mut model) = self.model.lock() {
                let _ = model.encoder.forward(&silence, true);
                model.reset_kv_cache();
            }
        }
    }
}

impl Engine for CandleEngine {
    fn transcribe(&self, audio: &Path, bias: &str) -> Result<String, EngineError> {
        let pcm = read_take(audio)?;
        // How much of the take is audio, before `pcm_to_mel` pads it out. The
        // decoder plans windows against this, not against the spectrogram's
        // length, so that its one-second guard is about audio and not padding.
        let real_frames = plan::real_frames(pcm.len());
        let frames_data = mel::spectrogram(&self.config, &pcm, &self.filters);
        let padded_frames = frames_data.len() / self.config.num_mel_bins;
        let mel = Tensor::from_vec(
            frames_data,
            (1, self.config.num_mel_bins, padded_frames),
            &self.device,
        )
        .map_err(|e| EngineError::Candle(format!("cannot build the spectrogram: {e}")))?;

        let bias_ids: Vec<u32> = if bias.is_empty() {
            Vec::new()
        } else {
            // A bias string that will not tokenise costs the take its terms, not
            // the take: the transcript is still worth having.
            self.tokenizer
                .encode(bias, false)
                .map(|e| e.get_ids().to_vec())
                .unwrap_or_default()
        };

        let mut model = self.model.lock().map_err(|_| {
            EngineError::Candle(
                "the speech model is in an unusable state after an earlier failure; \
                 restart the daemon"
                    .to_string(),
            )
        })?;
        decode::run(
            &mut model,
            &self.tokenizer,
            &self.tokens,
            &mel,
            real_frames,
            &bias_ids,
            self.language,
            &self.config,
            &self.device,
        )
        .map_err(EngineError::Candle)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unsupported_mel_bin_count_is_refused_by_name() {
        // Whisper is 80 or 128. Anything else means a model this engine cannot
        // run, and the message has to say so rather than producing a filterbank
        // of the wrong shape and a blank transcript.
        let error = check_mel_bins(64).expect_err("64 is not a Whisper model");
        let message = error.to_string();
        assert!(message.contains("64"), "got {message}");
        assert!(message.contains("80"), "got {message}");
        assert!(message.contains("128"), "got {message}");
    }

    #[test]
    fn eighty_and_a_hundred_and_twenty_eight_are_accepted() {
        assert!(check_mel_bins(80).is_ok());
        assert!(check_mel_bins(128).is_ok());
    }

    #[test]
    fn a_take_at_the_wrong_rate_is_refused_before_the_model_is_touched() {
        let dir = std::env::temp_dir().join(format!("candle-rate-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("take.wav");
        crate::audio::wav::write(&path, &[0.0f32; 1000], 44_100).unwrap();
        let error = read_take(&path).expect_err("must refuse");
        let message = error.to_string();
        assert!(
            message.contains("44100"),
            "it must say what it found: {message}"
        );
        assert!(message.contains("16000"), "got {message}");
    }

    #[test]
    fn a_take_at_sixteen_kilohertz_is_read() {
        let dir = std::env::temp_dir().join(format!("candle-ok-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("take.wav");
        crate::audio::wav::write(&path, &[0.1f32; 1600], 16_000).unwrap();
        assert_eq!(read_take(&path).expect("must read").len(), 1600);
    }
}

/// A check that drives the real engine over a real model and a real take.
///
/// `#[ignore]`d, so `cargo test` never runs it and the suite keeps passing with
/// no model and no network. It is kept rather than deleted because issue #2 —
/// comparing this engine with whisper.cpp — needs exactly this harness, and
/// because every timing in `docs/evidence.md` for the built-in engine was
/// produced with it. Run it as:
///
/// ```sh
/// LIVE_MODELS=<state>/models LIVE_MODEL=tiny LIVE_WAV=take.wav \
///   cargo test --release candle::live -- --ignored --nocapture
/// ```
///
/// `LIVE_BIAS` sets the bias string, and `LIVE_CPU` forces the CPU so the
/// fallback can be measured on a machine that has a GPU.
#[cfg(test)]
mod live {
    use super::*;

    #[test]
    #[ignore = "needs a downloaded model; see the module comment"]
    fn transcribes_a_real_take() {
        let models = std::path::PathBuf::from(
            std::env::var("LIVE_MODELS").expect("LIVE_MODELS: the models directory"),
        );
        let wav = std::path::PathBuf::from(
            std::env::var("LIVE_WAV").expect("LIVE_WAV: a 16 kHz mono WAV"),
        );
        let identifier = std::env::var("LIVE_MODEL").expect("LIVE_MODEL: a model identifier");
        let bias = std::env::var("LIVE_BIAS").unwrap_or_default();

        let entry = crate::stt::catalogue::get(&identifier);
        let found = store::locate(&models, &identifier, entry).expect("the model must verify");

        let selection = if std::env::var("LIVE_CPU").is_ok() {
            device::Selection::Cpu {
                why: "forced by the live check",
            }
        } else {
            device::select()
        };

        let started = std::time::Instant::now();
        let engine = CandleEngine::new(&found, "auto", &selection).expect("must build");
        eprintln!("LIVE build+warm: {:?}", started.elapsed());

        let started = std::time::Instant::now();
        let text = engine.transcribe(&wav, &bias).expect("must transcribe");
        eprintln!("LIVE transcribe: {:?}", started.elapsed());
        eprintln!("LIVE text: {text}");
        assert!(!text.is_empty(), "the transcript must not be empty");
    }
}
