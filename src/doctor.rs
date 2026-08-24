//! What is missing, and what to do about it.
//!
//! Five lines in a fixed order. The prototype's worst failure was silence: a parse
//! error produced no output and looked like a hang for a morning, so every line
//! that is not `ok` names the next action. See `tasks/3/DESIGN_3.md`, section 4.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::config::{self, Source};
use crate::transport;
use crate::MIN_HERDR_VERSION;

/// The one agent command-line tool `agent = "auto"` looks for. It is the only one
/// the prototype used (`spike/spike.sh`) and the only one the rewrite measurements
/// in `docs/evidence.md` were made with. A second name goes here when a second
/// tool is measured.
const AGENT_CANDIDATES: &[&str] = &["claude"];

#[derive(Debug, PartialEq, Eq)]
pub enum State {
    Ok,
    Default,
    Missing,
}

impl State {
    fn word(&self) -> &'static str {
        match self {
            State::Ok => "ok",
            State::Default => "default",
            State::Missing => "missing",
        }
    }
}

#[derive(Debug)]
pub struct Finding {
    pub name: &'static str,
    pub state: State,
    pub detail: String,
}

pub fn parse_version(text: &str) -> Option<(u32, u32, u32)> {
    for word in text.split_whitespace() {
        let candidate = word.trim_start_matches('v');
        let numbers: Vec<&str> = candidate.split('.').collect();
        if numbers.len() < 3 {
            continue;
        }
        let Ok(major) = numbers[0].parse() else {
            continue;
        };
        let Ok(minor) = numbers[1].parse() else {
            continue;
        };
        let patch: u32 = numbers[2]
            .chars()
            .take_while(char::is_ascii_digit)
            .collect::<String>()
            .parse()
            .ok()?;
        return Some((major, minor, patch));
    }
    None
}

pub fn at_least(found: &str, required: &str) -> Option<bool> {
    Some(parse_version(found)? >= parse_version(required)?)
}

pub fn render(findings: &[Finding]) -> String {
    let mut out = String::new();
    for finding in findings {
        out.push_str(&format!(
            "{:<8} {:<8} {}\n",
            finding.name,
            finding.state.word(),
            finding.detail
        ));
    }
    out
}

pub fn exit_code(findings: &[Finding]) -> u8 {
    u8::from(findings.iter().any(|f| f.state == State::Missing))
}

fn on_path(program: &str) -> bool {
    Command::new(program)
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok()
}

fn herdr_finding() -> Finding {
    let binary = std::env::var("HERDR_BIN_PATH").unwrap_or_else(|_| "herdr".to_string());
    match Command::new(&binary).arg("--version").output() {
        Err(_) => Finding {
            name: "herdr",
            state: State::Missing,
            detail: format!("cannot run {binary}; install herdr, or set HERDR_BIN_PATH to it"),
        },
        Ok(output) => {
            let printed = String::from_utf8_lossy(&output.stdout).trim().to_string();
            match at_least(&printed, MIN_HERDR_VERSION) {
                Some(true) => Finding {
                    name: "herdr",
                    state: State::Ok,
                    detail: format!("{printed}, this plugin needs {MIN_HERDR_VERSION} or newer"),
                },
                Some(false) => Finding {
                    name: "herdr",
                    state: State::Missing,
                    detail: format!("{printed} is older than {MIN_HERDR_VERSION}; upgrade herdr"),
                },
                None => Finding {
                    name: "herdr",
                    state: State::Missing,
                    detail: format!(
                        "cannot read a version out of {printed:?}; run `{binary} --version`"
                    ),
                },
            }
        }
    }
}

fn daemon_finding() -> Finding {
    match transport::address(&transport::Vars::from_env()) {
        Err(e) => Finding {
            name: "daemon",
            state: State::Missing,
            detail: e.to_string(),
        },
        Ok(address) => {
            if transport::connect(&address).is_ok() {
                Finding {
                    name: "daemon",
                    state: State::Ok,
                    detail: format!("listening at {}", address.display()),
                }
            } else {
                Finding {
                    name: "daemon",
                    state: State::Missing,
                    detail: format!(
                        "nothing is listening at {}; start it with `herdr-voice daemon`, \
                         or restart herdr",
                        address.display()
                    ),
                }
            }
        }
    }
}

pub fn config_finding(loaded: &config::Loaded) -> Finding {
    match &loaded.source {
        Source::File(path) => Finding {
            name: "config",
            state: State::Ok,
            detail: format!("{}", path.display()),
        },
        Source::Defaults(Some(path)) => Finding {
            name: "config",
            state: State::Default,
            detail: format!("no file at {}, defaults used", path.display()),
        },
        Source::Defaults(None) => Finding {
            name: "config",
            state: State::Default,
            detail: "no configuration directory could be derived, defaults used".to_string(),
        },
        Source::Invalid { path, why } => Finding {
            name: "config",
            state: State::Missing,
            detail: format!(
                "{} will not parse ({why}); fix it or delete it",
                path.display()
            ),
        },
    }
}

pub fn model_finding(models: &Path, model: &str) -> Finding {
    let found = std::fs::read_dir(models).ok().and_then(|entries| {
        entries
            .filter_map(Result::ok)
            .find(|entry| entry.file_name().to_string_lossy().contains(model))
    });
    match found {
        Some(entry) => Finding {
            name: "model",
            state: State::Ok,
            detail: format!("{}", entry.path().display()),
        },
        None => Finding {
            name: "model",
            state: State::Missing,
            detail: format!(
                "no file naming {model} in {}; put a speech model there — \
                 the chooser is not built yet",
                models.display()
            ),
        },
    }
}

