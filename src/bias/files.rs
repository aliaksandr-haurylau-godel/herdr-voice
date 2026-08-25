//! Recently touched file and directory names, from the repository the target
//! agent works in.
//!
//! A direct port of `spike/context.sh:57-62`: `git status --porcelain` for
//! working-tree changes, `git log -30 --name-only` for recent commits, split
//! into path components — not basenames alone, since a directory name can be
//! the term recognition needs (`docs/evidence.md`, "Context and its effect on
//! the transcript") — deduplicated in order, capped.

use std::process::Command;

/// File and directory names for the repository containing `cwd`, newest first,
/// capped at `max`. Empty when `cwd` is outside a git repository.
pub fn collect(cwd: &str, max: usize) -> Vec<String> {
    let Some(root) = toplevel(cwd) else {
        return Vec::new();
    };

    let mut paths = Vec::new();
    paths.extend(status_paths(&root));
    paths.extend(log_paths(&root));

    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for path in paths {
        for component in path.split('/') {
            if component.is_empty() {
                continue;
            }
            if seen.insert(component.to_string()) {
                out.push(component.to_string());
                if out.len() >= max {
                    return out;
                }
            }
        }
    }
    out
}

fn toplevel(cwd: &str) -> Option<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(cwd)
        .arg("rev-parse")
        .arg("--show-toplevel")
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let root = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if root.is_empty() {
        return None;
    }
    Some(root)
}

fn status_paths(root: &str) -> Vec<String> {
    let Ok(output) = Command::new("git")
        .arg("-C")
        .arg(root)
        .arg("status")
        .arg("--porcelain")
        .output()
    else {
        return Vec::new();
    };
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| line.split_whitespace().last())
        .map(str::to_string)
        .collect()
}

fn log_paths(root: &str) -> Vec<String> {
    let Ok(output) = Command::new("git")
        .arg("-C")
        .arg(root)
        .arg("log")
        .arg("-30")
        .arg("--name-only")
        .arg("--pretty=format:")
        .output()
    else {
        return Vec::new();
    };
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(str::to_string)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn scratch(tag: &str) -> PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!("herdr-voice-bias-files-{tag}-{}", std::process::id()));
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

    fn init_repo(dir: &std::path::Path) {
        run(dir, &["init", "-q"]);
        run(dir, &["config", "user.email", "test@example.invalid"]);
        run(dir, &["config", "user.name", "Test"]);
    }

    #[test]
    fn a_nested_directory_component_survives() {
        let dir = scratch("nested");
        init_repo(&dir);
        std::fs::create_dir_all(dir.join("sub/inner")).unwrap();
        std::fs::write(dir.join("sub/inner/note.txt"), "content").unwrap();
        run(&dir, &["add", "."]);
        run(&dir, &["commit", "-q", "-m", "add a nested file"]);

        let names = collect(dir.to_str().unwrap(), 40);
        assert!(names.contains(&"sub".to_string()), "got {names:?}");
        assert!(names.contains(&"inner".to_string()), "got {names:?}");
        assert!(names.contains(&"note.txt".to_string()), "got {names:?}");
    }

    #[test]
    fn outside_a_repository_the_result_is_empty() {
        let dir = scratch("outside");
        let names = collect(dir.to_str().unwrap(), 40);
        assert!(names.is_empty(), "got {names:?}");
    }

    #[test]
    fn more_entries_than_max_are_truncated() {
        let dir = scratch("many");
        init_repo(&dir);
        for i in 0..10 {
            std::fs::write(dir.join(format!("file-{i}.txt")), "content").unwrap();
        }
        run(&dir, &["add", "."]);
        run(&dir, &["commit", "-q", "-m", "add many files"]);

        let names = collect(dir.to_str().unwrap(), 3);
        assert_eq!(names.len(), 3, "got {names:?}");
    }
}
