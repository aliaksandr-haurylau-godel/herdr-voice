//! Reading a pane's screen contents through herdr, filtered.
//!
//! `argv` is pure — the exact command line, checked against the contract in
//! `spike/context.sh:42-45` without a live herdr. `read` runs a program (never
//! `herdr` itself, in a test) and filters its output the same way the
//! prototype does: trim trailing whitespace per line, drop lines with no
//! letter or digit, keep the last `PANE_LINES` lines.

use std::fmt;
use std::process::Command;

/// The pane branch has no budget of its own among the four `[context]` keys:
/// the prototype's 80 lines, plus the overall `prompt_chars` cap, already
/// bound the result (`tasks/21/DESIGN_21.md`, section 5).
pub const PANE_LINES: usize = 80;

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
    let arguments = &argv(pane, lines)[1..];
    let output = Command::new(binary).args(arguments).output();
    let output = match output {
        Ok(output) => output,
        Err(_) => {
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
}
