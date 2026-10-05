# PLAN_52 — failures say what actually failed

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans with
> superpowers:test-driven-development. Steps use checkbox (`- [ ]`) syntax.

**Goal:** Every failure of the two HTTP engines and of starting `herdr` produces a
message that names its own cause and says what to do next, and keeps what the
server said.

**Architecture:** A new `src/http_failure.rs` holds the four causes of an HTTP
failure, reads one out of a `ureq::Error`, and writes the message for each.
`src/rewrite/http.rs` and `src/stt/http.rs` hold a cause in their error and take
their time bound as a parameter. `src/delivery.rs` gets one pure function that
turns the `io::Error` from starting a program into the right variant.

**Tech stack:** Rust, `ureq` 2.12.1 (blocking), `std` only for the new code.

**Spec:** `tasks/52/DESIGN_52.md` (evidence: `tasks/52/DESIGN_52_evidence.md`),
acceptance criteria `tasks/52/AC_52.md`.

## Global constraints

- Everything in the repository is English: code, comments, output strings,
  commits, documents.
- No absolute paths, no employer, client or machine names in any file, comment,
  fixture or commit message. The pre-commit hook and the leak gate enforce this.
- No panic path in the daemon: no `unwrap`, `expect` or indexing on a path the
  daemon runs. Tests may `expect`.
- Every user-visible failure names what to do next.
- CI compiles with `-D warnings`: a variant or function nothing constructs fails
  the build. Every task leaves the crate with no dead code.
- No live endpoint and no live herdr in any test.
- One commit per task, each ending with the line
  `Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>`.
