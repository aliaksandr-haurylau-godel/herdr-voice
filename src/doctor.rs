//! What is missing, and what to do about it.
//!
//! Six lines in a fixed order: herdr, daemon, config, engine, model, rewrite. The
//! prototype's worst failure was silence: a parse error produced no output and
//! looked like a hang for a morning, so every line that is not `ok` names the next
//! action. See `tasks/3/DESIGN_3.md`, section 4.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::config::{self, Source};
use crate::stt;
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

/// What `stt::check_with` reports for the configured engine. Takes a model lookup
/// already performed elsewhere (`engine_and_model_findings`, below) so that
/// reporting on both the engine and the model needs only one `locate` call.
///
/// This is every check the daemon makes that does not need the weights, and
/// `resolve_with` is defined as `check_with` plus construction, so the two agree
/// on everything checkable here. One thing is not: whether `tokenizer.json` is a
/// genuinely Whisper tokenizer, which needs the file loaded and so is checked
/// only when the engine is built. A hand-placed model can therefore be `ok` here
/// and refused by the daemon; a pinned one cannot, because its tokenizer is
/// digest-verified.
fn engine_finding_from(stt: &config::Stt, state: &stt::ModelState) -> Finding {
    // `check_with`, never `resolve_with`: reporting must not load 1.6 GB of
    // weights and run an encoder pass to print six lines
    // (`tasks/15/DESIGN_15.md`, section 2c). `resolve_with` is defined as
    // `check_with` plus construction, so the two can never disagree about what
    // is wrong.
    match stt::check_with(stt, state) {
        Ok(stt::Ready::Candle { device, .. }) => Finding {
            name: "engine",
            state: State::Ok,
            detail: format!(
                "\"candle\" is ready, running {}",
                crate::stt::candle::device::describe(&device)
            ),
        },
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

/// The model line. Takes a model lookup already performed elsewhere
/// (`engine_and_model_findings`, below) for the same reason `engine_finding_from`
/// does: one `locate` call answers both lines, not one each.
fn model_finding_from(stt: &config::Stt, models: &Path, state: &stt::ModelState) -> Finding {
    match state {
        stt::ModelState::NotUsed => Finding {
            name: "model",
            state: State::NotUsed,
            detail: format!(
                "[stt] model ({}) is not used by this configuration; nothing in it \
                 asks for one. It would be looked for in {}",
                stt.model,
                models.display()
            ),
        },
        stt::ModelState::Ggml(Ok(path)) => Finding {
            name: "model",
            state: State::Ok,
            detail: format!("{}", path.display()),
        },
        stt::ModelState::Candle(Ok(found)) => match found {
            crate::stt::candle::store::Found::Verified(dir) => Finding {
                name: "model",
                state: State::Ok,
                detail: format!("{}", dir.dir.display()),
            },
            crate::stt::candle::store::Found::Unpinned { dir, identifier } => Finding {
                name: "model",
                state: State::Ok,
                detail: format!(
                    "{} — this plugin does not offer {identifier}, so it did not check \
                     its size or its digest. Run `herdr-voice model` for the models it \
                     does offer",
                    dir.dir.display()
                ),
            },
        },
        stt::ModelState::Ggml(Err(e)) => Finding {
            name: "model",
            state: State::Missing,
            detail: e.to_string(),
        },
        stt::ModelState::Candle(Err(e)) => Finding {
            name: "model",
            state: State::Missing,
            detail: e.to_string(),
        },
    }
}

/// The engine and model lines together, from one lookup: `doctor` needs both, and a
/// real model file is read and hashed only once for the pair, not once per line. See
/// PR #20's finding on `doctor` reading a multi-gigabyte model twice.
fn engine_and_model_findings(stt: &config::Stt, models: &Path) -> (Finding, Finding) {
    let state = stt::locate_configured_model(stt, models);
    // By reference, not cloned: the one-lookup property is literal rather than
    // nearly true, and a multi-gigabyte model is read and hashed once for the
    // pair.
    let engine = engine_finding_from(stt, &state);
    let model_line = model_finding_from(stt, models, &state);
    (engine, model_line)
}

pub fn rewrite_finding(rewrite: &config::Rewrite) -> Finding {
    match rewrite.engine.as_str() {
        "off" => Finding {
            name: "rewrite",
            state: State::Ok,
            detail: "turned off in the configuration".to_string(),
        },
        "agent" => {
            // The take path treats "agent" as "no engine available" outright
            // (`tasks/36/AC_36.md`, "Resolved during S1's gate") — this build
            // does not invoke it for rewrite, regardless of what is on PATH.
            // Reporting `Ok` here whenever the tool happens to be found would
            // tell somebody the opposite of what a take actually does.
            let candidates: Vec<&str> = if rewrite.agent == "auto" {
                AGENT_CANDIDATES.to_vec()
            } else {
                vec![rewrite.agent.as_str()]
            };
            let detail = match candidates.iter().find(|name| on_path(name)) {
                Some(found) => format!(
                    "{found:?} is on PATH, but this build does not yet invoke the agent \
                     engine for rewrite; transcripts are delivered unrewritten. Set \
                     [rewrite] engine to \"http\" or \"command\" for a working engine, or \
                     \"off\" to silence this."
                ),
                None => format!(
                    "none of {candidates:?} is on PATH, and this build does not yet invoke \
                     the agent engine for rewrite either way; transcripts are delivered \
                     unrewritten. Set [rewrite] engine to \"http\" or \"command\" for a \
                     working engine, or \"off\" to silence this."
                ),
            };
            Finding {
                name: "rewrite",
                state: State::Missing,
                detail,
            }
        }
        "http" => {
            if rewrite.url.is_empty() {
                Finding {
                    name: "rewrite",
                    state: State::Missing,
                    detail: "give [rewrite] a url, for example: url = \
                             \"http://127.0.0.1:1234/v1/chat/completions\""
                        .to_string(),
                }
            } else {
                Finding {
                    name: "rewrite",
                    state: State::Ok,
                    detail: format!("configured to post to {:?}", rewrite.url),
                }
            }
        }
        "command" => {
            if rewrite.command.is_empty() {
                Finding {
                    name: "rewrite",
                    state: State::Missing,
                    detail: "give [rewrite] a command, for example: command = [\"claude\", \
                             \"-p\", \"fix: {transcript}\"]"
                        .to_string(),
                }
            } else {
                Finding {
                    name: "rewrite",
                    state: State::Ok,
                    detail: format!("configured to run {:?}", rewrite.command),
                }
            }
        }
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
            let (engine, model) = engine_and_model_findings(&loaded.config.stt, &models);
            findings.push(engine);
            findings.push(model);
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
    findings.push(rewrite_finding(&loaded.config.rewrite));
    print!("{}", render(&findings));
    exit_code(&findings)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stt::model;

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
        let finding = engine_and_model_findings(&stt, &models).1;
        assert_eq!(finding.state, State::Ok);
        assert!(finding.detail.contains("large-v3-turbo"), "got {finding:?}");
    }

    #[test]
    fn the_model_line_is_missing_when_the_shared_check_refuses_it() {
        let models = scratch_models("missing");
        let stt = command_stt(&["prog", "-m", "{model}"]);
        let finding = engine_and_model_findings(&stt, &models).1;
        assert_eq!(finding.state, State::Missing);
        // The shared check's own message, not a substring rule.
        assert!(
            finding.detail.contains("no speech model"),
            "got {finding:?}"
        );
    }

    #[test]
    fn the_model_line_is_not_used_only_for_engines_that_ask_for_nothing() {
        // candle was in this list until issue #15 built it. It uses a model now,
        // and the assertion that said otherwise had to go rather than be weakened.
        let models = scratch_models("not-built");
        let stt = config::Stt {
            engine: "http".to_string(),
            ..config::Stt::default()
        };
        let finding = engine_and_model_findings(&stt, &models).1;
        assert_eq!(finding.state, State::NotUsed);
        assert!(finding.detail.contains("[stt] model"), "got {finding:?}");
    }

    #[test]
    fn the_candle_model_line_reports_the_directory_it_wants() {
        let models = scratch_models("candle-missing");
        let stt = config::Stt {
            engine: "candle".to_string(),
            ..config::Stt::default()
        };
        let finding = engine_and_model_findings(&stt, &models).1;
        assert_eq!(finding.state, State::Missing, "got {finding:?}");
        assert!(
            finding.detail.contains("large-v3-turbo"),
            "got {}",
            finding.detail
        );
        assert!(
            finding.detail.contains("model --choose"),
            "got {}",
            finding.detail
        );
    }

    #[test]
    fn a_pinned_model_whose_bytes_are_wrong_is_missing_not_ok() {
        // A small fixture under a pinned identifier is exactly the shape of a
        // truncated download, and doctor must say so rather than accept it.
        let models = scratch_models("candle-pinned-wrong");
        write_candle_model(&models, "large-v3-turbo");
        let stt = config::Stt {
            engine: "candle".to_string(),
            ..config::Stt::default()
        };
        let finding = engine_and_model_findings(&stt, &models).1;
        assert_eq!(finding.state, State::Missing, "got {finding:?}");
        assert!(
            finding.detail.contains("1617824864"),
            "it must name the size it wanted: {}",
            finding.detail
        );
    }

    #[test]
    fn an_unpinned_model_is_ok_and_says_which_checks_were_skipped() {
        // The path a model placed by hand takes: usable, and honest about what
        // was not verified.
        let models = scratch_models("candle-unpinned");
        write_candle_model(&models, "homegrown");
        let stt = config::Stt {
            engine: "candle".to_string(),
            model: "homegrown".to_string(),
            ..config::Stt::default()
        };
        let finding = engine_and_model_findings(&stt, &models).1;
        assert_eq!(finding.state, State::Ok, "an unpinned model still works");
        assert!(
            finding.detail.contains("did not check"),
            "it must name the checks it skipped: {}",
            finding.detail
        );
        assert!(
            finding.detail.contains("homegrown"),
            "got {}",
            finding.detail
        );
    }

    #[test]
    fn the_candle_engine_line_names_the_device() {
        let models = scratch_models("candle-device");
        write_candle_model(&models, "homegrown");
        let stt = config::Stt {
            engine: "candle".to_string(),
            model: "homegrown".to_string(),
            ..config::Stt::default()
        };
        let finding = engine_and_model_findings(&stt, &models).0;
        assert_eq!(finding.state, State::Ok, "got {finding:?}");
        let lowered = finding.detail.to_lowercase();
        assert!(
            lowered.contains("metal") || lowered.contains("cpu"),
            "the engine line must say what it runs on: {}",
            finding.detail
        );
    }

    #[test]
    fn a_language_nobody_speaks_is_caught_before_the_daemon_meets_it() {
        // Found by an S4 review: these checks lived only in CandleEngine::new, so
        // doctor said `engine ok` and exited 0 for a configuration the daemon
        // then refused at start. A typo in [stt] language was enough.
        let models = scratch_models("candle-language");
        write_candle_model(&models, "homegrown");
        let stt = config::Stt {
            engine: "candle".to_string(),
            model: "homegrown".to_string(),
            language: "klingon".to_string(),
            ..config::Stt::default()
        };
        let finding = engine_and_model_findings(&stt, &models).0;
        assert_eq!(finding.state, State::Missing, "got {finding:?}");
        assert!(finding.detail.contains("klingon"), "got {}", finding.detail);
        assert!(finding.detail.contains("auto"), "got {}", finding.detail);
    }

    #[test]
    fn a_config_that_is_not_a_whisper_config_is_caught_too() {
        let models = scratch_models("candle-badconfig");
        write_candle_model(&models, "homegrown");
        let dir = crate::stt::candle::store::directory(&models, "homegrown");
        std::fs::write(dir.join("config.json"), br#"{"num_mel_bins":80}"#).unwrap();
        let stt = config::Stt {
            engine: "candle".to_string(),
            model: "homegrown".to_string(),
            ..config::Stt::default()
        };
        let finding = engine_and_model_findings(&stt, &models).0;
        assert_eq!(finding.state, State::Missing, "got {finding:?}");
    }

    #[test]
    fn doctor_reads_no_weights() {
        // The property that makes the check/load split worth having. A present,
        // verified model must still cost doctor nothing to report on: if this
        // regresses, `herdr-voice doctor` silently starts taking seconds and
        // loading gigabytes, and nothing else would catch it.
        let models = scratch_models("no-weights");
        write_candle_model(&models, "homegrown");
        let stt = config::Stt {
            engine: "candle".to_string(),
            model: "homegrown".to_string(),
            ..config::Stt::default()
        };
        crate::stt::candle::store::weight_reads::reset();
        let _ = engine_and_model_findings(&stt, &models);
        assert_eq!(
            crate::stt::candle::store::weight_reads::get(),
            0,
            "doctor loaded the weights"
        );
    }

    /// A small, well-formed candle model: three real files, none of them the
    /// gigabyte the catalogue pins. Enough for every check but size and digest.
    fn write_candle_model(models: &Path, identifier: &str) {
        let dir = crate::stt::candle::store::directory(models, identifier);
        std::fs::create_dir_all(&dir).unwrap();
        let body = r#"{"a":{"dtype":"F32","shape":[1],"data_offsets":[0,4]}}"#;
        let mut weights = (body.len() as u64).to_le_bytes().to_vec();
        weights.extend_from_slice(body.as_bytes());
        weights.extend_from_slice(&[0u8; 4]);
        std::fs::write(dir.join("model.safetensors"), weights).unwrap();
        // A real Whisper config, not just valid JSON: `candle::precheck` reads it
        // as a `whisper::Config`, which is what lets `doctor` catch a config.json
        // that parses and is not a model's.
        std::fs::write(dir.join("config.json"), WHISPER_CONFIG).unwrap();
        std::fs::write(dir.join("tokenizer.json"), br#"{"version":"1.0"}"#).unwrap();
    }

    /// The smallest `config.json` that deserialises as a Whisper config.
    const WHISPER_CONFIG: &[u8] = br#"{
        "num_mel_bins": 80,
        "max_source_positions": 1500,
        "d_model": 384,
        "encoder_attention_heads": 6,
        "encoder_layers": 4,
        "vocab_size": 51865,
        "max_target_positions": 448,
        "decoder_attention_heads": 6,
        "decoder_layers": 4
    }"#;

    #[test]
    fn the_model_line_is_not_used_when_the_argument_list_has_no_placeholder() {
        let models = scratch_models("no-placeholder");
        let stt = command_stt(&["prog", "{audio}"]);
        let finding = engine_and_model_findings(&stt, &models).1;
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
        assert_eq!(
            engine_and_model_findings(&stt, &models).1.state,
            State::Missing
        );
    }

    #[test]
    fn the_engine_line_names_what_resolve_reports_for_each_engine() {
        // candle is built now: it fails for want of a model, not for want of
        // code, and the line says which model and what to do about it.
        let models = scratch_models("engine-candle");
        let candle = config::Stt {
            engine: "candle".to_string(),
            ..config::Stt::default()
        };
        let finding = engine_and_model_findings(&candle, &models).0;
        assert_eq!(finding.state, State::Missing);
        assert!(finding.detail.contains("large-v3-turbo"), "got {finding:?}");
        assert!(finding.detail.contains("model --choose"), "got {finding:?}");
        assert!(
            !finding.detail.contains("#15"),
            "candle is built; it must not report itself unbuilt: {finding:?}"
        );

        let models = scratch_models("engine-http");
        let http = config::Stt {
            engine: "http".to_string(),
            ..config::Stt::default()
        };
        let finding = engine_and_model_findings(&http, &models).0;
        assert_eq!(finding.state, State::Missing);
        assert!(finding.detail.contains("http"), "got {finding:?}");
        assert!(finding.detail.contains("#16"), "got {finding:?}");

        let models = scratch_models("engine-command");
        let command = command_stt(&["prog", "{audio}"]);
        let finding = engine_and_model_findings(&command, &models).0;
        assert_eq!(finding.state, State::Ok);

        let models = scratch_models("engine-unknown");
        let unknown = config::Stt {
            engine: "vosk".to_string(),
            ..config::Stt::default()
        };
        let finding = engine_and_model_findings(&unknown, &models).0;
        assert_eq!(finding.state, State::Missing);
        assert!(finding.detail.contains("vosk"), "got {finding:?}");
    }

    #[test]
    fn the_model_is_located_once_per_doctor_run() {
        let models = scratch_models("locate-once");
        write_real_model(&models, "large-v3-turbo");
        let stt = command_stt(&["prog", "-m", "{model}"]);

        model::locate_calls::reset();
        let (engine, model_line) = engine_and_model_findings(&stt, &models);
        assert_eq!(engine.state, State::Ok, "got {engine:?}");
        assert_eq!(model_line.state, State::Ok, "got {model_line:?}");
        assert_eq!(
            model::locate_calls::get(),
            1,
            "doctor must locate the model once per invocation, not once per report line"
        );
    }

    #[test]
    fn the_rewrite_engine_is_looked_for_by_name() {
        let unavailable = crate::config::Rewrite {
            engine: "agent".to_string(),
            agent: "definitely-not-installed".to_string(),
            ..Default::default()
        };
        assert_eq!(rewrite_finding(&unavailable).state, State::Missing);

        let off = crate::config::Rewrite {
            engine: "off".to_string(),
            ..Default::default()
        };
        assert_eq!(rewrite_finding(&off).state, State::Ok);

        let auto = crate::config::Rewrite {
            engine: "agent".to_string(),
            agent: "auto".to_string(),
            ..Default::default()
        };
        assert_eq!(rewrite_finding(&auto).name, "rewrite");
    }

    #[test]
    fn http_is_ok_when_a_url_is_configured() {
        let rewrite = crate::config::Rewrite {
            engine: "http".to_string(),
            url: "http://127.0.0.1:1234/v1/chat/completions".to_string(),
            ..Default::default()
        };
        assert_eq!(rewrite_finding(&rewrite).state, State::Ok);
    }

    #[test]
    fn http_is_missing_when_no_url_is_configured() {
        let rewrite = crate::config::Rewrite {
            engine: "http".to_string(),
            ..Default::default()
        };
        assert_eq!(rewrite_finding(&rewrite).state, State::Missing);
    }

    #[test]
    fn command_is_ok_when_a_program_is_configured() {
        let rewrite = crate::config::Rewrite {
            engine: "command".to_string(),
            command: vec!["echo".to_string()],
            ..Default::default()
        };
        assert_eq!(rewrite_finding(&rewrite).state, State::Ok);
    }

    #[test]
    fn agent_is_missing_even_when_found_on_path() {
        let rewrite = crate::config::Rewrite {
            engine: "agent".to_string(),
            agent: "auto".to_string(),
            ..Default::default()
        };
        // Whether or not a real agent binary happens to be on this machine's
        // PATH, the take path treats "agent" as unavailable, so doctor must
        // report the same thing regardless.
        assert_eq!(rewrite_finding(&rewrite).state, State::Missing);
    }
}
