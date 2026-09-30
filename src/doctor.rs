//! What is missing, and what to do about it.
//!
//! Eight lines in a fixed order: herdr, daemon, notifications, config, engine,
//! model, rewrite, record. The prototype's worst failure was silence: a parse
//! error produced no output and looked like a hang for a morning, so every line
//! that is not `ok` names the next action. See `tasks/3/DESIGN_3.md`, section 4.

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
    /// Something that may work but cannot be confirmed from here. Unlike
    /// `Missing` it does not fail the exit code.
    Warning,
}

impl State {
    fn word(&self) -> &'static str {
        match self {
            State::Ok => "ok",
            State::Default => "default",
            State::Missing => "missing",
            State::NotUsed => "unused",
            State::Warning => "warning",
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
            "{:<13} {:<8} {}\n",
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
    // weights and run an encoder pass to print seven lines
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

/// Where a take's record goes, and whether anything is being written there.
///
/// Never `Missing`: `Missing` is what makes this command exit non-zero
/// (`exit_code`), and a key sitting at its own default is not a fault to go and
/// fix.
pub fn record_finding(record: &config::Record, takes: &Path) -> Finding {
    if !record.transcripts {
        return Finding {
            name: "record",
            state: State::Default,
            detail: "off; set [record] transcripts = true to keep each take's \
                     transcript and rewrite beside its recording"
                .to_string(),
        };
    }
    Finding {
        name: "record",
        state: State::Ok,
        detail: format!(
            "on; each take's transcript and rewrite are written to {}, and the \
             last {} takes are kept",
            takes.display(),
            crate::record::KEEP
        ),
    }
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

/// Where herdr's own configuration file is expected to be.
#[derive(Debug, PartialEq, Eq)]
enum Location {
    Path(PathBuf),
    Unknown,
}

/// herdr's order for its configuration file: `HERDR_CONFIG_PATH`, then on
/// Windows `%APPDATA%\herdr\config.toml` (from herdr's documentation, not
/// verified on Windows), elsewhere `XDG_CONFIG_HOME/herdr/config.toml`, then
/// `~/.config/herdr/config.toml`. Takes the values rather than reading them, so
/// the tests do not mutate an environment the parallel suite shares.
fn herdr_config_path_from(
    explicit: Option<String>,
    xdg: Option<String>,
    home: Option<String>,
    appdata: Option<String>,
    windows: bool,
) -> Location {
    let non_empty = |v: Option<String>| v.filter(|s| !s.is_empty());
    if let Some(path) = non_empty(explicit) {
        return Location::Path(PathBuf::from(path));
    }
    let file = |dir: PathBuf| Location::Path(dir.join("herdr").join("config.toml"));
    if windows {
        return match non_empty(appdata) {
            Some(dir) => file(PathBuf::from(dir)),
            None => Location::Unknown,
        };
    }
    if let Some(dir) = non_empty(xdg) {
        return file(PathBuf::from(dir));
    }
    match non_empty(home) {
        Some(home) => file(PathBuf::from(home).join(".config")),
        None => Location::Unknown,
    }
}

/// The values herdr documents for `[ui.toast] delivery`.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum Delivery {
    Off,
    Herdr,
    Terminal,
    System,
}

/// What herdr's configuration says about `[ui.toast] delivery`.
#[derive(Debug, PartialEq, Eq)]
enum Configured {
    Value(Delivery),
    /// The key is not there. herdr's default is then `off`.
    Absent,
    /// Present, but not one of the four documented values; holds the value as
    /// TOML text.
    Other(String),
    Unparsable(String),
}

fn read_delivery(text: &str) -> Configured {
    let parsed: toml::Value = match toml::from_str(text) {
        Ok(parsed) => parsed,
        Err(e) => return Configured::Unparsable(e.to_string()),
    };
    let Some(value) = parsed
        .get("ui")
        .and_then(|ui| ui.get("toast"))
        .and_then(|toast| toast.get("delivery"))
    else {
        return Configured::Absent;
    };
    match value.as_str() {
        Some("off") => Configured::Value(Delivery::Off),
        Some("herdr") => Configured::Value(Delivery::Herdr),
        Some("terminal") => Configured::Value(Delivery::Terminal),
        Some("system") => Configured::Value(Delivery::System),
        _ => Configured::Other(value.to_string()),
    }
}

/// Why herdr's configuration file gave no text.
enum ReadFailure {
    Absent,
    Other(String),
}

/// The first line of `text`, trimmed. The `toml` crate's error text spans
/// several lines and a finding is one.
fn one_line(text: &str) -> String {
    text.lines().next().unwrap_or("").trim().to_string()
}

/// The advice every line that is not `ok` carries, so the person is told what to
/// change and where the messages are written when herdr does not show them.
const NOTIFICATIONS_ADVICE: &str = "to have herdr show this plugin's messages itself, \
    set `[ui.toast] delivery = \"herdr\"` in herdr's configuration and run \
    `herdr server reload-config`; they are also written to the plugin log: \
    `herdr plugin log list --plugin herdr-voice`";

/// Where herdr is configured to send notifications, and whether this plugin's
/// own messages can be expected to arrive. herdr has no command that reports the
/// setting, and the reply of `herdr notification show` is the same for every
/// value once a client is attached, so the configuration file is the only
/// source (`tasks/85/AC_85.md`). What is said is what is configured there, never
/// that it is in effect: a server that has not reloaded can differ.
fn notifications_finding(
    plugin_toasts: bool,
    location: &Location,
    file: Result<String, ReadFailure>,
) -> Finding {
    let finding = |state: State, detail: String| Finding {
        name: "notifications",
        state,
        detail,
    };
    if !plugin_toasts {
        return finding(
            State::NotUsed,
            "[ui] toasts = false in the plugin's configuration: the plugin raises no \
             toasts, so herdr's delivery setting does not matter"
                .to_string(),
        );
    }
    let advice = NOTIFICATIONS_ADVICE;
    let Location::Path(path) = location else {
        return finding(
            State::Missing,
            format!(
                "cannot tell where herdr's configuration is: none of HERDR_CONFIG_PATH, \
                 XDG_CONFIG_HOME, HOME (APPDATA on Windows) is set, so it is not known \
                 where notifications go; set HERDR_CONFIG_PATH to the file; {advice}"
            ),
        );
    };
    let path = path.display();
    let nowhere = "this plugin's failure messages will not appear anywhere in herdr";
    let text = match file {
        Ok(text) => text,
        Err(ReadFailure::Absent) => {
            return finding(
                State::Missing,
                format!(
                    "no file at {path} (herdr's default delivery is \"off\"): {nowhere}; {advice}"
                ),
            );
        }
        Err(ReadFailure::Other(reason)) => {
            return finding(
                State::Missing,
                format!(
                    "cannot read {path} ({}), so it is not known where notifications go; \
                     once it can be read, {advice}",
                    one_line(&reason)
                ),
            );
        }
    };
    match read_delivery(&text) {
        Configured::Unparsable(reason) => finding(
            State::Missing,
            format!(
                "{path} is not valid TOML ({}), so it is not known where notifications go; \
                 once it parses, {advice}",
                one_line(&reason)
            ),
        ),
        Configured::Value(Delivery::Herdr) => finding(
            State::Ok,
            format!("set to \"herdr\" in {path}: herdr draws the toast itself"),
        ),
        Configured::Value(Delivery::Terminal) => warning(&finding, "terminal", &path, advice),
        Configured::Value(Delivery::System) => warning(&finding, "system", &path, advice),
        Configured::Value(Delivery::Off) => finding(
            State::Missing,
            format!("set to \"off\" in {path}: {nowhere}; {advice}"),
        ),
        Configured::Absent => finding(
            State::Missing,
            format!(
                "no [ui.toast] delivery in {path} (herdr's default is \"off\"): {nowhere}; {advice}"
            ),
        ),
        Configured::Other(value) => finding(
            State::Missing,
            format!(
                "set to {value} in {path}, which herdr does not list (off, herdr, terminal, \
                 system): this plugin's failure messages may not appear; {advice}"
            ),
        ),
    }
}

fn warning(
    finding: &impl Fn(State, String) -> Finding,
    value: &str,
    path: &std::path::Display<'_>,
    advice: &str,
) -> Finding {
    finding(
        State::Warning,
        format!(
            "set to \"{value}\" in {path}: herdr hands the message on and cannot tell whether \
             it appeared, so this plugin's failure messages may not reach you; {advice}"
        ),
    )
}

fn herdr_config_location() -> Location {
    herdr_config_path_from(
        std::env::var("HERDR_CONFIG_PATH").ok(),
        std::env::var("XDG_CONFIG_HOME").ok(),
        std::env::var("HOME").ok(),
        std::env::var("APPDATA").ok(),
        cfg!(windows),
    )
}

fn read_herdr_config(location: &Location) -> Result<String, ReadFailure> {
    let Location::Path(path) = location else {
        // `notifications_finding` ignores the file when the location is unknown.
        return Err(ReadFailure::Absent);
    };
    std::fs::read_to_string(path).map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => ReadFailure::Absent,
        _ => ReadFailure::Other(e.to_string()),
    })
}

