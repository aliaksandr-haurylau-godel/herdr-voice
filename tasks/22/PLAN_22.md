# PLAN_22 — Delivery: put the transcript in the pane without submitting it

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A take that finishes transcribing has its text inserted into the pane
that was pinned when it began — submitted only when configured to, journalled
and toasted on failure, and never dropped silently.

**Architecture:** `transcribe` (`src/daemon.rs`) becomes the one place that
turns a finished `Take` into both a delivery attempt and a client reply. A new
`src/delivery.rs` module carries the outward call to herdr behind a `Deliverer`
trait, the shape `src/stt.rs` already uses for the transcriber. A `Journal`
trait in `src/daemon.rs` makes the ordering between "text written down" and
"delivery attempted" checkable from a test. Four values resolved once at
daemon start — the recognition engine, the deliverer, delivery's settings, and
the journal — are bundled into one `Runtime` and threaded down the call chain
that already threads `Recognition` today.

**Tech Stack:** Rust 2021, `serde`/`serde_json`/`toml` (already dependencies),
`std::process::Command` for the outward call to `herdr`.

**Spec:** `tasks/22/DESIGN_22.md` (S2 READY, third pass, `tasks/22/RUN_22.md`)
is what every task below cuts from; task text points at its section numbers
rather than restating them. `tasks/22/AC_22.md` states the fourteen criteria.

## Global Constraints

- English throughout — code, comments, commits (`CLAUDE.md`, "Language").
- No panic paths in delivery: every failure is a `Result` and a message
  naming the reason, never a process abort (`CLAUDE.md`; AC-13).
- Every user-visible failure names what to do next (`CLAUDE.md`).
- Configuration keys have defaults; an absent file, or one that omits a table
  this plan adds, is valid (`CLAUDE.md`; AC-1, R7).
- No test may require a live herdr, a microphone or a model (AC-14).
- The reply protocol reads one line (`src/proto.rs:158-177`); no reply built
  here may depend on an embedded newline surviving — DESIGN_22.md §3 collapses
  newlines in the failure reply for exactly this reason. Issue #19, the
  protocol's own truncation, is not this plan's to fix.
- `cargo test`, `cargo clippy --all-targets -- -D warnings`, `cargo fmt
  --check` pass after every task.

## Task table

| task | produces | depends on |
|---|---|---|
| 1 config | `config::Ui{toasts}`, `config::Delivery{submit}`, both defaulted | — |
| 2 capture: `Take.agent` | `Take.agent`, `Recorder::start(target, agent)` | — |
| 3 capture: `ToneSource` | `capture::tests_support::ToneSource` | — |
| 4 delivery: trait + `deliver()` + fake | `delivery::{Deliverer, DeliveryError, Settings, deliver, tests_support::FakeDeliverer}` | — |
| 5 delivery: `HerdrDeliverer` | `delivery::HerdrDeliverer`, `extract_reason` | 4 |
| 6 daemon: `Journal` | `daemon::{Journal, StderrJournal, delivering_line, delivery_failed_line}` | — |
| 7 daemon: wire `Runtime` | `daemon::Runtime`; `transcribe` calls `delivery::deliver`; both reply shapes; the journal order; the toast gate | 1 (`config::Ui`/`Delivery` read into `Runtime`), 2 (`Take.agent`, `Recorder::start`'s shape), 3 (`ToneSource`, every test needing `transcribe`'s success path), 4 and 5 (`delivery::deliver`, `HerdrDeliverer`, `Settings`, `FakeDeliverer`), 6 (`Journal`, the two line functions) |
| 8 S4 gate | review verdict in `RUN_22.md` | 7 |
| 9 S5 | `docs/evidence.md` entry, run closed in `RUN_22.md` | 8 |

`Task 7 depends on every other code task` is not decorative: `Runtime`
(DESIGN_22.md §5) is the struct that bundles `Recognition` (already present),
`Box<dyn delivery::Deliverer>` (task 4/5), `delivery::Settings` built from
`config::Delivery`/`config::Ui` (task 1), and `Box<dyn Journal>` (task 6);
`transcribe` cannot call `delivery::deliver` with `take.agent` (task 2) until
all four exist, and no test in task 7 can reach `transcribe`'s success path
without `ToneSource` (task 3).

---

### Task 1: Configuration — `[ui] toasts` and `[delivery] submit`

**Files:** Modify `src/config.rs:19-25` (`Config`); tests in the same file's
`#[cfg(test)] mod tests`.

**Produces:** `pub struct Ui { pub toasts: bool }` (default `true`),
`pub struct Delivery { pub submit: bool }` (default `false`), added to
`Config` as `ui: Ui, delivery: Delivery`. DESIGN_22.md §7.

- [ ] Write failing tests, next to `every_key_has_a_default`:

```rust
#[test]
fn the_ui_and_delivery_defaults_are_set() {
    let defaults = Config::default();
    assert!(defaults.ui.toasts);
    assert!(!defaults.delivery.submit);
}

#[test]
fn a_file_that_sets_neither_table_keeps_both_defaults() {
    let directory = scratch("ui-delivery-absent");
    std::fs::write(directory.join("config.toml"), "[stt]\nmodel = \"small\"\n").unwrap();
    let loaded = load(Some(&directory));
    assert!(loaded.config.ui.toasts);
    assert!(!loaded.config.delivery.submit);
}

#[test]
fn the_ui_and_delivery_tables_are_read_when_present() {
    let directory = scratch("ui-delivery-set");
    std::fs::write(
        directory.join("config.toml"),
        "[ui]\ntoasts = false\n\n[delivery]\nsubmit = true\n",
    )
    .unwrap();
    let loaded = load(Some(&directory));
    assert!(!loaded.config.ui.toasts);
    assert!(loaded.config.delivery.submit);
}
```

