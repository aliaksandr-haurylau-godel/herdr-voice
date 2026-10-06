# Issue #86 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans with superpowers:test-driven-development. Steps use checkbox (`- [ ]`) syntax.

**Goal:** `setup` says that a question could not be answered, and exits 1, instead of reporting a decline.

**Architecture:** `read_answer` turns a reader into `Option<String>`, `None` for zero bytes or a failed read. `run` matches on `answer()`: `None` prints a new message, the legacy report and returns 1; `Some` goes through the existing code. `main` calls `read_answer` on standard input.

**Tech Stack:** Rust, `src/setup.rs` only, plus one sentence in `docs/design.md`. No new dependency.

**Spec:** `tasks/86/DESIGN_86.md` (S2 READY), `tasks/86/AC_86.md`.

## Global Constraints

- Everything in the repository is English. No absolute path, employer, client or internal name in any file.
- Before each commit: `cargo test`, `cargo clippy --all-targets -- -D warnings`, `cargo fmt --check`, `python3 scripts/check_manifest.py`; grep every written file for the four stray edit-tool tags (`new_string`, `old_string`, opening and closing) and line-start conflict markers.
- Commit messages end with `Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>`.
- Do not change the question text, the `is_terminal` check or the non-interactive branch.

## Review Focus

- An answer of `Some("")` (an empty line is `"\n"`, but a closure may return `""`) is still a decline, not "no answer".
- `None` must not write the file and must not call herdr.
- `report_legacy` is printed on the `None` path.

---

### Task 1: `read_answer`

**Files:** Modify `src/setup.rs` (function above `pub fn main`, tests inside the existing `mod tests`).

**Interfaces:** Produces `fn read_answer(reader: &mut dyn std::io::BufRead) -> Option<String>`.

- [ ] **Step 1: Write the failing tests** in `mod tests`, after `a_declined_offer_changes_nothing_and_is_not_a_failure`:

```rust
    #[test]
    fn an_empty_reader_is_no_answer_and_an_empty_line_is_one() {
        assert_eq!(read_answer(&mut std::io::Cursor::new("")), None);
        assert_eq!(
            read_answer(&mut std::io::Cursor::new("\n")),
            Some("\n".to_string())
        );
    }

    #[test]
    fn a_last_line_without_a_newline_is_still_an_answer() {
        assert_eq!(
            read_answer(&mut std::io::Cursor::new("y")),
            Some("y".to_string())
        );
    }

    #[test]
    fn a_failed_read_is_no_answer() {
        // Bytes that are not UTF-8 make `read_line` fail with `InvalidData`.
        assert_eq!(read_answer(&mut std::io::Cursor::new(vec![0xff, 0xfe, b'\n'])), None);
    }
```

- [ ] **Step 2: Run** `cargo test setup::tests::an_empty_reader` — expected: compile error, `read_answer` not found in this scope.

- [ ] **Step 3: Implement**, directly above `pub fn main`:

```rust
/// One line from `reader`, or `None` when there is no answer to give: the read
/// failed, or it returned zero bytes, which is end of file. An empty line is
/// `"\n"`, not zero bytes, and is an answer.
fn read_answer(reader: &mut dyn std::io::BufRead) -> Option<String> {
    let mut line = String::new();
    match reader.read_line(&mut line) {
        Ok(0) | Err(_) => None,
        Ok(_) => Some(line),
    }
}
```

- [ ] **Step 4: Run** `cargo test setup::tests` — expected: the three new tests PASS with the rest of the module.

### Task 2: `run` and `main`

**Files:** Modify `src/setup.rs`: the lines `let said = answer().unwrap_or_default();` through the decline `return 0;` in `run`, and the closure in `main`.

**Interfaces:** Consumes `read_answer`.

- [ ] **Step 1: Write the failing tests** in `mod tests`, after the tests from Task 1:

```rust
    const UNANSWERED: &str = "the question could not be answered";

    #[test]
    fn no_answer_is_not_a_decline_and_changes_nothing() {
        let path = scratch("run-eof");
        std::fs::write(&path, "[theme]\n").unwrap();
        let herdr = FakeHerdr::clean();
        let (code, said) = capture(&herdr, Some(path.clone()), true, vec![]);
        assert_eq!(code, 1, "{said}");
        assert!(said.contains(UNANSWERED), "{said}");
        assert!(said.contains("herdr-voice setup"), "it says what to run: {said}");
        assert!(
            said.contains("terminal that passes your keystrokes on"),
            "{said}"
        );
        assert!(
            said.lines().all(|line| line != "nothing was changed."),
            "a decline prints that sentence as a line of its own; this must not: {said}"
        );
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "[theme]\n");
        assert!(herdr.calls().is_empty(), "{:?}", herdr.calls());
    }

    #[test]
    fn a_decline_does_not_say_the_question_went_unanswered() {
        let path = scratch("run-no-text");
        std::fs::write(&path, "[theme]\n").unwrap();
        for said_no in ["n\n", "\n", ""] {
            let (code, said) = capture(&FakeHerdr::clean(), Some(path.clone()), true, vec![said_no]);
            assert_eq!(code, 0, "{said_no:?}: {said}");
            assert!(said.contains("nothing was changed."), "{said_no:?}: {said}");
            assert!(!said.contains(UNANSWERED), "{said_no:?}: {said}");
        }
    }

    #[test]
    fn no_answer_still_prints_the_legacy_report() {
        let path = scratch("run-eof-legacy");
        std::fs::write(&path, "[theme]\n").unwrap();
        let legacy = Legacy {
            config_file: Some(PathBuf::from("/tmp/c/haurylau.voice/config.toml")),
            current_config_dir: Some(PathBuf::from("/tmp/c/herdr-voice")),
            daemon_socket: None,
        };
        let mut said = Vec::new();
        let code = run(
            &FakeHerdr::clean(),
            Some(path),
            &legacy,
            true,
            &mut || None,
            &mut said,
        );
        let said = String::from_utf8(said).unwrap();
        assert_eq!(code, 1);
        assert!(said.contains("/tmp/c/haurylau.voice/config.toml"), "{said}");
    }
```

- [ ] **Step 2: Run** `cargo test setup::tests::no_answer` — expected: `no_answer_is_not_a_decline_and_changes_nothing` FAILS with `left: 0, right: 1` (the unanswered run exits 0 today).

- [ ] **Step 3: Implement.** In `run`, replace

```rust
    let said = answer().unwrap_or_default();
    if !matches!(said.trim().to_ascii_lowercase().as_str(), "y" | "yes") {
```

with

```rust
    let Some(said) = answer() else {
        // Nobody could answer: a terminal that shows output and forwards no
        // keystrokes reads as a decline otherwise, and the person is left
        // believing their `y` was refused.
        let _ = writeln!(
            out,
            "\nthe question could not be answered: no input reached this process, \
             so nothing was changed. Run `herdr-voice setup` in a terminal that \
             passes your keystrokes on."
        );
        report_legacy(legacy, out);
        return 1;
    };
    if !matches!(said.trim().to_ascii_lowercase().as_str(), "y" | "yes") {
```

In `main`, replace the `use std::io::{BufRead, IsTerminal};` line with `use std::io::IsTerminal;` and the closure body with

```rust
    let mut answer = || read_answer(&mut std::io::stdin().lock());
```

- [ ] **Step 4: Run** `cargo test` — expected: every test in every target PASS, including `a_declined_offer_changes_nothing_and_is_not_a_failure`. The existing `capture` helper returns `None` when its answers run out; every existing caller supplies one answer, so none changes.

### Task 3: The design document

**Files:** Modify `docs/design.md`, section 7a, first bullet (the one that begins "**Keybindings that name an id herdr no longer knows.**").

- [ ] **Step 1:** After the sentence ending "and does both in one write." add: `A question that nobody could answer — standard input ended or could not be read — is reported as such and exits 1; a declined offer exits 0.`
- [ ] **Step 2:** Run `git diff docs/design.md` — expected: one added sentence.

### Task 4: Gates and commit

- [ ] **Step 1:** Run the four gates, then the Windows dead-code check on a scratch copy of the crate, never in the worktree:

```sh
W=$(mktemp -d) && rsync -a --exclude target --exclude .git ./ "$W/" && cd "$W" &&
find src -name '*.rs' -exec sed -i '' \
  -e 's/#\[cfg(all(test, unix))\]/#[cfg(any())]/g' \
  -e 's/#\[cfg(unix)\]/#[cfg(any())]/g' \
  -e 's/#\[cfg(not(unix))\]/#[cfg(not(any()))]/g' \
  -e 's/#\[cfg(windows)\]/#[cfg(not(any()))]/g' {} + &&
CARGO_TARGET_DIR="$W/target" cargo clippy --all-targets -- -D warnings
```

Expected: clippy exits 0 with no warning. It compiles the crate as if it were Windows, so an item reachable only from a Unix path is reported as dead code.
- [ ] **Step 2:** Grep the written files for stray tags and conflict markers.
- [ ] **Step 3:** `git add src/setup.rs docs/design.md tasks/86` and commit `fix: setup says when its question could not be answered (#86)`.
