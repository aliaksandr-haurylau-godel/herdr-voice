//! What is missing, and what to do about it.
//!
//! Five lines in a fixed order. The prototype's worst failure was silence: a parse
//! error produced no output and looked like a hang for a morning, so every line
//! that is not `ok` names the next action. See `tasks/3/DESIGN_3.md`, section 4.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::config::{self, Source};
use crate::stt::{self, model};
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
    /// Nothing in this configuration would ever look for a model: the engine is
    /// not built, or its argument list has no `{model}` placeholder. Distinct from
    /// `Missing`, which would send somebody to download a file nothing would read.
    NotUsed,
}

impl State {
    fn word(&self) -> &'static str {
        match self {
            State::Ok => "ok",
            State::Default => "default",
            State::Missing => "missing",
            State::NotUsed => "unused",
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

/// What `stt::resolve` reports for the configured engine, printed exactly as the
/// daemon would print it — so doctor and the daemon can never disagree.
pub fn engine_finding(stt: &config::Stt, models: &Path) -> Finding {
    match stt::resolve(stt, models) {
        Ok(_) => Finding {
            name: "engine",
            state: State::Ok,
            detail: format!("{:?} is ready", stt.engine),
        },
        Err(e) => Finding {
            name: "engine",
            state: State::Missing,
            detail: e.to_string(),
        },
    }
}

/// Whether this configuration ever asks for a model at all: only `command` with an
/// argument list containing `{model}` does. See `tasks/13/DESIGN_13.md`, section 3.
fn asks_for_a_model(stt: &config::Stt) -> bool {
    stt.engine == "command" && stt::wants_our_model(&stt.command)
}

pub fn model_finding(stt: &config::Stt, models: &Path) -> Finding {
    if !asks_for_a_model(stt) {
        return Finding {
            name: "model",
            state: State::NotUsed,
            detail: format!(
                "[stt] model ({}) is not used by this configuration; nothing in it \
                 asks for one. It would be looked for in {}",
                stt.model,
                models.display()
            ),
        };
    }
    match model::locate(models, &stt.model) {
        Ok(path) => Finding {
            name: "model",
            state: State::Ok,
            detail: format!("{}", path.display()),
        },
        Err(e) => Finding {
            name: "model",
            state: State::Missing,
            detail: e.to_string(),
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
    let mut findings = vec![herdr_finding(), daemon_finding(), config_finding(&loaded)];
    match models_directory() {
        Some(models) => {
            findings.push(engine_finding(&loaded.config.stt, &models));
            findings.push(model_finding(&loaded.config.stt, &models));
        }
        None => {
            let detail = "cannot tell where models live: neither HERDR_PLUGIN_STATE_DIR, \
                           XDG_STATE_HOME nor HOME is set"
                .to_string();
            findings.push(Finding {
                name: "engine",
                state: State::Missing,
                detail: detail.clone(),
            });
            findings.push(Finding {
                name: "model",
                state: State::Missing,
                detail,
            });
        }
    }
    findings.push(rewrite_finding(
        &loaded.config.rewrite.engine,
        &loaded.config.rewrite.agent,
    ));
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

    fn command_stt(argv: &[&str]) -> config::Stt {
        config::Stt {
            engine: "command".to_string(),
            command: argv.iter().map(|s| s.to_string()).collect(),
            ..config::Stt::default()
        }
    }

    fn scratch_models(tag: &str) -> PathBuf {
        let directory =
            std::env::temp_dir().join(format!("doctor-model-{tag}-{}", std::process::id()));
        let models = directory.join("models");
        std::fs::create_dir_all(&models).unwrap();
        models
    }

    fn write_real_model(models: &Path, name: &str) {
        let mut content = vec![0x6C, 0x6D, 0x67, 0x67];
        content.resize(2 * 1024 * 1024, 0u8);
        std::fs::write(models.join(format!("ggml-{name}.bin")), content).unwrap();
    }

    #[test]
    fn the_model_line_is_ok_when_the_shared_check_finds_it() {
        let models = scratch_models("ok");
        write_real_model(&models, "large-v3-turbo");
        let stt = command_stt(&["prog", "-m", "{model}"]);
        let finding = model_finding(&stt, &models);
        assert_eq!(finding.state, State::Ok);
        assert!(finding.detail.contains("large-v3-turbo"), "got {finding:?}");
    }

    #[test]
    fn the_model_line_is_missing_when_the_shared_check_refuses_it() {
        let models = scratch_models("missing");
        let stt = command_stt(&["prog", "-m", "{model}"]);
        let finding = model_finding(&stt, &models);
        assert_eq!(finding.state, State::Missing);
        // The shared check's own message, not a substring rule.
        assert!(
            finding.detail.contains("no speech model"),
            "got {finding:?}"
        );
    }

    #[test]
    fn the_model_line_is_not_used_when_the_engine_is_not_built() {
        let models = scratch_models("not-built");
        for engine in ["candle", "http"] {
            let stt = config::Stt {
                engine: engine.to_string(),
                ..config::Stt::default()
            };
            let finding = model_finding(&stt, &models);
            assert_eq!(finding.state, State::NotUsed, "engine {engine}");
            assert!(finding.detail.contains("[stt] model"), "got {finding:?}");
        }
    }

    #[test]
    fn the_model_line_is_not_used_when_the_argument_list_has_no_placeholder() {
        let models = scratch_models("no-placeholder");
        let stt = command_stt(&["prog", "{audio}"]);
        let finding = model_finding(&stt, &models);
        assert_eq!(finding.state, State::NotUsed);
        assert!(finding.detail.contains("[stt] model"), "got {finding:?}");
    }

    #[test]
    fn no_test_asserts_the_substring_rule_any_more() {
        // A file merely containing the model name must not be found: the shared
        // check requires the exact name, the size floor and the ggml magic bytes.
        let models = scratch_models("substring");
        std::fs::write(
            models.join("old-ggml-large-v3-turbo.bin"),
            vec![0u8; 2 * 1024 * 1024],
        )
        .unwrap();
        let stt = command_stt(&["prog", "-m", "{model}"]);
        assert_eq!(model_finding(&stt, &models).state, State::Missing);
    }

    #[test]
    fn the_engine_line_names_what_resolve_reports_for_each_engine() {
        let models = scratch_models("engine-candle");
        let candle = config::Stt {
            engine: "candle".to_string(),
            ..config::Stt::default()
        };
        let finding = engine_finding(&candle, &models);
        assert_eq!(finding.state, State::Missing);
        assert!(finding.detail.contains("candle"), "got {finding:?}");
        assert!(finding.detail.contains("#15"), "got {finding:?}");

        let models = scratch_models("engine-http");
        let http = config::Stt {
            engine: "http".to_string(),
            ..config::Stt::default()
        };
        let finding = engine_finding(&http, &models);
        assert_eq!(finding.state, State::Missing);
        assert!(finding.detail.contains("http"), "got {finding:?}");
        assert!(finding.detail.contains("#16"), "got {finding:?}");

        let models = scratch_models("engine-command");
        let command = command_stt(&["prog", "{audio}"]);
        let finding = engine_finding(&command, &models);
        assert_eq!(finding.state, State::Ok);

        let models = scratch_models("engine-unknown");
        let unknown = config::Stt {
            engine: "vosk".to_string(),
            ..config::Stt::default()
        };
        let finding = engine_finding(&unknown, &models);
        assert_eq!(finding.state, State::Missing);
        assert!(finding.detail.contains("vosk"), "got {finding:?}");
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
