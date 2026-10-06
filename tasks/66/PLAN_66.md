# PLAN_66 — spawn errors, script fixtures and download fixture

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans
> with superpowers:test-driven-development. Steps use checkbox (`- [ ]`) syntax.

**Goal:** Report why starting herdr failed in setup, the indicator and doctor
(#101); make every test that writes and runs a script immune to `ETXTBSY` (#66);
keep the download double's listener bound and record the paths it is asked for
(#62, #48).

**Architecture:** `src/delivery.rs` classifies a failed spawn once
(`StartFailure`) and holds the three sentences once; `DeliveryError`,
`PaintError` and `HerdrError` convert from it. A new test-only module
`src/script_fixture.rs` writes a script and returns only after a probe execution
succeeds. `serve` in `src/stt/fetch.rs` returns a `Server` that stops on request,
and records request paths.

**Tech Stack:** Rust 1.82 (`rust-version` in `Cargo.toml`; do not use APIs newer
than that), `std` only, no new dependencies.

**Spec:** `tasks/66/DESIGN_66.md` (decisions D1 to D6, and "Why the window is
closed by construction"); acceptance criteria `tasks/66/AC_66.md`.

## Global Constraints

- Every `cargo` command is prefixed `CARGO_BUILD_JOBS=6` and runs alone: never
  start a second cargo process, a subagent's build included, while one runs.
- Test-binary name: `herdr-voice`. Run one module with
  `CARGO_BUILD_JOBS=6 cargo test --bin herdr-voice <filter>`.
- Everything in the repository is English. No absolute path, no account name, no
  employer or client name anywhere (code, comments, commit messages). Cite paths
  relative to the repository root.
- Do not change `git config`. `core.hooksPath` stays `.githooks`.
- CI compiles Windows with `-D warnings`. Anything only reachable on unix is
  gated `cfg(unix)` (or `cfg(all(test, unix))`); do not leave an item that only a
  `cfg(unix)` path uses ungated.
- No production code change other than Tasks 1 to 4. Tests' assertions in
  `indicator`, `delivery`, `setup`, `bias` are not weakened or removed.
- Commit messages end with the line
  `Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>`.
- Before every commit run this and expect no output:
  `git diff --cached | grep -nE '^\+.*(</?new_string>|</?old_string>)|^\+(<<<<<<<|=======|>>>>>>>)'`

## Review Focus

Inputs and conditions the tasks' main tests do not reach; each has its own test.

1. A script given to `write_executable` that does not start with `#!` — refused
   with a message that says so (Task 5, `a_script_without_an_interpreter_line_is_refused`).
2. A spawn failure that is not `ETXTBSY` while waiting — panics at once, naming
   the script (Task 5, `a_script_that_cannot_be_executed_for_another_reason_panics_at_once`).
3. `ETXTBSY` that never clears — panics after the deadline, naming the script
   (Task 5, Linux only).
4. A request for a path the download table does not hold — answered 404 and still
   recorded (Task 9, `a_request_the_table_does_not_hold_is_answered_404_and_recorded`).
5. A herdr that exists but is not executable — reported as such by `setup`, the
   indicator and doctor, not as missing (Tasks 2, 3, 4).

---

## Task 0: Commit the run artifacts

**Files:** Add `tasks/66/RUN_66.md`, `AC_66.md`, `DESIGN_66.md`,
`DESIGN_66_evidence.md`, `PLAN_66.md`.

- [ ] **Step 1:** `git add tasks/66 && git status --short` — expect only
  `A  tasks/66/...` lines.
- [ ] **Step 2:** Run the stray-marker check from Global Constraints.
- [ ] **Step 3:** Commit:

```bash
git commit -m "Add the run artifacts for #66, #101, #62 and #48 (S1 to S3)

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

## Task 1: `StartFailure` and the shared sentences (D1, `src/delivery.rs`)

**Files:**
- Modify: `src/delivery.rs` (the `DeliveryError` Display at `:21-43`, the
  `start_failure` function at `:179-197`, the tests at `:578-616`)

**Interfaces:**
- Produces (used by Tasks 2, 3, 4):
  - `pub enum StartFailure { NotFound { binary: String, path: String }, NotExecutable { binary: String }, Other { binary: String, reason: String } }`, deriving `Debug, Clone, PartialEq, Eq`
  - `pub(crate) fn start_failure(binary: &str, error: &std::io::Error) -> StartFailure`
  - `pub(crate) fn not_found_message(binary: &str, path: &str) -> String`
  - `pub(crate) fn not_executable_message(binary: &str) -> String`
  - `pub(crate) fn start_failed_message(binary: &str, reason: &str) -> String`
  - `impl From<StartFailure> for DeliveryError`

- [ ] **Step 1: Make the existing two `start_failure` tests read a `StartFailure` (they fail to compile).**
  In `src/delivery.rs`, replace the test
  `start_failure_reads_the_kind_the_operating_system_gave` with:

```rust
    #[test]
    fn start_failure_reads_the_kind_the_operating_system_gave() {
        use std::io::{Error, ErrorKind};
        assert!(matches!(
            start_failure("herdr", &Error::from(ErrorKind::NotFound)),
            StartFailure::NotFound { .. }
        ));
        assert!(matches!(
            start_failure("herdr", &Error::from(ErrorKind::PermissionDenied)),
            StartFailure::NotExecutable { .. }
        ));
        assert!(matches!(
            start_failure("herdr", &Error::other("Text file busy")),
            StartFailure::Other { .. }
        ));
    }
```

  and in
  `another_failure_to_start_carries_the_operating_systems_text_and_not_the_path_sentence`
  replace the first statement

```rust
        let error = start_failure(
            "/opt/herdr",
            &std::io::Error::other("Text file busy (os error 26)"),
        );
```

  with

```rust
        let error = DeliveryError::from(start_failure(
            "/opt/herdr",
            &std::io::Error::other("Text file busy (os error 26)"),
        ));
```

  Leave the rest of that test unchanged.

- [ ] **Step 2: Run, expect a compile failure.**
  `CARGO_BUILD_JOBS=6 cargo test --bin herdr-voice delivery` — expected: error
  `cannot find type StartFailure`.

- [ ] **Step 3: Implement.** Replace the `start_failure` function and its doc
  comment (`src/delivery.rs:178-197`, from `/// What starting the program failed
  with` to the closing brace of the function) with:

```rust
/// Why starting a program failed, read from the kind the operating system gave.
/// Only `NotFound` means the program is not on the `PATH`; `PermissionDenied`
/// means it was found and cannot be run (no execute bit, or a directory);
/// anything else — a file still open for writing, exhausted processes — carries
/// the system's own text. Every place that starts herdr classifies through
/// `start_failure` and converts the result into its own error type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StartFailure {
    NotFound { binary: String, path: String },
    NotExecutable { binary: String },
    Other { binary: String, reason: String },
}

pub(crate) fn start_failure(binary: &str, error: &std::io::Error) -> StartFailure {
    match error.kind() {
        std::io::ErrorKind::NotFound => StartFailure::NotFound {
            binary: binary.to_string(),
            path: std::env::var("PATH").unwrap_or_default(),
        },
        std::io::ErrorKind::PermissionDenied => StartFailure::NotExecutable {
            binary: binary.to_string(),
        },
        _ => StartFailure::Other {
            binary: binary.to_string(),
            reason: error.to_string(),
        },
    }
}

impl From<StartFailure> for DeliveryError {
    fn from(failure: StartFailure) -> Self {
        match failure {
            StartFailure::NotFound { binary, path } => DeliveryError::NotFound { binary, path },
            StartFailure::NotExecutable { binary } => DeliveryError::NotExecutable { binary },
            StartFailure::Other { binary, reason } => {
                DeliveryError::StartFailed { binary, reason }
            }
        }
    }
}

/// The sentences for a failed start, written once. `DeliveryError`,
/// `PaintError` and `HerdrError` print these for their not-found,
/// not-executable and other-failure variants.
pub(crate) fn not_found_message(binary: &str, path: &str) -> String {
    // Mirrors CommandError::NotFound (src/stt/command.rs:81-86).
    format!(
        "cannot run {binary:?}: it is not on the PATH this process has, which is \
         {path:?}. Set HERDR_BIN_PATH to herdr's location, or start herdr from a shell \
         where it is on the PATH"
    )
}

pub(crate) fn not_executable_message(binary: &str) -> String {
    format!(
        "cannot run {binary:?}: the file was found but this process is not allowed to \
         run it. Make it executable (on Unix, chmod +x), or point HERDR_BIN_PATH at the \
         herdr program itself"
    )
}

pub(crate) fn start_failed_message(binary: &str, reason: &str) -> String {
    format!(
        "cannot run {binary:?}: the operating system reported {reason:?}. This is often \
         temporary: try again, and if it keeps happening, report that text"
    )
}
```

  Then in the `Display for DeliveryError` impl replace the three arms
  `DeliveryError::NotFound { binary, path } => write!(...)`,
  `DeliveryError::NotExecutable { binary } => write!(...)` and
  `DeliveryError::StartFailed { binary, reason } => write!(...)` (everything
  from the `// Mirrors CommandError::NotFound` comment to the end of the
  `StartFailed` arm) with:

```rust
            DeliveryError::NotFound { binary, path } => {
                write!(f, "{}", not_found_message(binary, path))
            }
            DeliveryError::NotExecutable { binary } => {
                write!(f, "{}", not_executable_message(binary))
            }
            DeliveryError::StartFailed { binary, reason } => {
                write!(f, "{}", start_failed_message(binary, reason))
            }
```

  And in `HerdrDeliverer::run` replace
  `Err(error) => Err(start_failure(&self.binary, &error)),` with
  `Err(error) => Err(start_failure(&self.binary, &error).into()),`.

- [ ] **Step 4: Run, expect pass.**
  `CARGO_BUILD_JOBS=6 cargo test --bin herdr-voice delivery` — all `delivery::`
  tests pass, including the three that pin the sentences
  (`a_program_that_does_not_exist_keeps_the_path_sentence` and the two
  `...says_so_and_not_the_path_sentence` tests).

- [ ] **Step 5: Format and commit.**
  `cargo fmt` then the stray-marker check, then

```bash
git add src/delivery.rs && git commit -m "Classify a failed start once and print its sentences from one place

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

## Task 2: `setup` reports why herdr did not start (#101, AC-1, AC-4)

**Depends on:** Task 1.

**Files:**
- Modify: `src/setup.rs` (`HerdrError` at `:398-419`, `HerdrCli::not_found` at
  `:492-497`, the arms at `:512`, `:532`, `:549`, tests in `mod herdr_cli` at
  `:2128` onward)

**Interfaces:**
- Consumes: `crate::delivery::{start_failure, StartFailure, not_found_message, not_executable_message, start_failed_message}`
- Produces: `HerdrError::NotExecutable { binary: String }`,
  `HerdrError::StartFailed { binary: String, reason: String }`,
  `impl From<StartFailure> for HerdrError`

- [ ] **Step 1: Write the failing tests.** In `mod herdr_cli` of `src/setup.rs`,
  directly after the test `a_binary_that_cannot_be_started_is_reported_as_not_found`
  (it ends with `assert!(cli.notify("t", "b").is_err());` and a closing brace),
  insert:

```rust
        /// A file that exists and may not be run: written without the execute bit.
        struct PlainFile {
            dir: std::path::PathBuf,
            path: std::path::PathBuf,
        }

        impl Drop for PlainFile {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.dir);
            }
        }

        fn a_file_that_is_not_executable(tag: &str) -> PlainFile {
            let dir = std::env::temp_dir().join(format!(
                "herdr-voice-setup-plain-{tag}-{}",
                std::process::id()
            ));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).expect("scratch dir");
            let path = dir.join("herdr");
            std::fs::write(&path, "#!/bin/sh\nexit 0\n").expect("write the file");
            PlainFile { dir, path }
        }

        #[test]
        fn a_program_without_the_execute_bit_is_reported_as_not_runnable_and_not_as_missing() {
            let file = a_file_that_is_not_executable("noexec");
            let binary = file.path.to_string_lossy().into_owned();
            let cli = HerdrCli::with_binary(binary.clone());
            for outcome in [
                cli.open_pane(),
                cli.check_config(std::path::Path::new("config.toml"))
                    .map(|_| ()),
                cli.notify("t", "b"),
            ] {
                assert_eq!(
                    outcome,
                    Err(HerdrError::NotExecutable {
                        binary: binary.clone()
                    })
                );
            }
            let message = HerdrError::NotExecutable { binary }.to_string();
            assert!(message.contains("was found"), "got {message}");
            assert!(!message.contains("not on the PATH"), "got {message}");
        }

        #[test]
        fn any_other_failure_to_start_carries_the_operating_systems_text_and_not_the_path_sentence()
        {
            let error = HerdrError::from(crate::delivery::start_failure(
                "/opt/herdr",
                &std::io::Error::other("Text file busy (os error 26)"),
            ));
            assert_eq!(
                error,
                HerdrError::StartFailed {
                    binary: "/opt/herdr".to_string(),
                    reason: "Text file busy (os error 26)".to_string(),
                }
            );
            let message = error.to_string();
            assert!(message.contains("Text file busy (os error 26)"), "got {message}");
            assert!(!message.contains("PATH"), "got {message}");
        }
