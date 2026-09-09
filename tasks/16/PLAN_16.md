# PLAN_16 — Recognition: the Whisper-compatible endpoint engine

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `[stt] engine = "http"` posts a take's WAV to a configured
Whisper-compatible endpoint and reads the transcript back, with four
distinct, named failure cases and no new network dependency.

**Architecture:** A new `src/stt/http.rs` module mirroring
`src/rewrite/http.rs`'s shape (a dedicated `ureq::Agent`, a fixed timeout,
one struct, one error enum), with a `multipart/form-data` body instead of
JSON and a three-variant error type instead of two, since this engine's
acceptance criteria ask for more granularity than the rewrite engine's do.

**Tech Stack:** Rust, `ureq` (already a dependency), `serde_json` (already a
dependency, for reading the response only — the request body is hand-built
multipart, not JSON).

**Spec:** `tasks/16/DESIGN_16.md`, against `tasks/16/AC_16.md` (S1: READY,
S2: READY after three rounds — `tasks/16/RUN_16.md`).

## Global Constraints

- No new HTTP client dependency. `ureq = { version = "2", features =
  ["json"] }` (`Cargo.toml:21`) already exists, added by issue #36.
- `[stt] url`, `[stt] token`, `[stt] http_model` are settled names, not
  provisional — confirmed directly with the owner (`tasks/16/RUN_16.md`).
- `[stt] language`'s `"auto"` is omitted from the request, not sent
  literally — the opposite of `command::render`'s treatment of the same
  value, and stated that way on purpose (`DESIGN_16.md` §2).
- No new logging of the transcript, the bias string, or the audio path
  anywhere — the standing rule since issue #21.
- Test first: write the failing test, confirm it fails for the stated
  reason, then implement.
- One commit per task.

---

### Task 1: `EngineError::NotConfigured` gains a payload

**Files:**
- Modify: `src/stt.rs:19-32` (`EngineError`), `:40` (`EXAMPLE`, renamed),
  `:44-68` (`Display`), `:121` (the `"command"` empty-argv check)
- Modify: `src/stt/command.rs:126` (the defensive fallback site)

**Before starting:** this task is pure refactor — no new behavior, the
`"command"` case's rendered message must stay byte-for-byte identical. Read
`src/stt.rs:234-245` (`a_command_engine_with_nothing_to_run_names_the_key_and_shows_one`)
first; it is the test that proves this.

**Interfaces:**
- Produces: `EngineError::NotConfigured { engine: &'static str, key:
  &'static str, example: &'static str }`
- Consumed by: Task 4 (the `"http"` empty-`url` case)

- [ ] **Step 1: Confirm the existing test passes before touching anything**

Run: `cargo test --bin herdr-voice stt::tests::a_command_engine_with_nothing_to_run_names_the_key_and_shows_one`
Expected: PASS (baseline).

- [ ] **Step 2: Rename `EXAMPLE` to `COMMAND_EXAMPLE`, add `HTTP_EXAMPLE`**

```rust
const COMMAND_EXAMPLE: &str = r#"command = ["whisper-cli", "-m", "{model}", "-f", "{audio}", "-l", "{language}", "-np", "-nt"]"#;
const HTTP_EXAMPLE: &str = r#"url = "http://127.0.0.1:8080/v1/audio/transcriptions""#;
```

- [ ] **Step 3: Widen `NotConfigured` and its `Display` arm**

```rust
NotConfigured {
    engine: &'static str,
    key: &'static str,
    example: &'static str,
},
```

```rust
EngineError::NotConfigured { engine, key, example } => write!(
    f,
    "[stt] engine is {engine:?} but [stt] {key} is empty, so there is nothing \
     to run. For example:\n  {example}"
),
```

- [ ] **Step 4: Update both existing construction sites**

