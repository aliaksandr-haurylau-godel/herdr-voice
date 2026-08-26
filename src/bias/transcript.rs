//! Finding and reading the target agent's transcript file, filtered of
//! service turns.
//!
//! Discovery is a directory search alone, against a transcript root the
//! caller computes and passes in — no session-id lookup, no herdr call
//! (`tasks/21/DESIGN_21.md`, section 3).

use std::path::{Path, PathBuf};

/// The one agent whose transcript location is known, deliberately separate
/// from `doctor::AGENT_CANDIDATES` (`src/doctor.rs`) — the two lists answer
/// different questions and only name the same value today because the same
/// agent is the only one measured for both (`tasks/21/DESIGN_21.md`, section
/// 4).
pub const KNOWN_TRANSCRIPT_AGENT: &str = "claude";

/// Turns whose text begins with one of these are machine turns — task
/// notifications, system reminders, cross-session messages, a command
/// wrapper — and carry nothing about speech (AC-2).
/// How much of one turn is kept, in characters. The prototype's number
/// (`spike/context.sh:107`), carried over unchanged: a single measured turn ran
/// to 7000 characters, more than the whole `prompt_chars` budget on its own.
const TURN_CHARS: usize = 300;

const SERVICE_MARKERS: &[&str] = &[
    "<task-notification>",
    "<system-reminder>",
    "<cross-session-message>",
    "<local-command>",
    "<command-name>",
];

/// Finds the target agent's transcript file. `None` when `agent` isn't the
/// known one — no directory is derived, no search runs — or when no
/// directory under `root` is found by walking up from `cwd`.
///
/// The walk stops at the repository root: above it, the first directory that
/// happens to exist under `root` belongs to somebody else's session.
pub fn find(cwd: &str, agent: Option<&str>, root: &Path) -> Option<PathBuf> {
    if agent != Some(KNOWN_TRANSCRIPT_AGENT) {
        return None;
    }
    let ceiling = repository_ceiling(cwd);
    let mut dir = cwd.to_string();
    loop {
        // A working directory that names no directory of its own slugifies to
        // `root` itself, or to a name resolved against the daemon's own
        // process — either way it would hand back a transcript belonging to
        // nothing this take is about.
        if dir.is_empty() || dir == "." {
            return None;
        }
        let project = root.join(slugify(&dir));
        if project.is_dir() {
            if let Some(newest) = newest_jsonl(&project) {
                return Some(newest);
            }
        }
        if dir == "/" || at_ceiling(&dir, ceiling.as_deref()) {
            return None;
        }
        match Path::new(&dir).parent() {
            Some(parent) if parent.to_string_lossy() != dir => {
                dir = parent.to_string_lossy().into_owned();
            }
            _ => return None,
        }
    }
}

/// The repository root the walk may not climb above, resolved through symbolic
/// links so that it can be compared with a directory the walk is holding.
fn repository_ceiling(cwd: &str) -> Option<PathBuf> {
    let root = crate::bias::files::toplevel(cwd)?;
    std::fs::canonicalize(root).ok()
}

/// Whether the walk has reached that ceiling. A directory that cannot be
/// resolved — one that does not exist — is never the ceiling, which leaves a
/// working directory outside any repository walking up as before.
fn at_ceiling(dir: &str, ceiling: Option<&Path>) -> bool {
    match (ceiling, std::fs::canonicalize(dir)) {
        (Some(ceiling), Ok(resolved)) => resolved == ceiling,
        _ => false,
    }
}

fn slugify(cwd: &str) -> String {
    cwd.chars()
        .map(|c| {
            if c == '/' || c == '.' || c == '@' {
                '-'
            } else {
                c
            }
        })
        .collect()
}

fn newest_jsonl(dir: &Path) -> Option<PathBuf> {
    std::fs::read_dir(dir)
        .ok()?
        .filter_map(Result::ok)
        .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "jsonl"))
        .filter_map(|entry| {
            let modified = entry.metadata().ok()?.modified().ok()?;
            Some((modified, entry.path()))
        })
        .max_by_key(|(modified, _)| *modified)
        .map(|(_, path)| path)
}

