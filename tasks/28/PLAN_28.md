# PLAN_28 — bounded outward calls and a safe cut of a program's error

> **For agentic workers:** REQUIRED SUB-SKILL: `superpowers:executing-plans` with
> `superpowers:test-driven-development`. Steps use checkbox syntax.

**Goal:** every outward call that can wedge a daemon thread has a bound and a
message that names what stopped; no program's text is cut off a character
boundary.

**Architecture:** one new module `src/outward.rs` (`run`, `shorten`, `RunError`).
Four call sites switch to it and gain a `TimedOut` error variant. One new key,
`[stt] command_timeout_seconds`. Decisions are recorded in `docs/decisions.md` and
the key in `docs/design.md`.

**Tech stack:** Rust 1.82 floor, standard library only (no new crate).

**Spec:** `tasks/28/DESIGN_28.md`; criteria in `tasks/28/AC_28.md`.

## Global constraints

- Every shell command that runs cargo sets `CARGO_BUILD_JOBS=6`. One cargo process
  at a time, never two together.
- No new dependency in `Cargo.toml`.
- No absolute path and no employer, client or private-machine name in any file,
  comment, test or commit message. Cite paths relative to the repository root.
- Everything in the repository is English.
- No panic path in the daemon: no `unwrap`, `expect` or indexing on a value that
  came from a program, a file or a socket, outside `#[cfg(test)]`.
- Every user-visible failure text names what to do next.
- Configuration keys have defaults; an absent file is valid.
- The key name `command_timeout_seconds` is **not final**: the owner has not
  answered. Use it as written; if the orchestrator sends another name before
  Task 3 starts, use that name everywhere this plan says `command_timeout_seconds`
  (the field in `src/config.rs`, the message in `src/stt/command.rs`, the docs).
- Bounds, exactly: delivery 10 s, pane read 5 s, transcriber 60 s default, rewrite
  30 s.
- Before every commit: grep every file written for `<new_string>`, `</new_string>`,
  `<old_string>`, `</old_string>` and line-start conflict markers
  (`^<<<<<<<`, `^=======`, `^>>>>>>>`). Commit trailer:
  `Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>`.
- Tests that start a program and must not depend on a script file use
  `sh -c "…"` as the program. Tests that need a herdr stand-in write a script the
  way `src/delivery.rs` tests already do (issue #66 explains an intermittent
  `Text file busy` on Linux CI for those; a rerun of that job once is the rule).

## Review focus

Failure modes the design implies and no single task's happy path exercises, each
pinned by a test named in the task that owns it:

1. A program that exits but leaves a background child holding its output pipe
   (`sh -c "sleep 30 & echo done"`): without a bound on the pipe the call blocks as
   it does today. Task 2.
2. A transcriber that is `sh -c "…"` around the real program: killing only `sh`
   leaves the real program running. Task 2 (group kill).
3. Output larger than a pipe buffer from both streams at once: a wait that does not
   read both deadlocks. Task 2.
4. A standard error whose multi-byte character straddles byte 400, for each of two,
   three and four bytes. Tasks 1, 4, 5.
5. A configuration value of `0` for the new key. Task 3.

---

## Task dependencies

```
T1 shorten ─────────────┬─> T4 transcriber ─┐
T2 run ─────────────────┼─> T5 rewrite ─────┼─> T8 daemon tests ─> T11 gates
T3 config key ──────────┘   (T4 needs T3)   │
T2 ─> T6 delivery ──────────────────────────┤
T2 ─> T7 pane read ─────────────────────────┤
T4, T5 ─> T9 source scan ───────────────────┤
T3..T7 ─> T10 docs ─────────────────────────┘
```

Tasks 1, 2 and 3 are independent of each other. Run them in numeric order anyway:
one cargo process at a time.

---

### Task 1: `shorten` and the `outward` module

**Files:**
- Create: `src/outward.rs`
- Modify: `src/main.rs` (add `mod outward;` in the alphabetical list, between
  `mod indicator;` and `mod proto;`)

**Interfaces:**
- Produces: `pub fn shorten(text: &str, limit: usize) -> String` — at most `limit`
  bytes, cut on a character boundary, never panics, returns `text` unchanged when
  it already fits.

- [ ] **Step 1: Write the failing tests.** Create `src/outward.rs` with only the
  module comment and this test module (no `shorten` yet):

```rust
//! Running somebody else's program with a bound on how long it may take, and
//! cutting what it printed. `docs/decisions.md` holds the bound for each call.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_that_fits_is_returned_unchanged() {
        assert_eq!(shorten("short", 400), "short");
        assert_eq!(shorten("exact", 5), "exact");
    }

    #[test]
    fn ascii_is_cut_at_the_limit() {
        assert_eq!(shorten("abcdef", 3), "abc");
    }

    #[test]
    fn a_limit_of_zero_gives_nothing() {
        assert_eq!(shorten("abc", 0), "");
    }

    #[test]
    fn a_two_byte_character_across_the_limit_is_dropped_whole() {
        let text = format!("{}é", "a".repeat(399));
        assert_eq!(text.len(), 401);
        assert_eq!(shorten(&text, 400), "a".repeat(399));
    }

    #[test]
    fn a_three_byte_character_across_the_limit_is_dropped_whole() {
        let text = format!("{}—", "a".repeat(398));
        assert_eq!(text.len(), 401);
        assert_eq!(shorten(&text, 400), "a".repeat(398));
        assert_eq!(shorten(&text, 399), "a".repeat(398));
    }

    #[test]
    fn a_four_byte_character_across_the_limit_is_dropped_whole() {
        let text = format!("{}😀", "a".repeat(397));
        assert_eq!(text.len(), 401);
        assert_eq!(shorten(&text, 400), "a".repeat(397));
    }

    #[test]
    fn no_limit_panics_on_cyrillic_text() {
        let text = "ошибка чтения файла".repeat(30);
        for limit in 0..=text.len() + 1 {
            let cut = shorten(&text, limit);
            assert!(cut.len() <= limit, "limit {limit}: {} bytes", cut.len());
            assert!(text.starts_with(&cut), "limit {limit}");
        }
    }
}
```

  In `src/main.rs` add `mod outward;` between `mod indicator;` and `mod proto;`.

- [ ] **Step 2: Run to see it fail.**
  `CARGO_BUILD_JOBS=6 cargo test outward::` — expected: compile error, `shorten`
  not found.

- [ ] **Step 3: Implement.** Add above the test module:

```rust
/// `text` cut to at most `limit` bytes on a character boundary.
///
/// `String::truncate` panics when the length is not on a boundary, and program
/// output reaches here through `String::from_utf8_lossy`, where any multi-byte
/// character, or the replacement character, can straddle the limit (issue #94).
pub fn shorten(text: &str, limit: usize) -> String {
    if text.len() <= limit {
        return text.to_string();
    }
    let mut end = limit;
    // Zero is always a boundary, so this ends.
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].to_string()
}
```

- [ ] **Step 4: Run to see it pass.** `CARGO_BUILD_JOBS=6 cargo test outward::` —
  all seven pass. `cargo clippy` will report `shorten` unused until Tasks 4 and 5;
  add `#![allow(dead_code)]`-free handling by adding `#[allow(dead_code)]` on
  `shorten` for this commit only, and remove it in Task 4.

- [ ] **Step 5: Commit.**

```bash
git add src/outward.rs src/main.rs
git commit -m "Add a character-boundary cut for text from a program (#94)"
```

---

### Task 2: `run`, a bounded wait for a program

**Files:**
- Modify: `src/outward.rs`
- Modify: `src/http_failure.rs:148` — change `fn bound_text` to `pub(crate) fn bound_text`

