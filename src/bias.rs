//! Biasing recognition with what the agent is talking about.
//!
//! `[context] source` selects how the conversation component is gathered: from
//! the target agent's transcript, from the pane's screen, or automatically —
//! transcript first, falling back to the pane on a miss. File and directory
//! names are collected independent of `source`. See `tasks/21/DESIGN_21.md`,
//! sections 1 and 2.

pub mod files;
pub mod pane;
pub mod source;
pub mod transcript;

/// The three values `[context] source` can resolve to. `Auto` is a member in
/// its own right, not an absence of one — `bias::source::resolve("auto")`
/// returns `Ok(Source::Auto)`, and nothing downstream reconstructs a third
/// state some other way (`tasks/21/DESIGN_21.md`, section 2a).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Transcript,
    Pane,
    Auto,
}

/// What a take's bias-string assembly produced. `bias` is the only field that
/// ever holds conversation or file content — the other fields are counts and
/// flags a caller can log without reading it (AC-9).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Collected {
    /// The finished, capped string — AC-6's interface. Never logged.
    pub bias: String,
    /// Each source actually tried, and whether it found anything. Entries are
    /// always `Transcript` or `Pane` — never `Auto`, which names a mode
    /// `collect` runs under, not a call it makes.
    pub attempted: Vec<(Source, bool)>,
    pub file_count: usize,
    /// How many characters the file-names component contributed, before the
    /// cap. Beyond `DESIGN_21.md`'s literal `Collected` fields — see
    /// `tasks/21/PLAN_21.md`, Task 7.
    pub file_chars: usize,
    /// How many characters the conversation component contributed, before the
    /// cap. Same rationale as `file_chars`.
    pub conversation_chars: usize,
    /// True iff the pre-cut length exceeded `prompt_chars`.
    pub truncated: bool,
}

/// What `collect` needs to assemble a bias string for one take.
pub struct CollectInput<'a> {
    pub source: Source,
    pub cwd: &'a str,
    pub agent: Option<&'a str>,
    pub pane: &'a str,
    pub transcript_root: &'a std::path::Path,
    pub herdr_binary: &'a str,
    pub conversation_turns: usize,
    pub file_names: usize,
    pub prompt_chars: usize,
}

/// A conversation component and whether it was found. `found` is
/// post-filter: a transcript file that exists but whose turns are all
/// filtered out counts as a miss, the same as no file at all.
struct Conversation {
    text: String,
    found: bool,
}

fn read_transcript(input: &CollectInput) -> Conversation {
    let turns = transcript::find(input.cwd, input.agent, input.transcript_root)
        .map(|path| transcript::read_turns(&path, input.conversation_turns))
        .unwrap_or_default();
    Conversation {
        found: !turns.is_empty(),
        text: turns.join("\n"),
    }
}

fn read_pane(input: &CollectInput) -> Conversation {
    match pane::read(input.pane, pane::PANE_LINES, input.herdr_binary) {
        Ok(text) if !text.is_empty() => Conversation { found: true, text },
        _ => Conversation {
            found: false,
            text: String::new(),
        },
    }
}