- [ ] Run `cargo test --lib config::` — fails to compile (no `ui`/`delivery` field).
- [ ] Add, next to `Rewrite`/`Default for Rewrite`:

```rust
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default)]
pub struct Ui {
    pub toasts: bool,
}
impl Default for Ui {
    fn default() -> Self { Ui { toasts: true } }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default)]
pub struct Delivery {
    pub submit: bool,
}
impl Default for Delivery {
    fn default() -> Self { Delivery { submit: false } }
}
```

and add `ui: Ui, delivery: Delivery` fields to `Config` (`src/config.rs:19-25`).

- [ ] Run `cargo test --lib config::` — passes.
- [ ] Commit: `git add src/config.rs && git commit -m "config: read [ui] toasts and [delivery] submit, both defaulted"`

---

### Task 2: `Take` carries the pane's agent, pinned at take start

**Files:** Modify `src/capture.rs:93-101` (`Take`), `:149-157` (`enum
Command`), `:166-172` (`struct Running`), `:215-229` (`Recorder::start`),
`:248-303` (`start_one`), `:305-348` (`stop_one`); `src/daemon.rs:91-101`
(`dictate`, one call site — see caveat below). DESIGN_22.md §2.

**Produces:** `Take.agent: Option<String>`; `Recorder::start(&self, target:
&str, agent: Option<&str>) -> Started`.

- [ ] Write failing tests, next to `a_take_starts_stops_and_lands_on_disk`:

```rust
#[test]
fn a_take_started_with_an_agent_reports_it() {
    let (recorder, _) = recorder_with("agent-yes", vec![Event::Samples(tone(0.3, 0.1))]);
    assert_eq!(recorder.start("w1:p2", Some("claude")), Started::Began);
    let take = recorder.stop().expect("a take");
    assert_eq!(take.agent.as_deref(), Some("claude"));
    std::fs::remove_file(&take.path).ok();
}

#[test]
fn a_take_started_with_no_agent_reports_none() {
    let (recorder, _) = recorder_with("agent-no", vec![Event::Samples(tone(0.3, 0.1))]);
    assert_eq!(recorder.start("w1:p2", None), Started::Began);
    let take = recorder.stop().expect("a take");
    assert_eq!(take.agent, None);
    std::fs::remove_file(&take.path).ok();
}
```

- [ ] Run `cargo test --lib capture::` — fails to compile.
- [ ] Thread `agent` beside `target` end to end: `Take` gains `pub agent:
      Option<String>`; `Command::Start` gains `agent: Option<String>`;
      `Running` gains `agent: Option<String>`; `Recorder::start` gains the
      parameter and forwards it in the `Command::Start` it sends;
      `start_one` gains the parameter and stores it into `Running` next to
      `target`; `stop_one` copies `take.agent` into the `Take` it returns
      next to `target`. In `src/daemon.rs`, `dictate`'s one call becomes
      `recorder.start(pane, None)`.

  **This `None` is a temporary lie, not a finished feature.** Nothing calls
  `Recorder::start` with a real agent name until task 7 replaces this `None`
  with the agent threaded from `answer` through `dictate`. Between this
  task's commit and task 7's, `Take.agent` is always `None` for every take
  the running daemon produces, so `[delivery] submit = true` cannot submit
  to anything yet — it falls back to inserting on every pane. That is not a
  regression fixed later; it is the daemon's current behavior (no
  submitting at all), left unchanged on purpose. Tasks 2 and 7 are one
  change to `Recorder`'s shape and the call chain, split into two commits so
  each has its own test — task 2 proves `Take.agent` is carried correctly
  given a name; task 7 proves the daemon reads a real invocation's agent
  rather than always passing `None`.

- [ ] Update every `recorder.start("w1:p2")` call site in `capture.rs`'s
      existing tests to `recorder.start("w1:p2", None)` (none of the
      existing tests assert on the agent, so `None` is correct for all of
      them).
- [ ] Run `cargo test --lib capture:: daemon::` — passes.
- [ ] Commit: `git add src/capture.rs src/daemon.rs && git commit -m "capture: carry the pinned pane's agent on Take, next to target"`

---

### Task 3: `capture::tests_support::ToneSource`

**Files:** Modify `src/capture.rs:366-384` (`tests_support`). DESIGN_22.md §4a.