- Before every commit, from the repository root:
  ```sh
  test -s .leakwords && git config core.hooksPath
  cargo test
  cargo clippy --all-targets -- -D warnings
  cargo fmt --check
  python3 scripts/check_manifest.py
  grep -n -E '</?(new|old)_string>|^(<<<<<<<|=======|>>>>>>>)' <every file you wrote>
  ```
  `git config core.hooksPath` must print `.githooks`; the grep must print nothing.
  If a download test in `src/stt/fetch.rs` (#62) or an indicator test (#66) fails,
  rerun once and say so in `tasks/52/RUN_52.md`; any other failure is this run's.

## Review focus

Inputs and conditions the design implies and no acceptance criterion names, most
likely first. Each has a test in the task that owns the code.

1. A refusing server's body that is not valid UTF-8: the explanation keeps the
   readable part and the message is still built (Task 1).
2. A long body in Cyrillic: the cut falls on a character boundary and the message
   does not panic (Task 1).
3. A body with newlines: the message is one line (Task 1).
4. A server that accepts the connection and closes it without answering: the
   message is the last row of the table, not a timeout and not a refusal (Task 1).
5. `HERDR_BIN_PATH` pointing at a directory: the message is the not-executable
   one, and its advice says to point at the program itself (Task 3).

## File structure

| File | Responsibility |
|---|---|
| `src/http_failure.rs` (new) | `Cause`, `Cause::from_ureq`, `Cause::describe`, the excerpt of a body |
| `src/main.rs` | `mod http_failure;` |
| `src/rewrite/http.rs` | `HttpError::Failed { url, cause }`, `HttpEngine::with_timeout` |
| `src/stt/http.rs` | the same, plus `HttpError::AudioUnreadable`; `Refused` removed |
| `src/delivery.rs` | `DeliveryError::NotExecutable`, `DeliveryError::StartFailed`, `start_failure` |

## Dependencies between tasks

Task 1 has none. Task 2 needs Task 1 (`Cause`). Task 3 has none and can be done
in parallel with Tasks 1 and 2, because it touches `src/delivery.rs` only. Task 4
needs Tasks 1 to 3.

---

### Task 1: The shared causes, and the rewrite engine that uses them

**Files:**
- Create: `src/http_failure.rs`
- Modify: `src/main.rs` (add `mod http_failure;` on its own line directly above `mod indicator;`)
- Modify: `src/rewrite/http.rs`

**Interfaces:**
- Produces, in `crate::http_failure`:
  ```rust
  pub enum Cause {
      TimedOut { bound: std::time::Duration },
      ConnectionRefused,
      Answered { status: u16, explanation: String },
      Other { detail: String },
  }
  impl Cause {
      pub fn from_ureq(error: ureq::Error, bound: std::time::Duration) -> Cause;
      pub fn describe(&self, url: &str) -> String;
  }
  ```
- Produces, in `crate::rewrite::http`:
  ```rust
  pub enum HttpError {
      Failed { url: String, cause: Cause },
      Unreadable { url: String, detail: String },
  }
  impl HttpEngine {
      pub fn new(url: String, token: String, model: String) -> HttpEngine;
      pub fn with_timeout(url: String, token: String, model: String, timeout: Duration) -> HttpEngine;
  }
  ```

- [ ] **Step 1: Declare the module and write its failing tests**

In `src/main.rs`, add `mod http_failure;` on its own line directly above `mod indicator;`.

Create `src/http_failure.rs` with the real types, the tests, and stub function bodies that return a wrong value, so that the tests fail on their assertions and not on compilation:

```rust
//! What an HTTP failure was, read out of a `ureq::Error`, and the message each
//! cause produces. Shared by the two HTTP engines (`src/rewrite/http.rs` and
//! `src/stt/http.rs`) so the wording lives in one place (`tasks/52/DESIGN_52.md`).

use std::io::{self, Read};
use std::time::Duration;

/// How much of a refusing server's body is read.
const BODY_READ_BYTES: u64 = 2048;
/// How much of it reaches the message.
const EXPLANATION_CHARS: usize = 300;

#[derive(Debug)]
pub enum Cause {
    /// No reply within the engine's bound.
    TimedOut { bound: Duration },
    /// Nothing listens at the address and port.
    ConnectionRefused,
    /// The server answered with a non-2xx status.
    Answered { status: u16, explanation: String },
    /// Any other transport failure, for example a name that does not resolve.
    Other { detail: String },
}

impl Cause {
    pub fn from_ureq(error: ureq::Error, bound: Duration) -> Cause {
        let _ = (error, bound);
        Cause::ConnectionRefused
    }

    pub fn describe(&self, url: &str) -> String {
        let _ = url;
        String::new()
    }
}

fn from_io_kind(kind: Option<io::ErrorKind>, bound: Duration, detail: String) -> Cause {
    let _ = (kind, bound, detail);
    Cause::ConnectionRefused
}

fn excerpt(text: &str) -> String {
    text.to_string()
}

fn explanation_from_bytes(bytes: &[u8]) -> String {
    excerpt(&String::from_utf8_lossy(bytes))
}

fn bound_text(bound: Duration) -> String {
    format!("{bound:?}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_connection_nothing_listens_on_is_a_refusal() {
        // Port 1: nothing listens there. Shows what ureq's error chain really
        // ends in on this platform, which the design could not verify.
        let error = ureq::get("http://127.0.0.1:1/")
            .call()
            .expect_err("nothing listens on port 1");
        let cause = Cause::from_ureq(error, Duration::from_secs(30));
        assert!(matches!(cause, Cause::ConnectionRefused), "got {cause:?}");
    }

    #[test]
    fn a_timed_out_kind_is_a_timeout_carrying_the_bound() {
        let bound = Duration::from_millis(200);
        for kind in [io::ErrorKind::TimedOut, io::ErrorKind::WouldBlock] {
            let cause = from_io_kind(Some(kind), bound, "detail".to_string());
            assert!(
                matches!(cause, Cause::TimedOut { bound: b } if b == bound),
                "got {cause:?} for {kind:?}"
            );
        }
    }

    #[test]
    fn a_refused_kind_is_a_refusal() {
        let cause = from_io_kind(
            Some(io::ErrorKind::ConnectionRefused),
            Duration::from_secs(1),
            "detail".to_string(),
        );
        assert!(matches!(cause, Cause::ConnectionRefused), "got {cause:?}");
    }

    #[test]
    fn any_other_kind_and_no_kind_are_other_and_keep_the_detail() {
        for kind in [Some(io::ErrorKind::UnexpectedEof), None] {
            let cause = from_io_kind(kind, Duration::from_secs(1), "the detail".to_string());
            assert!(
                matches!(&cause, Cause::Other { detail } if detail == "the detail"),
                "got {cause:?} for {kind:?}"
            );
        }
    }

    #[test]
    fn an_excerpt_at_the_limit_is_unchanged_and_one_over_is_cut() {
        let at_limit = "a".repeat(EXPLANATION_CHARS);
        assert_eq!(excerpt(&at_limit), at_limit);
        let over = "a".repeat(EXPLANATION_CHARS + 1);
        assert_eq!(excerpt(&over), format!("{at_limit}..."));
    }

    #[test]
    fn an_excerpt_is_one_line() {
        assert_eq!(excerpt("{\n  \"error\":\r\n\t\"bad model\"\n}\n"), "{ \"error\": \"bad model\" }");
    }

    #[test]
    fn an_empty_or_blank_excerpt_is_empty() {
        assert_eq!(excerpt(""), "");
        assert_eq!(excerpt(" \n\t "), "");
    }

    #[test]
    fn a_cut_never_leaves_a_space_before_the_dots() {
        // Character 300 is a space: the cut must not end in " ...".
        let text = format!("{} {}", "a".repeat(EXPLANATION_CHARS - 1), "b".repeat(50));
        assert_eq!(excerpt(&text), format!("{}...", "a".repeat(EXPLANATION_CHARS - 1)));
    }

    #[test]
    fn a_long_cyrillic_body_is_cut_on_a_character_boundary() {
        let text = "ж".repeat(EXPLANATION_CHARS + 20);
        let cut = excerpt(&text);
        assert_eq!(cut, format!("{}...", "ж".repeat(EXPLANATION_CHARS)));
    }

    #[test]
    fn a_body_that_is_not_utf8_keeps_its_readable_part() {
        let explanation = explanation_from_bytes(b"model \xff\xfe not found");
        assert!(explanation.contains("model"), "got {explanation}");
        assert!(explanation.contains("not found"), "got {explanation}");
    }

    #[test]
    fn a_bound_is_written_in_seconds_when_whole_and_milliseconds_otherwise() {
        assert_eq!(bound_text(Duration::from_secs(30)), "30 seconds");
        assert_eq!(bound_text(Duration::from_secs(1)), "1 second");
        assert_eq!(bound_text(Duration::from_millis(200)), "200 milliseconds");
    }

    #[test]
    fn a_timeout_message_names_the_url_and_the_bound_and_says_to_wait() {
        let message = Cause::TimedOut {
            bound: Duration::from_secs(30),
        }
        .describe("http://h/v1");
        assert!(message.contains("\"http://h/v1\""), "got {message}");
        assert!(message.contains("did not reply within 30 seconds"), "got {message}");
        assert!(message.contains("wait and try again"), "got {message}");
        assert!(!message.contains("check the server is running"), "got {message}");
    }

    #[test]
    fn a_refusal_message_says_nothing_listens_and_what_to_do() {
        let message = Cause::ConnectionRefused.describe("http://h/v1");
        assert!(message.contains("\"http://h/v1\""), "got {message}");
        assert!(message.contains("refused the connection"), "got {message}");
        assert!(message.contains("Start the server"), "got {message}");
        assert!(!message.contains("did not reply"), "got {message}");
    }

    #[test]
    fn an_answer_message_names_status_and_explanation_and_not_reachability() {
        let message = Cause::Answered {
            status: 400,
            explanation: "unknown model".to_string(),
        }
        .describe("http://h/v1");
        assert!(message.contains("\"http://h/v1\""), "got {message}");
        assert!(message.contains("answered with status 400"), "got {message}");
        assert!(message.contains("unknown model"), "got {message}");
        assert!(message.contains("Correct the address, model or token"), "got {message}");
        assert!(!message.contains("cannot reach"), "got {message}");
        assert!(!message.contains("check the server is running"), "got {message}");
    }

    #[test]
    fn a_status_of_500_and_above_sends_the_person_to_the_servers_log() {
        let describe = |status| {
            Cause::Answered {
                status,
                explanation: "boom".to_string(),
            }
            .describe("u")
        };
        assert!(describe(500).contains("its own log says why"), "got {}", describe(500));
        assert!(!describe(499).contains("its own log says why"), "got {}", describe(499));
        assert!(describe(499).contains("Correct the address, model or token"), "got {}", describe(499));
        assert!(!describe(500).contains("Correct the address"), "got {}", describe(500));
    }

    #[test]
    fn an_answer_with_no_explanation_says_so() {
        let message = Cause::Answered {
            status: 400,
            explanation: String::new(),
        }
        .describe("u");
        assert!(message.contains("the server gave no explanation"), "got {message}");
    }

    #[test]
    fn another_failure_keeps_the_old_sentence_and_its_detail() {
        let message = Cause::Other {
            detail: "dns error".to_string(),
        }
        .describe("http://h/v1");
        assert!(message.contains("cannot reach \"http://h/v1\": dns error"), "got {message}");
        assert!(
            message.contains("check the server is running and the address is correct"),
            "got {message}"
        );
    }
}
```

- [ ] **Step 2: Run the tests and see them fail**

Run: `cargo test http_failure -- --test-threads=1`
Expected: the module compiles (`Read` unused is a warning, not yet an error under
`cargo test`) and the tests named above FAIL on their assertions, for example
`a_timed_out_kind_is_a_timeout_carrying_the_bound` with `got ConnectionRefused`.

- [ ] **Step 3: Write the implementation**

Replace the four stub bodies and `bound_text` in `src/http_failure.rs`, keeping the
tests:

```rust
impl Cause {
    /// Reads the cause out of what `ureq` returned. `bound` is the time bound the
    /// request was made with, carried into a timeout so the message can name it.
    pub fn from_ureq(error: ureq::Error, bound: Duration) -> Cause {
        match error {
            ureq::Error::Status(status, response) => Cause::Answered {
                status,
                explanation: explanation_of(response),
            },
            ureq::Error::Transport(transport) => {
                let kind = io_kind(&transport);
                from_io_kind(kind, bound, transport.to_string())
            }
        }
    }

    /// The message for this cause. It names `url` and ends in what to do next.
    pub fn describe(&self, url: &str) -> String {
        match self {
            Cause::TimedOut { bound } => format!(
                "{url:?} did not reply within {}. If the server is still loading a model, wait \
                 and try again",
                bound_text(*bound)
            ),
            Cause::ConnectionRefused => format!(
                "{url:?} refused the connection: nothing is listening at that address and port. \
                 Start the server, or correct the address in the configuration"
            ),
            Cause::Answered {
                status,
                explanation,
            } => {
                let said = if explanation.is_empty() {
                    "the server gave no explanation"
                } else {
                    explanation.as_str()
                };
                let advice = if *status >= 500 {
                    "The server failed on its side; its own log says why."
                } else {
                    "Correct the address, model or token in the configuration."
                };
                format!(
                    "{url:?} answered with status {status} and refused the request: {said}. \
                     {advice}"
                )
            }
            Cause::Other { detail } => format!(
                "cannot reach {url:?}: {detail}; check the server is running and the address is \
                 correct"
            ),
        }
    }
}

/// The operating system's own classification, read from the first `io::Error`
/// in the error's chain of sources. Matching on message text would break when
/// `ureq` rewords.
fn io_kind(error: &(dyn std::error::Error + 'static)) -> Option<io::ErrorKind> {
    let mut current = error.source();
    while let Some(source) = current {
        if let Some(io_error) = source.downcast_ref::<io::Error>() {
            return Some(io_error.kind());
        }
        current = source.source();
    }
    None
}

/// `WouldBlock` is what a socket timeout is on some Unix platforms; `ureq`
/// normalises it, and it is accepted here too so that a change there cannot turn
/// a timeout into the last row of the table.
fn from_io_kind(kind: Option<io::ErrorKind>, bound: Duration, detail: String) -> Cause {
    match kind {
        Some(io::ErrorKind::TimedOut) | Some(io::ErrorKind::WouldBlock) => {
            Cause::TimedOut { bound }
        }
        Some(io::ErrorKind::ConnectionRefused) => Cause::ConnectionRefused,
        _ => Cause::Other { detail },
    }
}

/// What the server said, bounded. A failure while reading the body leaves the
/// explanation empty: it must never replace the status, which is already known.
fn explanation_of(response: ureq::Response) -> String {
    let mut bytes = Vec::new();
    let _ = response
        .into_reader()
        .take(BODY_READ_BYTES)
        .read_to_end(&mut bytes);
    explanation_from_bytes(&bytes)
}

fn explanation_from_bytes(bytes: &[u8]) -> String {
    excerpt(&String::from_utf8_lossy(bytes))
}

/// One line, at most `EXPLANATION_CHARS` characters. Whitespace runs become one
/// space, because the message reaches the run journal and a toast, both of which
/// are a single line. Characters, not bytes, are counted so that a Cyrillic body
/// is never cut inside a character.
fn excerpt(text: &str) -> String {
    let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() <= EXPLANATION_CHARS {
        return flat;
    }
    let kept: String = flat.chars().take(EXPLANATION_CHARS).collect();
    format!("{}...", kept.trim_end())
}

fn bound_text(bound: Duration) -> String {
    if bound.subsec_nanos() == 0 {
        match bound.as_secs() {
            1 => "1 second".to_string(),
            seconds => format!("{seconds} seconds"),
        }
    } else {
        format!("{} milliseconds", bound.as_millis())
    }
}
```

- [ ] **Step 4: Run the module's tests**

Run: `cargo test http_failure -- --test-threads=1`
Expected: all PASS. If `a_connection_nothing_listens_on_is_a_refusal` fails, read
what `Cause::from_ureq` returned: it is the measurement the design could not make.
If the chain ends in something other than `ConnectionRefused`, stop and write the
finding into `tasks/52/RUN_52.md` before changing the design.

The crate now has dead code (`Cause` is unused outside its tests); the next steps
remove it. Do not run clippy yet.

- [ ] **Step 5: Write the rewrite engine's failing tests**

In `src/rewrite/http.rs`, inside `mod tests`, add below `find_subslice`:

```rust
    /// Accepts one connection and reads it until the client gives up and
    /// closes: the server that never answers. Draining matters for the same
    /// reason as in `respond_once`.
    fn never_answers() -> (String, std::thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let addr = listener.local_addr().expect("addr");
        let url = format!("http://{addr}/v1/chat/completions");
        let handle = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept");
            let mut chunk = [0u8; 4096];
            while matches!(stream.read(&mut chunk), Ok(n) if n > 0) {}
        });
        (url, handle)
    }

    /// Accepts one connection and closes it without a word.
    fn closes_without_answering() -> (String, std::thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let addr = listener.local_addr().expect("addr");
        let url = format!("http://{addr}/v1/chat/completions");
        let handle = std::thread::spawn(move || {
            let (stream, _) = listener.accept().expect("accept");
            drop(stream);
        });
        (url, handle)
    }

    /// The message of the failure a request produced.
    fn failure_of(engine: &HttpEngine) -> String {
        engine.rewrite("x", "").expect_err("must fail").to_string()
    }
```

and, at the end of `mod tests`:

```rust
    #[test]
    fn a_reply_that_never_comes_is_a_timeout_naming_the_bound() {
        let (url, handle) = never_answers();
        let engine = HttpEngine::with_timeout(
            url.clone(),
            String::new(),
            String::new(),
            Duration::from_millis(200),
        );
        let message = failure_of(&engine);
        assert!(message.contains(&url), "got {message}");
        assert!(
            message.contains("did not reply within 200 milliseconds"),
            "got {message}"
        );
        assert!(message.contains("wait and try again"), "got {message}");
        assert!(!message.contains("check the server is running"), "got {message}");
        handle.join().expect("server thread");
    }

    #[test]
    fn the_shipped_engine_holds_the_thirty_second_bound() {
        let engine = HttpEngine::new("http://h/v1".to_string(), String::new(), String::new());
        assert_eq!(engine.timeout, TIMEOUT);
        let message = crate::http_failure::Cause::TimedOut {
            bound: engine.timeout,
        }
        .describe("http://h/v1");
        assert!(message.contains("within 30 seconds"), "got {message}");
    }

    #[test]
    fn a_port_nothing_listens_on_is_a_refusal_not_a_timeout() {
        let engine = HttpEngine::new(
            "http://127.0.0.1:1/v1/chat/completions".to_string(),
            String::new(),
            String::new(),
        );
        let message = failure_of(&engine);
        assert!(message.contains("127.0.0.1:1"), "got {message}");
        assert!(message.contains("refused the connection"), "got {message}");
        assert!(message.contains("Start the server"), "got {message}");
        assert!(!message.contains("did not reply"), "got {message}");
    }

    #[test]
    fn a_400_with_a_body_keeps_the_servers_explanation() {
        let (url, handle) = respond_once_with_status(
            "400 Bad Request",
            r#"{"error":"unknown model: no-such-model"}"#,
        );
        let engine = HttpEngine::new(url.clone(), String::new(), String::new());
        let error = engine.rewrite("x", "").expect_err("must fail");
        assert!(
            matches!(
                &error,
                HttpError::Failed {
                    cause: Cause::Answered { status: 400, .. },
                    ..
                }
            ),
            "got {error:?}"
        );
        let message = error.to_string();
        assert!(message.contains(&url), "got {message}");
        assert!(message.contains("answered with status 400"), "got {message}");
        assert!(message.contains("unknown model: no-such-model"), "got {message}");
        assert!(!message.contains("cannot reach"), "got {message}");
        assert!(!message.contains("check the server is running"), "got {message}");
        handle.join().expect("server thread");
    }

    #[test]
    fn another_status_is_reported_with_its_own_status_and_body() {
        for (status_line, status, body) in [
            ("404 Not Found", "404", r#"{"error":"no such route"}"#),
            ("500 Internal Server Error", "500", r#"{"error":"model crashed"}"#),
        ] {
            let (url, handle) = respond_once_with_status(status_line, body);
            let engine = HttpEngine::new(url, String::new(), String::new());
            let message = failure_of(&engine);
            assert!(message.contains(&format!("status {status}")), "got {message}");
            let said = body.split('"').nth(3).expect("the error text");
            assert!(message.contains(said), "got {message}");
            handle.join().expect("server thread");
        }
    }

    #[test]
    fn a_refusal_with_no_body_says_the_server_gave_no_explanation() {
        let (url, handle) = respond_once_with_status("400 Bad Request", "");
        let engine = HttpEngine::new(url, String::new(), String::new());
        let message = failure_of(&engine);
        assert!(message.contains("status 400"), "got {message}");
        assert!(message.contains("the server gave no explanation"), "got {message}");
        handle.join().expect("server thread");
    }

    #[test]
    fn a_long_body_reaches_the_message_cut_to_its_bound() {
        // Leaked so the test double, which takes a `'static` body, can serve it.
        let body: &'static str = Box::leak("x".repeat(5000).into_boxed_str());
        let (url, handle) = respond_once_with_status("400 Bad Request", body);
        let engine = HttpEngine::new(url, String::new(), String::new());
        let message = failure_of(&engine);
        assert!(message.contains(&format!("{}...", "x".repeat(300))), "got {message}");
        assert!(!message.contains(&"x".repeat(301)), "got {message}");
        handle.join().expect("server thread");
    }

    #[test]
    fn a_multi_line_body_reaches_the_message_as_one_line() {
        let (url, handle) =
            respond_once_with_status("400 Bad Request", "{\n  \"error\": \"bad\"\n}\n");
        let engine = HttpEngine::new(url, String::new(), String::new());
        let message = failure_of(&engine);
        assert!(!message.contains('\n'), "got {message:?}");
        assert!(message.contains("{ \"error\": \"bad\" }"), "got {message}");
        handle.join().expect("server thread");
    }

    #[test]
    fn a_server_that_closes_without_answering_is_neither_a_timeout_nor_a_refusal() {
        let (url, handle) = closes_without_answering();
        let engine = HttpEngine::new(url, String::new(), String::new());
        let error = engine.rewrite("x", "").expect_err("must fail");
        assert!(
            matches!(
                &error,
                HttpError::Failed {
                    cause: Cause::Other { .. },
                    ..
                }
            ),
            "got {error:?}"
        );
        handle.join().expect("server thread");
    }