pub fn run() -> u8 {
    let loaded = config::load(config::directory(&config::Vars::from_env()).as_deref());
    let location = herdr_config_location();
    let mut findings = vec![
        herdr_finding(),
        daemon_finding(),
        notifications_finding(
            loaded.config.ui.toasts,
            &location,
            read_herdr_config(&location),
        ),
        config_finding(&loaded),
    ];
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
    let takes = transport::takes_directory(&transport::Vars::from_env());
    findings.push(record_finding(&loaded.config.record, &takes));
    print!("{}", render(&findings));
    exit_code(&findings)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stt::model;

    #[test]
    fn recording_off_is_a_default_and_never_a_failure() {
        let finding = record_finding(&config::Record::default(), Path::new("/state/takes"));
        assert_eq!(finding.name, "record");
        assert_eq!(finding.state, State::Default);
        assert!(
            finding.detail.contains("[record] transcripts"),
            "it names the key that turns it on: {}",
            finding.detail
        );
        assert_eq!(exit_code(&[finding]), 0);
    }

    #[test]
    fn recording_on_names_the_directory() {
        let finding = record_finding(
            &config::Record { transcripts: true },
            Path::new("/state/takes"),
        );
        assert_eq!(finding.state, State::Ok);
        assert!(
            finding.detail.contains("/state/takes"),
            "it names where to look: {}",
            finding.detail
        );
        assert_eq!(exit_code(&[finding]), 0);
    }

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
        assert!(finding.detail.contains("url"), "got {finding:?}");

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

    #[test]
    fn a_warning_renders_as_the_word_warning() {
        let text = render(&[Finding {
            name: "notifications",
            state: State::Warning,
            detail: "x".into(),
        }]);
        assert_eq!(text, "notifications warning  x\n");
    }

    #[test]
    fn the_name_column_is_wide_enough_for_notifications() {
        let text = render(&[Finding {
            name: "notifications",
            state: State::Ok,
            detail: "x".into(),
        }]);
        assert_eq!(text, "notifications ok       x\n");
    }

    #[test]
    fn a_warning_does_not_fail_the_exit_code() {
        let findings = vec![
            Finding {
                name: "herdr",
                state: State::Ok,
                detail: String::new(),
            },
            Finding {
                name: "notifications",
                state: State::Warning,
                detail: String::new(),
            },
        ];
        assert_eq!(exit_code(&findings), 0);
    }

    fn some(text: &str) -> Option<String> {
        Some(text.to_string())
    }

    #[test]
    fn an_explicit_herdr_config_path_beats_everything_on_every_platform() {
        for windows in [false, true] {
            assert_eq!(
                herdr_config_path_from(
                    some("/a/config.toml"),
                    some("/xdg"),
                    some("/home"),
                    some("/appdata"),
                    windows
                ),
                Location::Path(PathBuf::from("/a/config.toml"))
            );
        }
    }

    #[test]
    fn xdg_beats_home_outside_windows() {
        assert_eq!(
            herdr_config_path_from(None, some("/xdg"), some("/home"), None, false),
            Location::Path(PathBuf::from("/xdg").join("herdr").join("config.toml"))
        );
    }

    #[test]
    fn home_alone_gives_dot_config_herdr() {
        assert_eq!(
            herdr_config_path_from(None, None, some("/home"), None, false),
            Location::Path(
                PathBuf::from("/home")
                    .join(".config")
                    .join("herdr")
                    .join("config.toml")
            )
        );
    }

    #[test]
    fn with_nothing_set_the_location_is_unknown() {
        assert_eq!(
            herdr_config_path_from(None, None, None, None, false),
            Location::Unknown
        );
        assert_eq!(
            herdr_config_path_from(None, None, None, None, true),
            Location::Unknown
        );
    }

    #[test]
    fn empty_strings_count_as_unset() {
        assert_eq!(
            herdr_config_path_from(some(""), some(""), some(""), some(""), false),
            Location::Unknown
        );
        assert_eq!(
            herdr_config_path_from(some(""), some(""), some(""), some(""), true),
            Location::Unknown
        );
    }

    #[test]
    fn windows_uses_appdata_and_ignores_xdg_and_home() {
        assert_eq!(
            herdr_config_path_from(None, some("/xdg"), some("/home"), some("/appdata"), true),
            Location::Path(PathBuf::from("/appdata").join("herdr").join("config.toml"))
        );
    }

    #[test]
    fn windows_without_appdata_is_unknown_even_with_home_set() {
        assert_eq!(
            herdr_config_path_from(None, some("/xdg"), some("/home"), None, true),
            Location::Unknown
        );
    }

    fn under_toast(line: &str) -> String {
        format!("[ui.toast]\n{line}\n")
    }

    #[test]
    fn each_listed_delivery_is_read() {
        for (word, expected) in [
            ("off", Delivery::Off),
            ("herdr", Delivery::Herdr),
            ("terminal", Delivery::Terminal),
            ("system", Delivery::System),
        ] {
            assert_eq!(
                read_delivery(&under_toast(&format!("delivery = \"{word}\""))),
                Configured::Value(expected)
            );
        }
    }

    #[test]
    fn a_missing_key_or_table_is_absent() {
        assert_eq!(read_delivery(""), Configured::Absent);
        assert_eq!(read_delivery("[ui]\ntoasts = false\n"), Configured::Absent);
        assert_eq!(read_delivery("[ui]\ntoast = \"x\"\n"), Configured::Absent);
        assert_eq!(read_delivery(&under_toast("other = 1")), Configured::Absent);
    }

    #[test]
    fn a_top_level_delivery_key_is_not_the_setting() {
        assert_eq!(read_delivery("delivery = \"off\"\n"), Configured::Absent);
    }

    #[test]
    fn a_value_herdr_does_not_list_is_quoted_back() {
        assert_eq!(
            read_delivery(&under_toast("delivery = \"Herdr\"")),
            Configured::Other("\"Herdr\"".to_string())
        );
        assert_eq!(
            read_delivery(&under_toast("delivery = 3")),
            Configured::Other("3".to_string())
        );
    }

    #[test]
    fn text_that_is_not_toml_is_unparsable() {
        assert!(matches!(
            read_delivery("[ui.toast]\ndelivery = \n"),
            Configured::Unparsable(_)
        ));
    }

    const REQUIRED: [&str; 4] = [
        "[ui.toast]",
        "delivery = \"herdr\"",
        "herdr server reload-config",
        "herdr plugin log list --plugin herdr-voice",
    ];

    fn file_at() -> Location {
        Location::Path(PathBuf::from("/x/herdr/config.toml"))
    }

    fn with_file(text: &str) -> Finding {
        notifications_finding(true, &file_at(), Ok(text.to_string()))
    }

    fn assert_full_advice(finding: &Finding) {
        for needle in REQUIRED {
            assert!(
                finding.detail.contains(needle),
                "missing {needle:?} in {}",
                finding.detail
            );
        }
    }

    #[test]
    fn the_finding_is_named_notifications() {
        assert_eq!(with_file("").name, "notifications");
    }

    #[test]
    fn herdr_delivery_is_ok_and_names_value_and_file() {
        let f = with_file(&under_toast("delivery = \"herdr\""));
        assert_eq!(f.state, State::Ok);
        assert!(f.detail.contains("set to \"herdr\""), "{}", f.detail);
        assert!(f.detail.contains("/x/herdr/config.toml"), "{}", f.detail);
        assert!(!f.detail.contains("reload-config"), "{}", f.detail);
    }

    #[test]
    fn terminal_and_system_are_warnings_with_the_full_advice() {
        for word in ["terminal", "system"] {
            let f = with_file(&under_toast(&format!("delivery = \"{word}\"")));
            assert_eq!(f.state, State::Warning);
            assert!(
                f.detail.contains(&format!("set to \"{word}\"")),
                "{}",
                f.detail
            );
            assert!(f.detail.contains("/x/herdr/config.toml"), "{}", f.detail);
            assert!(
                f.detail.contains("cannot tell whether it appeared"),
                "{}",
                f.detail
            );
            assert_full_advice(&f);
        }
    }

    #[test]
    fn off_is_missing_with_the_full_advice() {
        let f = with_file(&under_toast("delivery = \"off\""));
        assert_eq!(f.state, State::Missing);
        assert!(f.detail.contains("set to \"off\""), "{}", f.detail);
        assert!(f.detail.contains("/x/herdr/config.toml"), "{}", f.detail);
        assert_full_advice(&f);
    }

    #[test]
    fn an_absent_key_is_off_by_herdrs_default() {
        let f = with_file("[ui]\ntoasts = false\n");
        assert_eq!(f.state, State::Missing);
        assert!(
            f.detail.contains("no [ui.toast] delivery in"),
            "{}",
            f.detail
        );
        assert!(f.detail.contains("default is \"off\""), "{}", f.detail);
        assert!(f.detail.contains("/x/herdr/config.toml"), "{}", f.detail);
        assert_full_advice(&f);
    }

    #[test]
    fn an_absent_file_is_off_by_herdrs_default() {
        let f = notifications_finding(true, &file_at(), Err(ReadFailure::Absent));
        assert_eq!(f.state, State::Missing);
        assert!(f.detail.contains("no file at"), "{}", f.detail);
        assert!(
            f.detail.contains("default delivery is \"off\""),
            "{}",
            f.detail
        );
        assert!(f.detail.contains("/x/herdr/config.toml"), "{}", f.detail);
        assert_full_advice(&f);
    }

    #[test]
    fn an_unlisted_value_is_missing_and_quoted() {
        let f = with_file(&under_toast("delivery = \"Herdr\""));
        assert_eq!(f.state, State::Missing);
        assert!(f.detail.contains("set to \"Herdr\""), "{}", f.detail);
        assert!(f.detail.contains("does not list"), "{}", f.detail);
        assert!(f.detail.contains("/x/herdr/config.toml"), "{}", f.detail);
        assert_full_advice(&f);
    }

    #[test]
    fn an_unreadable_file_is_missing_with_the_reason() {
        let f = notifications_finding(
            true,
            &file_at(),
            Err(ReadFailure::Other("permission denied".into())),
        );
        assert_eq!(f.state, State::Missing);
        assert!(
            f.detail.contains("cannot read /x/herdr/config.toml"),
            "{}",
            f.detail
        );
        assert!(f.detail.contains("permission denied"), "{}", f.detail);
        assert_full_advice(&f);
    }

    #[test]
    fn a_file_that_is_not_toml_is_missing_with_the_first_line_of_the_reason() {
        let text = under_toast("delivery = ");
        let Configured::Unparsable(reason) = read_delivery(&text) else {
            panic!("the fixture must not parse");
        };
        assert!(
            reason.contains('\n'),
            "the fixture must give a multi-line reason"
        );
        let f = with_file(&text);
        assert_eq!(f.state, State::Missing);
        assert!(f.detail.contains("is not valid TOML"), "{}", f.detail);
        assert!(f.detail.contains("/x/herdr/config.toml"), "{}", f.detail);
        assert!(f.detail.contains(&one_line(&reason)), "{}", f.detail);
        assert_full_advice(&f);
    }

    #[test]
    fn an_unknown_location_is_missing_and_says_how_to_fix_it() {
        let f = notifications_finding(true, &Location::Unknown, Ok(String::new()));
        assert_eq!(f.state, State::Missing);
        assert!(f.detail.contains("HERDR_CONFIG_PATH"), "{}", f.detail);
        assert_full_advice(&f);
    }

    #[test]
    fn plugin_toasts_off_is_unused_and_wins_over_everything_else() {
        let cases = [
            notifications_finding(false, &Location::Unknown, Ok(String::new())),
            notifications_finding(false, &file_at(), Err(ReadFailure::Other("x".into()))),
            notifications_finding(false, &file_at(), Err(ReadFailure::Absent)),
            notifications_finding(
                false,
                &file_at(),
                Ok(under_toast("delivery = \"terminal\"")),
            ),
        ];
        for f in cases {
            assert_eq!(f.state, State::NotUsed);
            assert!(f.detail.contains("[ui] toasts = false"), "{}", f.detail);
            assert!(!f.detail.contains("reload-config"), "{}", f.detail);
            assert!(!f.detail.contains("delivery = \"herdr\""), "{}", f.detail);
        }
    }

    #[test]
    fn no_detail_spans_lines_or_claims_to_be_in_effect() {
        let broken = under_toast("delivery = ");
        let cases = [
            with_file(&broken),
            with_file(""),
            with_file(&under_toast("delivery = \"terminal\"")),
            with_file(&under_toast("delivery = \"herdr\"")),
            notifications_finding(true, &Location::Unknown, Ok(String::new())),
            notifications_finding(false, &file_at(), Ok(String::new())),
        ];
        for f in cases {
            assert!(!f.detail.contains('\n'), "{:?}", f.detail);
            assert!(!f.detail.contains("in effect"), "{}", f.detail);
        }
    }

    #[test]
    fn one_line_keeps_the_first_line_trimmed() {
        assert_eq!(one_line("first\nsecond"), "first");
        assert_eq!(one_line("   padded  \nsecond"), "padded");
        assert_eq!(one_line(""), "");
    }

    /// A directory of the test's own: named by test and process, because the
    /// tests run in parallel and one that removes a shared directory deletes
    /// another's fixture.
    fn scratch_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "herdr-voice-doctor-notifications-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_file_is_read_as_text() {
        let dir = scratch_dir("read");
        let path = dir.join("config.toml");
        std::fs::write(&path, "[ui.toast]\ndelivery = \"herdr\"\n").unwrap();
        match read_herdr_config(&Location::Path(path)) {
            Ok(text) => assert_eq!(text, "[ui.toast]\ndelivery = \"herdr\"\n"),
            Err(_) => panic!("the file exists and is readable"),
        }
    }

    #[test]
    fn a_missing_file_is_absent() {
        let dir = scratch_dir("missing");
        let result = read_herdr_config(&Location::Path(dir.join("nothing.toml")));
        assert!(matches!(result, Err(ReadFailure::Absent)));
    }

    #[test]
    fn a_directory_in_place_of_the_file_is_another_failure() {
        let dir = scratch_dir("directory");
        let result = read_herdr_config(&Location::Path(dir));
        assert!(matches!(result, Err(ReadFailure::Other(_))));
    }
}
