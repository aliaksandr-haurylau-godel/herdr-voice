# The microphone popup, the configuration writer and the reload request: Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `herdr-voice mic --choose` switches the microphone by name from a popup a key opens, through a tested configuration writer, and the daemon uses the new input from the next take without a restart.

**Architecture:** A new `config_edit` module edits one or more keys of `config.toml` and replaces the file whole only if the result still loads as `Config`. A `reload` request lets a popup tell the running daemon to re-read the file; the daemon applies `[audio]` through the recorder's own order queue and names every other changed section as needing a restart. A manifest action opens the popup pane; `setup` binds a key to that action.

**Tech Stack:** Rust 2021, `toml` 1 and `serde` (already dependencies), `cpal` 0.18 (already), no new crate.

**Spec:** `tasks/103/DESIGN_103.md` (evidence `tasks/103/DESIGN_103_evidence.md`), criteria `tasks/103/AC_103.md`. Read the design first: it states why each choice was made.

## Global Constraints

- Everything in the repository is English: code, comments, messages, commits. Cite paths relative to the repository root. No absolute path, no home directory, no account name in any file (the leak gate rejects them).
- No panic paths in the daemon. Every user-visible failure names what to do next.
- Device selection by name, never by index. The value written to `[audio] input` is the device's name.
- No new dependency. `Cargo.toml` and `Cargo.lock` do not change.
- Every cargo command runs with `CARGO_BUILD_JOBS=6`, one cargo process at a time, wrapped in a time limit: `perl -e 'alarm shift; exec @ARGV' <seconds> cargo ...`.
- Never `git push` without naming the branch: `git push origin HEAD:feat/103-mic-popup`. Never change `git config`.
- Commit messages end with the line `Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>`.
- Before every commit, check the files you wrote for stray editing-tool tags (the four angle-bracket tags the editing tool uses around old and new text) and for conflict markers at the start of a line; the pre-commit hook rejects both, and a plan or a note that quotes the tags literally trips it.
- The key `prefix+shift+i`, the action id `mic`, and the descriptions are proposals until the owner names them; they live in one place each (`BINDINGS`, `herdr-plugin.toml`).
- Four gates before the pull request: `cargo test`, `cargo clippy --all-targets -- -D warnings`, `cargo fmt --check`, `python3 scripts/check_manifest.py`, plus the Windows dead-code check in Task 10.

## Review Focus

Failure modes the design implies and no task would otherwise exercise, most likely first; each is pinned by a test in the task named.

1. A configuration file with Windows line endings must keep them, and every line the edit does not touch must stay byte-identical (Task 1).
2. A device name containing a double quote, a backslash, non-ASCII letters or a control character must be written so that it parses back equal (Task 1).
3. A configuration file that is a symbolic link, or that has mode `0600` because it holds a token, must keep both after an edit (Task 2).
4. A file that cannot be read as text, or that already fails to load, must not be replaced (Task 2, Task 6).
5. Two inputs with the same name must both be listed, and the second must say the plugin uses the first (Task 8).

## File structure

| File | Responsibility |
|---|---|
| `src/config_edit.rs` (new) | `Edit`, `quote`, `set_keys` (pure text edit), `WriteError`, `write_keys` (the file) |
| `src/reload.rs` (new) | The reload reply: `reply`, `parse`, `Applied` |
| `src/mic.rs` (new) | The popup (`choose`, `render`, `parse_answer`, `reached`, `run`) and the action (`open_command`, `open`) |
| `src/chooser.rs` | Loses `set_model_key`, `table_name`, `write_model_key` and their tests; calls the writer |
| `src/capture.rs` | `Command::Reconfigure`, `Recorder::reconfigure`, a recording test source |
| `src/capture/cpal_source.rs` | `input_names()` |
| `src/client.rs` | `exchange`; `send_to` becomes `outcome(exchange(..))` |
| `src/daemon.rs` | `Runtime.running`, `plan_reload`, `reload_from`, the `"reload"` arm |
| `src/main.rs` | `mod` lines, the `mic` arm, `IMPLEMENTED`, `USAGE`, tests |
| `herdr-plugin.toml` | The action `mic` |
| `src/setup.rs` | A fourth entry in `BINDINGS`; nine tests and one message adjusted |
| `docs/decisions.md` | Two rows |

Task order and dependencies: 1 → 2 → 3; 4; 5; 6 needs 4 and 5; 7; 8 needs 1, 2, 4, 5, 7; 9 needs 8; 10 needs all. Tasks 4, 5 and 7 can be done in any order after Task 1 starts.

---

### Task 1: The pure edit, `src/config_edit.rs` (part one)

**Files:**
- Create: `src/config_edit.rs`
- Modify: `src/main.rs` (add `mod config_edit;` between `mod config;` and `mod context;`)
- Modify: `src/chooser.rs` is **not** touched in this task.

**Interfaces:**
- Consumes: nothing from earlier tasks.
- Produces (exact):
  - `pub struct Edit<'a> { pub table: &'a str, pub key: &'a str, pub value: String }`
  - `pub fn quote(text: &str) -> String`
  - `pub fn set_keys(existing: &str, edits: &[Edit]) -> String`
  - `fn table_name(line: &str) -> Option<&str>` (private)

- [ ] **Step 1: Create the file with the failing tests**

Create `src/config_edit.rs` containing only the module comment and a test module. The nine tests that call `set_model_key` or `table_name` in `src/chooser.rs` (lines 361 to 490: `a_file_with_no_stt_table_gains_one`, `an_existing_model_line_is_replaced_and_the_comments_around_it_survive`, `a_stt_table_with_no_model_key_gains_one_inside_itself`, `a_model_key_in_another_table_is_not_the_one_that_changes`, `a_table_header_with_a_comment_after_it_is_still_that_table`, `headers_are_recognised_whatever_surrounds_them`, `spaces_inside_the_header_still_name_the_table`, `an_empty_file_becomes_a_valid_one`, `a_stt_table_that_is_the_last_one_still_gains_the_key_inside_itself`) are copied into this module **unchanged except** that each call `set_model_key(X, Y)` becomes `model(X, Y)`; `table_name(..)` calls stay as they are. They are not removed from `src/chooser.rs` until Task 3.

