# Indicator Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** While a take runs, the tab bar, the sidebar's tab row and the sidebar's agent row say what it is doing; when it ends any way at all, including the daemon being killed, nothing decorated is left behind.

**Architecture:** The take path publishes one value, `Activity`. A second thread reads it on a tick, writes a pane token that expires by itself, and renames the tab only when the text would differ. A sweep at daemon start cuts the plugin's own suffix off any tab that still carries it.

**Tech Stack:** Rust, `std` only. No new dependency. The outward calls are `herdr pane report-metadata`, `herdr tab rename` and `herdr tab list`.

**Spec:** `tasks/40/DESIGN_40.md`, built against `tasks/40/AC_40.md`.

## Global Constraints

- Line numbers here were taken against `main` at `914471a` and drift as tasks land. Every citation also names the function or test it points at; when the two disagree, the name is right.
- Everything in the repository is English: code, comments, output strings, commits.
- No employer, client, internal-system or personal name, and no absolute home path, in anything written. Cite paths relative to the repository root.
- The daemon has no panic paths. No `unwrap`, `expect` or indexing that can fail on a path reachable while serving a request or inside either thread.
- Every user-visible failure names what to do next. A silent failure is a defect of the same weight as a wrong transcript.
- Every configuration key has a default; an absent configuration file is a valid state.
- Nothing in the suite may need herdr, a microphone, a model or a network.
- Four gates before any commit: `cargo test`, `cargo clippy --all-targets -- -D warnings`, `cargo fmt --check`, `python3 scripts/check_manifest.py`. Never commit red.
- Grep every file written for `</new_string>`, `<new_string>`, `</old_string>`, `<old_string>` and for merge-conflict markers at line start, before committing it.
- Commits end with `Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>`.

## The three states, exactly

Settled by the owner. These strings are the contract; nothing invents a variant.

| state | steady form | blink form |
|---|---|---|
| recording | `🎙️🔴 REC 0:05` | `🎙️ REC 0:05` |
| transcribing | `🎙️📝 TRANSCR` | `🎙️ TRANSCR` |
| fixing | `🎙️🪄 FIX` | `🎙️ FIX` |

The blink form is the steady form with the second glyph removed, so the text
shifts by one glyph and back. Both forms begin with `🎙️`, which is what the
sweep cuts from. The clock is `m:ss`, minutes uncapped.

## Plan decisions

**Three surfaces, two mechanisms, and the values are data.** Argument lists are
built by functions returning `Vec<&str>`, the way `insert_args` already is
(`src/delivery.rs:212`), so a test asserts what a real subcommand would receive
rather than trusting a fake.

**The drawing thread owns everything about drawing.** `Activity` is written by
the take path and read by the thread; what was decorated, what the label was
before, what was last written, and whether drawing has failed for this take are
the thread's own locals. That is what keeps `herdr` off the keypress path.

**Task order is the dependency order.** Values and argument lists have no
dependencies and come first, because everything else asserts against them.

## File structure

- **Create `src/indicator.rs`** — the states, both forms, the elapsed format, the
  suffix decorate and strip, the argument lists, the `Painter` trait with its
  real and recording implementations, and the drawing loop. One responsibility:
  saying what is happening. It knows nothing about audio, recognition or takes.
- **Modify `src/config.rs`** — the three `[ui]` keys.
- **Modify `src/daemon.rs`** — `Activity` on `Runtime`, the write sites, the
  thread's spawn and join in `serve`, and `tab` carried on `Hold`.
- **Modify `src/ptt.rs`** — `Hold` gains `tab`.
- **Modify `src/capture.rs`** — `Take` gains `tab`.
- **Modify `docs/design.md`, `docs/decisions.md`, `docs/evidence.md`.**

---

### Task 1: The values

**Files:**
- Create: `src/indicator.rs`
- Modify: the module list in `src/main.rs`

**Interfaces:**
- Consumes: nothing.
- Produces, all `pub` in `crate::indicator`:
  - `enum State { Recording { elapsed_ms: u64 }, Transcribing, Fixing }`
  - `fn value(state: &State, blink: bool) -> String`
  - `fn elapsed(ms: u64) -> String`
  - `const MARKER: &str = "🎙️";`
  - `fn decorate(label: &str, value: &str) -> String`
  - `fn strip(label: &str) -> String`

- [ ] **Step 1: Write the failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_three_states_read_as_the_owner_settled_them() {
        assert_eq!(value(&State::Recording { elapsed_ms: 5_000 }, false), "🎙️🔴 REC 0:05");
        assert_eq!(value(&State::Transcribing, false), "🎙️📝 TRANSCR");
        assert_eq!(value(&State::Fixing, false), "🎙️🪄 FIX");
    }

    #[test]
    fn the_blink_form_drops_the_second_glyph_and_nothing_else() {
        assert_eq!(value(&State::Recording { elapsed_ms: 5_000 }, true), "🎙️ REC 0:05");
        assert_eq!(value(&State::Transcribing, true), "🎙️ TRANSCR");
        assert_eq!(value(&State::Fixing, true), "🎙️ FIX");
    }

    #[test]
    fn both_forms_begin_with_the_marker_the_sweep_cuts_from() {
        for blink in [false, true] {
            for state in [State::Recording { elapsed_ms: 0 }, State::Transcribing, State::Fixing] {
                assert!(
                    value(&state, blink).starts_with(MARKER),
                    "the sweep finds nothing without it: {:?} blink={blink}",
                    state
                );
            }
        }
    }

    #[test]
    fn the_clock_is_minutes_and_seconds_and_does_not_wrap() {
        assert_eq!(elapsed(0), "0:00");
        assert_eq!(elapsed(5_000), "0:05");
        assert_eq!(elapsed(65_000), "1:05");
        assert_eq!(elapsed(600_000), "10:00");
        // A hold nobody ended must not read as though it had just begun.
        assert_eq!(elapsed(3_600_000), "60:00");
    }

    #[test]
    fn decorating_and_stripping_are_inverses() {
        for original in ["1", "", "review", "a name with spaces", "1 🎙 not ours"] {
            let decorated = decorate(original, "🎙️🔴 REC 0:05");
            assert_eq!(strip(&decorated), original, "round trip failed for {original:?}");
        }
    }

    #[test]
    fn stripping_a_label_nobody_decorated_leaves_it_alone() {
        assert_eq!(strip("1"), "1");
        assert_eq!(strip(""), "");
        assert_eq!(strip("[thing] name"), "[thing] name");
    }

    #[test]
    fn an_empty_label_decorates_without_a_leading_space() {
        assert_eq!(decorate("", "🎙️🪄 FIX"), "🎙️🪄 FIX");
        assert_eq!(strip("🎙️🪄 FIX"), "");
    }
}
```

- [ ] **Step 2: Run them and watch them fail**

Run: `cargo test indicator::tests`
Expected: FAIL — module `indicator` does not exist.

- [ ] **Step 3: Write it**

```rust
//! What the indicator says.
//!
//! Three states, two forms each: the steady form and the blink form, which is
//! the steady form without its second glyph. The token alternates between them
//! on every tick — that is the blink — and the tab label always carries the
//! steady form, because a tab bar that flashes a character twice a second is
//! noise where nobody chose to look.
//!
//! Every form begins with `MARKER`, which is what the start-up sweep cuts from
//! when a previous daemon was killed with a tab still decorated.

