//! Repairing a finished take's transcript before it reaches delivery.
//!
//! One trait, resolved once at daemon start, mirroring `src/stt.rs`'s shape.
//! `"off"` is not represented here at all — `Resolution::Off` short-circuits
//! before any engine is built. `"agent"` and any unrecognised value fold into
//! `Resolution::Unavailable`, the same "no engine available" path a live
//! `http`/`command` failure takes — this issue does not invoke the agent
//! engine (out of scope; see `tasks/36/AC_36.md`, "Resolved during S1's
//! gate"). See `tasks/36/DESIGN_36.md`, sections 1 and 5.

pub mod command;
pub mod http;

pub trait Engine: Send + Sync {
    fn rewrite(&self, transcript: &str, bias: &str) -> Result<String, EngineError>;
}

#[derive(Debug)]
pub enum EngineError {
    Command(command::CommandError),
    Http(http::HttpError),
}

impl std::fmt::Display for EngineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EngineError::Command(e) => write!(f, "{e}"),
            EngineError::Http(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for EngineError {}

impl Engine for command::CommandEngine {
    fn rewrite(&self, transcript: &str, bias: &str) -> Result<String, EngineError> {
        self.rewrite(transcript, bias).map_err(EngineError::Command)
    }
}

impl Engine for http::HttpEngine {
    fn rewrite(&self, transcript: &str, bias: &str) -> Result<String, EngineError> {
        self.rewrite(transcript, bias).map_err(EngineError::Http)
    }
}

pub enum Resolution {
    /// `[rewrite] engine = "off"`. The step is not invoked at all — not a
    /// failure, and never produces the tell-once notice.
    Off,
    /// `"http"` or `"command"`, successfully resolved at start.
    Engine(Box<dyn Engine + Send + Sync>),
    /// `"agent"` (out of this issue's scope), an unknown value, or a
    /// configured `http`/`command` engine that failed to resolve at start
    /// (empty url, empty command list). Carries what to tell the person,
    /// once.
    Unavailable(String),
}

// A manual Debug impl, not a derive: `Engine` carries no `Debug` bound (it
// mirrors `stt::Engine`, which has none either), so `Box<dyn Engine + Send +
// Sync>` cannot derive it, and `#[derive(Debug)]` on `Resolution` would fail
// to compile for exactly that reason. This module's own tests print a
// `Resolution` in a panic message (`{other:?}`), so something has to exist —
// the `Engine` variant renders as a placeholder, never its contents.
impl std::fmt::Debug for Resolution {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Resolution::Off => write!(f, "Off"),
            Resolution::Engine(_) => write!(f, "Engine(..)"),
            Resolution::Unavailable(why) => write!(f, "Unavailable({why:?})"),
        }
    }
}

pub fn resolve(rewrite: &crate::config::Rewrite) -> Resolution {
    match rewrite.engine.as_str() {
        "off" => Resolution::Off,
        "agent" => Resolution::Unavailable(
            "[rewrite] engine = \"agent\" is not invoked by this build; the transcript is \
             delivered unrewritten. Set [rewrite] engine to \"http\" or \"command\" for a \
             working engine, or \"off\" to silence this."
                .to_string(),
        ),
        "http" if rewrite.url.is_empty() => Resolution::Unavailable(
            "[rewrite] engine = \"http\" but [rewrite] url is empty, so there is nothing to \
             post to."
                .to_string(),
        ),
        "http" => Resolution::Engine(Box::new(http::HttpEngine::new(
            rewrite.url.clone(),
            rewrite.token.clone(),
            rewrite.model.clone(),
        ))),
        "command" if rewrite.command.is_empty() => Resolution::Unavailable(
            "[rewrite] engine = \"command\" but [rewrite] command is empty, so there is \
             nothing to run."
                .to_string(),
        ),
        "command" => Resolution::Engine(Box::new(command::CommandEngine::new(
            rewrite.command.clone(),
        ))),
        other => Resolution::Unavailable(format!(
            "unknown [rewrite] engine {other:?}; it is one of \"agent\", \"http\", \
             \"command\" or \"off\""
        )),
    }
}

#[cfg(test)]
pub mod tests_support {
    use super::{Engine, EngineError};

    /// An engine that says whatever it was told to say.
    pub struct Fake(pub Result<String, String>);

    impl Engine for Fake {
        fn rewrite(&self, _transcript: &str, _bias: &str) -> Result<String, EngineError> {
            match &self.0 {
                Ok(text) => Ok(text.clone()),
                Err(why) => Err(EngineError::Command(
                    crate::rewrite::command::CommandError::Silent {
                        program: why.clone(),
                    },
                )),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn off_resolves_to_off() {
        let config = crate::config::Rewrite {
            engine: "off".to_string(),
            ..Default::default()
        };
        assert!(matches!(resolve(&config), Resolution::Off));
    }

    #[test]
    fn agent_resolves_to_unavailable() {
        let config = crate::config::Rewrite {
            engine: "agent".to_string(),
            ..Default::default()
        };
        match resolve(&config) {
            Resolution::Unavailable(why) => assert!(why.contains("agent"), "got {why:?}"),
            other => panic!("expected Unavailable, got {other:?}"),
        }
    }

    #[test]
    fn an_unknown_engine_resolves_to_unavailable_naming_the_value() {
        let config = crate::config::Rewrite {
            engine: "vosk".to_string(),
            ..Default::default()
        };
        match resolve(&config) {
            Resolution::Unavailable(why) => assert!(why.contains("vosk"), "got {why:?}"),
            other => panic!("expected Unavailable, got {other:?}"),
        }
    }

    #[test]
    fn http_with_an_empty_url_resolves_to_unavailable() {
        let config = crate::config::Rewrite {
            engine: "http".to_string(),
            ..Default::default()
        };
        assert!(matches!(resolve(&config), Resolution::Unavailable(_)));
    }

    #[test]
    fn http_with_a_url_resolves_to_an_engine() {
        let config = crate::config::Rewrite {
            engine: "http".to_string(),
            url: "http://127.0.0.1:1234/v1/chat/completions".to_string(),
            ..Default::default()
        };
        assert!(matches!(resolve(&config), Resolution::Engine(_)));
    }

    #[test]
    fn command_with_an_empty_list_resolves_to_unavailable() {
        let config = crate::config::Rewrite {
            engine: "command".to_string(),
            ..Default::default()
        };
        assert!(matches!(resolve(&config), Resolution::Unavailable(_)));
    }

    #[test]
    fn command_with_a_program_resolves_to_an_engine() {
        let config = crate::config::Rewrite {
            engine: "command".to_string(),
            command: vec!["echo".to_string()],
            ..Default::default()
        };
        assert!(matches!(resolve(&config), Resolution::Engine(_)));
    }
}
