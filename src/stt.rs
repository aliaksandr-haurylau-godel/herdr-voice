//! Turning a take into text.
//!
//! One trait, one method: a take's path and its bias string in, a transcript out.
//! Everything an engine needs beyond those two — the model, the language, the
//! argument list — is given to it when it is built, because the path and the bias
//! string are the only things that change between takes. See
//! `tasks/13/DESIGN_13.md`, section 1.

pub mod candle;
pub mod catalogue;
pub mod command;
pub mod fetch;
pub mod model;

use std::fmt;
use std::path::{Path, PathBuf};

use crate::config::Stt;

pub trait Engine: Send + Sync {
    fn transcribe(&self, audio: &Path, bias: &str) -> Result<String, EngineError>;
}

#[derive(Debug)]
pub enum EngineError {
    /// Named in the manifest and in the design, and not built in this version.
    NotBuilt {
        engine: String,
        issue: &'static str,
    },
    /// A name that is none of the three.
    Unknown(String),
    /// `command` with nothing to run.
    NotConfigured,
    /// Anything the built-in engine could not do: a model that is not there or
    /// not right, a device, a take of the wrong shape, a decode that failed. The
    /// message is carried whole because each of those already names what to do.
    // Constructed from Task 10 onward, when `check_with` builds the engine.
    #[allow(dead_code)]
    Candle(String),
    Model(model::ModelError),
    Command(command::CommandError),
}

/// The engines this build can be asked for.
const ENGINES: &[&str] = &["candle", "http", "command"];

/// What `[stt] command` might look like, taken from what the prototype ran.
const EXAMPLE: &str = r#"command = ["whisper-cli", "-m", "{model}", "-f", "{audio}", "-l", "{language}", "-np", "-nt"]"#;