```

- [ ] **Step 6: Run them and see them fail**

Run: `cargo test rewrite::http -- --test-threads=1`
Expected: compile errors: `with_timeout` not found, `timeout` field not found,
`Cause` not in scope in the tests, `HttpError::Failed` has no field `cause`.

- [ ] **Step 7: Change the rewrite engine**

In `src/rewrite/http.rs`:

1. Add the import next to the others at the top: `use crate::http_failure::Cause;`
2. Replace `HttpEngine`'s struct and `new`:

```rust
pub struct HttpEngine {
    url: String,
    token: String,
    model: String,
    /// The bound the agent below was built with, kept so a timeout's message can
    /// name it. `new` passes `TIMEOUT`; a test passes a shorter one.
    timeout: Duration,
    agent: ureq::Agent,
}

impl HttpEngine {
    pub fn new(url: String, token: String, model: String) -> HttpEngine {
        HttpEngine::with_timeout(url, token, model, TIMEOUT)
    }

    pub fn with_timeout(
        url: String,
        token: String,
        model: String,
        timeout: Duration,
    ) -> HttpEngine {
        // A dedicated agent, not the crate's shared default one: with pooling
        // disabled, this take's connection is never reused and never confused
        // with a later one — one POST per take needing rewrite (`DESIGN_36.md`
        // §6), so pooling buys nothing here.
        let agent = ureq::AgentBuilder::new()
            .timeout(timeout)
            .max_idle_connections_per_host(0)
            .build();
        HttpEngine {
            url,
            token,
            model,
            timeout,
            agent,
        }
    }
}
```

3. Replace the enum and its `Display`:

```rust
#[derive(Debug)]
pub enum HttpError {
    /// The request failed; `cause` says how (`src/http_failure.rs`).
    Failed { url: String, cause: Cause },
    Unreadable { url: String, detail: String },
}