**Produces:** `pub struct ToneSource` (public `Source` any module can build a
`Recorder` with — task 7 uses it to reach `transcribe`'s success path).

- [ ] Write the failing test:

```rust
#[test]
fn tone_source_clears_the_silence_floor() {
    let recorder = Recorder::spawn(
        || Box::new(tests_support::ToneSource),
        Audio::default(),
        takes_dir("tone-source"),
    );
    assert_eq!(recorder.start("w1:p2", None), Started::Began);
    let take = recorder.stop().expect("a take that clears the floor");
    assert!(take.level_dbfs > Audio::default().silence_db, "got {}", take.level_dbfs);
    std::fs::remove_file(&take.path).ok();
}
```

- [ ] Run `cargo test --lib capture::tests::tone_source_clears_the_silence_floor` — fails (no `ToneSource`).
- [ ] Add, next to `SilentSource` in `tests_support`:

```rust
/// Hears one loud moment and stops — a take that clears the silence floor, so a
/// test past capture can reach recognition and delivery without a microphone.
pub struct ToneSource;

impl Source for ToneSource {
    fn start(&mut self, _device: Option<&str>, sink: Sink) -> Result<Format, String> {
        let samples: Vec<f32> = (0..4_800)
            .map(|i| 0.3 * (i as f32 * std::f32::consts::TAU * 440.0 / 48_000.0).sin())
            .collect();
        sink.push(Event::Samples(samples));
        Ok(Format { rate: 48_000, channels: 1 })
    }
    fn stop(&mut self) {}
}
```

- [ ] Run `cargo test --lib capture::` — passes.
- [ ] Commit: `git add src/capture.rs && git commit -m "capture: export ToneSource, the minimal audible fake for tests outside this module"`

---

### Task 4: `src/delivery.rs` — the `Deliverer` trait, `DeliveryError`, `deliver()`, a fake

**Files:** Create `src/delivery.rs`; modify `src/main.rs:11-20` (add `mod
delivery;`); tests in `src/delivery.rs`'s own `#[cfg(test)] mod tests`.
DESIGN_22.md §4.

**Produces:**
- `pub trait Deliverer: Send + Sync { fn insert(&self, pane: &str, text:
  &str) -> Result<(), DeliveryError>; fn submit(&self, pane: &str, text:
  &str) -> Result<(), DeliveryError>; fn notify(&self, title: &str, body:
  &str) -> Result<(), DeliveryError>; }`
- `pub enum DeliveryError { Rejected(String), NotFound { binary: String,
  path: String } }` with `Display`/`std::error::Error`.
- `pub struct Settings { pub submit: bool, pub toasts: bool }`
- `pub fn deliver(deliverer: &dyn Deliverer, submit: bool, agent: Option<&str>,
  pane: &str, text: &str) -> Result<(), DeliveryError>`
- `delivery::tests_support::FakeDeliverer` — `Clone` (its call log lives
  behind an inner `Arc<Mutex<Vec<Call>>>`), so a test can hold one clone and
  move another into a `Runtime`, then read `.calls()` on the one it kept.
  Task 7 wires this in exactly as `stt::tests_support::Fake` is wired today.

**`DeliveryError::Rejected` carries the extracted code alone** — never
herdr's own message, which restates the pane the reply already names via
`{target}` (DESIGN_22.md §3, §4). Its `Display` prints that code verbatim.
**`DeliveryError::NotFound` follows the fuller wording
`stt::command::CommandError::NotFound` already uses** (`src/stt/command.rs:61-86`):
name the binary, the `PATH` searched, and what to do.

- [ ] Add `mod delivery;` to `src/main.rs` after `mod daemon;`.
- [ ] Write the failing tests in a new `src/delivery.rs`:

```rust
//! Putting a take's text into a pane, or telling somebody it could not go
//! there. Mirrors the shape src/stt.rs uses for the transcriber.

#[cfg(test)]
mod tests {
    use super::tests_support::{Call, FakeDeliverer};
    use super::*;

    #[test]
    fn insert_only_never_calls_submit() {
        let fake = FakeDeliverer::ok();
        assert_eq!(deliver(&fake, false, Some("claude"), "w1:p2", "hello"), Ok(()));
        assert_eq!(fake.calls(), vec![Call::Insert("w1:p2".into(), "hello".into())]);
    }

    #[test]
    fn submit_calls_submit_when_an_agent_is_named() {
        let fake = FakeDeliverer::ok();
        assert_eq!(deliver(&fake, true, Some("claude"), "w1:p2", "hello"), Ok(()));
        assert_eq!(fake.calls(), vec![Call::Submit("w1:p2".into(), "hello".into())]);
    }

    #[test]
    fn submit_falls_back_to_insert_when_no_agent_is_named() {
        let fake = FakeDeliverer::ok();
        assert_eq!(deliver(&fake, true, None, "w1:p2", "hello"), Ok(()));
        assert_eq!(fake.calls(), vec![Call::Insert("w1:p2".into(), "hello".into())]);
    }

    #[test]
    fn a_rejected_call_is_propagated_unchanged() {
        let fake = FakeDeliverer::failing(DeliveryError::Rejected("pane_not_found".into()));
        let result = deliver(&fake, false, None, "w99:p99", "hello");
        assert_eq!(result, Err(DeliveryError::Rejected("pane_not_found".into())));
    }
}
```

- [ ] Run `cargo test --lib delivery::` — fails to compile.
- [ ] Implement, above the test module:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeliveryError {
    /// The code alone, extracted from herdr's structured refusal — see
    /// docs/evidence.md, "Delivering into a pane that is gone" — or the raw
    /// output when it did not parse as that shape.
    Rejected(String),
    /// `herdr` itself could not be started.
    NotFound { binary: String, path: String },
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
        }
    }
}
impl std::error::Error for DeliveryError {}

pub trait Deliverer: Send + Sync {
    fn insert(&self, pane: &str, text: &str) -> Result<(), DeliveryError>;
    fn submit(&self, pane: &str, text: &str) -> Result<(), DeliveryError>;
    fn notify(&self, title: &str, body: &str) -> Result<(), DeliveryError>;
}

/// `[delivery] submit` and `[ui] toasts`, resolved once at daemon start.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Settings {
    pub submit: bool,
    pub toasts: bool,
}

/// `submit` is `Settings.submit`; `agent` is `Take.agent` — neither is
/// looked up again here.
pub fn deliver(
    deliverer: &dyn Deliverer,
    submit: bool,
    agent: Option<&str>,
    pane: &str,
    text: &str,
) -> Result<(), DeliveryError> {
    if submit && agent.is_some() {
        deliverer.submit(pane, text)
    } else {
        deliverer.insert(pane, text)
    }
}

#[cfg(test)]
pub mod tests_support {
    use super::{DeliveryError, Deliverer};
    use std::sync::{Arc, Mutex};

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum Call {
        Insert(String, String),
        Submit(String, String),
        Notify(String, String),
    }

