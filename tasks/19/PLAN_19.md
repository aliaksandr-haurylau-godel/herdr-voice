# Multi-line reply Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans with superpowers:test-driven-development to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A reply whose text contains a newline reaches the client whole, and a reply cut short is reported as an error instead of as a complete reply.

**Architecture:** `Reply::write_to` keeps the single-line form for a text without `\n`. For a text with `\n` it writes a header line `ok+<n>` or `error+<n>` followed by exactly `<n>` bytes; `Reply::read_from` recognises that header and reads the announced number of bytes. Nothing in the daemon's reply composition changes.

**Tech Stack:** Rust, `std::io`, the existing `transport` module (local sockets), `cargo test`.

**Spec:** `tasks/19/DESIGN_19.md` (the decision and the wire format), `tasks/19/AC_19.md` (AC-1 to AC-7). Facts behind the design are in `tasks/19/DESIGN_19.evidence.md`.

## Global Constraints

- Everything written into the repository is in English: code, comments, test names, commit messages.
- No absolute path and no name of an employer, a client or a machine in any file or commit message. Paths in documents are relative to the repository root.
- No panic path in non-test code: `Reply::read_from` returns `ProtoError` for every malformed input; `unwrap`, `expect` and indexing that can panic are for test code only.
- A reply without `\n` in its text is written byte for byte as before: `ok <text>\n` and `error <text>\n`.
- The request frame (`voice/1`) and the protocol token are not changed.
- `src/daemon.rs` is edited only by appending tests at the end of its `mod tests`; no non-test line of it changes. Another run edits the same file, so keep the edit small and at the end.
- Work on branch `fix/19-multiline-reply`. Never commit to `main`.
- Commit messages end with the line `Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>`.

## Review Focus

Inputs the design implies that are most likely to bite, each pinned by a test in the task named:

1. A text that ends with `\n`, is only `\n`, or is `\n\n` must read back equal, not lose its trailing newline (Task 1).
2. A text with multi-byte characters and a newline must announce its length in bytes, not characters (Task 1).
3. A header announcing more bytes than arrive, including an absurd number, must give an error naming both numbers and must not allocate the announced size (Task 1).
4. A single-line text that itself looks like a header (`ok+3`) must not be mistaken for one; a header with a non-numeric length must be refused (Task 1).
5. Bytes that follow the announced body must not be consumed, and `\r\n` inside the text must survive (Task 1).

## Task 1: Length-announced multi-line replies in the protocol

**Files:**
- Modify: `src/proto.rs` (the `Reply` impl at `:157-178`, `ProtoError` at `:33-67`, the module comment at `:1-6`, the tests module at `:180-299`)
- Modify: `src/client.rs` (the first line at `:1`; add one test at the end of `mod tests`)

**Interfaces:**
- Consumes: `Reply`, `ProtoError`, `ProtoError::ShortBody { expected: usize, got: usize }`, `ProtoError::BadHeader(String)` as they are; `crate::transport::{listen, connect}` and `crate::transport::tests_support::probe_address` for the socket tests.
- Produces: `ProtoError::NotText` (unit variant); `Reply::write_to` and `Reply::read_from` with unchanged signatures; the wire forms `ok <text>\n`, `error <text>\n` (text has no `\n`) and `ok+<n>\n<n bytes>`, `error+<n>\n<n bytes>` (text has `\n`, `<n>` is `text.len()`).

- [ ] **Step 1: Write the failing tests in `src/proto.rs`**

Append these inside `mod tests`, after `replies_survive_a_round_trip`:

```rust
    fn reply_bytes(reply: &Reply) -> Vec<u8> {
        let mut buffer = Vec::new();
        reply.write_to(&mut buffer).expect("write");
        buffer
    }

    fn reply_round_trip(reply: &Reply) -> Reply {
        Reply::read_from(&mut BufReader::new(&reply_bytes(reply)[..])).expect("read")
    }

    #[test]
    fn a_reply_without_a_newline_is_written_as_it_always_was() {
        assert_eq!(
            reply_bytes(&Reply::Ok("holding for w1:p1".into())),
            b"ok holding for w1:p1\n"
        );
        assert_eq!(
            reply_bytes(&Reply::Error("no pane".into())),
            b"error no pane\n"
        );
        assert_eq!(reply_bytes(&Reply::Ok(String::new())), b"ok \n");
    }

    #[test]
    fn a_reply_with_a_newline_announces_its_length_in_bytes() {
        assert_eq!(reply_bytes(&Reply::Ok("a\nb".into())), b"ok+3\na\nb");
        // Two bytes for the letter and one for the newline: the count is bytes.
        assert_eq!(
            reply_bytes(&Reply::Error("é\n".into())),
            "error+3\né\n".as_bytes()
        );
    }

    #[test]
    fn a_reply_carrying_newlines_arrives_whole() {
        for text in [
            "",
            "first line\nsecond line",
            "ends with a newline\n",
            "\n",
            "\n\n",
            "a\n\nb",
            "a\r\nb",
            "café\nnaïve — ü",
            "ok+3",
            "ok+3\nmore",
        ] {
            for reply in [Reply::Ok(text.to_string()), Reply::Error(text.to_string())] {
                assert_eq!(reply_round_trip(&reply), reply, "text {text:?}");
            }
        }
    }

    #[test]
    fn the_reader_stops_where_the_announced_length_stops() {
        let mut input = BufReader::new(&b"ok+3\na\nbXYZ"[..]);
        assert_eq!(
            Reply::read_from(&mut input).expect("read"),
            Reply::Ok("a\nb".into())
        );
        let mut rest = String::new();
        std::io::Read::read_to_string(&mut input, &mut rest).expect("rest");
        assert_eq!(rest, "XYZ");
    }

    #[test]
    fn a_body_shorter_than_announced_is_refused_with_both_numbers() {
        let error = Reply::read_from(&mut BufReader::new(&b"ok+10\nshort"[..]))
            .expect_err("must refuse");
        assert!(
            matches!(
                error,
                ProtoError::ShortBody {
                    expected: 10,
                    got: 5
                }
            ),
            "got {error:?}"
        );
        let error =
            Reply::read_from(&mut BufReader::new(&b"error+4\n"[..])).expect_err("must refuse");
        assert!(
            matches!(
                error,
                ProtoError::ShortBody {
                    expected: 4,
                    got: 0
                }
            ),
            "got {error:?}"
        );
    }

    #[test]
    fn an_absurd_announced_length_is_an_error_not_an_allocation() {
        let error = Reply::read_from(&mut BufReader::new(&b"ok+4000000000\nx"[..]))
            .expect_err("must refuse");
        assert!(
            matches!(
                error,
                ProtoError::ShortBody {
                    expected: 4_000_000_000,
                    got: 1
                }
            ),
            "got {error:?}"
        );
    }

    #[test]
    fn a_header_whose_length_is_not_digits_is_refused_as_a_bad_header() {
        for input in [
            &b"ok+\nabc"[..],
            b"ok+x\nabc",
            b"ok+-1\nabc",
            b"ok+1x\nabc",
            b"ok+3 tail\nabc",
            b"error+ 3\nabc",
            b"okay+3\nabc",
        ] {
            let error = Reply::read_from(&mut BufReader::new(input)).expect_err("must refuse");
            assert!(
                matches!(error, ProtoError::BadHeader(_)),
                "{:?} gave {error:?}",
                String::from_utf8_lossy(input)
            );
        }
    }

    #[test]
    fn a_body_that_is_not_text_is_refused_by_name() {
        let error = Reply::read_from(&mut BufReader::new(&b"ok+2\n\xff\xfe"[..]))
            .expect_err("must refuse");
        assert!(matches!(error, ProtoError::NotText), "got {error:?}");
    }

    #[test]
    fn a_multi_line_reply_crosses_a_real_socket_whole() {
        fn sent() -> Reply {
            Reply::Ok("first line\nsecond line [-20.0 dB, w1:p2]".into())
        }
        let address = crate::transport::tests_support::probe_address("proto-multiline-reply");
        let listener = crate::transport::listen(&address).expect("listen");
        let server = std::thread::spawn(move || {
            let mut stream = listener.accept().expect("accept");
            sent().write_to(&mut stream).expect("write");
        });
        let stream = crate::transport::connect(&address).expect("connect");
        let read = Reply::read_from(&mut BufReader::new(stream)).expect("read");
        server.join().expect("the server thread");
        assert_eq!(read, sent());
    }
```

