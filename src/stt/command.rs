//! Transcribing by running somebody else's program.
//!
//! The argument list is theirs, not ours: `whisper-cli` takes the audio behind
//! `-f`, the model behind `-m` and the language behind `-l`, and the next program
//! will want something else. See `tasks/13/DESIGN_13.md`, section 2.

use std::fmt;
use std::path::Path;
use std::process::Command;

use super::{Engine, EngineError};

/// Replace the placeholders, and append the audio path when the list never asks
/// for it — so a program that simply takes a file still works.
pub fn render(
    argv: &[String],
    audio: &Path,
    model: Option<&Path>,
    language: &str,
    bias: &str,
) -> Vec<String> {
    let audio = audio.to_string_lossy();
    let model = model.map(|m| m.to_string_lossy().into_owned());
    let mut rendered: Vec<String> = argv
        .iter()
        .map(|argument| {
            let mut out = argument.replace("{audio}", &audio);
            if let Some(model) = &model {
                out = out.replace("{model}", model);
            }
            // `auto` is substituted like any other value: it is what these programs
            // already take to mean "detect it".
            out = out.replace("{language}", language);
            // Unlike {audio}, a missing {prompt} placeholder is not force-appended:
            // the bias string is an opt-in enhancement, not something the program
            // cannot run without (AC-3).
            out.replace("{prompt}", bias)
        })
        .collect();
    if !argv.iter().any(|argument| argument.contains("{audio}")) {
        rendered.push(audio.into_owned());
    }
    rendered
}

pub struct CommandEngine {
    argv: Vec<String>,
    model: Option<std::path::PathBuf>,
    language: String,
}

impl CommandEngine {
    /// The argument list is kept unrendered: the audio path is not known until a
    /// take finishes, and rendering it early would leave the program pointed at a
    /// placeholder. That mistake was made once here and caught by the test below.
    pub fn new(
        argv: Vec<String>,
        model: Option<std::path::PathBuf>,
        language: String,
    ) -> CommandEngine {
        CommandEngine {
            argv,
            model,
            language,
        }
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
                 {path:?}. Give [stt] command an absolute path, or start herdr from a shell \
                 where it is on the PATH"
            ),
            CommandError::Failed {
                program,
                code,
                stderr,
            } => write!(f, "{program:?} failed ({code}): {stderr}"),
            CommandError::Silent { program } => write!(
                f,
                "{program:?} printed no transcript. Run it by hand on the take to see what \
                 it says"
            ),
        }
    }
}

impl std::error::Error for CommandError {}

/// How much of a program's complaint a message carries. Enough to name the cause,
/// short enough to read.
const STDERR_LIMIT: usize = 400;

impl Engine for CommandEngine {
    fn transcribe(&self, audio: &Path, bias: &str) -> Result<String, EngineError> {
        let rendered = render(
            &self.argv,
            audio,
            self.model.as_deref(),
            &self.language,
            bias,
        );
        let (program, arguments) = rendered.split_first().ok_or(EngineError::NotConfigured)?;

        let output = Command::new(program).args(arguments).output();
        let output = match output {
            Ok(output) => output,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Err(EngineError::Command(CommandError::NotFound {
                    program: program.clone(),
                    path: std::env::var("PATH").unwrap_or_default(),
                }))
            }
            Err(e) => {
                return Err(EngineError::Command(CommandError::Failed {
                    program: program.clone(),
                    code: "could not start".to_string(),
                    stderr: e.to_string(),
                }))
            }
        };

        if !output.status.success() {
            let mut stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            stderr.truncate(STDERR_LIMIT);
            if stderr.is_empty() {
                stderr = "it printed nothing on standard error".to_string();
            }
            return Err(EngineError::Command(CommandError::Failed {
                program: program.clone(),
                code: match output.status.code() {
                    Some(code) => format!("exit {code}"),
                    None => "killed by a signal".to_string(),
                },
                stderr,
            }));
        }

