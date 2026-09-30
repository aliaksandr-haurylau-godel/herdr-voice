# Issue #93 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans with superpowers:test-driven-development. Steps use checkbox (`- [ ]`) syntax.

**Goal:** A daemon whose herdr has gone still answers every request, and `doctor` fails when the daemon it finds cannot answer one.

**Architecture:** One module, `src/stderr.rs`, is the only place the daemon writes to standard error, and it ignores a failed write. `answer` gains a `ping` command with no effect. `daemon_finding` sends `ping` after its bare connect. `docs/design.md` records that a daemon left behind keeps serving.

**Tech Stack:** Rust, the existing `proto`, `transport` and `client` modules. No new dependency.

**Spec:** `tasks/93/DESIGN_93.md` (S2 READY, round 2), `tasks/93/AC_93.md`.

## Global Constraints

- Everything in the repository is English. No absolute path, employer, client or internal name in any file; cite paths relative to the repository root.
- No `eprintln!` or `println!` on a path reached from the moment the socket is bound (`AC_93.md`, requirement 2).
- `src/doctor.rs`: change `daemon_finding` and its tests only; issue #85 edits the same file.
- `ping` is not added to `herdr-plugin.toml`.
- Before each commit: `cargo test`, `cargo clippy --all-targets -- -D warnings`, `cargo fmt --check`, `python3 scripts/check_manifest.py`; grep every written file for stray edit-tool tags (the four `new_string` / `old_string` opening and closing tags) and line-start conflict markers.
- Commit messages end with `Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>`.

## Review Focus

- A `ptt` or `dictate` request served while standard error is dead: the reply must still be written (covered indirectly by Task 2; the whole `answer` path shares the journal).
- `doctor` run while a hold is open: `ping` must not touch the hold (Task 3 asserts `Control::Continue` and no state change).
- A daemon from an older build answers `unknown command: ping`: `doctor` says `missing` with that text (Task 4).
- A listener that accepts and closes without replying: `doctor` says `missing`, not `ok` (Task 4).
- Nothing listening: the existing text stays and is not replaced by the new one (Task 4).

---

### Task 1: The standard error writer

**Files:**
- Create: `src/stderr.rs`
- Modify: `src/main.rs` (add `mod stderr;` in alphabetical position, after `mod setup;`)

**Interfaces:**
- Produces: `pub fn write_line(sink: &mut dyn std::io::Write, line: &str)` and `pub fn line(line: &str)`.

- [ ] **Step 1: Write the failing tests.** Create `src/stderr.rs` with only the tests:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{self, Write};

    struct Broken;
    impl Write for Broken {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            Err(io::Error::from(io::ErrorKind::BrokenPipe))
        }
        fn flush(&mut self) -> io::Result<()> {
            Err(io::Error::from(io::ErrorKind::BrokenPipe))
        }
    }

    #[test]
    fn a_line_is_written_with_a_newline() {
        let mut sink = Vec::new();
        write_line(&mut sink, "listening at x");
        assert_eq!(sink, b"listening at x\n");
    }

    #[test]
    fn a_failed_write_is_ignored() {
        write_line(&mut Broken, "request command=cancel");
    }

    #[test]
    fn the_process_writer_does_not_panic() {
        line("stderr writer check");
    }
}
```

- [ ] **Step 2: Add `mod stderr;` to `src/main.rs`, run `cargo test stderr::` and see it fail to compile** (`write_line` not found).

- [ ] **Step 3: Implement**, above the tests:

```rust
//! The one place the daemon writes to standard error.
//!
//! herdr connects the daemon's standard error to a pipe it reads. When herdr is
//! gone nobody reads it, and `eprintln!` panics on the failed write, which took
//! the connection thread down before it wrote a reply (issue #93). A journal line
//! is a courtesy to whoever reads the log, never a condition of answering, so a
//! failed write is dropped.

use std::io::Write;

/// Writes `line` and a newline to `sink`; a failed write is ignored.
pub fn write_line(sink: &mut dyn Write, line: &str) {
    let _ = writeln!(sink, "{line}");
}