    /// Records every call, in order, and returns one fixed result for every
    /// call. `Clone`, sharing its log through an inner `Arc`, so a test can
    /// keep one clone while another is moved into a `Runtime`.
    #[derive(Clone)]
    pub struct FakeDeliverer {
        result: Result<(), DeliveryError>,
        calls: Arc<Mutex<Vec<Call>>>,
    }

    impl FakeDeliverer {
        pub fn ok() -> Self {
            FakeDeliverer { result: Ok(()), calls: Arc::new(Mutex::new(Vec::new())) }
        }
        pub fn failing(with: DeliveryError) -> Self {
            FakeDeliverer { result: Err(with), calls: Arc::new(Mutex::new(Vec::new())) }
        }
        pub fn calls(&self) -> Vec<Call> {
            self.calls.lock().unwrap().clone()
        }
    }

    impl Deliverer for FakeDeliverer {
        fn insert(&self, pane: &str, text: &str) -> Result<(), DeliveryError> {
            self.calls.lock().unwrap().push(Call::Insert(pane.to_string(), text.to_string()));
            self.result.clone()
        }
        fn submit(&self, pane: &str, text: &str) -> Result<(), DeliveryError> {
            self.calls.lock().unwrap().push(Call::Submit(pane.to_string(), text.to_string()));
            self.result.clone()
        }
        fn notify(&self, title: &str, body: &str) -> Result<(), DeliveryError> {
            self.calls.lock().unwrap().push(Call::Notify(title.to_string(), body.to_string()));
            Ok(())
        }
    }
}
```

- [ ] Run `cargo test --lib delivery::` — passes (4 tests).
- [ ] Commit: `git add src/main.rs src/delivery.rs && git commit -m "delivery: add the Deliverer trait, deliver()'s branching, and a fake"`

---

### Task 5: `HerdrDeliverer` — the real call to `herdr`

**Files:** Modify `src/delivery.rs` (adds to task 4's module). DESIGN_22.md §4.

**Produces:** `pub struct HerdrDeliverer`, built with `HerdrDeliverer::new()`,
implementing `Deliverer` by shelling out to `herdr pane send-text` / `herdr
agent prompt` / `herdr notification show ... --body ...`, via `HERDR_BIN_PATH`
with `"herdr"` fallback (mirrors `src/doctor.rs:106`).

**The two literal shapes `extract_reason` parses**, measured in
`docs/evidence.md:460-461` ("Delivering into a pane that is gone"):

```
{"error":{"code":"pane_not_found","message":"pane w99:p99 not found"}}
{"error":{"code":"agent_not_found","message":"agent target w99:p99 not found"}}
```

`extract_reason` returns the `code` field alone (`"pane_not_found"` /
`"agent_not_found"`) when the text parses as that shape, and the trimmed raw
text otherwise — this is what makes `DeliveryError::Rejected`'s code-only
`Display` (task 4) hold for the `(pane_not_found)` in the failure reply
(DESIGN_22.md §3).

- [ ] Write the failing tests, using the two shapes above verbatim:

```rust
#[test]
fn a_structured_rejection_yields_its_code() {
    let text = br#"{"error":{"code":"pane_not_found","message":"pane w99:p99 not found"}}"#;
    assert_eq!(extract_reason(text), "pane_not_found");
    let text = br#"{"error":{"code":"agent_not_found","message":"agent target w99:p99 not found"}}"#;
    assert_eq!(extract_reason(text), "agent_not_found");
}

#[test]
fn text_that_is_not_the_structured_shape_is_kept_as_is() {
    assert_eq!(extract_reason(b"herdr: unknown flag --bogus\n"), "herdr: unknown flag --bogus");
}
```

- [ ] Run `cargo test --lib delivery::` — fails to compile (`extract_reason` absent).
- [ ] Implement:

```rust
fn herdr_binary() -> String {
    std::env::var("HERDR_BIN_PATH").unwrap_or_else(|_| "herdr".to_string())
}

fn extract_reason(output: &[u8]) -> String {
    #[derive(serde::Deserialize)]
    struct Envelope { error: ErrorBody }
    #[derive(serde::Deserialize)]
    struct ErrorBody { code: String }

    let text = String::from_utf8_lossy(output).trim().to_string();
    match serde_json::from_str::<Envelope>(&text) {
        Ok(envelope) => envelope.error.code,
        Err(_) => text,
    }
}

pub struct HerdrDeliverer {
    binary: String,
}

impl HerdrDeliverer {
    pub fn new() -> Self {
        HerdrDeliverer { binary: herdr_binary() }
    }

    fn run(&self, args: &[&str]) -> Result<(), DeliveryError> {
        match std::process::Command::new(&self.binary).args(args).output() {
            // herdr starts plugin commands with a minimal PATH — the same
            // reasoning src/stt/command.rs:78-86 states for the transcriber.
            Err(_) => Err(DeliveryError::NotFound {
                binary: self.binary.clone(),
                path: std::env::var("PATH").unwrap_or_default(),
            }),
            Ok(output) if output.status.success() => Ok(()),
            Ok(output) => {
                let text = if !output.stdout.is_empty() { &output.stdout } else { &output.stderr };
                Err(DeliveryError::Rejected(extract_reason(text)))
            }
        }
    }
}

impl Default for HerdrDeliverer {
    fn default() -> Self { Self::new() }
}

impl Deliverer for HerdrDeliverer {
    fn insert(&self, pane: &str, text: &str) -> Result<(), DeliveryError> {
        self.run(&["pane", "send-text", pane, text])
    }
    fn submit(&self, pane: &str, text: &str) -> Result<(), DeliveryError> {
        self.run(&["agent", "prompt", pane, text])
    }
    fn notify(&self, title: &str, body: &str) -> Result<(), DeliveryError> {
        self.run(&["notification", "show", title, "--body", body])
    }
}
```

- [ ] Run `cargo test --lib delivery::` — passes; `cargo clippy --all-targets -- -D warnings` — clean.
- [ ] Commit: `git add src/delivery.rs && git commit -m "delivery: add HerdrDeliverer, the real call to herdr pane send-text / agent prompt"`

---

### Task 6: `daemon::Journal` and the two lines delivery writes

**Files:** Modify `src/daemon.rs`, next to `context_note`/`request_line`
(after line 150); tests in `src/daemon.rs`'s own `#[cfg(test)] mod tests`.
DESIGN_22.md §5.