- [ ] **Step 2: Write the failing client test in `src/client.rs`**

Append inside `mod tests`, after `the_context_travels_verbatim`:

```rust
    #[test]
    fn a_reply_with_newlines_reaches_the_caller_whole_with_the_exit_code_of_its_kind() {
        const TEXT: &str = "the engine is not configured, for example:\n  command = [\"x\"]\n\
                            the take is kept at /takes/1.wav";
        let address = crate::transport::tests_support::probe_address("client-multiline");
        let listener = crate::transport::listen(&address).expect("listen");
        let server = std::thread::spawn(move || {
            let mut reader = std::io::BufReader::new(listener.accept().expect("accept"));
            Request::read_from(&mut reader).expect("read");
            Reply::Error(TEXT.into())
                .write_to(reader.get_mut())
                .expect("write");
        });

        let outcome = send_to(&address, "cancel", Some("cancel".into()), Vec::new());
        server.join().expect("the server thread");
        assert_eq!(
            outcome,
            Outcome {
                code: 1,
                message: Some(TEXT.to_string()),
            }
        );
    }
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cargo test -- proto::tests client::tests 2>&1 | tail -40`

Expected: the build fails with ``no variant or associated item named `NotText` found for enum `ProtoError` `` (the test names it). That is the expected red: the variant does not exist yet.

- [ ] **Step 4: Implement in `src/proto.rs`**

1. Change the import line `use std::io::{self, BufRead, Write};` to `use std::io::{self, BufRead, Read, Write};`.

2. Add a variant to `ProtoError`, after `ShortBody { .. }` and before `Io(io::Error)`:

```rust
    /// A reply body that is not valid UTF-8. The daemon only ever writes text,
    /// so this is a peer that is not one.
    NotText,
```

3. Add its arm to `impl fmt::Display for ProtoError`, after the `ShortBody` arm:

```rust
            ProtoError::NotText => write!(f, "the reply text is not valid UTF-8"),
```

4. Replace the whole `impl Reply { .. }` block (the one holding `write_to` and `read_from`) with:

```rust
/// Splits `ok+<n>` or `error+<n>` into the kind and the announced length. Anything
/// else, including a length that is not plain digits, is `None`.
fn announced(line: &str) -> Option<(&str, usize)> {
    let (kind, digits) = line.split_once('+')?;
    if kind != "ok" && kind != "error" {
        return None;
    }
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    Some((kind, digits.parse().ok()?))
}

impl Reply {
    /// A text without a newline travels as one line, as it always has. A text
    /// with a newline travels as a header line that announces its length in
    /// bytes, followed by exactly that many bytes, so the reader never has to
    /// guess where the text ends.
    pub fn write_to<W: Write>(&self, w: &mut W) -> Result<(), ProtoError> {
        let (kind, text) = match self {
            Reply::Ok(text) => ("ok", text),
            Reply::Error(text) => ("error", text),
        };
        if text.contains('\n') {
            writeln!(w, "{kind}+{}", text.len())?;
            w.write_all(text.as_bytes())?;
        } else {
            writeln!(w, "{kind} {text}")?;
        }
        w.flush()?;
        Ok(())
    }

    pub fn read_from<R: BufRead>(r: &mut R) -> Result<Reply, ProtoError> {
        let mut line = String::new();
        r.read_line(&mut line)?;
        let line = line.trim_end_matches(['\r', '\n']);
        match line.split_once(' ') {
            Some(("ok", rest)) => return Ok(Reply::Ok(rest.to_string())),
            Some(("error", rest)) => return Ok(Reply::Error(rest.to_string())),
            _ if line == "ok" => return Ok(Reply::Ok(String::new())),
            _ => {}
        }
        let Some((kind, length)) = announced(line) else {
            return Err(ProtoError::BadHeader(line.to_string()));
        };
        // Read what arrives, up to the announced length, rather than allocating
        // the announced length: a wrong number cannot cost more than was sent.
        let mut body = Vec::new();
        Read::take(&mut *r, length as u64).read_to_end(&mut body)?;
        if body.len() != length {
            return Err(ProtoError::ShortBody {
                expected: length,
                got: body.len(),
            });
        }
        let text = String::from_utf8(body).map_err(|_| ProtoError::NotText)?;
        Ok(if kind == "ok" {
            Reply::Ok(text)
        } else {
            Reply::Error(text)
        })
    }
}
```