```rust
//! Editing the plugin's configuration file: one key at a time, comments and every
//! other line left alone.
//!
//! The edit is line-oriented because the `toml` crate in the tree parses and does
//! not preserve formatting, and `toml_edit` would be a new dependency for a few
//! lines of text. See `tasks/103/DESIGN_103.md`, section 2.1.

#[cfg(test)]
mod tests {
    use super::*;

    /// `[stt] model = "<id>"`, the edit the model chooser makes.
    fn model(existing: &str, id: &str) -> String {
        set_keys(
            existing,
            &[Edit {
                table: "stt",
                key: "model",
                value: quote(id),
            }],
        )
    }

    // The nine moved tests go here, with `set_model_key(` replaced by `model(`.

    #[test]
    fn quote_writes_one_line_that_parses_back_equal() {
        for text in [
            "MacBook Pro Microphone",
            "Mic \"A\" \\ B",
            "Микрофон é 🎙",
            "line\nbreak\ttab\r",
            "bell\u{7} and delete\u{7f}",
            "",
        ] {
            let quoted = quote(text);
            assert!(!quoted.contains('\n'), "one line, got {quoted:?}");
            let parsed: toml::Value = toml::from_str(&format!("k = {quoted}\n"))
                .unwrap_or_else(|e| panic!("{quoted:?} must parse: {e}"));
            assert_eq!(parsed["k"].as_str(), Some(text), "round trip of {text:?}");
        }
    }

    #[test]
    fn a_name_with_quotes_and_a_backslash_is_written_and_read_back() {
        let out = set_keys(
            "[audio]\ninput = \"Old\"\n",
            &[Edit {
                table: "audio",
                key: "input",
                value: quote("Mic \"A\" \\ B"),
            }],
        );
        let parsed: toml::Value = toml::from_str(&out).expect("must stay valid TOML");
        assert_eq!(parsed["audio"]["input"].as_str(), Some("Mic \"A\" \\ B"));
    }

    #[test]
    fn two_edits_in_one_call_both_land() {
        let out = set_keys(
            "[stt]\nlanguage = \"ru\"\n",
            &[
                Edit {
                    table: "stt",
                    key: "model",
                    value: quote("tiny"),
                },
                Edit {
                    table: "stt",
                    key: "engine",
                    value: quote("candle"),
                },
            ],
        );
        let parsed: toml::Value = toml::from_str(&out).expect("must stay valid TOML");
        assert_eq!(parsed["stt"]["model"].as_str(), Some("tiny"), "got {out}");
        assert_eq!(parsed["stt"]["engine"].as_str(), Some("candle"), "got {out}");
        assert_eq!(parsed["stt"]["language"].as_str(), Some("ru"), "got {out}");
    }

    #[test]
    fn a_key_of_the_same_name_in_another_table_is_not_touched_in_either_direction() {
        let before = "[stt]\nmodel = \"a\"\n\n[rewrite]\nmodel = \"b\"\n";
        let out = set_keys(
            before,
            &[Edit {
                table: "rewrite",
                key: "model",
                value: quote("c"),
            }],
        );
        let parsed: toml::Value = toml::from_str(&out).unwrap();
        assert_eq!(parsed["stt"]["model"].as_str(), Some("a"), "got {out}");
        assert_eq!(parsed["rewrite"]["model"].as_str(), Some("c"), "got {out}");
        let out = model(before, "d");
        let parsed: toml::Value = toml::from_str(&out).unwrap();
        assert_eq!(parsed["stt"]["model"].as_str(), Some("d"), "got {out}");
        assert_eq!(parsed["rewrite"]["model"].as_str(), Some("b"), "got {out}");
    }

    #[test]
    fn windows_line_endings_survive_and_every_other_line_is_byte_identical() {
        let before = "# mine\r\n[audio]\r\ninput = \"Old\"\r\nsilence_db = -50.0\r\n\r\n[ui]\r\ntoasts = false\r\n";
        let out = set_keys(
            before,
            &[Edit {
                table: "audio",
                key: "input",
                value: quote("New"),
            }],
        );
        assert_eq!(
            out,
            before.replace("input = \"Old\"", "input = \"New\""),
            "only the one line may differ"
        );
    }

    #[test]
    fn an_edit_changes_one_line_and_nothing_else() {
        let before = "# my configuration\n[audio]\n# the input\ninput = \"Old\"\nsilence_db = -50.0\n\n[ui]\ntoasts = false\n";
        let out = set_keys(
            before,
            &[Edit {
                table: "audio",
                key: "input",
                value: quote("New"),
            }],
        );
        assert_eq!(out, before.replace("input = \"Old\"", "input = \"New\""));
    }
}
```

- [ ] **Step 2: Run the tests to see them fail**

Add `mod config_edit;` to `src/main.rs`. Run:
`CARGO_BUILD_JOBS=6 perl -e 'alarm shift; exec @ARGV' 1500 cargo test --bin herdr-voice config_edit::`
Expected: compile error, `cannot find function set_keys` / `quote` / `Edit` / `table_name`.

- [ ] **Step 3: Write the implementation**

Above the test module in `src/config_edit.rs`:

```rust
/// One key to set: `[table] key = value`, where `value` is TOML text (`quote`
/// builds it for a string).
pub struct Edit<'a> {
    pub table: &'a str,
    pub key: &'a str,
    pub value: String,
}

/// A TOML basic string on one line. The `toml` crate's own rendering writes a
/// string with a line break over several lines, which a line edit cannot replace
/// (`tasks/103/DESIGN_103_evidence.md`, D2).
pub fn quote(text: &str) -> String {
    let mut out = String::from("\"");
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 || c as u32 == 0x7f => {
                out.push_str(&format!("\\u{:04X}", c as u32))
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// The table a line opens, if it opens one: `[stt] # speech` is the `[stt]`
/// table, and TOML says so.
///
/// A naive `starts_with('[') && ends_with(']')` misses exactly that line, and
/// missing it made the editor append a second `[stt]` table and produce a file
/// that does not parse — which `config::load` then discards whole, losing every
/// other setting in it. Found by an S4 review of the model chooser.
fn table_name(line: &str) -> Option<&str> {
    // Paste the body of `table_name` from `src/chooser.rs:103-129` here, unchanged.
}

/// `existing` with every edit applied, in order. Comments, other keys and other
/// tables are left alone, and so are the line endings: a file that uses `\r\n`
/// keeps them.
pub fn set_keys(existing: &str, edits: &[Edit]) -> String {
    let eol = if existing.contains("\r\n") { "\r\n" } else { "\n" };
    let mut text = existing.to_string();
    for edit in edits {
        text = set_one(&text, edit, eol);
    }
    text
}

/// One edit: replace the first `key =` line inside `[table]`, or add the line at
/// the end of that table, or add the table at the end of the file.
fn set_one(existing: &str, edit: &Edit, eol: &str) -> String {
    let line = format!("{} = {}", edit.key, edit.value);
    let mut out: Vec<String> = Vec::new();
    let mut in_table = false;
    let mut wrote = false;
    let mut saw_table = false;

    for text in existing.lines() {
        let trimmed = text.trim();
        if let Some(name) = table_name(text) {
            // Leaving the table without having found the key: add it at its end.
            if in_table && !wrote {
                out.push(line.clone());
                wrote = true;
            }
            in_table = name == edit.table;
            saw_table |= in_table;
            out.push(text.to_string());
            continue;
        }
        // Only a key inside the named table. Another table may have a key of the
        // same name (`[rewrite]` and `[stt]` both have `model`), and editing the
        // wrong one would silently repoint something else.
        if in_table && !wrote {
            if let Some(rest) = trimmed.strip_prefix(edit.key) {
                if rest.trim_start().starts_with('=') {
                    out.push(line.clone());
                    wrote = true;
                    continue;
                }
            }
        }
        out.push(text.to_string());
    }

    if in_table && !wrote {
        out.push(line.clone());
        wrote = true;
    }
    if !saw_table {
        if !out.is_empty() && !out.last().is_some_and(|l| l.trim().is_empty()) {
            out.push(String::new());
        }
        out.push(format!("[{}]", edit.table));
        out.push(line);
    } else if !wrote {
        out.push(line);
    }

    let mut text = out.join(eol);
    text.push_str(eol);
    text
}
```

`table_name` is moved, not rewritten: open `src/chooser.rs` at lines 103 to 129 and copy the function body into the stub above. The two doc comments on it are already in the stub.

- [ ] **Step 4: Run the tests to see them pass**

`CARGO_BUILD_JOBS=6 perl -e 'alarm shift; exec @ARGV' 1500 cargo test --bin herdr-voice config_edit::`
Expected: all tests in `config_edit::` pass (the nine moved ones and the six above).

- [ ] **Step 5: Commit**

```bash
git add src/config_edit.rs src/main.rs
git commit -m "Add the pure part of the configuration writer

Edit, quote and set_keys: the line edit the model chooser had, taking the table
and the key as parameters, keeping line endings, and writing a name on one line.

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 2: The file part, `write_keys` and `WriteError` (#49)

**Files:**
- Modify: `src/config_edit.rs`

**Interfaces:**
- Consumes: `Edit`, `set_keys` (Task 1); `crate::config::{Config, FILE_NAME}`.
- Produces (exact):
  - `pub enum WriteError { NoDirectory, Unreadable { path: String, why: String }, AlreadyInvalid { path: String, why: String }, Refused { path: String, why: String }, Io { path: String, why: String } }` with `Debug, PartialEq, Eq` and `impl std::fmt::Display`.
  - `pub fn write_keys(directory: Option<&std::path::Path>, edits: &[Edit]) -> Result<std::path::PathBuf, WriteError>` — returns the path of the file written (`directory/config.toml`, not the link target).

- [ ] **Step 1: Write the failing tests**

Add to the `tests` module of `src/config_edit.rs`:

```rust
    fn scratch(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "herdr-voice-edit-{tag}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn input(name: &str) -> Edit<'static> {
        Edit {
            table: "audio",
            key: "input",
            value: quote(name),
        }
    }

    const ORIGINAL: &str =
        "# mine\n[audio]\ninput = \"Old\"\nsilence_db = -50.0\n\n[ui]\ntoasts = false\n";

    #[test]
    fn a_good_edit_is_written_and_every_other_byte_is_unchanged() {
        let dir = scratch("good");
        std::fs::write(dir.join("config.toml"), ORIGINAL).unwrap();
        let path = write_keys(Some(&dir), &[input("New")]).expect("written");
        assert_eq!(path, dir.join("config.toml"));
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            ORIGINAL.replace("input = \"Old\"", "input = \"New\"")
        );
        let leftovers: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
            .collect();
        assert_eq!(leftovers, vec!["config.toml".to_string()], "no candidate left");
    }

    #[test]
    fn a_value_of_the_wrong_type_is_refused_and_the_file_is_untouched() {
        // `toml::Value` accepts this and `config::load` does not, which makes
        // `config::load` fall back to every default.
        let dir = scratch("wrong-type");
        let before = "[ui]\nblink_ms = 250\n[audio]\ninput = \"Old\"\n";
        std::fs::write(dir.join("config.toml"), before).unwrap();
        let wrong = Edit {
            table: "ui",
            key: "blink_ms",
            value: quote("fast"),
        };
        let error = write_keys(Some(&dir), &[wrong]).expect_err("must be refused");
        assert!(matches!(error, WriteError::Refused { .. }), "got {error:?}");
        let said = error.to_string();
        assert!(said.contains("config.toml"), "names the file: {said}");
        assert!(said.contains("nothing was written"), "{said}");
        assert_eq!(std::fs::read_to_string(dir.join("config.toml")).unwrap(), before);
    }

    #[test]
    fn a_file_that_already_does_not_load_is_not_edited() {
        let dir = scratch("already-invalid");
        let before = "[audio\ninput = ";
        std::fs::write(dir.join("config.toml"), before).unwrap();
        let error = write_keys(Some(&dir), &[input("New")]).expect_err("refused");
        assert!(matches!(error, WriteError::AlreadyInvalid { .. }), "got {error:?}");
        assert!(error.to_string().contains("nothing was written"));
        assert_eq!(std::fs::read_to_string(dir.join("config.toml")).unwrap(), before);
    }

    #[test]
    fn a_file_that_is_not_text_is_not_replaced_by_one_holding_the_new_key() {
        let dir = scratch("not-text");
        let bytes = [0xffu8, 0xfe, 0x00, 0x41];
        std::fs::write(dir.join("config.toml"), bytes).unwrap();
        let error = write_keys(Some(&dir), &[input("New")]).expect_err("refused");
        assert!(matches!(error, WriteError::Unreadable { .. }), "got {error:?}");
        assert_eq!(std::fs::read(dir.join("config.toml")).unwrap(), bytes);
    }

    #[test]
    fn no_file_and_no_directory_gives_both_and_only_the_edited_key() {
        let dir = scratch("absent").join("nested").join("deeper");
        let path = write_keys(Some(&dir), &[input("New")]).expect("created");
        assert_eq!(
            std::fs::read_to_string(path).unwrap(),
            "[audio]\ninput = \"New\"\n"
        );
    }

    #[test]
    fn no_known_directory_says_so() {
        let error = write_keys(None, &[input("New")]).expect_err("nowhere to write");
        assert_eq!(error, WriteError::NoDirectory);
        assert!(error.to_string().contains("HERDR_PLUGIN_CONFIG_DIR"));
    }

    #[test]
    fn a_directory_that_cannot_be_created_is_an_io_error_naming_it() {
        let dir = scratch("blocked");
        let in_the_way = dir.join("occupied");
        std::fs::write(&in_the_way, "a file, not a directory").unwrap();
        let error = write_keys(Some(&in_the_way.join("config")), &[input("New")])
            .expect_err("cannot create");
        assert!(matches!(error, WriteError::Io { .. }), "got {error:?}");
        assert!(error.to_string().contains("occupied"), "{error}");
    }

    #[test]
    fn one_refused_edit_among_two_changes_neither() {
        let dir = scratch("all-or-nothing");
        std::fs::write(dir.join("config.toml"), ORIGINAL).unwrap();
        let edits = [
            input("New"),
            Edit {
                table: "ui",
                key: "blink_ms",
                value: quote("fast"),
            },
        ];
        write_keys(Some(&dir), &edits).expect_err("refused as a whole");
        assert_eq!(std::fs::read_to_string(dir.join("config.toml")).unwrap(), ORIGINAL);
        let both = [
            input("New"),
            Edit {
                table: "ui",
                key: "toasts",
                value: "true".to_string(),
            },
        ];
        write_keys(Some(&dir), &both).expect("both written");
        let after = std::fs::read_to_string(dir.join("config.toml")).unwrap();
        assert!(after.contains("input = \"New\"") && after.contains("toasts = true"), "{after}");
    }

    #[cfg(unix)]
    #[test]
    fn a_read_only_directory_is_an_io_error_and_leaves_the_file_and_no_candidate() {
        use std::os::unix::fs::PermissionsExt;
        let dir = scratch("read-only");
        std::fs::write(dir.join("config.toml"), ORIGINAL).unwrap();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o500)).unwrap();
        let result = write_keys(Some(&dir), &[input("New")]);
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
        let error = result.expect_err("cannot write");
        assert!(matches!(error, WriteError::Io { .. }), "got {error:?}");
        assert_eq!(std::fs::read_to_string(dir.join("config.toml")).unwrap(), ORIGINAL);
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1, "no candidate left");
    }

    #[cfg(unix)]
    #[test]
    fn the_mode_of_the_file_survives_because_it_may_hold_a_token() {
        use std::os::unix::fs::PermissionsExt;
        let dir = scratch("mode");
        let file = dir.join("config.toml");
        std::fs::write(&file, ORIGINAL).unwrap();
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o640)).unwrap();
        write_keys(Some(&dir), &[input("New")]).expect("written");
        let mode = std::fs::metadata(&file).unwrap().permissions().mode() & 0o777;
        // The candidate is created with 0600, so only copying the original's mode gives 0640.
        assert_eq!(mode, 0o640);
    }

    #[cfg(unix)]
    #[test]
    fn a_symbolic_link_stays_a_link_and_the_file_it_points_at_changes() {
        let dir = scratch("link");
        let real_dir = scratch("link-real");
        let real = real_dir.join("dotfiles-config.toml");
        std::fs::write(&real, ORIGINAL).unwrap();
        std::os::unix::fs::symlink(&real, dir.join("config.toml")).unwrap();
        write_keys(Some(&dir), &[input("New")]).expect("written");
        assert!(
            std::fs::symlink_metadata(dir.join("config.toml"))
                .unwrap()
                .file_type()
                .is_symlink(),
            "the link must still be a link"
        );
        assert!(std::fs::read_to_string(&real).unwrap().contains("input = \"New\""));
    }
```

- [ ] **Step 2: Run the tests to see them fail**

`CARGO_BUILD_JOBS=6 perl -e 'alarm shift; exec @ARGV' 1500 cargo test --bin herdr-voice config_edit::`
Expected: compile error, `cannot find type WriteError` / `write_keys`.

- [ ] **Step 3: Write the implementation**

Above the test module, after `set_one`:

```rust
use std::path::{Path, PathBuf};

use crate::config::{self, Config};

/// Why a write did not happen, each with what to do next. In every case but `Io`
/// the file on disk is exactly what it was.
#[derive(Debug, PartialEq, Eq)]
pub enum WriteError {
    /// No configuration directory can be worked out from the environment.
    NoDirectory,
    /// The file exists and cannot be read as text.
    Unreadable { path: String, why: String },
    /// The file as it stands does not load as the configuration, so the daemon is
    /// already running on defaults and an edit would be blamed for it.
    AlreadyInvalid { path: String, why: String },
    /// The edit would have made the file unloadable.
    Refused { path: String, why: String },
    /// A step of creating or replacing the file failed.
    Io { path: String, why: String },
}

impl std::fmt::Display for WriteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WriteError::NoDirectory => write!(
                f,
                "there is no configuration directory to write to: none of \
                 HERDR_PLUGIN_CONFIG_DIR, XDG_CONFIG_HOME and HOME is set. Set one and try again"
            ),
            WriteError::Unreadable { path, why } => write!(
                f,
                "cannot read {path} as text ({why}), so nothing was written. \
                 Fix the file's permissions or contents and try again"
            ),
            WriteError::AlreadyInvalid { path, why } => write!(
                f,
                "{path} does not load as a configuration ({why}), so nothing was written. \
                 Fix the file and try again"
            ),
            WriteError::Refused { path, why } => write!(
                f,
                "the edit would have made {path} unloadable ({why}), so nothing was written"
            ),
            WriteError::Io { path, why } => write!(f, "cannot write {path}: {why}"),
        }
    }
}

/// Write the file with `edits` applied, and return its path.
///
/// Never leaves a file that does not load: `config::load` treats an unparsable
/// file as absent and falls back to every default, so a bad edit would silently
/// discard every other setting the person has. The edited text is therefore parsed
/// as `Config`, the type `config::load` parses, before anything is written.
///
/// The file is replaced whole, by writing a candidate beside it and renaming it
/// over the target: a write in place that fails halfway leaves an empty file,
/// which loads as "every default". A symbolic link is followed, so the link
/// survives and the file it points at changes; the file's permissions carry over,
/// because it may hold a token.
pub fn write_keys(directory: Option<&Path>, edits: &[Edit]) -> Result<PathBuf, WriteError> {
    let directory = directory.ok_or(WriteError::NoDirectory)?;
    let io = |path: &Path, e: std::io::Error| WriteError::Io {
        path: path.display().to_string(),
        why: e.to_string(),
    };
    std::fs::create_dir_all(directory).map_err(|e| io(directory, e))?;
    let path = directory.join(config::FILE_NAME);
    let shown = path.display().to_string();
    let target = std::fs::canonicalize(&path).unwrap_or_else(|_| path.clone());

    // An absent file is a valid state, so that is a create. Any other failure to
    // read is not "empty": treating it so would replace the person's file with one
    // holding a single key.
    let original = match std::fs::read_to_string(&target) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => {
            return Err(WriteError::Unreadable {
                path: shown,
                why: e.to_string(),
            })
        }
    };
    if let Err(e) = toml::from_str::<Config>(&original) {
        return Err(WriteError::AlreadyInvalid {
            path: shown,
            why: e.message().to_string(),
        });
    }

    let edited = set_keys(&original, edits);
    if let Err(e) = toml::from_str::<Config>(&edited) {
        return Err(WriteError::Refused {
            path: shown,
            why: e.message().to_string(),
        });
    }

    let candidate = target.with_extension("toml.herdr-voice-candidate");
    write_candidate(&candidate, &edited).map_err(|e| {
        let _ = std::fs::remove_file(&candidate);
        io(&candidate, e)
    })?;
    // The target exists when it was read: carry its mode onto the candidate, so
    // the rename does not change who may read it.
    if let Ok(meta) = std::fs::metadata(&target) {
        let _ = std::fs::set_permissions(&candidate, meta.permissions());
    }
    std::fs::rename(&candidate, &target).map_err(|e| {
        let _ = std::fs::remove_file(&candidate);
        io(&path, e)
    })?;
    Ok(path)
}

/// Writes `text` to a new file that only its owner can read until the target's
/// mode replaces that: the text may contain a token.
fn write_candidate(path: &Path, text: &str) -> std::io::Result<()> {
    use std::io::Write;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)?.write_all(text.as_bytes())
}
```

`toml::de::Error::message()` is the method used in the design's probe; if it does not compile, use `e.to_string()` instead and keep the tests unchanged.

- [ ] **Step 4: Run the tests to see them pass**

`CARGO_BUILD_JOBS=6 perl -e 'alarm shift; exec @ARGV' 1500 cargo test --bin herdr-voice config_edit::`
Expected: all pass.

- [ ] **Step 5: Prove the guard is tested (AC-4)**

In `src/config_edit.rs` change the line `if let Err(e) = toml::from_str::<Config>(&edited) {` to `if false && toml::from_str::<Config>(&edited).is_err() {` and adjust nothing else so it compiles (remove the `e` use by replacing the block's `why: e.message().to_string()` with `why: String::new()`). Run `cargo test --bin herdr-voice config_edit::` once. Expected: `a_value_of_the_wrong_type_is_refused_and_the_file_is_untouched` and `one_refused_edit_among_two_changes_neither` FAIL. Restore the original lines exactly and run the tests again; expected: all pass. Record the two failing test names in `tasks/103/RUN_103.md` under a heading `### S3/S4 notes`.

- [ ] **Step 6: Commit**

```bash
git add src/config_edit.rs tasks/103/RUN_103.md
git commit -m "Add write_keys: a configuration edit that cannot leave a file that does not load

Parses the original and the edit as Config, refuses an unreadable file instead of
treating it as empty, and replaces the file by rename with its mode and symbolic
link kept. Tests for each (#49).

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 3: The model chooser uses the writer

**Files:**
- Modify: `src/chooser.rs`

**Interfaces:**
- Consumes: `config_edit::{Edit, quote, write_keys, WriteError}` (Tasks 1 and 2).
- Produces: nothing new. `chooser::run` behaves as before except that its error line now carries the writer's wording.

- [ ] **Step 1: Remove what moved**

In `src/chooser.rs` delete: the doc comment and body of `table_name` (lines 96 to 129), `set_model_key` (lines 131 to 184), `write_model_key` (lines 186 to 213), and the nine tests listed in Task 1 step 1 together with their section of the test module (lines 361 to 490). Keep `human`, `list`, `pick`, `Line`, `run`, and the tests `the_listing_shows_every_model_with_a_size_before_anything_is_downloaded`, `the_configured_model_is_marked_in_the_listing`, `sizes_are_rendered_the_way_a_person_reads_them`, `a_choice_outside_the_list_is_refused_rather_than_guessed`. Update the module comment at the top: replace the sentence that begins "The configuration edit is line-oriented" with "The configuration edit is `config_edit`'s.".

- [ ] **Step 2: Call the writer**

Replace the `match write_model_key(&vars, entry.identifier) {` arm in `run` (the whole `match`, lines 285 to 306) with:

```rust
    let edit = [crate::config_edit::Edit {
        table: "stt",
        key: "model",
        value: crate::config_edit::quote(entry.identifier),
    }];
    match crate::config_edit::write_keys(crate::config::directory(&vars).as_deref(), &edit) {
        Ok(path) => {
            println!(
                "{} is installed, and {} now names it.",
                entry.identifier,
                path.display()
            );
            println!("The daemon still holds the previous model. Restart it to use this one.");
            0
        }
        Err(why) => {
            // The model is on disk and good; only the configuration edit failed,
            // so the person needs one line, not another three gigabytes.
            eprintln!(
                "{} is installed, but the configuration could not be written: {why}",
                entry.identifier
            );
            eprintln!("Add this to [stt] in your configuration file by hand:");
            eprintln!("  model = \"{}\"", entry.identifier);
            1
        }
    }
```

- [ ] **Step 3: Run the tests**

`CARGO_BUILD_JOBS=6 perl -e 'alarm shift; exec @ARGV' 1500 cargo test --bin herdr-voice chooser::` then `... cargo test --bin herdr-voice config_edit::`
Expected: both pass; `chooser::` has four tests.

- [ ] **Step 4: Commit**

```bash
git add src/chooser.rs
git commit -m "Make the model chooser write through config_edit

The line edit and its tests live in config_edit now; the chooser asks for one key.

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 4: The reload reply, `src/reload.rs`

**Files:**
- Create: `src/reload.rs`
- Modify: `src/main.rs` (add `mod reload;` after `mod record;`)

**Interfaces:**
- Consumes: nothing.
- Produces (exact):
  - `pub struct Applied { pub applied: Vec<String>, pub restart: Vec<String> }` with `Debug, PartialEq, Eq`
  - `pub fn reply(applied: &[&str], restart: &[&str]) -> String`
  - `pub fn parse(text: &str) -> Option<Applied>`

- [ ] **Step 1: Write the failing tests**

Create `src/reload.rs`:

```rust
//! What the daemon answers to a `reload` request, and how a popup reads it.
//!
//! One line, because a reply travels as one line (`src/proto.rs`). The writer and
//! the reader sit together so that one cannot change without the other's test
//! failing. See `tasks/103/DESIGN_103.md`, section 2.4.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_applied_and_nothing_to_restart() {
        assert_eq!(reply(&[], &[]), "applied: nothing");
    }

    #[test]
    fn audio_applied() {
        assert_eq!(reply(&["audio"], &[]), "applied: audio");
    }

    #[test]
    fn a_restart_is_named_after_what_was_applied() {
        assert_eq!(
            reply(&["audio"], &["stt", "rewrite"]),
            "applied: audio; needs a restart: stt, rewrite"
        );
        assert_eq!(
            reply(&[], &["stt"]),
            "applied: nothing; needs a restart: stt"
        );
    }

    #[test]
    fn what_is_written_is_read_back() {
        for (applied, restart) in [
            (vec![], vec![]),
            (vec!["audio"], vec![]),
            (vec![], vec!["stt"]),
            (vec!["audio"], vec!["stt", "rewrite", "ui"]),
        ] {
            let text = reply(&applied, &restart);
            let read = parse(&text).unwrap_or_else(|| panic!("{text:?} must parse"));
            assert_eq!(read.applied, applied, "{text:?}");
            assert_eq!(read.restart, restart, "{text:?}");
        }
    }

    #[test]
    fn what_is_not_a_reload_reply_is_not_read_as_one() {
        for text in ["pong", "", "applied:", "applied audio", "stopping", "nothing to cancel"] {
            assert_eq!(parse(text), None, "{text:?}");
        }
    }
}
```

- [ ] **Step 2: Run to see it fail**

Add `mod reload;` to `src/main.rs`. Run `CARGO_BUILD_JOBS=6 perl -e 'alarm shift; exec @ARGV' 1500 cargo test --bin herdr-voice reload::`
Expected: compile error, `cannot find function reply`.

- [ ] **Step 3: Implement**

Above the tests:

```rust
/// What a reload reply says.
#[derive(Debug, PartialEq, Eq)]
pub struct Applied {
    /// The sections the daemon now uses the new values of.
    pub applied: Vec<String>,
    /// The sections whose values in the file differ from what the daemon runs and
    /// that only a restart applies.
    pub restart: Vec<String>,
}

const RESTART: &str = "; needs a restart: ";

pub fn reply(applied: &[&str], restart: &[&str]) -> String {
    let head = if applied.is_empty() {
        "applied: nothing".to_string()
    } else {
        format!("applied: {}", applied.join(", "))
    };
    if restart.is_empty() {
        head
    } else {
        format!("{head}{RESTART}{}", restart.join(", "))
    }
}

pub fn parse(text: &str) -> Option<Applied> {
    let (head, tail) = match text.split_once(RESTART) {
        Some((head, tail)) => (head, Some(tail)),
        None => (text, None),
    };
    let applied = head.strip_prefix("applied: ")?;
    let list = |s: &str| -> Vec<String> { s.split(", ").map(str::to_string).collect() };
    Some(Applied {
        applied: if applied == "nothing" {
            Vec::new()
        } else {
            list(applied)
        },
        restart: tail.map(list).unwrap_or_default(),
    })
}
```

`parse("applied:")` returns `None` because `strip_prefix("applied: ")` needs the space; `parse("applied: ")` would return `Some` with one empty name, which no writer produces and is not tested.

- [ ] **Step 4: Run to see it pass**

Same command. Expected: 5 tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/reload.rs src/main.rs
git commit -m "Add the reload reply: one line, written and read in one place

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 5: The recorder takes a new `[audio]`, and the input names

**Files:**
- Modify: `src/capture.rs` (the `Command` enum near line 165, `Recorder::spawn` near line 196, `tests_support` near line 415, tests near line 495)
- Modify: `src/capture/cpal_source.rs`

**Interfaces:**
- Consumes: `crate::config::Audio`.
- Produces (exact):
  - `Recorder::reconfigure(&self, audio: Audio) -> bool` — true when the recorder's thread took it
  - `pub fn input_names() -> Result<Vec<String>, String>` in `src/capture/cpal_source.rs`
  - `capture::tests_support::RecordingSource(pub std::sync::Arc<std::sync::Mutex<Vec<Option<String>>>>)`, a `Source` that pushes the device each `start` was given

- [ ] **Step 1: Write the failing tests**

In the `tests` module of `src/capture.rs` (after `an_empty_configured_name_asks_for_the_default`):

```rust
    #[test]
    fn a_reconfigure_changes_the_input_the_next_take_opens() {
        let asked_for = Arc::new(Mutex::new(None));
        let seen = Arc::clone(&asked_for);
        let recorder = Recorder::spawn(
            move || {
                let mut fake = Fake::new(vec![Event::Samples(tone(0.3, 0.1))]);
                fake.asked_for = seen;
                Box::new(fake)
            },
            Audio {
                input: "Old".to_string(),
                ..Audio::default()
            },
            takes_dir("reconfigure"),
        );
        assert_eq!(recorder.start("w1:p2", None, None, None), Started::Began);
        assert_eq!(asked_for.lock().unwrap().as_deref(), Some("Old"));
        let take = recorder.stop().expect("a take");
        std::fs::remove_file(&take.path).ok();

        assert!(recorder.reconfigure(Audio {
            input: "New".to_string(),
            ..Audio::default()
        }));
        assert_eq!(recorder.start("w1:p2", None, None, None), Started::Began);
        assert_eq!(
            asked_for.lock().unwrap().as_deref(),
            Some("New"),
            "the next take must open the new input"
        );
    }

    #[test]
    fn a_reconfigure_while_a_take_runs_does_not_move_that_take() {
        // The take is quiet on purpose: its refusal names the device it was
        // recorded from, which is how a test sees which input it ended on.
        let (recorder, _) = recorder_named(
            "reconfigure-mid-take",
            vec![Event::Samples(tone(0.00002, 1.0))],
            Audio {
                input: "Headset".to_string(),
                ..Audio::default()
            },
        );
        assert_eq!(recorder.start("w1:p2", None, None, None), Started::Began);
        assert!(recorder.reconfigure(Audio {
            input: "Other".to_string(),
            ..Audio::default()
        }));
        let message = recorder.stop().expect_err("too quiet").to_string();
        assert!(message.contains("Headset"), "the take ended on its own input: {message}");
        assert!(!message.contains("Other"), "{message}");
    }
```

In `src/capture.rs`, `tests_support`, add (after `ToneSource`):

```rust
    /// Remembers the device every `start` was asked for, in order. The daemon's
    /// tests use it to see which input a take opened after a reload.
    pub struct RecordingSource(pub std::sync::Arc<std::sync::Mutex<Vec<Option<String>>>>);

    impl Source for RecordingSource {
        fn start(&mut self, device: Option<&str>, sink: Sink) -> Result<Format, String> {
            if let Ok(mut asked) = self.0.lock() {
                asked.push(device.map(str::to_string));
            }
            sink.push(Event::Samples(vec![0.0; 4_800]));
            Ok(Format {
                rate: 48_000,
                channels: 1,
            })
        }

        fn stop(&mut self) {}
    }
```

- [ ] **Step 2: Run to see them fail**

`CARGO_BUILD_JOBS=6 perl -e 'alarm shift; exec @ARGV' 1500 cargo test --bin herdr-voice capture::`
Expected: compile error, `no method named reconfigure`.

- [ ] **Step 3: Implement**

In `src/capture.rs` add a variant to the `Command` enum (after `Stop { reply: ... },`):

```rust
    /// Replace the `[audio]` values the next take starts with. A take already
    /// recording keeps the device it was started on.
    Reconfigure {
        audio: Audio,
        reply: mpsc::Sender<()>,
    },
```

In `Recorder::spawn`, make the thread's copy mutable and handle the order. Before `while let Ok(order) = orders.recv() {` add `let mut audio = audio;`, and add this arm after the `Command::Stop { reply } => { ... }` arm:

```rust
                    Command::Reconfigure { audio: next, reply } => {
                        audio = next;
                        let _ = reply.send(());
                    }
```

Add the method to `impl Recorder` after `stop`:

```rust
    /// Hand the recorder new `[audio]` values, and wait until it has them. The
    /// order is handled between the orders already queued, so a take that is
    /// recording is not touched. `false` means the recorder's thread is gone.
    pub fn reconfigure(&self, audio: Audio) -> bool {
        let (reply, answer) = mpsc::channel();
        let sent = self
            .commands
            .lock()
            .map(|commands| commands.send(Command::Reconfigure { audio, reply }));
        matches!(sent, Ok(Ok(()))) && answer.recv().is_ok()
    }
```

In `src/capture/cpal_source.rs`, after `impl CpalSource { ... }` add:

```rust
/// The names of the machine's input devices, the same strings `CpalSource::start`
/// matches a configured name against, so the microphone popup and a take agree on
/// what a device is called.
pub fn input_names() -> Result<Vec<String>, String> {
    let host = cpal::default_host();
    let devices = host
        .input_devices()
        .map_err(|e| format!("cannot list input devices: {e}"))?;
    Ok(devices.map(|d| d.to_string()).collect())
}
```

If clippy flags `input_names` as unused at this point, add `#[allow(dead_code)]` with the comment `// Called by src/mic.rs (Task 8).` and remove the attribute in Task 8 step 5.

- [ ] **Step 4: Run to see them pass**

`CARGO_BUILD_JOBS=6 perl -e 'alarm shift; exec @ARGV' 1500 cargo test --bin herdr-voice capture::`
Expected: all `capture::` tests pass, including the two new ones.

- [ ] **Step 5: Commit**

```bash
git add src/capture.rs src/capture/cpal_source.rs
git commit -m "Let the recorder take new [audio] values between takes

A take that is recording keeps its device. input_names lists the inputs by the same
names a take matches against.

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 6: The daemon answers `reload`

**Files:**
- Modify: `src/daemon.rs`

**Interfaces:**
- Consumes: `Recorder::reconfigure` (Task 5), `reload::reply` (Task 4), `capture::tests_support::RecordingSource` (Task 5), `config::{load, Source}`.
- Produces (exact):
  - field `pub running: std::sync::Mutex<crate::config::Config>` on `Runtime`
  - `pub struct ReloadPlan { pub audio: Option<crate::config::Audio>, pub restart: Vec<&'static str> }`
  - `pub fn plan_reload(running: &crate::config::Config, loaded: &crate::config::Config) -> ReloadPlan`
  - `pub fn reload_from(directory: Option<&std::path::Path>, recorder: &Recorder, runtime: &Runtime) -> Reply`
  - the request `"reload"` in `answer`

- [ ] **Step 1: Write the failing tests**

In the `tests` module of `src/daemon.rs`, after `fn request(...)` (line 1677):

```rust
    fn config_dir(tag: &str, text: Option<&str>) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("herdr-voice-reload-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        if let Some(text) = text {
            std::fs::write(dir.join("config.toml"), text).unwrap();
        }
        dir
    }

    /// A recorder whose source remembers every device it was asked to open.
    fn recording_recorder() -> (
        Recorder,
        std::sync::Arc<std::sync::Mutex<Vec<Option<String>>>>,
    ) {
        let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let source = std::sync::Arc::clone(&seen);
        let recorder = Recorder::spawn(
            move || Box::new(crate::capture::tests_support::RecordingSource(source)),
            crate::config::Audio::default(),
            std::env::temp_dir().join(format!("daemon-reload-takes-{}", std::process::id())),
        );
        (recorder, seen)
    }

    #[test]
    fn a_configuration_equal_to_the_running_one_plans_nothing() {
        let config = crate::config::Config::default();
        let plan = plan_reload(&config, &config);
        assert_eq!(plan.audio, None);
        assert!(plan.restart.is_empty());
    }

    #[test]
    fn a_changed_audio_section_is_applied_and_the_others_are_not_named() {
        let running = crate::config::Config::default();
        let mut loaded = running.clone();
        loaded.audio.input = "Headset".to_string();
        let plan = plan_reload(&running, &loaded);
        assert_eq!(plan.audio.map(|a| a.input), Some("Headset".to_string()));
        assert!(plan.restart.is_empty());
    }

    #[test]
    fn every_other_changed_section_needs_a_restart_and_is_named_in_a_fixed_order() {
        let running = crate::config::Config::default();
        let mut loaded = running.clone();
        loaded.stt.language = "ru".to_string();
        loaded.rewrite.agent = "other".to_string();
        loaded.ui.toasts = !running.ui.toasts;
        loaded.delivery.submit = !running.delivery.submit;
        loaded.context.file_names += 1;
        loaded.ptt.release_ms += 1;
        loaded.record.transcripts = !running.record.transcripts;
        let plan = plan_reload(&running, &loaded);
        assert_eq!(plan.audio, None);
        assert_eq!(
            plan.restart,
            vec!["stt", "rewrite", "ui", "delivery", "context", "ptt", "record"]
        );
    }

    #[test]
    fn a_reload_applies_audio_and_the_next_take_opens_the_new_input() {
        let (runtime, recorder, seen) = {
            let (recorder, seen) = recording_recorder();
            (fake_runtime("x"), recorder, seen)
        };
        let dir = config_dir("audio", Some("[audio]\ninput = \"New\"\n"));
        let reply = reload_from(Some(&dir), &recorder, &runtime);
        assert_eq!(reply, Reply::Ok("applied: audio".to_string()));
        assert_eq!(runtime.running.lock().unwrap().audio.input, "New");
        assert_eq!(recorder.start("w1:p2", None, None, None), crate::capture::Started::Began);
        assert_eq!(seen.lock().unwrap().as_slice(), [Some("New".to_string())]);
    }

    #[test]
    fn a_reload_with_nothing_different_applies_nothing() {
        let (recorder, _) = recording_recorder();
        let runtime = fake_runtime("x");
        let dir = config_dir("same", Some("[audio]\nsilence_db = -60.0\n"));
        assert_eq!(
            reload_from(Some(&dir), &recorder, &runtime),
            Reply::Ok("applied: nothing".to_string())
        );
    }

    #[test]
    fn another_section_is_named_as_needing_a_restart_every_time_until_the_daemon_restarts() {
        let (recorder, _) = recording_recorder();
        let runtime = fake_runtime("x");
        let dir = config_dir("stt", Some("[stt]\nlanguage = \"ru\"\n"));
        let said = Reply::Ok("applied: nothing; needs a restart: stt".to_string());
        assert_eq!(reload_from(Some(&dir), &recorder, &runtime), said);
        assert_eq!(
            reload_from(Some(&dir), &recorder, &runtime),
            said,
            "the running configuration still holds the old value"
        );
    }

    #[test]
    fn a_file_that_does_not_parse_changes_nothing_and_says_why() {
        let (recorder, seen) = recording_recorder();
        let runtime = fake_runtime("x");
        runtime.running.lock().unwrap().audio.input = "Kept".to_string();
        let dir = config_dir("invalid", Some("[audio\ninput = "));
        let Reply::Error(said) = reload_from(Some(&dir), &recorder, &runtime) else {
            panic!("an unparsable file must be an error");
        };
        assert!(said.contains("does not parse"), "{said}");
        assert!(said.contains("nothing was changed"), "{said}");
        assert_eq!(runtime.running.lock().unwrap().audio.input, "Kept");
        assert_eq!(recorder.start("w1:p2", None, None, None), crate::capture::Started::Began);
        assert_eq!(
            seen.lock().unwrap().as_slice(),
            [None],
            "the recorder was not given the defaults"
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_file_that_cannot_be_read_is_not_taken_for_an_absent_one() {
        use std::os::unix::fs::PermissionsExt;
        let (recorder, _) = recording_recorder();
        let runtime = fake_runtime("x");
        runtime.running.lock().unwrap().audio.input = "Kept".to_string();
        let dir = config_dir("unreadable", Some("[audio]\ninput = \"Other\"\n"));
        let file = dir.join("config.toml");
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o000)).unwrap();
        let reply = reload_from(Some(&dir), &recorder, &runtime);
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600)).unwrap();
        let Reply::Error(said) = reply else {
            panic!("an unreadable file must be an error");
        };
        assert!(said.contains("cannot read"), "{said}");
        assert_eq!(runtime.running.lock().unwrap().audio.input, "Kept");
    }

    #[test]
    fn the_request_is_known_to_the_daemon() {
        let (recorder, _) = recording_recorder();
        let runtime = fake_runtime("x");
        assert!(!needs_target_pane("reload"));
        let (reply, control) = answer(&request("reload", b""), &recorder, &runtime);
        assert!(matches!(control, Control::Continue));
        assert!(
            !matches!(&reply, Reply::Error(said) if said.starts_with("unknown command")),
            "reload must not fall through to the unknown-command arm: {reply:?}"
        );
    }
```

- [ ] **Step 2: Run to see them fail**

`CARGO_BUILD_JOBS=6 perl -e 'alarm shift; exec @ARGV' 1500 cargo test --bin herdr-voice daemon::tests::`
Expected: compile errors (`running` is not a field; `plan_reload` and `reload_from` do not exist).

- [ ] **Step 3: Implement**

1. Add the field to `Runtime` (after `takes`, before the closing `}` at line 148):

```rust
    /// The configuration this daemon is running on: what was read at start, with
    /// `[audio]` replaced each time a reload applies it. A section whose value in
    /// the file differs from this one and that a reload cannot apply is reported
    /// as needing a restart every time, because this still holds the old value.
    pub running: std::sync::Mutex<crate::config::Config>,
```

2. In `start()`, right after `let loaded = config::load(config::directory(&vars).as_deref());` (line 1306) add `let started_with = loaded.config.clone();`, and in the `Runtime { ... }` literal at line 1348 add, after `takes,`: `running: std::sync::Mutex::new(started_with),`.

3. Add `running: std::sync::Mutex::new(crate::config::Config::default()),` after the line `takes: std::path::PathBuf::new(),` in each of the four test literals at lines 1566, 1626, 1768 and 2416.

4. Add the functions after `answer` (before `fn dictate`):

```rust
/// What a reload would do, given what the daemon runs and what the file now says.
pub struct ReloadPlan {
    /// The new `[audio]`, when it differs.
    pub audio: Option<crate::config::Audio>,
    /// Every other section that differs: only a restart applies it.
    pub restart: Vec<&'static str>,
}

pub fn plan_reload(running: &config::Config, loaded: &config::Config) -> ReloadPlan {
    let mut restart = Vec::new();
    if running.stt != loaded.stt {
        restart.push("stt");
    }
    if running.rewrite != loaded.rewrite {
        restart.push("rewrite");
    }
    if running.ui != loaded.ui {
        restart.push("ui");
    }
    if running.delivery != loaded.delivery {
        restart.push("delivery");
    }
    if running.context != loaded.context {
        restart.push("context");
    }
    if running.ptt != loaded.ptt {
        restart.push("ptt");
    }
    if running.record != loaded.record {
        restart.push("record");
    }
    ReloadPlan {
        audio: (running.audio != loaded.audio).then(|| loaded.audio.clone()),
        restart,
    }
}

/// `reload`: read the configuration file again, apply what can be applied
/// between takes, and say what was and what needs a restart.
///
/// This runs on the connection's thread, not on the recorder's, and is asked for
/// by a popup after it wrote the file, so reading the file here puts no file access
/// on the path of a keypress (the reason the daemon reads once, at start).
pub fn reload_from(
    directory: Option<&std::path::Path>,
    recorder: &Recorder,
    runtime: &Runtime,
) -> Reply {
    let loaded = config::load(directory);
    match &loaded.source {
        config::Source::Invalid { path, why } => {
            return Reply::Error(format!(
                "{} does not parse: {why}; nothing was changed. Fix the file and try again",
                path.display()
            ))
        }
        // `load` reports an unreadable file the same way as an absent one. Applying
        // the defaults for a file that exists would reset the person's settings.
        config::Source::Defaults(Some(path)) if path.exists() => {
            return Reply::Error(format!(
                "cannot read {}; nothing was changed. Check its permissions and try again",
                path.display()
            ))
        }
        _ => {}
    }
    let mut running = match runtime.running.lock() {
        Ok(running) => running,
        Err(poisoned) => poisoned.into_inner(),
    };
    let plan = plan_reload(&running, &loaded.config);
    let mut applied = Vec::new();
    if let Some(audio) = plan.audio {
        if !recorder.reconfigure(audio.clone()) {
            return Reply::Error(
                "the recorder thread is gone; nothing was applied. Restart herdr".to_string(),
            );
        }
        running.audio = audio;
        applied.push("audio");
    }
    Reply::Ok(crate::reload::reply(&applied, &plan.restart))
}
```

5. Add the arm to `answer`, after the `"cancel" => (...)` arm and before `command if needs_target_pane(command) => {`:

```rust
        "reload" => (
            reload_from(
                config::directory(&config::Vars::from_env()).as_deref(),
                recorder,
                runtime,
            ),
            Control::Continue,
        ),
```

`Config`, `Audio` and the section structs derive `Clone` and `PartialEq` already (`src/config.rs`), so no derive changes are needed.

- [ ] **Step 4: Run to see them pass**

`CARGO_BUILD_JOBS=6 perl -e 'alarm shift; exec @ARGV' 1500 cargo test --bin herdr-voice daemon::tests::`
Expected: every daemon test passes, the new ones included.

- [ ] **Step 5: Commit**

```bash
git add src/daemon.rs
git commit -m "Answer a reload request: apply [audio] between takes, name what needs a restart

The daemon keeps the configuration it is running on. An invalid or unreadable file
changes nothing and says why.

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 7: The client's exchange

**Files:**
- Modify: `src/client.rs`

**Interfaces:**
- Consumes: nothing new.
- Produces (exact): `pub fn exchange(address: &Address, command: &str, entrypoint: Option<String>, context: Vec<u8>) -> Result<Reply, ClientError>`; `send_to` keeps its signature and behaviour.

- [ ] **Step 1: Write the failing test**

In the `tests` module of `src/client.rs`:

```rust
    #[test]
    fn exchange_with_nobody_listening_says_no_daemon_rather_than_a_code() {
        let dir = std::env::temp_dir().join(format!("herdr-voice-exchange-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let address = transport::address(&transport::Vars {
            state_dir: Some(dir.display().to_string()),
            xdg_state_home: None,
            home: None,
        })
        .expect("an address");
        assert!(matches!(
            exchange(&address, "reload", None, Vec::new()),
            Err(ClientError::NoDaemon(_))
        ));
    }
```

- [ ] **Step 2: Run to see it fail**

`CARGO_BUILD_JOBS=6 perl -e 'alarm shift; exec @ARGV' 1500 cargo test --bin herdr-voice client::`
Expected: compile error, `cannot find function exchange`.

- [ ] **Step 3: Implement**

Replace the whole of `send_to` (`src/client.rs:109-140`) with:

```rust
pub fn send_to(
    address: &Address,
    command: &str,
    entrypoint: Option<String>,
    context: Vec<u8>,
) -> Outcome {
    outcome(exchange(address, command, entrypoint, context))
}

/// One connection, one frame, one reply: what `send_to` does, before every result
/// is turned into a code and a message. A caller that has to tell "no daemon" from
/// "the daemon refused" reads this and not the message text.
pub fn exchange(
    address: &Address,
    command: &str,
    entrypoint: Option<String>,
    context: Vec<u8>,
) -> Result<Reply, ClientError> {
    let waited = timeout_for(command);
    let mut stream = match transport::connect(address) {
        Ok(stream) => stream,
        Err(_) => return Err(ClientError::NoDaemon(address.display().to_string())),
    };

    let request = Request {
        command: command.to_string(),
        entrypoint,
        context,
    };
    if let Err(e) = request.write_to(&mut stream) {
        return Err(ClientError::Transport(e.to_string()));
    }

    // The reply is read on another thread so a daemon that never answers costs a
    // bounded wait rather than a hang. The process exits right after, so the
    // abandoned thread has nothing to clean up.
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let mut reader = BufReader::new(stream);
        let _ = sender.send(Reply::read_from(&mut reader).map_err(|e| e.to_string()));
    });

    match receiver.recv_timeout(waited) {
        Ok(Ok(reply)) => Ok(reply),
        Ok(Err(why)) => Err(ClientError::Protocol(why)),
        Err(_) => Err(ClientError::Timeout(waited)),
    }
}
```

- [ ] **Step 4: Run to see it pass, and the callers' tests**

`CARGO_BUILD_JOBS=6 perl -e 'alarm shift; exec @ARGV' 1500 cargo test --bin herdr-voice client::` then `... cargo test --bin herdr-voice doctor::`
Expected: both pass (`doctor` calls `send_to`).

- [ ] **Step 5: Commit**

```bash
git add src/client.rs
git commit -m "Split the client's exchange from the code and message it is turned into

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 8: The popup and the action that opens it, `src/mic.rs`

**Files:**
- Create: `src/mic.rs`
- Modify: `src/main.rs` (add `mod mic;` after `mod indicator;`)

**Interfaces:**
- Consumes: `config_edit::{Edit, quote, write_keys, WriteError}` (Tasks 1, 2), `reload::{parse, Applied}` (Task 4), `capture::cpal_source::input_names` (Task 5), `client::{exchange, outcome, ClientError}` (Task 7), `outward::{run, RunError}`, `delivery::herdr_binary`, `transport::{address, PLUGIN_ID, Vars}`, `proto::Reply`.
- Produces (exact):
  - `pub enum Reached { Applied(reload::Applied), NoDaemon, Failed(String) }`
  - `pub fn reached(result: Result<Reply, ClientError>) -> Reached`
  - `pub fn render(names: &[String], configured: &str) -> String`
  - `pub enum Answer { Leave, Pick(usize), Invalid(String) }` and `pub fn parse_answer(line: &str, count: usize) -> Answer`
  - `pub fn prompt(out: &mut dyn Write)`
  - `pub fn choose(names: &[String], configured: &str, input: &mut dyn BufRead, out: &mut dyn Write, save: &mut dyn FnMut(&str) -> Result<PathBuf, WriteError>, tell: &mut dyn FnMut() -> Reached) -> u8`
  - `pub fn run(choosing: bool) -> u8`
  - `pub fn open_command(herdr: &str, plugin: &str) -> Vec<String>` and `pub fn open() -> u8`

The design's `Reached::Applied { restart }` carries the whole `reload::Applied` here, because the message for "nothing to apply" needs the applied list too.

- [ ] **Step 1: Write the failing tests**

Create `src/mic.rs` with the module comment and this test module only:

```rust
//! Choosing the microphone: the popup `mic --choose` and the action `mic --open`
//! that opens it. See `tasks/103/DESIGN_103.md`, sections 2.2 and 2.5.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::ClientError;
    use crate::proto::Reply;
    use std::io::Cursor;

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    /// Runs `choose` with the typed `answer`, a writer that stores `Ok(path)` or
    /// fails with `save_error`, and a daemon that answers `reached`. Returns the
    /// exit code, what was printed, and the names that were saved.
    fn run_choose(
        list: &[&str],
        configured: &str,
        answer: &str,
        save_error: Option<WriteError>,
        reached: Reached,
    ) -> (u8, String, Vec<String>) {
        let list = names(list);
        let mut input = Cursor::new(answer.as_bytes().to_vec());
        let mut out: Vec<u8> = Vec::new();
        let saved = std::cell::RefCell::new(Vec::new());
        let mut save_error = save_error;
        let mut save = |name: &str| {
            saved.borrow_mut().push(name.to_string());
            match save_error.take() {
                Some(error) => Err(error),
                None => Ok(PathBuf::from("config.toml")),
            }
        };
        let mut told = Some(reached);
        let mut tell = || told.take().unwrap_or(Reached::NoDaemon);
        let code = choose(&list, configured, &mut input, &mut out, &mut save, &mut tell);
        (code, String::from_utf8(out).unwrap(), saved.into_inner())
    }

    fn applied(applied: &[&str], restart: &[&str]) -> Reached {
        Reached::Applied(crate::reload::Applied {
            applied: applied.iter().map(|s| s.to_string()).collect(),
            restart: restart.iter().map(|s| s.to_string()).collect(),
        })
    }

    #[test]
    fn the_list_numbers_every_input_and_marks_the_configured_one() {
        let text = render(&names(&["Built-in", "Headset"]), "Headset");
        assert!(text.contains("1. Built-in"), "{text}");
        let headset: Vec<&str> = text.lines().filter(|l| l.contains("Headset")).collect();
        assert_eq!(headset.len(), 1, "{text}");
        assert!(headset[0].contains("2. Headset") && headset[0].contains("(current)"), "{text}");
        assert!(!text.lines().any(|l| l.contains("Built-in") && l.contains("(current)")), "{text}");
    }

    #[test]
    fn a_configured_name_that_matches_nothing_is_said_and_names_the_setting() {
        let text = render(&names(&["Built-in"]), "Old headset");
        assert!(text.contains("[audio] input"), "{text}");
        assert!(text.contains("\"Old headset\""), "{text}");
        assert!(text.contains("matches none"), "{text}");
        assert!(!text.contains("(current)"), "{text}");
    }

    #[test]
    fn an_unset_input_says_the_default_is_used() {
        let text = render(&names(&["Built-in"]), "");
        assert!(text.contains("not set"), "{text}");
        assert!(text.contains("default input"), "{text}");
    }

    #[test]
    fn two_inputs_with_one_name_are_both_listed_and_the_second_says_the_first_is_used() {
        let text = render(&names(&["USB Mic", "Built-in", "USB Mic"]), "USB Mic");
        assert!(text.contains("3. USB Mic"), "{text}");
        let marked: Vec<&str> = text.lines().filter(|l| l.contains("(current)")).collect();
        assert_eq!(marked.len(), 1, "only the first is the one selected: {text}");
        assert!(marked[0].contains("1. USB Mic"), "{text}");
        let third = text.lines().find(|l| l.contains("3. USB Mic")).unwrap();
        assert!(third.contains("same name as 1"), "{text}");
        assert!(third.contains("uses the first"), "{text}");
    }

    #[test]
    fn answers_are_read_the_way_the_prompt_says() {
        assert_eq!(parse_answer("2\n", 3), Answer::Pick(1));
        assert_eq!(parse_answer(" 1 \r\n", 3), Answer::Pick(0));
        assert_eq!(parse_answer("\n", 3), Answer::Leave);
        assert_eq!(parse_answer("\u{1b}\n", 3), Answer::Leave);
        // An arrow key reaches a line read as an Esc-led sequence.
        assert_eq!(parse_answer("\u{1b}[A\n", 3), Answer::Leave);
        for bad in ["0\n", "4\n", "-1\n", "x\n", "2x\n"] {
            assert!(matches!(parse_answer(bad, 3), Answer::Invalid(_)), "{bad:?}");
        }
    }

    #[test]
    fn the_prompt_is_flushed_before_anyone_waits_for_the_answer() {
        #[derive(Default)]
        struct Watch {
            written: Vec<u8>,
            flushed_at: usize,
        }
        impl Write for Watch {
            fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
                self.written.extend_from_slice(bytes);
                Ok(bytes.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                self.flushed_at = self.written.len();
                Ok(())
            }
        }
        let mut watch = Watch::default();
        prompt(&mut watch);
        assert!(!watch.written.is_empty());
        assert_eq!(watch.flushed_at, watch.written.len(), "the whole prompt must be on screen");
        let text = String::from_utf8(watch.written).unwrap();
        assert!(text.contains("Enter"), "a keystroke alone is not an answer: {text}");
    }

    #[test]
    fn choosing_a_number_saves_that_inputs_name_and_says_the_next_take_uses_it() {
        let (code, said, saved) = run_choose(
            &["Built-in", "Headset"],
            "Built-in",
            "2\n",
            None,
            applied(&["audio"], &[]),
        );
        assert_eq!(code, 0, "{said}");
        assert_eq!(saved, vec!["Headset".to_string()], "the name, never the number");
        assert!(said.contains("\"Headset\""), "{said}");
        assert!(said.contains("next take"), "{said}");
    }

    #[test]
    fn esc_leaves_the_file_alone_and_says_so() {
        for typed in ["\u{1b}\n", "\n", "\u{1b}[B\n"] {
            let (code, said, saved) = run_choose(&["A", "B"], "A", typed, None, Reached::NoDaemon);
            assert_eq!(code, 0, "{typed:?}: {said}");
            assert!(saved.is_empty(), "{typed:?}");
            assert!(said.contains("Nothing was changed"), "{said}");
        }
    }

    #[test]
    fn an_answer_outside_the_list_is_refused_and_names_the_range() {
        let (code, said, saved) = run_choose(&["A", "B"], "A", "7\n", None, Reached::NoDaemon);
        assert_eq!(code, 1);
        assert!(saved.is_empty());
        assert!(said.contains("\"7\"") && said.contains("between 1 and 2"), "{said}");
    }

    #[test]
    fn no_input_devices_says_so_and_what_to_do_and_changes_nothing() {
        let (code, said, saved) = run_choose(&[], "", "1\n", None, Reached::NoDaemon);
        assert_eq!(code, 1);
        assert!(saved.is_empty());
        assert!(said.contains("No input devices"), "{said}");
        assert!(said.contains("Connect a microphone"), "{said}");
    }

    #[test]
    fn an_empty_terminal_is_not_an_answer() {
        let (code, said, saved) = run_choose(&["A"], "", "", None, Reached::NoDaemon);
        assert_eq!(code, 1, "{said}");
        assert!(saved.is_empty());
        assert!(said.contains("nothing was read"), "{said}");
    }

    #[test]
    fn a_daemon_that_already_uses_the_input_has_nothing_to_apply() {
        let (code, said, _) = run_choose(&["A", "B"], "A", "2\n", None, applied(&[], &[]));
        assert_eq!(code, 0, "{said}");
        assert!(said.contains("already uses it"), "{said}");
    }

    #[test]
    fn other_changed_sections_are_named_as_needing_a_restart() {
        let (code, said, _) = run_choose(&["A", "B"], "A", "2\n", None, applied(&["audio"], &["stt"]));
        assert_eq!(code, 0, "{said}");
        assert!(said.contains("stt"), "{said}");
        assert!(said.contains("restart of herdr"), "{said}");
    }

    #[test]
    fn no_daemon_means_the_change_applies_when_it_starts() {
        let (code, said, saved) = run_choose(&["A", "B"], "A", "2\n", None, Reached::NoDaemon);
        assert_eq!(code, 0, "{said}");
        assert_eq!(saved, vec!["B".to_string()], "the file is still written");
        assert!(said.contains("No dictation daemon is running"), "{said}");
        assert!(said.contains("applies when it starts"), "{said}");
    }

    #[test]
    fn a_daemon_that_did_not_take_it_fails_and_says_what_to_check() {
        let (code, said, _) = run_choose(
            &["A", "B"],
            "A",
            "2\n",
            None,
            Reached::Failed("the daemon did not answer within 2 seconds".to_string()),
        );
        assert_eq!(code, 1, "{said}");
        assert!(said.contains("did not answer within 2 seconds"), "{said}");
        assert!(said.contains("herdr plugin log list --plugin herdr-voice"), "{said}");
    }

    #[test]
    fn a_write_that_fails_prints_the_error_and_the_line_to_add_by_hand() {
        let (code, said, _) = run_choose(
            &["A", "Mic \"B\""],
            "A",
            "2\n",
            Some(WriteError::Io {
                path: "config.toml".to_string(),
                why: "permission denied".to_string(),
            }),
            Reached::NoDaemon,
        );
        assert_eq!(code, 1, "{said}");
        assert!(said.contains("permission denied"), "{said}");
        assert!(said.contains("Add this under [audio]"), "{said}");
        assert!(said.contains("input = \"Mic \\\"B\\\"\""), "the line must be valid TOML: {said}");
        assert!(!said.contains("daemon"), "the daemon is not told of a change that was not written: {said}");
    }

    #[test]
    fn every_result_of_the_exchange_maps_to_what_the_popup_does() {
        assert_eq!(
            reached(Ok(Reply::Ok("applied: audio".to_string()))),
            applied(&["audio"], &[])
        );
        assert_eq!(
            reached(Ok(Reply::Ok("applied: nothing; needs a restart: stt".to_string()))),
            applied(&[], &["stt"])
        );
        assert!(matches!(reached(Ok(Reply::Ok("pong".to_string()))), Reached::Failed(_)));
        assert_eq!(
            reached(Ok(Reply::Error("nothing was changed".to_string()))),
            Reached::Failed("nothing was changed".to_string())
        );
        assert_eq!(reached(Err(ClientError::NoDaemon("x".to_string()))), Reached::NoDaemon);
        for error in [
            ClientError::Timeout(std::time::Duration::from_secs(2)),
            ClientError::Transport("broken".to_string()),
            ClientError::Protocol("odd".to_string()),
        ] {
            assert!(matches!(reached(Err(error)), Reached::Failed(_)));
        }
    }

    #[test]
    fn the_action_opens_this_plugins_mic_pane_through_herdr() {
        assert_eq!(
            open_command("herdr", "herdr-voice"),
            vec![
                "herdr", "plugin", "pane", "open", "--plugin", "herdr-voice", "--entrypoint", "mic"
            ]
        );
    }
}
```

- [ ] **Step 2: Run to see them fail**

Add `mod mic;` to `src/main.rs`. Run `CARGO_BUILD_JOBS=6 perl -e 'alarm shift; exec @ARGV' 1500 cargo test --bin herdr-voice mic::`
Expected: compile errors, `cannot find ...` for each function and type.

- [ ] **Step 3: Implement**

Above the test module in `src/mic.rs`:

```rust
use std::io::{BufRead, IsTerminal, Write};
use std::path::PathBuf;
use std::time::Duration;

use crate::client::{self, ClientError};
use crate::config_edit::{self, Edit, WriteError};
use crate::proto::Reply;
use crate::reload;

/// The bound on asking herdr to open the popup, the same as every other call to
/// herdr (`docs/decisions.md`).
const OPEN_BOUND: Duration = Duration::from_secs(10);

/// What came of telling the daemon about the change.
#[derive(Debug, PartialEq, Eq)]
pub enum Reached {
    /// The daemon answered a reload.
    Applied(reload::Applied),
    /// Nothing is listening: the change applies when a daemon starts.
    NoDaemon,
    /// The daemon did not take it, with the reason in words.
    Failed(String),
}

/// Maps what `client::exchange` returned onto what the popup does. The wording of
/// a failure is the client's own, so a person sees the same sentence here as from
/// `herdr-voice cancel`.
pub fn reached(result: Result<Reply, ClientError>) -> Reached {
    match result {
        Err(ClientError::NoDaemon(_)) => Reached::NoDaemon,
        Ok(Reply::Ok(text)) => match reload::parse(&text) {
            Some(applied) => Reached::Applied(applied),
            None => Reached::Failed(format!("the daemon answered {text:?}, which is not a reload")),
        },
        Ok(Reply::Error(text)) => Reached::Failed(text),
        Err(other) => Reached::Failed(client::outcome(Err(other)).message.unwrap_or_default()),
    }
}

/// The inputs, numbered, with the one the configuration names marked and a line
/// about the setting when it does not match anything.
pub fn render(names: &[String], configured: &str) -> String {
    let mut out = String::from("Microphones this machine offers:\n\n");
    for (i, name) in names.iter().enumerate() {
        // The first input of a name is the one a take selects.
        let first = names.iter().position(|n| n == name).unwrap_or(i);
        let current = if !configured.is_empty() && name == configured && first == i {
            "  (current)"
        } else {
            ""
        };
        let same = if first != i {
            format!(
                "  (same name as {}; the plugin selects by name and uses the first)",
                first + 1
            )
        } else {
            String::new()
        };
        out.push_str(&format!("  {}. {name}{current}{same}\n", i + 1));
    }
    out.push('\n');
    if names.is_empty() {
        out.push_str("No input devices were found. Connect a microphone and open this again.\n");
    } else if configured.is_empty() {
        out.push_str("[audio] input is not set, so the system default input is used.\n");
    } else if !names.iter().any(|n| n == configured) {
        out.push_str(&format!(
            "[audio] input is {configured:?}, which matches none of these inputs; a take is \
             refused until you choose one of them.\n"
        ));
    }
    out
}

#[derive(Debug, PartialEq, Eq)]
pub enum Answer {
    /// Esc, or nothing: leave the file as it is.
    Leave,
    /// The zero-based position in the list.
    Pick(usize),
    Invalid(String),
}

/// A line is read, so Esc and the arrow keys arrive as text that begins with the
/// Esc character, and only after Enter. Anything that begins with it leaves.
pub fn parse_answer(line: &str, count: usize) -> Answer {
    let text = line.trim();
    if text.is_empty() || text.starts_with('\u{1b}') {
        return Answer::Leave;
    }
    match text.parse::<usize>() {
        Ok(n) if (1..=count).contains(&n) => Answer::Pick(n - 1),
        _ => Answer::Invalid(text.to_string()),
    }
}

/// The question, flushed: standard output is line buffered and this has no newline,
/// so without the flush it stays unseen while the process waits for the answer.
pub fn prompt(out: &mut dyn Write) {
    let _ = write!(
        out,
        "Type the number of the input, then Enter. Esc then Enter, or an empty line, leaves it as it is: "
    );
    let _ = out.flush();
}

/// The popup, over its inputs so a test needs no device and no terminal.
///
/// `save` writes `[audio] input`; `tell` asks the running daemon to take it. The
/// daemon is told only after the write succeeded.
pub fn choose(
    names: &[String],
    configured: &str,
    input: &mut dyn BufRead,
    out: &mut dyn Write,
    save: &mut dyn FnMut(&str) -> Result<PathBuf, WriteError>,
    tell: &mut dyn FnMut() -> Reached,
) -> u8 {
    let _ = write!(out, "{}", render(names, configured));
    if names.is_empty() {
        return 1;
    }
    prompt(out);
    let mut line = String::new();
    match input.read_line(&mut line) {
        Ok(0) | Err(_) => {
            let _ = writeln!(
                out,
                "\nnothing was read from the terminal; run `herdr-voice mic --choose` again"
            );
            return 1;
        }
        Ok(_) => {}
    }
    let name = match parse_answer(&line, names.len()) {
        Answer::Leave => {
            let _ = writeln!(out, "Nothing was changed.");
            return 0;
        }
        Answer::Invalid(text) => {
            let _ = writeln!(
                out,
                "{text:?} is not one of the numbers above. Open this again and type a number \
                 between 1 and {}",
                names.len()
            );
            return 1;
        }
        Answer::Pick(at) => names[at].as_str(),
    };

    let path = match save(name) {
        Ok(path) => path,
        Err(why) => {
            let _ = writeln!(out, "{why}");
            let _ = writeln!(out, "Add this under [audio] in your configuration file by hand:");
            let _ = writeln!(out, "  input = {}", config_edit::quote(name));
            return 1;
        }
    };
    let _ = writeln!(out, "[audio] input is now {name:?} in {}.", path.display());
    match tell() {
        Reached::Applied(done) => {
            if done.applied.iter().any(|s| s == "audio") {
                let _ = writeln!(out, "The daemon applied it: the next take records from it.");
            } else {
                let _ = writeln!(out, "The daemon already uses it; nothing to apply.");
            }
            if !done.restart.is_empty() {
                let _ = writeln!(
                    out,
                    "Other changes in the file need a restart of herdr to apply: {}.",
                    done.restart.join(", ")
                );
            }
            0
        }
        Reached::NoDaemon => {
            let _ = writeln!(
                out,
                "No dictation daemon is running, so nothing was told; the change applies when it starts."
            );
            0
        }
        Reached::Failed(why) => {
            let _ = writeln!(
                out,
                "The file was changed, but the daemon did not take it: {why}. Restart herdr, \
                 or check `herdr plugin log list --plugin herdr-voice`."
            );
            1
        }
    }
}

/// `herdr-voice mic`, and `--choose`. Returns the process's exit code.
pub fn run(choosing: bool) -> u8 {
    let names = match crate::capture::cpal_source::input_names() {
        Ok(names) => names,
        Err(why) => {
            eprintln!(
                "{why}. Check that an input is connected and that this program may use the microphone"
            );
            return 1;
        }
    };
    let vars = crate::config::Vars::from_env();
    let directory = crate::config::directory(&vars);
    let configured = crate::config::load(directory.as_deref()).config.audio.input;
    if !choosing {
        print!("{}", render(&names, &configured));
        return 0;
    }

    let mut out = std::io::stdout();
    let mut save = |name: &str| {
        config_edit::write_keys(
            directory.as_deref(),
            &[Edit {
                table: "audio",
                key: "input",
                value: config_edit::quote(name),
            }],
        )
    };
    let mut tell = || match crate::transport::address(&crate::transport::Vars::from_env()) {
        Ok(address) => reached(client::exchange(&address, "reload", None, Vec::new())),
        Err(e) => Reached::Failed(e.to_string()),
    };
    // The lock on standard input is released before `pause` reads from it again: the
    // lock is not re-entrant, and a second one taken on the same thread waits for the
    // first for ever, which in a popup is a pane that never closes.
    let code = {
        let stdin = std::io::stdin();
        let mut input = stdin.lock();
        choose(&names, &configured, &mut input, &mut out, &mut save, &mut tell)
    };
    pause();
    code
}

/// Waits for Enter when there is a person at a terminal, so a result printed just
/// before the process exits is not gone with the pane. Whether herdr closes a
/// popup the moment its command exits was not established
/// (`tasks/103/DESIGN_103.md`, decision 5).
fn pause() {
    if std::io::stdin().is_terminal() {
        print!("\nPress Enter to close.");
        let _ = std::io::stdout().flush();
        let mut ignored = String::new();
        let _ = std::io::stdin().read_line(&mut ignored);
    }
}

/// The command that asks herdr to open this plugin's `mic` pane.
pub fn open_command(herdr: &str, plugin: &str) -> Vec<String> {
    [
        herdr,
        "plugin",
        "pane",
        "open",
        "--plugin",
        plugin,
        "--entrypoint",
        "mic",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

/// `herdr-voice mic --open`: what the manifest's `mic` action runs.
pub fn open() -> u8 {
    let herdr = crate::delivery::herdr_binary();
    let plugin = std::env::var("HERDR_PLUGIN_ID")
        .unwrap_or_else(|_| crate::transport::PLUGIN_ID.to_string());
    let argv = open_command(&herdr, &plugin);
    let mut command = std::process::Command::new(&argv[0]);
    command.args(&argv[1..]);
    match crate::outward::run(&mut command, OPEN_BOUND) {
        Ok(output) if output.status.success() => 0,
        Ok(output) => {
            eprintln!(
                "herdr could not open the microphone popup: {}. Check \
                 `herdr plugin log list --plugin herdr-voice`",
                crate::outward::shorten(String::from_utf8_lossy(&output.stderr).trim(), 300)
            );
            1
        }
        Err(crate::outward::RunError::TimedOut) => {
            eprintln!(
                "herdr did not answer within {} seconds when asked to open the popup. Check \
                 that herdr is running, then try again",
                OPEN_BOUND.as_secs()
            );
            1
        }
        Err(crate::outward::RunError::Start(e)) => {
            eprintln!("cannot run {herdr:?}: {e}. Check that herdr is installed and on the PATH");
            1
        }
    }
}
```

- [ ] **Step 4: Run to see them pass**

`CARGO_BUILD_JOBS=6 perl -e 'alarm shift; exec @ARGV' 1500 cargo test --bin herdr-voice mic::`
Expected: all `mic::` tests pass. If `Reached` needs `Clone` or `Debug` for a test, it already derives `Debug, PartialEq, Eq`; do not add more.

- [ ] **Step 5: Wire nothing else yet, remove the temporary allow**

If Task 5 step 3 added `#[allow(dead_code)]` on `input_names`, remove it now. Run `CARGO_BUILD_JOBS=6 perl -e 'alarm shift; exec @ARGV' 1500 cargo clippy --all-targets -- -D warnings`. Expected: warnings that `run` and `open` are unused until Task 9 wires them. If so, finish Task 9 step 1 before running clippy, and run clippy there.

- [ ] **Step 6: Commit**

```bash
git add src/mic.rs src/main.rs src/capture/cpal_source.rs
git commit -m "Build the microphone popup and the action that opens it

Lists inputs by name, writes the choice through the writer, tells the running daemon,
and says what happened and what to do next in each case.

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 9: Wiring: the command, the manifest, the key, the decisions

**Files:**
- Modify: `src/main.rs`, `herdr-plugin.toml`, `src/setup.rs`, `docs/decisions.md`

**Interfaces:**
- Consumes: `mic::{run, open}` (Task 8).
- Produces: the user-visible command `herdr-voice mic [--choose | --open]`, the manifest action `mic`, the fourth binding.

- [ ] **Step 1: `src/main.rs`**

1. Replace the arm
```rust
        other @ (Command::Status | Command::Mic) => {
            eprintln!("{}: not implemented yet", other.name());
            ExitCode::from(NOT_IMPLEMENTED)
        }
```
with
```rust
        Command::Mic => {
            if args.iter().any(|a| a == "--open") {
                ExitCode::from(mic::open())
            } else {
                ExitCode::from(mic::run(args.iter().any(|a| a == "--choose")))
            }
        }
        other @ Command::Status => {
            eprintln!("{}: not implemented yet", other.name());
            ExitCode::from(NOT_IMPLEMENTED)
        }
```
2. In `IMPLEMENTED` (line 128) add `"mic"`: the list becomes `"daemon", "doctor", "cancel", "dictate", "model", "ptt", "setup", "mic"`.
3. In `USAGE` add, after the `model` line: `  herdr-voice mic        list the microphones, or --choose to switch the input`.
4. In the test `the_commands_this_issue_implements_are_not_in_the_unimplemented_arm`: add `"mic"` to the first `for name in [...]` list and change `for name in ["status", "mic"] {` to `for name in ["status"] {`.

- [ ] **Step 2: Run `main` tests**

`CARGO_BUILD_JOBS=6 perl -e 'alarm shift; exec @ARGV' 1500 cargo test --bin herdr-voice tests::` (the `main.rs` tests). Expected: pass; `the_usage_text_names_every_implemented_command` now covers `mic`.

- [ ] **Step 3: Manifest**

Append to the actions in `herdr-plugin.toml`, after the `setup` action (line 56) and before `[[panes]]`:

```toml
[[actions]]
id = "mic"
title = "Dictation: choose a microphone"
description = "Open the popup that lists the inputs and switches the one the next take records from"
contexts = ["global"]
command = ["target/release/herdr-voice", "mic", "--open"]
```

Run `python3 scripts/check_manifest.py`. Expected: `manifest: 13 entries, all commands known`. It prints `12` before the new action is added: the count is the `build`, `startup`, `actions` and `panes` entries (2, 2, 4 and 4 before, 5 actions after).

- [ ] **Step 4: The fourth binding and the nine tests in `src/setup.rs`**

Measured on a scratch copy: adding the binding fails exactly these nine tests, and the edits below fix them.

1. `pub static BINDINGS: [Binding; 3] = [` becomes `[Binding; 4]`, and a fourth entry is added after the `cancel` one:
```rust
    Binding {
        action: "mic",
        key: "prefix+shift+i",
        description: "dictation: choose a microphone",
    },
```
Add above it the comment `// A proposal until the owner names the key (tasks/103/AC_103.md, AC-40).`.
2. The message at line 878: `"\nnothing was added to {}: {} of the three bindings are on keys \` becomes `"\nnothing was added to {}: {} of the {} bindings are on keys \`, and its arguments `path.display(), decision.blocked.len()` become `path.display(), decision.blocked.len(), BINDINGS.len()`.
3. Test `the_three_bindings_are_the_ones_the_owner_chose` is renamed `the_bindings_are_the_ones_the_owner_chose` and its expected vector gains `("mic", "prefix+shift+i"),` after `("cancel", "ctrl+shift+g"),`.
4. In `what_is_rendered_parses_back_as_toml`, `assert_eq!(commands.len(), 3);` becomes `assert_eq!(commands.len(), BINDINGS.len());`.
5. In `the_question_has_reached_the_screen_before_the_answer_is_waited_for`, `on_screen.contains("3 bindings")` becomes `on_screen.contains("4 bindings")`.
6. `a_clean_configuration_takes_all_three` is renamed `a_clean_configuration_takes_all_four`; `assert_eq!(d.to_add.len(), 3);` becomes `4`.
7. In `a_binding_of_ours_that_is_already_there_is_not_added_again`, `assert_eq!(d.to_add.len(), 2);` becomes `3`.
8. In `a_key_held_by_something_else_blocks_that_block_and_no_other` and in `a_command_block_with_a_key_and_no_command_still_reserves_that_key`, `assert_eq!(d.to_add.len(), 2, "the other two are still added");` becomes `assert_eq!(d.to_add.len(), 3, "the other three are still added");`.
9. In `with_every_key_taken_it_says_nothing_was_added_rather_than_nothing_to_add`, add a fourth holder to the written file: after the `ctrl+shift+g` block add `\n\n[[keys.command]]\nkey = \"prefix+shift+i\"\ntype = \"shell\"\ncommand = \"four\"\n` (inside the same string literal, before the closing `",`), and extend the loop to `for holder in ["one", "two", "three", "four"] {`.
10. `every_action_the_snippet_names_is_declared_in_the_manifest` needs no edit: Step 3 made it pass.

Run `CARGO_BUILD_JOBS=6 perl -e 'alarm shift; exec @ARGV' 1500 cargo test --bin herdr-voice setup::`. Expected: all pass. Also grep for words that still say three: `grep -n "three" src/setup.rs README.md docs/design.md`; each hit that describes how many bindings `setup` offers must say four or count from the list (README.md line 59 says "so three blocks go into your" — change the word `three` to `four` and change nothing else in the README: it does not list the actions, so there is no list to extend).

- [ ] **Step 5: `docs/decisions.md`**

Append two rows at the end of the table, in the file's column order (Decision | Basis | Where):

```
| A popup tells the daemon of a changed configuration with a `reload` request after it has written the file; the daemon re-reads the file, applies `[audio]` between takes, and replies with what it applied and which sections need a restart | Re-reading before each take puts file access on the path of the keypress, the reason the daemon reads once (`src/daemon.rs`), and could not apply a speech-model change without replacing the resident engine; a request gives both popups one mechanism and gives the popup the answer it needs to say truthfully whether a change applies. A file edited by hand is not picked up until a restart | 2026-10-07, #103 |
| A key opens a popup through a manifest action whose command runs `herdr plugin pane open`, bound as a `plugin_action` | herdr's binding types are `shell`, `pane`, `popup` and `plugin_action`; none opens a plugin's declared pane, and an installed plugin opens its pane this way. Whether `pane open` honours the pane's `popup` placement without `--placement` is recorded in `docs/evidence.md` | 2026-10-07, #103 |
```

- [ ] **Step 6: Run the four gates**

```
export CARGO_BUILD_JOBS=6
perl -e 'alarm shift; exec @ARGV' 2400 cargo test
perl -e 'alarm shift; exec @ARGV' 1500 cargo clippy --all-targets -- -D warnings
cargo fmt --check
python3 scripts/check_manifest.py
```
Expected: every test passes, no clippy warning, `cargo fmt --check` prints nothing (run `cargo fmt` first if it does and look at the diff before committing), the manifest check prints its line. If `cargo test` fails in a test of `src/stt/fetch.rs` or a test that writes and runs a script (`Text file busy`, `NotFound`), rerun that test alone once and record it in `tasks/103/RUN_103.md`; any other failure is this change's.

- [ ] **Step 7: Commit**

```bash
git add src/main.rs src/setup.rs herdr-plugin.toml docs/decisions.md README.md
git commit -m "Wire the mic command, its manifest action and its key

mic is built, opened by an action, offered by setup as a fourth binding (the key is
a proposal), and the two decisions behind it are recorded.

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 10: The Windows dead-code check, and what is left for S4 and S5

**Files:** none changed unless the check finds something.

**Interfaces:** none.

- [ ] **Step 1: The Windows dead-code check**

CI compiles Windows with `-D warnings`, and macOS cannot see an item reachable only from a `#[cfg(unix)]` path. Run on a scratch copy, never in the worktree:

```sh
export CARGO_BUILD_JOBS=6
W=$(mktemp -d) && rsync -a --exclude target --exclude .git ./ "$W/" && cd "$W" &&
find src -name '*.rs' -exec sed -i '' \
  -e 's/#\[cfg(all(test, unix))\]/#[cfg(any())]/g' \
  -e 's/#\[cfg(unix)\]/#[cfg(any())]/g' \
  -e 's/#\[cfg(not(unix))\]/#[cfg(not(any()))]/g' \
  -e 's/#\[cfg(windows)\]/#[cfg(not(any()))]/g' {} + &&
CARGO_TARGET_DIR="$TMPDIR/hv-103-target" perl -e 'alarm shift; exec @ARGV' 2400 cargo clippy --all-targets -- -D warnings
```
Expected: no warning. Likeliest finding: the `use` or helper that only the unix-only tests of `src/config_edit.rs` and `src/daemon.rs` use. Fix it in the worktree by moving the item under the same `#[cfg(unix)]` as its user, rerun the four gates, and commit with the message `Keep unix-only test helpers out of the Windows build`.

- [ ] **Step 2: Leave the stages that follow**

Do not push and do not open the pull request here. The orchestrator starts S4 (review of the diff by a fresh reviewer, then the mutation tester, one after the other) and S5 (the isolated herdr session: the placement of the popup, whether the popup closes on exit, the environment of the popup pane, and the next take using the chosen input). S5 must also press Enter in the popup and see it close: the path through `pause()` has no unit test, and the deadlock that Task 8 step 3 guards against is visible only there.

If S5 shows that `herdr plugin pane open --entrypoint mic` needs `--placement`, the change is one line in `open_command` (`src/mic.rs`) and its test `the_action_opens_this_plugins_mic_pane_through_herdr`, and a note in the second `docs/decisions.md` row.

---

## Self-review

**Spec coverage** (design section 5 against tasks): AC-1 to 9 and the writer: Tasks 1, 2. AC-10, 11: Task 5 (recorder) and Task 6 (daemon). AC-12: Task 9 step 5. AC-13: Tasks 4, 6, 8. AC-14: Task 8 (`NoDaemon` message and `reached`) and Task 7. AC-15 to 20: Task 8. AC-21: Task 9 step 1. AC-22: Tasks 8, 9, and by hand in S5. AC-39: S5. AC-40: `BINDINGS` and the manifest, each one place. AC-41: Task 9 step 3. The chooser's move is Task 3.

**Placeholders:** the only copied-not-written code is `table_name` (Task 1 step 3), which names the exact lines to copy from; nothing else is left open.

**Types:** `Edit { table, key, value: String }`, `WriteError` variants, `Reached::Applied(reload::Applied)`, `Recorder::reconfigure(Audio) -> bool`, `exchange(..) -> Result<Reply, ClientError>`, `plan_reload(&Config, &Config) -> ReloadPlan`, `reload_from(Option<&Path>, &Recorder, &Runtime) -> Reply` are used with the same names and signatures in every task that consumes them. `Reached::Applied` carries `reload::Applied`, which differs from the design's `Applied { restart }`: it adds `applied` so that "nothing to apply" can be said.

**Review focus:** each of the five lines has its test: CRLF (Task 1), quoting (Task 1), link and mode (Task 2), unreadable and invalid files (Tasks 2 and 6), duplicate names (Task 8).