```

- [ ] **Step 2: Run, expect a compile failure.**
  `CARGO_BUILD_JOBS=6 cargo test --bin herdr-voice setup::` — expected: no
  variant `NotExecutable` / `StartFailed` on `HerdrError`.

- [ ] **Step 3: Implement.** In `src/setup.rs`:

  a. Replace the `HerdrError` enum and its `Display` impl (`:398-419`) with:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HerdrError {
    /// herdr itself was not found.
    NotFound { binary: String, path: String },
    /// herdr was found and this process is not allowed to run it.
    NotExecutable { binary: String },
    /// Starting herdr failed for another reason, which is the operating
    /// system's own text.
    StartFailed { binary: String, reason: String },
    /// herdr ran and refused. The string is what it said.
    Rejected(String),
}

impl std::fmt::Display for HerdrError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use crate::delivery::{not_executable_message, not_found_message, start_failed_message};
        match self {
            HerdrError::Rejected(why) => write!(f, "{why}"),
            HerdrError::NotFound { binary, path } => {
                write!(f, "{}", not_found_message(binary, path))
            }
            HerdrError::NotExecutable { binary } => {
                write!(f, "{}", not_executable_message(binary))
            }
            HerdrError::StartFailed { binary, reason } => {
                write!(f, "{}", start_failed_message(binary, reason))
            }
        }
    }
}

impl From<crate::delivery::StartFailure> for HerdrError {
    fn from(failure: crate::delivery::StartFailure) -> Self {
        use crate::delivery::StartFailure;
        match failure {
            StartFailure::NotFound { binary, path } => HerdrError::NotFound { binary, path },
            StartFailure::NotExecutable { binary } => HerdrError::NotExecutable { binary },
            StartFailure::Other { binary, reason } => HerdrError::StartFailed { binary, reason },
        }
    }
}
```

  (Keep the doc comment line above `pub enum HerdrError` if there is one other
  than the `#[derive]`; the old text there was `/// herdr could not be started at
  all.` on the `NotFound` variant only.)

  b. Replace the method `not_found` (`:492-497`) with:

```rust
    fn start_failed(&self, error: &std::io::Error) -> HerdrError {
        crate::delivery::start_failure(&self.binary, error).into()
    }
```

  c. Replace each of the three arms `Err(_) => Err(self.not_found()),` (in
  `open_pane`, `check_config`, `notify`) with
  `Err(error) => Err(self.start_failed(&error)),`.

- [ ] **Step 4: Run, expect pass.**
  `CARGO_BUILD_JOBS=6 cargo test --bin herdr-voice setup::` — all pass, including
  the existing `a_binary_that_cannot_be_started_is_reported_as_not_found`.

- [ ] **Step 5: Check the arms are gone.**
  `grep -n 'Err(_) =>' src/setup.rs` — expect no line that starts a herdr
  process. (Other `Err(_)` lines in the file, if any, are not process starts.)

- [ ] **Step 6: Format and commit.**

```bash
cargo fmt && git add src/setup.rs && git commit -m "Say why setup could not start herdr instead of calling it missing

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

## Task 3: the indicator reports why herdr did not start (#101, AC-2, AC-4)

**Depends on:** Task 1.

**Files:**
- Modify: `src/indicator.rs` (`PaintError` at `:86-107`, `HerdrPainter::output` at
  `:223-243`, tests near `:1222`)

**Interfaces:**
- Consumes: the Task 1 items.
- Produces: `PaintError::NotExecutable { binary: String }`,
  `PaintError::StartFailed { binary: String, reason: String }`,
  `impl From<StartFailure> for PaintError`

- [ ] **Step 1: Write the failing tests.** In `mod tests` of `src/indicator.rs`,
  directly after the test
  `a_herdr_that_cannot_be_started_is_not_found_and_names_the_path` (it ends with
  the `for` loop's closing brace and the test's closing brace), insert:

```rust
    #[cfg(unix)]
    #[test]
    fn a_herdr_without_the_execute_bit_is_not_runnable_and_not_missing() {
        let fake = FakeHerdr::new("noexec", "herdr.sh");
        std::fs::write(&fake.script, "#!/bin/sh\nexit 0\n").expect("write the file");
        let painter = fake.painter();
        for outcome in [
            painter.tabs().map(|_| ()),
            painter.rename("w1:t1", "1"),
            painter.token("w1:p1", "🎙️🔴 REC 0:00", 1_800),
        ] {
            assert_eq!(
                outcome,
                Err(PaintError::NotExecutable {
                    binary: fake.binary()
                })
            );
        }
        let why = PaintError::NotExecutable {
            binary: fake.binary(),
        }
        .to_string();
        assert!(why.contains("was found"), "got {why}");
        assert!(!why.contains("not on the PATH"), "got {why}");
        let line = paint_failed_line(&why);
        assert!(line.contains("was found"), "got {line}");
    }

    #[test]
    fn any_other_failure_to_start_is_the_operating_systems_text_and_not_the_path_sentence() {
        let error = PaintError::from(crate::delivery::start_failure(
            "/opt/herdr",
            &std::io::Error::other("Text file busy (os error 26)"),
        ));
        assert_eq!(
            error,
            PaintError::StartFailed {
                binary: "/opt/herdr".to_string(),
                reason: "Text file busy (os error 26)".to_string(),
            }
        );
        let why = error.to_string();
        assert!(why.contains("Text file busy (os error 26)"), "got {why}");
        assert!(!why.contains("PATH"), "got {why}");
    }
```

- [ ] **Step 2: Run, expect a compile failure.**
  `CARGO_BUILD_JOBS=6 cargo test --bin herdr-voice indicator::` — expected: no
  variant `NotExecutable` on `PaintError`.

- [ ] **Step 3: Implement.** In `src/indicator.rs`:

  a. Replace the `PaintError` enum and its `Display` impl (`:86-106`, up to but
  not including `impl std::error::Error for PaintError {}`) with:

```rust
pub enum PaintError {
    /// The code alone, extracted from herdr's structured refusal, or the raw
    /// output when it did not parse as that shape.
    Rejected(String),
    /// `herdr` itself was not found.
    NotFound { binary: String, path: String },
    /// `herdr` was found and this process is not allowed to run it.
    NotExecutable { binary: String },
    /// Starting `herdr` failed for another reason, which is the operating
    /// system's own text.
    StartFailed { binary: String, reason: String },
}

impl std::fmt::Display for PaintError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use crate::delivery::{not_executable_message, not_found_message, start_failed_message};
        match self {
            PaintError::Rejected(why) => write!(f, "{why}"),
            PaintError::NotFound { binary, path } => {
                write!(f, "{}", not_found_message(binary, path))
            }
            PaintError::NotExecutable { binary } => {
                write!(f, "{}", not_executable_message(binary))
            }
            PaintError::StartFailed { binary, reason } => {
                write!(f, "{}", start_failed_message(binary, reason))
            }
        }
    }
}

impl From<crate::delivery::StartFailure> for PaintError {
    fn from(failure: crate::delivery::StartFailure) -> Self {
        use crate::delivery::StartFailure;
        match failure {
            StartFailure::NotFound { binary, path } => PaintError::NotFound { binary, path },
            StartFailure::NotExecutable { binary } => PaintError::NotExecutable { binary },
            StartFailure::Other { binary, reason } => PaintError::StartFailed { binary, reason },
        }
    }
}
```

  (Before the replacement, read `src/indicator.rs:83-93` and keep the existing
  doc comment above `pub enum PaintError` and the `#[derive(...)]` line exactly
  as they are; only the enum body and the Display impl change.)

  b. In `HerdrPainter::output` replace

```rust
            Err(_) => Err(PaintError::NotFound {
                binary: self.binary.clone(),
                path: std::env::var("PATH").unwrap_or_default(),
            }),
```

  with

```rust
            Err(error) => Err(crate::delivery::start_failure(&self.binary, &error).into()),
```

