# DESIGN_36

Design for issue #36, against `tasks/36/AC_36.md` (gate S1: READY,
`tasks/36/RUN_36.md`).

## 1. New module: `src/rewrite.rs`

Mirrors `src/stt.rs`'s shape exactly — one trait, resolved once at daemon
start, submodules per engine:

```rust
pub trait Engine: Send + Sync {
    fn rewrite(&self, transcript: &str, bias: &str) -> Result<String, EngineError>;
}
```

`bias: &str` is included from the start, not added later the way #26 had to
widen `stt::Engine::transcribe` after the fact. Reason: the prototype's own
measurement (`docs/evidence.md`, "Context and its effect on the transcript")
already shows a rewrite step with context restores a term recognition alone
does not, and `bias::Collected.bias` (#21, merged) is already exactly the
string to pass — no new collection work, per `AC_36.md`'s own instruction to
decide against `bias::collect`'s existing shape rather than a new one.

`daemon::Runtime` gains one field:

```rust
pub rewrite: rewrite::Resolution,
```

```rust
pub enum Resolution {
    /// `[rewrite] engine = "off"`. The step is not invoked at all — not a
    /// failure, and never produces the tell-once notice.
    Off,
    /// `"http"` or `"command"`, successfully resolved at start.
    Engine(Box<dyn Engine + Send + Sync>),
    /// `"agent"` (out of this issue's scope), an unknown value, or a
    /// configured `http`/`command` engine that failed to resolve at start
    /// (empty url, empty command list). Carries what to tell the person,
    /// once.
    Unavailable(String),
}
```

Resolved once, in `start()`, the same place `Recognition` already is
(`src/daemon.rs:451` in this checkout) — mirroring the precedent
`tasks/21/PLAN_21.md`'s Task 8 already established for `bias_source`.

`Runtime` also gains a plain `skip_if_plain: bool` field, read once from
`[rewrite] skip_if_plain` at the same construction site — not a nested
`rewrite_settings` struct; `delivery_settings` groups two fields because
delivery already had two, and one bare `bool` needs no group of its own.

## 2. Pipeline wiring

In `daemon::transcribe` (`src/daemon.rs:303-338`), between `text` coming back
from recognition and `delivering_line`/`delivery::deliver` being called:

```rust
let text = match &runtime.rewrite {
    Resolution::Off => text,
    Resolution::Unavailable(why) => {
        tell_once(runtime, why);
        text
    }
    Resolution::Engine(engine) => {
        if skip::plain(&text, &collected.bias, runtime.skip_if_plain) {
            text
        } else {
            match engine.rewrite(&text, &collected.bias) {
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

This needs `transcribe` to receive `collected` (or at least `collected.bias`),
which today it does not — `dictate` (`:139-166`) discards `take_bias`'s
return exactly the way it did before #26 widened `daemon::transcribe` to take
a `bias: &str` parameter for recognition. **Sequencing note carried from
`AC_36.md`:** if #26 merges before this branch does, this task rebases onto a
`transcribe` that already threads `collected.bias` through for recognition,
and this design's own threading combines with it rather than re-adding a
second binding — the plan's task for this must say so explicitly, not assume
either order.

## 3. The tell-once mechanism

```rust
told: std::sync::atomic::AtomicBool,
```

A new `Runtime` field. `tell_once`:

```rust
fn tell_once(runtime: &Runtime, why: &str) {
    if runtime.told.swap(true, std::sync::atomic::Ordering::Relaxed) {
        return;
    }
    runtime.journal.write(&rewrite_unavailable_line(why));
    if runtime.delivery_settings.toasts {
        if let Err(toast_why) = runtime.deliverer.notify("Rewrite unavailable", why) {
            runtime.journal.write(&toast_failed_line(&toast_why.to_string().replace('\n', " ")));
        }
    }
}
```

Reuses `Deliverer::notify` (`src/delivery.rs:33`) exactly as delivery's own
failed-delivery toast already does (`src/daemon.rs:349-358`) — no new toast
mechanism, no new `Deliverer` method. `Relaxed` ordering is enough: `serve`
spawns a thread per connection (`src/daemon.rs:509-513`), so two takes
finishing at nearly the same moment on different connections is a real case,
not a hypothetical one — the worst case is two notices instead of one, which
is a cosmetic risk, not a correctness one. The rule is "not on every take,"
not "exactly once under concurrency," and `swap`'s atomicity is what keeps
the flag itself from tearing; it does not need to also serialize the two
notifications.

## 4. The skip heuristic (`skip_if_plain`)

No prototype measurement exists for this (`AC_36.md`'s "what this stage has
to settle"). Read literally, `docs/design.md`'s own sentence has two
conditions joined by "and" plus an implicit third: "**a short phrase**
containing **neither foreign terms nor names from the context** skips this
stage." Three checks, all must hold to skip:

1. **Short**: at most `SKIP_WORD_LIMIT` (8) whitespace-delimited words. A
   named constant, not a configuration key — the same footing
   `bias::transcript::TURN_CHARS` is on (`tasks/21/PLAN_21.md`'s per-turn cut).
2. **No foreign term**: no run of two or more consecutive ASCII Latin letters
   anywhere in the transcript.
3. **No context name**: no whitespace-delimited token in the transcript
   case-insensitively equals a whitespace-delimited token in
   `collected.bias`.

Why length matters and is not decoration: the measured failure case this
issue exists to fix ("Recognition, by hand on macOS") was a 70-second take —
far past any reasonable short-phrase threshold — so it hits the rewrite path
regardless of how precisely checks 2 and 3 are tuned. The length gate is what
makes an imprecise foreign-term/name check safe to ship: it only ever skips
short utterances, where a false skip costs a missed punctuation fix, not a
mangled technical term in a long take. `skip_if_plain` (config, default
`true`) gates whether this check runs at all; `false` always attempts the
configured engine.

```rust
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
```

(`has_latin_run`/`shares_a_word` are private helpers; the plan gives them
their own test-first steps.)

## 5. Configuration: new keys, provisional

`Rewrite` (`src/config.rs:67-70`) gains, on the same footing `[stt] command`
was introduced on (`docs/decisions.md`, 2026-08-25, #13 — provisional,
renameable on the owner's word):

```rust
pub struct Rewrite {
    pub engine: String,           // unchanged: "agent" | "http" | "command" | "off"
    pub agent: String,            // unchanged
    pub url: String,              // new. Empty means "http" is unconfigured.
    pub token: String,            // new. Empty means no Authorization header.
    pub model: String,            // new. Sent as-is in the request body.
    pub command: Vec<String>,     // new. {transcript}/{bias} substituted, mirrors [stt] command.
    pub skip_if_plain: bool,      // new. Default true.
}
```

`prompt_file` from `docs/design.md`'s aspirational table is **not** added by
this issue: nothing in `AC_36.md` asks for a customizable prompt, and the
fixed prompt below (§6) is a straight port of the one prototype measurement
this rests on. Adding a template-loading path with no measurement behind it
is exactly the kind of inflation `octoflow-assess`'s own rule warns against;
left for whoever needs it.

## 6. The `http` engine

One POST per take needing rewrite, to `[rewrite] url`, OpenAI-compatible chat
completions shape:

```json
{
  "model": "<[rewrite] model, may be empty>",
  "messages": [
    {"role": "system", "content": "<fixed prompt, ported from spike/spike.sh's rewrite(), adapted: context is the bias string, not four separate CTX_* fields>"},
    {"role": "user", "content": "<transcript>"}
  ],
  "temperature": 0
}
```

Read back `choices[0].message.content`, trimmed. Any of: connection refused,
non-2xx status, a body that does not parse as this shape, or an empty
`choices` array — all map to `EngineError::Failed`-equivalent (mirrors
`stt::command::CommandError::Failed`'s naming pattern), never a panic.

**HTTP client: `ureq`.** Blocking, no async runtime pulled in — matches this
repository's standing rule against asynchronous runtimes until a task cannot
be done without one (`docs/decisions.md`, 2026-08-24, #3; this is that task).
`ureq` defaults to `rustls`, so no platform-specific TLS dependency crosses
the three supported platforms. Decision recorded in `docs/decisions.md`
(Task 3 of the plan).

**Timeout:** a bound, the same reason #28 exists for delivery and
transcription — a wedged local server must turn into a message, not a leaked
thread. `ureq`'s agent-level timeout, set to a fixed duration (30 seconds;
longer than any measured recognition or rewrite round trip in
`docs/evidence.md`, short enough that a hang still turns into a message within
the client's own 2-minute reply bound rather than eating all of it).

Issue #16 (STT endpoint engine) is out of bounds here but will likely want the
same crate — noted, not coordinated further; whichever issue lands first adds
`ureq` to `Cargo.toml` and the other reuses it.

## 7. The `command` engine

Mirrors `stt::command`'s `render`/`CommandEngine` shape closely enough to
name directly, deliberately kept as its own small module rather than a shared
generic templating utility — two near-identical implementations do not yet
justify one, per this repository's own preference for following existing
patterns over speculative abstraction; if a third placeholder-substituting
engine appears later, extracting a shared helper becomes worth it then.

```rust
pub fn render(argv: &[String], transcript: &str, bias: &str) -> Vec<String>
```

`{transcript}` substitutes by name and is **force-appended** when the list
names no `{transcript}` placeholder — mirroring `{audio}`'s precedent in
`stt::command::render`: a rewrite program cannot do its one job without
receiving the transcript somehow. `{bias}` substitutes by name and is
**never** force-appended — mirroring `{prompt}`'s precedent from #26: an
opt-in enhancement, not something the program cannot run without.

## 8. `doctor::rewrite_finding` (AC-11, and the divergence the S1 gate noted)

`rewrite_finding`'s signature changes from `(engine: &str, agent: &str)` to
`(rewrite: &config::Rewrite)` — it needs `url` and `command` now, which the
two-`&str` shape cannot carry. Its one caller (`src/doctor.rs:336-339`)
updates to pass `&loaded.config.rewrite` instead of the two fields.

`"http"`: `Ok` if `[rewrite] url` is non-empty, else `Missing`, "give
`[rewrite]` a `url`, for example: ...". `"command"`: `Ok` if `[rewrite]
command` is non-empty, else `Missing`, mirroring `[stt] command`'s own
message shape. Neither probes the network or spawns the program — a static
configuration check, the same class `engine_finding_from` already does for
`[stt] engine`.

`"agent"`: the S1 gate found that treating `"agent"` as "no engine available"
on the take path, while `doctor` kept reporting `Ok` whenever the tool is on
`PATH`, leaves `doctor` telling a person the opposite of what a take actually
does. Fixed by changing the state, not just the wording: `rewrite_finding`
now reports `State::Missing` for `"agent"` even when the tool is found, with
a detail naming both facts — "`{found}` is on PATH, but this build does not
yet invoke the agent engine for rewrite; transcripts are delivered
unrewritten. Set `[rewrite] engine` to `\"http\"` or `\"command\"` for a
working engine, or `\"off\"` to silence this." `State::Missing` rather than a
new state variant: nothing else in `doctor` needs a fourth state for this one
case, and `Missing` is exactly true — a working rewrite is missing from this
configuration, regardless of what is on `PATH`.

## 9. Test doubles

`http`: a scratch `std::net::TcpListener` bound to `127.0.0.1:0` in a
background thread, reading one HTTP request and writing back a canned
response — no HTTP server crate as a test dependency, the same
no-live-dependency rule `bias::pane`'s scratch-script tests and
`stt::command`'s scratch-program tests already follow. `command`: a scratch
script, exactly `stt::command`'s own test pattern (and, if this branch lands
after #26 merges, `src/bias.rs`'s cross-platform `.sh`/`.cmd` scratch-script
shape — the plan names which to copy from once the merge order is known).

## 10. Coverage against AC_36.md

| AC | Covered by |
|---|---|
| AC-1 | §2 (the match arms cover `Off`/`Unavailable`/`Engine` for every non-off case) |
| AC-2 | §6 |
| AC-3 | §7 |
| AC-4 | §2 (`Resolution::Off` short-circuits before any of this issue's new code runs) |
| AC-5 | §4 |
| AC-6 | §2, §3 (`Unavailable` and a live engine error both route to `tell_once` and the original `text`) |
| AC-7 | §3 |
| AC-8 | §6, §7 (failure messages name the address/program and what to check, mirroring `stt::command::CommandError`'s style) |
| AC-9 | §9 |
| AC-10 | Not a design concern — the plan schedules the manual runs and the `docs/evidence.md` entry, the shape `tasks/21/PLAN_21.md`'s S5 task and `tasks/26/PLAN_26.md`'s S5 task both used |
| AC-11 | §8 |

## What this design does not decide

The exact wording of every error message and the precise constant
`SKIP_WORD_LIMIT`'s final value (8, reasoned in §4, not measured) are left to
implementation — both are mechanical once this contract is fixed. The exact
`docs/decisions.md` wording for the `ureq` pick is the plan's to draft, not
this document's.
