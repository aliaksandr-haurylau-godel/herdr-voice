//! herdr-voice — voice dictation for the herdr terminal multiplexer.
//!
//! The binary is both the daemon and the client: `daemon` runs the long-lived
//! process that owns the model and the recording, every other subcommand is a
//! short call that talks to it. See `docs/design.md`, section 2.
//!
//! Nothing of the pipeline is implemented yet. This entry point exists so that
//! the crate, the manifest, the tests and the release pipeline can be exercised
//! before the first feature lands.

mod audio;
mod capture;
mod client;
mod config;
mod context;
mod daemon;
mod delivery;
mod doctor;
mod proto;
mod stt;
mod transport;

use std::process::ExitCode;

/// What the binary was asked to do.
///
/// Every subcommand the plugin manifest names appears here, including the ones
/// that are not implemented yet: the manifest is the contract with herdr, and a
/// command it names but the binary rejects surfaces as a plugin that installs
/// and then silently does nothing. `scripts/check_manifest.py` enforces the
/// match in CI.
#[derive(Debug, PartialEq, Eq)]
enum Command {
    /// Run the long-lived process.
    Daemon,
    /// Report what is missing: permissions, model, rewrite engine, herdr.
    Doctor,
    /// Toggle recording.
    Dictate,
    /// One keypress of hold-to-talk.
    Ptt,
    /// Stop and discard, and clear what a dead run left behind.
    Cancel,
    /// Print the keybindings to add, and offer to add them.
    Setup,
    /// Show what is happening now.
    Status,
    /// Choose the speech model.
    Model,
    /// Choose the microphone.
    Mic,
    /// Print the version and exit.
    Version,
    /// Print usage.
    Help,
    /// Anything the binary does not know.
    Unknown(String),
}

impl Command {
    /// A name for messages, matching the manifest.
    fn name(&self) -> &'static str {
        match self {
            Command::Daemon => "daemon",
            Command::Doctor => "doctor",
            Command::Dictate => "dictate",
            Command::Ptt => "ptt",
            Command::Cancel => "cancel",
            Command::Setup => "setup",
            Command::Status => "status",
            Command::Model => "model",
            Command::Mic => "mic",
            Command::Version => "version",
            Command::Help => "help",
            Command::Unknown(_) => "unknown",
        }
    }
}

fn parse(args: &[String]) -> Command {
    match args.first().map(String::as_str) {
        None | Some("help") | Some("-h") | Some("--help") => Command::Help,
        Some("daemon") => Command::Daemon,
        Some("doctor") => Command::Doctor,
        Some("dictate") => Command::Dictate,
        Some("ptt") => Command::Ptt,
        Some("cancel") => Command::Cancel,
        Some("setup") => Command::Setup,
        Some("status") => Command::Status,
        Some("model") => Command::Model,
        Some("mic") => Command::Mic,
        Some("--version") | Some("-V") | Some("version") => Command::Version,
        Some(other) => Command::Unknown(other.to_string()),
    }
}

/// Exit code for a command that exists but is not built yet. Distinct from the
/// code for an unknown command, so a caller can tell "not yet" from "never".
const NOT_IMPLEMENTED: u8 = 69;

/// The oldest herdr this plugin works with. `scripts/check_manifest.py` fails if
/// this and `min_herdr_version` in `herdr-plugin.toml` disagree, so the number
/// cannot drift between the two files.
pub const MIN_HERDR_VERSION: &str = "0.8.0";

/// The commands this build actually performs. Everything else reports
/// `not implemented yet` and exits `NOT_IMPLEMENTED`. Test-only: a constant used
/// nowhere else would trip `dead_code`, and CI runs clippy with `-D warnings`.
#[cfg(test)]
const IMPLEMENTED: &[&str] = &["daemon", "doctor", "cancel", "dictate"];

const USAGE: &str = "\
herdr-voice — voice dictation for herdr

usage:
  herdr-voice daemon     run the long-lived process
  herdr-voice doctor     report what is missing
  herdr-voice cancel     stop and discard the current recording
  herdr-voice dictate    start a recording, or finish the one running
  herdr-voice --version  print the version
";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match parse(&args) {
        Command::Help => {
            print!("{USAGE}");
            ExitCode::SUCCESS
        }
        Command::Version => {
            println!("herdr-voice {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Command::Daemon => match daemon::start() {
            Ok(daemon::Outcome::AlreadyRunning(address)) => {
                eprintln!("a daemon is already listening at {address}");
                ExitCode::SUCCESS
            }
            Ok(daemon::Outcome::Served) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("cannot start the daemon: {e}");
                ExitCode::FAILURE
            }
        },
        Command::Doctor => ExitCode::from(doctor::run()),
        other @ (Command::Cancel | Command::Dictate) => {
            let outcome = client::send(other.name());
            if let Some(message) = outcome.message {
                // A result on standard output, a failure on standard error, so a
                // caller can read one without the other.
                if outcome.code == 0 {
                    println!("{message}");
                } else {
                    eprintln!("{message}");
                }
            }
            ExitCode::from(outcome.code)
        }
        other @ (Command::Ptt
        | Command::Setup
        | Command::Status
        | Command::Model
        | Command::Mic) => {
            eprintln!("{}: not implemented yet", other.name());
            ExitCode::from(NOT_IMPLEMENTED)
        }
        Command::Unknown(what) => {
            eprintln!("unknown command: {what}");
            eprint!("{USAGE}");
            ExitCode::from(2)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn no_arguments_prints_usage() {
        assert_eq!(parse(&args(&[])), Command::Help);
    }

    #[test]
    fn known_commands_are_recognised() {
        assert_eq!(parse(&args(&["daemon"])), Command::Daemon);
        assert_eq!(parse(&args(&["doctor"])), Command::Doctor);
        assert_eq!(parse(&args(&["--version"])), Command::Version);
    }

    #[test]
    fn every_manifest_command_is_accepted() {
        // The manifest names these; rejecting one would produce a plugin that
        // installs and then does nothing when the action is invoked.
        for name in [
            "daemon", "dictate", "ptt", "cancel", "setup", "status", "model", "mic", "doctor",
        ] {
            assert!(
                !matches!(parse(&args(&[name])), Command::Unknown(_)),
                "the binary rejects '{name}', which the manifest calls"
            );
        }
    }

    #[test]
    fn the_commands_this_issue_implements_are_not_in_the_unimplemented_arm() {
        // A guard against a later change quietly folding one back into the 69 arm.
        for name in ["daemon", "doctor", "cancel", "dictate"] {
            assert!(
                IMPLEMENTED.contains(&name),
                "{name} is implemented and must not report 'not implemented yet'"
            );
        }
        for name in ["ptt", "setup", "status", "model", "mic"] {
            assert!(
                !IMPLEMENTED.contains(&name),
                "{name} is not implemented in this issue"
            );
        }
    }

    #[test]
    fn the_usage_text_names_every_implemented_command() {
        for name in IMPLEMENTED {
            assert!(USAGE.contains(name), "usage does not mention {name}");
        }
    }

    #[test]
    fn an_unknown_command_is_named_back() {
        assert_eq!(
            parse(&args(&["transcribe"])),
            Command::Unknown("transcribe".to_string())
        );
    }
}