/// Reads the last `max` `user`/`assistant` turns from `path`, filtered of
/// service turns. A missing or empty file yields no turns — not an error.
pub fn read_turns(path: &Path, max: usize) -> Vec<String> {
    let Ok(content) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let mut turns = Vec::new();
    for line in content.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        let Some(role) = value.get("type").and_then(|v| v.as_str()) else {
            continue;
        };
        if role != "user" && role != "assistant" {
            continue;
        }
        let Some(text) = extract_text(value.get("message").and_then(|m| m.get("content"))) else {
            continue;
        };
        if text.is_empty() || is_service_turn(&text) {
            continue;
        }
        // Cut, then collapse — the prototype's order (`spike/context.sh:107`).
        // Both matter: one turn can otherwise fill the whole budget, and a
        // newline inside a turn would split a line the reader treats as one.
        let text: String = text.chars().take(TURN_CHARS).collect();
        turns.push(format!("{role}: {}", text.replace('\n', " ")));
    }
    let start = turns.len().saturating_sub(max);
    turns[start..].to_vec()
}

fn extract_text(content: Option<&serde_json::Value>) -> Option<String> {
    match content {
        Some(serde_json::Value::String(s)) => Some(s.clone()),
        Some(serde_json::Value::Array(items)) => {
            let joined: Vec<String> = items
                .iter()
                .filter(|item| item.get("type").and_then(|t| t.as_str()) == Some("text"))
                .filter_map(|item| item.get("text").and_then(|t| t.as_str()))
                .map(str::to_string)
                .collect();
            Some(joined.join(" "))
        }
        _ => None,
    }
}

