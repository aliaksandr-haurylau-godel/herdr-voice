# Issue #27 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans with superpowers:test-driven-development. Steps use checkbox (`- [ ]`) syntax.

**Goal:** The `doctor` engine line for `command` and `http` says what was established and what was not, instead of "is ready".

**Architecture:** In `engine_finding_from` (`src/doctor.rs`) the wildcard arm `Ok(_)` becomes two arms, `Ok(stt::Ready::Command { .. })` and `Ok(stt::Ready::Http)`, each with its own sentence and state `Ok`.

**Tech Stack:** Rust, `src/doctor.rs` only. No new dependency.

**Spec:** `tasks/27/DESIGN_27.md` (S2 READY), `tasks/27/AC_27.md`.

## Global Constraints

- Everything in the repository is English. No absolute path, employer, client or internal name in any file.
- **Task 2 starts only after the orchestrator has approved the wording.** The two sentences below are the proposal; if the approved text differs, only the two string literals in Task 1 and Task 2 change.
- Do not touch the candle arm, any `missing` finding, or the exit code.
- Every cargo command runs as `CARGO_BUILD_JOBS=6 perl -e 'alarm shift; exec @ARGV' 570 cargo ...`, one at a time. Targeted test runs use `cargo test <module path>` with no `--lib` flag: the crate has only a binary target.
- Before each commit: `cargo test`, `cargo clippy --all-targets -- -D warnings`, `cargo fmt --check` (run `cargo fmt` first), `python3 scripts/check_manifest.py`, and the Windows dead-code check in Task 3; grep every written file for the four stray edit-tool tags (`new_string` and `old_string`, opening and closing) and line-start conflict markers.
- Commit messages end with `Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>`.

## The wording (proposal)

- `command`: `[stt] command is set; its program is not looked for until a take is transcribed`
- `http`: `[stt] url is set; the endpoint is not contacted until a take is transcribed`

## Review Focus

- The two lines must not contain `is ready`, nor the engine kind in quotes.
- A program that does not exist must still give `State::Ok`: the line claims nothing about it.
- The candle `ok` line test (`the_candle_engine_line_names_the_device`) and the `missing` assertions in `the_engine_line_names_what_resolve_reports_for_each_engine` keep passing unchanged.

---

### Task 1: Tests that pin the new text

**Files:** Modify `src/doctor.rs`, inside `mod tests`, directly after `the_engine_line_names_what_resolve_reports_for_each_engine`.

- [ ] **Step 1: Write the tests.**

```rust
    const COMMAND_LINE: &str =
        "[stt] command is set; its program is not looked for until a take is transcribed";
    const HTTP_LINE: &str =
        "[stt] url is set; the endpoint is not contacted until a take is transcribed";

    #[test]
    fn the_command_engine_line_claims_only_that_the_command_is_set() {
        // The program is not on PATH. The line is ok all the same, because it
        // claims nothing about the program.
        let models = scratch_models("engine-command-text");
        let command = command_stt(&["hv27-no-such-program", "{audio}"]);
        let finding = engine_and_model_findings(&command, &models).0;
        assert_eq!(finding.state, State::Ok, "got {finding:?}");
        assert_eq!(finding.detail, COMMAND_LINE);
    }

    #[test]
    fn the_http_engine_line_claims_only_that_the_url_is_set() {
        let models = scratch_models("engine-http-text");
        let http = config::Stt {
            engine: "http".to_string(),
            url: "http://127.0.0.1:9/transcribe".to_string(),
            ..config::Stt::default()
        };
        let finding = engine_and_model_findings(&http, &models).0;
        assert_eq!(finding.state, State::Ok, "got {finding:?}");
        assert_eq!(finding.detail, HTTP_LINE);
    }

    #[test]
    fn neither_external_engine_line_says_ready_or_quotes_the_engine_kind() {
        let models = scratch_models("engine-no-ready");
        let command = command_stt(&["prog", "{audio}"]);
        let http = config::Stt {
            engine: "http".to_string(),
            url: "http://127.0.0.1:9/transcribe".to_string(),
            ..config::Stt::default()
        };
        for (stt, kind) in [(command, "\"command\""), (http, "\"http\"")] {
            let detail = engine_and_model_findings(&stt, &models).0.detail;
            assert!(!detail.contains("is ready"), "{detail}");
            assert!(!detail.contains(kind), "{detail}");
        }
    }
```

- [ ] **Step 2: Run** `cargo test doctor::tests` — expected: the three new tests FAIL (the detail is `"command" is ready` and `"http" is ready`); every other test in the module passes.

### Task 2: The two arms

**Files:** Modify `src/doctor.rs`, `engine_finding_from`.

- [ ] **Step 1: Implement.** Replace

```rust
        Ok(_) => Finding {
            name: "engine",
            state: State::Ok,
            detail: format!("{:?} is ready", stt.engine),
        },
```

with

```rust
        // What `check_with` established is that the key is set. It does not look
        // for the program or contact the endpoint, and must not: the first
        // element of the list is often a shell, and running a configured program
        // to test it can hang. The line says so instead of claiming readiness.
        Ok(stt::Ready::Command { .. }) => Finding {
            name: "engine",
            state: State::Ok,
            detail: "[stt] command is set; its program is not looked for until a take is transcribed"
                .to_string(),
        },
        Ok(stt::Ready::Http) => Finding {
            name: "engine",
            state: State::Ok,
            detail: "[stt] url is set; the endpoint is not contacted until a take is transcribed"
                .to_string(),
        },
```

- [ ] **Step 2: Run** `cargo test` — expected: every test in every target passes, including the three new ones and `the_candle_engine_line_names_the_device`.
- [ ] **Step 3:** `grep -rn 'is ready' src` — expected: only the candle format string in `src/doctor.rs` and the test assertions.

### Task 3: Gates and commit

- [ ] **Step 1:** Run `cargo fmt`, then the four gates, then the Windows dead-code check on a scratch copy of the crate, never in the worktree, with one shared target directory:

```sh
W=$(mktemp -d) && rsync -a --exclude target --exclude .git ./ "$W/" && cd "$W" &&
find src -name '*.rs' -exec sed -i '' \
  -e 's/#\[cfg(all(test, unix))\]/#[cfg(any())]/g' \
  -e 's/#\[cfg(unix)\]/#[cfg(any())]/g' \
  -e 's/#\[cfg(not(unix))\]/#[cfg(not(any()))]/g' \
  -e 's/#\[cfg(windows)\]/#[cfg(not(any()))]/g' {} + &&
CARGO_BUILD_JOBS=6 CARGO_TARGET_DIR="$TMPDIR/hv-27-target" perl -e 'alarm shift; exec @ARGV' 570 cargo clippy --all-targets -- -D warnings
```

Expected: clippy finishes with no warning; then delete the scratch copy.
- [ ] **Step 2:** Grep the written files for stray tags and conflict markers.
- [ ] **Step 3:** `git add src/doctor.rs tasks/27` and commit `fix: doctor's engine line says what it established (#27)`.