impl fmt::Display for HttpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HttpError::Failed { url, cause } => write!(f, "{}", cause.describe(url)),
            HttpError::Unreadable { url, detail } => write!(
                f,
                "{url:?} answered with something this could not read: {detail}"
            ),
        }
    }
}
```

4. Replace the `map_err` on `send_json`:

```rust
        let response = request.send_json(body).map_err(|e| HttpError::Failed {
            url: self.url.clone(),
            cause: Cause::from_ureq(e, self.timeout),
        })?;
```

5. In `mod tests`, `use super::*;` already brings `Cause` and `Duration` into scope
   through the new import and the existing `use std::time::Duration;`.

- [ ] **Step 8: Run the whole crate's tests and clippy**

Run: `cargo test -- --test-threads=1`
Expected: all PASS, including the two rewrite tests that already existed
(`a_non_2xx_response_is_a_failure`, `a_connection_that_refuses_is_named_by_address`).

Run: `cargo clippy --all-targets -- -D warnings`
Expected: no output. `src/stt/http.rs` does not use `http_failure` yet, and every
item of the module is used by the rewrite engine, so nothing is dead.

Run: `cargo fmt` and then `cargo fmt --check`; expected: no output. Re-run the
tests once after formatting.

- [ ] **Step 9: Commit**

```bash
git add src/http_failure.rs src/main.rs src/rewrite/http.rs
git commit -m "fix: the rewrite engine says what actually failed, and keeps what the server said" -m "A timeout, a refused connection, a refusal with a body and any other transport failure each produce their own message. The causes and their wording live in src/http_failure.rs so the transcriber can share them. The engine takes its time bound as a parameter so a test can make a timeout in milliseconds. Part of #52." -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

**Done when:** AC-1 to AC-7 hold for `src/rewrite/http.rs`, and the tests named in
steps 1 and 5 pass.

---

### Task 2: The transcriber

**Depends on:** Task 1.

**Files:**
- Modify: `src/stt/http.rs`

**Interfaces:**
- Consumes: `crate::http_failure::Cause` and `Cause::from_ureq`, `Cause::describe`
  from Task 1.
- Produces:
  ```rust
  pub enum HttpError {
      Failed { url: String, cause: Cause },
      Unreadable { url: String, detail: String },
      AudioUnreadable { path: String, detail: String },
  }
  impl HttpEngine {
      pub fn new(url: String, token: String, model: String, language: String) -> HttpEngine;
      pub fn with_timeout(url: String, token: String, model: String, language: String, timeout: Duration) -> HttpEngine;
  }
  ```

- [ ] **Step 1: Write the failing tests**

In `src/stt/http.rs`, `mod tests`:

1. Replace the two existing tests that match removed shapes.

   `a_non_2xx_response_is_a_failure_naming_the_status` — replace its `matches!` with:

   ```rust
        assert!(
            matches!(
                &error,
                EngineError::Http(HttpError::Failed {
                    cause: Cause::Answered { status: 500, .. },
                    ..
                })
            ),
            "got {error:?}"
        );
   ```

   `a_different_non_2xx_status_is_carried_through_unchanged` — the same with
   `status: 503`.

   `a_connection_that_refuses_is_named_by_address` — replace its final assertion
   with:

   ```rust
        let message = error.to_string();
        assert!(
            matches!(
                &error,
                EngineError::Http(HttpError::Failed {
                    cause: Cause::ConnectionRefused,
                    ..
                })
            ),
            "got {error:?}"
        );
        assert!(message.contains("127.0.0.1:1"), "got {message}");
        assert!(message.contains("refused the connection"), "got {message}");
   ```

   `a_missing_wav_file_is_refused_not_a_panic` — rename it
   `a_missing_wav_file_is_named_as_the_file_not_as_the_server` and replace its
   final assertion with:

   ```rust
        let message = error.to_string();
        assert!(
            matches!(
                &error,
                EngineError::Http(HttpError::AudioUnreadable { .. })
            ),
            "got {error:?}"
        );
        assert!(message.contains("stt-http-test-missing-"), "got {message}");
        assert!(message.contains("cannot read the recording"), "got {message}");
        assert!(!message.contains("cannot reach"), "got {message}");
        assert!(!message.contains("check the server is running"), "got {message}");
   ```