`src/stt.rs:121` (inside `resolve_with`'s `"command"` arm, where `stt.command.is_empty()`):

```rust
return Err(EngineError::NotConfigured {
    engine: "command",
    key: "command",
    example: COMMAND_EXAMPLE,
});
```

`src/stt/command.rs:126`:

```rust
let (program, arguments) = rendered.split_first().ok_or(EngineError::NotConfigured {
    engine: "command",
    key: "command",
    example: COMMAND_EXAMPLE,
})?;
```

Fix the `NotBuilt` arm's reference to the renamed constant (`src/stt.rs:51`,
`{EXAMPLE}` → `{COMMAND_EXAMPLE}`) and the `NotConfigured` `Display` arm's
own former direct reference (already replaced by the `example` field above).

- [ ] **Step 5: Run to verify nothing broke**

Run: `cargo test --bin herdr-voice stt::`
Expected: PASS, including
`a_command_engine_with_nothing_to_run_names_the_key_and_shows_one` unchanged
in substance (it still asserts `"[stt] command"` and `"whisper-cli"` are in
the message; only the construction syntax behind that message changed).

- [ ] **Step 6: Commit**

```bash
git add src/stt.rs src/stt/command.rs
git commit -m "Give EngineError::NotConfigured a payload, so http and command name themselves correctly"
```

---

### Task 2: `[stt]`'s three new configuration keys

**Files:**
- Modify: `src/config.rs:53-63` (`Stt`), `:88-97` (`impl Default for Stt`)

**Before starting:** read `src/config.rs:56`'s doc comment on `engine`
("`candle`, `http` or `command`. The first two are not built yet.") — it
needs correcting once this issue lands; do it in this task, since it is the
same struct this task already touches.

**Interfaces:**
- Produces: `config::Stt { model, engine, language, command, url, token,
  http_model }`, all new fields defaulting to an empty string.

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn an_stt_table_with_only_engine_set_to_http_keeps_the_other_defaults() {
    let toml = r#"
        [stt]
        engine = "http"
    "#;
    let config: Config = toml::from_str(toml).expect("parse");
    assert_eq!(config.stt.engine, "http");
    assert_eq!(config.stt.url, "");
    assert_eq!(config.stt.token, "");
    assert_eq!(config.stt.http_model, "");
}
```

(If an existing helper in this test module already parses a TOML fragment
into `Config`, reuse it instead of `toml::from_str` directly.)

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test --bin herdr-voice config::tests::an_stt_table_with_only_engine_set_to_http_keeps_the_other_defaults`
Expected: FAIL to compile — `Stt` has no `url`/`token`/`http_model` fields
yet.

- [ ] **Step 3: Add the fields**

```rust
pub struct Stt {
    pub model: String,
    /// `candle`, `http` or `command`. `candle` is not built yet (issue #15).
    pub engine: String,
    pub language: String,
    pub command: Vec<String>,
    /// The endpoint address for `engine = "http"`. Empty means unconfigured.
    pub url: String,
    /// An optional bearer token for `engine = "http"`. Empty means no
    /// `Authorization` header is sent.
    pub token: String,
    /// The model name sent in the request for `engine = "http"`. Separate
    /// from `model`, which stays a local-model identifier for `engine =
    /// "command"` and is not read by the http engine.
    pub http_model: String,
}
```

Update `impl Default for Stt` with `url: String::new(), token: String::new(),
http_model: String::new(),`.

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test --bin herdr-voice config::`
Expected: PASS, including every pre-existing `config` test.

- [ ] **Step 5: Commit**

```bash
git add src/config.rs
git commit -m "Add [stt]'s new keys: url, token, http_model"
```

---

### Task 3: `stt::http` — the endpoint engine

**Files:**
- Create: `src/stt/http.rs`
- Modify: `src/stt.rs`: `pub mod http;` beside the existing `pub mod
  command;`; `EngineError` (`:22-34`) gains a sixth variant, `Http(http::
  HttpError)`, alongside `Command(command::CommandError)`; the `Display`
  impl (`:44-68`) gains a matching arm, `EngineError::Http(e) => write!(f,
  "{e}")`, the same one-line delegation `Command(e)` already uses.

