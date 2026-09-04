//! Rewriting by running somebody else's program.
//!
//! Mirrors `src/stt/command.rs`'s shape: the argument list is theirs, not ours.
//! `{transcript}` is force-appended when the list never asks for it, the same
//! way `{audio}` is there; `{bias}` is never force-appended, an opt-in
//! enhancement a program can run without. See `tasks/36/DESIGN_36.md`, section 7.

use std::fmt;
use std::process::Command;

/// Replace the placeholders, and append the transcript when the list never
/// asks for it — so a program that simply takes text still works. `{bias}`
/// is substituted when present but never force-appended.
pub fn render(argv: &[String], transcript: &str, bias: &str) -> Vec<String> {
    let mut rendered: Vec<String> = argv
        .iter()
        .map(|argument| {
            let out = argument.replace("{transcript}", transcript);
            out.replace("{bias}", bias)
        })
        .collect();
    if !argv.iter().any(|argument| argument.contains("{transcript}")) {
        rendered.push(transcript.to_string());
    }
    rendered
}

pub struct CommandEngine {
    argv: Vec<String>,
}

impl CommandEngine {
    pub fn new(argv: Vec<String>) -> CommandEngine {
        CommandEngine { argv }
    }
}

#[derive(Debug)]
pub enum CommandError {
    NotFound {
        program: String,
        path: String,
    },
    Failed {
        program: String,
        code: String,
        stderr: String,
    },
    Silent {
        program: String,
    },
}

impl fmt::Display for CommandError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            // herdr starts plugin commands with a minimal PATH, and the daemon is
            // started by herdr — so a program that works in a shell can be absent
            // here. Naming the PATH searched is what makes that findable.
            CommandError::NotFound { program, path } => write!(
                f,
                "cannot run {program:?}: it is not on the PATH this process has, which is \
                 {path:?}. Give [rewrite] command an absolute path, or start herdr from a shell \
                 where it is on the PATH"
            ),
            CommandError::Failed {
                program,
                code,
                stderr,
            } => write!(f, "{program:?} failed ({code}): {stderr}"),
            CommandError::Silent { program } => write!(
                f,
                "{program:?} printed no rewritten text. Run it by hand on the take to see what \
                 it says"
            ),
        }
    }
}

impl std::error::Error for CommandError {}

/// How much of a program's complaint a message carries. Enough to name the cause,
/// short enough to read.
const STDERR_LIMIT: usize = 400;

impl CommandEngine {
    pub fn rewrite(&self, transcript: &str, bias: &str) -> Result<String, CommandError> {
        let rendered = render(&self.argv, transcript, bias);
        let (program, arguments) = rendered.split_first().ok_or_else(|| CommandError::Silent {
            program: String::new(),
        })?;

        let output = Command::new(program).args(arguments).output();
        let output = match output {
            Ok(output) => output,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Err(CommandError::NotFound {
                    program: program.clone(),
                    path: std::env::var("PATH").unwrap_or_default(),
                })
            }
            Err(e) => {
                return Err(CommandError::Failed {
                    program: program.clone(),
                    code: "could not start".to_string(),
                    stderr: e.to_string(),
                })
            }
        };

        if !output.status.success() {
            let mut stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            stderr.truncate(STDERR_LIMIT);
            if stderr.is_empty() {
                stderr = "it printed nothing on standard error".to_string();
            }
            return Err(CommandError::Failed {
                program: program.clone(),
                code: match output.status.code() {
                    Some(code) => format!("exit {code}"),
                    None => "killed by a signal".to_string(),
                },
                stderr,
            });
        }

        let rewritten = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if rewritten.is_empty() {
            return Err(CommandError::Silent {
                program: program.clone(),
            });
        }
        Ok(rewritten)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn argv(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn each_placeholder_is_replaced() {
        let rendered = render(
            &argv(&["prog", "--fix", "{transcript}", "--context", "{bias}"]),
            "hello there",
            "recent terms: pull request",
        );
        assert_eq!(
            rendered,
            argv(&["prog", "--fix", "hello there", "--context", "recent terms: pull request"])
        );
    }

    #[test]
    fn a_list_that_never_asks_for_the_transcript_gets_it_appended() {
        let rendered = render(&argv(&["prog"]), "hello there", "");
        assert_eq!(rendered, argv(&["prog", "hello there"]));
    }

    #[test]
    fn a_list_with_no_bias_placeholder_does_not_gain_one() {
        let rendered = render(&argv(&["prog", "{transcript}"]), "hello there", "recent terms");
        assert_eq!(rendered, argv(&["prog", "hello there"]));
    }

    #[test]
    fn the_transcript_reaches_the_program() {
        let engine = CommandEngine::new(argv(&["echo", "{transcript}"]));
        let rewritten = engine.rewrite("hello there", "").expect("text");
        assert_eq!(rewritten, "hello there");
    }

    #[test]
    fn a_nonexistent_program_names_the_path_searched() {
        let engine = CommandEngine::new(argv(&["definitely-not-a-program-here"]));
        let error = engine.rewrite("hello there", "").expect_err("must fail");
        let message = error.to_string();
        assert!(message.contains("definitely-not-a-program-here"), "got {message}");
        assert!(message.contains("PATH"), "got {message}");
    }

    #[test]
    fn a_program_that_prints_nothing_is_not_an_empty_success() {
        let engine = CommandEngine::new(argv(&["true"]));
        let error = engine.rewrite("hello there", "").expect_err("must fail");
        assert!(error.to_string().contains("no rewritten text"), "got {error}");
    }
}