2. Add below `find_subslice`:

   ```rust
    /// Accepts one connection and reads it until the client gives up and
    /// closes: the server that never answers.
    fn never_answers() -> (String, std::thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let addr = listener.local_addr().expect("addr");
        let url = format!("http://{addr}/v1/audio/transcriptions");
        let handle = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept");
            let mut chunk = [0u8; 4096];
            while matches!(stream.read(&mut chunk), Ok(n) if n > 0) {}
        });
        (url, handle)
    }

    /// The message of the failure a transcription produced.
    fn failure_of(engine: &HttpEngine, tag: &str) -> String {
        engine
            .transcribe(&wav_path(tag), "")
            .expect_err("must fail")
            .to_string()
    }
   ```

3. Add at the end of `mod tests`:

   ```rust
    #[test]
    fn a_reply_that_never_comes_is_a_timeout_naming_the_bound() {
        let (url, handle) = never_answers();
        let engine = HttpEngine::with_timeout(
            url.clone(),
            String::new(),
            String::new(),
            "auto".to_string(),
            Duration::from_millis(200),
        );
        let message = failure_of(&engine, "a_reply_that_never_comes");
        assert!(message.contains(&url), "got {message}");
        assert!(
            message.contains("did not reply within 200 milliseconds"),
            "got {message}"
        );
        assert!(message.contains("wait and try again"), "got {message}");
        assert!(!message.contains("check the server is running"), "got {message}");
        handle.join().expect("server thread");
    }

    #[test]
    fn the_shipped_engine_holds_the_thirty_second_bound() {
        let engine = HttpEngine::new(
            "http://h/v1".to_string(),
            String::new(),
            String::new(),
            "auto".to_string(),
        );
        assert_eq!(engine.timeout, TIMEOUT);
        let message = Cause::TimedOut {
            bound: engine.timeout,
        }
        .describe("http://h/v1");
        assert!(message.contains("within 30 seconds"), "got {message}");
    }

    #[test]
    fn a_400_with_a_body_keeps_the_servers_explanation() {
        let (url, handle) = respond_once_with_status(
            "400 Bad Request",
            r#"{"error":"unknown model: no-such-model"}"#,
        );
        let engine = HttpEngine::new(url.clone(), String::new(), String::new(), "auto".to_string());
        let message = failure_of(&engine, "a_400_with_a_body");
        assert!(message.contains(&url), "got {message}");
        assert!(message.contains("answered with status 400"), "got {message}");
        assert!(message.contains("unknown model: no-such-model"), "got {message}");
        assert!(!message.contains("cannot reach"), "got {message}");
        assert!(!message.contains("check the server is running"), "got {message}");
        handle.join().expect("server thread");
    }

    #[test]
    fn a_refusal_with_no_body_says_the_server_gave_no_explanation() {
        let (url, handle) = respond_once_with_status("400 Bad Request", "");
        let engine = HttpEngine::new(url, String::new(), String::new(), "auto".to_string());
        let message = failure_of(&engine, "a_refusal_with_no_body");
        assert!(message.contains("status 400"), "got {message}");
        assert!(message.contains("the server gave no explanation"), "got {message}");
        handle.join().expect("server thread");
    }

    #[test]
    fn a_long_body_reaches_the_message_cut_to_its_bound() {
        // Leaked so the test double, which takes a `'static` body, can serve it.
        let body: &'static str = Box::leak("x".repeat(5000).into_boxed_str());
        let (url, handle) = respond_once_with_status("404 Not Found", body);
        let engine = HttpEngine::new(url, String::new(), String::new(), "auto".to_string());
        let message = failure_of(&engine, "a_long_body");
        assert!(message.contains(&format!("{}...", "x".repeat(300))), "got {message}");
        assert!(!message.contains(&"x".repeat(301)), "got {message}");
        handle.join().expect("server thread");
    }
   ```

- [ ] **Step 2: Run them and see them fail**

Run: `cargo test stt::http -- --test-threads=1`
Expected: compile errors: `Cause` not in scope, `with_timeout` not found, no field
`cause` on `Failed`, no variant `AudioUnreadable`.

- [ ] **Step 3: Change the transcriber**

In `src/stt/http.rs`:

1. Add `use crate::http_failure::Cause;` below `use super::{Engine, EngineError};`.
2. Update the module comment: replace "the error enum has three variants, not two,
   so a connection failure and a non-2xx response are told apart by message" with
   "the error says which of four causes failed (`src/http_failure.rs`)", keeping the
   reference to `tasks/16/DESIGN_16.md`.
3. Struct and constructors:

```rust
pub struct HttpEngine {
    url: String,
    token: String,
    model: String,
    language: String,
    /// The bound the agent below was built with, kept so a timeout's message can
    /// name it. `new` passes `TIMEOUT`; a test passes a shorter one.
    timeout: Duration,
    agent: ureq::Agent,
}

impl HttpEngine {
    pub fn new(url: String, token: String, model: String, language: String) -> HttpEngine {
        HttpEngine::with_timeout(url, token, model, language, TIMEOUT)
    }

    pub fn with_timeout(
        url: String,
        token: String,
        model: String,
        language: String,
        timeout: Duration,
    ) -> HttpEngine {
        // A dedicated agent, not the crate's shared default one: with pooling
        // disabled, this take's connection is never reused and never confused
        // with a later one, the same reason `rewrite::http::HttpEngine` does
        // this.
        let agent = ureq::AgentBuilder::new()
            .timeout(timeout)
            .max_idle_connections_per_host(0)
            .build();
        HttpEngine {
            url,
            token,
            model,
            language,
            timeout,
            agent,
        }
    }
}
```

4. Enum and `Display`:

```rust
#[derive(Debug)]
pub enum HttpError {
    /// The request failed; `cause` says how (`src/http_failure.rs`).
    Failed { url: String, cause: Cause },
    /// A 2xx response whose body does not parse as the expected JSON shape.
    Unreadable { url: String, detail: String },
    /// The take's own audio file could not be read, so no request was made.
    AudioUnreadable { path: String, detail: String },
}

impl std::fmt::Display for HttpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HttpError::Failed { url, cause } => write!(f, "{}", cause.describe(url)),
            HttpError::Unreadable { url, detail } => write!(
                f,
                "{url:?} answered with something this could not read: {detail}"
            ),
            HttpError::AudioUnreadable { path, detail } => {
                write!(f, "cannot read the recording {path:?}: {detail}")
            }
        }
    }
}
```

5. In `transcribe`, the audio read and the `send_bytes` mapping:

```rust
        let audio_bytes = std::fs::read(audio).map_err(|e| {
            EngineError::Http(HttpError::AudioUnreadable {
                path: audio.display().to_string(),
                detail: e.to_string(),
            })
        })?;
```

```rust
        let response = request.send_bytes(&body).map_err(|e| {
            EngineError::Http(HttpError::Failed {
                url: self.url.clone(),
                cause: Cause::from_ureq(e, self.timeout),
            })
        })?;
```

- [ ] **Step 4: Run tests, clippy and format**

Run: `cargo test -- --test-threads=1` — Expected: all PASS.
Run: `cargo clippy --all-targets -- -D warnings` — Expected: no output.
Run: `cargo fmt` then `cargo fmt --check` — Expected: no output; re-run the tests once.

- [ ] **Step 5: Commit**

```bash
git add src/stt/http.rs
git commit -m "fix: the transcriber says what actually failed, and names an unreadable recording as such" -m "Same causes and wording as the rewrite engine, through src/http_failure.rs. A failure to read the take's own audio file is no longer reported as an unreachable server. Part of #52." -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

**Done when:** AC-8 and AC-9 hold, and `grep -n "HttpError::Refused" src/stt/http.rs`
prints nothing.

---

### Task 3: Starting herdr

**Depends on:** nothing. Touches `src/delivery.rs` only.

**Files:**
- Modify: `src/delivery.rs`

