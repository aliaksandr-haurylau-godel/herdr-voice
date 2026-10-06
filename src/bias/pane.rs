//! Reading a pane's screen contents through herdr, filtered.
//!
//! `argv` is pure — the exact command line, checked against the contract in
//! `spike/context.sh:42-45` without a live herdr. `read` runs a program (never
//! `herdr` itself, in a test) and filters its output the same way the
//! prototype does: trim trailing whitespace per line, drop lines with no
//! letter or digit, keep the last `PANE_LINES` lines.

use std::fmt;
use std::process::Command;
use std::time::Duration;

use crate::outward::{self, RunError};

/// The pane branch has no budget of its own among the four `[context]` keys:
/// the prototype's 80 lines, plus the overall `prompt_chars` cap, already
/// bound the result (`tasks/21/DESIGN_21.md`, section 5).
pub const PANE_LINES: usize = 80;

/// How long `herdr pane read` may take. The take is waiting on it before
/// recognition starts, and a miss costs only the bias, so it is shorter than
/// delivery's bound. `docs/decisions.md`, the entry for the pane read.
pub const BOUND: Duration = Duration::from_secs(5);

/// Builds the exact `herdr pane read` command line. Pure: no process, no I/O.
pub fn argv(pane: &str, lines: usize) -> Vec<String> {
    vec![
        "herdr".to_string(),
        "pane".to_string(),
        "read".to_string(),
        pane.to_string(),
        "--source".to_string(),
        "recent".to_string(),
        "--lines".to_string(),
        lines.to_string(),
        "--format".to_string(),
        "text".to_string(),
    ]
}

#[derive(Debug)]
pub enum PaneError {
    NotFound { program: String },
    Failed { program: String, code: String },
    TimedOut { program: String, bound: Duration },
}

impl fmt::Display for PaneError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Quoted by hand, not with `{program:?}`: Debug escapes a backslash to
        // a doubled one, which turns a Windows path into text nobody typed —
        // `program` is quoted for readability, not re-parsed, so plain
        // interpolation keeps it exactly as given.
        match self {
            PaneError::NotFound { program } => write!(
                f,
                "cannot run \"{program}\"; install herdr, or set HERDR_BIN_PATH to it"
            ),
            PaneError::TimedOut { program, bound } => write!(
                f,
                "\"{program}\" did not answer within {}, so the plugin stopped it and went on \
                 without the pane's text. If herdr is not responding, restart it",
                crate::http_failure::bound_text(*bound)
            ),
            PaneError::Failed { program, code } => {
                write!(f, "\"{program}\" failed ({code})")
            }
        }
    }
}

impl std::error::Error for PaneError {}

/// Runs `binary` with `argv`'s arguments (minus the program name), and filters
/// its standard output. `binary` is an explicit parameter — this function does
/// not resolve `HERDR_BIN_PATH` itself; that happens at the daemon call site.
pub fn read(pane: &str, lines: usize, binary: &str) -> Result<String, PaneError> {
    read_within(pane, lines, binary, BOUND)
}