impl fmt::Display for EngineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            // No silent substitution: an engine that is not built does not quietly
            // become one that is. Somebody who believes the built-in engine is
            // running while an external one is finds out at the worst moment.
            EngineError::NotBuilt { engine, issue } => write!(
                f,
                "the {engine:?} engine is not built in this version ({issue}); set \
                 [stt] engine = \"command\" and give [stt] command a transcriber, for example:\n  {EXAMPLE}"
            ),
            EngineError::Unknown(name) => write!(
                f,
                "unknown [stt] engine {name:?}; it is one of {}",
                ENGINES
                    .iter()
                    .map(|e| format!("{e:?}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            EngineError::NotConfigured => write!(
                f,
                "[stt] engine is \"command\" but [stt] command is empty, so there is nothing \
                 to run. For example:\n  {EXAMPLE}"
            ),
            EngineError::Candle(why) => write!(f, "{why}"),
            EngineError::Model(e) => write!(f, "{e}"),
            EngineError::Command(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for EngineError {}

/// Whether a command's argument list asks this plugin for a model. One that does
/// not brings its own, and demanding ours would refuse a working setup.
pub fn wants_our_model(argv: &[String]) -> bool {
    argv.iter().any(|argument| argument.contains("{model}"))
}

/// What the configured engine wants from the models directory, and whether it is
/// there. One value, looked up once, read by both `check_with` and `doctor` — the
/// property PR #20 gave `doctor` when it stopped reading a model twice per run.
#[derive(Debug)]
pub enum ModelState {
    /// Nothing in this configuration would ever look for a model: the engine is
    /// `http`, or `command` with no `{model}` placeholder.
    NotUsed,
    /// The ggml file `[stt] command` asks for, unchanged in every respect.
    Ggml(Result<PathBuf, model::ModelError>),
    /// The directory `[stt] engine = "candle"` loads.
    Candle(Result<candle::store::Found, candle::store::StoreError>),
}

/// Everything that can be decided without reading weights.
#[derive(Debug)]
pub enum Ready {
    /// The ggml path when the argument list has a `{model}` placeholder, and
    /// `None` when it brings its own. Carried because `check_with` consumes the
    /// lookup and `resolve_with` builds only from what `check_with` returned.
    ///
    /// No `program` field: nothing reads one. `resolve_with` takes the whole
    /// argument list from the configuration, and giving `doctor` a program name
    /// to print would change the `command` engine's report, which is not this
    /// issue's to change.
    Command { model: Option<PathBuf> },
    Candle {
        device: candle::device::Selection,
        model: candle::store::Found,
    },
}

/// The model this configuration would use, if it asks for one at all. Exposed so
/// that a caller who needs both the engine's readiness and the model's own state
/// — `doctor`, in particular — can locate it once and derive both from that
/// single result, instead of asking twice for the same read.
pub fn locate_configured_model(stt: &Stt, models: &Path) -> ModelState {
    match stt.engine.as_str() {
        "candle" => ModelState::Candle(candle::store::locate(
            models,
            &stt.model,
            catalogue::get(&stt.model),
        )),
        "command" if wants_our_model(&stt.command) => {
            ModelState::Ggml(model::locate(models, &stt.model))
        }
        _ => ModelState::NotUsed,
    }
}

/// The engine the configuration asks for, built and ready, or the reason it is not.
pub fn resolve(stt: &Stt, models: &Path) -> Result<Box<dyn Engine + Send + Sync>, EngineError> {
    resolve_with(stt, locate_configured_model(stt, models))
}

/// Every check the daemon makes before it loads anything, and every check
/// `doctor` makes at all. Takes the lookup by reference so the caller keeps it:
/// `doctor` reports on the model separately from the engine, out of this one
/// value. Reads no weights, which is the whole reason it exists apart from
/// `resolve_with` (`tasks/15/DESIGN_15.md`, section 2c).
pub fn check_with(stt: &Stt, state: &ModelState) -> Result<Ready, EngineError> {
    match stt.engine.as_str() {
        "candle" => match state {
            ModelState::Candle(Ok(found)) => Ok(Ready::Candle {
                device: candle::device::select(),
                model: found.clone(),
            }),
            ModelState::Candle(Err(why)) => Err(EngineError::Candle(why.to_string())),
            // Only reachable if a caller pairs a configuration with a lookup made
            // for a different one, which is a programming error rather than a
            // state a person can reach. Reported, never panicked on.
            other => Err(EngineError::Candle(format!(
                "the model was looked up for a different engine ({other:?}); this is \
                 a defect in the plugin, not in your configuration"
            ))),
        },
        "http" => Err(EngineError::NotBuilt {
            engine: "http".to_string(),
            issue: "issue #16",
        }),
        "command" => {
            if stt.command.is_empty() {
                return Err(EngineError::NotConfigured);
            }
            let model = match state {
                ModelState::Ggml(Ok(path)) => Some(path.clone()),
                ModelState::Ggml(Err(e)) => return Err(EngineError::Model(e.clone())),
                _ => None,
            };
            Ok(Ready::Command { model })
        }
        other => Err(EngineError::Unknown(other.to_string())),
    }
}

/// `check_with`, and then build what it approved. Defined this way on purpose:
/// every error `doctor` prints is produced by the code the daemon runs, and the
/// only thing this adds is the load itself.
pub fn resolve_with(
    stt: &Stt,
    state: ModelState,
) -> Result<Box<dyn Engine + Send + Sync>, EngineError> {
    match check_with(stt, &state)? {
        Ready::Command { model } => Ok(Box::new(command::CommandEngine::new(
            stt.command.clone(),
            model,
            stt.language.clone(),
        ))),
        Ready::Candle { device, model } => Ok(Box::new(candle::CandleEngine::new(
            &model,
            &stt.language,
            &device,
        )?)),
    }
}

#[cfg(test)]
pub mod tests_support {
    use super::{Engine, EngineError};
    use std::path::Path;

    /// An engine that says whatever it was told to say.
    pub struct Fake(pub Result<String, String>);

    impl Engine for Fake {
        fn transcribe(&self, _audio: &Path, _bias: &str) -> Result<String, EngineError> {
            match &self.0 {
                Ok(text) => Ok(text.clone()),
                Err(why) => Err(EngineError::Unknown(why.clone())),
            }
        }
    }

    /// An engine that records the bias string it was called with, so a test can
    /// assert on it, alongside a canned result it returns the way `Fake` does.
    pub struct CapturingFake {
        result: Result<String, String>,
        received_bias: std::sync::Arc<std::sync::Mutex<Option<String>>>,
    }

    impl CapturingFake {
        pub fn new(
            result: Result<String, String>,
        ) -> (Self, std::sync::Arc<std::sync::Mutex<Option<String>>>) {
            let received = std::sync::Arc::new(std::sync::Mutex::new(None));
            (
                CapturingFake {
                    result,
                    received_bias: received.clone(),
                },
                received,
            )
        }
    }

    impl Engine for CapturingFake {
        fn transcribe(&self, _audio: &Path, bias: &str) -> Result<String, EngineError> {
            *self.received_bias.lock().unwrap() = Some(bias.to_string());
            match &self.result {
                Ok(text) => Ok(text.clone()),
                Err(why) => Err(EngineError::Unknown(why.clone())),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stt(engine: &str, command: &[&str]) -> Stt {
        Stt {
            engine: engine.to_string(),
            command: command.iter().map(|s| s.to_string()).collect(),
            ..Stt::default()
        }
    }

    fn nowhere() -> std::path::PathBuf {
        std::env::temp_dir().join(format!("stt-resolve-{}", std::process::id()))
    }

    #[test]
    fn http_is_the_only_engine_still_unbuilt_here() {
        // candle was in this list until this issue. http leaves it in #16.
        let error = match resolve(&stt("http", &[]), &nowhere()) {
            Err(error) => error,
            Ok(_) => panic!("http must not resolve: it is not built"),
        };
        let message = error.to_string();
        assert!(message.contains("http"), "got {message}");
        assert!(message.contains("#16"), "got {message}");
        assert!(
            message.contains("command"),
            "it must name what works: {message}"
        );
    }

    #[test]
    fn candle_no_longer_reports_itself_unbuilt() {
        let error = match resolve(&stt("candle", &[]), &nowhere()) {
            Err(error) => error,
            Ok(_) => panic!("with no model at all it cannot resolve"),
        };
        assert!(
            !matches!(error, EngineError::NotBuilt { .. }),
            "candle is built; it fails for want of a model, not for want of code: {error:?}"
        );
        let message = error.to_string();
        assert!(
            message.contains("large-v3-turbo"),
            "it must name the model: {message}"
        );
        assert!(
            message.contains("model --choose"),
            "it must say what to do: {message}"
        );
    }

    #[test]
    fn the_lookup_answers_for_the_engine_that_is_configured() {
        let models = nowhere();

        // http asks for nothing of ours.
        assert!(matches!(
            locate_configured_model(&stt("http", &[]), &models),
            ModelState::NotUsed
        ));
        // command with no placeholder brings its own.
        assert!(matches!(
            locate_configured_model(&stt("command", &["prog", "{audio}"]), &models),
            ModelState::NotUsed
        ));
        // command with a placeholder asks for the ggml file.
        assert!(matches!(
            locate_configured_model(&stt("command", &["prog", "-m", "{model}"]), &models),
            ModelState::Ggml(Err(_))
        ));
        // candle asks for its own directory — this is what changed.
        assert!(matches!(
            locate_configured_model(&stt("candle", &[]), &models),
            ModelState::Candle(Err(_))
        ));
    }

    #[test]
    fn check_with_approves_a_command_engine_and_keeps_its_model_path() {
        // Ready::Command carries the path because check_with consumed the lookup
        // and resolve_with builds only from what check_with returned.
        let path = std::path::PathBuf::from("/models/ggml-tiny.bin");
        let ready = check_with(
            &stt("command", &["prog", "-m", "{model}"]),
            &ModelState::Ggml(Ok(path.clone())),
        )
        .expect("must approve");
        match ready {
            Ready::Command { model } => assert_eq!(model, Some(path)),
            other => panic!("expected Command, got {other:?}"),
        }
    }

    #[test]
    fn check_with_refuses_an_empty_command_and_an_unknown_engine_as_before() {
        assert!(matches!(
            check_with(&stt("command", &[]), &ModelState::NotUsed),
            Err(EngineError::NotConfigured)
        ));
        assert!(matches!(
            check_with(&stt("vosk", &[]), &ModelState::NotUsed),
            Err(EngineError::Unknown(_))
        ));
    }

    #[test]
    fn check_with_passes_a_store_failure_through_with_its_own_words() {
        let state = ModelState::Candle(Err(candle::store::StoreError::DigestMismatch {
            path: std::path::PathBuf::from("/models/candle/tiny/model.safetensors"),
        }));
        let error = check_with(&stt("candle", &[]), &state).expect_err("must refuse");
        let message = error.to_string();
        assert!(message.contains("digest"), "got {message}");
        assert!(message.contains("model --choose"), "got {message}");
    }

    #[test]
    fn resolve_with_never_disagrees_with_check_with() {
        // The property that makes the split safe. Everything check_with refuses,
        // resolve_with must refuse with the same words — it is defined as
        // check_with plus construction.
        let cases = [
            (stt("command", &[]), ModelState::NotUsed),
            (stt("vosk", &[]), ModelState::NotUsed),
            (stt("http", &[]), ModelState::NotUsed),
            (
                stt("candle", &[]),
                ModelState::Candle(Err(candle::store::StoreError::Missing {
                    dir: std::path::PathBuf::from("/models/candle/tiny"),
                    identifier: "tiny".to_string(),
                    absent: "model.safetensors",
                })),
            ),
        ];
        for (config, state) in cases {
            let checked = check_with(&config, &state).err().map(|e| e.to_string());
            let resolved = resolve_with(&config, state).err().map(|e| e.to_string());
            assert_eq!(checked, resolved, "for engine {:?}", config.engine);
            assert!(checked.is_some(), "for engine {:?}", config.engine);
        }
    }

    #[test]
    fn an_unknown_engine_lists_the_three() {
        let error = match resolve(&stt("vosk", &[]), &nowhere()) {
            Err(error) => error,
            Ok(_) => panic!("an unknown engine must not resolve"),
        };
        let message = error.to_string();
        for name in ENGINES {
            assert!(message.contains(name), "got {message}");
        }
    }

    #[test]
    fn a_command_engine_with_nothing_to_run_names_the_key_and_shows_one() {
        let error = match resolve(&stt("command", &[]), &nowhere()) {
            Err(error) => error,
            Ok(_) => panic!("an empty command must not resolve"),
        };
        let message = error.to_string();
        assert!(message.contains("[stt] command"), "got {message}");
        assert!(
            message.contains("whisper-cli"),
            "an example helps: {message}"
        );
    }

    #[test]
    fn a_command_that_asks_for_our_model_fails_when_there_is_none() {
        let error = match resolve(&stt("command", &["prog", "-m", "{model}"]), &nowhere()) {
            Err(error) => error,
            Ok(_) => panic!("a missing model must not resolve"),
        };
        assert!(matches!(error, EngineError::Model(_)), "got {error:?}");
    }

    #[test]
    fn a_command_that_brings_its_own_model_needs_none_of_ours() {
        assert!(!wants_our_model(&[
            "prog".to_string(),
            "{audio}".to_string()
        ]));
        assert!(resolve(&stt("command", &["prog", "{audio}"]), &nowhere()).is_ok());
    }
}
