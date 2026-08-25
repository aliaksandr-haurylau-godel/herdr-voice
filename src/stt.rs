//! Turning a take into text.
//!
//! One trait, one method: a take's path in, a transcript out. Everything an engine
//! needs beyond the path — the model, the language, the argument list — is given to
//! it when it is built, because the path is the only thing that changes between
//! takes. See `tasks/13/DESIGN_13.md`, section 1.

pub mod command;
pub mod model;

use std::fmt;
use std::path::Path;

use crate::config::Stt;

pub trait Engine {
    fn transcribe(&self, audio: &Path) -> Result<String, EngineError>;
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

/// The engine the configuration asks for, built and ready, or the reason it is not.
pub fn resolve(stt: &Stt, models: &Path) -> Result<Box<dyn Engine>, EngineError> {
    match stt.engine.as_str() {
        "candle" => Err(EngineError::NotBuilt {
            engine: "candle".to_string(),
            issue: "issue #15",
        }),
        "http" => Err(EngineError::NotBuilt {
            engine: "http".to_string(),
            issue: "issue #16",
        }),
        "command" => {
            if stt.command.is_empty() {
                return Err(EngineError::NotConfigured);
            }
            let model = if wants_our_model(&stt.command) {
                Some(model::locate(models, &stt.model).map_err(EngineError::Model)?)
            } else {
                None
            };
            Ok(Box::new(command::CommandEngine::new(
                stt.command.clone(),
                model,
                stt.language.clone(),
            )))
        }
        other => Err(EngineError::Unknown(other.to_string())),
    }
}

#[cfg(test)]
pub mod tests_support {
    use super::{Engine, EngineError};
    use std::path::Path;

    /// An engine that says whatever it was told to say.
    pub struct Fake(pub Result<String, String>);

    impl Engine for Fake {
        fn transcribe(&self, _audio: &Path) -> Result<String, EngineError> {
            match &self.0 {
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
    fn the_unbuilt_engines_say_so_and_name_the_one_that_works() {
        for (name, issue) in [("candle", "#15"), ("http", "#16")] {
            let error = match resolve(&stt(name, &[]), &nowhere()) {
                Err(error) => error,
                Ok(_) => panic!("{name} must not resolve: it is not built"),
            };
            let message = error.to_string();
            assert!(message.contains(name), "got {message}");
            assert!(message.contains(issue), "got {message}");
            assert!(
                message.contains("command"),
                "it must name what works: {message}"
            );
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