**Produces:** `pub trait Journal: Send + Sync { fn write(&self, line: &str);
}`; `pub struct StderrJournal` (writes via `eprintln!`); `pub fn
delivering_line(text: &str) -> String`; `pub fn delivery_failed_line(target:
&str, why: &str) -> String`.

- [ ] Write failing tests, next to `the_recorded_line_names_the_command_and_the_entrypoint`:

```rust
#[test]
fn the_delivering_line_carries_the_text() {
    assert!(delivering_line("fix the worklog entry").contains("fix the worklog entry"));
}

#[test]
fn the_delivery_failed_line_names_the_pane_and_the_reason() {
    let line = delivery_failed_line("w99:p99", "pane_not_found");
    assert!(line.contains("w99:p99"));
    assert!(line.contains("pane_not_found"));
}
```

- [ ] Run `cargo test --lib daemon::` — fails to compile.
- [ ] Implement:

```rust
/// Where a journal line goes. Production writes to standard error, the
/// channel request_line/context_note already use; a test substitutes
/// something it can read back, in order, without touching real stderr.
pub trait Journal: Send + Sync {
    fn write(&self, line: &str);
}

pub struct StderrJournal;
impl Journal for StderrJournal {
    fn write(&self, line: &str) { eprintln!("{line}"); }
}

/// Written before a delivery attempt, so the text is not held only in
/// memory while the outward call to herdr runs.
pub fn delivering_line(text: &str) -> String {
    format!("delivering: {text}")
}

/// Written when a delivery attempt is rejected.
pub fn delivery_failed_line(target: &str, why: &str) -> String {
    format!("delivery failed: pane={target} reason={why}")
}
```

- [ ] Run `cargo test --lib daemon::` — passes.
- [ ] Commit: `git add src/daemon.rs && git commit -m "daemon: add the Journal trait and the two lines delivery will write"`

---

### Task 7: Wire delivery into `transcribe`

**Depends on:** tasks 1–6, per the task table above.

**Why one task:** `answer`/`dictate`/`transcribe` change together as one
signature threaded through `start → serve → serve_one → answer → dictate →
transcribe`; splitting the plumbing from the behavior it enables would leave
an intermediate task with no failing test to write — a behavior-preserving
refactor has none. Each criterion below still gets its own red-green cycle
inside this one task.

**Files:** Modify `src/daemon.rs:46` (add `Runtime` next to `Recognition`),
`:48-84` (`answer`), `:91-101` (`dictate`), `:104-126` (`transcribe`),
`:152-188` (`start`), `:190-218` (`serve`), `:220-247` (`serve_one`), and its
`#[cfg(test)] mod tests` (every helper/call site using the old
`&Recognition` shape). DESIGN_22.md §1, §3, §5.

**The three exact strings this task's tests assert on** (DESIGN_22.md §3, §5):

- Success reply (unchanged): `format!("delivered to {} [{:.1} dB]", take.target, take.level_dbfs)`
- Failure reply: `format!("could not deliver to {} ({why}) — the take is kept at {}; text: {text}", take.target, take.path.display())`
  — with `why` and `text` newline-collapsed to single spaces (`.replace('\n', " ")`)
  before the string is built, and the transcript last, so a newline inside it
  costs only its own tail, never the reason or the path.
- Toast: title `"Delivery failed"`, body `format!("{}: the text is in the plugin log", take.target)`.

**The failure reply must be `Reply::Error`, not `Reply::Ok`.** `client::outcome`
(`src/client.rs:58-65`) maps `Reply::Ok` to exit 0 and `Reply::Error` to exit
1; both existing failure branches of `transcribe` already return
`Reply::Error`, and the new one does too — a failure that read as success on
exit code is exactly the silent-failure class `CLAUDE.md` weighs heavily.

#### Step group A — `Runtime` replaces `Recognition`, no new behavior

- [ ] Baseline: run `cargo test --lib daemon::tests::dictate_with_a_pane_starts_a_take_and_names_the_pane` — passes on the old shape.
- [ ] Add, next to the `Recognition` alias:

```rust
/// The four things resolved once, at daemon start, and needed everywhere a
/// take can finish.
pub struct Runtime {
    pub recognition: Recognition,
    pub deliverer: Box<dyn crate::delivery::Deliverer>,
    pub delivery_settings: crate::delivery::Settings,
    pub journal: Box<dyn Journal>,
}
```

- [ ] Change `answer(request, recorder, runtime: &Runtime)`: the
      `dictate`-matching arm becomes `dictate(recorder, runtime, pane,
      invocation.focused_pane_agent.as_deref())`.
- [ ] Change `dictate(recorder, runtime: &Runtime, pane: &str, agent:
      Option<&str>)`: calls `recorder.start(pane, agent)`; on
      `Started::AlreadyRunning`, `recorder.stop()` then `transcribe(runtime,
      &take)`.