**Interfaces:**
- Produces:
  ```rust
  pub enum DeliveryError {
      Rejected(String),
      NotFound { binary: String, path: String },     // unchanged
      NotExecutable { binary: String },
      StartFailed { binary: String, reason: String },
  }
  fn start_failure(binary: &str, error: &std::io::Error) -> DeliveryError; // private
  ```

- [ ] **Step 1: Write the failing tests**

In `src/delivery.rs`, `mod tests`, at the end:

```rust
    #[test]
    fn a_program_that_does_not_exist_keeps_the_path_sentence() {
        let deliverer = HerdrDeliverer::with_binary("herdr-voice-no-such-program");
        let error = deliverer.insert("w1:p2", "hello").expect_err("must fail");
        assert!(matches!(error, DeliveryError::NotFound { .. }), "got {error:?}");
        let message = error.to_string();
        assert!(message.contains("herdr-voice-no-such-program"), "got {message}");
        assert!(message.contains("not on the PATH"), "got {message}");
        assert!(message.contains("HERDR_BIN_PATH"), "got {message}");
    }

    #[test]
    fn start_failure_reads_the_kind_the_operating_system_gave() {
        use std::io::{Error, ErrorKind};
        assert!(matches!(
            start_failure("herdr", &Error::from(ErrorKind::NotFound)),
            DeliveryError::NotFound { .. }
        ));
        assert!(matches!(
            start_failure("herdr", &Error::from(ErrorKind::PermissionDenied)),
            DeliveryError::NotExecutable { .. }
        ));
        assert!(matches!(
            start_failure("herdr", &Error::other("Text file busy")),
            DeliveryError::StartFailed { .. }
        ));
    }

    #[test]
    fn another_failure_to_start_carries_the_operating_systems_text_and_not_the_path_sentence() {
        let error = start_failure(
            "/opt/herdr",
            &std::io::Error::other("Text file busy (os error 26)"),
        );
        assert_eq!(
            error,
            DeliveryError::StartFailed {
                binary: "/opt/herdr".to_string(),
                reason: "Text file busy (os error 26)".to_string(),
            }
        );
        let message = error.to_string();
        assert!(message.contains("/opt/herdr"), "got {message}");
        assert!(message.contains("Text file busy (os error 26)"), "got {message}");
        assert!(message.contains("try again"), "got {message}");
        assert!(!message.contains("PATH"), "got {message}");
    }

    #[test]
    fn a_found_program_that_cannot_be_run_says_so_and_not_the_path_sentence() {
        let message = DeliveryError::NotExecutable {
            binary: "/opt/herdr".to_string(),
        }
        .to_string();
        assert!(message.contains("/opt/herdr"), "got {message}");
        assert!(message.contains("was found"), "got {message}");
        assert!(message.contains("chmod +x"), "got {message}");
        assert!(message.contains("point HERDR_BIN_PATH at the herdr program itself"), "got {message}");
        assert!(!message.contains("not on the PATH"), "got {message}");
    }

    #[cfg(unix)]
    #[test]
    fn a_file_without_the_execute_bit_is_not_executable() {
        let recorder = Recorder::new("not-executable", "herdr-without-x-bit");
        // Written and left with the mode the umask gives it: no execute bit.
        std::fs::write(&recorder.script, "#!/bin/sh\nexit 0\n").expect("write");
        let binary = recorder.script.to_string_lossy().into_owned();
        let deliverer = HerdrDeliverer::with_binary(binary.clone());
        let error = deliverer.insert("w1:p2", "hello").expect_err("must fail");
        assert_eq!(error, DeliveryError::NotExecutable { binary }, "got {error:?}");
        assert!(!error.to_string().contains("not on the PATH"), "got {error}");
    }

    #[cfg(unix)]
    #[test]
    fn a_directory_named_as_the_program_is_not_executable() {
        let recorder = Recorder::new("directory", "unused");
        let binary = recorder.dir.to_string_lossy().into_owned();
        let deliverer = HerdrDeliverer::with_binary(binary.clone());
        let error = deliverer.insert("w1:p2", "hello").expect_err("must fail");
        assert_eq!(error, DeliveryError::NotExecutable { binary }, "got {error:?}");
        assert!(
            error
                .to_string()
                .contains("point HERDR_BIN_PATH at the herdr program itself"),
            "got {error}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_program_that_starts_and_fails_is_still_a_rejection() {
        let recorder = Recorder::new("rejects", "herdr-rejecting");
        std::fs::write(
            &recorder.script,
            "#!/bin/sh\necho '{\"error\":{\"code\":\"pane_not_found\",\"message\":\"gone\"}}'\nexit 1\n",
        )
        .expect("write");
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&recorder.script).expect("stat").permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&recorder.script, perms).expect("chmod");
        let deliverer = HerdrDeliverer::with_binary(recorder.script.to_string_lossy().into_owned());
        assert_eq!(
            deliverer.insert("w1:p2", "hello"),
            Err(DeliveryError::Rejected("pane_not_found".to_string()))
        );
    }
```

- [ ] **Step 2: Run them and see them fail**

Run: `cargo test delivery -- --test-threads=1`
Expected: compile errors: `start_failure` not found, no variants `NotExecutable`
and `StartFailed`.

- [ ] **Step 3: Implement**

In `src/delivery.rs`:

1. Extend the enum and its `Display`:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeliveryError {
    /// The code alone, extracted from herdr's structured refusal — see
    /// docs/evidence.md, "Delivering into a pane that is gone" — or the raw
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

impl std::fmt::Display for DeliveryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DeliveryError::Rejected(why) => write!(f, "{why}"),
            // Mirrors CommandError::NotFound (src/stt/command.rs:81-86).
            DeliveryError::NotFound { binary, path } => write!(
                f,
                "cannot run {binary:?}: it is not on the PATH this process has, which is \
                 {path:?}. Set HERDR_BIN_PATH to herdr's location, or start herdr from a shell \
                 where it is on the PATH"
            ),
            DeliveryError::NotExecutable { binary } => write!(
                f,
                "cannot run {binary:?}: the file was found but this process is not allowed to \
                 run it. Make it executable (on Unix, chmod +x), or point HERDR_BIN_PATH at the \
                 herdr program itself"
            ),
            DeliveryError::StartFailed { binary, reason } => write!(
                f,
                "cannot run {binary:?}: the operating system reported {reason:?}. This is often \
                 temporary: try again, and if it keeps happening, report that text"
            ),
        }
    }
}
```

2. Add below `extract_reason`:

```rust
/// What starting the program failed with, read from the kind the operating
/// system gave. Only `NotFound` means the program is not on the `PATH`;
/// `PermissionDenied` means it was found and cannot be run (no execute bit, or a
/// directory); anything else — a file still open for writing, exhausted
/// processes — carries the system's own text.
fn start_failure(binary: &str, error: &std::io::Error) -> DeliveryError {
    match error.kind() {
        std::io::ErrorKind::NotFound => DeliveryError::NotFound {
            binary: binary.to_string(),
            path: std::env::var("PATH").unwrap_or_default(),
        },
        std::io::ErrorKind::PermissionDenied => DeliveryError::NotExecutable {
            binary: binary.to_string(),
        },
        _ => DeliveryError::StartFailed {
            binary: binary.to_string(),
            reason: error.to_string(),
        },
    }
}
```

3. In `run`, replace the `Err(_)` arm (keep the comment):

```rust
            // herdr starts plugin commands with a minimal PATH — the same
            // reasoning src/stt/command.rs:78-86 states for the transcriber.
            Err(error) => Err(start_failure(&self.binary, &error)),