**Interfaces:**
- Consumes: nothing from earlier tasks.
- Produces:
  - `pub enum RunError { Start(std::io::Error), TimedOut }` (derives `Debug`)
  - `pub fn run(command: &mut std::process::Command, bound: std::time::Duration) -> Result<std::process::Output, RunError>`
  - `crate::http_failure::bound_text(Duration) -> String` visible crate-wide
    ("1 second", "30 seconds", "200 milliseconds").

Behaviour: standard input is null; standard output and error are piped and read
by one thread each; the child is polled every 10 ms; on Unix the child is its own
process group. On the deadline the group is killed, the child reaped, and
`TimedOut` returned. After a normal exit, each pipe is read for at most
`max(time left, 1 second)`; a pipe still open then (a background child holds it)
kills the group and returns `TimedOut`. Reader threads of a timed-out call are not
waited for: the output of a timed-out call is discarded, so nothing needs them, and
they end when the killed group closes its pipes. (`DESIGN_28.md` section 2 step 5
gives these threads one second; that wait has no purpose once output is discarded.
Record this as a deviation in `RUN_28.md`.)

- [ ] **Step 1: Write the failing tests.** Append inside `mod tests` in
  `src/outward.rs` (the whole group is Unix-only because it runs `sh`):

```rust
    #[cfg(unix)]
    mod run_tests {
        use super::super::*;
        use std::process::Command;
        use std::time::{Duration, Instant};

        fn sh(script: &str) -> Command {
            let mut command = Command::new("sh");
            command.arg("-c").arg(script);
            command
        }

        /// Whether a process still exists, polled because a killed process is
        /// briefly a zombie until its parent reaps it.
        fn gone(pid: &str) -> bool {
            for _ in 0..40 {
                let alive = Command::new("kill")
                    .arg("-0")
                    .arg(pid)
                    .stderr(std::process::Stdio::null())
                    .status()
                    .map(|status| status.success())
                    .unwrap_or(false);
                if !alive {
                    return true;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            false
        }

        fn scratch(tag: &str) -> std::path::PathBuf {
            let dir = std::env::temp_dir().join(format!(
                "herdr-voice-outward-{tag}-{}",
                std::process::id()
            ));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).expect("create scratch");
            dir
        }

        #[test]
        fn a_program_that_finishes_returns_what_it_printed_and_its_status() {
            let output = run(
                &mut sh("printf out; printf err >&2; exit 3"),
                Duration::from_secs(5),
            )
            .expect("runs");
            assert_eq!(output.stdout, b"out");
            assert_eq!(output.stderr, b"err");
            assert_eq!(output.status.code(), Some(3));
        }

        #[test]
        fn a_program_that_is_not_there_is_a_start_failure() {
            let error = run(
                &mut Command::new("/definitely/not/a/real/program"),
                Duration::from_secs(5),
            )
            .expect_err("must fail");
            match error {
                RunError::Start(e) => assert_eq!(e.kind(), std::io::ErrorKind::NotFound),
                other => panic!("expected Start, got {other:?}"),
            }
        }

        #[test]
        fn standard_input_is_closed_so_a_program_that_reads_it_does_not_wait() {
            let started = Instant::now();
            let output = run(&mut sh("cat"), Duration::from_secs(5)).expect("runs");
            assert!(output.stdout.is_empty());
            assert!(started.elapsed() < Duration::from_secs(3));
        }

        #[test]
        fn output_larger_than_a_pipe_on_both_streams_does_not_deadlock() {
            let output = run(
                &mut sh("head -c 300000 /dev/zero | tr '\\0' x; \
                         head -c 300000 /dev/zero | tr '\\0' y >&2"),
                Duration::from_secs(10),
            )
            .expect("runs");
            assert_eq!(output.stdout.len(), 300_000);
            assert_eq!(output.stderr.len(), 300_000);
        }

        #[test]
        fn a_program_that_outlasts_the_bound_is_stopped_and_reported() {
            let started = Instant::now();
            let error = run(&mut sh("sleep 30"), Duration::from_millis(200)).expect_err("times out");
            assert!(matches!(error, RunError::TimedOut), "got {error:?}");
            assert!(
                started.elapsed() < Duration::from_secs(3),
                "returned after {:?}",
                started.elapsed()
            );
        }

        #[test]
        fn a_timeout_kills_what_the_program_started_not_only_the_program() {
            let dir = scratch("group");
            let pidfile = dir.join("pid");
            // `sh -c "program"` is how a transcriber is usually configured;
            // the grandchild is the real program.
            let script = format!("sleep 30 & echo $! > {pidfile:?}; wait");
            let error = run(&mut sh(&script), Duration::from_millis(500)).expect_err("times out");
            assert!(matches!(error, RunError::TimedOut), "got {error:?}");
            let pid = std::fs::read_to_string(&pidfile).expect("the script wrote it");
            assert!(gone(pid.trim()), "process {} is still running", pid.trim());
        }

        #[test]
        fn a_background_child_holding_the_pipe_open_is_a_timeout_not_a_hang() {
            let dir = scratch("pipe");
            let pidfile = dir.join("pid");
            let script = format!("sleep 30 & echo $! > {pidfile:?}; echo done");
            let started = Instant::now();
            let error = run(&mut sh(&script), Duration::from_millis(300)).expect_err("times out");
            assert!(matches!(error, RunError::TimedOut), "got {error:?}");
            assert!(
                started.elapsed() < Duration::from_secs(4),
                "returned after {:?}",
                started.elapsed()
            );
            let pid = std::fs::read_to_string(&pidfile).expect("the script wrote it");
            assert!(gone(pid.trim()), "process {} is still running", pid.trim());
        }
    }
```

- [ ] **Step 2: Run to see it fail.**
  `CARGO_BUILD_JOBS=6 cargo test outward::` — compile error: `run` and `RunError`
  not found.

- [ ] **Step 3: Implement.** Add to `src/outward.rs`, above the test module,
  these imports first (top of file, after the module comment):

```rust
use std::io::Read;
use std::process::{Child, Command, Output, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};
```

  then:

```rust
/// Why a run produced no output.
#[derive(Debug)]
pub enum RunError {
    /// The program could not be started, or its state could not be read.
    Start(std::io::Error),
    /// The program, or something it started, was still going at the bound. It
    /// has been stopped.
    TimedOut,
}

/// How often the child is looked at. Short enough that a bound of a fraction of a
/// second is met, long enough to cost nothing.
const POLL: Duration = Duration::from_millis(10);

/// The least a closed program's pipes are given to reach their end. A pipe that
/// is still open after that is held by something the program left behind.
const DRAIN: Duration = Duration::from_secs(1);

/// Reads `source` to its end on a thread of its own. Two of these, one per
/// stream, because a single reader blocks on one full pipe while the program
/// fills the other.
fn drain<R: Read + Send + 'static>(mut source: R) -> mpsc::Receiver<Vec<u8>> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let mut bytes = Vec::new();
        // What was read before an error is kept: it is all there is.
        let _ = source.read_to_end(&mut bytes);
        let _ = sender.send(bytes);
    });
    receiver
}

/// What a stream carried, or `None` when it did not end in `wait`.
fn collect(stream: Option<mpsc::Receiver<Vec<u8>>>, wait: Duration) -> Option<Vec<u8>> {
    match stream {
        None => Some(Vec::new()),
        Some(receiver) => receiver.recv_timeout(wait).ok(),
    }
}

/// Stops the child and everything it started, then reaps it.
///
/// The standard library cannot signal a process group and `libc` is not a
/// dependency, so on Unix the group is signalled by running `kill`. The child
/// is its own group leader (see `run`), so the group id is its process id. The
/// direct kill after it covers a machine where `kill` is not there.
fn stop(child: &mut Child) {
    #[cfg(unix)]
    {
        let _ = Command::new("kill")
            .arg("-s")
            .arg("KILL")
            .arg("--")
            .arg(format!("-{}", child.id()))
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    let _ = child.kill();
    let _ = child.wait();
}

/// Runs `command` and waits at most `bound` for it and for its output.
///
/// Standard input is closed, so a program that reads it ends rather than waiting
/// for a person. Standard output and error are captured, as `Command::output`
/// does; the difference is that this returns.
pub fn run(command: &mut Command, bound: Duration) -> Result<Output, RunError> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // Its own group, so a timeout reaches what a shell wrapper started.
        command.process_group(0);
    }
    let mut child = command.spawn().map_err(RunError::Start)?;
    let stdout = child.stdout.take().map(drain);
    let stderr = child.stderr.take().map(drain);

    let deadline = Instant::now() + bound;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => thread::sleep(POLL),
            Ok(None) => {
                stop(&mut child);
                return Err(RunError::TimedOut);
            }
            Err(error) => {
                stop(&mut child);
                return Err(RunError::Start(error));
            }
        }
    };

    let wait = deadline.saturating_duration_since(Instant::now()).max(DRAIN);
    match (collect(stdout, wait), collect(stderr, wait)) {
        (Some(stdout), Some(stderr)) => Ok(Output {
            status,
            stdout,
            stderr,
        }),
        _ => {
            stop(&mut child);
            Err(RunError::TimedOut)
        }
    }
}
```

  In `src/http_failure.rs:148` change `fn bound_text(bound: Duration) -> String {`
  to `pub(crate) fn bound_text(bound: Duration) -> String {`.