/// `read` with the bound given, so a test does not wait the real one.
pub fn read_within(
    pane: &str,
    lines: usize,
    binary: &str,
    bound: Duration,
) -> Result<String, PaneError> {
    let arguments = &argv(pane, lines)[1..];
    let mut command = Command::new(binary);
    command.args(arguments);
    let output = match outward::run(&mut command, bound) {
        Ok(output) => output,
        Err(RunError::TimedOut) => {
            return Err(PaneError::TimedOut {
                program: binary.to_string(),
                bound,
            })
        }
        Err(RunError::Start(_)) => {
            return Err(PaneError::NotFound {
                program: binary.to_string(),
            })
        }
    };
    if !output.status.success() {
        let code = match output.status.code() {
            Some(code) => code.to_string(),
            None => "killed by a signal".to_string(),
        };
        return Err(PaneError::Failed {
            program: binary.to_string(),
            code,
        });
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let filtered: Vec<&str> = text
        .lines()
        .map(|line| line.trim_end())
        .filter(|line| line.chars().any(|c| c.is_alphanumeric()))
        .collect();
    let start = filtered.len().saturating_sub(lines);
    Ok(filtered[start..].join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// `stdout` and `stderr` are written verbatim to a fixture file each, and
    /// the script only replays them and exits with `exit_code` — the same
    /// shape as `src/delivery.rs`'s `recorder`, so a script every platform CI
    /// checks can actually run, not just this one.
    fn scratch_dir(tag: &str) -> PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "herdr-voice-bias-pane-script-{tag}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("create scratch");
        path
    }

    #[cfg(unix)]
    fn scratch_script(tag: &str, stdout: &str, stderr: &str, exit_code: i32) -> PathBuf {
        let dir = scratch_dir(tag);
        let stdout_path = dir.join("stdout.txt");
        let stderr_path = dir.join("stderr.txt");
        std::fs::write(&stdout_path, stdout).expect("write stdout fixture");
        std::fs::write(&stderr_path, stderr).expect("write stderr fixture");
        let script = dir.join("script.sh");
        std::fs::write(
            &script,
            format!("#!/bin/sh\ncat {stdout_path:?}\ncat {stderr_path:?} >&2\nexit {exit_code}\n"),
        )
        .expect("write script");
        let mut perms = std::fs::metadata(&script).unwrap().permissions();
        std::os::unix::fs::PermissionsExt::set_mode(&mut perms, 0o755);
        std::fs::set_permissions(&script, perms).expect("chmod script");
        script
    }

    #[cfg(windows)]
    fn scratch_script(tag: &str, stdout: &str, stderr: &str, exit_code: i32) -> PathBuf {
        let dir = scratch_dir(tag);
        let stdout_path = dir.join("stdout.txt");
        let stderr_path = dir.join("stderr.txt");
        std::fs::write(&stdout_path, stdout).expect("write stdout fixture");
        std::fs::write(&stderr_path, stderr).expect("write stderr fixture");
        let script = dir.join("script.cmd");
        std::fs::write(
            &script,
            format!(
                "@echo off\r\ntype \"{}\"\r\ntype \"{}\" 1>&2\r\nexit /b {exit_code}\r\n",
                stdout_path.display(),
                stderr_path.display()
            ),
        )
        .expect("write script");
        script
    }

    #[test]
    fn argv_matches_the_contract_exactly() {
        assert_eq!(
            argv("w1:p2", 80),
            vec![
                "herdr", "pane", "read", "w1:p2", "--source", "recent", "--lines", "80",
                "--format", "text",
            ]
        );
    }

    #[test]
    fn output_is_filtered_and_line_capped() {
        let script = scratch_script("filter", "line one   \n\n   \nline two\n", "", 0);
        let result = read("w1:p2", 80, script.to_str().unwrap()).expect("ok");
        assert_eq!(result, "line one\nline two");
    }

    #[test]
    fn the_line_cap_is_the_one_the_caller_asked_for() {
        let script = scratch_script("line-cap", "one\ntwo\nthree\nfour\nfive\n", "", 0);
        let result = read("w1:p2", 2, script.to_str().unwrap()).expect("ok");
        assert_eq!(result, "four\nfive");
    }

    #[test]
    fn a_nonexistent_program_is_not_found() {
        let error = read("w1:p2", 80, "/definitely/not/a/real/herdr-binary").expect_err("err");
        assert!(matches!(error, PaneError::NotFound { .. }), "got {error:?}");
    }

    #[test]
    fn a_program_that_is_not_there_says_what_to_set() {
        let error = read("w1:p2", 80, "/definitely/not/a/real/herdr-binary").expect_err("err");
        let text = error.to_string();
        assert!(text.contains("HERDR_BIN_PATH"), "got {text:?}");
    }

    #[test]
    fn a_nonzero_exit_names_the_code() {
        let script = scratch_script("fail", "", "", 3);
        let error = read("w1:p2", 80, script.to_str().unwrap()).expect_err("err");
        match &error {
            PaneError::Failed { code, .. } => assert_eq!(code, "3"),
            other => panic!("expected Failed, got {other:?}"),
        }
        // The rendering, which is what reaches the journal and a person: the
        // variant carrying the code proves nothing about the message printing
        // it.
        let text = error.to_string();
        assert!(text.contains("failed (3)"), "got {text:?}");
        assert!(
            text.contains(script.to_str().unwrap()),
            "the message must name the program, got {text:?}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_herdr_that_never_answers_is_a_timeout_naming_it_and_the_bound() {
        let binary = crate::delivery::tests_support::herdr_that_hangs_on("pane-hang", &["pane"]);
        let started = std::time::Instant::now();
        let error = read_within("w1:p2", 80, &binary, std::time::Duration::from_millis(200))
            .expect_err("must fail");
        assert!(started.elapsed() < std::time::Duration::from_secs(3));
        match &error {
            PaneError::TimedOut { program, bound } => {
                assert_eq!(program, &binary);
                assert_eq!(*bound, std::time::Duration::from_millis(200));
            }
            other => panic!("expected TimedOut, got {other:?}"),
        }
        let text = error.to_string();
        assert!(text.contains(&binary), "{text}");
        assert!(text.contains("200 milliseconds"), "{text}");
        assert!(
            text.contains("without the pane"),
            "says what the take does next: {text}"
        );
        assert!(text.contains("restart"), "says what to do: {text}");
    }

    #[test]
    fn the_bound_is_five_seconds() {
        assert_eq!(BOUND, std::time::Duration::from_secs(5));
    }

    #[cfg(unix)]
    #[test]
    fn read_gives_herdr_the_real_five_second_bound() {
        let binary = crate::delivery::tests_support::herdr_that_hangs_on("pane-real", &["pane"]);
        let started = std::time::Instant::now();
        let error = read("w1:p2", 80, &binary).expect_err("must fail");
        assert!(
            matches!(&error, PaneError::TimedOut { bound, .. } if *bound == BOUND),
            "got {error:?}"
        );
        assert!(started.elapsed() >= std::time::Duration::from_millis(4_900));
        assert!(started.elapsed() < std::time::Duration::from_secs(8));
    }
}