```

- [ ] **Step 4: Run tests, clippy and format**

Run: `cargo test -- --test-threads=1` — Expected: all PASS. If
`a_directory_named_as_the_program_is_not_executable` fails because this platform
reports a different kind for a directory, read what `start_failure` returned, and
write what the platform did into `tasks/52/RUN_52.md`; then change the test to
assert what the platform reports, and the review-focus line to match.
Run: `cargo clippy --all-targets -- -D warnings` — Expected: no output.
Run: `cargo fmt` then `cargo fmt --check` — Expected: no output.

- [ ] **Step 5: Commit**

```bash
git add src/delivery.rs
git commit -m "fix: a failure to start herdr says what the operating system reported" -m "Only a program that is not found is reported as missing from the PATH. A file without the execute bit, or a directory, is reported as found and not runnable; any other failure carries the system's own text. Part of #78." -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

**Done when:** AC-10 to AC-13 hold.

---

### Task 4: The gates

**Depends on:** Tasks 1, 2 and 3.

**Files:** none changed; `tasks/52/RUN_52.md` gets a note.

- [ ] **Step 1: The four gates, fresh**

```sh
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt --check
python3 scripts/check_manifest.py
```
Expected: all pass, clippy and fmt print nothing.

- [ ] **Step 2: The Windows dead-code check**

```sh
find src -name '*.rs' -exec sed -i '' \
  -e 's/#\[cfg(all(test, unix))\]/#[cfg(any())]/g' \
  -e 's/#\[cfg(unix)\]/#[cfg(any())]/g' \
  -e 's/#\[cfg(not(unix))\]/#[cfg(not(any()))]/g' \
  -e 's/#\[cfg(windows)\]/#[cfg(not(any()))]/g' {} +
cargo clippy --all-targets -- -D warnings
git checkout -- src
git status --short
```
Expected: clippy prints no warning; after `git checkout -- src`, `git status --short`
shows nothing under `src/`. If clippy reports dead code, the item it names is
constructed only on a Unix path: fix it in the task that owns it and commit there.

- [ ] **Step 3: Record the run**

