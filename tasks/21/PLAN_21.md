# PLAN_21 — Context: bias recognition with what the agent is talking about

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Assemble a capped bias string per take — recent conversation content
plus recently touched file and directory names — behind an interface a later
issue can hand to recognition, and log only metadata about it.

**Architecture:** A new `bias` module with one submodule per source of
failure (`source`, `transcript`, `pane`, `files`), holding a `Source` enum
and a `collect` function in `src/bias.rs` that dispatches on `[context]
source` and assembles a `Collected` value. `src/daemon.rs` resolves the
source once at start, calls `bias::collect` per take, and logs `Collected`'s
counts and flags — never the string itself.

**Tech Stack:** Rust, `serde`/`toml` for configuration, `git` and `herdr`
run as external processes, no new dependencies.

**Spec:** `tasks/21/DESIGN_21.md` (closed READY), argued from
`tasks/21/AC_21.md` (closed READY). Both travel with this plan; read
`docs/design.md` sections 4 and 7, and `spike/context.sh`, alongside it.

## Global Constraints

- No panic paths on the context-assembly path (`CLAUDE.md`, "Rules for the
  code"; AC-8).
- Every user-visible failure names what to do next; a miss never fails a
  take — recognition proceeds on whatever was collected (`DESIGN_21.md`
  section 8).
- `[context] source` has exactly three values — `auto`, `transcript`,
  `pane` — default `auto`; an unrecognised value is refused with all three
  named, never silently defaulted, and the take still runs on file names
  alone (`DESIGN_21.md` section 2a).
- Only metadata is logged about the collected string: which sources were
  tried and what each found, how many file names, whether the cap
  truncated. `Collected.bias` — the finished string — is never written to a
  log, on a hit or a miss (AC-9, `DESIGN_21.md` section 8).
- Configuration keys have defaults; an absent configuration file, or a file
  that omits `[context]` or one of its keys, is a valid state (`CLAUDE.md`;
  AC-7).
- Configuration is read once at daemon start, not per take
  (`docs/decisions.md:38`).
- No test needs a microphone, a live herdr, a model or a network
  (`CLAUDE.md`, "Testing"; `DESIGN_21.md` section 11). `bias::pane`'s tests
  run a small script under an explicit `binary` argument, never `herdr`
  itself. `bias::files`'s tests run real `git` against a scratch directory.
  `bias::transcript`'s tests use a scratch directory as the transcript
  root, never the real home directory.
- Device selection and every other existing rule in `CLAUDE.md` is
  unaffected by this run; nothing here touches audio capture.
- Everything written is English; no absolute home path, personal name or
  employer/client name enters any file, comment, test fixture or commit
  message. Cite paths relative to the repository root.

### Dependency on issue #22

This run lands **after** issue #22, which is landing first. #22 replaces
the `Recognition` value threaded through `src/daemon.rs`'s `start`, `serve`,
`serve_one`, `answer`, `dictate` and `transcribe` with one per-daemon
bundle (referred to below as **Runtime**, since its actual name is decided
by #22's own implementation), and extracts from `src/doctor.rs` a helper
that runs the `herdr` binary through `HERDR_BIN_PATH` (defaulting to
`herdr`). #22 does **not** thread the working directory or the agent name
into `dictate`.

Tasks 1 through 7 below build and test the `bias` module and the
`[context]` configuration table entirely on their own — none of them touch
`src/daemon.rs` and none of them depend on #22. **Tasks 8, 9 and 10 touch
`src/daemon.rs` and cannot start until #22 has merged into this branch's
target.** Each of those three tasks says explicitly, in its own text, what
about #22's landed code it needs to be checked against before being
written, because this plan cannot cite line numbers for code that does not
exist yet in this worktree. Do not guess at Runtime's field names from
this document — read the merged `src/daemon.rs` first.

---

### Task 1: Document the fourth `[context]` key

**Files:**
- Modify: `docs/design.md` (section 7, the `[context]` block)

**Interfaces:**
- Consumes: nothing.
- Produces: the documented default configuration block that Task 2's test
  values are checked against.

**Context:** `docs/design.md` section 7 currently lists only three
`[context]` keys (`conversation_turns`, `file_names`, `prompt_chars`).
`DESIGN_21.md` section 10 decides the document gains the fourth,
`source = "auto"`, as part of this stage's own work — the key did not
exist when that section of the design document was written.

- [ ] **Step 1: Add the `source` line to the `[context]` block**

In `docs/design.md`, in the `[context]` block inside section 7
("Configuration"), change:

```toml
[context]
conversation_turns = 6
file_names = 40
prompt_chars = 600
```

to:

```toml
[context]
source = "auto"            # auto | transcript | pane
conversation_turns = 6
file_names = 40
prompt_chars = 600
```

- [ ] **Step 2: Note the pane source in section 4**

In section 4 ("Context"), after the existing bullet list, add one sentence
stating that when no transcript file can be found — or when `[context]
source` is set to `pane` — the conversation component is read instead from
the pane's own screen contents through `herdr pane read`, filtered to lines
containing a letter or digit.

- [ ] **Step 3: Check the edit**

Run: `grep -n "source = \"auto\"" docs/design.md`
Expected: one match, inside the `[context]` block.

- [ ] **Step 4: Commit**

```bash
git add docs/design.md
git commit -m "Document the [context] source key and the pane source"
```

---

### Task 2: The `[context]` configuration table

**Files:**
- Modify: `src/config.rs`
- Test: `src/config.rs` (inline `#[cfg(test)] mod tests`)

**Interfaces:**
- Consumes: nothing beyond `serde`/`toml`, already a dependency.
- Produces: `config::Context` with fields `source: String`,
  `conversation_turns: usize`, `file_names: usize`, `prompt_chars: usize`,
  defaults `"auto"`, `6`, `40`, `600`; `Config.context: Context`. Later
  tasks read these fields; `bias::source::resolve` (Task 3) is what turns
  `Context.source` into a checked value — `config.rs` itself does not
  validate it, the same way `Stt.engine` is not validated here either.

- [ ] **Step 1: Write the failing tests**

Add to `src/config.rs`'s `#[cfg(test)] mod tests`:

```rust
#[test]
fn context_defaults_are_auto_six_forty_six_hundred() {
    let defaults = Config::default();
    assert_eq!(defaults.context.source, "auto");
    assert_eq!(defaults.context.conversation_turns, 6);
    assert_eq!(defaults.context.file_names, 40);
    assert_eq!(defaults.context.prompt_chars, 600);
}

#[test]
fn a_partial_context_table_keeps_the_other_defaults() {
    let directory = scratch("context-partial");
    std::fs::write(
        directory.join("config.toml"),
        "[context]\nsource = \"pane\"\n",
    )
    .unwrap();
    let loaded = load(Some(&directory));
    assert_eq!(loaded.config.context.source, "pane");
    assert_eq!(loaded.config.context.conversation_turns, 6);
    assert_eq!(loaded.config.context.file_names, 40);
    assert_eq!(loaded.config.context.prompt_chars, 600);
    assert!(
        matches!(loaded.source, Source::File(_)),
        "got {:?}",
        loaded.source
    );
}

#[test]
fn an_absent_context_table_yields_all_four_defaults() {
    let directory = scratch("context-absent");
    std::fs::write(directory.join("config.toml"), "[stt]\nmodel = \"small\"\n").unwrap();
    let loaded = load(Some(&directory));
    assert_eq!(loaded.config.context, Context::default());
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --lib config::tests::context_ -- --nocapture`
Expected: FAIL — `Config` has no field `context`, `Context` does not exist.

- [ ] **Step 3: Add the `Context` struct and wire it into `Config`**

In `src/config.rs`, add `pub context: Context` to `Config` (next to
`audio`, `stt`, `rewrite`), and add:

```rust
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default)]
pub struct Context {
    /// `auto`, `transcript` or `pane`. Not validated here — an unrecognised
    /// value is a `bias::source::resolve` concern, not a `config` one, the
    /// same way `Stt.engine` is not validated in this module either.
    pub source: String,
    pub conversation_turns: usize,
    pub file_names: usize,
    pub prompt_chars: usize,
}

impl Default for Context {
    fn default() -> Self {
        Context {
            source: "auto".to_string(),
            conversation_turns: 6,
            file_names: 40,
            prompt_chars: 600,
        }
    }
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test --lib config::`
Expected: PASS, including the pre-existing `every_key_has_a_default` test
(unaffected — it does not assert on `context`).

- [ ] **Step 5: Commit**

```bash
git add src/config.rs
git commit -m "Read the [context] configuration table, with its four defaults"
```

---

### Task 3: `bias::Source` and `bias::source::resolve`

**Files:**
- Create: `src/bias.rs`
- Create: `src/bias/source.rs`
- Modify: `src/main.rs` (register the `bias` module — see Step 3)
- Test: `src/bias/source.rs` (inline)

**Interfaces:**
- Consumes: nothing.
- Produces: `pub enum Source { Transcript, Pane, Auto }` in `src/bias.rs`,
  deriving `Debug, Clone, Copy, PartialEq, Eq`; `pub fn
  source::resolve(value: &str) -> Result<Source, String>` in
  `src/bias/source.rs`, where `Err` carries a message naming all three
  valid values (`auto`, `transcript`, `pane`), the same shape
  `EngineError::Unknown` gives an unrecognised `[stt] engine`
  (`src/stt.rs:52-60`). Tasks 4 through 7 depend on `Source` existing and
  being `Copy` (Task 7's `Collected.attempted: Vec<(Source, bool)>` needs
  to store it by value repeatedly).

- [ ] **Step 1: Write the failing tests**

Create `src/bias/source.rs`:

```rust
//! Turning `[context] source` into a checked value, or refusing it.
//!
//! `auto` is a member of `Source`, not an absence of one: `bias::collect`
//! (src/bias.rs) reads `Source` as one three-way type, so nothing downstream
//! can disagree about what a resolved `auto` means. See
//! `tasks/21/DESIGN_21.md`, section 2a.

use super::Source;

pub fn resolve(value: &str) -> Result<Source, String> {
    match value {
        "auto" => Ok(Source::Auto),
        "transcript" => Ok(Source::Transcript),
        "pane" => Ok(Source::Pane),
        other => Err(format!(
            "unknown [context] source {other:?}; it is one of \"auto\", \"transcript\", \"pane\""
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_three_values_resolve() {
        assert_eq!(resolve("auto"), Ok(Source::Auto));
        assert_eq!(resolve("transcript"), Ok(Source::Transcript));
        assert_eq!(resolve("pane"), Ok(Source::Pane));
    }

    #[test]
    fn an_unrecognised_value_lists_all_three() {
        let error = resolve("vosk").expect_err("must refuse");
        assert!(error.contains("vosk"), "got {error}");
        for name in ["auto", "transcript", "pane"] {
            assert!(error.contains(name), "got {error}");
        }
    }
}
```

This will not compile yet: `Source` does not derive `PartialEq`, and
`super::Source` does not exist.

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test --lib bias::source:: 2>&1 | head -20`
Expected: FAIL to compile — `src/bias.rs` does not exist, `bias` is not a
module of the crate.

- [ ] **Step 3: Create `src/bias.rs` and register the module**

Create `src/bias.rs`:

```rust
//! Assembling a capped bias string for recognition from what the target
//! pane's agent is talking about.
//!
//! Distinct from `src/context.rs`, which parses the invocation herdr passes
//! per request. This module's "context" is the bias-string kind: recent
//! conversation content and recently touched file names. See
//! `tasks/21/DESIGN_21.md`, section 1.

pub mod files;
pub mod pane;
pub mod source;
pub mod transcript;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Transcript,
    Pane,
    Auto,
}
```

In `src/main.rs`, add `mod bias;` alongside the other `mod` declarations
(next to `mod context;` — match the existing ordering and style in that
file).

- [ ] **Step 4: Run the test to verify it passes**

Run: `cargo test --lib bias::`
Expected: PASS (2 tests in `bias::source::tests`).

- [ ] **Step 5: Commit**

```bash
git add src/bias.rs src/bias/source.rs src/main.rs
git commit -m "Resolve [context] source into a checked Source, or refuse it"
```

---

### Task 4: `bias::files::collect`

**Files:**
- Create: `src/bias/files.rs`
- Modify: `src/bias.rs` (the `pub mod files;` line already added in Task 3)
- Test: `src/bias/files.rs` (inline)

**Interfaces:**
- Consumes: nothing from other tasks.
- Produces: `pub fn files::collect(cwd: &str, max: usize) -> Vec<String>` —
  recently touched file and directory names for the repository containing
  `cwd`, newest-first, path components rather than basenames, deduplicated,
  capped at `max` entries; an empty `Vec` when `cwd` is not inside a git
  repository. Task 7's `bias::collect` calls this directly for the
  file-names component, always, regardless of `[context] source`
  (`DESIGN_21.md` section 2, "independent of the file-names component").

**Context:** `DESIGN_21.md` section 6 ports the prototype's two `git`
commands unchanged: `git status --porcelain` for working-tree changes,
`git log -30 --name-only` for recent commits, both run from the repository
root (`git rev-parse --show-toplevel` against `cwd`, not `cwd` itself).

- [ ] **Step 1: Write the failing tests**

Create `src/bias/files.rs`:

```rust
//! Recently touched file and directory names, from `git`.
//!
//! A direct port of `spike/context.sh:57-62`: the term evidence restored in
//! `docs/evidence.md` was a directory name, missing only because an earlier
//! version collected basenames. See `tasks/21/DESIGN_21.md`, section 6.

use std::path::Path;
use std::process::Command;

pub fn collect(cwd: &str, max: usize) -> Vec<String> {
    let Some(root) = toplevel(cwd) else {
        return Vec::new();
    };

    let mut paths: Vec<String> = Vec::new();
    paths.extend(status_paths(&root));
    paths.extend(log_paths(&root));

    let mut seen = std::collections::HashSet::new();
    let mut components: Vec<String> = Vec::new();
    for path in paths {
        for part in split_components(&path) {
            if seen.insert(part.clone()) {
                components.push(part);
            }
        }
    }
    components.truncate(max);
    components
}

fn toplevel(cwd: &str) -> Option<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(cwd)
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}

