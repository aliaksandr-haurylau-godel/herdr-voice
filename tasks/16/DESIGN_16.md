# DESIGN_16

Design for issue #16, against `tasks/16/AC_16.md` (gate S1: READY,
`tasks/16/RUN_16.md`).

## 1. New module: `src/stt/http.rs`

Mirrors `src/rewrite/http.rs`'s shape — a dedicated `ureq::Agent` per engine
instance, pooling disabled, a fixed timeout — with two differences the wire
contract requires: the request body is `multipart/form-data`, not JSON, and
the error enum needs a third distinct variant, since AC-5 asks for a
connection failure and a non-2xx response to be told apart by message, which
`rewrite::http::HttpError`'s two-variant shape does not do (it collapses
both into one `Failed` variant).

```rust
pub struct HttpEngine {
    url: String,
    token: String,
    model: String,
    agent: ureq::Agent,
}

pub enum HttpError {
    Refused { url: String, detail: String },       // AC-5, case 1
    Failed { url: String, status: u16, detail: String }, // AC-5, case 2
    Unreadable { url: String, detail: String },     // AC-5, case 3
}

impl Engine for HttpEngine {
    fn transcribe(&self, audio: &Path, bias: &str) -> Result<String, EngineError> { ... }
}
```

`EngineError` (`src/stt.rs:19-31`) gains one variant, `Http(http::HttpError)`,
the same shape `Command(command::CommandError)` already has.

## 2. The request

`POST [stt] url`, `multipart/form-data`:

- `file` — the WAV at the given path. Always present; the one field the
  request cannot go without.
- `model` — `[stt] http_model`, only when non-empty.
- `language` — `[stt] language`, only when it is not `"auto"`. The API's
  `language` field has no meaning for the literal string `"auto"` (unlike
  `command::render`'s `{language}` substitution, which passes `"auto"`
  through because `whisper-cli` reads it as "detect it," `src/stt/
  command.rs:32`) — so this field is omitted rather than sent with that
  value.
- `prompt` — `bias`, only when non-empty. The same role `{prompt}` plays for
  the `command` engine (`src/stt/command.rs:37`): an opt-in enhancement,
  never force-added the way `file` is.

`Authorization: Bearer [stt] token`, only when the token is non-empty.

