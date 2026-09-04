# PLAN_36 — Rewrite: wire the stage into the pipeline, with the http and command engines

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A finished take's transcript passes through a configured rewrite
engine — `http` (OpenAI-compatible, cloud or local) or `command` (an
arbitrary external program) — before delivery, with `off`/`agent`/anything
unrecognised treated uniformly as "no engine available": deliver the
transcript unchanged, tell the person once per daemon lifetime.

**Architecture:** A new `src/rewrite.rs` module, mirroring `src/stt.rs`'s
shape (one trait, resolved once at daemon start, one submodule per engine).
`daemon::Runtime` gains the resolved engine, a skip-heuristic flag, and an
atomic "already told" flag; `daemon::transcribe` calls the rewrite step
between recognition and delivery.

**Tech Stack:** Rust, `ureq` (new, blocking HTTP client), `serde_json`
(already a dependency), `std::process::Command`, `std::sync::atomic::AtomicBool`.

**Spec:** `tasks/36/DESIGN_36.md`, against `tasks/36/AC_36.md` (S1: READY, S2:
READY — `tasks/36/RUN_36.md`).

## Global Constraints

- No async runtime. `ureq` is blocking by design; do not add `tokio` or any
  other executor (`docs/decisions.md`, 2026-08-24, #3).
- New `[rewrite]` configuration keys (`url`, `token`, `model`, `command`,
  `skip_if_plain`) are provisional, the same footing `[stt] command` was
  introduced on (`docs/decisions.md`, 2026-08-25, #13) — say so in the
  `docs/decisions.md` entry Task 1 adds.
- `{transcript}` is force-appended to `[rewrite] command`'s argument list when
  the placeholder is absent; `{bias}` never is — mirrors `{audio}`/`{prompt}`
  in `stt::command` and #26 exactly (`DESIGN_36.md` §7).
- Nothing in this diff logs the transcript, the bias string, or a rewritten
  transcript beyond what a journal line already reports for other stages —
  no new leak of conversation content (the standing rule since #21's AC-9).
- Test first: write the failing test, confirm it fails for the stated reason,
  then implement.
- One commit per task.
- Before Task 7 (the daemon wiring): check whether issue #26's pull request
  has merged to `origin/main`. If it has, `git fetch && git rebase
  origin/main` first, and thread the rewrite step's `collected.bias` access
  alongside whatever parameter #26 already added to `daemon::transcribe` for
  recognition — do not add a second, redundant binding of `take_bias`'s
  return.

---

### Task 1: Add the `ureq` dependency, and record the decision

**Files:**
- Modify: `Cargo.toml:15-20`
- Modify: `docs/decisions.md` (append one row)

- [ ] **Step 1: Add the dependency**

In `Cargo.toml`, after `toml = "1"` (line 20), add:

```toml
ureq = "2"
```

- [ ] **Step 2: Confirm it builds**

Run: `cargo build`
Expected: succeeds, `Cargo.lock` gains `ureq` and its transitive dependencies
(`rustls` and friends — `ureq`'s default TLS backend, no native OpenSSL
dependency).

- [ ] **Step 3: Append one row to `docs/decisions.md`'s table**, next to the
  2026-08-24, #3 row about asynchronous runtimes:

```markdown
| `ureq` is the HTTP client for `[rewrite] engine = "http"`, blocking, default TLS backend (`rustls`) | This repository keeps asynchronous runtimes out until a task cannot be done without one, and posting a take's transcript to a configured endpoint is that task. `ureq` needs no executor and no native TLS dependency, so it adds one crate rather than a runtime | 2026-09-04, #36 |
```

- [ ] **Step 4: Commit**

```bash
git add Cargo.toml Cargo.lock docs/decisions.md
git commit -m "Add ureq, a blocking HTTP client, for the rewrite http engine"
```

---

### Task 2: `[rewrite]`'s new configuration keys

**Files:**
- Modify: `src/config.rs:67-70` (`Rewrite` struct), `:79-84` (`impl Default for Rewrite`)

**Before starting:** read `src/config.rs:56-66` (`Stt`'s `command` field and
its doc comment) for the exact style new fields should follow, and the
existing `a_partial_context_table_keeps_the_other_defaults`-style test
(search `config.rs` for `partial` to find it) for the test pattern this
task's own test follows.

**Interfaces:**
- Produces: `config::Rewrite { engine, agent, url, token, model, command,
  skip_if_plain }`, all with defaults such that an absent `[rewrite]` table
  behaves exactly as it does today (`engine = "agent"`, `agent = "auto"`,
  everything new empty or `true`).

- [ ] **Step 1: Write the failing test**

Add to `config.rs`'s test module:

```rust
#[test]
fn a_rewrite_table_with_only_engine_set_keeps_the_other_defaults() {
    let toml = r#"
        [rewrite]
        engine = "http"
    "#;
    let config: Config = toml::from_str(toml).expect("parse");
    assert_eq!(config.rewrite.engine, "http");
    assert_eq!(config.rewrite.agent, "auto");
    assert_eq!(config.rewrite.url, "");
    assert_eq!(config.rewrite.token, "");
    assert_eq!(config.rewrite.model, "");
    assert!(config.rewrite.command.is_empty());
    assert!(config.rewrite.skip_if_plain);
}
```

(If the existing test module already has a helper that parses a TOML
fragment into `Config` under a different name, reuse it instead of
`toml::from_str` directly — check for one before adding a second.)

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test --bin herdr-voice config::tests::a_rewrite_table_with_only_engine_set_keeps_the_other_defaults`
Expected: FAIL to compile — `Rewrite` has no `url`/`token`/`model`/`command`/
`skip_if_plain` fields yet.

- [ ] **Step 3: Add the fields**

```rust
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default)]
pub struct Rewrite {
    pub engine: String,
    pub agent: String,
    /// The address for `engine = "http"`. Empty means unconfigured.
    /// Provisional name (`docs/decisions.md`, 2026-09-04, #36).
    pub url: String,
    /// An optional bearer token for `engine = "http"`. Empty means no
    /// `Authorization` header is sent.
    pub token: String,
    /// Sent as the request body's `model` field for `engine = "http"`.
    /// May be empty; some local servers accept that and pick their own.
    pub model: String,
    /// The program and its arguments for `engine = "command"`, with
    /// `{transcript}` and `{bias}` replaced before it runs. `{transcript}`
    /// is force-appended when absent; `{bias}` never is.
    pub command: Vec<String>,
    /// Whether a short transcript with no foreign term and no name from the
    /// collected bias string skips the engine entirely.
    pub skip_if_plain: bool,
}
```

Update `impl Default for Rewrite`:

```rust
impl Default for Rewrite {
    fn default() -> Self {
        Rewrite {
            engine: "agent".to_string(),
            agent: "auto".to_string(),
            url: String::new(),
            token: String::new(),
            model: String::new(),
            command: Vec::new(),
            skip_if_plain: true,
        }
    }
}
```

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test --bin herdr-voice config::`
Expected: PASS, including every pre-existing `config` test.

- [ ] **Step 5: Commit**

```bash
git add src/config.rs
git commit -m "Add [rewrite]'s new keys: url, token, model, command, skip_if_plain"
```

---

### Task 3: `rewrite::command` — the external-program engine

**Files:**
- Create: `src/rewrite/command.rs`

**Before starting:** read `src/stt/command.rs` in full — this task ports its
`render`/`CommandEngine`/`CommandError` shape almost line for line, with
`{transcript}` in place of `{audio}` (force-appended), `{bias}` in place of
`{prompt}` (never force-appended, per #26's precedent), and no `{model}`/
`{language}` substitution at all (rewrite's command engine has no model or
language of its own to bake in).

**Interfaces:**
- Consumes: nothing outside `std`.
- Produces: `pub fn render(argv: &[String], transcript: &str, bias: &str) ->
  Vec<String>`; `pub struct CommandEngine { argv: Vec<String> }` with `pub fn
  new(argv: Vec<String>) -> CommandEngine`; `pub enum CommandError { NotFound
  { program: String, path: String }, Failed { program: String, code: String,
  stderr: String }, Silent { program: String } }` with a `Display` impl
  mirroring `stt::command::CommandError`'s wording, substituting "rewritten
  text" for "transcript".
- Consumed by: Task 5 (`rewrite::resolve`)

- [ ] **Step 1: Write the failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn argv(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn each_placeholder_is_replaced() {
        let rendered = render(
            &argv(&["prog", "--fix", "{transcript}", "--context", "{bias}"]),
            "hello there",
            "recent terms: pull request",
        );
        assert_eq!(
            rendered,
            argv(&["prog", "--fix", "hello there", "--context", "recent terms: pull request"])
        );
    }

    #[test]
    fn a_list_that_never_asks_for_the_transcript_gets_it_appended() {
        let rendered = render(&argv(&["prog"]), "hello there", "");
        assert_eq!(rendered, argv(&["prog", "hello there"]));
    }

    #[test]
    fn a_list_with_no_bias_placeholder_does_not_gain_one() {
        let rendered = render(&argv(&["prog", "{transcript}"]), "hello there", "recent terms");
        assert_eq!(rendered, argv(&["prog", "hello there"]));
    }

    #[test]
    fn the_transcript_reaches_the_program() {
        let engine = CommandEngine::new(argv(&["echo", "{transcript}"]));
        let rewritten = engine.rewrite("hello there", "").expect("text");
        assert_eq!(rewritten, "hello there");
    }

    #[test]
    fn a_nonexistent_program_names_the_path_searched() {
        let engine = CommandEngine::new(argv(&["definitely-not-a-program-here"]));
        let error = engine.rewrite("hello there", "").expect_err("must fail");
        let message = error.to_string();
        assert!(message.contains("definitely-not-a-program-here"), "got {message}");
        assert!(message.contains("PATH"), "got {message}");
    }

    #[test]
    fn a_program_that_prints_nothing_is_not_an_empty_success() {
        let engine = CommandEngine::new(argv(&["true"]));
        let error = engine.rewrite("hello there", "").expect_err("must fail");
        assert!(error.to_string().contains("no rewritten text"), "got {error}");
    }
}
```

- [ ] **Step 2: Run to verify the tests fail**

Run: `cargo test --bin herdr-voice rewrite::command`
Expected: FAIL to compile — the module does not exist yet.

- [ ] **Step 3: Implement `render`, `CommandEngine`, `CommandError`**

Port `stt::command.rs`'s `render` (`src/stt/command.rs:15-31`), replacing the
`{audio}`/`{model}`/`{language}` substitutions and force-append with
`{transcript}` (substitute, then force-append if absent from `argv`) and
`{bias}` (substitute, never force-appended). Port `CommandError` and its
`Display` impl (`src/stt/command.rs:56-98`) verbatim except for the wording
change noted above. Port the subprocess-spawning body of
`CommandEngine::transcribe` (`src/stt/command.rs:107-140`) as
`CommandEngine::rewrite(&self, transcript: &str, bias: &str) ->
Result<String, CommandError>`, trimming and returning stdout the same way,
`Silent` on empty output.

`rewrite::command` does not itself implement `rewrite::Engine` — Task 5 wraps
it, the way `stt::command::CommandEngine` implements `stt::Engine` directly
but this module stays a plain, trait-agnostic building block so Task 5's
`resolve` can wrap either engine behind one `Box<dyn Engine>` without this
module depending on the trait's own module.

- [ ] **Step 4: Run to verify the tests pass**

Run: `cargo test --bin herdr-voice rewrite::command`
Expected: PASS, all six tests.

- [ ] **Step 5: Commit**

```bash
git add src/rewrite/command.rs
git commit -m "Add rewrite::command, the external-program rewrite engine"
```

---

### Task 4: `rewrite::http` — the OpenAI-compatible endpoint engine

**Files:**
- Create: `src/rewrite/http.rs`

**Before starting:** read `DESIGN_36.md` §6 for the exact request/response
shape. Check `ureq`'s current documentation (`cargo doc --open -p ureq`, or
docs.rs for the version `Cargo.toml` pins after Task 1) for the exact
builder method chain to set a header, a timeout, and send/read a JSON body —
this plan fixes the contract, not `ureq`'s own API surface.

**Interfaces:**
- Consumes: `ureq` (Task 1)
- Produces: `pub struct HttpEngine { url: String, token: String, model: String
  }` with `pub fn new(url: String, token: String, model: String) ->
  HttpEngine`; `pub enum HttpError { Failed { url: String, detail: String },
  Unreadable { url: String, detail: String } }` with a `Display` impl naming
  the address and what to check; `impl HttpEngine { pub fn rewrite(&self,
  transcript: &str, bias: &str) -> Result<String, HttpError> }`.
- Consumed by: Task 5 (`rewrite::resolve`)

- [ ] **Step 1: Write the failing tests**

A scratch `TcpListener` stands in for the endpoint — no external HTTP server
crate, no live network:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    /// Accepts exactly one connection, reads the request, writes back a
    /// fixed HTTP response, then returns. Runs on a background thread so the
    /// test can drive a real `ureq` call against `http://127.0.0.1:<port>`.
    fn respond_once(response_body: &'static str) -> (String, std::thread::JoinHandle<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let addr = listener.local_addr().expect("addr");
        let url = format!("http://{addr}/v1/chat/completions");
        let handle = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept");
            let mut buf = [0u8; 4096];
            let n = stream.read(&mut buf).unwrap_or(0);
            let request = String::from_utf8_lossy(&buf[..n]).to_string();
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                response_body.len(),
                response_body
            );
            let _ = stream.write_all(response.as_bytes());
            request
        });
        (url, handle)
    }

    #[test]
    fn the_rewritten_text_is_read_back() {
        let (url, handle) = respond_once(
            r#"{"choices":[{"message":{"content":"pull request, not \"pulley quest\""}}]}"#,
        );
        let engine = HttpEngine::new(url, String::new(), "local-model".to_string());
        let rewritten = engine.rewrite("pulley quest", "").expect("text");
        assert_eq!(rewritten, "pull request, not \"pulley quest\"");
        let request = handle.join().expect("server thread");
        assert!(request.contains("\"model\":\"local-model\""), "got {request}");
        assert!(request.contains("pulley quest"), "got {request}");
    }

    #[test]
    fn an_empty_token_sends_no_authorization_header() {
        let (url, handle) = respond_once(r#"{"choices":[{"message":{"content":"x"}}]}"#);
        let engine = HttpEngine::new(url, String::new(), String::new());
        engine.rewrite("x", "").expect("text");
        let request = handle.join().expect("server thread");
        assert!(!request.to_lowercase().contains("authorization"), "got {request}");
    }

    #[test]
    fn a_token_is_sent_as_a_bearer_header() {
        let (url, handle) = respond_once(r#"{"choices":[{"message":{"content":"x"}}]}"#);
        let engine = HttpEngine::new(url, "secret-token".to_string(), String::new());
        engine.rewrite("x", "").expect("text");
        let request = handle.join().expect("server thread");
        assert!(request.contains("Bearer secret-token"), "got {request}");
    }

    #[test]
    fn an_unreadable_body_is_named_as_such() {
        let (url, handle) = respond_once("not json");
        let engine = HttpEngine::new(url, String::new(), String::new());
        let error = engine.rewrite("x", "").expect_err("must fail");
        assert!(matches!(error, HttpError::Unreadable { .. }), "got {error:?}");
        handle.join().expect("server thread");
    }

    #[test]
    fn a_connection_that_refuses_is_named_by_address() {
        // Nothing is listening on this port.
        let engine = HttpEngine::new(
            "http://127.0.0.1:1".to_string(),
            String::new(),
            String::new(),
        );
        let error = engine.rewrite("x", "").expect_err("must fail");
        let message = error.to_string();
        assert!(message.contains("127.0.0.1:1"), "got {message}");
    }
}
```

- [ ] **Step 2: Run to verify the tests fail**

Run: `cargo test --bin herdr-voice rewrite::http`
Expected: FAIL to compile — the module does not exist yet.

- [ ] **Step 3: Implement `HttpEngine`, `HttpError`**

Build the request body with `serde_json::json!({"model": self.model, "messages": [{"role": "system", "content": PROMPT}, {"role": "user", "content": transcript}], "temperature": 0})` — `PROMPT` a module-level `const`, ported from `spike/spike.sh`'s `rewrite()` prompt text (`spike/spike.sh:300-320`), with the four separate `CTX_*` fields collapsed into one line referencing `bias` instead. POST it to `self.url` with `ureq`, setting an `Authorization: Bearer {token}` header only when `self.token` is non-empty, and a timeout (§6 of the design: 30 seconds — check `ureq`'s current API for how a per-request or per-agent timeout is set in the pinned version). On a non-2xx response or a connection failure, return `HttpError::Failed { url: self.url.clone(), detail: ... }` naming the underlying cause. On a 2xx response whose body does not parse as JSON, or whose JSON has no `choices[0].message.content` string, return `HttpError::Unreadable { url: self.url.clone(), detail: ... }`. On success, return the trimmed `content` string.

`HttpError`'s `Display` impl names the URL and what to check, mirroring
`CommandError::NotFound`'s "what to check" framing — e.g. `"cannot reach
{url:?}: {detail}; check the server is running and the address is correct"`
for `Failed`, and `"{url:?} answered with something this could not read:
{detail}"` for `Unreadable`.

- [ ] **Step 4: Run to verify the tests pass**

Run: `cargo test --bin herdr-voice rewrite::http`
Expected: PASS, all five tests.

- [ ] **Step 5: Commit**

```bash
git add src/rewrite/http.rs
git commit -m "Add rewrite::http, the OpenAI-compatible rewrite engine"
```

---

### Task 5: `rewrite` — the trait, `Resolution`, and `resolve`

**Files:**
- Create: `src/rewrite.rs`
- Modify: `src/main.rs` (register the module, the way `src/main.rs` already
  declares `mod stt;` — find that line and add `mod rewrite;` beside it)

**Before starting:** read `src/stt.rs:1-135` in full — this task's `Engine`
trait, `EngineError`, and `resolve`/`resolve_with` split mirror it exactly,
substituting `rewrite`'s two real engines (`command`, `http`) for `stt`'s
`command` (`candle`/`http` there are stubs; here `agent` is the stub, folded
into `Resolution::Unavailable` rather than an `EngineError` variant, per
`AC_36.md`'s "Resolved during S1's gate" section).

**Interfaces:**
- Consumes: `rewrite::command::CommandEngine`/`CommandError` (Task 3),
  `rewrite::http::HttpEngine`/`HttpError` (Task 4)
- Produces: `pub trait Engine: Send + Sync { fn rewrite(&self, transcript:
  &str, bias: &str) -> Result<String, EngineError>; }`; `pub enum
  EngineError { Command(command::CommandError), Http(http::HttpError) }`
  with a `Display` impl delegating to each variant's own; `pub enum
  Resolution { Off, Engine(Box<dyn Engine + Send + Sync>), Unavailable(String)
  }`; `pub fn resolve(rewrite: &config::Rewrite) -> Resolution`
- Consumed by: Task 7 (`daemon::Runtime`)

- [ ] **Step 1: Write the failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn off_resolves_to_off() {
        let config = crate::config::Rewrite {
            engine: "off".to_string(),
            ..Default::default()
        };
        assert!(matches!(resolve(&config), Resolution::Off));
    }

    #[test]
    fn agent_resolves_to_unavailable() {
        let config = crate::config::Rewrite {
            engine: "agent".to_string(),
            ..Default::default()
        };
        match resolve(&config) {
            Resolution::Unavailable(why) => assert!(why.contains("agent"), "got {why:?}"),
            other => panic!("expected Unavailable, got {other:?}"),
        }
    }

    #[test]
    fn an_unknown_engine_resolves_to_unavailable_naming_the_value() {
        let config = crate::config::Rewrite {
            engine: "vosk".to_string(),
            ..Default::default()
        };
        match resolve(&config) {
            Resolution::Unavailable(why) => assert!(why.contains("vosk"), "got {why:?}"),
            other => panic!("expected Unavailable, got {other:?}"),
        }
    }

    #[test]
    fn http_with_an_empty_url_resolves_to_unavailable() {
        let config = crate::config::Rewrite {
            engine: "http".to_string(),
            ..Default::default()
        };
        assert!(matches!(resolve(&config), Resolution::Unavailable(_)));
    }

    #[test]
    fn http_with_a_url_resolves_to_an_engine() {
        let config = crate::config::Rewrite {
            engine: "http".to_string(),
            url: "http://127.0.0.1:1234/v1/chat/completions".to_string(),
            ..Default::default()
        };
        assert!(matches!(resolve(&config), Resolution::Engine(_)));
    }

    #[test]
    fn command_with_an_empty_list_resolves_to_unavailable() {
        let config = crate::config::Rewrite {
            engine: "command".to_string(),
            ..Default::default()
        };
        assert!(matches!(resolve(&config), Resolution::Unavailable(_)));
    }

    #[test]
    fn command_with_a_program_resolves_to_an_engine() {
        let config = crate::config::Rewrite {
            engine: "command".to_string(),
            command: vec!["echo".to_string()],
            ..Default::default()
        };
        assert!(matches!(resolve(&config), Resolution::Engine(_)));
    }
}
```

- [ ] **Step 2: Run to verify the tests fail**

Run: `cargo test --bin herdr-voice rewrite::tests`
Expected: FAIL to compile — `src/rewrite.rs` does not exist yet.

- [ ] **Step 3: Implement**

```rust
pub mod command;
pub mod http;

pub trait Engine: Send + Sync {
    fn rewrite(&self, transcript: &str, bias: &str) -> Result<String, EngineError>;
}

#[derive(Debug)]
pub enum EngineError {
    Command(command::CommandError),
    Http(http::HttpError),
}

// Display delegates to each inner error's own Display, the way stt::EngineError
// does for stt::command::CommandError and stt::model::ModelError.

impl Engine for command::CommandEngine {
    fn rewrite(&self, transcript: &str, bias: &str) -> Result<String, EngineError> {
        self.rewrite(transcript, bias).map_err(EngineError::Command)
    }
}

impl Engine for http::HttpEngine {
    fn rewrite(&self, transcript: &str, bias: &str) -> Result<String, EngineError> {
        self.rewrite(transcript, bias).map_err(EngineError::Http)
    }
}

pub enum Resolution {
    Off,
    Engine(Box<dyn Engine + Send + Sync>),
    Unavailable(String),
}

pub fn resolve(rewrite: &crate::config::Rewrite) -> Resolution {
    match rewrite.engine.as_str() {
        "off" => Resolution::Off,
        "agent" => Resolution::Unavailable(
            "[rewrite] engine = \"agent\" is not invoked by this build; the transcript is \
             delivered unrewritten. Set [rewrite] engine to \"http\" or \"command\" for a \
             working engine, or \"off\" to silence this.".to_string(),
        ),
        "http" if rewrite.url.is_empty() => Resolution::Unavailable(
            "[rewrite] engine = \"http\" but [rewrite] url is empty, so there is nothing to \
             post to.".to_string(),
        ),
        "http" => Resolution::Engine(Box::new(http::HttpEngine::new(
            rewrite.url.clone(),
            rewrite.token.clone(),
            rewrite.model.clone(),
        ))),
        "command" if rewrite.command.is_empty() => Resolution::Unavailable(
            "[rewrite] engine = \"command\" but [rewrite] command is empty, so there is \
             nothing to run.".to_string(),
        ),
        "command" => Resolution::Engine(Box::new(command::CommandEngine::new(
            rewrite.command.clone(),
        ))),
        other => Resolution::Unavailable(format!(
            "unknown [rewrite] engine {other:?}; it is one of \"agent\", \"http\", \
             \"command\" or \"off\""
        )),
    }
}
```

Note the naming collision this creates on purpose: `command::CommandEngine`
and `http::HttpEngine` each have their own inherent `rewrite(&self, ...)`
method (Tasks 3/4) with a concrete error type, and the `Engine for
CommandEngine`/`Engine for HttpEngine` impls here delegate to it and wrap the
error — the same shape `stt::command::CommandEngine::transcribe` would have
if `stt::Engine` were implemented for a type with its own inherent method of
the same name. If this reads awkwardly once written, an implementer may
instead name the inherent methods `run` and keep `rewrite` only on the
trait — a naming choice, not a behavior change; either is acceptable and
needs no second gate round.

- [ ] **Step 4: Run to verify the tests pass**

Run: `cargo test --bin herdr-voice rewrite::`
Expected: PASS — every test in `rewrite`, `rewrite::command` and
`rewrite::http`.

- [ ] **Step 5: Register the module and run the whole suite**

Add `mod rewrite;` to `src/main.rs` beside the existing `mod stt;`.

Run: `cargo test`
Expected: PASS, no regressions elsewhere.

- [ ] **Step 6: Commit**

```bash
git add src/rewrite.rs src/main.rs
git commit -m "Add rewrite::Engine, Resolution and resolve, tying the two engines together"
```

---

### Task 6: `rewrite::skip` — the short-phrase heuristic

**Files:**
- Create: `src/rewrite/skip.rs`
- Modify: `src/rewrite.rs` (`pub mod skip;`)

**Before starting:** read `DESIGN_36.md` §4 for the exact three-check
definition and the reasoning for the length gate.

**Interfaces:**
- Produces: `pub fn plain(transcript: &str, bias: &str, enabled: bool) ->
  bool`
- Consumed by: Task 7 (`daemon::transcribe`)

- [ ] **Step 1: Write the failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_never_skips() {
        assert!(!plain("да", "", false));
    }

    #[test]
    fn a_short_plain_phrase_with_no_context_name_skips() {
        assert!(plain("открой файл", "", true));
    }

    #[test]
    fn a_long_phrase_does_not_skip_even_with_no_foreign_term() {
        let long = "слово ".repeat(20);
        assert!(!plain(&long, "", true));
    }

    #[test]
    fn a_short_phrase_with_a_latin_run_does_not_skip() {
        assert!(!plain("открой pull request", "", true));
    }

    #[test]
    fn a_short_phrase_matching_a_context_word_does_not_skip() {
        // "журнал" (journal/log) has no Latin letters, so this transcript
        // passes both earlier checks — length and has_latin_run — and must
        // be caught by shares_a_word alone. A shares_a_word that always
        // returns false would pass every other test in this module but
        // fail this one and the next: neither transcript here contains any
        // ASCII letter, so has_latin_run cannot short-circuit before
        // shares_a_word runs, unlike a Latin loanword would.
        assert!(!plain("открой журнал", "журнал notes.txt", true));
    }

    #[test]
    fn the_context_match_is_case_insensitive() {
        assert!(!plain("открой ЖУРНАЛ", "журнал notes.txt", true));
    }
}
```

- [ ] **Step 2: Run to verify the tests fail**

Run: `cargo test --bin herdr-voice rewrite::skip`
Expected: FAIL to compile — the module does not exist yet.

- [ ] **Step 3: Implement**

```rust
/// A named constant, not a configuration key — the same footing
/// `bias::transcript::TURN_CHARS` is on. Short enough that a false skip
/// costs a missed punctuation fix, not a mangled technical term in a take
/// long enough to actually need one (`DESIGN_36.md` §4).
const SKIP_WORD_LIMIT: usize = 8;

pub fn plain(transcript: &str, bias: &str, enabled: bool) -> bool {
    if !enabled {
        return false;
    }
    let words: Vec<&str> = transcript.split_whitespace().collect();
    if words.len() > SKIP_WORD_LIMIT {
        return false;
    }
    if has_latin_run(transcript) {
        return false;
    }
    !shares_a_word(transcript, bias)
}

fn has_latin_run(text: &str) -> bool {
    let mut run = 0;
    for c in text.chars() {
        if c.is_ascii_alphabetic() {
            run += 1;
            if run >= 2 {
                return true;
            }
        } else {
            run = 0;
        }
    }
    false
}

fn shares_a_word(transcript: &str, bias: &str) -> bool {
    let bias_words: std::collections::HashSet<String> = bias
        .split_whitespace()
        .map(|w| w.to_lowercase())
        .collect();
    transcript
        .split_whitespace()
        .any(|w| bias_words.contains(&w.to_lowercase()))
}
```

- [ ] **Step 4: Run to verify the tests pass**

Run: `cargo test --bin herdr-voice rewrite::skip`
Expected: PASS, all six tests.

- [ ] **Step 5: Commit**

```bash
git add src/rewrite/skip.rs src/rewrite.rs
git commit -m "Add rewrite::skip, the short-phrase-with-nothing-to-fix heuristic"
```

---

### Task 7: Wire the rewrite step into `daemon.rs`

**Files:**
- Modify: `src/daemon.rs`: `Runtime` (`:51-71`), `start()` (`:451-479`),
  `dictate` (`:139-166`), `transcribe` (`:303-338`), the three test-only
  `Runtime` literals (`fake_runtime` at `:564`, `runtime_with` at `:652`, the
  inline one inside
  `the_failure_reply_survives_one_read_line_when_the_transcript_and_the_reason_carry_newlines`
  near `:774`)

**Before starting, in this order:**
1. Check whether #26 has merged to `origin/main` (per this plan's Global
   Constraints). If it has, rebase this branch first.
2. Read `src/daemon.rs:349-358` for the exact shape of the existing
   failed-delivery toast, which `tell_once` (below) reuses.
3. Read `DESIGN_36.md` §§1-3 for the exact `Resolution`/`told`/`tell_once`
   shapes this task builds.

**Interfaces:**
- Consumes: `rewrite::Resolution`, `rewrite::resolve` (Task 5),
  `rewrite::skip::plain` (Task 6)
- Produces: `Runtime.rewrite: rewrite::Resolution`, `Runtime.skip_if_plain:
  bool`, `Runtime.told: std::sync::atomic::AtomicBool`

- [ ] **Step 1: Write the failing tests**

Four new tests in `src/daemon.rs`'s test module, each driving `answer(...)`
against a `runtime_with`-built `Runtime` whose `rewrite` field is overridden
after construction (`runtime.rewrite = ...`):

Recall `runtime_with`'s canned recognition always returns `"fix the worklog
entry"` (`src/daemon.rs:658` in this checkout) — that string is 4 words, under
`SKIP_WORD_LIMIT`, so a test that needs the skip heuristic to NOT short-circuit
must use a longer sentence instead (asserted on `Reply`/`Call` values built
from that longer text, not the fixed default). `FakeDeliverer` is `Clone`
(`src/delivery.rs:79`, an `Arc<Mutex<Vec<Call>>>` inside); the established
idiom for inspecting what was actually delivered is to clone it *before*
passing it into `runtime_with`, and call `.calls()` on the kept clone
afterward — see `a_toast_is_raised_on_a_failed_delivery_only_when_ui_toasts_is_on`
(`src/daemon.rs:979-1002`) for the exact pattern this reuses.

```rust
#[test]
fn a_working_rewrite_engine_changes_the_delivered_text() {
    let fake = crate::delivery::tests_support::FakeDeliverer::ok();
    let mut runtime = runtime_with(fake.clone(), false);
    runtime.rewrite = crate::rewrite::Resolution::Engine(Box::new(
        crate::rewrite::tests_support::Fake(Ok(
            "rewritten text with plenty of words so the skip heuristic never applies here"
                .to_string(),
        )),
    ));
    let recorder = tone_recorder("rewrite-hit");
    let request = dictate_request();
    answer(&request, &recorder, &runtime);
    answer(&request, &recorder, &runtime);

    assert!(
        fake.calls().iter().any(|c| matches!(
            c,
            crate::delivery::tests_support::Call::Insert(pane, text)
                if pane == "w1:p2"
                    && text == "rewritten text with plenty of words so the skip heuristic never applies here"
        )),
        "got {:?}",
        fake.calls()
    );
}

#[test]
fn a_failed_rewrite_engine_delivers_the_original_text_and_tells_once() {
    let journal = std::sync::Arc::new(RecordingJournal::default());
    let fake = crate::delivery::tests_support::FakeDeliverer::ok();
    let mut runtime = runtime_with(fake.clone(), false);
    runtime.rewrite = crate::rewrite::Resolution::Engine(Box::new(
        crate::rewrite::tests_support::Fake(Err("engine refused the connection".to_string())),
    ));
    runtime.journal = Box::new(TestJournal(std::sync::Arc::clone(&journal)));
    let recorder = tone_recorder("rewrite-fail");

    // Two takes; the skip heuristic does not matter here since the canned
    // recognition text is short and plain — a short circuit still delivers
    // the same unrewritten text this test asserts on either way.
    let request = dictate_request();
    answer(&request, &recorder, &runtime);
    answer(&request, &recorder, &runtime);
    answer(&request, &recorder, &runtime);
    answer(&request, &recorder, &runtime);

    assert!(
        fake.calls().iter().all(|c| !matches!(
            c,
            crate::delivery::tests_support::Call::Insert(_, text) if text != "fix the worklog entry"
        )),
        "got {:?}",
        fake.calls()
    );

    let lines = journal.0.lock().unwrap();
    let notices = lines.iter().filter(|l| l.contains("rewrite unavailable")).count();
    assert_eq!(notices, 1, "got {lines:?}");
}

#[test]
fn an_unavailable_rewrite_resolution_delivers_the_original_text_and_tells_once() {
    let journal = std::sync::Arc::new(RecordingJournal::default());
    let fake = crate::delivery::tests_support::FakeDeliverer::ok();
    let mut runtime = runtime_with(fake.clone(), false);
    runtime.rewrite = crate::rewrite::Resolution::Unavailable("agent not invoked".to_string());
    runtime.journal = Box::new(TestJournal(std::sync::Arc::clone(&journal)));
    let recorder = tone_recorder("rewrite-unavailable");
    let request = dictate_request();
    answer(&request, &recorder, &runtime);
    answer(&request, &recorder, &runtime);
    answer(&request, &recorder, &runtime);
    answer(&request, &recorder, &runtime);

    assert!(
        fake.calls().iter().any(|c| matches!(
            c,
            crate::delivery::tests_support::Call::Insert(_, text) if text == "fix the worklog entry"
        )),
        "got {:?}",
        fake.calls()
    );

    // Distinguishes Unavailable from Off: Unavailable tells once, Off never
    // tells at all (see resolution_off_never_tells_and_never_reaches_the_engine
    // below). Two takes, one notice — the same shape
    // a_failed_rewrite_engine_delivers_the_original_text_and_tells_once proves.
    let lines = journal.0.lock().unwrap();
    let notices = lines.iter().filter(|l| l.contains("rewrite unavailable")).count();
    assert_eq!(notices, 1, "got {lines:?}");
}

#[test]
fn resolution_off_never_tells() {
    let journal = std::sync::Arc::new(RecordingJournal::default());
    let fake = crate::delivery::tests_support::FakeDeliverer::ok();
    let mut runtime = runtime_with(fake.clone(), false);
    runtime.rewrite = crate::rewrite::Resolution::Off;
    runtime.journal = Box::new(TestJournal(std::sync::Arc::clone(&journal)));
    let recorder = tone_recorder("rewrite-off");
    let request = dictate_request();
    answer(&request, &recorder, &runtime);
    answer(&request, &recorder, &runtime);

    assert!(
        fake.calls().iter().any(|c| matches!(
            c,
            crate::delivery::tests_support::Call::Insert(_, text) if text == "fix the worklog entry"
        )),
        "got {:?}",
        fake.calls()
    );

    // The distinguishing assertion versus the Unavailable test above: Off
    // produces zero notices, not one.
    let lines = journal.0.lock().unwrap();
    assert!(
        !lines.iter().any(|l| l.contains("rewrite unavailable")),
        "Off must never tell, got {lines:?}"
    );
}
```

This step also needs `crate::rewrite::tests_support::Fake`, a canned-result
test double for `rewrite::Engine` mirroring `stt::tests_support::Fake`
exactly (`src/stt.rs:137-152`, a plain `#[cfg(test)] pub mod tests_support` —
cross-module access from `daemon.rs`'s own `#[cfg(test)]` code already works
this way for `stt::tests_support::Fake`, since `cfg(test)` code across the
whole crate compiles together under `cargo test`). Add the same shape to
`src/rewrite.rs` in this same task:

```rust
#[cfg(test)]
pub mod tests_support {
    use super::{Engine, EngineError};

    pub struct Fake(pub Result<String, String>);

    impl Engine for Fake {
        fn rewrite(&self, _transcript: &str, _bias: &str) -> Result<String, EngineError> {
            match &self.0 {
                Ok(text) => Ok(text.clone()),
                Err(why) => Err(EngineError::Command(
                    crate::rewrite::command::CommandError::Silent {
                        program: why.clone(),
                    },
                )),
            }
        }
    }
}
```

(Wrapping the canned failure in `CommandError::Silent` is a convenience so
`Fake`'s `Err` variant still produces a real `EngineError` without adding a
third error-wrapping variant just for tests; its `Display` text is not
asserted on by any test in this plan, only that an `Err` was returned.)

- [ ] **Step 2: Run to verify the new tests fail**

Run: `cargo test --bin herdr-voice daemon::tests`
Expected: FAIL to compile — `Runtime` has no `rewrite`/`skip_if_plain`/`told`
fields, `crate::rewrite` is not referenced from `daemon.rs` yet.

- [ ] **Step 3: Add the fields, resolve once at start, wire `transcribe`**

`Runtime` gains:

```rust
pub rewrite: crate::rewrite::Resolution,
pub skip_if_plain: bool,
pub told: std::sync::atomic::AtomicBool,
```

In `start()` (`:451`), after `bias_source`/`transcript_root` are resolved:

```rust
let rewrite = crate::rewrite::resolve(&loaded.config.rewrite);
```

Add `rewrite`, `skip_if_plain: loaded.config.rewrite.skip_if_plain`, `told:
std::sync::atomic::AtomicBool::new(false)` to the `Runtime` literal in
`start()`.

In `dictate` (`:151-165`), bind `take_bias`'s return (already needed if #26
merged first; if not, this is this task's own first binding of it — check
which applies per this task's "before starting" step 1):

```rust
let collected = take_bias(runtime, &take.target, take.cwd.as_deref(), take.agent.as_deref());
transcribe(runtime, &take, &collected.bias)
```

(If #26 has merged, `transcribe` already takes a `bias: &str` parameter for
recognition — reuse that same `&collected.bias` argument for both recognition
and rewrite, rather than adding a second parameter.)

In `transcribe` (`:303`), between recognition producing `text` and
`delivering_line`/`deliver` (`:325` onward), insert:

```rust
let text = match &runtime.rewrite {
    crate::rewrite::Resolution::Off => text,
    crate::rewrite::Resolution::Unavailable(why) => {
        tell_once(runtime, why);
        text
    }
    crate::rewrite::Resolution::Engine(engine) => {
        if crate::rewrite::skip::plain(&text, bias, runtime.skip_if_plain) {
            text
        } else {
            match engine.rewrite(&text, bias) {
                Ok(rewritten) => rewritten,
                Err(why) => {
                    tell_once(runtime, &why.to_string());
                    text
                }
            }
        }
    }
};
```

(`bias` here is `transcribe`'s existing or newly-added `bias: &str`
parameter — see the note above about reusing #26's if it has landed.)

Add `tell_once` near `toast_failed_line` (`:419-424`):

```rust
fn tell_once(runtime: &Runtime, why: &str) {
    if runtime.told.swap(true, std::sync::atomic::Ordering::Relaxed) {
        return;
    }
    runtime.journal.write(&rewrite_unavailable_line(why));
    if runtime.delivery_settings.toasts {
        if let Err(toast_why) = runtime.deliverer.notify("Rewrite unavailable", why) {
            runtime
                .journal
                .write(&toast_failed_line(&toast_why.to_string().replace('\n', " ")));
        }
    }
}

fn rewrite_unavailable_line(why: &str) -> String {
    format!("rewrite unavailable: {}", why.replace('\n', " "))
}
```

Update the three test-only `Runtime` literals (`fake_runtime`, `runtime_with`,
and the inline one near `:774`) to add `rewrite:
crate::rewrite::Resolution::Off, skip_if_plain: true, told:
std::sync::atomic::AtomicBool::new(false),`.

- [ ] **Step 4: Run to verify the new tests pass, and the whole daemon suite too**

Run: `cargo test --bin herdr-voice daemon::`
Expected: PASS, including the four new tests and every pre-existing test in
`src/daemon.rs`.

- [ ] **Step 5: Run the whole suite**

Run: `cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt --check && python3 scripts/check_manifest.py`
Expected: all four green.

- [ ] **Step 6: Commit**

```bash
git add src/daemon.rs src/rewrite.rs
git commit -m "Wire the rewrite step into transcribe, between recognition and delivery"
```

---

### Task 8: `doctor::rewrite_finding`

**Files:**
- Modify: `src/doctor.rs:259-296` (`rewrite_finding`), `:336-339` (its caller)

**Before starting:** read `DESIGN_36.md` §8 for the exact new signature and
message wording.

- [ ] **Step 1: Write the failing tests**

Add to `doctor.rs`'s test module, alongside `the_rewrite_engine_is_looked_for_by_name`:

```rust
#[test]
fn http_is_ok_when_a_url_is_configured() {
    let rewrite = crate::config::Rewrite {
        engine: "http".to_string(),
        url: "http://127.0.0.1:1234/v1/chat/completions".to_string(),
        ..Default::default()
    };
    assert_eq!(rewrite_finding(&rewrite).state, State::Ok);
}

#[test]
fn http_is_missing_when_no_url_is_configured() {
    let rewrite = crate::config::Rewrite {
        engine: "http".to_string(),
        ..Default::default()
    };
    assert_eq!(rewrite_finding(&rewrite).state, State::Missing);
}

#[test]
fn command_is_ok_when_a_program_is_configured() {
    let rewrite = crate::config::Rewrite {
        engine: "command".to_string(),
        command: vec!["echo".to_string()],
        ..Default::default()
    };
    assert_eq!(rewrite_finding(&rewrite).state, State::Ok);
}

#[test]
fn agent_is_missing_even_when_found_on_path() {
    let rewrite = crate::config::Rewrite {
        engine: "agent".to_string(),
        agent: "auto".to_string(),
        ..Default::default()
    };
    // Whether or not a real agent binary happens to be on this machine's
    // PATH, the take path treats "agent" as unavailable, so doctor must
    // report the same thing regardless.
    assert_eq!(rewrite_finding(&rewrite).state, State::Missing);
}
```

- [ ] **Step 2: Run to verify the tests fail**

Run: `cargo test --bin herdr-voice doctor::tests::http_is_ok_when_a_url_is_configured`
Expected: FAIL to compile — `rewrite_finding` still takes `(engine: &str,
agent: &str)`, not `(&config::Rewrite)`.

- [ ] **Step 3: Change the signature and the `"agent"`/`"http"`/`"command"` branches**

`rewrite_finding(rewrite: &config::Rewrite) -> Finding`. `"http"`: `Ok` if
`rewrite.url` is non-empty, else `Missing` naming the key to set. `"command"`:
`Ok` if `rewrite.command` is non-empty, else `Missing`. `"agent"`: always
`Missing`, with the detail text from `DESIGN_36.md` §8 (the tool being on
`PATH` is no longer sufficient — say so in the message, mentioning whether it
was found, but keep the state `Missing`).

Update the caller (`:336-339`) to pass `&loaded.config.rewrite` instead of
`&loaded.config.rewrite.engine, &loaded.config.rewrite.agent`.

The pre-existing test `the_rewrite_engine_is_looked_for_by_name`
(`src/doctor.rs:568-580`) calls `rewrite_finding` three times with the old
two-`&str` signature; update it to the new one-argument shape. Its first two
assertions still hold once converted — `"agent"` is `Missing` regardless of
`PATH` now, which was already the assertion's outcome, just for a different
reason (unconfigured, not not-found) — but the comment on the third
assertion ("auto resolves against the candidate list") no longer describes
anything observable in `state`, since `state` no longer reflects whether the
candidate was found. Replace it:

```rust
#[test]
fn the_rewrite_engine_is_looked_for_by_name() {
    let unavailable = crate::config::Rewrite {
        engine: "agent".to_string(),
        agent: "definitely-not-installed".to_string(),
        ..Default::default()
    };
    assert_eq!(rewrite_finding(&unavailable).state, State::Missing);

    let off = crate::config::Rewrite {
        engine: "off".to_string(),
        ..Default::default()
    };
    assert_eq!(rewrite_finding(&off).state, State::Ok);

    let auto = crate::config::Rewrite {
        engine: "agent".to_string(),
        agent: "auto".to_string(),
        ..Default::default()
    };
    assert_eq!(rewrite_finding(&auto).name, "rewrite");
}
```

- [ ] **Step 4: Run to verify the tests pass**

Run: `cargo test --bin herdr-voice doctor::`
Expected: PASS, including the four new tests and every pre-existing
`rewrite_finding` test, updated to the new call shape.

- [ ] **Step 5: Run the whole suite**

Run: `cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt --check && python3 scripts/check_manifest.py`
Expected: all four green.

- [ ] **Step 6: Commit**

```bash
git add src/doctor.rs
git commit -m "Make doctor::rewrite_finding read the whole [rewrite] table, and stop trusting agent"
```

---

### Task 9: S4 — review the diff before a pull request exists

Invoke `superpowers:requesting-code-review` against the full diff on
`feat/36-rewrite-http-command` since it diverged from `main`, naming this
plan and `DESIGN_36.md` as what it should satisfy. Explicitly ask the
reviewer to mutation-test: the tell-once flag (does a second failure really
stay silent, and does `Resolution::Off` really never call the engine); the
skip heuristic's three checks independently (each one removed should turn a
different test red); and that no new code path logs the transcript, the bias
string, or a rewritten transcript.

- [ ] Run the review.
- [ ] Append a `## Gate S4` block to `tasks/36/RUN_36.md` with the verdict; if
  not `READY`, address the findings and re-run before proceeding.
- [ ] Commit: `git add tasks/36/RUN_36.md && git commit -m "Record the S4 diff review for #36"`

---

### Task 10: S5 — verify by test suite, then by real local servers

- [ ] Run `cargo test`, reading the whole output; then `cargo clippy
  --all-targets -- -D warnings`, `cargo fmt --check`,
  `python3 scripts/check_manifest.py`.
- [ ] Append a section to `docs/evidence.md`: what the test suite establishes
  (both engines rewrite through a scratch stand-in, the skip heuristic's
  three checks, the tell-once mechanism, `off`/`agent`/unknown all route to
  "no engine available") and what it does not — AC-10 needs a real, running
  local server and a real external program, and no test here runs either.
- [ ] **The manual step (AC-10), for the owner:** point `[rewrite] engine =
  "http"` at a locally running LM Studio or Ollama instance (whichever is at
  hand), `[rewrite] url` at its OpenAI-compatible chat-completions address,
  and rewrite a real transcript containing a foreign term or a file name.
  Separately, point `[rewrite] engine = "command"` at an arbitrary program
  (even a trivial `sed`/`awk`/shell one-liner) and confirm it also rewrites
  a real transcript. Record the platform, what was configured, and the
  result in `docs/evidence.md`, next to the other stages' first live
  measurements.
- [ ] Append a `## Gate S5` block to `tasks/36/RUN_36.md` with the verdict —
  note explicitly if the manual step is still pending the owner.
- [ ] Commit: `git add docs/evidence.md tasks/36/RUN_36.md && git commit -m "Verify the rewrite stage by test suite, and record what a live server confirmed"`

---

## Coverage

AC-1 → Task 7. AC-2 → Task 4. AC-3 → Task 3. AC-4 → Task 7
(`resolution_off_never_tells`). AC-5 → Task 6. AC-6 →
Tasks 5, 7. AC-7 → Task 7
(`an_unavailable_rewrite_resolution_delivers_the_original_text_and_tells_once`,
`a_failed_rewrite_engine_delivers_the_original_text_and_tells_once`, and
`resolution_off_never_tells` for the negative case). AC-8 → Tasks 3, 4 (error
`Display` impls). AC-9 →
Tasks 3, 4 (scratch script, scratch `TcpListener`). AC-10 → Task 10's manual
step. AC-11 → Task 8.

## What could not be cut into a checkable task

AC-10's manual runs cannot be scheduled as a task with a done-criterion this
plan can check itself — they need a person with a local server or a program
at hand, the same limit `tasks/21/PLAN_21.md`'s and `tasks/26/PLAN_26.md`'s
own S5 tasks recorded. Task 10 states plainly what a person must do and what
claim would be false without it.