pub fn rewrite_finding(engine: &str, agent: &str) -> Finding {
    match engine {
        "off" => Finding {
            name: "rewrite",
            state: State::Ok,
            detail: "turned off in the configuration".to_string(),
        },
        "agent" => {
            let candidates: Vec<&str> = if agent == "auto" {
                AGENT_CANDIDATES.to_vec()
            } else {
                vec![agent]
            };
            match candidates.iter().find(|name| on_path(name)) {
                Some(found) => Finding {
                    name: "rewrite",
                    state: State::Ok,
                    detail: format!("found {found:?} in PATH"),
                },
                None => Finding {
                    name: "rewrite",
                    state: State::Missing,
                    detail: format!(
                        "none of {candidates:?} is in PATH; install one, or set \
                         [rewrite] engine = \"off\" to insert transcripts unchanged"
                    ),
                },
            }
        }
        "http" | "command" => Finding {
            name: "rewrite",
            state: State::Missing,
            detail: format!("the {engine:?} engine is not built yet; use \"agent\" or \"off\""),
        },
        other => Finding {
            name: "rewrite",
            state: State::Missing,
            detail: format!(
                "unknown engine {other:?} in [rewrite]; use \"agent\", \"http\", \
                 \"command\" or \"off\""
            ),
        },
    }
}

/// Where a speech model lives. Derived from the state directory rather than from
/// the socket name: on Windows the socket is a pipe name and has no parent
/// directory at all.
fn models_directory() -> Option<PathBuf> {
    transport::state_directory(&transport::Vars::from_env()).map(|state| state.join("models"))
}

pub fn run() -> u8 {
    let loaded = config::load(config::directory(&config::Vars::from_env()).as_deref());
    let findings = vec![
        herdr_finding(),
        daemon_finding(),
        config_finding(&loaded),
        match models_directory() {
            Some(models) => model_finding(&models, &loaded.config.stt.model),
            None => Finding {
                name: "model",
                state: State::Missing,
                detail: "cannot tell where models live: neither HERDR_PLUGIN_STATE_DIR, \
                         XDG_STATE_HOME nor HOME is set"
                    .to_string(),
            },
        },
        rewrite_finding(&loaded.config.rewrite.engine, &loaded.config.rewrite.agent),
    ];
    print!("{}", render(&findings));
    exit_code(&findings)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_version_is_read_out_of_what_herdr_prints() {
        assert_eq!(parse_version("herdr 0.8.2"), Some((0, 8, 2)));
        assert_eq!(parse_version("0.8.2"), Some((0, 8, 2)));
        assert_eq!(parse_version("herdr 1.0.0-rc1"), Some((1, 0, 0)));
        assert_eq!(parse_version("nothing here"), None);
    }

    #[test]
    fn versions_compare_by_three_numbers() {
        assert_eq!(at_least("herdr 0.8.2", "0.8.0"), Some(true));
        assert_eq!(at_least("herdr 0.8.0", "0.8.0"), Some(true));
        assert_eq!(at_least("herdr 0.7.9", "0.8.0"), Some(false));
        assert_eq!(at_least("herdr 1.0.0", "0.8.0"), Some(true));
        assert_eq!(at_least("nonsense", "0.8.0"), None);
    }

    #[test]
    fn every_line_that_is_not_ok_names_a_next_action() {
        let findings = vec![
            Finding {
                name: "herdr",
                state: State::Ok,
                detail: "0.8.2".into(),
            },
            Finding {
                name: "model",
                state: State::Missing,
                detail: "put a speech model in /tmp/models".into(),
            },
        ];
        let text = render(&findings);
        assert!(text.contains("herdr"), "got {text}");
        assert!(text.contains("ok"), "got {text}");
        assert!(text.contains("missing"), "got {text}");
        assert!(text.contains("/tmp/models"), "got {text}");
        assert_eq!(text.lines().count(), 2);
    }

    #[test]
    fn the_exit_code_follows_the_worst_line() {
        let ok = vec![Finding {
            name: "herdr",
            state: State::Ok,
            detail: String::new(),
        }];
        assert_eq!(exit_code(&ok), 0);

        let defaults = vec![Finding {
            name: "config",
            state: State::Default,
            detail: String::new(),
        }];
        assert_eq!(exit_code(&defaults), 0);

        let missing = vec![Finding {
            name: "model",
            state: State::Missing,
            detail: String::new(),
        }];
        assert_eq!(exit_code(&missing), 1);
    }

    #[test]
    fn defaults_are_reported_as_defaults_not_as_a_failure() {
        let directory = std::env::temp_dir().join(format!("doctor-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let finding = config_finding(&crate::config::load(Some(&directory)));
        assert_eq!(finding.state, State::Default);
        assert!(
            finding.detail.contains("defaults"),
            "got {}",
            finding.detail
        );
        assert!(
            finding.detail.contains("config.toml"),
            "got {}",
            finding.detail
        );
    }

    #[test]
    fn a_model_is_present_when_a_file_carries_its_name() {
        let directory = std::env::temp_dir().join(format!("doctor-model-{}", std::process::id()));
        let models = directory.join("models");
        std::fs::create_dir_all(&models).unwrap();
        assert_eq!(
            model_finding(&models, "large-v3-turbo").state,
            State::Missing
        );
        std::fs::write(models.join("ggml-large-v3-turbo.bin"), b"x").unwrap();
        assert_eq!(model_finding(&models, "large-v3-turbo").state, State::Ok);
    }

    #[test]
    fn the_rewrite_engine_is_looked_for_by_name() {
        assert_eq!(
            rewrite_finding("agent", "definitely-not-installed").state,
            State::Missing
        );
        assert_eq!(rewrite_finding("off", "auto").state, State::Ok);
        assert_eq!(
            rewrite_finding("agent", "auto").name,
            "rewrite",
            "auto resolves against the candidate list"
        );
    }
}