- [ ] Change `transcribe`'s signature to `transcribe(runtime: &Runtime,
      take: &crate::capture::Take)`, body unchanged for now (still just
      replies with the raw transcript — step group B rewrites the body).
- [ ] Change `serve_one`, `serve`, `start` to carry `&Runtime`/`Arc<Runtime>`
      in place of `&Recognition`/`Arc<Recognition>`, unchanged otherwise. In
      `start`, after `recognition` is resolved, build:

```rust
let runtime = Runtime {
    recognition,
    deliverer: Box::new(crate::delivery::HerdrDeliverer::new()),
    delivery_settings: crate::delivery::Settings {
        submit: loaded.config.delivery.submit,
        toasts: loaded.config.ui.toasts,
    },
    journal: Box::new(StderrJournal),
};
serve(listener, address, Arc::new(recorder), Arc::new(runtime));
```

- [ ] Replace the test helper `fake_recognition` with:

```rust
fn fake_runtime(text: &str) -> Runtime {
    Runtime {
        recognition: Ok(Box::new(crate::stt::tests_support::Fake(Ok(text.to_string())))),
        deliverer: Box::new(crate::delivery::tests_support::FakeDeliverer::ok()),
        delivery_settings: crate::delivery::Settings { submit: false, toasts: false },
        journal: Box::new(StderrJournal),
    }
}
```

  and update every existing test call from `&fake_recognition("...")` /
  `Arc::new(fake_recognition("..."))` to `&fake_runtime("...")` /
  `Arc::new(fake_runtime("..."))`.
- [ ] Run `cargo test --lib daemon::` — passes, including the baseline test; no behavior changed yet.
- [ ] Commit: `git add src/daemon.rs && git commit -m "daemon: thread Runtime (recognition, deliverer, delivery settings, journal) in place of Recognition"`

#### Step group B — `transcribe` calls delivery: AC-2, AC-3, AC-4, AC-6, AC-10, AC-11, AC-12

- [ ] Write the failing tests. All need a take that clears the silence
      floor, so they build their own recorder with `ToneSource` (task 3):

```rust
fn tone_recorder(tag: &str) -> Recorder {
    Recorder::spawn(
        || Box::new(crate::capture::tests_support::ToneSource),
        crate::config::Audio::default(),
        std::env::temp_dir().join(format!("daemon-takes-{tag}-{}", std::process::id())),
    )
}