/// The glyph every value begins with, and the one the sweep looks for. Not
/// something a person types into a tab name by accident — which matters,
/// because the sweep renames every tab whose label contains it.
pub const MARKER: &str = "🎙️";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum State {
    Recording { elapsed_ms: u64 },
    Transcribing,
    Fixing,
}

/// The value for a state, in the steady form or the blink form.
pub fn value(state: &State, blink: bool) -> String {
    let (icon, text) = match state {
        State::Recording { elapsed_ms } => ("🔴", format!("REC {}", elapsed(*elapsed_ms))),
        State::Transcribing => ("📝", "TRANSCR".to_string()),
        State::Fixing => ("🪄", "FIX".to_string()),
    };
    if blink {
        format!("{MARKER} {text}")
    } else {
        format!("{MARKER}{icon} {text}")
    }
}

/// Minutes and seconds, minutes uncapped: a take that has run for an hour says
/// so rather than reading as though it had just begun.
pub fn elapsed(ms: u64) -> String {
    let seconds = ms / 1_000;
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

/// The label with the value appended. An empty label takes no leading space, so
/// that stripping it again gives the empty label back rather than a space.
pub fn decorate(label: &str, value: &str) -> String {
    if label.is_empty() {
        value.to_string()
    } else {
        format!("{label} {value}")
    }
}

/// The label with our decoration cut off, or unchanged if it carries none.
///
/// Cuts from the first `MARKER` and takes the space before it with it. A label
/// nobody decorated is returned as it is — including one that happens to
/// contain a lone microphone that is not `MARKER`.
pub fn strip(label: &str) -> String {
    match label.find(MARKER) {
        None => label.to_string(),
        Some(at) => label[..at].trim_end().to_string(),
    }
}
```

- [ ] **Step 4: Run the tests**

Run: `cargo test indicator::tests`
Expected: PASS, seven tests.

- [ ] **Step 5: Declare the module, satisfy clippy, and commit**

Add `mod indicator;` beside the other module declarations in `src/main.rs`.
Nothing outside this module uses it yet, so `cargo clippy --all-targets -- -D warnings`
will report dead code. Add the allowance with the comment that says when it goes,
exactly as `src/ptt.rs` did in issue #17:

```rust
// Nothing outside this module's own tests calls any of it yet: the painter
// arrives in Task 2 and the drawing loop in Task 5, which removes this line.
#![allow(dead_code)]
```

```bash
cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt --check && python3 scripts/check_manifest.py
git add src/indicator.rs src/main.rs
git commit -m "The indicator's three states, both forms, and a decoration that strips back off"
```

---

### Task 2: The painter

**Files:**
- Modify: `src/indicator.rs`

**Interfaces:**
- Consumes: `MARKER` from Task 1.
- Produces: `trait Painter`, `struct HerdrPainter`, `tests_support::RecordingPainter`, and the three argument-list functions.

- [ ] **Step 1: Write the failing tests**

```rust
    #[test]
    fn the_token_arguments_are_what_herdr_expects() {
        assert_eq!(
            token_args("w1:p1", "🎙️🔴 REC 0:05", 1800),
            vec![
                "pane", "report-metadata", "w1:p1",
                "--source", "haurylau.voice",
                "--token", "voice=🎙️🔴 REC 0:05",
                "--ttl-ms", "1800",
            ]
        );
    }

    #[test]
    fn the_rename_and_list_arguments_are_what_herdr_expects() {
        assert_eq!(rename_args("w1:t1", "1 🎙️🪄 FIX"), vec!["tab", "rename", "w1:t1", "1 🎙️🪄 FIX"]);
        assert_eq!(list_args(), vec!["tab", "list"]);
    }

    #[test]
    fn a_recording_painter_records_in_order_and_answers_what_it_was_told_to() {
        let painter = tests_support::RecordingPainter::ok();
        painter.token("w1:p1", "🎙️🔴 REC 0:00", 1800).unwrap();
        painter.rename("w1:t1", "1 🎙️🔴 REC 0:00").unwrap();
        assert_eq!(
            painter.calls(),
            vec![
                tests_support::Paint::Token("w1:p1".into(), "🎙️🔴 REC 0:00".into(), 1800),
                tests_support::Paint::Rename("w1:t1".into(), "1 🎙️🔴 REC 0:00".into()),
            ]
        );
    }
```

- [ ] **Step 2: Run them, watch them fail, write it**

The shape follows `src/delivery.rs` exactly: argument lists as data
(`insert_args`, `src/delivery.rs:212`), a trait, a real implementation that runs
the herdr binary resolved by `crate::delivery::herdr_binary()`, and a cloneable
recording fake sharing its log through an inner `Arc`, like `FakeDeliverer`
(`src/delivery.rs:76`).

```rust
pub trait Painter: Send + Sync {
    /// Set the pane's token, to expire after `ttl_ms` unless renewed.
    fn token(&self, pane: &str, value: &str, ttl_ms: u64) -> Result<(), PaintError>;
    /// Rename a tab.
    fn rename(&self, tab: &str, label: &str) -> Result<(), PaintError>;
    /// Every tab herdr knows about, as (tab_id, label).
    fn tabs(&self) -> Result<Vec<(String, String)>, PaintError>;
}
```

`PaintError` mirrors `DeliveryError`: a rejection carrying what herdr said, and
a not-found carrying the binary and the `PATH`, with the same wording about
`HERDR_BIN_PATH` (`src/delivery.rs:19-24`).

`tabs()` parses the JSON `herdr tab list` returns — `result.tabs[]`, each with
`tab_id` and `label` — with `serde_json`, which is already a dependency. A tab
whose label is absent counts as the empty label.

**`RecordingPainter` in full**, because Tasks 5 and 6 lean on all of it. It is
`Clone` and shares everything through one inner `Arc<Mutex<Inner>>`, the way
`FakeDeliverer` shares its log (`src/delivery.rs:76-86`), so a test keeps a
clone while another is moved into the drawing thread.

```rust
#[cfg(test)]
pub mod tests_support {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum Paint {
        Token(String, String, u64),
        Rename(String, String),
        Tabs,
    }

    #[derive(Default)]
    struct Inner {
        calls: Vec<Paint>,
        /// What `tabs()` answers, and what `rename` writes into.
        labels: Vec<(String, String)>,
        /// Every paint fails while this is set.
        failing: bool,
        /// Only `tabs()` fails while this is set.
        tabs_failing: bool,
    }

    /// Records every call in order and answers what it was told to.
    #[derive(Clone, Default)]
    pub struct RecordingPainter(Arc<Mutex<Inner>>);

    impl RecordingPainter {
        /// Everything succeeds; `tabs()` answers with nothing.
        pub fn ok() -> Self { Self::default() }

        /// Everything succeeds; `tabs()` answers with these.
        pub fn with_tabs(tabs: &[(&str, &str)]) -> Self { /* fill `labels` */ }

        /// Every paint fails from the start.
        pub fn failing() -> Self { /* set `failing` */ }

        /// From now on every paint fails.
        pub fn start_failing(&self) { /* set `failing` */ }
        /// From now on they succeed again.
        pub fn stop_failing(&self) { /* clear `failing` and `tabs_failing` */ }
        /// From now on only `tabs()` fails.
        pub fn fail_tabs(&self) { /* set `tabs_failing` */ }

        /// Change a label behind the painter's back, as a person renaming a tab
        /// would. Adds no call to the log.
        pub fn set_label(&self, tab: &str, label: &str) { /* update `labels` */ }

        pub fn calls(&self) -> Vec<Paint> { /* clone the log */ }
        pub fn total_calls(&self) -> usize { /* the log's length */ }
        /// Every rename, as (tab, label), in order.
        pub fn renames(&self) -> Vec<(String, String)> { /* filter the log */ }
        /// Every token value, in order.
        pub fn tokens(&self) -> Vec<String> { /* filter the log */ }
        /// How many times `tabs()` was called.
        pub fn tab_listings(&self) -> usize { /* count Paint::Tabs */ }
    }

    impl Painter for RecordingPainter {
        fn token(&self, pane: &str, value: &str, ttl_ms: u64) -> Result<(), PaintError> {
            // Record first, then answer: a test asserting on a failed call's
            // arguments needs the call to be in the log.
        }
        fn rename(&self, tab: &str, label: &str) -> Result<(), PaintError> {
            // Records, updates `labels`, then answers.
        }
        fn tabs(&self) -> Result<Vec<(String, String)>, PaintError> {
            // Records `Paint::Tabs`, then answers `labels` or fails.
        }
    }
}
```

Each body is three or four lines over the same mutex; they are elided here
because the shape is what matters and repeating `self.0.lock()` twelve times
would obscure it. Write them out; none of them is a decision.

`total_calls()` is what `Drawing::tick` waits on, so every one of the three
methods must record **before** it can fail, or a test driving a failing painter
waits forever.

- [ ] **Step 3: Gates and commit**

```bash
git add src/indicator.rs
git commit -m "A painter: three herdr calls, their argument lists as data, and a fake that records them"
```

---

### Task 3: The configuration keys

**Files:**
- Modify: `src/config.rs` — the `Ui` struct, which today holds `toasts` alone

**Interfaces:**
- Produces: `Ui { toasts, sidebar_token, tab_indicator, blink_ms }`, defaulting to `true, true, true, 600`.

- [ ] **Step 1: Write the failing tests**

```rust
    #[test]
    fn the_indicator_keys_have_defaults_and_are_read() {
        let directory = scratch("indicator");
        std::fs::write(
            directory.join("config.toml"),
            "[ui]\nblink_ms = 250\ntab_indicator = false\n",
        )
        .unwrap();
        let loaded = load(Some(&directory));
        assert_eq!(loaded.config.ui.blink_ms, 250);
        assert!(!loaded.config.ui.tab_indicator);
        // The keys the file did not name keep their defaults.
        assert!(loaded.config.ui.sidebar_token);
        assert!(loaded.config.ui.toasts);
    }

    #[test]
    fn the_indicator_defaults_are_the_documented_ones() {
        let ui = Ui::default();
        assert!(ui.sidebar_token);
        assert!(ui.tab_indicator);
        assert_eq!(ui.blink_ms, 600);
    }
```

- [ ] **Step 2: Run, fail, add the fields, run again, commit**

```rust
pub struct Ui {
    pub toasts: bool,
    /// The token on the pane's row in the sidebar.
    pub sidebar_token: bool,
    /// The suffix on the tab's label, which paints the tab bar and the
    /// sidebar's tab row at once.
    pub tab_indicator: bool,
    /// How often the token is rewritten, which is also how fast it blinks. The
    /// token's time to live is three times this and is not configurable, so the
    /// two cannot be set into a combination that makes the token lapse between
    /// renewals.
    pub blink_ms: u64,
}
```

```bash
git add src/config.rs
git commit -m "[ui] sidebar_token, tab_indicator and blink_ms, with the defaults the design names"
```

---

### Task 4: What the take path publishes

**Files:**
- Modify: `src/ptt.rs` — `Hold` gains `tab: Option<String>`
- Modify: `src/capture.rs` — `Take` gains `tab: Option<String>`, and `Running` with it
- Modify: `src/daemon.rs` — `Activity` on `Runtime`, and the write sites

**Interfaces:**
- Consumes: `indicator::State` from Task 1.
- Produces: `enum Activity { Idle, Recording { target, tab, since }, Working { target, tab, stage } }` and `enum Stage { Transcribing, Fixing }` in `src/daemon.rs`; `Runtime.activity: Mutex<Activity>`; `Runtime.ui: crate::config::Ui`; and the signature change below.

**`Runtime` carries the whole `[ui]` table, not three loose fields.** The
drawing thread reads `sidebar_token`, `tab_indicator` and `blink_ms`, and
`draw()` takes no configuration parameter — it has the `Arc<Runtime>` and reads
from it, the way every other resolved-once setting is read.

```rust
    /// `[ui]`, read once with the rest of the configuration. The drawing thread
    /// reads all three of its indicator keys from here.
    pub ui: crate::config::Ui,
```

Built in `start()` from `loaded.config.ui` — note it is currently moved from
there into `delivery_settings`, so clone it rather than move it — and added to
both test helpers, `fake_runtime_with_clock` and `runtime_with_clock`, with
`crate::config::Ui::default()`.

**`toasts` then exists in two places**, on `Runtime.ui` and on
`Runtime.delivery_settings`, which is where delivery reads it today. That
duplication is deliberate and bounded: delivery keeps reading what it already
reads, this task adds no change to it, and unifying the two is a tidy-up that
belongs to whoever touches `delivery::Settings` next. Say so in a comment where
the field is added, so the next reader does not take it for an accident.

**The signature change, and every call site it touches.** `tab` goes last, after
`agent`, because that is the order the invocation context is read in and it
keeps the existing arguments where every reader expects them:

```rust
// src/capture.rs — was: start(&self, target, cwd, agent)
pub fn start(&self, target: &str, cwd: Option<&str>, agent: Option<&str>, tab: Option<&str>) -> Started
```

`Command::Start` gains `tab: Option<String>` beside `agent`, `Running` gains
`tab: Option<String>`, and `Take` gains `pub tab: Option<String>` filled from it
in `stop_one`.

`dictate` and `ptt` in `src/daemon.rs` each gain a `tab: Option<&str>`
parameter, passed through from `answer`, which already has the parsed
`invocation` in hand and reads `invocation.tab_id.as_deref()` the same way it
reads `focused_pane_cwd` and `focused_pane_agent` today.

Call sites to update, all of them:

- **Two in production:** `dictate` (`src/daemon.rs:230`) and `ptt`
  (`src/daemon.rs:356`), plus the two `answer` arms that call them.
- **Sixteen in `src/capture.rs`'s own test module**, which pass `None` for the
  new argument unless the test is about the tab — none is.
- **Every `daemon.rs` test that calls `dictate` or `ptt` directly**, if any;
  `grep -n "recorder.start(\|dictate(\|ptt(" src/` finds them.
- **One `Take` built as a struct literal rather than by the recorder**, in
  `daemon.rs`'s tests at roughly `src/daemon.rs:1515` — it takes `tab: None`.
  Struct literals do not show up in a search for call sites, which is why it is
  named here.

Run `cargo test` after the signature change and before anything else: the
compiler names every site, and the task is not finished until they all build.

**Why the tab has to be plumbed.** `tab_id` is parsed from the invocation
context (`src/context.rs:15`) and thrown away. `Hold` carries `target`, `cwd`,
`agent` and the stamps; `Take` carries `path`, `level_dbfs`, `target`, `agent`,
`cwd`. Neither has a tab, and `transcribe` — which publishes two of the three
states — has only the `Take`. The tab travels beside the pane, pinned at the
same moment and for the same reason.

- [ ] **Step 1: Write the failing tests**

```rust
    #[test]
    fn a_hold_publishes_recording_with_the_pane_and_the_tab_it_was_pinned_to() {
        let (runtime, _clock) = fake_runtime_with_clock("a transcript");
        let recorder = tone_recorder("activity-recording");
        answer(
            &request("ptt", br#"{"focused_pane_id":"w1:p1","tab_id":"w1:t1"}"#),
            &recorder,
            &runtime,
        );
        match &*runtime.activity.lock().unwrap() {
            Activity::Recording { target, tab, .. } => {
                assert_eq!(target, "w1:p1");
                assert_eq!(tab.as_deref(), Some("w1:t1"));
            }
            other => panic!("expected Recording, got {other:?}"),
        }
    }

    #[test]
    fn a_toggle_take_publishes_the_same_states_as_a_hold() {
        // dictate's first half publishes Recording; its second half runs the
        // same pipeline, so the stages come from the same code.
        let (runtime, _clock) = fake_runtime_with_clock("a transcript");
        let recorder = tone_recorder("activity-toggle");
        let request = request("dictate", br#"{"focused_pane_id":"w1:p2","tab_id":"w1:t2"}"#);
        answer(&request, &recorder, &runtime);
        assert!(matches!(&*runtime.activity.lock().unwrap(), Activity::Recording { .. }));
        answer(&request, &recorder, &runtime);
        assert!(
            matches!(&*runtime.activity.lock().unwrap(), Activity::Idle),
            "a finished take publishes Idle, or the token never lapses"
        );
    }

    #[test]
    fn a_take_that_fails_still_ends_at_idle() {
        // A recognition failure must not leave the indicator saying TRANSCR
        // forever: the token would keep being renewed by a thread that thinks
        // work is in progress.
    }
```

```rust
    #[test]
    fn a_take_that_fails_still_ends_at_idle() {
        // A recognition failure must not leave the indicator saying TRANSCR
        // forever: the token would go on being renewed by a thread that thinks
        // work is in progress, and the tab would stay decorated.
        let mut runtime = fake_runtime_with_clock("unused").0;
        runtime.recognition = Err("no engine".to_string());
        let runtime = std::sync::Arc::new(runtime);
        let recorder = tone_recorder("activity-failed");
        let request = request("dictate", br#"{"focused_pane_id":"w1:p3","tab_id":"w1:t3"}"#);
        answer(&request, &recorder, &runtime);
        answer(&request, &recorder, &runtime);
        assert!(
            matches!(&*runtime.activity.lock().unwrap(), Activity::Idle),
            "a take that failed is still a take that ended: {:?}",
            runtime.activity.lock().unwrap()
        );
    }
```

- [ ] **Step 2: Run, fail, write it**

The write sites are section 1's table: `ptt` and `dictate` on
`Started::Began` publish `Recording`; `end_take` and `dictate`'s second half
publish `Working { Transcribing }` before `take_bias`; `transcribe` publishes
`Working { Fixing }` before the rewrite and `Idle` after delivery returns — on
every return path, including the two that give up before delivery, which is what
the third test pins.

- [ ] **Step 3: Gates and commit**

```bash
git add src/ptt.rs src/capture.rs src/daemon.rs
git commit -m "The take path says what it is doing, and the tab travels with the pane"
```

---

### Task 5: The drawing loop

**Files:**
- Modify: `src/indicator.rs` — the loop and its tests
- Modify: `src/daemon.rs` — spawn and join it in `serve`, and a new
  `tests_support` module so the tests in `src/indicator.rs` can reach three
  helpers that are private to `daemon`'s own test module today

**Interfaces:**
- Consumes: everything from Tasks 1 to 4, the `Clock` seam from `src/ptt.rs:147`.
- Produces: `fn draw(painter: Arc<dyn Painter>, runtime: Arc<Runtime>, clock: Arc<dyn Clock>, stop: Arc<AtomicBool>)`.

**The rules it implements, from the design:**

- The token is written on every tick, alternating the steady and blink forms,
  with a time to live of three times `blink_ms`. It is never cleared.
- The tab is renamed only when the steady form differs from the one last
  written — once a second while recording, once on entering each other state.
- On the first tick of a take the thread reads the label with `tabs()` and keeps
  it. On the first tick at which `Activity` is `Idle` while it still holds a
  decoration, it restores: read again, and if the label equals what it last
  wrote, write the original back; if it differs, leave it alone and record that.
- Elapsed time is `runtime.clock.now()` minus `since` — the take's clock, never
  the drawing clock, which counts from its own origin and is used only for
  waiting.
- A failed paint is recorded once per take and disables drawing for that take.
  It does not disable the restore.

- [ ] **Step 1: Make the daemon's test helpers reachable from another module**

The harness below lives in `src/indicator.rs`'s test module and calls three
things that live in `src/daemon.rs`'s: `runtime_with_clocks`, `RecordingJournal`
and `TestJournal`. That module is a bare `#[cfg(test)] mod tests`
(`src/daemon.rs:1144`), so none of them is visible outside the file, and the
`Ui` and `Activity` plumbing of Task 4 does not change that.

This repository already has a pattern for exactly this, and it is not making a
test module public: every module that shares test helpers declares
`#[cfg(test)] pub mod tests_support` beside its own tests —
`src/ptt.rs:207`, `src/capture.rs:397`, `src/delivery.rs:64`, `src/stt.rs:223`.
Follow it:

```rust
#[cfg(test)]
pub mod tests_support {
    use super::*;

    /// Records every line written, in order.
    #[derive(Default)]
    pub struct RecordingJournal(pub std::sync::Mutex<Vec<String>>);
    impl Journal for RecordingJournal { /* moved unchanged from mod tests */ }

    /// Lets a Runtime own a Journal while a test keeps its own handle.
    pub struct TestJournal(pub std::sync::Arc<RecordingJournal>);
    impl Journal for TestJournal { /* moved unchanged from mod tests */ }

    /// The runtime the drawing tests need: `activity` at `Idle`, the given
    /// `[ui]`, and the take's clock as `runtime.clock`.
    pub fn runtime_with_clocks(
        take_clock: &std::sync::Arc<crate::ptt::tests_support::TestClock>,
        ui: crate::config::Ui,
    ) -> Runtime { /* as `runtime_with_clock`, with `ui` and `activity` set */ }
}
```

`RecordingJournal` and `TestJournal` **move** rather than being copied — they
are used by a dozen tests in `daemon`'s own module, which keep compiling by
adding `use super::tests_support::{RecordingJournal, TestJournal};` to that
module's imports. Two copies of a recording fake is how the two drift apart.

Run `cargo test` after the move and before writing anything new: it is a
mechanical change and the compiler names every site.

- [ ] **Step 2: Write the failing tests**

One harness, used by every test below and by Task 6's. It starts the thread the
way `serve` does and hands back everything a test needs to drive it.

Two things about it are load-bearing. `tick` waits for the painter's call count
to move rather than for a duration, so the suite neither sleeps nor goes flaky
on a loaded machine — the same shape `wait_for_calls` takes in `src/daemon.rs`.
And there are two clocks with different jobs: advancing `draw_clock` makes the
thread act, advancing `take_clock` makes time pass for the take. A test that
confuses them proves nothing.

`runtime_with_clocks(&take_clock, ui)` is a new helper beside the existing
`runtime_with_clock` in `src/daemon.rs`'s test module: the same `Runtime`, with
`activity` set to `Idle`, the given `Ui` in `runtime.ui`, and the take's clock
as `runtime.clock`. The interval comes back out of `runtime.ui.blink_ms`, so a
test that changes it does not have to restate it anywhere.

```rust
    struct Drawing {
        painter: tests_support::RecordingPainter,
        runtime: std::sync::Arc<crate::daemon::Runtime>,
        take_clock: std::sync::Arc<crate::ptt::tests_support::TestClock>,
        draw_clock: std::sync::Arc<crate::ptt::tests_support::TestClock>,
        stop: std::sync::Arc<AtomicBool>,
        thread: Option<std::thread::JoinHandle<()>>,
    }

    impl Drawing {
        /// Everything defaults; the journal is the runtime's own.
        fn start(painter: tests_support::RecordingPainter) -> Drawing {
            Drawing::build(painter, Ui::default(), None)
        }

        /// A given `[ui]`, for the two tests about the configuration keys.
        fn with_ui(painter: tests_support::RecordingPainter, ui: Ui) -> Drawing {
            Drawing::build(painter, ui, None)
        }

        /// A journal the test can read, for the tests about what is recorded.
        fn with_journal(
            painter: tests_support::RecordingPainter,
            journal: &std::sync::Arc<RecordingJournal>,
        ) -> Drawing {
            Drawing::build(painter, Ui::default(), Some(std::sync::Arc::clone(journal)))
        }

        fn build(
            painter: tests_support::RecordingPainter,
            ui: Ui,
            journal: Option<std::sync::Arc<RecordingJournal>>,
        ) -> Drawing {
            let take_clock = std::sync::Arc::new(crate::ptt::tests_support::TestClock::default());
            let draw_clock = std::sync::Arc::new(crate::ptt::tests_support::TestClock::default());
            let mut built = runtime_with_clocks(&take_clock, ui);
            if let Some(journal) = journal {
                built.journal = Box::new(TestJournal(journal));
            }
            let runtime = std::sync::Arc::new(built);
            let stop = std::sync::Arc::new(AtomicBool::new(false));
            let thread = {
                let painter: std::sync::Arc<dyn Painter> = std::sync::Arc::new(painter.clone());
                let runtime = std::sync::Arc::clone(&runtime);
                let clock: std::sync::Arc<dyn crate::ptt::Clock> =
                    std::sync::Arc::clone(&draw_clock) as _;
                let stop = std::sync::Arc::clone(&stop);
                std::thread::spawn(move || draw(painter, runtime, clock, stop))
            };
            Drawing { painter, runtime, take_clock, draw_clock, stop, thread: Some(thread) }
        }

        /// Advance the drawing clock by `times` intervals and wait until the
        /// thread has actually acted on each, so no test ever sleeps for a
        /// duration or races the thread it is driving.
        fn tick(&self, times: u64) {
            for _ in 0..times {
                let before = self.painter.total_calls();
                self.draw_clock.advance(self.runtime.ui.blink_ms);
                let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
                while self.painter.total_calls() == before && std::time::Instant::now() < deadline {
                    std::thread::yield_now();
                }
            }
        }

        fn set(&self, activity: crate::daemon::Activity) {
            *self.runtime.activity.lock().unwrap() = activity;
        }

        fn stop(mut self) {
            self.stop.store(true, Ordering::SeqCst);
            self.draw_clock.wake();
            if let Some(thread) = self.thread.take() {
                thread.join().expect("the drawing thread ended badly");
            }
        }
    }

    fn recording(target: &str, tab: &str, since: u64) -> crate::daemon::Activity {
        crate::daemon::Activity::Recording {
            target: target.to_string(),
            tab: Some(tab.to_string()),
            since,
        }
    }
```

```rust
    #[test]
    fn the_token_is_renewed_every_tick_and_alternates() {
        let d = Drawing::start(tests_support::RecordingPainter::ok());
        d.set(recording("w1:p1", "w1:t1", 0));
        d.tick(4);
        let values: Vec<String> = d
            .painter
            .calls()
            .into_iter()
            .filter_map(|c| match c {
                tests_support::Paint::Token(_, value, _) => Some(value),
                _ => None,
            })
            .collect();
        assert_eq!(values.len(), 4, "one token per tick: {values:?}");
        assert!(values[0].contains('🔴') != values[1].contains('🔴'), "it alternates: {values:?}");
        assert!(values[0].contains('🔴') == values[2].contains('🔴'), "and alternates back: {values:?}");
        for value in &values {
            assert!(value.starts_with(MARKER), "every form carries the marker: {value:?}");
        }
        d.stop();
    }

    #[test]
    fn the_token_carries_a_time_to_live_longer_than_the_interval() {
        let d = Drawing::start(tests_support::RecordingPainter::ok());
        d.set(recording("w1:p1", "w1:t1", 0));
        d.tick(1);
        match d.painter.calls().first() {
            Some(tests_support::Paint::Token(_, _, ttl)) => assert!(
                *ttl > Ui::default().blink_ms,
                "a token that lapses between renewals flickers: {ttl}"
            ),
            other => panic!("expected a token, got {other:?}"),
        }
        d.stop();
    }

    #[test]
    fn the_tab_is_renamed_once_a_second_while_recording_and_not_on_a_blink() {
        let d = Drawing::start(tests_support::RecordingPainter::with_tabs(&[("w1:t1", "1")]));
        d.set(recording("w1:p1", "w1:t1", 0));
        // Two ticks inside the same second: one rename, not two.
        d.tick(1);
        d.take_clock.advance(300);
        d.tick(1);
        assert_eq!(d.painter.renames().len(), 1, "the blink must not rename: {:?}", d.painter.renames());
        // Past the second boundary: a second rename, carrying the new clock.
        d.take_clock.advance(800);
        d.tick(1);
        let renames = d.painter.renames();
        assert_eq!(renames.len(), 2, "{renames:?}");
        assert!(renames[1].1.ends_with("REC 0:01"), "the clock moved: {:?}", renames[1].1);
        assert!(renames[1].1.starts_with("1 🎙️🔴"), "steady form, and a suffix: {:?}", renames[1].1);
        d.stop();
    }

    #[test]
    fn a_state_with_no_clock_renames_the_tab_once_and_then_leaves_it() {
        let d = Drawing::start(tests_support::RecordingPainter::with_tabs(&[("w1:t1", "1")]));
        d.set(crate::daemon::Activity::Working {
            target: "w1:p1".into(),
            tab: Some("w1:t1".into()),
            stage: crate::daemon::Stage::Transcribing,
        });
        d.tick(5);
        let renames = d.painter.renames();
        assert_eq!(renames.len(), 1, "nothing in TRANSCR changes: {renames:?}");
        assert_eq!(renames[0].1, "1 🎙️📝 TRANSCR");
        d.stop();
    }

    #[test]
    fn the_restore_puts_the_original_back() {
        let d = Drawing::start(tests_support::RecordingPainter::with_tabs(&[("w1:t1", "1")]));
        d.set(recording("w1:p1", "w1:t1", 0));
        d.tick(2);
        d.set(crate::daemon::Activity::Idle);
        d.tick(1);
        assert_eq!(
            d.painter.renames().last().map(|r| r.1.clone()),
            Some("1".to_string()),
            "the tab goes back to what it was: {:?}",
            d.painter.renames()
        );
        d.stop();
    }

    #[test]
    fn a_tab_renamed_by_somebody_else_during_a_take_is_left_alone() {
        let painter = tests_support::RecordingPainter::with_tabs(&[("w1:t1", "1")]);
        let d = Drawing::start(painter);
        d.set(recording("w1:p1", "w1:t1", 0));
        d.tick(2);
        // Somebody renames the tab themselves, to something that is not ours.
        d.painter.set_label("w1:t1", "mine now");
        d.set(crate::daemon::Activity::Idle);
        d.tick(1);
        assert!(
            !d.painter.renames().iter().any(|r| r.1 == "1"),
            "restoring over somebody's own rename is the defect: {:?}",
            d.painter.renames()
        );
        d.stop();
    }

    #[test]
    fn an_empty_original_label_is_restored_as_empty() {
        let d = Drawing::start(tests_support::RecordingPainter::with_tabs(&[("w1:t1", "")]));
        d.set(recording("w1:p1", "w1:t1", 0));
        d.tick(1);
        assert_eq!(d.painter.renames()[0].1, "🎙️🔴 REC 0:00", "no leading space");
        d.set(crate::daemon::Activity::Idle);
        d.tick(1);
        assert_eq!(
            d.painter.renames().last().map(|r| r.1.clone()),
            Some(String::new()),
            "empty is a value to restore, not a reason to skip: {:?}",
            d.painter.renames()
        );
        d.stop();
    }

    #[test]
    fn a_failed_paint_is_recorded_once_and_does_not_fail_the_take() {
        let journal = std::sync::Arc::new(RecordingJournal::default());
        let d = Drawing::with_journal(tests_support::RecordingPainter::failing(), &journal);
        d.set(recording("w1:p1", "w1:t1", 0));
        d.tick(5);
        let lines = journal.0.lock().unwrap();
        assert_eq!(
            lines.iter().filter(|l| l.contains("indicator")).count(),
            1,
            "once per take, not once per tick: {lines:?}"
        );
        drop(lines);
        d.stop();
    }

    #[test]
    fn a_failed_paint_does_not_disable_the_restore() {
        // The paints fail only after the tab has been decorated, so there is a
        // decoration to put back and a disabled painter to put it back with.
        let painter = tests_support::RecordingPainter::with_tabs(&[("w1:t1", "1")]);
        let d = Drawing::start(painter);
        d.set(recording("w1:p1", "w1:t1", 0));
        d.tick(1);
        d.painter.start_failing();
        d.tick(2);
        d.painter.stop_failing();
        d.set(crate::daemon::Activity::Idle);
        d.tick(1);
        assert_eq!(
            d.painter.renames().last().map(|r| r.1.clone()),
            Some("1".to_string()),
            "a broken painter must not leave a tab decorated forever: {:?}",
            d.painter.renames()
        );
        d.stop();
    }

    #[test]
    fn elapsed_comes_from_the_takes_clock_not_the_drawing_clock() {
        let d = Drawing::start(tests_support::RecordingPainter::ok());
        d.set(recording("w1:p1", "w1:t1", 0));
        // Five drawing ticks, no time passing for the take: the number must not move.
        d.tick(5);
        for call in d.painter.tokens() {
            assert!(call.contains("0:00"), "the drawing clock must not be the elapsed clock: {call:?}");
        }
        // Time passes for the take, one more tick: now it moves.
        d.take_clock.advance(7_000);
        d.tick(1);
        assert!(
            d.painter.tokens().last().unwrap().contains("0:07"),
            "and the take's clock must be: {:?}",
            d.painter.tokens().last()
        );
        d.stop();
    }

    #[test]
    fn neither_half_is_painted_when_its_configuration_key_is_off() {
        let d = Drawing::with_ui(
            tests_support::RecordingPainter::with_tabs(&[("w1:t1", "1")]),
            Ui { sidebar_token: false, tab_indicator: true, ..Ui::default() },
        );
        d.set(recording("w1:p1", "w1:t1", 0));
        d.tick(3);
        assert!(d.painter.tokens().is_empty(), "sidebar_token off: {:?}", d.painter.tokens());
        assert!(!d.painter.renames().is_empty(), "tab_indicator on");
        d.stop();

        let d = Drawing::with_ui(
            tests_support::RecordingPainter::with_tabs(&[("w1:t1", "1")]),
            Ui { sidebar_token: true, tab_indicator: false, ..Ui::default() },
        );
        d.set(recording("w1:p1", "w1:t1", 0));
        d.tick(3);
        assert!(!d.painter.tokens().is_empty(), "sidebar_token on");
        assert!(d.painter.renames().is_empty(), "tab_indicator off: {:?}", d.painter.renames());
        d.stop();
    }
```

`elapsed_comes_from_the_takes_clock_not_the_drawing_clock` is the one that
catches the mistake this design made and its own gate caught twice: advance only
the drawing clock and the token's number must not move; advance only the take's
and it must.

- [ ] **Step 3 to 5: run, fail, write, run, commit**

The tests reach the moved helpers through
`use crate::daemon::tests_support::{RecordingJournal, TestJournal, runtime_with_clocks};`
and the take's clock through `crate::ptt::tests_support::TestClock`, the same
way `daemon`'s tests already reach it.

```bash
git add src/indicator.rs src/daemon.rs
git commit -m "A second thread draws, on its own clock, and puts the tab back when the take is over"
```

---

### Task 6: The sweep

**Files:**
- Modify: `src/indicator.rs`

**Interfaces:**
- Consumes: `Painter::tabs`, `strip` from Task 1.
- Produces: the sweep, run once at daemon start and retried at most once more.

**The rules:** at start, list every tab and rename each whose label contains
`MARKER` to `strip(label)`. It runs whether or not `[ui] tab_indicator` is on,
because the key means this plugin does not decorate and cannot mean that
decorations it left earlier stay forever. On failure it is recorded once and
retried at most once more, at the first tick where `Activity` is `Idle`, the
thread holds no decoration, and some paint has succeeded since the daemon
started — never during a take, because it would strip a decoration just written.

- [ ] **Step 1: Write the failing tests**

```rust
    #[test]
    fn the_sweep_strips_what_a_previous_daemon_left() {
        let d = Drawing::start(tests_support::RecordingPainter::with_tabs(&[
            ("w1:t1", "1 🎙️🔴 REC 0:12"),
            ("w1:t2", "review 🎙️📝 TRANSCR"),
            ("w1:t3", "🎙️🪄 FIX"),
        ]));
        d.tick(1);
        let renames = d.painter.renames();
        assert_eq!(renames.len(), 3, "every decorated tab: {renames:?}");
        assert!(renames.contains(&("w1:t1".to_string(), "1".to_string())), "{renames:?}");
        assert!(renames.contains(&("w1:t2".to_string(), "review".to_string())), "{renames:?}");
        assert!(renames.contains(&("w1:t3".to_string(), String::new())), "{renames:?}");
        d.stop();
    }

    #[test]
    fn the_sweep_leaves_tabs_nobody_decorated_alone() {
        let d = Drawing::start(tests_support::RecordingPainter::with_tabs(&[
            ("w1:t1", "1"),
            ("w1:t2", "[thing] name"),
            ("w1:t3", ""),
        ]));
        d.tick(1);
        assert!(d.painter.renames().is_empty(), "nothing to strip: {:?}", d.painter.renames());
        d.stop();
    }

    #[test]
    fn the_sweep_runs_with_the_tab_indicator_off() {
        let d = Drawing::with_ui(
            tests_support::RecordingPainter::with_tabs(&[("w1:t1", "1 🎙️🔴 REC 0:12")]),
            Ui { tab_indicator: false, ..Ui::default() },
        );
        d.tick(1);
        assert_eq!(
            d.painter.renames(),
            vec![("w1:t1".to_string(), "1".to_string())],
            "switching the indicator off is when its leftovers go, not when they are frozen"
        );
        d.stop();
    }

    #[test]
    fn a_failed_sweep_is_retried_once_and_only_once() {
        let painter = tests_support::RecordingPainter::with_tabs(&[("w1:t1", "1 🎙️🔴 REC 0:12")]);
        painter.fail_tabs();
        let d = Drawing::start(painter);
        d.tick(1);
        assert_eq!(d.painter.tab_listings(), 1, "attempted at start");
        // No paint has succeeded yet, so no retry however many ticks pass.
        d.tick(5);
        assert_eq!(d.painter.tab_listings(), 1, "no retry without evidence herdr answers");
        // A take paints successfully, then ends: now the one retry may run.
        d.painter.stop_failing();
        d.set(recording("w1:p1", "w1:t1", 0));
        d.tick(2);
        d.set(crate::daemon::Activity::Idle);
        d.tick(3);
        assert_eq!(d.painter.tab_listings(), 2, "retried once");
        d.tick(10);
        assert_eq!(d.painter.tab_listings(), 2, "and only once, whatever happens after");
        d.stop();
    }

    #[test]
    fn the_retry_never_runs_during_a_take() {
        let painter = tests_support::RecordingPainter::with_tabs(&[("w1:t1", "1")]);
        painter.fail_tabs();
        let d = Drawing::start(painter);
        d.tick(1);
        d.painter.stop_failing();
        // A take is running and the thread holds a decoration: the retry must
        // wait, or it strips the decoration it just wrote.
        d.set(recording("w1:p1", "w1:t1", 0));
        d.tick(4);
        assert_eq!(
            d.painter.tab_listings(),
            1,
            "a sweep during a take strips its own fresh decoration"
        );
        d.stop();
    }
```

- [ ] **Step 2 to 4: run, fail, write, run, commit**

The dead-code allowance from Task 1 comes out here, once the loop and the sweep
have callers. Clippy must pass without it; if it names something as unused, some
piece of the plan was not written.

```bash
git add src/indicator.rs src/daemon.rs
git commit -m "A daemon that was killed leaves a decorated tab; the next one takes it off"
```

---

### Task 7: The documents

**Files:**
- Modify: `docs/design.md` sections 6 and 7
- Modify: `docs/decisions.md`

- [ ] **Step 1: Correct section 6** to say what is built: the three states with
  their strings, the blink as an alternating glyph rather than a disappearing
  token, the two mechanisms and the three surfaces, and the token's time to live
  replacing "cleared on every exit path" — with the reason, which is that there
  is then no exit path to miss.

- [ ] **Step 2: Correct section 7's sketch** so `[ui]` shows the keys that exist.

- [ ] **Step 3: One row in `docs/decisions.md`**, four-part form, for the token
  that expires instead of being cleared and for `--display-agent` being unused.

- [ ] **Step 4: Grep for editing debris, then commit.**

---

### Task 8: Verify it by hand

**This task writes no production code.** It is AC-10.

- [ ] **Step 1:** Build, link, and start the daemon from this checkout.
- [ ] **Step 2:** Hold the bound key and watch all three surfaces: the tab bar,
  the sidebar's tab row, the sidebar's agent row.
- [ ] **Step 3:** While the take is still recording, kill the daemon with
  `kill -9`. Watch the token disappear on its own, and the tab stay decorated.
- [ ] **Step 4:** Start the daemon again and watch the sweep take the decoration
  off.
- [ ] **Step 5:** Record it in `docs/evidence.md` with the platform: what each
  surface showed, how long the token took to lapse after the kill, and whether
  the label came back exactly as it was. Describe the take in English; do not
  reproduce what was said.
- [ ] **Step 6:** Four gates, then the pull request with `Closes #40`.

## Self-review

**Criteria coverage.** AC-1 Tasks 1, 4, 5. AC-2 Task 5 for the renewal, Task 8
for the kill. AC-3 Tasks 1, 4. AC-4 Tasks 5, 8. AC-5 Task 5. AC-5a Tasks 1, 5.
AC-6 Task 4 — no herdr call is added to `ptt` or `dictate`; the thread does all
of it. AC-7 Task 5. AC-8 Tasks 3, 5. AC-9 every task. AC-10 Task 8.

**Design coverage.** Section 1 → Task 4. Section 2 → Task 5. Section 3 → Tasks
2, 5. Section 4 → Task 5. Section 4a → Task 6. Section 5 → Tasks 1, 5. Section 6
→ Task 5. Section 7 → Task 3. Section 8 → every task. Section 8a → Task 4.
Section 10 → Task 1, which fixes the strings, and Task 5, which decides where
each goes.

**Type consistency.** `State` is the indicator's, `Stage` is the daemon's, and
Task 5 maps one to the other in exactly one place — the tick that reads
`Activity`. `MARKER` is defined once and used by `strip`, by the tests that
check both forms begin with it, and by the sweep.

**The one thing an implementer must not change.** Elapsed time comes from the
take's clock, not the drawing thread's. They count from different origins, and
subtracting one from the other is arithmetic on two different time bases — a
mistake this design made and its own gate caught twice.