Append to `tasks/52/RUN_52.md` under Notes: the gate results, and whether `src/stt/fetch.rs`
(#62) or an indicator test (#66) failed and was rerun. Commit:

```bash
git add tasks/52
git commit -m "docs: S1-S3 run artifacts for issues #52 and #78" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

**Done when:** AC-14 to AC-16 hold and the working tree is clean.

---

## Amendment 2026-09-30 — Task 5: the explanation on a 2xx response with no readable text

Implements the design amendment of the same date; covers AC-17 to AC-20. All global
constraints above hold, and the same four gates run before the commit.

### Task 5: `body_note`, and the two engines that append it

**Depends on:** Tasks 1 and 2 (both engines exist in their final shape).

**Files:**
- Modify: `src/http_failure.rs`
- Modify: `src/rewrite/http.rs`
- Modify: `src/stt/http.rs`

**Interfaces:**
- Consumes: `excerpt(text: &str) -> String` in `src/http_failure.rs` (private, same file).
- Produces: `pub fn body_note(text: &str) -> String` in `crate::http_failure`.

- [ ] **Step 1: Write the failing tests for `body_note`**

In `src/http_failure.rs`, add a stub above `fn bound_text` so the tests compile and
fail on their assertions:

```rust
/// What the server said in a response that had no readable text, for the detail
/// of an unreadable-answer message.
pub fn body_note(text: &str) -> String {
    let _ = text;
    String::new()
}
```

and, in `mod tests`:

```rust
    #[test]
    fn a_body_note_carries_the_excerpt() {
        assert_eq!(
            body_note("{\n  \"error\": \"no route\"\n}"),
            "the server said: { \"error\": \"no route\" }"
        );
    }

    #[test]
    fn an_empty_or_blank_body_is_noted_as_empty() {
        for text in ["", " \n\t "] {
            assert_eq!(body_note(text), "the body was empty", "for {text:?}");
        }
    }

    #[test]
    fn a_body_note_is_cut_like_any_excerpt() {
        let note = body_note(&"a".repeat(EXPLANATION_CHARS + 1));
        assert_eq!(
            note,
            format!("the server said: {}...", "a".repeat(EXPLANATION_CHARS))
        );
    }
```

- [ ] **Step 2: Run them and see them fail**

Run: `cargo test http_failure::tests::a_body_note http_failure::tests::an_empty_or_blank -- --test-threads=1`
Expected: three FAILED, for example `left: "", right: "the server said: …"`.
(If the filter form is rejected, run `cargo test http_failure -- --test-threads=1`.)

- [ ] **Step 3: Implement `body_note`**

Replace the stub:

```rust
/// What the server said in a response that had no readable text, for the detail
/// of an unreadable-answer message: the excerpt of the body, or a statement that
/// it was empty. The same excerpt and bound as a refusal's explanation.
pub fn body_note(text: &str) -> String {
    let said = excerpt(text);
    if said.is_empty() {
        "the body was empty".to_string()
    } else {
        format!("the server said: {said}")
    }
}
```

- [ ] **Step 4: Run the module's tests** — `cargo test http_failure -- --test-threads=1`; Expected: all PASS.

- [ ] **Step 5: Write the failing tests for the rewrite engine**

In `src/rewrite/http.rs`, `mod tests`, at the end:

```rust
    /// The message of a 2xx response whose body the engine could not use.
    fn unreadable_message(body: &'static str) -> (String, String) {
        let (url, handle) = respond_once(body);
        let engine = HttpEngine::new(url.clone(), String::new(), String::new());
        let error = engine.rewrite("x", "").expect_err("must fail");
        assert!(
            matches!(&error, HttpError::Unreadable { .. }),
            "got {error:?}"
        );
        handle.join().expect("server thread");
        (url, error.to_string())
    }

    #[test]
    fn a_2xx_error_object_reaches_the_message() {
        let (url, message) = unreadable_message(
            r#"{"error":"Unexpected endpoint or method. (POST /v1/chat/completionz)"}"#,
        );
        assert!(message.contains(&url), "got {message}");
        assert!(
            message.contains("answered with something this could not read"),
            "got {message}"
        );
        assert!(
            message.contains("no choices[0].message.content string in the response body"),
            "got {message}"
        );
        assert!(message.contains("the server said:"), "got {message}");
        assert!(
            message.contains("Unexpected endpoint or method."),
            "got {message}"
        );
    }

    #[test]
    fn a_2xx_body_without_choices_is_quoted() {
        let (_, message) = unreadable_message(r#"{"choices":[]}"#);
        assert!(
            message.contains(r#"the server said: {"choices":[]}"#),
            "got {message}"
        );
    }

    #[test]
    fn a_2xx_body_that_is_not_json_is_quoted() {
        let (_, message) = unreadable_message("not json at all");
        assert!(
            message.contains("the server said: not json at all"),
            "got {message}"
        );
    }

    #[test]
    fn a_2xx_body_that_is_empty_is_noted_as_empty() {
        let (_, message) = unreadable_message("");
        assert!(message.contains("the body was empty"), "got {message}");
        assert!(!message.contains("the server said"), "got {message}");
    }

    #[test]
    fn a_long_2xx_body_is_cut_to_its_bound() {
        // Leaked so the test double, which takes a `'static` body, can serve it.
        let body: &'static str = Box::leak("x".repeat(5000).into_boxed_str());
        let (_, message) = unreadable_message(body);
        assert!(message.contains(&format!("{}...", "x".repeat(300))), "got {message}");
        assert_eq!(message.matches(&"x".repeat(300)).count(), 1, "got {message}");
        assert!(!message.contains(&"x".repeat(301)), "got {message}");
    }

    #[test]
    fn a_successful_answer_longer_than_the_excerpt_is_read_whole() {
        let answer = "y".repeat(5000);
        let body: &'static str = Box::leak(
            format!(r#"{{"choices":[{{"message":{{"content":"{answer}"}}}}]}}"#).into_boxed_str(),
        );
        let (url, handle) = respond_once(body);
        let engine = HttpEngine::new(url, String::new(), String::new());
        assert_eq!(engine.rewrite("x", "").expect("text"), answer);
        handle.join().expect("server thread");
    }
```

- [ ] **Step 6: Run them and see them fail**

Run: `cargo test rewrite::http -- --test-threads=1`
Expected: the five quoting tests FAIL (for example `got "\"http://…\" answered with something this could not read: no choices[0].message.content string in the response body"`, missing `the server said:`); `a_successful_answer_longer_than_the_excerpt_is_read_whole` and the existing tests PASS.

- [ ] **Step 7: Change the rewrite engine**

In `src/rewrite/http.rs`: add `use std::io::Read;` with the other `std` imports (the trait `read_to_string` needs), change the import to `use crate::http_failure::{body_note, Cause};`, and replace the code from `let parsed` to the end of `rewrite`'s `match content` with:

```rust
        // Unbounded, as `into_json` was: `into_string` would stop at 10 MB.
        let mut text = String::new();
        response
            .into_reader()
            .read_to_string(&mut text)
            .map_err(|e| HttpError::Unreadable {
                url: self.url.clone(),
                detail: e.to_string(),
            })?;
        let parsed: serde_json::Value =
            serde_json::from_str(&text).map_err(|e| HttpError::Unreadable {
                url: self.url.clone(),
                detail: format!("{e}; {}", body_note(&text)),
            })?;

        let content = parsed
            .get("choices")
            .and_then(|c| c.get(0))
            .and_then(|c| c.get("message"))
            .and_then(|m| m.get("content"))
            .and_then(|c| c.as_str());

        match content {
            Some(text) => Ok(text.trim().to_string()),
            None => Err(HttpError::Unreadable {
                url: self.url.clone(),
                detail: format!(
                    "no choices[0].message.content string in the response body; {}",
                    body_note(&text)
                ),
            }),
        }
```

Inside the `Some(text)` arm the name `text` shadows the body text on purpose: it is
the content and the body is no longer needed there.

- [ ] **Step 8: Run the crate's tests** — `cargo test -- --test-threads=1`; Expected: all PASS, including the five quoting tests and the long successful answer.

- [ ] **Step 9: Write the failing tests for the transcriber**

In `src/stt/http.rs`, `mod tests`, at the end:

```rust
    /// The message of a 2xx response whose body the engine could not use.
    fn unreadable_message(body: &'static str, tag: &str) -> (String, String) {
        let (url, handle) = respond_once(body);
        let engine = HttpEngine::new(url.clone(), String::new(), String::new(), "auto".to_string());
        let error = engine
            .transcribe(&wav_path(tag), "")
            .expect_err("must fail");
        assert!(
            matches!(&error, EngineError::Http(HttpError::Unreadable { .. })),
            "got {error:?}"
        );
        handle.join().expect("server thread");
        (url, error.to_string())
    }

    #[test]
    fn a_2xx_error_object_reaches_the_message() {
        let (url, message) = unreadable_message(
            r#"{"error":"Unexpected endpoint or method. (POST /v1/audio/transcriptionz)"}"#,
            "a_2xx_error_object",
        );
        assert!(message.contains(&url), "got {message}");
        assert!(
            message.contains("answered with something this could not read"),
            "got {message}"
        );
        assert!(
            message.contains("no text string in the response body"),
            "got {message}"
        );
        assert!(message.contains("the server said:"), "got {message}");
        assert!(
            message.contains("Unexpected endpoint or method."),
            "got {message}"
        );
    }

    #[test]
    fn a_2xx_body_that_is_not_json_is_quoted() {
        let (_, message) = unreadable_message("not json at all", "a_2xx_not_json");
        assert!(
            message.contains("the server said: not json at all"),
            "got {message}"
        );
    }

    #[test]
    fn a_2xx_body_that_is_empty_is_noted_as_empty() {
        let (_, message) = unreadable_message("", "a_2xx_empty");
        assert!(message.contains("the body was empty"), "got {message}");
        assert!(!message.contains("the server said"), "got {message}");
    }

    #[test]
    fn a_long_2xx_body_is_cut_to_its_bound() {
        // Leaked so the test double, which takes a `'static` body, can serve it.
        let body: &'static str = Box::leak("x".repeat(5000).into_boxed_str());
        let (_, message) = unreadable_message(body, "a_2xx_long");
        assert!(message.contains(&format!("{}...", "x".repeat(300))), "got {message}");
        assert_eq!(message.matches(&"x".repeat(300)).count(), 1, "got {message}");
        assert!(!message.contains(&"x".repeat(301)), "got {message}");
    }

    #[test]
    fn a_successful_transcript_longer_than_the_excerpt_is_read_whole() {
        let answer = "y".repeat(5000);
        let body: &'static str =
            Box::leak(format!(r#"{{"text":"{answer}"}}"#).into_boxed_str());
        let (url, handle) = respond_once(body);
        let engine = HttpEngine::new(url, String::new(), String::new(), "auto".to_string());
        let text = engine
            .transcribe(&wav_path("a_successful_transcript_longer"), "")
            .expect("text");
        assert_eq!(text, answer);
        handle.join().expect("server thread");
    }
```

The existing `a_response_with_no_readable_text_is_unreadable` keeps its variant assertion and is
left as it is.

- [ ] **Step 10: Run them and see them fail**

Run: `cargo test stt::http -- --test-threads=1`
Expected: the four quoting tests FAIL on the missing `the server said:` or `the body was empty`; the long successful transcript and the existing tests PASS.

- [ ] **Step 11: Change the transcriber**

In `src/stt/http.rs`: add `use std::io::Read;` with the other `std` imports, change the import to `use crate::http_failure::{body_note, Cause};`, and replace the code from `let parsed` to the end of `transcribe` with:

```rust
        // Unbounded, as `into_json` was: `into_string` would stop at 10 MB.
        let mut text = String::new();
        response
            .into_reader()
            .read_to_string(&mut text)
            .map_err(|e| {
                EngineError::Http(HttpError::Unreadable {
                    url: self.url.clone(),
                    detail: e.to_string(),
                })
            })?;
        let parsed: serde_json::Value = serde_json::from_str(&text).map_err(|e| {
            EngineError::Http(HttpError::Unreadable {
                url: self.url.clone(),
                detail: format!("{e}; {}", body_note(&text)),
            })
        })?;

        match parsed.get("text").and_then(|t| t.as_str()) {
            Some(transcript) => Ok(transcript.to_string()),
            None => Err(EngineError::Http(HttpError::Unreadable {
                url: self.url.clone(),
                detail: format!(
                    "no text string in the response body; {}",
                    body_note(&text)
                ),
            })),
        }
```

- [ ] **Step 12: Run tests, clippy and format**

Run: `cargo test -- --test-threads=1` — Expected: all PASS.
Run: `cargo clippy --all-targets -- -D warnings` — Expected: no output.
Run: `cargo fmt` then `cargo fmt --check` — Expected: no output; re-run the tests once.

- [ ] **Step 13: Commit**

```bash
git add src/http_failure.rs src/rewrite/http.rs src/stt/http.rs
git commit -m "fix: a 2xx response with no readable text keeps what the server said" -m "A server can refuse with a 2xx status and an error in the body; LM Studio answers a wrong path that way. The engines read the body as text, and when it is not JSON or lacks the expected field, the message carries the same bounded, single-line excerpt a refusal's explanation does, or says the body was empty. Part of #52." -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

**Done when:** AC-17 to AC-20 hold.