fn dictate_request() -> Request {
    request("dictate", br#"{"focused_pane_id":"w1:p2","focused_pane_agent":"claude"}"#)
}

fn runtime_with(deliverer: crate::delivery::tests_support::FakeDeliverer, submit: bool) -> Runtime {
    Runtime {
        recognition: Ok(Box::new(crate::stt::tests_support::Fake(Ok("fix the worklog entry".to_string())))),
        deliverer: Box::new(deliverer),
        delivery_settings: crate::delivery::Settings { submit, toasts: false },
        journal: Box::new(StderrJournal),
    }
}

#[test]
fn a_finished_take_reaches_delivery_and_the_reply_confirms_the_pane_not_the_text() {
    let recorder = tone_recorder("delivered");
    let runtime = runtime_with(crate::delivery::tests_support::FakeDeliverer::ok(), false);
    let request = dictate_request();
    answer(&request, &recorder, &runtime);
    let (reply, _) = answer(&request, &recorder, &runtime);
    match reply {
        Reply::Ok(text) => {
            assert!(text.contains("delivered to w1:p2"), "got {text:?}");
            assert!(text.contains("dB"), "got {text:?}");
            assert!(!text.contains("fix the worklog entry"), "got {text:?}");
        }
        other => panic!("expected a confirmation, got {other:?}"),
    }
}

#[test]
fn submit_off_inserts_and_never_submits() {
    let recorder = tone_recorder("insert-only");
    let fake = crate::delivery::tests_support::FakeDeliverer::ok();
    let runtime = runtime_with(fake.clone(), false);
    let request = dictate_request();
    answer(&request, &recorder, &runtime);
    answer(&request, &recorder, &runtime);
    assert_eq!(
        fake.calls(),
        vec![crate::delivery::tests_support::Call::Insert("w1:p2".into(), "fix the worklog entry".into())]
    );
}

#[test]
fn submit_on_with_an_agent_submits() {
    let recorder = tone_recorder("submit");
    let fake = crate::delivery::tests_support::FakeDeliverer::ok();
    let runtime = runtime_with(fake.clone(), true); // dictate_request() names "claude"
    let request = dictate_request();
    answer(&request, &recorder, &runtime);
    answer(&request, &recorder, &runtime);
    assert_eq!(
        fake.calls(),
        vec![crate::delivery::tests_support::Call::Submit("w1:p2".into(), "fix the worklog entry".into())]
    );
}

#[test]
fn submit_on_with_no_agent_falls_back_to_insert() {
    let recorder = tone_recorder("fallback");
    let fake = crate::delivery::tests_support::FakeDeliverer::ok();
    let runtime = runtime_with(fake.clone(), true);
    let request = request("dictate", br#"{"focused_pane_id":"w1:p2"}"#); // no agent
    answer(&request, &recorder, &runtime);
    answer(&request, &recorder, &runtime);
    assert_eq!(
        fake.calls(),
        vec![crate::delivery::tests_support::Call::Insert("w1:p2".into(), "fix the worklog entry".into())]
    );
}

#[test]
fn a_rejected_call_fails_the_delivery_keeps_the_audio_and_carries_the_text_and_the_reason() {
    let recorder = tone_recorder("rejected");
    let fake = crate::delivery::tests_support::FakeDeliverer::failing(
        crate::delivery::DeliveryError::Rejected("pane_not_found".into()),
    );
    let runtime = runtime_with(fake, false);
    let request = dictate_request();
    answer(&request, &recorder, &runtime);
    let (reply, _) = answer(&request, &recorder, &runtime);
    let text = match reply {
        Reply::Error(text) => text,
        other => panic!("expected a failed delivery (Reply::Error), got {other:?}"),
    };
    assert!(text.contains("could not deliver to w1:p2 (pane_not_found)"), "got {text:?}");
    assert!(text.contains("text: fix the worklog entry"), "got {text:?}");
    let path = text.split("kept at ").nth(1).unwrap().split(';').next().unwrap();
    assert!(std::path::Path::new(path).exists(), "AC-10: {path}");
    std::fs::remove_file(path).ok();
}
```

- [ ] Run `cargo test --lib daemon::` — fails: `transcribe`'s body still
      just replies with the raw transcript, so none of the new strings
      appear.
- [ ] Rewrite `transcribe`:

```rust
/// A finished take becomes text, and the text is delivered — or, if either
/// step fails, the reply is a Reply::Error naming why and what to do next
/// (client::outcome maps Reply::Ok to exit 0, Reply::Error to exit 1).
fn transcribe(runtime: &Runtime, take: &crate::capture::Take) -> Reply {
    let engine = match &runtime.recognition {
        Ok(engine) => engine,
        Err(why) => {
            return Reply::Error(format!("{why} — the take is kept at {}", take.path.display()))
        }
    };
    let text = match engine.transcribe(&take.path) {
        Ok(text) => text,
        Err(why) => {
            return Reply::Error(format!("{why} — the take is kept at {}", take.path.display()))
        }
    };

    // Written before the delivery attempt: the text must not be held only
    // in memory while the outward call to herdr runs.
    runtime.journal.write(&delivering_line(&text));

    match crate::delivery::deliver(
        runtime.deliverer.as_ref(),
        runtime.delivery_settings.submit,
        take.agent.as_deref(),
        &take.target,
        &text,
    ) {
        Ok(()) => Reply::Ok(format!("delivered to {} [{:.1} dB]", take.target, take.level_dbfs)),
        Err(why) => {
            let why = why.to_string().replace('\n', " ");
            runtime.journal.write(&delivery_failed_line(&take.target, &why));
            if runtime.delivery_settings.toasts {
                // A toast that could not be shown must not stop the journal
                // line or the client's reply from getting through.
                let _ = runtime
                    .deliverer
                    .notify("Delivery failed", &format!("{}: the text is in the plugin log", take.target));
            }
            Reply::Error(format!(
                "could not deliver to {} ({why}) — the take is kept at {}; text: {}",
                take.target,
                take.path.display(),
                text.replace('\n', " "),
            ))
        }
    }
}
```

- [ ] Run `cargo test --lib daemon::` — passes, all five new tests plus every
      test from step group A.
- [ ] Commit: `git add src/daemon.rs && git commit -m "daemon: call delivery from transcribe; the reply confirms or carries the text and the reason"`

#### Step group C — journal order and the toast gate: AC-7, AC-8, AC-9

- [ ] Write the failing tests:

```rust
/// Records every line written, in order.
#[derive(Default)]
struct RecordingJournal(std::sync::Mutex<Vec<String>>);
impl Journal for RecordingJournal {
    fn write(&self, line: &str) { self.0.lock().unwrap().push(line.to_string()); }
}
/// Lets a Runtime own a Journal while the test keeps its own handle to read
/// what was written — the same shape FakeDeliverer::clone() gives above.
struct TestJournal(std::sync::Arc<RecordingJournal>);
impl Journal for TestJournal {
    fn write(&self, line: &str) { self.0.write(line); }
}

#[test]
fn the_delivering_line_precedes_the_failure_line_and_names_the_reason() {
    let recorder = tone_recorder("journal-order");
    let fake = crate::delivery::tests_support::FakeDeliverer::failing(
        crate::delivery::DeliveryError::Rejected("pane_not_found".into()),
    );
    let journal = std::sync::Arc::new(RecordingJournal::default());
    let mut runtime = runtime_with(fake, false);
    runtime.journal = Box::new(TestJournal(std::sync::Arc::clone(&journal)));
    let request = dictate_request();
    answer(&request, &recorder, &runtime);
    answer(&request, &recorder, &runtime);

    let lines = journal.0.lock().unwrap();
    assert_eq!(lines.len(), 2, "got {lines:?}");
    assert!(lines[0].contains("fix the worklog entry"), "got {lines:?}");
    assert!(lines[1].contains("w1:p2") && lines[1].contains("pane_not_found"), "got {lines:?}");
}

#[test]
fn a_toast_is_raised_on_a_failed_delivery_only_when_ui_toasts_is_on() {
    for (toasts, expect_notify) in [(true, true), (false, false)] {
        let recorder = tone_recorder(&format!("toast-{toasts}"));
        let fake = crate::delivery::tests_support::FakeDeliverer::failing(
            crate::delivery::DeliveryError::Rejected("pane_not_found".into()),
        );
        let mut runtime = runtime_with(fake.clone(), false);
        runtime.delivery_settings.toasts = toasts;
        let request = dictate_request();
        answer(&request, &recorder, &runtime);
        answer(&request, &recorder, &runtime);
        let notified = fake.calls().iter().any(|c| matches!(c, crate::delivery::tests_support::Call::Notify(..)));
        assert_eq!(notified, expect_notify, "toasts = {toasts}");
        if notified {
            assert!(matches!(
                fake.calls().last(),
                Some(crate::delivery::tests_support::Call::Notify(title, body))
                    if title == "Delivery failed" && body == "w1:p2: the text is in the plugin log"
            ));
        }
    }
}
```

- [ ] Run `cargo test --lib daemon::` and record the actual result rather
      than assuming — step group B's `transcribe` already writes both
      journal lines in order and already gates `notify` on
      `runtime.delivery_settings.toasts`, so both tests are expected to pass
      as soon as they compile. If either fails, the gap is in `transcribe`
      from step group B: fix the order of the two `journal.write` calls
      relative to `delivery::deliver`, or the `if
      runtime.delivery_settings.toasts { ... }` guard, until both pass.
- [ ] Commit: `git add src/daemon.rs && git commit -m "daemon: add RecordingJournal-backed tests proving the journal order and the toast gate"`

#### Step group D — whole-crate checks

- [ ] `cargo test` — passes.
- [ ] `cargo clippy --all-targets -- -D warnings` — clean.
- [ ] `cargo fmt --check` — clean (if not, `cargo fmt` and fold the
      formatting into whichever commit above is still open, or its own
      `cargo fmt` commit).
- [ ] `python3 scripts/check_manifest.py` — clean (regression check; this
      task adds no new command name).

---

### Task 8: S4 gate — review the diff before a pull request exists

- [ ] Diff task 1–7's changes (`src/config.rs`, `src/capture.rs`,
      `src/delivery.rs`, `src/daemon.rs`, `src/main.rs`) against `main` and
      run `superpowers:requesting-code-review` against it.
- [ ] Address each finding (fix and re-run the affected `cargo test --lib
      <module>::`, or record why not, per
      `superpowers:receiving-code-review`); commit any fixes separately,
      each naming what was fixed.
- [ ] Append the S4 block to `tasks/22/RUN_22.md`, same shape as the S1/S2
      blocks already there.
- [ ] Commit: `git add tasks/22/RUN_22.md && git commit -m "run: record the S4 code-review gate for issue 22"`

---

### Task 9: S5 — run it, and write what happened into `docs/evidence.md`

**Files:** Modify `docs/evidence.md` (new section, same form as "Delivering
into a pane that is gone", `docs/evidence.md:453-467`); `tasks/22/RUN_22.md`
(closing S5 block).

**What a person must do, and what the result may claim.** Every test in
tasks 1–8 proves the pipeline from a `Take` onward against fakes — precisely
so it needs no microphone — which also means nothing run so far exercises
real audio capture, a real `herdr` binary, or a real pane. This plan can
honestly support "every reachable branch of `deliver()` is proven against a
fake, with no live herdr required" (AC-14); it cannot support "a spoken take
was delivered end to end," and no step here may claim that.

- [ ] `cargo build --release && herdr plugin link .`
- [ ] A person, at a working microphone, against a real herdr pane running
      an agent, with `[delivery] submit = false` (default): press the
      dictation key, speak one sentence, press it again. Confirm the text
      appears in the pane's input, unsent, and the client printed `delivered
      to <pane> [<level> dB]`, not the transcript.
- [ ] Same pane, with `[delivery] submit = true`: repeat, confirm the agent
      received and started responding to the text (not merely holding it).
- [ ] Against a pane id that no longer exists: dictate against it (via the
      plugin binding if it allows targeting a stale id directly, otherwise
      by invoking the client with a `dictate` context naming a closed
      pane's id). Confirm the client's reply carries the transcript and a
      reason, `herdr plugin log list --plugin haurylau.voice` shows
      `delivering: ...` before `delivery failed: ...`, and (with `[ui]
      toasts` at its default) a toast titled "Delivery failed" appeared.
- [ ] Write a new dated section into `docs/evidence.md`, in the form of
      `docs/evidence.md:453-467`: machine, herdr version (`herdr
      --version`), platform, exact commands/keybindings, and the exact
      observation for each of the three runs above. State plainly which
      were actually performed — do not imply one happened if it did not.
- [ ] Append the S5 block to `tasks/22/RUN_22.md`, naming the new
      `docs/evidence.md` section and which of the three runs completed.
- [ ] Commit: `git add docs/evidence.md tasks/22/RUN_22.md && git commit -m "evidence: verify delivery by hand against a real pane, and close run 22"`

---

## Self-review

**Spec coverage** — every AC maps to a task: AC-1→1; AC-2→3 (the input), 7B;
AC-3, AC-4→7B; AC-5→2 (`Take.target` unchanged, `Take.agent` added the same
way); AC-6→4 (no existence check), 7B; AC-7, AC-9→6, 7C; AC-8→7C; AC-10→7B;
AC-11, AC-12→7B (exact strings above); AC-13→5 (`Result`, no `unwrap` on a
fallible path), 7; AC-14→4.

**Placeholder scan** — no "TBD"/"add appropriate handling"; every step that
changes behavior carries the code to write and the command to run.

**Type consistency** — `Recorder::start(&self, target: &str, agent:
Option<&str>)` (task 2) is the shape every later call site uses; `Runtime`
names `recognition`/`deliverer`/`delivery_settings`/`journal` the same way
everywhere it is built; `delivery::deliver`'s parameter order (`deliverer,
submit, agent, pane, text`) matches its one call site in `transcribe` and its
four direct tests (task 4).

**Kept long on purpose, not by oversight:** task 7's five delivery-outcome
tests (step group B) are written in full rather than described, because they
are the ones the AC-2/AC-3/AC-4/AC-6/AC-10/AC-11/AC-12 verdict rests on — a
description would push the exact assertions (the reply substrings, the call
log shape) onto the implementer to invent, which is exactly what "no
placeholders" forbids. The `FakeDeliverer`/`RecordingJournal` ownership
pattern (clone behind an inner `Arc`, keep one handle, move the other into
`Runtime`) is written out once, in task 4, and reused by reference in task 7
rather than repeated.

**One deviation from `superpowers:writing-plans`'s default, stated up front:**
saved to `tasks/22/PLAN_22.md`, not
`docs/superpowers/plans/YYYY-MM-DD-<feature-name>.md` — this repository's own
`CLAUDE.md` fixes the run root as `tasks/<issue>/` and names `PLAN_<issue>.md`
as S3's artifact, which is the path this run was given; the skill's own note
("User preferences for plan location override this default") covers it.