        let transcript = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if transcript.is_empty() {
            return Err(EngineError::Command(CommandError::Silent {
                program: program.clone(),
            }));
        }
        Ok(transcript)
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
            &argv(&[
                "whisper-cli",
                "-m",
                "{model}",
                "-f",
                "{audio}",
                "-l",
                "{language}",
            ]),
            Path::new("/takes/one.wav"),
            Some(Path::new("/models/ggml-tiny.bin")),
            "en",
            "",
        );
        assert_eq!(
            rendered,
            argv(&[
                "whisper-cli",
                "-m",
                "/models/ggml-tiny.bin",
                "-f",
                "/takes/one.wav",
                "-l",
                "en"
            ])
        );
    }

    #[test]
    fn auto_is_substituted_like_any_other_language() {
        let rendered = render(
            &argv(&["prog", "-l", "{language}", "{audio}"]),
            Path::new("/takes/one.wav"),
            None,
            "auto",
            "",
        );
        assert_eq!(rendered, argv(&["prog", "-l", "auto", "/takes/one.wav"]));
    }

    #[test]
    fn a_list_that_never_asks_for_the_audio_gets_it_appended() {
        let rendered = render(
            &argv(&["prog"]),
            Path::new("/takes/one.wav"),
            None,
            "auto",
            "",
        );
        assert_eq!(rendered, argv(&["prog", "/takes/one.wav"]));
    }

    #[test]
    fn the_take_reaches_the_program() {
        // The defect this guards against: the argument list was rendered when the
        // engine was built, before any take existed, so the program was handed a
        // placeholder and never saw the audio.
        let engine = CommandEngine::new(argv(&["echo", "{audio}"]), None, "auto".to_string());
        let transcript = engine
            .transcribe(Path::new("/takes/1787-0.wav"), "")
            .expect("text");
        assert_eq!(transcript, "/takes/1787-0.wav");
    }

    #[test]
    fn a_list_with_no_placeholder_still_receives_the_take() {
        let engine = CommandEngine::new(argv(&["echo"]), None, "auto".to_string());
        assert_eq!(
            engine
                .transcribe(Path::new("/takes/2.wav"), "")
                .expect("text"),
            "/takes/2.wav"
        );
    }

    #[test]
    fn a_program_that_prints_a_transcript_gives_one_back() {
        // The take is passed and ignored, so what is asserted is the trimming and
        // nothing else.
        let engine = CommandEngine::new(
            argv(&["sh", "-c", "printf '  hello there  '", "--", "{audio}"]),
            None,
            "auto".to_string(),
        );
        assert_eq!(
            engine
                .transcribe(Path::new("/takes/one.wav"), "")
                .expect("text"),
            "hello there",
            "surrounding whitespace is trimmed"
        );
    }

    #[test]
    fn a_program_that_is_not_there_names_the_path_it_searched() {
        let engine = CommandEngine::new(
            argv(&["definitely-not-a-program-here"]),
            None,
            "auto".to_string(),
        );
        let error = engine
            .transcribe(Path::new("/takes/one.wav"), "")
            .expect_err("must fail");
        let message = error.to_string();
        assert!(
            message.contains("definitely-not-a-program-here"),
            "got {message}"
        );
        assert!(message.contains("PATH"), "got {message}");
    }

    #[test]
    fn a_program_that_fails_carries_what_it_complained_about() {
        let engine = CommandEngine::new(
            argv(&["sh", "-c", "echo 'model not found' >&2; exit 3"]),
            None,
            "auto".to_string(),
        );
        let error = engine
            .transcribe(Path::new("/takes/one.wav"), "")
            .expect_err("must fail");
        let message = error.to_string();
        assert!(message.contains("exit 3"), "got {message}");
        assert!(message.contains("model not found"), "got {message}");
    }

    #[test]
    fn a_program_that_prints_nothing_is_not_an_empty_success() {
        let engine = CommandEngine::new(argv(&["true"]), None, "auto".to_string());
        let error = engine
            .transcribe(Path::new("/takes/one.wav"), "")
            .expect_err("must fail");
        assert!(error.to_string().contains("no transcript"), "got {error}");
    }

    #[test]
    fn the_prompt_placeholder_is_substituted() {
        let rendered = render(
            &argv(&["whisper-cli", "--prompt", "{prompt}", "{audio}"]),
            Path::new("/takes/one.wav"),
            None,
            "auto",
            "recent terms: pull request, worklog",
        );
        assert_eq!(
            rendered,
            argv(&[
                "whisper-cli",
                "--prompt",
                "recent terms: pull request, worklog",
                "/takes/one.wav"
            ])
        );
    }

    #[test]
    fn a_list_with_no_prompt_placeholder_does_not_gain_one() {
        // Unlike {audio}, an absent {prompt} is not force-appended: the bias
        // string is an opt-in enhancement, not something the program cannot
        // run without (AC-3).
        let rendered = render(
            &argv(&["whisper-cli", "{audio}"]),
            Path::new("/takes/one.wav"),
            None,
            "auto",
            "recent terms: pull request",
        );
        assert_eq!(rendered, argv(&["whisper-cli", "/takes/one.wav"]));
    }

    #[test]
    fn an_empty_bias_substitutes_to_an_empty_string() {
        let rendered = render(
            &argv(&["whisper-cli", "--prompt", "{prompt}", "{audio}"]),
            Path::new("/takes/one.wav"),
            None,
            "auto",
            "",
        );
        assert_eq!(
            rendered,
            argv(&["whisper-cli", "--prompt", "", "/takes/one.wav"])
        );
    }
}