**Building the multipart body.** `ureq` 2.x has no built-in multipart
support; this needs either a small hand-written body (a multipart/form-data
encoder is a boundary string, a header and a byte payload per field — on the
order of thirty lines, no parsing involved since this side only writes) or a
companion crate. Preference: hand-written, not a new dependency — this
repository adds one deliberately when a task cannot be done without it
(`docs/decisions.md`, 2026-08-24, #3), and encoding one file and up to three
short string fields is not that task. The plan fixes this preference; if the
implementer finds `ureq`'s current version awkward to drive with a
hand-written body, that is a plan-stage or implementation-stage finding to
report, not a silent switch to a new crate.

## 3. The response

2xx: parse the JSON body, read `text` as a string. Missing or non-string
`text` is `HttpError::Unreadable`. Non-2xx: `HttpError::Failed { status,
... }`, the status code named in the message. A connection that refuses,
times out, or is otherwise unreachable: `HttpError::Refused`.

A bound exists the same way `rewrite::http::HttpEngine`'s does (`src/
rewrite/http.rs:18-20`): a fixed timeout, so an endpoint that accepts a
connection and never answers turns into a message rather than a leaked
thread.

## 4. Resolution: `stt::resolve_with`

```rust
"http" if stt.url.is_empty() => Err(EngineError::NotConfigured),
"http" => Ok(Box::new(http::HttpEngine::new(
    stt.url.clone(),
    stt.token.clone(),
    stt.http_model.clone(),
))),
```

`EngineError::NotConfigured` already exists and is what `"command"` returns
for an empty argument list (`src/stt.rs:121`, and a defensive second site,
`src/stt/command.rs:126`, for the case `render` somehow produces an empty
argument list even though `resolve_with` already checked `stt.command` was
non-empty — unreachable in practice, still has to compile). Today it is a
unit variant with a hard-coded `Display` naming `[stt] command` specifically
(`src/stt.rs:62-66`) — reused as-is for `"http"`, it would tell somebody
whose `[stt] url` is empty to fill in `[stt] command` instead, which fails
AC-2 ("a distinct, named case") and AC-6 alike, and reaches `doctor` verbatim
through `engine_finding_from`'s `e.to_string()` (`src/doctor.rs:212`).

`NotConfigured` gains a payload instead of a second variant:

```rust
NotConfigured { engine: &'static str, key: &'static str, example: &'static str },
```

```rust
EngineError::NotConfigured { engine, key, example } => write!(
    f,
    "[stt] engine is {engine:?} but [stt] {key} is empty, so there is nothing \
     to run. For example:\n  {example}"
),
```

The existing `EXAMPLE` constant (`src/stt.rs:40`) is renamed `COMMAND_EXAMPLE`
and a sibling `HTTP_EXAMPLE` is added (`url = "http://127.0.0.1:8080/v1/
audio/transcriptions"`, a generic local address, not a real one). Both
existing construction sites (`src/stt.rs:121`, `src/stt/command.rs:126`,
reachable from a child module via `super::COMMAND_EXAMPLE` since Rust's
privacy allows a descendant module to see an ancestor's private items) pass
`{ engine: "command", key: "command", example: COMMAND_EXAMPLE }` — the
rendered text for the existing `"command"` case is unchanged, so
`a_command_engine_with_nothing_to_run_names_the_key_and_shows_one`
(`src/stt.rs:234`) keeps asserting the same string, only through the new
payload shape; the plan names updating it explicitly rather than assuming it
compiles unchanged. `"http"`'s new empty-`url` site passes `{ engine: "http",
key: "url", example: HTTP_EXAMPLE }`.

## 5. Configuration

`Stt` (`src/config.rs:53-63`) gains three fields, settled names, not
provisional (confirmed directly, `tasks/16/RUN_16.md`):

```rust
pub struct Stt {
    pub model: String,
    pub engine: String,
    pub language: String,
    pub command: Vec<String>,
    pub url: String,         // new. Empty means engine = "http" is unconfigured.
    pub token: String,       // new. Empty means no Authorization header.
    pub http_model: String,  // new. Sent as the request's model field. Separate
                              // from `model`, which stays a local-model identifier.
}
```

Defaults: all three empty strings, matching every other optional key in this
file.

## 6. `doctor`

No change needed. `engine_finding_from` (`src/doctor.rs:199-215`) already
delegates to `stt::resolve_with` and reports `Ok`/`Missing` from whatever it
returns — unlike `rewrite_finding`, which hand-writes a per-engine match
because `rewrite::Resolution` carries a third state (`Unavailable`) a plain
`Result` cannot express. `stt::resolve_with`'s return type is already a
`Result`, so once its `"http"` arm stops returning `NotBuilt`, `doctor`'s
existing generic path reports the real state on its own. AC-9 is satisfied
by section 4 alone.

## 7. Test doubles

The same scratch `TcpListener` double `rewrite::http`'s own tests use
(`respond_once`/`respond_once_with_status`, `src/rewrite/http.rs`) — copied
or shared, the plan's call. A multipart request means the double also needs
to read a request whose body is not plain JSON; the same "drain the full
request, headers plus body, before responding" fix issue #36 already found
necessary (`tasks/36/RUN_36.md`) applies again here, so the double should be
copied along with that lesson already built in rather than rediscovered.

## Coverage against AC_16.md

| AC | Covered by |
|---|---|
| AC-1 | §4 |
| AC-2 | §4 (`EngineError::NotConfigured`, reused) |
| AC-3 | §2, §3 |
| AC-3a | §2 (`prompt` field) |
| AC-4 | §2 (`Authorization` header) |
| AC-5 | §1 (three distinct `HttpError` variants), §3 |
| AC-6 | §1, §3 (each variant's `Display` names what to check) |
| AC-7 | §3 (fixed timeout) |
| AC-8 | §7 |
| AC-9 | §6 |

## What this design does not decide

The exact wording of each `HttpError` variant's `Display` message, and
whether the multipart body is written by hand or through a small helper
function versus inline in `HttpEngine::transcribe` — both are mechanical
once this contract is fixed, and the plan assigns them.