5. Replace the module comment's last sentence area: after the existing paragraph in `//!` lines at the top, add a second paragraph (keep the existing one):

```rust
//!
//! The reply is one line when its text has no newline. A text with a newline is
//! a header line, `ok+<n>` or `error+<n>`, followed by exactly `<n>` bytes.
```

- [ ] **Step 5: Correct the comment in `src/client.rs`**

Replace line 1, `//! The short-lived half: one connection, one frame, one reply line, exit.`, with:

```rust
//! The short-lived half: one connection, one frame, one reply, exit.
```

- [ ] **Step 6: Run the tests to verify they pass**

Run: `cargo test -- proto::tests client::tests 2>&1 | tail -40`

Expected: all tests in both modules pass, including the ten new ones and the existing `replies_survive_a_round_trip` and `a_daemon_answers_a_real_client`.

Then run `cargo test 2>&1 | tail -15`. Expected: everything passes. If a test in `src/stt/fetch.rs` fails, rerun once and record it in `RUN_19.md` (issue #62); any other failure is this task's.

- [ ] **Step 7: Commit**

Before the first commit of the run, check both gates of the public-repository rule:

```sh
test -s .leakwords && echo leakwords-present
git config core.hooksPath
```

Expected: `leakwords-present` and `.githooks`. If either is missing, stop and report to the orchestrator.

Then grep every file written for stray markers; expected, no output:

```sh
grep -n -e '</*\(new\|old\)_string>' -e '^<<<<<<<' -e '^=======' -e '^>>>>>>>' src/proto.rs src/client.rs tasks/19/*.md
```

Then:

```sh
git add src/proto.rs src/client.rs tasks/19
git commit -m "fix: carry a reply that contains a newline whole (#19)" \
  -m "A reply was written with every line it had and read back as one, so the client kept the first line and exited 0. A text with a newline now travels as a header that announces its length in bytes followed by exactly that many bytes; a text without one is unchanged on the wire." \
  -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

## Task 2: Real-socket tests through the client and the daemon's connection handler

**Files:**
- Modify: `src/daemon.rs` (append tests at the end of `mod tests`; check the end with `tail -5 src/daemon.rs`)

**Interfaces:**
- Consumes: from Task 1, the new framing (this task's tests are red without it). Existing helpers in `mod tests` of `src/daemon.rs`: `tone_recorder(tag: &str) -> Recorder` (`:1694`), `fake_runtime(text: &str) -> Runtime` (`:1619`), `takes_dir(tag)`. From elsewhere: `super::serve_one(connection, &AtomicBool, &Address, &Recorder, &Runtime)`, `crate::client::send_to(&Address, &str, Option<String>, Vec<u8>) -> crate::client::Outcome`, `crate::stt::Engine`, `crate::stt::EngineError`, `crate::stt::command::CommandError`, `crate::delivery::tests_support::FakeDeliverer::ok()`. `Runtime.recognition` is `Result<Box<dyn Engine + Send + Sync>, String>` (`src/daemon.rs:47`) and is a public field.
- Produces: nothing later tasks use.

- [ ] **Step 1: Write the tests**

Append inside `mod tests`, before its closing brace:

```rust
    /// Two `dictate` presses through a real listener and the real client: the
    /// first begins a take, the second ends it and is answered with what the
    /// pipeline produced. The daemon side is `serve_one`, the function the
    /// daemon's accept loop calls for every connection.
    fn two_presses_over_a_socket(
        tag: &str,
        runtime: Runtime,
    ) -> (crate::client::Outcome, crate::client::Outcome) {
        let address = crate::transport::tests_support::probe_address(tag);
        let listener = crate::transport::listen(&address).expect("listen");
        let recorder = std::sync::Arc::new(tone_recorder(tag));
        let runtime = std::sync::Arc::new(runtime);
        let server = {
            let address = address.clone();
            std::thread::spawn(move || {
                for _ in 0..2 {
                    let connection = listener.accept().expect("accept");
                    let stop = AtomicBool::new(false);
                    super::serve_one(connection, &stop, &address, &recorder, &runtime)
                        .expect("serve one connection");
                }
            })
        };
        let context = br#"{"focused_pane_id":"w1:p2","focused_pane_agent":"claude"}"#.to_vec();
        let first = crate::client::send_to(
            &address,
            "dictate",
            Some("dictate".into()),
            context.clone(),
        );
        let second = crate::client::send_to(&address, "dictate", Some("dictate".into()), context);
        server.join().expect("the server thread");
        (first, second)
    }

    /// The path a failure reply ends with: "... the take is kept at <path>".
    fn kept_path(message: &str) -> &str {
        message
            .rsplit("the take is kept at ")
            .next()
            .expect("the reply names the kept take")
    }

    #[test]
    fn ac2_a_two_line_transcript_leaves_the_target_and_the_level_in_the_client_output() {
        let runtime = fake_runtime("First sentence of the take.\nSecond sentence of the take.");
        let (first, second) = two_presses_over_a_socket("multiline-ac2", runtime);
        assert_eq!(first.code, 0, "{first:?}");
        assert_eq!(second.code, 0, "{second:?}");
        let message = second.message.expect("a success names where the take went");
        assert!(message.contains("delivered to w1:p2"), "got {message:?}");
        assert!(message.contains(" dB]"), "got {message:?}");
    }

    #[test]
    fn ac3_a_refusal_that_carries_an_example_delivers_the_example_and_the_kept_path() {
        let mut runtime = fake_runtime("unused");
        runtime.recognition = Err(crate::stt::EngineError::NotConfigured {
            engine: "command",
            key: "command",
            example: "command = [\"whisper-cli\", \"-f\", \"{audio}\"]",
        }
        .to_string());
        let (_, second) = two_presses_over_a_socket("multiline-ac3", runtime);
        assert_eq!(second.code, 1, "{second:?}");
        let message = second.message.expect("a failure says why");
        assert!(
            message.contains("For example:\n  command = [\"whisper-cli\""),
            "got {message:?}"
        );
        let path = kept_path(&message);
        assert!(std::path::Path::new(path).exists(), "kept at {path:?}");
        std::fs::remove_file(path).ok();
    }

    /// An engine whose failure carries the transcriber's standard error, which
    /// has two lines.
    struct TwoLineFailure;

    impl crate::stt::Engine for TwoLineFailure {
        fn transcribe(
            &self,
            _audio: &std::path::Path,
            _bias: &str,
        ) -> Result<String, crate::stt::EngineError> {
            Err(crate::stt::EngineError::Command(
                crate::stt::command::CommandError::Failed {
                    program: "whisper-cli".into(),
                    code: "exit status: 1".into(),
                    stderr: "model not found\nrun `whisper-cli --help`".into(),
                },
            ))
        }
    }

    #[test]
    fn ac4_a_transcriber_that_fails_on_two_lines_shows_both_and_the_kept_path() {
        let mut runtime = fake_runtime("unused");
        runtime.recognition = Ok(Box::new(TwoLineFailure));
        let (_, second) = two_presses_over_a_socket("multiline-ac4", runtime);
        assert_eq!(second.code, 1, "{second:?}");
        let message = second.message.expect("a failure says why");
        assert!(
            message.contains("model not found\nrun `whisper-cli --help`"),
            "got {message:?}"
        );
        let path = kept_path(&message);
        assert!(std::path::Path::new(path).exists(), "kept at {path:?}");
        std::fs::remove_file(path).ok();
    }
```

- [ ] **Step 2: Run them with Task 1 in place; they must pass**

Run: `cargo test daemon::tests::ac 2>&1 | tail -20`

Expected: `ac2_...`, `ac3_...` and `ac4_...` pass. If a test fails on something other than the reply (for example the second press answers "nothing to cancel" because the first take had not begun), report it instead of adjusting the assertion.

- [ ] **Step 3: Prove AC-3 and AC-4 fail without the fix**

Task 1 is committed, so `src/proto.rs` can be swapped for the version on `main` and restored from the commit. Run exactly:

```sh
git show 13733c5:src/proto.rs > src/proto.rs
cargo test daemon::tests::ac 2>&1 | tail -30
git checkout -- src/proto.rs
git diff --stat -- src/proto.rs
```

Expected: `ac3_...` and `ac4_...` FAIL (the message ends at the first newline); `ac2_...` passes, because on `main` the success reply carries no transcript and so has nothing to lose; `git diff --stat` after the checkout prints nothing. The file from `main` brings its own tests module with it, so the build does not depend on anything Task 1 added.

- [ ] **Step 4: Commit**

Run the marker grep of Task 1, Step 7 on `src/daemon.rs`, then:

```sh
git add src/daemon.rs
git commit -m "test: a multi-line reply through the real client and the connection handler (#19)" \
  -m "Two dictate presses over a real socket: a two-line transcript keeps the target and the level, and the two failures whose text has a newline keep their example, both lines and the path of the kept recording. The last two fail against the protocol on main." \
  -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

## Task 3: The gates

**Files:**
- No file is edited. Everything is committed before this task starts; `git status --short` must print nothing.

**Interfaces:**
- Consumes: Tasks 1 and 2, committed.
- Produces: the pass or fail of AC-7.

- [ ] **Step 1: The four gates, run fresh**

```sh
cargo test 2>&1 | tail -15
cargo clippy --all-targets -- -D warnings 2>&1 | tail -15
cargo fmt --check && echo fmt-ok
python3 scripts/check_manifest.py
```

Expected: tests pass (a failure in `src/stt/fetch.rs` download tests is issue #62 and an indicator test failing on Linux is issue #66: rerun once, and record the rerun in `RUN_19.md`; any other failure is this run's), clippy prints no warning, `fmt-ok`, and the manifest check exits 0. If `cargo fmt --check` prints a diff, run `cargo fmt`, re-run all four, and commit the formatting with the task that owns the file.

- [ ] **Step 2: The Windows dead-code check**

CI compiles Windows with `-D warnings`, and macOS cannot see an item that is reachable only from a `#[cfg(unix)]` path. This check rewrites `src/` in place and restores it with `git checkout -- src`, which discards uncommitted edits, so it runs only on a clean tree:

```sh
git diff --quiet -- src && git diff --cached --quiet -- src && echo src-clean
```

Expected: `src-clean`. If it does not print that, stop: commit or set aside the edits first. Then:

```sh
find src -name '*.rs' -exec sed -i '' \
  -e 's/#\[cfg(all(test, unix))\]/#[cfg(any())]/g' \
  -e 's/#\[cfg(unix)\]/#[cfg(any())]/g' \
  -e 's/#\[cfg(not(unix))\]/#[cfg(not(any()))]/g' \
  -e 's/#\[cfg(windows)\]/#[cfg(not(any()))]/g' {} +
cargo clippy --all-targets -- -D warnings 2>&1 | tail -15
git checkout -- src
git status --short
```

Expected: clippy prints no warning, and `git status --short` prints nothing (the rewrite is undone). Never commit the rewritten files.

- [ ] **Step 3: Record the result**

Append to `tasks/19/RUN_19.md`, under a new `### S4 Implement` heading, one line per gate with its result and any rerun. Commit:

```sh
git add tasks/19/RUN_19.md
git commit -m "docs: S4 gate results for issue #19" \
  -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

## After the tasks (the rest of S4 and S5, run by the author of the run, not tasks of this plan)

- S4 closes on two fresh subagents that did not write the code: a code reviewer over the whole diff (`superpowers:requesting-code-review`), and a mutation tester that mutates the lines the diff adds or changes one at a time and reports each mutation that left `cargo test` green. Each survivor is killed by a new test or explained in `RUN_19.md`.
- S5 runs a real take of two sentences long enough for the transcriber to split them, through a daemon started from this worktree with its own socket and state directory, and records the command, the whole output and the platform in `docs/evidence.md`.

## Coverage of the acceptance criteria

| AC | Where |
|---|---|
| AC-1 | Task 1: `a_reply_carrying_newlines_arrives_whole`, `a_body_shorter_than_announced_is_refused_with_both_numbers`, the client test |
| AC-2 | Task 2: `ac2_...` |
| AC-3 | Task 2: `ac3_...` |
| AC-4 | Task 2: `ac4_...` |
| AC-5 | Task 1: `a_multi_line_reply_crosses_a_real_socket_whole` |
| AC-6 | Task 1: `a_reply_without_a_newline_is_written_as_it_always_was` |
| AC-7 | Task 3 |