/// Assembles and caps the bias string, dispatching on `input.source` exactly
/// as `tasks/21/DESIGN_21.md` section 2 states. Never fails: a miss on every
/// source still yields a files-only bias (design section 8).
///
/// Not called from `main` yet — Task 10 wires it into `dictate` once #22
/// merges (`tasks/21/PLAN_21.md`). CI runs clippy with `-D warnings`, so an
/// unreached `pub` item in a binary crate must be allowed explicitly rather
/// than left to warn.
#[allow(dead_code)]
pub fn collect(input: CollectInput) -> Collected {
    let file_names = files::collect(input.cwd, input.file_names);
    let file_count = file_names.len();
    let file_line = file_names.join(" ");
    let file_chars = file_line.chars().count();

    let mut attempted = Vec::new();
    let conversation = match input.source {
        Source::Transcript => {
            let result = read_transcript(&input);
            attempted.push((Source::Transcript, result.found));
            result
        }
        Source::Pane => {
            let result = read_pane(&input);
            attempted.push((Source::Pane, result.found));
            result
        }
        Source::Auto => {
            let transcript_result = read_transcript(&input);
            attempted.push((Source::Transcript, transcript_result.found));
            if transcript_result.found {
                transcript_result
            } else {
                let pane_result = read_pane(&input);
                attempted.push((Source::Pane, pane_result.found));
                pane_result
            }
        }
    };

    let conversation_chars = conversation.text.chars().count();
    let raw = format!("{file_line}\n{}", conversation.text);
    let raw_len = raw.chars().count();
    let truncated = raw_len > input.prompt_chars;
    let bias: String = raw.chars().take(input.prompt_chars).collect();

    Collected {
        bias,
        attempted,
        file_count,
        file_chars,
        conversation_chars,
        truncated,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::process::Command;

    fn scratch(tag: &str) -> PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "herdr-voice-bias-collect-{tag}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("create scratch");
        path
    }

    fn run(dir: &std::path::Path, args: &[&str]) {
        let status = Command::new("git")
            .current_dir(dir)
            .args(args)
            .status()
            .expect("run git");
        assert!(status.success(), "git {args:?} failed");
    }

    /// A scratch git repository, so `files::collect` finds something real
    /// without needing the actual checkout this test runs in.
    fn git_repo(dir: &std::path::Path) {
        run(dir, &["init", "-q"]);
        run(dir, &["config", "user.email", "test@example.invalid"]);
        run(dir, &["config", "user.name", "Test"]);
        std::fs::write(dir.join("notes.txt"), "content").unwrap();
        run(dir, &["add", "."]);
        run(dir, &["commit", "-q", "-m", "add a file"]);
    }

    /// A transcript root with a fixture `.jsonl` under the project directory
    /// that `cwd` slugifies to, holding one distinctive turn.
    fn transcript_root_with_fixture(root: &std::path::Path, cwd: &str, text: &str) {
        let slug: String = cwd
            .chars()
            .map(|c| {
                if c == '/' || c == '.' || c == '@' {
                    '-'
                } else {
                    c
                }
            })
            .collect();
        let project = root.join(slug);
        std::fs::create_dir_all(&project).unwrap();
        std::fs::write(
            project.join("session.jsonl"),
            format!(r#"{{"type":"user","message":{{"content":{text:?}}}}}"#),
        )
        .unwrap();
    }

    const MISSING_HERDR: &str = "/definitely/not/a/real/herdr-binary";

    #[test]
    fn transcript_source_on_a_miss_attempts_only_transcript() {
        let cwd = scratch("t-miss-cwd");
        let transcript_root = scratch("t-miss-root");
        let input = CollectInput {
            source: Source::Transcript,
            cwd: cwd.to_str().unwrap(),
            agent: Some(transcript::KNOWN_TRANSCRIPT_AGENT),
            pane: "w1:p1",
            transcript_root: &transcript_root,
            herdr_binary: MISSING_HERDR,
            conversation_turns: 6,
            file_names: 40,
            prompt_chars: 600,
        };
        let collected = collect(input);
        assert_eq!(collected.attempted, vec![(Source::Transcript, false)]);
    }

    #[test]
    fn pane_source_on_a_miss_attempts_only_pane() {
        let cwd = scratch("p-miss-cwd");
        let transcript_root = scratch("p-miss-root");
        let input = CollectInput {
            source: Source::Pane,
            cwd: cwd.to_str().unwrap(),
            agent: Some(transcript::KNOWN_TRANSCRIPT_AGENT),
            pane: "w1:p1",
            transcript_root: &transcript_root,
            herdr_binary: MISSING_HERDR,
            conversation_turns: 6,
            file_names: 40,
            prompt_chars: 600,
        };
        let collected = collect(input);
        assert_eq!(collected.attempted, vec![(Source::Pane, false)]);
    }

    #[test]
    fn auto_tries_both_on_a_double_miss() {
        let cwd = scratch("auto-miss-cwd");
        let transcript_root = scratch("auto-miss-root");
        let input = CollectInput {
            source: Source::Auto,
            cwd: cwd.to_str().unwrap(),
            agent: Some(transcript::KNOWN_TRANSCRIPT_AGENT),
            pane: "w1:p1",
            transcript_root: &transcript_root,
            herdr_binary: MISSING_HERDR,
            conversation_turns: 6,
            file_names: 40,
            prompt_chars: 600,
        };
        let collected = collect(input);
        assert_eq!(
            collected.attempted,
            vec![(Source::Transcript, false), (Source::Pane, false)]
        );
    }

    #[test]
    fn auto_does_not_try_the_pane_when_the_transcript_hits() {
        let cwd = scratch("auto-hit-cwd");
        let transcript_root = scratch("auto-hit-root");
        let cwd_str = cwd.to_str().unwrap();
        transcript_root_with_fixture(
            &transcript_root,
            cwd_str,
            "a distinctive note about a neutral project term",
        );
        let input = CollectInput {
            source: Source::Auto,
            cwd: cwd_str,
            agent: Some(transcript::KNOWN_TRANSCRIPT_AGENT),
            pane: "w1:p1",
            transcript_root: &transcript_root,
            herdr_binary: MISSING_HERDR,
            conversation_turns: 6,
            file_names: 40,
            prompt_chars: 600,
        };
        let collected = collect(input);
        assert_eq!(collected.attempted, vec![(Source::Transcript, true)]);
        assert!(
            collected
                .bias
                .contains("a distinctive note about a neutral project term"),
            "got {:?}",
            collected.bias
        );
    }

    #[test]
    fn a_miss_on_every_source_still_yields_a_files_only_bias() {
        let cwd = scratch("files-only-cwd");
        git_repo(&cwd);
        let transcript_root = scratch("files-only-root");
        let input = CollectInput {
            source: Source::Auto,
            cwd: cwd.to_str().unwrap(),
            agent: Some(transcript::KNOWN_TRANSCRIPT_AGENT),
            pane: "w1:p1",
            transcript_root: &transcript_root,
            herdr_binary: MISSING_HERDR,
            conversation_turns: 6,
            file_names: 40,
            prompt_chars: 600,
        };
        let collected = collect(input);
        assert_eq!(
            collected.attempted,
            vec![(Source::Transcript, false), (Source::Pane, false)]
        );
        assert!(collected.file_count > 0, "got {collected:?}");
        assert!(collected.bias.contains("notes.txt"), "got {collected:?}");
        assert_eq!(collected.conversation_chars, 0);
    }

    #[test]
    fn a_long_conversation_is_truncated_and_the_count_exceeds_the_cap() {
        let cwd = scratch("long-cwd");
        let transcript_root = scratch("long-root");
        let cwd_str = cwd.to_str().unwrap();
        let long_text = "a fairly long sentence about a project ".repeat(10);
        transcript_root_with_fixture(&transcript_root, cwd_str, &long_text);
        let input = CollectInput {
            source: Source::Transcript,
            cwd: cwd_str,
            agent: Some(transcript::KNOWN_TRANSCRIPT_AGENT),
            pane: "w1:p1",
            transcript_root: &transcript_root,
            herdr_binary: MISSING_HERDR,
            conversation_turns: 6,
            file_names: 40,
            prompt_chars: 20,
        };
        let collected = collect(input);
        assert!(collected.truncated);
        assert!(
            collected.conversation_chars > 20,
            "got {}",
            collected.conversation_chars
        );
    }

    #[test]
    fn a_short_result_leaves_truncated_false() {
        let cwd = scratch("short-cwd");
        let transcript_root = scratch("short-root");
        let cwd_str = cwd.to_str().unwrap();
        transcript_root_with_fixture(&transcript_root, cwd_str, "short");
        let input = CollectInput {
            source: Source::Transcript,
            cwd: cwd_str,
            agent: Some(transcript::KNOWN_TRANSCRIPT_AGENT),
            pane: "w1:p1",
            transcript_root: &transcript_root,
            herdr_binary: MISSING_HERDR,
            conversation_turns: 6,
            file_names: 40,
            prompt_chars: 600,
        };
        let collected = collect(input);
        assert!(!collected.truncated);
    }
}