fn is_service_turn(text: &str) -> bool {
    let trimmed = text.trim_start();
    SERVICE_MARKERS
        .iter()
        .any(|marker| trimmed.starts_with(marker))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "herdr-voice-bias-transcript-{tag}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("create scratch");
        path
    }

    fn write_jsonl(dir: &Path, name: &str, lines: &[&str]) -> PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, lines.join("\n")).expect("write fixture");
        path
    }

    const FIXTURE_TURNS: &[&str] = &[
        r#"{"type":"user","message":{"content":"a note about a neutral project term"}}"#,
        r#"{"type":"assistant","message":{"content":"acknowledged"}}"#,
    ];

    #[test]
    fn a_fixture_transcript_is_found_by_directory() {
        let root = scratch("found");
        let project = root.join("-work-example-project");
        std::fs::create_dir_all(&project).unwrap();
        let fixture = write_jsonl(&project, "session.jsonl", FIXTURE_TURNS);

        let found = find("/work/example/project", Some("claude"), &root);
        assert_eq!(found, Some(fixture));
    }

    #[test]
    fn an_agent_other_than_the_known_one_is_never_searched_for() {
        let root = scratch("wrong-agent");
        let project = root.join("-work-example-project");
        std::fs::create_dir_all(&project).unwrap();
        write_jsonl(&project, "session.jsonl", FIXTURE_TURNS);

        assert_eq!(find("/work/example/project", Some("codex"), &root), None);
        assert_eq!(find("/work/example/project", None, &root), None);
    }

    #[test]
    fn no_project_directory_under_root_finds_nothing() {
        let root = scratch("no-project");
        assert_eq!(find("/work/example/project", Some("claude"), &root), None);
    }

    #[test]
    fn walking_up_finds_the_first_existing_directory() {
        let root = scratch("walk-up");
        let project = root.join("-work-example-project");
        std::fs::create_dir_all(&project).unwrap();
        let fixture = write_jsonl(&project, "session.jsonl", FIXTURE_TURNS);

        // Only the parent directory exists under root — the pane's cwd is a
        // subdirectory of the project, and discovery must walk up to find it.
        let found = find("/work/example/project/sub/deeper", Some("claude"), &root);
        assert_eq!(found, Some(fixture));
    }

    #[test]
    fn a_working_directory_that_names_nothing_finds_nothing() {
        let root = scratch("no-cwd");
        // A stray transcript sitting in the root itself, which an empty or
        // relative working directory would otherwise slugify straight onto.
        write_jsonl(&root, "session.jsonl", FIXTURE_TURNS);

        assert_eq!(find("", Some("claude"), &root), None);
        assert_eq!(find(".", Some("claude"), &root), None);
    }

    #[test]
    fn the_walk_stops_at_the_repository_root() {
        let root = scratch("ceiling-root");
        let repo = scratch("ceiling-repo");
        for args in [
            vec!["init", "-q"],
            vec!["config", "user.email", "test@example.invalid"],
            vec!["config", "user.name", "Test"],
        ] {
            let status = std::process::Command::new("git")
                .current_dir(&repo)
                .args(&args)
                .status()
                .expect("run git");
            assert!(status.success(), "git {args:?} failed");
        }
        let inner = repo.join("sub");
        std::fs::create_dir_all(&inner).unwrap();

        // A session directory exists for the repository's parent — the case
        // the home directory presents on a real machine. It belongs to
        // whatever else lives there, not to this repository.
        let parent = repo
            .parent()
            .expect("a parent")
            .to_string_lossy()
            .to_string();
        let elsewhere = root.join(slugify(&parent));
        std::fs::create_dir_all(&elsewhere).unwrap();
        write_jsonl(&elsewhere, "session.jsonl", FIXTURE_TURNS);

        assert_eq!(find(&inner.to_string_lossy(), Some("claude"), &root), None);
    }

    #[test]
    fn service_turns_are_excluded_from_count_and_content() {
        let root = scratch("service-turns");
        let lines = [
            r#"{"type":"user","message":{"content":"<system-reminder>internal note</system-reminder>"}}"#,
            r#"{"type":"assistant","message":{"content":"a real reply about the project"}}"#,
            r#"{"type":"user","message":{"content":"<task-notification>background task done</task-notification>"}}"#,
        ];
        let path = write_jsonl(&root, "session.jsonl", &lines);

        let turns = read_turns(&path, 10);
        assert_eq!(turns, vec!["assistant: a real reply about the project"]);
    }

    #[test]
    fn a_long_turn_is_cut_and_its_newlines_collapsed() {
        let root = scratch("long-turn");
        // The newline falls at character 100, inside the 300 the cut keeps —
        // put past the cut it would be removed before the collapse ran, and
        // the assertion below would pass on the cut alone.
        let long = format!("{}\n{}", "word ".repeat(20), "word ".repeat(200));
        let path = write_jsonl(
            &root,
            "session.jsonl",
            &[&format!(
                r#"{{"type":"user","message":{{"content":{long:?}}}}}"#
            )],
        );

        let turns = read_turns(&path, 6);
        assert_eq!(turns.len(), 1, "got {turns:?}");
        // "user: " plus the prototype's 300 characters — the cut — and
        // nothing that would split the joined string into a second line —
        // the collapse. The two are proved separately: the length holds
        // whether or not newlines are collapsed, and the newline is inside
        // what the cut keeps.
        assert_eq!(turns[0].chars().count(), "user: ".len() + TURN_CHARS);
        assert!(!turns[0].contains('\n'), "got {:?}", turns[0]);
    }

    #[test]
    fn only_the_last_max_filtered_turns_are_kept() {
        let root = scratch("cap");
        let lines: Vec<String> = (0..5)
            .map(|i| format!(r#"{{"type":"user","message":{{"content":"turn {i}"}}}}"#))
            .collect();
        let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
        let path = write_jsonl(&root, "session.jsonl", &refs);

        let turns = read_turns(&path, 2);
        assert_eq!(turns, vec!["user: turn 3", "user: turn 4"]);
    }

    #[test]
    fn a_missing_file_yields_no_turns() {
        let root = scratch("missing");
        let turns = read_turns(&root.join("absent.jsonl"), 10);
        assert!(turns.is_empty());
    }

    #[test]
    fn an_empty_file_yields_no_turns() {
        let root = scratch("empty");
        let path = write_jsonl(&root, "session.jsonl", &[]);
        let turns = read_turns(&path, 10);
        assert!(turns.is_empty());
    }

    #[test]
    fn array_shaped_content_is_joined_from_its_text_items() {
        let root = scratch("array");
        let lines = [
            r#"{"type":"assistant","message":{"content":[{"type":"text","text":"first part"},{"type":"tool_use","id":"x"},{"type":"text","text":"second part"}]}}"#,
        ];
        let path = write_jsonl(&root, "session.jsonl", &lines);

        let turns = read_turns(&path, 10);
        assert_eq!(turns, vec!["assistant: first part second part"]);
    }
}