**Before starting:** read `src/rewrite/http.rs` in full — this task ports
its `HttpEngine` shape (a dedicated `ureq::Agent`, pooling disabled, a fixed
30-second timeout, the scratch-`TcpListener` test double
`respond_once`/`respond_once_with_status`) closely enough to copy the test
double verbatim. Two differences from that precedent: the request body is
`multipart/form-data`, not JSON, and the error enum has three variants, not
two — `Refused` for a transport-level failure (`ureq::Error::Transport`),
`Failed { status, .. }` for a non-2xx response (`ureq::Error::Status`), and
`Unreadable` for a 2xx response whose body does not parse as expected.

**Interfaces:**
- Consumes: `ureq` (already a dependency)
- Produces: `pub struct HttpEngine { .. }` with `pub fn new(url: String,
  token: String, model: String, language: String) -> HttpEngine`; `pub enum
  HttpError { Refused { url: String, detail: String }, Failed { url: String,
  status: u16, detail: String }, Unreadable { url: String, detail: String }
  }` with a `Display` impl naming the address and what to check for each
  variant; `impl Engine for HttpEngine { fn transcribe(&self, audio: &Path,
  bias: &str) -> Result<String, EngineError> }` (wraps `HttpError` into
  `EngineError::Http`).
- Consumed by: Task 4 (`stt::resolve_with`)

- [ ] **Step 1: Write the failing tests**