/// The same, to the process's standard error.
pub fn line(line: &str) {
    write_line(&mut std::io::stderr(), line);
}
```

- [ ] **Step 4: Run `cargo test stderr::`.** Expected: 3 passed.

- [ ] **Step 5: Commit** `git add src/stderr.rs src/main.rs && git commit -m "feat: a standard error writer that ignores a failed write"` (body: why, per issue #93).

### Task 2: Route every daemon write through it

**Files:**
- Modify: `src/daemon.rs` (lines `1097`, `1296`, `1316`, `1324`, `1339`, `1430`, `1443`, `1452`, `1459`, `1478`, `1480`; tests in the existing `mod tests`)
- Modify: `src/capture/cpal_source.rs:44`

**Interfaces:**
- Consumes: `crate::stderr::line(&str)` (Task 1); `Runtime.journal: Box<dyn Journal>` with `fn write(&self, line: &str)`.

- [ ] **Step 1: Write the failing tests** at the end of `mod tests` in `src/daemon.rs`:

```rust
    /// Every line the request path writes goes to the runtime's journal, so a
    /// dead standard error cannot reach the reply (issue #93). Before the change
    /// the request line went to the process's standard error and this is empty.
    #[test]
    fn a_served_request_is_journalled_and_answered() {
        let address = crate::transport::tests_support::probe_address("journalled");
        let listener = crate::transport::listen(&address).expect("listen");
        let journal = std::sync::Arc::new(RecordingJournal::default());
        let served = {
            let address = address.clone();
            let journal = std::sync::Arc::clone(&journal);
            std::thread::spawn(move || {
                let connection = listener.accept().expect("accept");
                let stop = AtomicBool::new(false);
                let mut runtime = fake_runtime("x");
                runtime.journal = Box::new(TestJournal(journal));
                super::serve_one(connection, &stop, &address, &silent_recorder(), &runtime)
                    .map_err(|e| e.to_string())
            })
        };
        let mut client =
            std::io::BufReader::new(crate::transport::connect(&address).expect("connect"));
        let sent = Request {
            command: "cancel".into(),
            entrypoint: Some("cancel".into()),
            context: vec![],
        };
        sent.write_to(client.get_mut()).expect("write");
        let reply = Reply::read_from(&mut client).expect("reply");
        assert_eq!(reply, Reply::Ok("nothing to cancel".to_string()));
        assert_eq!(served.join().expect("the handler must finish"), Ok(()));
        let lines = journal.0.lock().unwrap().clone();
        assert!(
            lines.contains(&request_line(&sent)),
            "the request line must reach the journal, got {lines:?}"
        );
    }

    /// The last thing that goes wrong in a dead pipe is the next `eprintln!`
    /// somebody adds. The production half of each file, up to its first
    /// `#[cfg(test)]`, must not contain one.
    #[test]
    fn no_print_macro_remains_on_a_serving_path() {
        for (name, source) in [
            ("src/daemon.rs", include_str!("daemon.rs")),
            (
                "src/capture/cpal_source.rs",
                include_str!("capture/cpal_source.rs"),
            ),
        ] {
            let production = source.split("#[cfg(test)]").next().unwrap();
            for mark in ["eprintln!", "println!", "eprint!", "print!("] {
                assert!(
                    !production.contains(mark),
                    "{name} writes with {mark} outside its tests; use crate::stderr::line"
                );
            }
        }
    }
```

  The second test's own text contains the marks, but it sits after the first `#[cfg(test)]` in `daemon.rs` (line 1492), so it is outside the slice it inspects.

- [ ] **Step 2: Run `cargo test daemon::tests::a_served_request daemon::tests::no_print`.** Expected: both FAIL (empty journal; `eprintln!` found).