- [ ] **Step 4: Run, expect pass.**
  `CARGO_BUILD_JOBS=6 cargo test --bin herdr-voice indicator::` — all pass,
  including the existing `a_herdr_that_cannot_be_started_is_not_found_and_names_the_path`.

- [ ] **Step 5:** `grep -n 'Err(_) =>' src/indicator.rs` — expect no line that
  starts a herdr process.

- [ ] **Step 6: Format and commit.**

```bash
cargo fmt && git add src/indicator.rs && git commit -m "Say why the indicator could not start herdr instead of calling it missing

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

## Task 4: doctor says why herdr did not start (#101, AC-3, AC-4)

**Depends on:** Task 1.

**Files:**
- Modify: `src/doctor.rs` (`herdr_finding` at `:109-140`, tests after
  `scratch_dir` at `:1689`)

**Interfaces:**
- Consumes: `crate::delivery::{start_failure, StartFailure}`
- Produces: `fn herdr_finding_at(binary: &str) -> Finding` (private),
  `fn cannot_run_herdr(binary: &str, error: &std::io::Error) -> String` (private)

- [ ] **Step 1: Write the failing tests.** In `mod tests` of `src/doctor.rs`,
  directly after the function `scratch_dir` (it ends with `dir` and a closing
  brace), insert:

```rust
    #[test]
    fn a_herdr_that_is_not_there_says_to_install_it_or_set_the_path() {
        let finding = herdr_finding_at("herdr-voice-no-such-program");
        assert_eq!(finding.state, State::Missing, "got {finding:?}");
        assert!(
            finding.detail.contains("install herdr") && finding.detail.contains("HERDR_BIN_PATH"),
            "got {}",
            finding.detail
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_herdr_without_the_execute_bit_says_it_was_found_and_cannot_be_run() {
        let dir = scratch_dir("herdr-noexec");
        let path = dir.join("herdr");
        std::fs::write(&path, "#!/bin/sh\nexit 0\n").unwrap();
        let finding = herdr_finding_at(&path.to_string_lossy());
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(finding.state, State::Missing, "got {finding:?}");
        assert!(finding.detail.contains("was found"), "got {}", finding.detail);
        assert!(
            finding.detail.contains("HERDR_BIN_PATH"),
            "got {}",
            finding.detail
        );
        assert!(
            !finding.detail.contains("install herdr"),
            "got {}",
            finding.detail
        );
    }

    #[test]
    fn any_other_failure_to_start_herdr_carries_the_operating_systems_text() {
        let detail = cannot_run_herdr(
            "/opt/herdr",
            &std::io::Error::other("Text file busy (os error 26)"),
        );
        assert!(detail.contains("Text file busy (os error 26)"), "got {detail}");
        assert!(detail.contains("try again"), "got {detail}");
        assert!(!detail.contains("install herdr"), "got {detail}");
    }
```

- [ ] **Step 2: Run, expect a compile failure.**
  `CARGO_BUILD_JOBS=6 cargo test --bin herdr-voice doctor::` — expected:
  `cannot find function herdr_finding_at`.

- [ ] **Step 3: Implement.** In `src/doctor.rs` replace the first four lines of
  `herdr_finding` and its `Err(_)` arm, i.e.

```rust
fn herdr_finding() -> Finding {
    let binary = std::env::var("HERDR_BIN_PATH").unwrap_or_else(|_| "herdr".to_string());
    match Command::new(&binary).arg("--version").output() {
        Err(_) => Finding {
            name: "herdr",
            state: State::Missing,
            detail: format!("cannot run {binary}; install herdr, or set HERDR_BIN_PATH to it"),
        },
```

  with

```rust
fn herdr_finding() -> Finding {
    herdr_finding_at(&std::env::var("HERDR_BIN_PATH").unwrap_or_else(|_| "herdr".to_string()))
}

/// What to tell a person whose herdr could not be started, read from the kind
/// the operating system gave: absent, present and not runnable, or something
/// else with the system's own text.
fn cannot_run_herdr(binary: &str, error: &std::io::Error) -> String {
    use crate::delivery::StartFailure;
    match crate::delivery::start_failure(binary, error) {
        StartFailure::NotFound { .. } => {
            format!("cannot run {binary}; install herdr, or set HERDR_BIN_PATH to it")
        }
        StartFailure::NotExecutable { .. } => format!(
            "{binary} was found but this process is not allowed to run it; make it executable \
             (on Unix, chmod +x), or point HERDR_BIN_PATH at the herdr program itself"
        ),
        StartFailure::Other { reason, .. } => format!(
            "cannot run {binary}: the operating system reported {reason:?}; try again, and if \
             it keeps happening, report that text"
        ),
    }
}

/// Takes the program as a parameter so a test can reach it without setting an
/// environment variable the parallel suite shares.
fn herdr_finding_at(binary: &str) -> Finding {
    match Command::new(binary).arg("--version").output() {
        Err(error) => Finding {
            name: "herdr",
            state: State::Missing,
            detail: cannot_run_herdr(binary, &error),
        },
```

  The rest of the function (the `Ok(output) => { ... }` arm) stays exactly as it
  is: `binary` is now a `&str` and is only used inside `format!`.

- [ ] **Step 4: Run, expect pass.**
  `CARGO_BUILD_JOBS=6 cargo test --bin herdr-voice doctor::` — all pass.

- [ ] **Step 5:** `grep -n 'Err(_) =>' src/doctor.rs` — the only remaining hit is
  the test `panic!` in the test module (`the file exists and is readable`), which
  does not start herdr.

- [ ] **Step 6: Format and commit.**

```bash
cargo fmt && git add src/doctor.rs && git commit -m "Say why doctor could not start herdr instead of calling it missing

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

## Task 5: the shared script fixture (D3, AC-6 to AC-8)

**Files:**
- Create: `src/script_fixture.rs`
- Modify: `src/main.rs` (module list)

**Interfaces:**
- Produces (used by Task 6):
  - `pub fn write_executable(path: &std::path::Path, content: &str)`
  - `pub fn wait_until_executable(path: &std::path::Path, deadline: std::time::Duration) -> u32`

- [ ] **Step 1: Declare the module.** In `src/main.rs`, between the lines
  `mod rewrite;` and `mod setup;` insert:

```rust
/// Test-only, unix-only: writes a script a test then runs, and returns only
/// once it can be executed.
#[cfg(all(test, unix))]
mod script_fixture;
```

- [ ] **Step 2: Create `src/script_fixture.rs` with the tests first.** Write the
  file with this content (the implementation functions are written in Step 4;
  for now include only the `tests` module and the two `use` lines it needs, so
  the build fails on the missing functions):

```rust
//! Writing a script a test then runs.
//!
//! On Linux an `exec` fails with `ETXTBSY` ("Text file busy") while any file
//! description open for writing on the file exists. The unit tests run as
//! threads of one process; a thread that forks while another has the script
//! open for writing gives its child a copy of that description until the
//! child's own `exec`, so the first thread's spawn can land in that window.
//! macOS does not refuse (see docs/evidence.md, "Text file busy in test
//! fixtures"), which is why only Linux CI shows it.
//!
//! `write_executable` closes the window for the file it writes: after the write
//! it executes the script once with a probe variable set and retries while the
//! spawn fails with `ETXTBSY`. A successful probe means no description open for
//! writing exists any more, and nothing opens the file for writing again, so
//! every later execution of it cannot fail that way.

use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Set only by the probe. The guard line makes a probed script exit before it
/// does anything.
const PROBE_VAR: &str = "HERDR_VOICE_FIXTURE_PROBE";
const GUARD: &str = "[ -n \"$HERDR_VOICE_FIXTURE_PROBE\" ] && exit 0";
/// The error number of `ETXTBSY`, the same on Linux and macOS. `ErrorKind`
/// does not name it before Rust 1.83 and this crate builds on 1.82.
const ETXTBSY: i32 = 26;

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    struct Scratch(std::path::PathBuf);

    impl Scratch {
        fn new(tag: &str) -> Self {
            let dir = std::env::temp_dir().join(format!(
                "herdr-voice-script-fixture-{tag}-{}",
                std::process::id()
            ));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).expect("scratch dir");
            Scratch(dir)
        }

        fn path(&self, name: &str) -> std::path::PathBuf {
            self.0.join(name)
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// A script that already carries the guard, created the way
    /// `write_executable` creates it but without waiting, so a test can keep it
    /// open.
    fn guarded_script(path: &Path) {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o755)
            .open(path)
            .expect("create the script");
        file.write_all(format!("#!/bin/sh\n{GUARD}\n").as_bytes())
            .expect("write the script");
    }

    #[test]
    fn the_probe_runs_nothing_of_the_script_and_a_real_run_runs_all_of_it() {
        let scratch = Scratch::new("probe");
        let out = scratch.path("out.txt");
        let script = scratch.path("s.sh");
        write_executable(&script, &format!("#!/bin/sh\nprintf 'ran\\n' > {out:?}\n"));
        assert!(
            !out.exists(),
            "the probe must exit before the script's own commands"
        );
        let status = Command::new(&script).status().expect("run the script");
        assert!(status.success());
        assert_eq!(std::fs::read_to_string(&out).unwrap(), "ran\n");
    }

    #[test]
    fn a_script_still_open_for_writing_is_waited_for_and_not_reported_as_busy() {
        let scratch = Scratch::new("held");
        let path = scratch.path("s.sh");
        guarded_script(&path);
        let held = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .expect("hold the script open for writing");
        let released = Arc::new(AtomicBool::new(false));
        let release = released.clone();
        let (go, started) = std::sync::mpsc::channel::<()>();
        let releaser = std::thread::spawn(move || {
            started.recv().expect("the wait is about to begin");
            std::thread::sleep(Duration::from_millis(400));
            // Set before the handle goes, so a call that has returned can only
            // have seen it set.
            release.store(true, Ordering::SeqCst);
            drop(held);
        });
        go.send(()).unwrap();
        let retries = wait_until_executable(&path, Duration::from_secs(10));
        let released_when_it_returned = released.load(Ordering::SeqCst);
        releaser.join().unwrap();
        if cfg!(target_os = "linux") {
            assert!(
                released_when_it_returned,
                "it returned while the script was still open for writing"
            );
            assert!(retries >= 1, "Linux refuses a busy file; got {retries} retries");
        } else {
            // macOS executes a file that is open for writing: there is nothing
            // to wait for, so it returns at once, before the handle is released.
            assert_eq!(retries, 0);
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    #[should_panic(expected = "cannot be executed")]
    fn a_script_that_stays_open_past_the_deadline_panics_naming_the_script() {
        let scratch = Scratch::new("deadline");
        let path = scratch.path("s.sh");
        guarded_script(&path);
        let _held = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .expect("hold the script open for writing");
        wait_until_executable(&path, Duration::from_millis(100));
    }

    #[test]
    #[should_panic(expected = "cannot be executed")]
    fn a_script_that_cannot_be_executed_for_another_reason_panics_at_once() {
        let scratch = Scratch::new("noexec");
        let path = scratch.path("s.sh");
        // `fs::write` creates the file without the execute bit.
        std::fs::write(&path, format!("#!/bin/sh\n{GUARD}\n")).unwrap();
        wait_until_executable(&path, Duration::from_secs(10));
    }

    #[test]
    #[should_panic(expected = "must start with #!")]
    fn a_script_without_an_interpreter_line_is_refused() {
        let scratch = Scratch::new("nointerpreter");
        write_executable(&scratch.path("s.sh"), "echo hello\nexit 0\n");
    }
}
```

- [ ] **Step 3: Run, expect a compile failure.**
  `CARGO_BUILD_JOBS=6 cargo test --bin herdr-voice script_fixture` — expected:
  `cannot find function write_executable` and `wait_until_executable`.

- [ ] **Step 4: Implement.** In `src/script_fixture.rs`, insert between the
  `const ETXTBSY` line and `#[cfg(test)]`:

```rust
/// Writes `content` to `path` with mode `0o755` and returns once the script can
/// be executed. `content` must start with a `#!` line; a guard line is inserted
/// after it so the probe execution does nothing.
pub fn write_executable(path: &Path, content: &str) {
    let (interpreter, rest) = content
        .split_once('\n')
        .expect("a fixture script has more than one line");
    assert!(
        interpreter.starts_with("#!"),
        "a fixture script must start with #!, got {interpreter:?}"
    );
    {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o755)
            .open(path)
            .unwrap_or_else(|e| panic!("cannot create {}: {e}", path.display()));
        file.write_all(format!("{interpreter}\n{GUARD}\n{rest}").as_bytes())
            .unwrap_or_else(|e| panic!("cannot write {}: {e}", path.display()));
    } // The write descriptor is closed here, before anything is executed.
    wait_until_executable(path, Duration::from_secs(10));
}

/// Executes the script once with the probe variable set, retrying every 5 ms
/// while the spawn fails with `ETXTBSY`, and returns the number of retries.
/// Any other spawn error, or `ETXTBSY` still present after `deadline`, panics
/// with the path and the error.
pub fn wait_until_executable(path: &Path, deadline: Duration) -> u32 {
    let start = Instant::now();
    let mut retries = 0;
    loop {
        let spawned = Command::new(path)
            .env(PROBE_VAR, "1")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        match spawned {
            Ok(_) => return retries,
            Err(e) if e.raw_os_error() == Some(ETXTBSY) && start.elapsed() < deadline => {
                retries += 1;
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(e) => panic!("{} cannot be executed: {e}", path.display()),
        }
    }
}
```

- [ ] **Step 5: Run, expect pass.**
  `CARGO_BUILD_JOBS=6 cargo test --bin herdr-voice script_fixture` — on macOS 4
  tests pass (the Linux-only one is not compiled). Expected line:
  `test result: ok. 4 passed`.

- [ ] **Step 6: Format and commit.**

```bash
cargo fmt && git add src/script_fixture.rs src/main.rs && git commit -m "Add a test fixture that writes a script and waits until it can be executed

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

## Task 6: the five fixtures use it (#66, AC-6)

**Depends on:** Task 5.

**Files:** Modify `src/indicator.rs`, `src/delivery.rs`, `src/setup.rs`,
`src/bias.rs`, `src/bias/pane.rs` (unix fixture functions only; the
`cfg(windows)` variants are not touched).

**Interfaces:** Consumes `crate::script_fixture::write_executable(&Path, &str)`.

Each replacement keeps the script's text identical; only the write-and-`chmod`
sequence changes. A fixture is correct when `grep -n 'set_permissions\|set_mode'`
finds nothing in its function.

- [ ] **Step 1: `src/indicator.rs`, `fake_herdr` (`#[cfg(unix)]`).** Replace

```rust
        std::fs::write(
            &fake.script,
            format!(
                "#!/bin/sh\nprintf '%s\\n' \"$@\" > {:?}\ncat {:?}\nexit {code}\n",
                fake.argv, fake.answer
            ),
        )
        .expect("write the script");
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&fake.script).expect("stat").permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&fake.script, perms).expect("chmod");
        fake
```

  with

```rust
        crate::script_fixture::write_executable(
            &fake.script,
            &format!(
                "#!/bin/sh\nprintf '%s\\n' \"$@\" > {:?}\ncat {:?}\nexit {code}\n",
                fake.argv, fake.answer
            ),
        );
        fake
```

- [ ] **Step 2: `src/delivery.rs`, `recorder` (`#[cfg(unix)]`).** Replace

```rust
        let recorder = Recorder::new(tag, "record.sh");
        std::fs::write(
            &recorder.script,
            format!("#!/bin/sh\nprintf '%s\\n' \"$@\" > {:?}\n", recorder.out),
        )
        .expect("write recorder script");
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&recorder.script)
            .expect("stat")
            .permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&recorder.script, perms).expect("chmod");
        recorder
```

  with

```rust
        let recorder = Recorder::new(tag, "record.sh");
        crate::script_fixture::write_executable(
            &recorder.script,
            &format!("#!/bin/sh\nprintf '%s\\n' \"$@\" > {:?}\n", recorder.out),
        );
        recorder
```

  and in `a_program_that_starts_and_fails_is_still_a_rejection` replace

```rust
        std::fs::write(
            &recorder.script,
            "#!/bin/sh\necho '{\"error\":{\"code\":\"pane_not_found\",\"message\":\"gone\"}}'\nexit 1\n",
        )
        .expect("write");
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&recorder.script)
            .expect("stat")
            .permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&recorder.script, perms).expect("chmod");
        let deliverer
```

  with

```rust
        crate::script_fixture::write_executable(
            &recorder.script,
            "#!/bin/sh\necho '{\"error\":{\"code\":\"pane_not_found\",\"message\":\"gone\"}}'\nexit 1\n",
        );
        let deliverer
```

- [ ] **Step 3: `src/setup.rs`, `herdr_cli::Recorder::new`.** Remove the line
  `use std::os::unix::fs::PermissionsExt;` at the top of that function, and
  replace

```rust
                std::fs::write(
                    &recorder.script,
                    format!(
                        "#!/bin/sh\n\
                         {{ printf '%s\\n' \"$@\"; \
                         printf 'HERDR_CONFIG_PATH=%s\\n' \"${{HERDR_CONFIG_PATH-unset}}\"; \
                         }} > {out:?}\n\
                         echo {text:?}\n\
                         exit {code}\n",
                        out = recorder.out,
                        text = text,
                        code = code,
                    ),
                )
                .expect("write the recorder script");
                let mut perms = std::fs::metadata(&recorder.script)
                    .expect("stat")
                    .permissions();
                perms.set_mode(0o755);
                std::fs::set_permissions(&recorder.script, perms).expect("chmod");
                recorder
```

  with

```rust
                crate::script_fixture::write_executable(
                    &recorder.script,
                    &format!(
                        "#!/bin/sh\n\
                         {{ printf '%s\\n' \"$@\"; \
                         printf 'HERDR_CONFIG_PATH=%s\\n' \"${{HERDR_CONFIG_PATH-unset}}\"; \
                         }} > {out:?}\n\
                         echo {text:?}\n\
                         exit {code}\n",
                        out = recorder.out,
                        text = text,
                        code = code,
                    ),
                );
                recorder
```

- [ ] **Step 4: `src/bias.rs` and `src/bias/pane.rs`, `scratch_script`
  (`#[cfg(unix)]`, identical text in both files).** Replace in each

```rust
        let script = dir.join("script.sh");
        std::fs::write(
            &script,
            format!("#!/bin/sh\ncat {stdout_path:?}\ncat {stderr_path:?} >&2\nexit {exit_code}\n"),
        )
        .expect("write script");
        let mut perms = std::fs::metadata(&script).unwrap().permissions();
        std::os::unix::fs::PermissionsExt::set_mode(&mut perms, 0o755);
        std::fs::set_permissions(&script, perms).expect("chmod script");
        script
```

  with

```rust
        let script = dir.join("script.sh");
        crate::script_fixture::write_executable(
            &script,
            &format!("#!/bin/sh\ncat {stdout_path:?}\ncat {stderr_path:?} >&2\nexit {exit_code}\n"),
        );
        script
```

- [ ] **Step 5: Confirm none is left.**
  `grep -n 'set_mode(0o755)\|from_mode(0o755)' src/indicator.rs src/delivery.rs src/setup.rs src/bias.rs src/bias/pane.rs`
  — expect no output. (`src/setup.rs` has other `set_permissions` uses with
  modes `0o600`, `0o500`, `0o700` that are not script fixtures; leave them.)

- [ ] **Step 6: Run the five modules.**
  `CARGO_BUILD_JOBS=6 cargo test --bin herdr-voice` — whole suite passes. Expect
  `test result: ok.` with the count higher than before by the tests added in
  Tasks 2 to 5.

- [ ] **Step 7: Format and commit.**

```bash
cargo fmt && git add src && git commit -m "Write every fixture script through one function that waits for it to be executable

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

## Task 7: `serve` keeps its listener and records the paths (D5, AC-11, AC-12)

**Files:** Modify `src/stt/fetch.rs` (`mod tests`: the doc comment and body of
`serve` at `:247-322`, and the six call sites).

**Interfaces:**
- Produces (used by Tasks 8 and 9):
  - `struct Server` (test-only) with `fn finish(self) -> Vec<String>`
  - `fn serve(files: Vec<(&'static str, &'static str, Vec<u8>)>) -> (String, Server)`

- [ ] **Step 1: Write the failing test first.** At the end of `mod tests` (before
  its final closing brace) add:

```rust
    #[test]
    fn a_request_the_table_does_not_hold_is_answered_404_and_recorded() {
        let dir = scratch("unheld");
        let (base, server) = serve(vec![]);
        let error = fetch_into(&base, &entry(), &dir, &mut Silent).expect_err("must refuse");
        let asked = server.finish();

        assert!(
            matches!(error, FetchError::Status { code: 404, .. }),
            "got {error:?}"
        );
        assert_eq!(
            asked,
            vec!["/openai/whisper-fixture/resolve/0000000000000000000000000000000000000000/model.safetensors".to_string()]
        );
    }
```

  Run `CARGO_BUILD_JOBS=6 cargo test --bin herdr-voice stt::fetch` — expected:
  compile error, `no method named finish` on the `JoinHandle`.

- [ ] **Step 2: Replace `serve`.** In `mod tests` replace the whole doc comment
  and function `serve` (from the line `/// Serves each requested path from a
  fixed table until nothing has connected` through the closing `}` of the
  function, just before `struct Silent;`) with:

```rust
    /// A running download double. The listener lives in its thread and stays
    /// bound until `finish` (or a drop, when a test panics first) sets the stop
    /// flag, so nothing but the test itself decides when the address stops
    /// answering.
    struct Server {
        stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
        thread: Option<std::thread::JoinHandle<Vec<String>>>,
    }

    impl Server {
        /// Stops the double and returns the path of every request it received,
        /// in the order they arrived, the ones it answered 404 included.
        fn finish(mut self) -> Vec<String> {
            self.stop.store(true, std::sync::atomic::Ordering::SeqCst);
            self.thread
                .take()
                .expect("finished once")
                .join()
                .expect("the double must not panic")
        }
    }

    impl Drop for Server {
        fn drop(&mut self) {
            self.stop.store(true, std::sync::atomic::Ordering::SeqCst);
        }
    }

    /// Serves each requested path from a fixed table, answering 404 for a path
    /// the table does not hold, and records the path of every request. One
    /// thread, any number of connections, until the returned `Server` is
    /// finished.
    ///
    /// It stops on a request from the test and not on a timer: a timer closes
    /// the listener under a client that is late because the machine is busy, and
    /// the client then gets "Connection refused" on its own address. It stops on
    /// a request and not on a connection count because the count is not knowable
    /// from the test: `fetch_into` fetches three files but stops at the first
    /// failure, so a double waiting for three connections would block forever in
    /// exactly the tests that exercise a failure.
    ///
    /// The request is read to the end of its headers before the response is
    /// written, for the reason `src/rewrite/http.rs` records: a stream dropped
    /// while the kernel still holds unread bytes can turn the close into a reset,
    /// which surfaces as an intermittent, unrelated-looking failure.
    fn serve(files: Vec<(&'static str, &'static str, Vec<u8>)>) -> (String, Server) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        listener.set_nonblocking(true).expect("nonblocking");
        let addr = listener.local_addr().expect("addr");
        let base = format!("http://{addr}");
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let stopped = stop.clone();
        let thread = std::thread::spawn(move || {
            let mut paths = Vec::new();
            while !stopped.load(std::sync::atomic::Ordering::SeqCst) {
                let (mut stream, _) = match listener.accept() {
                    Ok(pair) => pair,
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(std::time::Duration::from_millis(5));
                        continue;
                    }
                    Err(_) => return paths,
                };
                stream.set_nonblocking(false).expect("blocking stream");
                // A client that connects and never sends must not hold the
                // thread, and with it `finish`, forever.
                stream
                    .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                    .expect("read timeout");
                let mut buf = Vec::new();
                let mut chunk = [0u8; 4096];
                loop {
                    if buf.windows(4).any(|w| w == b"\r\n\r\n") {
                        break;
                    }
                    match stream.read(&mut chunk) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => buf.extend_from_slice(&chunk[..n]),
                    }
                }
                let request = String::from_utf8_lossy(&buf).to_string();
                let first = request.lines().next().unwrap_or_default().to_string();
                paths.push(first.split_whitespace().nth(1).unwrap_or_default().to_string());
                let matched = files.iter().find(|(suffix, _, _)| first.contains(suffix));
                match matched {
                    Some((_, status, body)) => {
                        let head = format!(
                            "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            body.len()
                        );
                        let _ = stream.write_all(head.as_bytes());
                        let _ = stream.write_all(body);
                    }
                    None => {
                        let _ = stream.write_all(
                            b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                        );
                    }
                }
                let _ = stream.flush();
            }
            paths
        });
        (
            base,
            Server {
                stop,
                thread: Some(thread),
            },
        )
    }
```

- [ ] **Step 3: Update the six call sites.** In each of the six tests
  (`a_good_transfer_leaves_three_files_and_no_part`,
  `a_short_transfer_names_the_size_and_leaves_nothing_behind`,
  `the_right_length_and_the_wrong_bytes_are_caught_by_the_digest`,
  `a_non_2xx_response_names_the_status_and_the_address`,
  `a_right_length_wrong_bytes_file_is_fetched_again_rather_than_skipped`,
  `a_file_already_there_and_whole_is_not_fetched_again`) replace the statement
  `handle.join().unwrap();` with `handle.finish();`. The variable stays named
  `handle`. (`grep -n 'handle.join' src/stt/fetch.rs` must then print nothing.)

- [ ] **Step 4: Run, expect pass.**
  `CARGO_BUILD_JOBS=6 cargo test --bin herdr-voice stt::fetch` — expect
  `test result: ok. 10 passed` (nine existing plus the new one).

- [ ] **Step 5: Format and commit.**

```bash
cargo fmt && git add src/stt/fetch.rs && git commit -m "Keep the download double's listener until its test is done and record what it was asked for

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

## Task 8: the unreachable address no longer frees a port (D4, AC-10)

**Depends on:** Task 7 (same file).

**Files:** Modify `src/stt/fetch.rs`, test
`an_unreachable_address_says_so_rather_than_hanging`.

- [ ] **Step 1:** In that test replace

```rust
        // Bound and immediately dropped: the port is not listening.
        let port = {
            let l = TcpListener::bind("127.0.0.1:0").unwrap();
            l.local_addr().unwrap().port()
        };
        let error = fetch_into(
            &format!("http://127.0.0.1:{port}"),
            &entry(),
            &dir,
            &mut Silent,
        )
        .expect_err("must refuse");
```

  with

```rust
        // Nothing can listen on port 0, so no sibling test can have been handed
        // it, and the connection attempt fails at once.
        let error = fetch_into("http://127.0.0.1:0", &entry(), &dir, &mut Silent)
            .expect_err("must refuse");
```

- [ ] **Step 2: Run.**
  `CARGO_BUILD_JOBS=6 cargo test --bin herdr-voice an_unreachable` — expect
  `1 passed`, finished in well under a second. If it hangs or takes seconds, stop
  and report: do not change the fixture.

- [ ] **Step 3:** `grep -n 'local_addr().unwrap().port()' src/stt/fetch.rs` —
  expect no output.

- [ ] **Step 4: Commit.**

```bash
cargo fmt && git add src/stt/fetch.rs && git commit -m "Point the unreachable-address test at port 0 instead of a port it freed

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

## Task 9: a test pins the revision (D6, AC-13, AC-14)

**Depends on:** Task 7.

**Files:** Modify `src/stt/fetch.rs` (`mod tests`).

- [ ] **Step 1: Add the test** after `a_good_transfer_leaves_three_files_and_no_part`:

```rust
    #[test]
    fn every_file_is_requested_at_the_revision_the_entry_pins() {
        let dir = scratch("pinned");
        let (base, server) = serve(all_three("200 OK"));
        let entry = entry();
        fetch_into(&base, &entry, &dir, &mut Silent).expect("must succeed");

        let mut asked = server.finish();
        asked.sort();
        let mut expected: Vec<String> = entry
            .files
            .iter()
            .map(|file| format!("/{}/resolve/{}/{}", entry.repo, entry.revision, file.name))
            .collect();
        expected.sort();
        assert_eq!(asked, expected);
    }
```

- [ ] **Step 2: Run, expect pass.**
  `CARGO_BUILD_JOBS=6 cargo test --bin herdr-voice every_file_is_requested` — `1 passed`.

- [ ] **Step 3: Run the mutation of #48 and see it fail.** In `src/stt/fetch.rs`
  function `one`, temporarily change

```rust
        entry.repo, entry.revision, file.name
```

  to

```rust
        entry.repo, "main", file.name
```

  then `CARGO_BUILD_JOBS=6 cargo test --bin herdr-voice stt::fetch 2>&1 | tail -30`.
  Expected: `every_file_is_requested_at_the_revision_the_entry_pins ... FAILED`
  (the left and right vectors differ in `main` against forty zeros). Copy the
  command and the failing lines into your working notes: they go into
  `docs/evidence.md` in S5.

- [ ] **Step 4: Revert the mutation.** Change `"main"` back to `entry.revision`
  by editing the same line, then `git diff src/stt/fetch.rs | grep -n '"main"'`
  — expect no output — and re-run
  `CARGO_BUILD_JOBS=6 cargo test --bin herdr-voice stt::fetch` — all pass.

- [ ] **Step 5: Commit.**

```bash
cargo fmt && git add src/stt/fetch.rs && git commit -m "Fail a test when the model download stops using the pinned revision

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

## Task 10: the gates

**Depends on:** Tasks 1 to 9. Run one command at a time.

- [ ] **Step 1:** `CARGO_BUILD_JOBS=6 cargo test` — whole suite, read all of it;
  expect `test result: ok.` and no `FAILED`.
- [ ] **Step 2:** `CARGO_BUILD_JOBS=6 cargo clippy --all-targets -- -D warnings`
  — expect no warning.
- [ ] **Step 3:** `cargo fmt --check` — expect no output.
- [ ] **Step 4:** `python3 scripts/check_manifest.py` — expect exit 0.
- [ ] **Step 5: Windows dead-code check, on a scratch copy**, with the one shared
  target directory for the run:

```bash
W=$(mktemp -d) && rsync -a --exclude target --exclude .git ./ "$W/" && cd "$W" &&
find src -name '*.rs' -exec sed -i '' \
  -e 's/#\[cfg(all(test, unix))\]/#[cfg(any())]/g' \
  -e 's/#\[cfg(unix)\]/#[cfg(any())]/g' \
  -e 's/#\[cfg(not(unix))\]/#[cfg(not(any()))]/g' \
  -e 's/#\[cfg(windows)\]/#[cfg(not(any()))]/g' {} + &&
CARGO_BUILD_JOBS=6 CARGO_TARGET_DIR="$TMPDIR/hv-66-target" cargo clippy --all-targets -- -D warnings
```

  Expect no warning. Then return to the worktree (`cd` back) and
  `rm -rf "$W"`. If it reports an unused item, the item is reachable only from a
  `cfg(unix)` path: gate it the same way and re-run Steps 1 to 5.
- [ ] **Step 6:** Public-repository check: `test -s .leakwords && git config core.hooksPath`
  prints `.githooks`; `git diff main --stat` lists only files named in this plan
  plus `tasks/66/`.
- [ ] **Step 7:** Nothing to commit unless a gate needed a fix; if one did,
  commit it with a message that names the gate.

---

## Task 11: S5 evidence and the pull request

**Depends on:** Task 10 and the S4 review and mutation testing, which are done
by the run and recorded in `tasks/66/RUN_66.md` before this task starts.

**Files:** Modify `docs/evidence.md` (append one section).

- [ ] **Step 1: The local loop.** On this machine, the affected modules, 6 test
  threads, a hard timeout and no more than 6 concurrent processes in total:

```bash
CARGO_BUILD_JOBS=6 cargo test --no-run --bin herdr-voice 2>&1 | tail -3
BIN=$(ls -t target/debug/deps/herdr_voice-* | grep -v '\.d$' | head -1)
pass=0; fail=0
for i in $(seq 1 50); do
  if timeout 120 "$BIN" --test-threads=6 indicator:: delivery:: setup:: bias:: stt::fetch script_fixture >/dev/null 2>&1; then pass=$((pass+1)); else fail=$((fail+1)); fi
done
echo "runs=50 pass=$pass fail=$fail"
```

  Record the output verbatim. Say next to it that macOS does not return
  `ETXTBSY` (measured: Python `subprocess` executes a file open for writing), so
  this loop cannot show the #66 flake gone; the CI attempts below do.
- [ ] **Step 2: Push and read CI.** `git push -u origin fix/66-101-spawned-fixtures`,
  then `gh run list --branch fix/66-101-spawned-fixtures --limit 5` and
  `gh run view <run-id> --json jobs --jq '.jobs[] | {name, databaseId, conclusion}'`
  to find the `ubuntu-latest` test job's `databaseId`.
- [ ] **Step 3: Six clean attempts.** Rerun that job until six attempts of it
  have concluded `success` (the push's first run is attempt 1):
  `gh run rerun <run-id> --job <job-id>`, wait for it to finish
  (`gh run watch <run-id> --exit-status` or poll `gh run view <run-id> --attempt <n> --json conclusion`),
  and record every attempt's number, id and conclusion. **One failure is a
  finding, not a retry:** stop, read the failing test's whole output, and put the
  test name, the run id and the error text into `tasks/66/RUN_66.md`.
- [ ] **Step 4: Write the evidence.** Append to `docs/evidence.md` a section
  `## Text file busy in test fixtures, and the download double (#66, #101, #62, #48)`
  containing, each with the command, its output and the platform: the mutation of
  Task 9 failing; the local loop result and the sentence about macOS; the
  before rate (about 5 failures in about 12 `ubuntu-latest` runs between
  2026-09-30 and 2026-10-05, from the issue); the table of attempts with run id,
  attempt number and conclusion; the measurements from
  `tasks/66/DESIGN_66_evidence.md` items 1, 2 and 4 with the commands as written
  there (the measurement that executes a file open for writing, the 1200 ms
  late-connect probe, port 0). No absolute path, no account name.
- [ ] **Step 3b:** Stray-marker check, then commit
  `git add docs/evidence.md tasks/66 && git commit` with the message
  `Record the evidence that the spawn fixtures and the download double are stable`
  and the trailer.
- [ ] **Step 5: Open the pull request** with the repository template
  (`.github/pull_request_template.md`) and these lines, each alone on its line:

```
Closes #66
Closes #101
Closes #62
Closes #48
```

  then end the body with `🤖 Generated with [Claude Code](https://claude.com/claude-code)`.
  After the last push read the **whole** output of `gh pr checks <number>`.