- [ ] **Step 4: Run to see it pass.** `CARGO_BUILD_JOBS=6 cargo test outward::` —
  all pass, the longest under about 2 seconds. If
  `a_timeout_kills_what_the_program_started_not_only_the_program` fails on macOS
  because `kill -s KILL -- -<pid>` is refused, read the error text from `kill` by
  running it by hand against a scratch `sleep` group and use the form the platform's
  `kill` accepts; the test is the arbiter, not the form.
  Add `#[allow(dead_code)]` on `run`, `RunError` for this commit only; Tasks 4 to 7
  remove it.

- [ ] **Step 5: Commit.**

```bash
git add src/outward.rs src/http_failure.rs
git commit -m "Add a bounded wait for a program that kills what it started (#28)"
```

---

### Task 3: the `command_timeout_seconds` key

**Files:**
- Modify: `src/config.rs` (struct `Stt` at line 55, `Default for Stt` at line 100,
  `load` at line 314, tests near line 662)

**Interfaces:**
- Produces: `Stt.command_timeout_seconds: u64` (default 60);
  `Stt::MIN_COMMAND_TIMEOUT_SECONDS: u64 = 1`; `load` raises a smaller value to it.

- [ ] **Step 1: Write the failing tests.** Next to
  `a_blink_interval_under_the_floor_is_raised_to_it_rather_than_obeyed` in
  `src/config.rs` add (`scratch` is the helper that test uses):

```rust
    #[test]
    fn the_transcriber_bound_defaults_to_sixty_seconds() {
        assert_eq!(Stt::default().command_timeout_seconds, 60);
        let directory = scratch("command-timeout-absent");
        std::fs::write(directory.join("config.toml"), "[stt]\nengine = \"command\"\n").unwrap();
        assert_eq!(load(Some(&directory)).config.stt.command_timeout_seconds, 60);
    }

    #[test]
    fn the_transcriber_bound_is_read_from_the_file() {
        let directory = scratch("command-timeout-set");
        std::fs::write(
            directory.join("config.toml"),
            "[stt]\ncommand_timeout_seconds = 300\n",
        )
        .unwrap();
        assert_eq!(load(Some(&directory)).config.stt.command_timeout_seconds, 300);
    }

    #[test]
    fn a_transcriber_bound_of_zero_is_raised_to_the_floor_rather_than_obeyed() {
        let directory = scratch("command-timeout-zero");
        std::fs::write(
            directory.join("config.toml"),
            "[stt]\ncommand_timeout_seconds = 0\n",
        )
        .unwrap();
        let loaded = load(Some(&directory));
        assert_eq!(
            loaded.config.stt.command_timeout_seconds,
            Stt::MIN_COMMAND_TIMEOUT_SECONDS
        );
        assert!(matches!(loaded.source, Source::File(_)), "the file still loads");
    }
```

- [ ] **Step 2: Run to see it fail.**
  `CARGO_BUILD_JOBS=6 cargo test config::` — compile error: no field.

- [ ] **Step 3: Implement.** In `struct Stt` add after `http_model`:

```rust
    /// How long `engine = "command"` lets the program run before it is stopped
    /// and the take is reported as failed. Not a cap on work somebody wants to
    /// wait for: it exists so a program that hangs becomes a message. Read through
    /// `load`, which raises anything under `MIN_COMMAND_TIMEOUT_SECONDS` to it.
    pub command_timeout_seconds: u64,
```

  Add an impl block after the struct:

```rust
impl Stt {
    /// A bound of zero would stop every program the moment it started.
    pub const MIN_COMMAND_TIMEOUT_SECONDS: u64 = 1;
}
```

  In `Default for Stt` add `command_timeout_seconds: 60,` after `http_model`
  (with a comment: sixty seconds is thirty-six times the longest take measured, in
  `docs/evidence.md`, and under the client's two-minute wait for `dictate`).
  In `load`, beside the `blink_ms` clamp add:

```rust
                config.stt.command_timeout_seconds = config
                    .stt
                    .command_timeout_seconds
                    .max(Stt::MIN_COMMAND_TIMEOUT_SECONDS);
```

- [ ] **Step 4: Run to see it pass.** `CARGO_BUILD_JOBS=6 cargo test config::`
  then `CARGO_BUILD_JOBS=6 cargo test` (other tests build `Stt` with
  `..Stt::default()`, so nothing else needs touching).

- [ ] **Step 5: Commit.**

```bash
git add src/config.rs
git commit -m "Add [stt] command_timeout_seconds with a default of sixty (#28)"
```

---

### Task 4: the transcriber command — bound, timeout message, safe cut

**Files:**
- Modify: `src/stt/command.rs` (imports, `CommandEngine` struct and `new` at
  lines 46-61, `CommandError` at 63-75 and its `Display`, `transcribe` at 111-168,
  tests)
- Modify: `src/stt.rs:203` (`resolve_with`)
- Modify: `src/outward.rs` (remove the `#[allow(dead_code)]` on `shorten`)

**Interfaces:**
- Consumes: `outward::run`, `outward::RunError`, `outward::shorten`,
  `http_failure::bound_text`, `config::Stt::command_timeout_seconds`.
- Produces:
  - `CommandEngine::with_bound(self, bound: Duration) -> CommandEngine`
  - `CommandError::TimedOut { program: String, bound: Duration }`
  - `pub const DEFAULT_BOUND: Duration` (60 s) in `src/stt/command.rs`

- [ ] **Step 1: Write the failing tests.** In `mod tests` of `src/stt/command.rs`
  (it has an `argv` helper and `use super::*;`) add:

```rust
    #[cfg(unix)]
    #[test]
    fn standard_error_with_a_two_byte_character_across_byte_400_is_cut_not_a_panic() {
        // 399 ASCII bytes, then "é" (0xC3 0xA9): the cut at 400 lands inside it.
        let engine = CommandEngine::new(
            argv(&[
                "sh",
                "-c",
                "head -c 399 /dev/zero | tr '\\0' a >&2; printf '\\303\\251' >&2; exit 1",
            ]),
            None,
            "auto".to_string(),
        );
        let error = engine
            .transcribe(Path::new("/takes/one.wav"), "")
            .expect_err("must fail");
        let message = error.to_string();
        assert!(message.contains("exit 1"), "got {message}");
        assert!(message.contains(&"a".repeat(399)), "got {message}");
        assert!(!message.contains('é'), "the straddling character is dropped whole");
    }

    #[cfg(unix)]
    #[test]
    fn standard_error_with_a_three_byte_character_across_byte_400_is_cut_not_a_panic() {
        // "—" is 0xE2 0x80 0x94, starting at byte 398.
        let engine = CommandEngine::new(
            argv(&[
                "sh",
                "-c",
                "head -c 398 /dev/zero | tr '\\0' a >&2; printf '\\342\\200\\224' >&2; exit 1",
            ]),
            None,
            "auto".to_string(),
        );
        let message = engine
            .transcribe(Path::new("/takes/one.wav"), "")
            .expect_err("must fail")
            .to_string();
        assert!(message.contains(&"a".repeat(398)), "got {message}");
        assert!(!message.contains('—'));
    }

    #[cfg(unix)]
    #[test]
    fn a_program_that_outlasts_the_bound_is_named_with_the_bound_and_the_key() {
        let engine = CommandEngine::new(argv(&["sh", "-c", "sleep 30"]), None, "auto".to_string())
            .with_bound(std::time::Duration::from_millis(200));
        let started = std::time::Instant::now();
        let error = engine
            .transcribe(Path::new("/takes/one.wav"), "")
            .expect_err("must fail");
        assert!(started.elapsed() < std::time::Duration::from_secs(3));
        let message = error.to_string();
        assert!(message.contains("\"sh\""), "names the program: {message}");
        assert!(message.contains("200 milliseconds"), "names the bound: {message}");
        assert!(
            message.contains("[stt] command_timeout_seconds"),
            "says what to change: {message}"
        );
        assert!(!message.contains("daemon"), "does not blame the daemon: {message}");
    }

    #[test]
    fn the_default_bound_is_sixty_seconds() {
        assert_eq!(DEFAULT_BOUND, std::time::Duration::from_secs(60));
    }
```

- [ ] **Step 2: Run to see it fail.**
  `CARGO_BUILD_JOBS=6 cargo test stt::command::` — compile errors (`with_bound`,
  `DEFAULT_BOUND`), and once those exist the first two tests panic at
  `stderr.truncate`.

- [ ] **Step 3: Implement.**
  1. Replace `use std::process::Command;` with `use std::process::Command;` plus
     `use std::time::Duration;` and `use crate::outward::{self, RunError};`.
  2. Above `CommandEngine`:

```rust
/// How long the program may run when `[stt] command_timeout_seconds` says
/// nothing. See `docs/decisions.md`, the entry for the transcriber command.
pub const DEFAULT_BOUND: Duration = Duration::from_secs(60);
```

  3. Add `bound: Duration,` to `struct CommandEngine`; `new` sets
     `bound: DEFAULT_BOUND`; add:

```rust
    /// The bound from `[stt] command_timeout_seconds`.
    pub fn with_bound(mut self, bound: Duration) -> CommandEngine {
        self.bound = bound;
        self
    }
```

  4. Add to `CommandError`: `TimedOut { program: String, bound: Duration },` and
     to its `Display`:

```rust
            CommandError::TimedOut { program, bound } => write!(
                f,
                "{program:?} did not finish within {}, so it was stopped. A long take can \
                 need more: raise [stt] command_timeout_seconds, or run the program by hand \
                 on the take to see where it stops",
                crate::http_failure::bound_text(*bound)
            ),
```

  5. In `transcribe`, replace
     `let output = Command::new(program).args(arguments).output();` and the
     `match output { Ok(output) => output, Err(e) if …` block so that the call is
     `outward::run(Command::new(program).args(arguments), self.bound)` and the arms
     become: `Ok(output) => output`; `Err(RunError::TimedOut) => return
     Err(EngineError::Command(CommandError::TimedOut { program: program.clone(),
     bound: self.bound }))`; `Err(RunError::Start(e)) if e.kind() == NotFound =>`
     (existing NotFound body); `Err(RunError::Start(e)) =>` (existing "could not
     start" body). `Command::new(program).args(arguments)` is a temporary; bind it:
     `let mut command = Command::new(program); command.args(arguments);` then
     `outward::run(&mut command, self.bound)`.
  6. Replace
     `let mut stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
     stderr.truncate(STDERR_LIMIT);` with
     `let mut stderr = outward::shorten(String::from_utf8_lossy(&output.stderr).trim(), STDERR_LIMIT);`.
  7. In `src/stt.rs` `resolve_with`, change the `Ready::Command` arm to build
     `command::CommandEngine::new(stt.command.clone(), model, stt.language.clone())
     .with_bound(std::time::Duration::from_secs(stt.command_timeout_seconds))`.
  8. In `src/outward.rs` remove the `#[allow(dead_code)]` on `shorten` only;
     the ones on `run` and `RunError` stay until Task 7.

- [ ] **Step 4: Run to see it pass.**
  `CARGO_BUILD_JOBS=6 cargo test stt::` then `CARGO_BUILD_JOBS=6 cargo test`.
  The existing `a_program_that_fails_carries_what_it_complained_about` and
  `a_nonexistent_program_…` tests must still pass unchanged.

- [ ] **Step 5: Commit.**

```bash
git add src/stt/command.rs src/stt.rs src/outward.rs
git commit -m "Bound the transcriber command and cut its error on a character boundary (#28, #94)"
```

---

### Task 5: the rewrite command — bound, timeout message, safe cut

**Files:**
- Modify: `src/rewrite/command.rs` (imports, struct and `new` at 33-41, `CommandError`
  and `Display`, `rewrite` at 90-135, tests)

**Interfaces:**
- Consumes: `outward::run`, `outward::RunError`, `outward::shorten`,
  `http_failure::bound_text`.
- Produces: `CommandError::TimedOut { program: String, bound: Duration }`,
  `pub const BOUND: Duration` (30 s), and, for tests only,
  `#[cfg(test)] pub(crate) fn with_bound(self, bound: Duration) -> CommandEngine`
  (crate-visible because the daemon tests in Task 8 use it).

- [ ] **Step 1: Write the failing tests** in `mod tests` of
  `src/rewrite/command.rs`:

```rust
    #[cfg(unix)]
    #[test]
    fn standard_error_with_a_two_byte_character_across_byte_400_is_cut_not_a_panic() {
        let engine = CommandEngine::new(argv(&[
            "sh",
            "-c",
            "head -c 399 /dev/zero | tr '\\0' a >&2; printf '\\303\\251' >&2; exit 1",
        ]));
        let message = engine
            .rewrite("hello there", "")
            .expect_err("must fail")
            .to_string();
        assert!(message.contains("exit 1"), "got {message}");
        assert!(message.contains(&"a".repeat(399)), "got {message}");
        assert!(!message.contains('é'));
    }

    #[cfg(unix)]
    #[test]
    fn standard_error_with_a_four_byte_character_across_byte_400_is_cut_not_a_panic() {
        // 397 bytes, then a four-byte character (0xF0 0x9F 0x98 0x80).
        let engine = CommandEngine::new(argv(&[
            "sh",
            "-c",
            "head -c 397 /dev/zero | tr '\\0' a >&2; printf '\\360\\237\\230\\200' >&2; exit 1",
        ]));
        let message = engine
            .rewrite("hello there", "")
            .expect_err("must fail")
            .to_string();
        assert!(message.contains(&"a".repeat(397)), "got {message}");
        assert!(!message.contains('😀'));
    }

    #[cfg(unix)]
    #[test]
    fn a_program_that_outlasts_the_bound_is_named_with_the_bound() {
        let engine = CommandEngine::new(argv(&["sh", "-c", "sleep 30"]))
            .with_bound(std::time::Duration::from_millis(200));
        let started = std::time::Instant::now();
        let error = engine.rewrite("hello there", "").expect_err("must fail");
        assert!(started.elapsed() < std::time::Duration::from_secs(3));
        let message = error.to_string();
        assert!(message.contains("\"sh\""), "names the program: {message}");
        assert!(message.contains("200 milliseconds"), "names the bound: {message}");
        assert!(
            message.contains("by hand"),
            "says what to do next: {message}"
        );
        assert!(!message.contains("daemon"), "{message}");
    }

    #[test]
    fn the_bound_is_thirty_seconds() {
        assert_eq!(BOUND, std::time::Duration::from_secs(30));
    }
```

- [ ] **Step 2: Run to see it fail.**
  `CARGO_BUILD_JOBS=6 cargo test rewrite::command::` — compile errors, then the two
  cut tests panic.

- [ ] **Step 3: Implement.** Mirror Task 4 in `src/rewrite/command.rs`:
  imports `std::time::Duration` and `crate::outward::{self, RunError}`;
  `pub const BOUND: Duration = Duration::from_secs(30);` with a comment that it is
  the bound `src/rewrite/http.rs` already uses; `bound: Duration` on the struct,
  `new` sets `BOUND`;

```rust
    #[cfg(test)]
    pub(crate) fn with_bound(mut self, bound: Duration) -> CommandEngine {
        self.bound = bound;
        self
    }
```

  `CommandError::TimedOut { program: String, bound: Duration }` with `Display`:

```rust
            CommandError::TimedOut { program, bound } => write!(
                f,
                "{program:?} did not finish within {}, so it was stopped and the transcript \
                 was delivered unrewritten. Run the command by hand on a transcript to see \
                 where it stops",
                crate::http_failure::bound_text(*bound)
            ),
```

  `rewrite` runs `outward::run(&mut command, self.bound)` with the same four arms
  as Task 4 (`Ok`, `TimedOut`, `Start` NotFound, `Start` other), and the stderr
  line becomes
  `outward::shorten(String::from_utf8_lossy(&output.stderr).trim(), STDERR_LIMIT)`.
  Check every `match` over this `CommandError` elsewhere (`grep -rn "CommandError"
  src/rewrite.rs src/daemon.rs`) and add the arm where one is exhaustive.

- [ ] **Step 4: Run to see it pass.**
  `CARGO_BUILD_JOBS=6 cargo test rewrite::` then the full `cargo test`.

- [ ] **Step 5: Commit.**

```bash
git add src/rewrite/command.rs
git commit -m "Bound the rewrite command and cut its error on a character boundary (#28, #94)"
```

---

### Task 6: delivery's herdr calls

**Files:**
- Modify: `src/delivery.rs` (`DeliveryError` at lines 5-44, `HerdrDeliverer` at
  199-232, `tests_support` at 80+, tests at 520+)

**Interfaces:**
- Consumes: `outward::run`, `outward::RunError`, `http_failure::bound_text`.
- Produces:
  - `DeliveryError::TimedOut { binary: String, bound: Duration }`
  - `pub const BOUND: Duration` (10 s) in `src/delivery.rs`
  - `#[cfg(test)] pub(crate) HerdrDeliverer::with_bound(self, bound: Duration) -> Self`
    (crate-visible because the daemon and pane tests use it)
  - `#[cfg(all(test, unix))] pub fn tests_support::herdr_that_hangs_on(tag: &str, subcommands: &[&str]) -> String`
    — returns the path of a script that sleeps 30 seconds when its first argument
    is in `subcommands` (use `"*"` for every call) and exits 0 otherwise.
    (Used by Task 8.)

- [ ] **Step 1: Write the failing tests** in `mod tests` of `src/delivery.rs`
  (it already has the recorder helper; the new tests use the new helper
  `tests_support::herdr_that_hangs_on`, written in Step 3):

```rust
    #[cfg(unix)]
    #[test]
    fn a_herdr_that_never_answers_is_stopped_and_named() {
        let binary = tests_support::herdr_that_hangs_on("hang-insert", &["pane"]);
        let deliverer =
            HerdrDeliverer::with_binary(binary.clone()).with_bound(Duration::from_millis(200));
        let started = std::time::Instant::now();
        let error = deliverer.insert("w1:p2", "hello").expect_err("must fail");
        assert!(started.elapsed() < Duration::from_secs(3));
        assert_eq!(
            error,
            DeliveryError::TimedOut {
                binary: binary.clone(),
                bound: Duration::from_millis(200)
            }
        );
        let message = error.to_string();
        assert!(message.contains(&binary), "names herdr: {message}");
        assert!(message.contains("200 milliseconds"), "names the bound: {message}");
        assert!(message.contains("restart"), "says what to do next: {message}");
        assert!(!message.contains("daemon"), "{message}");
    }

    #[cfg(unix)]
    #[test]
    fn each_of_the_three_subcommands_is_bounded() {
        let binary = tests_support::herdr_that_hangs_on("hang-all", &["*"]);
        let deliverer =
            HerdrDeliverer::with_binary(binary).with_bound(Duration::from_millis(200));
        for result in [
            deliverer.insert("w1:p2", "hello"),
            deliverer.submit("w1:p2", "hello"),
            deliverer.notify("title", "body"),
        ] {
            assert!(matches!(result, Err(DeliveryError::TimedOut { .. })), "{result:?}");
        }
    }

    #[test]
    fn the_bound_is_ten_seconds() {
        assert_eq!(BOUND, Duration::from_secs(10));
        assert_eq!(HerdrDeliverer::with_binary("herdr").bound, BOUND);
    }
```

  Add `use std::time::Duration;` to the test module if it is not there.

- [ ] **Step 2: Run to see it fail.**
  `CARGO_BUILD_JOBS=6 cargo test delivery::` — compile errors.

- [ ] **Step 3: Implement.**
  1. `use std::time::Duration;` at the top of the file. Add above
     `HerdrDeliverer`:

```rust
/// How long one herdr call may take. `docs/decisions.md`, the entry for
/// delivery's herdr calls.
pub const BOUND: Duration = Duration::from_secs(10);
```

  2. `DeliveryError`: add `TimedOut { binary: String, bound: Duration },` with doc
     comment "herdr was started and did not finish; it has been stopped." and its
     `Display` arm:

```rust
            DeliveryError::TimedOut { binary, bound } => write!(
                f,
                "{binary:?} did not answer within {}, so the plugin stopped it. If herdr is \
                 not responding, restart it, then dictate again",
                crate::http_failure::bound_text(*bound)
            ),
```

     The enum already derives `PartialEq` and `Eq` (a test compares errors);
     `Duration` supports both.
  3. `HerdrDeliverer` gets `bound: Duration`, set to `BOUND` in `with_binary`, and

```rust
    #[cfg(test)]
    pub(crate) fn with_bound(mut self, bound: Duration) -> Self {
        self.bound = bound;
        self
    }
```

  4. `run` becomes:

```rust
    fn run(&self, args: &[&str]) -> Result<(), DeliveryError> {
        let mut command = std::process::Command::new(&self.binary);
        command.args(args);
        match crate::outward::run(&mut command, self.bound) {
            Err(crate::outward::RunError::TimedOut) => Err(DeliveryError::TimedOut {
                binary: self.binary.clone(),
                bound: self.bound,
            }),
            // herdr starts plugin commands with a minimal PATH — the same
            // reasoning src/stt/command.rs states for the transcriber.
            Err(crate::outward::RunError::Start(error)) => {
                Err(start_failure(&self.binary, &error))
            }
            Ok(output) if output.status.success() => Ok(()),
            Ok(output) => {
                let text = if !output.stdout.is_empty() {
                    &output.stdout
                } else {
                    &output.stderr
                };
                Err(DeliveryError::Rejected(extract_reason(text)))
            }
        }
    }
```

     (Keep the existing comment text the original arm carried.)
  5. In `pub mod tests_support` add:

```rust
    /// A script that sleeps thirty seconds when its first argument is one of
    /// `subcommands` (`"*"` means any), and exits 0 otherwise. Written the way
    /// the recorder in this file's tests is: closed before it is run.
    #[cfg(unix)]
    pub fn herdr_that_hangs_on(tag: &str, subcommands: &[&str]) -> String {
        let dir = std::env::temp_dir().join(format!(
            "herdr-voice-hang-{tag}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create scratch");
        let script = dir.join("herdr.sh");
        let patterns = subcommands.join("|");
        std::fs::write(
            &script,
            format!("#!/bin/sh\ncase \"$1\" in {patterns}) sleep 30;; esac\nexit 0\n"),
        )
        .expect("write script");
        let mut permissions = std::fs::metadata(&script).unwrap().permissions();
        std::os::unix::fs::PermissionsExt::set_mode(&mut permissions, 0o755);
        std::fs::set_permissions(&script, permissions).expect("chmod script");
        script.to_string_lossy().into_owned()
    }
```

     (`case "$1" in *)` matches anything, which is what `"*"` means.)
     Because `"pane"` is the first argument of `pane send-text`, the first test's
     `["pane"]` hangs insert only; `notification` and `agent` exit 0.
  6. Add an arm for `TimedOut` anywhere `DeliveryError` is matched exhaustively
     (`grep -rn "DeliveryError::" src`); the compiler lists them.

- [ ] **Step 4: Run to see it pass.**
  `CARGO_BUILD_JOBS=6 cargo test delivery::` then the full `cargo test`.

- [ ] **Step 5: Commit.**

```bash
git add src/delivery.rs
git commit -m "Bound delivery's herdr calls and name the one that wedged (#28)"
```

---

### Task 7: the pane read

**Files:**
- Modify: `src/bias/pane.rs` (`PaneError` at 33-60, `read` at 58-88, tests)
- Modify: `src/outward.rs` (remove the remaining `#[allow(dead_code)]` on `run`
  and `RunError`)

**Interfaces:**
- Consumes: `outward::run`, `outward::RunError`, `http_failure::bound_text`.
- Produces: `pub const BOUND: Duration` (5 s);
  `pub fn read_within(pane, lines, binary, bound) -> Result<String, PaneError>`;
  `PaneError::TimedOut { program: String, bound: Duration }`. `read` keeps its
  signature and calls `read_within(…, BOUND)`, so `src/bias.rs:93` is unchanged.

- [ ] **Step 1: Write the failing tests** in `mod tests` of `src/bias/pane.rs`:

```rust
    #[cfg(unix)]
    #[test]
    fn a_herdr_that_never_answers_is_a_timeout_naming_it_and_the_bound() {
        let binary = crate::delivery::tests_support::herdr_that_hangs_on("pane-hang", &["pane"]);
        let started = std::time::Instant::now();
        let error = read_within(
            "w1:p2",
            80,
            &binary,
            std::time::Duration::from_millis(200),
        )
        .expect_err("must fail");
        assert!(started.elapsed() < std::time::Duration::from_secs(3));
        match &error {
            PaneError::TimedOut { program, bound } => {
                assert_eq!(program, &binary);
                assert_eq!(*bound, std::time::Duration::from_millis(200));
            }
            other => panic!("expected TimedOut, got {other:?}"),
        }
        let text = error.to_string();
        assert!(text.contains(&binary), "{text}");
        assert!(text.contains("200 milliseconds"), "{text}");
        assert!(
            text.contains("without the pane"),
            "says what the take does next: {text}"
        );
    }

    #[test]
    fn the_bound_is_five_seconds() {
        assert_eq!(BOUND, std::time::Duration::from_secs(5));
    }
```

- [ ] **Step 2: Run to see it fail.**
  `CARGO_BUILD_JOBS=6 cargo test bias::pane::` — compile errors.

- [ ] **Step 3: Implement.** `use std::time::Duration;` and
  `use crate::outward::{self, RunError};`. Add

```rust
/// How long `herdr pane read` may take. The take is waiting on it before
/// recognition starts, and a miss costs only the bias, so it is shorter than
/// delivery's bound. `docs/decisions.md`, the entry for the pane read.
pub const BOUND: Duration = Duration::from_secs(5);
```

  `PaneError::TimedOut { program: String, bound: Duration }` with `Display`:

```rust
            PaneError::TimedOut { program, bound } => write!(
                f,
                "\"{program}\" did not answer within {}, so the plugin stopped it and went on \
                 without the pane's text. If herdr is not responding, restart it",
                crate::http_failure::bound_text(*bound)
            ),
```

  Rename the body of `read` to `read_within` with a fourth parameter
  `bound: Duration`, build the command with
  `let mut command = Command::new(binary); command.args(arguments);`, call
  `outward::run(&mut command, bound)` and map: `Err(RunError::TimedOut) =>
  Err(PaneError::TimedOut { program: binary.to_string(), bound })`;
  `Err(RunError::Start(_)) => Err(PaneError::NotFound { … })` (the existing arm).
  Then

```rust
pub fn read(pane: &str, lines: usize, binary: &str) -> Result<String, PaneError> {
    read_within(pane, lines, binary, BOUND)
}
```

  `read_within` is `pub` (used by `read`; tests call it) so there is no dead code.
  Remove the `#[allow(dead_code)]` markers left on `run` and `RunError`. Add an
  arm for `TimedOut` wherever `PaneError` is matched (the compiler lists them).

- [ ] **Step 4: Run to see it pass.**
  `CARGO_BUILD_JOBS=6 cargo test bias::` then the full `cargo test`, then
  `CARGO_BUILD_JOBS=6 cargo clippy --all-targets -- -D warnings` (no
  `#[allow(dead_code)]` should remain in `src/outward.rs`).

- [ ] **Step 5: Commit.**

```bash
git add src/bias/pane.rs src/outward.rs
git commit -m "Bound the pane read so a wedged herdr cannot hold up recognition (#28)"
```

---

### Task 8: what the person gets, on the toggle path and the hold path

**Files:**
- Modify: `src/daemon.rs` — tests only, in `mod tests`, after
  `a_delivery_failure_during_a_hold_is_reported_once_and_not_twice` (line 3796) and
  after `ac4_a_real_transcriber_that_writes_two_lines_to_standard_error_shows_both`.
  No production change is expected. If a test shows one is needed, stop and report
  it to the orchestrator: it means the design missed something.

**Interfaces:**
- Consumes: `HerdrDeliverer::with_binary(..).with_bound(..)` (Task 6),
  `tests_support::herdr_that_hangs_on` (Task 6), `CommandEngine::with_bound`
  (Tasks 4 and 5), the daemon test helpers `runtime_with_clock`, `runtime_reading_back`,
  `TestJournal`, `RecordingJournal`, `tone_recorder`, `request`, `dictate_request`,
  `answer`, `wait_for_journal`, `wait_for_idle`, `WITHIN`, `PANE_1` and
  `two_presses_over_a_socket`, all already in `mod tests`.
- Produces: tests only.

- [ ] **Step 1: Write the tests.** All are `#[cfg(unix)]`.

  **(a) Toggle path, delivery (AC-6).**

```rust
    #[cfg(unix)]
    #[test]
    fn a_delivery_that_times_out_on_dictate_is_journaled_and_names_herdr_not_the_daemon() {
        let binary = crate::delivery::tests_support::herdr_that_hangs_on("toggle-hang", &["pane"]);
        let (mut runtime, journal) = runtime_reading_back(
            crate::delivery::tests_support::FakeDeliverer::ok(),
            false,
        );
        runtime.deliverer = Box::new(
            crate::delivery::HerdrDeliverer::with_binary(binary.clone())
                .with_bound(std::time::Duration::from_millis(200)),
        );
        let recorder = tone_recorder("toggle-hang");
        let request = dictate_request();
        answer(&request, &recorder, &runtime);
        let (reply, _) = answer(&request, &recorder, &runtime);
        let message = match reply {
            Reply::Error(message) => message,
            other => panic!("a wedged herdr is an error reply, got {other:?}"),
        };
        assert!(message.contains(&binary), "names herdr: {message}");
        assert!(message.contains("did not answer within 200 milliseconds"), "{message}");
        assert!(!message.contains("the daemon did not answer"), "{message}");
        let lines = journalled(&journal);
        assert!(
            lines.iter().any(|line| line.starts_with("delivery failed:")
                && line.contains("did not answer within 200 milliseconds")),
            "the delivery-failed line is written: {lines:?}"
        );
    }
```

  (`answer` returns a pair whose first element is the reply: confirm the exact
  shape at its definition and use the matching destructuring; the existing test
  `the_delivering_line_is_written_before_delivery_is_attempted…` shows the call
  form.)

  **(b) Toggle path, transcriber (AC-7).**

```rust
    #[cfg(unix)]
    #[test]
    fn a_transcriber_that_times_out_on_dictate_names_the_program_not_the_daemon() {
        let mut runtime = fake_runtime("unused");
        runtime.recognition = Ok(Box::new(
            crate::stt::command::CommandEngine::new(
                vec!["sh".into(), "-c".into(), "sleep 30".into()],
                None,
                "en".into(),
            )
            .with_bound(std::time::Duration::from_millis(200)),
        ));
        let (_, second) = two_presses_over_a_socket("transcriber-timeout", runtime);
        assert_eq!(second.code, 1, "{second:?}");
        let message = second.message.expect("a failure says why");
        assert!(message.contains("\"sh\" did not finish within 200 milliseconds"), "{message}");
        assert!(message.contains("command_timeout_seconds"), "{message}");
        assert!(!message.contains("the daemon did not answer"), "{message}");
        let path = kept_path(&message);
        assert!(std::path::Path::new(path).exists(), "the take is kept at {path:?}");
        std::fs::remove_file(path).ok();
    }
```

  **(c) Toggle path, rewrite (AC-7).** A rewrite that times out delivers the
  transcript unrewritten and writes the once-only notice to the journal
  (`tell_once` writes `rewrite_unavailable_line(why)`):

```rust
    #[cfg(unix)]
    #[test]
    fn a_rewrite_that_times_out_delivers_the_transcript_and_says_which_program() {
        let fake = crate::delivery::tests_support::FakeDeliverer::ok();
        let (mut runtime, journal) = runtime_reading_back(fake.clone(), false);
        runtime.rewrite = crate::rewrite::Resolution::Engine(Box::new(
            crate::rewrite::command::CommandEngine::new(vec![
                "sh".into(),
                "-c".into(),
                "sleep 30".into(),
            ])
            .with_bound(std::time::Duration::from_millis(200)),
        ));
        runtime.skip_if_plain = false;
        let recorder = tone_recorder("rewrite-timeout");
        let request = dictate_request();
        answer(&request, &recorder, &runtime);
        answer(&request, &recorder, &runtime);
        assert!(
            fake.calls().iter().any(|call| matches!(
                call,
                crate::delivery::tests_support::Call::Insert(_, text)
                    if text == "fix the worklog entry"
            )),
            "the transcript is delivered unrewritten: {:?}",
            fake.calls()
        );
        let lines = journalled(&journal);
        assert!(
            lines.iter().any(|line| line.contains("\"sh\" did not finish within 200 milliseconds")),
            "{lines:?}"
        );
    }
```

  **(d) and (e) Hold path (AC-14).** One helper runs a hold over a herdr that
  hangs on the given subcommands, with a 200-millisecond delivery bound and toasts
  on, and returns what the test asserts on. It follows the watcher setup of
  `a_delivery_failure_during_a_hold_is_reported_once_and_not_twice`:

```rust
    /// Returns the journal, how long the watcher took from the release to the
    /// hold being idle again, and whether a following `ptt` was accepted.
    #[cfg(unix)]
    fn a_hold_over_a_wedged_herdr(
        tag: &str,
        wedged: &[&str],
    ) -> (Vec<String>, std::time::Duration, bool) {
        let binary = crate::delivery::tests_support::herdr_that_hangs_on(tag, wedged);
        let (mut runtime, clock) =
            runtime_with_clock(crate::delivery::tests_support::FakeDeliverer::ok(), false);
        runtime.deliverer = Box::new(
            crate::delivery::HerdrDeliverer::with_binary(binary)
                .with_bound(std::time::Duration::from_millis(200)),
        );
        runtime.delivery_settings.toasts = true;
        let journal = std::sync::Arc::new(RecordingJournal::default());
        runtime.journal = Box::new(TestJournal(std::sync::Arc::clone(&journal)));
        let runtime = std::sync::Arc::new(runtime);
        let recorder = std::sync::Arc::new(tone_recorder(tag));
        let stop = std::sync::Arc::new(AtomicBool::new(false));
        let watcher = {
            let recorder = std::sync::Arc::clone(&recorder);
            let runtime = std::sync::Arc::clone(&runtime);
            let stop = std::sync::Arc::clone(&stop);
            thread::spawn(move || watch(recorder, runtime, stop))
        };

        answer(&request("ptt", PANE_1), &recorder, &runtime);
        clock.advance(400);
        answer(&request("ptt", PANE_1), &recorder, &runtime);
        let released = std::time::Instant::now();
        clock.advance(1_000);

        wait_for_journal(&journal, "delivery failed", WITHIN);
        wait_for_idle(&runtime);
        let took = released.elapsed();

        // The hold is over, so a press starts a new one rather than being
        // refused with "still being transcribed".
        let (reply, _) = answer(&request("ptt", PANE_1), &recorder, &runtime);
        let accepted = !matches!(
            &reply,
            Reply::Error(message) if message.contains("still being transcribed")
        );

        stop.store(true, Ordering::SeqCst);
        runtime.clock.wake();
        clock.advance(1);
        watcher.join().unwrap();
        let lines = journal.0.lock().unwrap().clone();
        (lines, took, accepted)
    }

    #[cfg(unix)]
    #[test]
    fn a_hold_whose_delivery_times_out_ends_and_is_reported_once_naming_herdr() {
        let (lines, took, accepted) = a_hold_over_a_wedged_herdr("hold-hang", &["pane"]);
        let failures: Vec<&String> = lines
            .iter()
            .filter(|line| line.starts_with("delivery failed:"))
            .collect();
        assert_eq!(failures.len(), 1, "one failure, one line: {lines:?}");
        assert!(
            failures[0].contains("did not answer within 200 milliseconds"),
            "{}",
            failures[0]
        );
        assert!(accepted, "the next press is not refused");
        assert!(took < std::time::Duration::from_secs(5), "took {took:?}");
        assert!(
            !lines.iter().any(|line| line.starts_with("toast failed:")),
            "only the delivery was wedged, so the toast went out: {lines:?}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_hold_over_a_herdr_that_answers_nothing_still_ends_within_two_bounds() {
        let (lines, took, accepted) = a_hold_over_a_wedged_herdr("hold-hang-all", &["*"]);
        assert!(
            lines.iter().any(|line| line.starts_with("delivery failed:")),
            "{lines:?}"
        );
        assert!(
            lines.iter().any(|line| line.starts_with("toast failed:")
                && line.contains("did not answer within 200 milliseconds")),
            "the toast goes through the same herdr and its failure is journaled: {lines:?}"
        );
        assert!(accepted, "the next press is not refused");
        assert!(took < std::time::Duration::from_secs(5), "took {took:?}");
    }
```

- [ ] **Step 2: Run to see them pass or fail for the right reason.**
  `CARGO_BUILD_JOBS=6 cargo test daemon::tests::` (the whole module, one cargo
  process). They should pass as the sites already carry the behaviour; a failure is
  information about the design, not a reason to weaken the test.

- [ ] **Step 3: Run the daemon module three times** to catch timing flakiness:
  `for i in 1 2 3; do CARGO_BUILD_JOBS=6 cargo test daemon:: || break; done`.

- [ ] **Step 4: Commit.**

```bash
git add src/daemon.rs
git commit -m "Test that a wedged herdr, transcriber or rewrite reaches the person by name (#28)"
```

---

### Task 9: no `truncate` on outside text remains

**Files:**
- Modify: `src/outward.rs` (tests)

**Interfaces:** consumes the two replaced sites from Tasks 4 and 5; produces a test.

- [ ] **Step 1: Write the test.** In `mod tests` of `src/outward.rs`:

```rust
    /// No `truncate` on text from outside the process may come back (issue #94).
    /// The needle is assembled so this file does not match itself. The one
    /// allowed use cuts a byte vector a test fixture owns.
    #[test]
    fn no_source_file_cuts_a_string_with_truncate() {
        let needle = [".trunc", "ate("].concat();
        let allowed = [("stt/fetch.rs", "files[0].2.truncate(8);")];
        let mut found = Vec::new();
        let mut pending = vec![std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src")];
        while let Some(directory) = pending.pop() {
            for entry in std::fs::read_dir(&directory).expect("read src") {
                let path = entry.expect("entry").path();
                if path.is_dir() {
                    pending.push(path);
                } else if path.extension().is_some_and(|e| e == "rs") {
                    let text = std::fs::read_to_string(&path).expect("read source");
                    for (index, line) in text.lines().enumerate() {
                        if line.contains(&needle)
                            && !allowed.iter().any(|(tail, code)| {
                                path.ends_with(tail) && line.trim() == *code
                            })
                        {
                            found.push(format!("{}:{}", path.display(), index + 1));
                        }
                    }
                }
            }
        }
        assert!(found.is_empty(), "cut with a truncate: {found:?}");
    }
```

  The allowed line is `src/stt/fetch.rs:415`, a `Vec<u8>` in a fixture; confirm
  that before relying on it: `sed -n 410,416p src/stt/fetch.rs`.

- [ ] **Step 2: Run.** `CARGO_BUILD_JOBS=6 cargo test no_source_file` — passes
  after Tasks 4 and 5. To see it fail once (proves it can), temporarily add a line
  containing the needle to any other source file, run, then remove the line;
  `git diff --stat` must show no change outside `src/outward.rs` afterwards.

- [ ] **Step 3: Commit.**

```bash
git add src/outward.rs
git commit -m "Fail the build if a string from outside is cut with truncate (#94)"
```

---

### Task 10: the decisions and the key, in the documents

**Files:**
- Modify: `docs/decisions.md` (append four rows to the table, after the last row)
- Modify: `docs/design.md` (section 4, section 7)

- [ ] **Step 1: `docs/decisions.md`.** Append four rows in the table's own format
  `| Decision | Basis | Where |`, each with `2026-10-06, #28` in the third column.
  The basis column states, in this order and in one cell: the context, the problem,
  and the reason (the decision is in the first column). Content — taken from
  `tasks/28/DESIGN_28.md` section 3, which is the source; do not reword the numbers:
  1. **Delivery's herdr calls (`pane send-text`, `agent prompt`, `notification
     show`) are each stopped after 10 seconds and reported as herdr not answering;
     the bound is not a configuration key.**
  2. **The pane read is stopped after 5 seconds and the take goes on with file
     names alone; the bound is not a configuration key.**
  3. **The transcriber command is stopped after 60 seconds by default, raised by
     `[stt] command_timeout_seconds`; a timeout names the program, the bound and the
     key.**
  4. **The rewrite command is stopped after 30 seconds, the bound the HTTP rewrite
     engine already has; the transcript is delivered unrewritten.**

  Write the cells in English, one paragraph each, each naming: what is at the
  call site, why "no bound" is not an option (a wedged call holds a connection
  thread on `dictate` and the single watcher on a hold, which keeps the hold in
  `Ending` so every later press is refused and shutdown waits for ever), and the
  figure the number rests on (measured times in `docs/evidence.md`; the sum
  5 + 60 + 30 + 10 = 105 seconds, 115 with a toast, under the client's 120).
  Add one fifth row: **A program that outlasts its bound is killed with the group
  it started, by running `kill`, and `libc` is not added** — basis: the documented
  transcriber command is `sh -c "…"`, so killing only the shell leaves the program
  running; the standard library has no group signal; a crate for one call is what
  the third row of this table's rule against dependencies keeps out.
  No row refers to who decided it or when it was discussed; the third column
  carries only the date and issue number, as every other row does.
  AC-2 is met by rows 1 and 3 differing in policy and each saying why.

- [ ] **Step 2: `docs/design.md`.**
  - In the TOML block of section 7, under `[stt]`, add
    `command_timeout_seconds = 60  # stop a command transcriber after this long`.
  - After the paragraph on `blink_ms` in section 7, add a paragraph: the key is
    read only by `engine = "command"`; a value under 1 is raised to 1; a transcriber
    that outlasts it is stopped, with the program it started, and the take is
    reported failed naming the program, the bound and this key.
  - Before `## 5. Push-to-talk`, add `### Bounds on outward calls`: a four-row
    table (call, bound, what the person sees) and two sentences saying that on the
    `dictate` path the bounds add to under the client's 120 seconds, and that on a
    hold the watcher is back within the same sum and the report is the journal line
    (the toast goes through herdr and is best effort when herdr is the one wedged).

- [ ] **Step 3: Read both files** for the four-part content, for any absolute
  path, any name of a person, employer or machine, and for each number against
  the constants in `src/delivery.rs`, `src/bias/pane.rs`, `src/stt/command.rs`,
  `src/rewrite/command.rs` and `src/config.rs`.

- [ ] **Step 4: Commit.**

```bash
git add docs/decisions.md docs/design.md
git commit -m "Record the bound for each outward call and the new key (#28)"
```

---

### Task 11: gates

**Files:** none changed unless a gate fails.

- [ ] **Step 1: The four gates, fresh, one at a time:**

```bash
CARGO_BUILD_JOBS=6 cargo test
CARGO_BUILD_JOBS=6 cargo clippy --all-targets -- -D warnings
cargo fmt --check
python3 scripts/check_manifest.py
```

  `cargo fmt --check` will likely want formatting of the new code: run
  `cargo fmt`, read the diff, commit as `Format`.

- [ ] **Step 2: The Windows dead-code check** from the run brief, on a scratch copy,
  with `CARGO_TARGET_DIR=$TMPDIR/hv-28-target` and `CARGO_BUILD_JOBS=6`. The items
  to watch: `RunError::Start`'s payload and `stop` are used on both platforms;
  `start_failure` in `src/delivery.rs` is still used; `herdr_that_hangs_on` is
  `cfg(unix)` so no Windows warning.

- [ ] **Step 3: Leak check.** `git diff origin/main --stat` lists only the files in
  this plan plus `tasks/28/`. Confirm `git config core.hooksPath` prints `.githooks`
  and `test -s .leakwords` succeeds; the hook runs on every commit and the leak gate
  runs in CI.

- [ ] **Step 4: Stop.** S4 continues with the code review and the mutation test, run
  one after the other (brief: "One run at a time"); this plan ends here.