fn status_paths(root: &str) -> Vec<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["status", "--porcelain"])
        .output();
    let Ok(output) = output else { return Vec::new() };
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| line.split_whitespace().last())
        .map(str::to_string)
        .collect()
}

fn log_paths(root: &str) -> Vec<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["log", "-30", "--name-only", "--pretty=format:"])
        .output();
    let Ok(output) = output else { return Vec::new() };
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect()
}

/// A path like `a/b/c.rs` becomes `["a", "b", "c.rs"]` — intermediate
/// components, not basename alone (AC-4).
fn split_components(path: &str) -> Vec<String> {
    Path::new(path)
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn scratch_repo(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("bias-files-{tag}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        run(&dir, &["init", "-q"]);
        run(&dir, &["config", "user.email", "test@example.com"]);
        run(&dir, &["config", "user.name", "Test"]);
        dir
    }

    fn run(dir: &Path, args: &[&str]) {
        let status = Command::new("git").arg("-C").arg(dir).args(args).status().unwrap();
        assert!(status.success(), "git {args:?} failed");
    }

    #[test]
    fn a_directory_component_survives_not_just_the_basename() {
        let dir = scratch_repo("component");
        std::fs::create_dir_all(dir.join("src/deep")).unwrap();
        std::fs::write(dir.join("src/deep/file.rs"), "x").unwrap();
        run(&dir, &["add", "."]);
        run(&dir, &["commit", "-q", "-m", "add"]);

        let names = collect(dir.to_str().unwrap(), 40);
        assert!(names.contains(&"src".to_string()), "got {names:?}");
        assert!(names.contains(&"deep".to_string()), "got {names:?}");
        assert!(names.contains(&"file.rs".to_string()), "got {names:?}");
    }

    #[test]
    fn outside_a_repository_the_component_is_empty() {
        let dir = std::env::temp_dir().join(format!("bias-files-norepo-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(collect(dir.to_str().unwrap(), 40), Vec::<String>::new());
    }

    #[test]
    fn more_entries_than_the_cap_are_truncated() {
        let dir = scratch_repo("cap");
        for i in 0..5 {
            std::fs::write(dir.join(format!("file{i}.txt")), "x").unwrap();
        }
        run(&dir, &["add", "."]);
        run(&dir, &["commit", "-q", "-m", "add"]);

        let names = collect(dir.to_str().unwrap(), 2);
        assert_eq!(names.len(), 2, "got {names:?}");
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --lib bias::files:: 2>&1 | head -20`
Expected: FAIL to compile — `files` module has no content beyond the
`pub mod files;` declaration (the file did not exist before this step).

- [ ] **Step 3: Run the tests to verify they pass**

The implementation was written in Step 1 alongside the tests (this task's
subject is a pure port of an already-measured mechanism, not something to
discover through failing assertions one at a time). Run:

Run: `cargo test --lib bias::files::`
Expected: PASS (3 tests).

- [ ] **Step 4: Commit**

```bash
git add src/bias/files.rs
git commit -m "Collect recently touched file and directory names for the bias string"
```

---

### Task 5: `bias::pane::argv` and `bias::pane::read`

**Files:**
- Create: `src/bias/pane.rs`
- Test: `src/bias/pane.rs` (inline)

**Interfaces:**
- Consumes: nothing from other tasks.
- Produces: `pub const PANE_LINES: usize = 80;`; `pub fn pane::argv(pane:
  &str, lines: usize) -> Vec<String>` (pure, no process); `pub fn
  pane::read(pane: &str, lines: usize, binary: &str) -> Result<String,
  PaneError>` — runs `binary` with `argv(pane, lines)`'s arguments (minus
  the program name), filters the output (trim trailing whitespace per
  line, drop lines with no letter or digit, keep the last `lines` lines
  after filtering), and returns it. `binary` is an explicit parameter —
  **this task does not resolve `HERDR_BIN_PATH` itself**; that resolution,
  and the helper #22 extracts from `src/doctor.rs` for it, is consumed at
  the call site in Task 10, not here. Task 7's `bias::collect` calls
  `read` when the resolved `Source` requires it.

**Context:** `DESIGN_21.md` section 5 fixes the exact contract:
`herdr pane read "$pane" --source recent --lines "$CTX_LINES" --format
text` (`spike/context.sh:42-45`).

- [ ] **Step 1: Write the failing tests**

Create `src/bias/pane.rs`:

```rust
//! Reading a pane's screen contents through herdr.
//!
//! Split into a pure argument builder and the process that runs it, the
//! same way `src/stt/command.rs` splits `render` from execution — so the
//! exact command line can be checked without a live herdr. See
//! `tasks/21/DESIGN_21.md`, section 5.

use std::fmt;
use std::process::Command;

/// Both the `--lines` value sent to herdr and the number of filtered lines
/// kept: the prototype uses the same figure both places
/// (`spike/context.sh:42-45`). Not a `[context]` key — the pane branch has
/// no budget of its own among the four; the overall `prompt_chars` cap
/// bounds the result together with this.
pub const PANE_LINES: usize = 80;

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
        match self {
            PaneError::NotFound { program } => {
                write!(f, "cannot run {program:?} to read the pane")
            }
            PaneError::Failed { program, code } => {
                write!(f, "{program:?} failed reading the pane ({code})")
            }
        }
    }
}

impl std::error::Error for PaneError {}

pub fn read(pane: &str, lines: usize, binary: &str) -> Result<String, PaneError> {
    let full = argv(pane, lines);
    let arguments = &full[1..];
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
        return Err(PaneError::Failed {
            program: binary.to_string(),
            code: match output.status.code() {
                Some(code) => format!("exit {code}"),
                None => "killed by a signal".to_string(),
            },
        });
    }
    let filtered: Vec<&str> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim_end)
        .filter(|line| line.chars().any(|c| c.is_alphanumeric()))
        .collect();
    let kept: Vec<&str> = filtered
        .iter()
        .rev()
        .take(lines)
        .rev()
        .copied()
        .collect();
    Ok(kept.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn argv_matches_the_prototype_contract() {
        assert_eq!(
            argv("w1:p2", 80),
            vec![
                "herdr", "pane", "read", "w1:p2", "--source", "recent", "--lines", "80",
                "--format", "text"
            ]
        );
    }

    #[test]
    fn a_program_that_prints_lines_is_filtered_and_returned() {
        let script = "printf 'hello world  \\n   \\n---\\nfoo1\\n'";
        let result = read("w1:p2", 80, "sh").unwrap_err(); // "sh" alone needs -c; see below
        let _ = result; // placeholder removed in favor of the explicit case below
        let output = std::process::Command::new("sh")
            .arg("-c")
            .arg(script)
            .output()
            .unwrap();
        assert!(output.status.success());
        // Exercise `read` against a real small script via a temp file, so the
        // `binary` parameter is exactly what production would pass.
        let script_path = std::env::temp_dir()
            .join(format!("bias-pane-print-{}", std::process::id()));
        std::fs::write(&script_path, "#!/bin/sh\nprintf 'hello world  \\n   \\n---\\nfoo1\\n'\n")
            .unwrap();
        make_executable(&script_path);
        let text = read("w1:p2", 80, script_path.to_str().unwrap()).expect("text");
        assert_eq!(text, "hello world\n---\nfoo1");
    }

    #[test]
    fn a_program_that_is_not_there_is_reported() {
        let error = read("w1:p2", 80, "definitely-not-a-program-here").unwrap_err();
        assert!(matches!(error, PaneError::NotFound { .. }), "got {error:?}");
    }

    #[test]
    fn a_program_that_fails_is_reported() {
        let script_path = std::env::temp_dir()
            .join(format!("bias-pane-fail-{}", std::process::id()));
        std::fs::write(&script_path, "#!/bin/sh\nexit 3\n").unwrap();
        make_executable(&script_path);
        let error = read("w1:p2", 80, script_path.to_str().unwrap()).unwrap_err();
        match error {
            PaneError::Failed { code, .. } => assert!(code.contains("3"), "got {code}"),
            other => panic!("got {other:?}"),
        }
    }

    fn make_executable(path: &std::path::Path) {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(path).unwrap().permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(path, perms).unwrap();
        }
    }
}
```

(The first test above has one throwaway line exercising `sh -c` merely to
show a script string can be turned into an output before the real
assertion; remove it in Step 3 in favor of the clean version below — see
the note in that step.)

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --lib bias::pane:: 2>&1 | head -20`
Expected: FAIL to compile — `src/bias/pane.rs` does not exist yet as a
module member (only declared via `pub mod pane;` in Task 3's `src/bias.rs`
with no file content before this task).

- [ ] **Step 3: Clean up the first test and verify the implementation**

Replace `a_program_that_prints_lines_is_filtered_and_returned` with the
clean version (drop the throwaway `sh -c` lines at the top):

```rust
#[test]
fn a_program_that_prints_lines_is_filtered_and_returned() {
    let script_path =
        std::env::temp_dir().join(format!("bias-pane-print-{}", std::process::id()));
    std::fs::write(
        &script_path,
        "#!/bin/sh\nprintf 'hello world  \\n   \\n---\\nfoo1\\n'\n",
    )
    .unwrap();
    make_executable(&script_path);
    let text = read("w1:p2", 80, script_path.to_str().unwrap()).expect("text");
    assert_eq!(text, "hello world\n---\nfoo1");
}
```

Run: `cargo test --lib bias::pane::`
Expected: PASS (4 tests). On Windows this test module is conditionally
built without the `make_executable` `unix`-only body; the plan does not
attempt a Windows-shaped fixture here — see `docs/evidence.md` for what is
proven only by hand.

- [ ] **Step 4: Commit**

```bash
git add src/bias/pane.rs
git commit -m "Read a pane's screen contents through herdr, filtered"
```

---

### Task 6: `bias::transcript::find` and `bias::transcript::read_turns`

**Files:**
- Create: `src/bias/transcript.rs`
- Test: `src/bias/transcript.rs` (inline)

**Interfaces:**
- Consumes: nothing from other tasks.
- Produces: `pub fn transcript::find(cwd: &str, agent: Option<&str>, root:
  &Path) -> Option<PathBuf>` — the newest `.jsonl` file under the project
  directory derived from `cwd`, walking up to the first directory that
  exists under `root`, or `None` when `agent` is not the one known agent
  or no such directory exists; `pub fn transcript::read_turns(path: &Path,
  max: usize) -> Vec<String>` — the last `max` `user`/`assistant` turns
  whose text does not begin with a service-turn marker, formatted
  `"{role}: {text}"`. Task 7's `bias::collect` calls both.

**Context:** `DESIGN_21.md` section 3 drops the session-id lookup entirely
and keeps only the directory search; section 4 gates discovery on one
named agent; the filter list is `<task-notification>`,
`<system-reminder>`, `<cross-session-message>`, `<local-command>`,
`<command-name>` (AC-2, `spike/context.sh:98-108`).

- [ ] **Step 1: Write the failing tests for `find`**

Create `src/bias/transcript.rs`:

```rust
//! Finding and reading the target agent's transcript file.
//!
//! Discovery is a single-shot directory search against an injected root —
//! no session-id lookup, which was measured to name a file that may not
//! exist yet (`spike/context.sh:74-76`). See `tasks/21/DESIGN_21.md`,
//! sections 3 and 4.

use serde::Deserialize;
use std::path::{Path, PathBuf};

/// The one agent whose transcript layout is known — the one measured in
/// `docs/evidence.md`. Deliberately separate from `doctor::AGENT_CANDIDATES`
/// (`src/doctor.rs:20`): the two lists answer different questions and only
/// name the same value today because the same agent is the only one
/// measured for both.
const KNOWN_TRANSCRIPT_AGENT: &str = "claude";

const SERVICE_PREFIXES: &[&str] = &[
    "<task-notification>",
    "<system-reminder>",
    "<cross-session-message>",
    "<local-command>",
    "<command-name>",
];

pub fn find(cwd: &str, agent: Option<&str>, root: &Path) -> Option<PathBuf> {
    if agent != Some(KNOWN_TRANSCRIPT_AGENT) {
        return None;
    }
    let mut dir = PathBuf::from(cwd);
    loop {
        let slug = slugify(dir.to_str()?);
        let project = root.join(slug);
        if project.is_dir() {
            return newest_jsonl(&project);
        }
        if !dir.pop() {
            return None;
        }
        if dir.as_os_str().is_empty() {
            return None;
        }
    }
}

fn slugify(path: &str) -> String {
    path.chars()
        .map(|c| if c == '/' || c == '.' || c == '@' { '-' } else { c })
        .collect()
}

fn newest_jsonl(dir: &Path) -> Option<PathBuf> {
    std::fs::read_dir(dir)
        .ok()?
        .filter_map(Result::ok)
        .filter(|entry| entry.path().extension().is_some_and(|e| e == "jsonl"))
        .max_by_key(|entry| {
            entry
                .metadata()
                .and_then(|m| m.modified())
                .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
        })
        .map(|entry| entry.path())
}

#[derive(Deserialize)]
struct Line {
    #[serde(rename = "type")]
    kind: Option<String>,
    message: Option<Message>,
}

#[derive(Deserialize)]
struct Message {
    content: Option<serde_json::Value>,
}

fn text_of(content: &serde_json::Value) -> Option<String> {
    match content {
        serde_json::Value::String(s) => Some(s.clone()),
        serde_json::Value::Array(items) => {
            let joined: Vec<String> = items
                .iter()
                .filter(|item| item.get("type").and_then(|t| t.as_str()) == Some("text"))
                .filter_map(|item| item.get("text").and_then(|t| t.as_str()))
                .map(str::to_string)
                .collect();
            if joined.is_empty() {
                None
            } else {
                Some(joined.join(" "))
            }
        }
        _ => None,
    }
}

pub fn read_turns(path: &Path, max: usize) -> Vec<String> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let mut turns = Vec::new();
    for line in text.lines() {
        let Ok(record) = serde_json::from_str::<Line>(line) else {
            continue;
        };
        let Some(kind) = record.kind else { continue };
        if kind != "user" && kind != "assistant" {
            continue;
        }
        let Some(message) = record.message else { continue };
        let Some(content) = message.content else { continue };
        let Some(body) = text_of(&content) else { continue };
        if body.trim().is_empty() {
            continue;
        }
        if SERVICE_PREFIXES
            .iter()
            .any(|prefix| body.trim_start().starts_with(prefix))
        {
            continue;
        }
        turns.push(format!("{kind}: {body}"));
    }
    let start = turns.len().saturating_sub(max);
    turns.split_off(start)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fixture_transcript_is_found_by_directory() {
        let root = std::env::temp_dir().join(format!("bias-transcript-{}", std::process::id()));
        let cwd = "/work/example-project";
        let project_dir = root.join(slugify(cwd));
        std::fs::create_dir_all(&project_dir).unwrap();
        std::fs::write(project_dir.join("session-1.jsonl"), "").unwrap();

        let found = find(cwd, Some("claude"), &root).expect("must find the fixture");
        assert_eq!(found, project_dir.join("session-1.jsonl"));
    }

    #[test]
    fn an_agent_other_than_the_known_one_is_never_searched_for() {
        let root = std::env::temp_dir().join(format!("bias-transcript-other-{}", std::process::id()));
        let cwd = "/work/example-project";
        let project_dir = root.join(slugify(cwd));
        std::fs::create_dir_all(&project_dir).unwrap();
        std::fs::write(project_dir.join("session-1.jsonl"), "").unwrap();

        assert_eq!(find(cwd, Some("codex"), &root), None);
        assert_eq!(find(cwd, None, &root), None);
    }

    #[test]
    fn a_working_directory_with_no_project_under_root_finds_nothing() {
        let root = std::env::temp_dir().join(format!("bias-transcript-none-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        assert_eq!(find("/nowhere/at/all", Some("claude"), &root), None);
    }

    #[test]
    fn walking_up_finds_the_first_existing_directory() {
        let root = std::env::temp_dir().join(format!("bias-transcript-walk-{}", std::process::id()));
        let parent_cwd = "/work/example-project";
        let child_cwd = "/work/example-project/nested/deeper";
        let project_dir = root.join(slugify(parent_cwd));
        std::fs::create_dir_all(&project_dir).unwrap();
        std::fs::write(project_dir.join("s.jsonl"), "").unwrap();

        let found = find(child_cwd, Some("claude"), &root);
        assert_eq!(found, Some(project_dir.join("s.jsonl")));
    }

    fn fixture(lines: &[&str]) -> PathBuf {
        let path = std::env::temp_dir().join(format!("bias-turns-{}-{}", lines.len(), std::process::id()));
        std::fs::write(&path, lines.join("\n")).unwrap();
        path
    }

    #[test]
    fn service_turns_are_excluded_from_count_and_content() {
        let path = fixture(&[
            r#"{"type":"user","message":{"content":"hello there"}}"#,
            r#"{"type":"user","message":{"content":"<system-reminder>ignore me"}}"#,
            r#"{"type":"assistant","message":{"content":"got it"}}"#,
        ]);
        let turns = read_turns(&path, 10);
        assert_eq!(turns.len(), 2, "got {turns:?}");
        assert!(!turns.iter().any(|t| t.contains("ignore me")), "got {turns:?}");
    }

    #[test]
    fn only_the_last_max_turns_after_filtering_are_kept() {
        let path = fixture(&[
            r#"{"type":"user","message":{"content":"one"}}"#,
            r#"{"type":"assistant","message":{"content":"two"}}"#,
            r#"{"type":"user","message":{"content":"three"}}"#,
        ]);
        let turns = read_turns(&path, 2);
        assert_eq!(turns, vec!["assistant: two", "user: three"]);
    }

    #[test]
    fn an_empty_or_missing_file_yields_no_turns() {
        assert_eq!(read_turns(Path::new("/does/not/exist.jsonl"), 6), Vec::<String>::new());
        let path = fixture(&[]);
        assert_eq!(read_turns(&path, 6), Vec::<String>::new());
    }

    #[test]
    fn array_shaped_content_is_joined_from_its_text_items() {
        let path = fixture(&[
            r#"{"type":"assistant","message":{"content":[{"type":"text","text":"hi"},{"type":"tool_use","id":"x"}]}}"#,
        ]);
        let turns = read_turns(&path, 6);
        assert_eq!(turns, vec!["assistant: hi"]);
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --lib bias::transcript:: 2>&1 | head -20`
Expected: FAIL to compile — `src/bias/transcript.rs` has no content yet.

- [ ] **Step 3: Run the tests to verify they pass**

Run: `cargo test --lib bias::transcript::`
Expected: PASS (9 tests).

- [ ] **Step 4: Commit**

```bash
git add src/bias/transcript.rs
git commit -m "Find and read the target agent's transcript, filtered of service turns"
```

---

### Task 7: `bias::Collected` and `bias::collect`

**Files:**
- Modify: `src/bias.rs`
- Test: `src/bias.rs` (inline `#[cfg(test)] mod tests`)

**Interfaces:**
- Consumes: `Source` (Task 3), `files::collect` (Task 4), `pane::read` and
  `pane::PANE_LINES` (Task 5), `transcript::find` and
  `transcript::read_turns` (Task 6).
- Produces:

```rust
pub struct Collected {
    pub bias: String,
    pub attempted: Vec<(Source, bool)>,
    pub file_count: usize,
    pub file_chars: usize,
    pub conversation_chars: usize,
    pub truncated: bool,
}

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

pub fn collect(input: CollectInput) -> Collected
```

  Task 10 calls `collect` from the daemon's take path and logs everything
  in `Collected` except `bias`.

**A note on `file_chars` and `conversation_chars`, not named in
`DESIGN_21.md` section 2a.** The design's `Collected` has `bias`,
`attempted`, `file_count` and `truncated` only. This plan adds the two
character-count fields to make a real trap in the defaults observable: at
`file_names = 40` a busy repository's file-and-directory list runs to
roughly 800 characters on its own (see the run's own sequencing brief),
while `prompt_chars = 600` caps the *whole* string — so on such a
repository the conversation component can be truncated away entirely,
every take, and `truncated: bool` alone would not tell a person reading
the log that it was specifically the conversation, not the file list, that
lost the cut. `file_chars` and `conversation_chars` are counts, exactly
like `file_count` already is — nothing here logs content, so AC-9 is
unaffected. Flag this addition at the S4 code review (Task 11): it is a
plan-level decision, not one `DESIGN_21.md` made, and the reviewer should
judge whether it belongs on `Collected` or should be computed some other
way. **The defaults themselves — `file_names = 40` and `prompt_chars =
600` — are not changed by this task, and no reserved conversation budget
is invented**, per this run's brief.

**Context:** `DESIGN_21.md` section 2's dispatch table, section 2a's
refusal handling (consumed by Task 10, not here — `collect` takes an
already-resolved `Source`), section 7's assembly order and cap.

- [ ] **Step 1: Write the failing tests**

Add to `src/bias.rs`, after the `Source` enum:

```rust
pub struct Collected {
    pub bias: String,
    pub attempted: Vec<(Source, bool)>,
    pub file_count: usize,
    pub file_chars: usize,
    pub conversation_chars: usize,
    pub truncated: bool,
}

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

pub fn collect(input: CollectInput) -> Collected {
    let file_names = files::files::collect(input.cwd, input.file_names); // placeholder, fixed in Step 3
    todo!()
}
```

This intentionally does not compile — it exists only so the tests below
have something to fail against.

Add to `src/bias.rs`'s `#[cfg(test)] mod tests` (create the module if
Task 3 did not leave one — it did not, `src/bias.rs` only holds the enum
so far):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn scratch_root(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("bias-collect-{tag}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn no_repo_input<'a>(source: Source, root: &'a std::path::Path) -> CollectInput<'a> {
        CollectInput {
            source,
            cwd: "/no/such/repository",
            agent: None,
            pane: "w1:p1",
            transcript_root: root,
            herdr_binary: "definitely-not-a-program-here",
            conversation_turns: 6,
            file_names: 40,
            prompt_chars: 600,
        }
    }

    #[test]
    fn transcript_source_never_calls_the_pane() {
        let root = scratch_root("transcript-only");
        let collected = collect(no_repo_input(Source::Transcript, &root));
        assert_eq!(collected.attempted, vec![(Source::Transcript, false)]);
    }

    #[test]
    fn pane_source_never_seeks_a_transcript() {
        let root = scratch_root("pane-only");
        let collected = collect(no_repo_input(Source::Pane, &root));
        assert_eq!(collected.attempted, vec![(Source::Pane, false)]);
    }

    #[test]
    fn auto_tries_transcript_then_pane_when_the_transcript_misses() {
        let root = scratch_root("auto-both");
        let collected = collect(no_repo_input(Source::Auto, &root));
        assert_eq!(
            collected.attempted,
            vec![(Source::Transcript, false), (Source::Pane, false)]
        );
    }

    #[test]
    fn auto_does_not_try_the_pane_when_the_transcript_is_found() {
        let root = scratch_root("auto-hit");
        let cwd = "/work/example-project";
        let project_dir = root.join("-work-example-project");
        std::fs::create_dir_all(&project_dir).unwrap();
        std::fs::write(
            project_dir.join("s.jsonl"),
            r#"{"type":"user","message":{"content":"hello"}}"#,
        )
        .unwrap();

        let input = CollectInput {
            source: Source::Auto,
            cwd,
            agent: Some("claude"),
            pane: "w1:p1",
            transcript_root: &root,
            herdr_binary: "definitely-not-a-program-here",
            conversation_turns: 6,
            file_names: 40,
            prompt_chars: 600,
        };
        let collected = collect(input);
        assert_eq!(collected.attempted, vec![(Source::Transcript, true)]);
        assert!(collected.bias.contains("hello"), "got {:?}", collected.bias);
    }

    #[test]
    fn a_miss_on_every_source_still_produces_a_bias_from_files_alone() {
        let root = scratch_root("files-only");
        let repo = std::env::temp_dir().join(format!("bias-collect-repo-{}", std::process::id()));
        std::fs::create_dir_all(&repo).unwrap();
        std::process::Command::new("git").arg("-C").arg(&repo).args(["init", "-q"]).status().unwrap();
        std::process::Command::new("git").arg("-C").arg(&repo).args(["config", "user.email", "t@example.com"]).status().unwrap();
        std::process::Command::new("git").arg("-C").arg(&repo).args(["config", "user.name", "T"]).status().unwrap();
        std::fs::write(repo.join("marker.txt"), "x").unwrap();
        std::process::Command::new("git").arg("-C").arg(&repo).args(["add", "."]).status().unwrap();
        std::process::Command::new("git").arg("-C").arg(&repo).args(["commit", "-q", "-m", "add"]).status().unwrap();

        let input = CollectInput {
            source: Source::Auto,
            cwd: repo.to_str().unwrap(),
            agent: None,
            pane: "w1:p1",
            transcript_root: &root,
            herdr_binary: "definitely-not-a-program-here",
            conversation_turns: 6,
            file_names: 40,
            prompt_chars: 600,
        };
        let collected = collect(input);
        assert!(collected.bias.contains("marker.txt"), "got {:?}", collected.bias);
        assert!(collected.file_count >= 1);
    }

    #[test]
    fn the_string_is_capped_at_prompt_chars_and_truncated_is_set() {
        let root = scratch_root("cap");
        let cwd = "/work/big-project";
        let project_dir = root.join("-work-big-project");
        std::fs::create_dir_all(&project_dir).unwrap();
        let long_turn = "word ".repeat(400); // far more than prompt_chars
        let line = format!(r#"{{"type":"user","message":{{"content":"{long_turn}"}}}}"#);
        std::fs::write(project_dir.join("s.jsonl"), line).unwrap();

        let input = CollectInput {
            source: Source::Transcript,
            cwd,
            agent: Some("claude"),
            pane: "w1:p1",
            transcript_root: &root,
            herdr_binary: "definitely-not-a-program-here",
            conversation_turns: 6,
            file_names: 40,
            prompt_chars: 50,
        };
        let collected = collect(input);
        assert_eq!(collected.bias.chars().count(), 50);
        assert!(collected.truncated);
        assert!(collected.conversation_chars > 50, "got {}", collected.conversation_chars);
    }

    #[test]
    fn a_short_string_is_not_marked_truncated() {
        let root = scratch_root("no-cap");
        let input = no_repo_input(Source::Pane, &root);
        let collected = collect(input);
        assert!(!collected.truncated);
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --lib bias::tests:: 2>&1 | head -20`
Expected: FAIL to compile — the placeholder `collect` above does not
compile (`files::files::collect` typo, `todo!()`).

- [ ] **Step 3: Write the real implementation**

Replace the placeholder `collect` in `src/bias.rs` with:

```rust
pub fn collect(input: CollectInput) -> Collected {
    let file_names = files::collect(input.cwd, input.file_names);
    let file_line = file_names.join(" ");
    let file_chars = file_line.chars().count();

    let (conversation, attempted) = match input.source {
        Source::Transcript => {
            let (text, found) = try_transcript(&input);
            (text, vec![(Source::Transcript, found)])
        }
        Source::Pane => {
            let (text, found) = try_pane(&input);
            (text, vec![(Source::Pane, found)])
        }
        Source::Auto => {
            let (text, found) = try_transcript(&input);
            if found {
                (text, vec![(Source::Transcript, true)])
            } else {
                let (pane_text, pane_found) = try_pane(&input);
                (
                    pane_text,
                    vec![(Source::Transcript, false), (Source::Pane, pane_found)],
                )
            }
        }
    };
    let conversation_chars = conversation.chars().count();

    let mut assembled = file_line;
    if !conversation.is_empty() {
        if !assembled.is_empty() {
            assembled.push('\n');
        }
        assembled.push_str(&conversation);
    }
    let total_chars = assembled.chars().count();
    let truncated = total_chars > input.prompt_chars;
    let bias: String = assembled.chars().take(input.prompt_chars).collect();

    Collected {
        bias,
        attempted,
        file_count: file_names.len(),
        file_chars,
        conversation_chars,
        truncated,
    }
}

fn try_transcript(input: &CollectInput) -> (String, bool) {
    match transcript::find(input.cwd, input.agent, input.transcript_root) {
        Some(path) => {
            let turns = transcript::read_turns(&path, input.conversation_turns);
            if turns.is_empty() {
                (String::new(), false)
            } else {
                (turns.join("\n"), true)
            }
        }
        None => (String::new(), false),
    }
}

fn try_pane(input: &CollectInput) -> (String, bool) {
    match pane::read(input.pane, pane::PANE_LINES, input.herdr_binary) {
        Ok(text) if !text.trim().is_empty() => (text, true),
        _ => (String::new(), false),
    }
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test --lib bias::`
Expected: PASS — every test in `bias::source`, `bias::files`, `bias::pane`,
`bias::transcript` and `bias::tests`.

- [ ] **Step 5: Run the whole suite and lints**

Run: `cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt --check`
Expected: all three PASS. (`bias::collect` has one caller so far — nothing
in `main` or `daemon` — so clippy may flag it as dead code; if it does,
add `#[allow(dead_code)]` with a one-line comment pointing at Task 10,
the same way `IMPLEMENTED` in `src/main.rs:107` is `#[cfg(test)]` rather
than left unused, per `DESIGN_21.md` section 9's note on that exact
pattern.)

- [ ] **Step 6: Commit**

```bash
git add src/bias.rs
git commit -m "Assemble and cap the bias string from files and conversation"
```

---

### Task 8 — BLOCKED on #22: resolve `[context] source` once, at daemon start

**Files:**
- Modify: `src/daemon.rs`
- Test: `src/daemon.rs` (inline)

**Before starting:** #22 must have merged. Read the current
`src/daemon.rs` and find the bundle that replaced the `Recognition`
parameter (referred to as **Runtime** in this plan). Confirm its actual
name, whether it is constructed in `start()` the same way `Recognition` is
today (`src/daemon.rs:175-178`, this worktree's version, pre-#22), and
whether `config::Vars::from_env().home` is already threaded to wherever
Runtime is built.

**Interfaces:**
- Consumes: `bias::source::resolve` (Task 3), `config::Context` (Task 2).
- Produces: Runtime gains a field holding `Result<Source, String>` — the
  same resolved-once-at-start shape `Recognition` already has
  (`src/daemon.rs:46`, this worktree's version) — plus a field holding the
  transcript root: `<home>/.claude/projects` as a `PathBuf`, built from
  `config::Vars::from_env().home` the same way `config::directory` already
  reads it (`src/config.rs:117-121`), computed only when `home` is
  `Some(_)`. Task 10 reads both fields per take.

**Context:** `DESIGN_21.md` section 2a, "why resolved once, not per
take", and section 3, "the root, named" — `transcript::find` takes the
root as a parameter; production computes it once.

- [ ] **Step 1: Write the failing test**

Wherever Runtime's construction is tested today (or, if it is not tested
directly, in a new test placed next to Runtime's definition), add a test
asserting: given a `config::Context { source: "pane", .. }`, the built
Runtime's resolved source is `Ok(Source::Pane)`; given `source: "vosk"`,
it is an `Err` whose text contains `"vosk"`. Write the exact test body
against the real Runtime type and constructor found in Step 0 above — this
plan cannot write it sight-unseen, since Runtime's name and shape are
#22's, not this plan's, to fix.

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test --lib daemon:: 2>&1 | head -20`
Expected: FAIL — no such field on Runtime yet.

- [ ] **Step 3: Add the field and its resolution to `start()`**

In `start()` (or wherever Runtime is constructed), add:

```rust
let context_source: Result<bias::Source, String> =
    bias::source::resolve(&loaded.config.context.source);
if let Err(e) = &context_source {
    eprintln!("context source unavailable: {e}");
}
let transcript_root = config::Vars::from_env()
    .home
    .map(|home| std::path::PathBuf::from(home).join(".claude/projects"));
```

and add both to Runtime's construction, following whatever field-naming
convention Runtime already uses for `Recognition`.

- [ ] **Step 4: Run the test to verify it passes**

Run: `cargo test --lib daemon::`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/daemon.rs
git commit -m "Resolve [context] source and the transcript root once, at daemon start"
```

---

### Task 9 — BLOCKED on #22 and Task 8: thread the working directory and the agent name into `dictate`

**Files:**
- Modify: `src/daemon.rs`
- Test: `src/daemon.rs` (inline)

**Before starting:** confirm from the merged `src/daemon.rs` that `answer`
still parses the `Invocation` before calling `dictate` (this worktree's
pre-#22 version does so at `src/daemon.rs:59-72`) and that `dictate`'s
signature is the one #22 leaves it — some form of `fn dictate(recorder:
&Recorder, runtime: &Runtime, pane: &str)`.

**Interfaces:**
- Consumes: `context::Invocation::focused_pane_cwd` and
  `focused_pane_agent`, already parsed by `answer` (`src/context.rs:12-14`,
  unaffected by #22 — `src/context.rs` is a different module from this
  issue's `bias`).
- Produces: `dictate` (or whatever #22 named it) gains two more
  parameters: `cwd: Option<&str>` and `agent: Option<&str>`, both read
  from the already-parsed `Invocation` in `answer`, not re-parsed.

**Context:** `DESIGN_21.md` section 9, "What #21 itself does" — "That
call needs the working directory and the agent name, not only the pane id
`dictate` receives today... threading them into `dictate`'s own
parameters is a signature change inside one file, not a design decision."

- [ ] **Step 1: Write the failing test**

Add a test in `src/daemon.rs`'s test module calling `answer` with a
context body naming `focused_pane_cwd` and `focused_pane_agent`, and
assert (through whatever seam Task 10 will use to observe it — most
directly, by having Task 10's log-line test already in place and checking
it names the cwd-derived project or the agent) that both values reached
the take. If Task 10 has not yet added an observable effect, write this
test as part of Task 10 instead and treat Task 9 as producing the
signature change alone, verified by `cargo build` succeeding with the new
parameters wired through every call site.

- [ ] **Step 2: Update `answer` and `dictate`'s signatures**

In `answer`, where `dictate(recorder, runtime, pane)` (or its #22 name) is
called, pass `invocation.focused_pane_cwd.as_deref()` and
`invocation.focused_pane_agent.as_deref()` as the two new arguments. Update
`dictate`'s signature and every test call site in `src/daemon.rs`'s test
module to pass `None, None` (or a fixture value) for the two new
parameters where the test does not care about them.

- [ ] **Step 3: Run the whole daemon test module**

Run: `cargo test --lib daemon::`
Expected: PASS — every existing dispatch test still passes with the wider
signature.

- [ ] **Step 4: Commit**

```bash
git add src/daemon.rs
git commit -m "Thread the working directory and the agent name into dictate"
```

---

### Task 10 — BLOCKED on #22, Task 8 and Task 9: call `bias::collect` per take and log its metadata

**Files:**
- Modify: `src/daemon.rs`
- Test: `src/daemon.rs` (inline)

**Before starting:** confirm from the merged `src/doctor.rs` the exact
name and signature of the helper #22 extracts for running the `herdr`
binary through `HERDR_BIN_PATH` (this worktree's pre-#22
`doctor::herdr_finding`, `src/doctor.rs:105-112`, is the code #22 factors
that resolution out of). Use that helper here — **do not read
`HERDR_BIN_PATH` a second time in `src/daemon.rs`.**

**Interfaces:**
- Consumes: `bias::collect`, `bias::CollectInput`, `bias::Collected`
  (Task 7); Runtime's resolved `context_source` and `transcript_root`
  (Task 8); `dictate`'s new `cwd`/`agent` parameters (Task 9); #22's
  herdr-binary-path helper.
- Produces: one stderr line per take on the channel `request_line` and
  `context_note` already write to (`src/daemon.rs:141-150`, pre-#22
  numbering — confirm against the merged file), naming `attempted`,
  `file_count`, `file_chars`, `conversation_chars`, `truncated` and the
  configured `prompt_chars` — **never `Collected.bias`.** On an unresolved
  `context_source`, the take proceeds on `bias::files::collect` alone and
  the line names the configuration error once.

**Context:** `DESIGN_21.md` section 8 (uniform miss handling, the
per-request log channel, "that line never contains `Collected.bias`
itself — on a miss or on a hit") and section 2a ("The refusal's effect on
the take").

- [ ] **Step 1: Write the failing tests**

In `src/daemon.rs`'s test module, add (adapting to Runtime's real shape
found above):

```rust
#[test]
fn a_fixture_transcript_reaches_the_log_line_as_metadata_only() {
    // Build a Runtime whose context_source resolves to Source::Auto and
    // whose transcript_root points at a scratch directory containing a
    // fixture .jsonl with the text "the secret sentence nobody should see
    // in a log". Call the take path with a cwd/agent matching the fixture.
    // Assert the resulting log line contains "attempted", "file_chars" and
    // "conversation_chars", and does NOT contain "the secret sentence" or
    // any substring of it.
}

#[test]
fn an_unresolved_source_logs_the_configuration_error_and_still_produces_a_files_only_bias() {
    // Build a Runtime whose context_source is Err("unknown [context]
    // source \"vosk\"; ..."). Call the take path. Assert the take is not
    // refused for this reason, the log line contains "vosk", and
    // Collected (or its logged fields) shows an empty conversation
    // component with file_count from a scratch repository still present.
}

#[test]
fn a_miss_on_every_source_does_not_block_the_take() {
    // With no fixture transcript and a herdr_binary that does not exist,
    // assert dictate's reply is still Ok(_) (or whatever the successful
    // reply shape is) — the missing bias string does not fail recognition.
}
```

Write these against the real `Runtime`/`dictate`/`transcribe` functions
found in Steps for Tasks 8 and 9 — this plan states the assertions, not
the exact construction syntax, because that syntax is #22's.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --lib daemon:: 2>&1 | head -30`
Expected: FAIL — nothing calls `bias::collect` yet from `src/daemon.rs`.

- [ ] **Step 3: Wire `bias::collect` into the take path and log its metadata**

In `dictate` (or wherever the take's pane/cwd/agent are all in scope
together with Runtime), before or after starting the recording — whichever
`DESIGN_21.md` section 9's "called from exactly one place: `dictate`"
implies is workable given Runtime's actual shape — build a `CollectInput`
and call `bias::collect`, or, when `context_source` is `Err`, call
`bias::files::collect` directly and log the configuration error:

```rust
match &runtime.context_source {
    Ok(source) => {
        let root = runtime.transcript_root.clone().unwrap_or_else(|| std::path::PathBuf::from("/nonexistent"));
        let collected = bias::collect(bias::CollectInput {
            source: *source,
            cwd: cwd.unwrap_or(""),
            agent,
            pane,
            transcript_root: &root,
            herdr_binary: &herdr_binary_path(), // #22's helper — call site adapts
            conversation_turns: runtime.context.conversation_turns,
            file_names: runtime.context.file_names,
            prompt_chars: runtime.context.prompt_chars,
        });
        eprintln!(
            "context bias: attempted={:?} file_count={} file_chars={} conversation_chars={} \
             prompt_chars={} truncated={}",
            collected.attempted,
            collected.file_count,
            collected.file_chars,
            collected.conversation_chars,
            runtime.context.prompt_chars,
            collected.truncated
        );
    }
    Err(e) => {
        let names = bias::files::collect(cwd.unwrap_or(""), runtime.context.file_names);
        eprintln!("context source unavailable: {e}; using file names only ({} found)", names.len());
    }
}
```

Adapt field access (`runtime.context`, `runtime.transcript_root`,
`herdr_binary_path()`) to whatever #22's Runtime actually names them.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test --lib daemon::`
Expected: PASS, including the three new tests from Step 1.

- [ ] **Step 5: Run the whole suite and lints**

Run: `cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt --check && python3 scripts/check_manifest.py`
Expected: all PASS.

- [ ] **Step 6: Commit**

```bash
git add src/daemon.rs
git commit -m "Bias each take's recognition, logging only metadata about the result"
```

---

### Task 11: S4 — review the diff before a pull request exists

**Files:** none (a review of the accumulated diff from Tasks 1-10).

**Interfaces:**
- Consumes: the full diff on `feat/21-context` since it diverged from
  `main`.
- Produces: a recorded verdict in `tasks/21/RUN_21.md`, under a new `##
  Gate S4` heading, per `CLAUDE.md`'s stage table.

**Context:** `CLAUDE.md`, "How work is run": "S4 closes on a review of the
diff, not on a pull request... use it after the pull request is open, as a
second pass." Flag explicitly, in the review request, the two additions
this plan made beyond `DESIGN_21.md`'s literal text: `Collected.file_chars`
and `Collected.conversation_chars` (Task 7), and ask the reviewer to judge
whether they belong on `Collected` as written or should be logged another
way.

- [ ] **Step 1: Run `superpowers:requesting-code-review` against the diff**

Invoke the skill against `git diff main...feat/21-context` (or the
equivalent full-branch diff), naming this plan and `DESIGN_21.md` as the
artifacts the diff should satisfy.

- [ ] **Step 2: Record the verdict**

Append to `tasks/21/RUN_21.md`:

```markdown
## Gate S4

```yaml
gate:
  stage: S4
  artifact: diff (feat/21-context since main)
  reviewer: superpowers:requesting-code-review
  verdict: <READY | QUESTIONS | BLOCKED>
  date: <date>
  questions: []
  blocker: null
```
```

filling in the actual verdict and any questions the review raised. If
`QUESTIONS`, address them, re-run Step 1, and update this section before
proceeding — this stage is never skipped (`CLAUDE.md`, "Infrastructure
runs may skip S1 to S3... S4 and S5 are never skipped").

- [ ] **Step 3: Commit**

```bash
git add tasks/21/RUN_21.md
git commit -m "Record the S4 diff review for context"
```

---

### Task 12: S5 — verify and write `docs/evidence.md`

**Files:**
- Modify: `docs/evidence.md`

**Interfaces:**
- Consumes: `cargo test` output from every prior task, run fresh on this
  step.
- Produces: a new section in `docs/evidence.md` naming what was run, what
  it proved, and the platform.

**Context:** `CLAUDE.md`, "S5 is a step that can fail, not a request...
Run the thing, read the whole output, and write what happened." This
issue collects a bias string and passes it to nothing — `Engine` never
sees it (`DESIGN_21.md` section 9) — so the thing a spoken take would
prove (that biasing actually changes a real transcript) is not this
issue's to prove; it belongs to #26, which passes the string to the
engine. Be explicit about that boundary in what is written.

- [ ] **Step 1: Run the full test suite fresh**

Run: `cargo test 2>&1 | tee /tmp/context-s5-test-output.txt`
Read the whole output, not just the final line.

- [ ] **Step 2: Run the manifest and lint checks**

Run: `python3 scripts/check_manifest.py && cargo clippy --all-targets -- -D warnings && cargo fmt --check`

- [ ] **Step 3: Write the evidence section**

Append to `docs/evidence.md`, following the existing section style (a
heading, then prose with numbers, no narrated history):

```markdown
## Context bias string, by test suite

`cargo test` on <platform, from `uname` or equivalent>: <N> tests passed,
0 failed, covering `bias::source`, `bias::files`, `bias::pane`,
`bias::transcript`, `bias::` (assembly and cap) and the daemon's per-take
logging. `cargo clippy --all-targets -- -D warnings`, `cargo fmt --check`
and `scripts/check_manifest.py` all pass.

What this establishes: the bias string is assembled from file and
directory names plus conversation content according to `[context]
source`, is capped at `prompt_chars` characters, and the per-request log
carries only metadata — `attempted`, `file_count`, `file_chars`,
`conversation_chars`, `truncated` — never the string's own content, proven
by a negative assertion against fixture text designed to be obviously
absent from the log if it worked correctly.

What this does not establish: whether the bias string, once collected,
changes what a real recognition run outputs for a real take. This issue
assembles the string and passes it to nothing — `Engine::transcribe` is
untouched by design (`DESIGN_21.md` section 9) — so there is no consumer
to run a spoken take against yet. That proof belongs to issue #26, which
wires the string into recognition.
```

- [ ] **Step 4: Record the S5 verdict in `RUN_21.md`**

Append to `tasks/21/RUN_21.md`:

```markdown
## Gate S5

```yaml
gate:
  stage: S5
  artifact: docs/evidence.md, "Context bias string, by test suite"
  reviewer: the run
  verdict: <PASS | FAIL>
  date: <date>
```
```

- [ ] **Step 5: Commit**

```bash
git add docs/evidence.md tasks/21/RUN_21.md
git commit -m "Verify the context bias string by test suite, and record what #26 still owes"
```

---

## Self-review notes (writer's own pass)

- **Spec coverage:** AC-1 (Task 6/7), AC-2 (Task 6), AC-3 (Task 7), AC-4
  (Task 4), AC-5 (Task 7), AC-6 (Task 7's `Collected`/`collect` being
  `pub` and called from Task 10, `Engine::transcribe` untouched
  throughout), AC-7 (Task 2), AC-8 (Tasks 4-7, no panics — every function
  returns `Option`/`Result`/an empty `Vec` rather than panicking), AC-9
  (Task 7's `Collected` carries counts only, Task 10's log line and its
  negative-content test).
- **Placeholder scan:** the only intentionally non-compiling snippet is
  Task 7 Step 1's placeholder `collect` body, which Step 3 replaces before
  the task's own tests are asserted to pass — flagged explicitly as
  provisional in the text next to it, not left as an unresolved TODO.
- **Type consistency:** `Source` is `Copy`, used by value throughout;
  `Collected.attempted: Vec<(Source, bool)>` matches every task's use of
  it; `CollectInput` field names are used identically in Tasks 7 and 10.

## What could not be cut into a checkable task

`DESIGN_21.md` leaves `Engine::transcribe`'s widened shape to issue #26
and explicitly declines to sketch it even as a non-binding note (section
9). This plan follows that boundary and cuts no task toward it — there is
nothing to check, by the design's own argument, until #26 exists.

Tasks 8 through 10 could not be written with the same precision as Tasks 1
through 7, because they modify code (`src/daemon.rs`'s post-#22 Runtime
bundle) that does not exist in this worktree yet. Each of those three
tasks says explicitly what to verify against the merged code before
writing it, rather than asserting a signature this plan cannot see.