- [ ] **Step 3: Implement.**
  - `StderrJournal::write`: body `crate::stderr::line(line);`.
  - `start()`: `crate::stderr::line(&format!("listening at {}", address.display()));`, and the same conversion for the three others (`1316`: the `recognition: the built-in engine, {}` line; `1324`: `recognition unavailable: {e}`, inside the `map_err` closure; `1339`: `crate::stderr::line(why)`, where `why` is the `&String` the `if let Err(why) = &bias_source` binds).
  - `serve()`: `runtime.journal.write(&format!("accept failed: {e}"));`, `"connection failed: {e}"` (inside the spawned closure, `runtime` is the closure's clone), `"the watcher thread ended badly: {e:?}"`, `"the drawing thread ended badly: {e:?}"`.
  - `serve_one`: `runtime.journal.write(&request_line(&request));` and `runtime.journal.write(&note);`.
  - `src/capture/cpal_source.rs:44`: `crate::stderr::line(&format!("more than one input is called {name:?}; taking the first"));`.

- [ ] **Step 4: Run `cargo test daemon::` and `cargo test capture::`.** Expected: all pass, including the two new tests.

- [ ] **Step 5: Windows dead-code check.** CI compiles Windows with `-D warnings`, and macOS cannot see an item reachable only from a `#[cfg(unix)]` path. Run, then restore, and never commit the edit:

```sh
find src -name '*.rs' -exec sed -i '' \
  -e 's/#\[cfg(all(test, unix))\]/#[cfg(any())]/g' \
  -e 's/#\[cfg(unix)\]/#[cfg(any())]/g' \
  -e 's/#\[cfg(not(unix))\]/#[cfg(not(any()))]/g' \
  -e 's/#\[cfg(windows)\]/#[cfg(not(any()))]/g' {} +
cargo clippy --all-targets -- -D warnings
git checkout -- src
```

  Expected: no warning. Run `git status` afterwards and see only the task's own files changed.

- [ ] **Step 6: Commit** `fix: the daemon writes standard error through one function that cannot panic`.

### Task 3: The `ping` command

**Files:**
- Modify: `src/daemon.rs` (`answer`, near `"cancel"` at `:169`; tests in `mod tests`)

**Interfaces:**
- Produces: request command `ping` answered `Reply::Ok("pong")`, `Control::Continue`; `needs_target_pane("ping")` stays false.

- [ ] **Step 1: Write the failing tests** in `mod tests`:

```rust
    #[test]
    fn ping_answers_pong_and_changes_nothing() {
        let runtime = fake_runtime("x");
        let (reply, control) = answer(&request("ping", b""), &silent_recorder(), &runtime);
        assert_eq!(reply, Reply::Ok("pong".to_string()));
        assert!(matches!(control, Control::Continue));
        assert!(matches!(&*hold_of(&runtime), crate::ptt::HoldState::Idle));
        assert!(!needs_target_pane("ping"));
    }

    #[test]
    fn ping_is_answered_while_a_hold_is_open_and_leaves_it_alone() {
        let (runtime, _clock) = fake_runtime_with_clock("a transcript");
        let recorder = tone_recorder("ping-hold");
        let (started, _) = answer(&request("ptt", PANE_1), &recorder, &runtime);
        assert_eq!(started, Reply::Ok("holding for w1:p1".to_string()));
        let (reply, control) = answer(&request("ping", b""), &recorder, &runtime);
        assert_eq!(reply, Reply::Ok("pong".to_string()));
        assert!(matches!(control, Control::Continue));
        let held = runtime.hold.lock().unwrap();
        let hold = held.hold().expect("the hold must still be open");
        assert_eq!(hold.target, "w1:p1");
        assert_eq!(hold.pokes, 1, "a ping is not a repeat");
    }
```

- [ ] **Step 2: Run `cargo test daemon::tests::ping`.** Expected: FAIL (`unknown command: ping`).

- [ ] **Step 3: Implement.** In `answer`, before the `"cancel"` arm:

```rust
        // Answers without reading a context or touching a take. `doctor` sends it
        // to learn that a request is served, and a probe must be safe to send
        // while somebody is speaking.
        "ping" => (Reply::Ok("pong".to_string()), Control::Continue),
```

- [ ] **Step 4: Run `cargo test daemon::`.** Expected: all pass.
- [ ] **Step 5: Commit** `feat: a ping command the doctor can send without touching a take`.

### Task 4: `doctor` sends the ping

**Files:**
- Modify: `src/doctor.rs` (`daemon_finding` at `:138`, and its tests)

**Interfaces:**
- Consumes: `client::send_to(&Address, &str, Option<String>, Vec<u8>) -> client::Outcome { code: u8, message: Option<String> }`; `transport::connect`, `transport::listen`, `transport::tests_support::probe_address`; `proto::{Request, Reply}`.
- Produces: `fn daemon_finding_at(address: &transport::Address) -> Finding`; `daemon_finding()` keeps its signature.

- [ ] **Step 1: Write the failing tests** in `src/doctor.rs`'s `mod tests`. All four are `#[cfg(unix)]`: the helper's first accepted connection is the bare probe, and a Windows named pipe does not queue a connection whose client has already left (see `daemon.rs`, `a_liveness_probe_is_not_reported_as_a_failure`). The Windows behaviour is recorded as unverified in `docs/evidence.md`.

```rust
    /// A listener written by hand. Its first connection is the bare connect
    /// `daemon_finding_at` makes; its second carries the `ping`, which it answers
    /// with `reply`, or by closing when `reply` is `None`.
    #[cfg(unix)]
    fn listener_answering(tag: &str, reply: Option<crate::proto::Reply>) -> transport::Address {
        let address = transport::tests_support::probe_address(tag);
        let listener = transport::listen(&address).expect("listen");
        std::thread::spawn(move || {
            drop(listener.accept().expect("the bare connect"));
            let connection = listener.accept().expect("the ping");
            let mut reader = std::io::BufReader::new(connection);
            let _ = crate::proto::Request::read_from(&mut reader);
            if let Some(reply) = reply {
                let _ = reply.write_to(reader.get_mut());
            }
        });
        address
    }

    #[cfg(unix)]
    #[test]
    fn a_daemon_that_answers_pong_is_ok() {
        let address = listener_answering(
            "doctor-pong",
            Some(crate::proto::Reply::Ok("pong".to_string())),
        );
        let finding = daemon_finding_at(&address);
        assert_eq!(finding.state, State::Ok, "got {finding:?}");
    }

    #[cfg(unix)]
    #[test]
    fn a_daemon_that_closes_without_answering_is_missing() {
        let address = listener_answering("doctor-silent", None);
        let finding = daemon_finding_at(&address);
        assert_eq!(finding.state, State::Missing, "got {finding:?}");
        assert!(finding.detail.contains(&address.display().to_string()));
        assert!(finding.detail.contains("herdr-voice daemon"), "{}", finding.detail);
    }

    #[cfg(unix)]
    #[test]
    fn a_daemon_that_does_not_know_ping_is_missing_and_says_so() {
        let address = listener_answering(
            "doctor-old",
            Some(crate::proto::Reply::Error("unknown command: ping".to_string())),
        );
        let finding = daemon_finding_at(&address);
        assert_eq!(finding.state, State::Missing, "got {finding:?}");
        assert!(finding.detail.contains("unknown command: ping"), "{}", finding.detail);
    }

    #[test]
    fn nothing_listening_keeps_the_existing_text() {
        let address = transport::tests_support::probe_address("doctor-nobody");
        let finding = daemon_finding_at(&address);
        assert_eq!(finding.state, State::Missing, "got {finding:?}");
        assert!(finding.detail.starts_with("nothing is listening at"), "{}", finding.detail);
    }
```

- [ ] **Step 2: Run `cargo test doctor::tests::a_daemon doctor::tests::nothing_listening`.** Expected: compile failure (`daemon_finding_at` not found).

- [ ] **Step 3: Implement.** Replace `daemon_finding` with:

```rust
fn daemon_finding() -> Finding {
    match transport::address(&transport::Vars::from_env()) {
        Err(e) => Finding {
            name: "daemon",
            state: State::Missing,
            detail: e.to_string(),
        },
        Ok(address) => daemon_finding_at(&address),
    }
}

/// `ok` only when a request comes back with a reply. A bare connect proves that a
/// process holds the socket, not that it serves anything: the daemon a herdr
/// restart leaves behind held the socket and answered nothing (issue #93).
fn daemon_finding_at(address: &transport::Address) -> Finding {
    if transport::connect(address).is_err() {
        return Finding {
            name: "daemon",
            state: State::Missing,
            detail: format!(
                "nothing is listening at {}; start it with `herdr-voice daemon`, \
                 or restart herdr",
                address.display()
            ),
        };
    }
    let outcome = crate::client::send_to(address, "ping", None, Vec::new());
    if outcome.code == 0 && outcome.message.as_deref() == Some("pong") {
        return Finding {
            name: "daemon",
            state: State::Ok,
            detail: format!("listening at {}", address.display()),
        };
    }
    let recovery = if cfg!(unix) {
        "end it with `pkill -f 'herdr-voice daemon'`"
    } else {
        "end the herdr-voice process that was started with `daemon`"
    };
    Finding {
        name: "daemon",
        state: State::Missing,
        detail: format!(
            "did not answer a request at {}: {}; {recovery}, then restart herdr or \
             run `herdr-voice daemon`",
            address.display(),
            outcome
                .message
                .as_deref()
                .unwrap_or("the reply was not `pong`")
        ),
    }
}
```

  Check that `transport::Address` is the type's public path and that `display()` exists (`daemon.rs` uses `address.display()`); adjust the import only, not the behaviour.

- [ ] **Step 4: Run `cargo test doctor::`.** Expected: all pass. Then the Windows dead-code check, the command of Task 2 Step 5: `cfg!(unix)` is a value, so neither `recovery` branch is dead code.
- [ ] **Step 5: Commit** `fix: doctor sends a request instead of only connecting`.

### Task 5: The design document and the decisions list

**Files:**
- Modify: `docs/design.md` (new subsection at the end of section 2; one line in section 9)
- Modify: `docs/decisions.md` (one row)

- [ ] **Step 1: Write the subsection** "The daemon outlives its herdr", after section 2's "Why", in the Context / Problem / Decision / Why form, with the text of `DESIGN_93.md` sections 1 to 3 stated for a reader who has not seen them: the daemon is not stopped when herdr exits and keeps the socket; standard error goes to a pipe herdr reads, and a failed write must not stop a reply; the decision is that the daemon keeps serving; `doctor` sends `ping`; standard error is written only through `src/stderr.rs`.
- [ ] **Step 2: Add to section 9** an open question, kept until S5 answers it: whether an old daemon's `HERDR_*` environment still reaches the new herdr.
- [ ] **Step 3: Add the row** to `docs/decisions.md`: decision "A daemon whose herdr has gone keeps serving; `doctor` finds one that cannot answer with a `ping`", basis "exiting needs a detection signal that exists on no platform uniformly, and loses a take in progress; a request that reaches `answer` fails where a keypress fails", where `2026-09-30, #93`.
- [ ] **Step 4: Read the subsection once as somebody who has not seen this plan.** It must not cite a file for its meaning and must not mention who decided or when.
- [ ] **Step 5: Commit** `docs: the daemon outlives its herdr`.

### Task 6: Gates

- [ ] Run the four gates and the Windows dead-code check (the command of Task 2 Step 5) fresh, and the marker grep over `src/stderr.rs`, `src/daemon.rs`, `src/doctor.rs`, `src/capture/cpal_source.rs`, `docs/design.md`, `docs/decisions.md`, `tasks/93/*`. If `stt::fetch` tests (issue #62) fail, rerun once and say so in `RUN_93.md`.
- [ ] `test -s .leakwords` succeeds and `git config core.hooksPath` prints `.githooks` before the first commit of the run.