Copy `respond_once`/`respond_once_with_status`/`find_subslice` from
`src/rewrite/http.rs`'s test module verbatim (same signatures; the double
only inspects the raw request as a string, so it needs no multipart parser
of its own — the same reason `rewrite::http`'s own tests never construct a
real chat-completions server, only a stand-in that hands back a fixed body).

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    // respond_once / respond_once_with_status / find_subslice: copied from
    // src/rewrite/http.rs, unchanged.

    fn wav_path() -> std::path::PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!("stt-http-test-{}.wav", std::process::id()));
        std::fs::write(&path, b"RIFF....WAVEfmt ").expect("write fixture wav");
        path
    }

    #[test]
    fn the_transcript_is_read_back() {
        let (url, handle) = respond_once(r#"{"text":"pull request, not \"pulley quest\""}"#);
        let engine = HttpEngine::new(url, String::new(), String::new(), "auto".to_string());
        let text = engine.transcribe(&wav_path(), "").expect("text");
        assert_eq!(text, "pull request, not \"pulley quest\"");
        let request = handle.join().expect("server thread");
        assert!(request.contains("multipart/form-data"), "got {request}");
        assert!(
            request.contains("name=\"file\""),
            "the file field must be present: {request}"
        );
    }

    #[test]
    fn the_model_field_is_sent_when_configured() {
        let (url, handle) = respond_once(r#"{"text":"x"}"#);
        let engine = HttpEngine::new(url, String::new(), "whisper-1".to_string(), "auto".to_string());
        engine.transcribe(&wav_path(), "").expect("text");
        let request = handle.join().expect("server thread");
        assert!(
            request.contains("name=\"model\"") && request.contains("whisper-1"),
            "got {request}"
        );
    }

    #[test]
    fn the_model_field_is_absent_when_not_configured() {
        let (url, handle) = respond_once(r#"{"text":"x"}"#);
        let engine = HttpEngine::new(url, String::new(), String::new(), "auto".to_string());
        engine.transcribe(&wav_path(), "").expect("text");
        let request = handle.join().expect("server thread");
        assert!(!request.contains("name=\"model\""), "got {request}");
    }

    #[test]
    fn the_language_field_is_sent_unless_auto() {
        let (url, handle) = respond_once(r#"{"text":"x"}"#);
        let engine = HttpEngine::new(url, String::new(), String::new(), "ru".to_string());
        engine.transcribe(&wav_path(), "").expect("text");
        let request = handle.join().expect("server thread");
        assert!(
            request.contains("name=\"language\"") && request.contains("\r\nru\r\n"),
            "got {request}"
        );
    }

    #[test]
    fn the_language_field_is_absent_when_auto() {
        let (url, handle) = respond_once(r#"{"text":"x"}"#);
        let engine = HttpEngine::new(url, String::new(), String::new(), "auto".to_string());
        engine.transcribe(&wav_path(), "").expect("text");
        let request = handle.join().expect("server thread");
        assert!(!request.contains("name=\"language\""), "got {request}");
    }

    #[test]
    fn the_prompt_field_carries_the_bias_string_when_non_empty() {
        let (url, handle) = respond_once(r#"{"text":"x"}"#);
        let engine = HttpEngine::new(url, String::new(), String::new(), "auto".to_string());
        engine
            .transcribe(&wav_path(), "recent terms: pull request")
            .expect("text");
        let request = handle.join().expect("server thread");
        assert!(
            request.contains("name=\"prompt\"") && request.contains("recent terms: pull request"),
            "got {request}"
        );
    }

    #[test]
    fn an_empty_bias_sends_no_prompt_field() {
        let (url, handle) = respond_once(r#"{"text":"x"}"#);
        let engine = HttpEngine::new(url, String::new(), String::new(), "auto".to_string());
        engine.transcribe(&wav_path(), "").expect("text");
        let request = handle.join().expect("server thread");
        assert!(!request.contains("name=\"prompt\""), "got {request}");
    }

    #[test]
    fn an_empty_token_sends_no_authorization_header() {
        let (url, handle) = respond_once(r#"{"text":"x"}"#);
        let engine = HttpEngine::new(url, String::new(), String::new(), "auto".to_string());
        engine.transcribe(&wav_path(), "").expect("text");
        let request = handle.join().expect("server thread");
        assert!(!request.to_lowercase().contains("authorization"), "got {request}");
    }

    #[test]
    fn a_token_is_sent_as_a_bearer_header() {
        let (url, handle) = respond_once(r#"{"text":"x"}"#);
        let engine = HttpEngine::new(url, "secret-token".to_string(), String::new(), "auto".to_string());
        engine.transcribe(&wav_path(), "").expect("text");
        let request = handle.join().expect("server thread");
        assert!(request.contains("Bearer secret-token"), "got {request}");
    }

    #[test]
    fn a_non_2xx_response_is_a_failure_naming_the_status() {
        let (url, handle) = respond_once_with_status("500 Internal Server Error", r#"{"error":"model not loaded"}"#);
        let engine = HttpEngine::new(url.clone(), String::new(), String::new(), "auto".to_string());
        let error = engine.transcribe(&wav_path(), "").expect_err("must fail");
        assert!(matches!(error, EngineError::Http(HttpError::Failed { status: 500, .. })), "got {error:?}");
        handle.join().expect("server thread");
    }

    #[test]
    fn a_response_with_no_readable_text_is_unreadable() {
        let (url, handle) = respond_once(r#"{"choices":[]}"#);
        let engine = HttpEngine::new(url, String::new(), String::new(), "auto".to_string());
        let error = engine.transcribe(&wav_path(), "").expect_err("must fail");
        assert!(matches!(error, EngineError::Http(HttpError::Unreadable { .. })), "got {error:?}");
        handle.join().expect("server thread");
    }

    #[test]
    fn a_connection_that_refuses_is_named_by_address() {
        let engine = HttpEngine::new(
            "http://127.0.0.1:1/v1/audio/transcriptions".to_string(),
            String::new(),
            String::new(),
            "auto".to_string(),
        );
        let error = engine.transcribe(&wav_path(), "").expect_err("must fail");
        assert!(matches!(error, EngineError::Http(HttpError::Refused { .. })), "got {error:?}");
    }
}
```

- [ ] **Step 2: Run to verify the tests fail**

Run: `cargo test --bin herdr-voice stt::http`
Expected: FAIL to compile — the module does not exist yet.

- [ ] **Step 3: Give `EngineError` its sixth variant, then implement
  `HttpEngine`, `HttpError`, the multipart body**

In `src/stt.rs`, add the variant and the `Display` arm before writing
`src/stt/http.rs` — the new module's own tests construct
`EngineError::Http(...)` and cannot compile without it:

```rust
// In EngineError, alongside Command(command::CommandError):
Http(http::HttpError),
```

```rust
// In the Display impl, alongside EngineError::Command(e) => write!(f, "{e}"):
EngineError::Http(e) => write!(f, "{e}"),
```

`pub mod http;` also needs adding to `src/stt.rs` at this point (not
deferred to a later step) — the variant's own type, `http::HttpError`,
does not resolve until the module is declared.

At the top of `src/stt/http.rs`, the same import `src/stt/command.rs:11` uses:

```rust
use super::{Engine, EngineError};
```

```rust
pub struct HttpEngine {
    url: String,
    token: String,
    model: String,
    language: String,
    agent: ureq::Agent,
}

impl HttpEngine {
    pub fn new(url: String, token: String, model: String, language: String) -> HttpEngine {
        let agent = ureq::AgentBuilder::new()
            .timeout(TIMEOUT) // same 30-second constant as src/rewrite/http.rs
            .max_idle_connections_per_host(0)
            .build();
        HttpEngine { url, token, model, language, agent }
    }
}

#[derive(Debug)]
pub enum HttpError {
    Refused { url: String, detail: String },
    Failed { url: String, status: u16, detail: String },
    Unreadable { url: String, detail: String },
}
```

`Display` for `HttpError`: `Refused` — `"cannot reach {url:?}: {detail}; \
check the server is running and the address is correct"`; `Failed` —
`"{url:?} answered with status {status}: {detail}"`; `Unreadable` —
`"{url:?} answered with something this could not read: {detail}"`.

Building the multipart body: a fixed boundary string (e.g.
`"----herdr-voice-boundary"`), one part per field in this order — `file`
(binary, `Content-Type: application/octet-stream`, the WAV bytes read from
`audio`), `model` (text, only when `self.model` is non-empty), `language`
(text, only when `self.language != "auto"`), `prompt` (text, only when
`bias` is non-empty) — each part as:

```
--<boundary>\r\n
Content-Disposition: form-data; name="<field>"[; filename="take.wav"]\r\n
[Content-Type: application/octet-stream\r\n]
\r\n
<bytes>\r\n
```

closed with `--<boundary>--\r\n`. Set the request's `Content-Type` header to
`multipart/form-data; boundary=<boundary>` and send the assembled bytes with
`request.send_bytes(&body)` (check `ureq`'s current documentation for the
exact method name on the pinned version if `send_bytes` has moved — this
plan fixes the byte layout, not `ureq`'s own API surface).

Map `ureq::Error::Transport(t)` to `HttpError::Refused { url, detail: t.to_string() }`
and `ureq::Error::Status(code, _)` to `HttpError::Failed { url, status: code,
detail: format!("server answered with status {code}") }`. On a 2xx response,
parse the body as JSON and read `text` as a string; anything else (parse
failure, missing field, non-string) is `HttpError::Unreadable`.

`impl Engine for HttpEngine`'s `transcribe` reads the WAV bytes from `audio`
with `std::fs::read`, maps any read failure the same way a missing take
would be reported elsewhere in this codebase (an `io::Error` wrapped into
`HttpError::Refused` with the path in `detail` is acceptable — this path is
not expected to fail in practice, since the caller always has a real take
file, but must not panic).

- [ ] **Step 4: Run to verify the tests pass**

Run: `cargo test --bin herdr-voice stt::http`
Expected: PASS, all twelve tests.

- [ ] **Step 5: Run the whole suite**

`pub mod http;` and the `Http` variant were already added in Step 3, so
this step is the whole-crate check, not a registration step.

Run: `cargo test`
Expected: PASS, no regressions elsewhere.

- [ ] **Step 6: Commit**

```bash
git add src/stt/http.rs src/stt.rs
git commit -m "Add stt::http, the Whisper-compatible endpoint recognition engine"
```

---

### Task 4: Wire `"http"` into `stt::resolve_with`, and clean up what it leaves stale

**Files:**
- Modify: `src/stt.rs:106-132` (`resolve_with`), `:204-217` (the
  `the_unbuilt_engines_say_so_and_name_the_one_that_works` test)
- Modify: `src/config.rs:56` (the stale `engine` doc comment, if Task 2 did
  not already correct it)
- Modify: `src/doctor.rs:561-569` (the `"engine-http"` block inside
  `the_engine_line_names_what_resolve_reports_for_each_engine` — see Step 6;
  this is a real, necessary edit, not an optional cleanup)

**Before starting:** read `DESIGN_16.md` §4 for the exact match arms.

**Interfaces:**
- Consumes: `stt::http::HttpEngine` (Task 3)

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn http_with_an_empty_url_names_the_key() {
    let error = match resolve(&stt("http", &[]), &nowhere()) {
        Err(error) => error,
        Ok(_) => panic!("an empty url must not resolve"),
    };
    let message = error.to_string();
    assert!(message.contains("[stt] url"), "got {message}");
}

#[test]
fn http_with_a_url_resolves() {
    let mut config = stt("http", &[]);
    config.url = "http://127.0.0.1:1234/v1/audio/transcriptions".to_string();
    assert!(resolve(&config, &nowhere()).is_ok());
}
```

(`stt(...)`'s test helper, `src/stt.rs:192-198`, builds a `Stt` via `..
Stt::default()` — set `.url` on the returned value directly, the helper
itself needs no change.)

- [ ] **Step 2: Run to verify the tests fail**

Run: `cargo test --bin herdr-voice stt::tests::http_with_an_empty_url_names_the_key stt::tests::http_with_a_url_resolves`
Expected: FAIL — `"http"` still returns `NotBuilt` unconditionally.

- [ ] **Step 3: Replace the `"http"` arm**

```rust
"http" if stt.url.is_empty() => Err(EngineError::NotConfigured {
    engine: "http",
    key: "url",
    example: HTTP_EXAMPLE,
}),
"http" => Ok(Box::new(http::HttpEngine::new(
    stt.url.clone(),
    stt.token.clone(),
    stt.http_model.clone(),
    stt.language.clone(),
))),
```

- [ ] **Step 4: Narrow the unbuilt-engines loop test**

`the_unbuilt_engines_say_so_and_name_the_one_that_works`'s loop,
`[("candle", "#15"), ("http", "#16")]`, becomes `[("candle", "#15")]` — the
only engine this test's premise ("must not resolve: it is not built") still
holds for.

- [ ] **Step 5: Run to verify everything passes**

Run: `cargo test --bin herdr-voice stt::`
Expected: PASS, including the two new tests and the narrowed loop test.

- [ ] **Step 6: Fix the one pre-existing `doctor` test this wiring breaks**

`DESIGN_16.md` §6's claim — that `engine_finding_from`'s generic delegation
means `doctor.rs` itself needs no code change — holds for `doctor.rs`'s
production code, but not for one of its existing *tests*:
`the_engine_line_names_what_resolve_reports_for_each_engine`'s `"engine-
http"` block (`src/doctor.rs:561-569`) builds a plain `Stt::default()` with
`engine = "http"` (so `url` is empty) and asserts
`finding.detail.contains("#16")` — text only `EngineError::NotBuilt`'s
`Display` produces. Once `"http"` with an empty `url` returns
`NotConfigured` instead, that assertion fails.

Replace the `"#16"` assertion with one matching what `NotConfigured` now
says:

```rust
let models = scratch_models("engine-http");
let http = config::Stt {
    engine: "http".to_string(),
    ..config::Stt::default()
};
let finding = engine_and_model_findings(&http, &models).0;
assert_eq!(finding.state, State::Missing);
assert!(finding.detail.contains("http"), "got {finding:?}");
assert!(finding.detail.contains("url"), "got {finding:?}");
```

Run: `cargo test --bin herdr-voice doctor::`
Expected: PASS, including this test in its corrected form and every other
`doctor` test unchanged.

- [ ] **Step 7: Run the whole suite**

Run: `cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt --check && python3 scripts/check_manifest.py`
Expected: all four green.

- [ ] **Step 8: Commit**

```bash
git add src/stt.rs src/config.rs src/doctor.rs
git commit -m "Wire [stt] engine = \"http\" into resolve_with"
```

---

### Task 5: S4 — review the diff before a pull request exists

Invoke `superpowers:requesting-code-review` against the full diff on
`feat/16-http-recognition` since it diverged from `main`, naming this plan
and `DESIGN_16.md` as what it should satisfy. Explicitly ask the reviewer to
mutation-test: each of `HttpError`'s three variants (does a mutation forcing
the wrong variant get caught by a distinct test); the `model`/`language`/
`prompt` fields' presence rules (each one's "send when" and "omit when"
condition, independently); and that no new code path logs the transcript,
the bias string or the audio path.

- [ ] Run the review.
- [ ] Append a `## Gate S4` block to `tasks/16/RUN_16.md` with the verdict;
  if not `READY`, address the findings and re-run before proceeding.
- [ ] Commit: `git add tasks/16/RUN_16.md && git commit -m "Record the S4 diff review for #16"`

---

### Task 6: S5 — verify by test suite, then by a spoken take against a real endpoint

- [ ] Run `cargo test`, reading the whole output; then `cargo clippy
  --all-targets -- -D warnings`, `cargo fmt --check`,
  `python3 scripts/check_manifest.py`.
- [ ] Append a section to `docs/evidence.md` stating what the test suite
  establishes (all four request fields' presence rules, all three error
  variants, the settled configuration keys) and what it does not (no test
  here posts to a real Whisper-compatible server).
- [ ] If a real endpoint is reachable in this session the way Ollama was for
  issue #36 (`docs/evidence.md`, "The rewrite stage, against a real local
  server and a real program"), drive a real take through `stt::http::HttpEngine`
  against it and record the platform, what was configured, and the result —
  the same way that entry was written. If no such endpoint is reachable, say
  so plainly and leave the live measurement pending.
- [ ] Append a `## Gate S5` block to `tasks/16/RUN_16.md` with the verdict.
- [ ] Commit: `git add docs/evidence.md tasks/16/RUN_16.md && git commit -m "Verify the http recognition engine by test suite"`

---

## Coverage

AC-1 → Task 4. AC-2 → Task 1, Task 4. AC-3 → Task 3
(`the_transcript_is_read_back`, `the_model_field_is_sent_when_configured`,
`the_model_field_is_absent_when_not_configured`,
`the_language_field_is_sent_unless_auto`,
`the_language_field_is_absent_when_auto`). AC-3a → Task 3
(`the_prompt_field_carries_the_bias_string_when_non_empty`,
`an_empty_bias_sends_no_prompt_field`). AC-4 → Task 3
(`an_empty_token_sends_no_authorization_header`,
`a_token_is_sent_as_a_bearer_header`). AC-5 → Task 3 (the three
`HttpError`-variant tests). AC-6 → Task 1, Task 3 (every `Display` arm names
what to check). AC-7 → Task 3 (the fixed timeout, ported unchanged from
`rewrite::http`). AC-8 → Task 3 (the scratch listener; no test opens a real
connection). AC-9 → Task 4, Step 6 (no code change needed, confirmed by
running `doctor`'s existing tests).

## What could not be cut into a checkable task

A live measurement against a real Whisper-compatible server (the
"Done when" section of the issue does not name this the way #36's did, and
`AC_16.md`'s own "Out of scope" section says so explicitly) is not a task
this plan schedules as required — Task 6 attempts it opportunistically, the
way #36's S5 did when Ollama happened to be running, but does not block
closing this issue on it being available.
