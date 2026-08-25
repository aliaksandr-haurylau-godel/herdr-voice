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
pub fn find(cwd: &str, agent: Option<&str>, root: &Path) -> Option<PathBuf> {
    if agent != Some(KNOWN_TRANSCRIPT_AGENT) {
        return None;
    }
    let mut dir = cwd.to_string();
    loop {
        let project = root.join(slugify(&dir));
        if project.is_dir() {
            if let Some(newest) = newest_jsonl(&project) {
                return Some(newest);
            }
        }
        if dir == "/" || dir.is_empty() {
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

fn slugify(cwd: &str) -> String {
    cwd.chars()
        .map(|c| if c == '/' || c == '.' || c == '@' { '-' } else { c })
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
        turns.push(format!("{role}: {text}"));
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
    SERVICE_MARKERS.iter().any(|marker| trimmed.starts_with(marker))
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
