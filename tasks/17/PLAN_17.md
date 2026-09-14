# Push-to-talk Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Holding a bound key records; one second after the last keypress the recording ends and the transcript is inserted into the pane the key was held over, unsubmitted.

**Architecture:** The daemon keeps one `Hold` in memory — the pinned pane and two stamps. Every `ptt` request refreshes the last-poke stamp and answers at once. A watcher thread asks a pure decision function what to do and, when the deadline has passed, stops the recorder and runs the take through the path `dictate` already uses. No file is written; the clock is behind a seam.

**Tech Stack:** Rust, `std` only. No new dependency. `serde`/`toml` for the new configuration section, already present.

**Spec:** `tasks/17/DESIGN_17.md`, built against `tasks/17/AC_17.md`.

## Global Constraints

- **Line numbers in this plan drift as the tasks land.** They were taken against `main` at `b8a07a8`. Every citation also names the function or test it points at; when the two disagree, the name is right and the number is stale. `src/daemon.rs` grows by several hundred lines over Tasks 3 to 5.

- Everything in the repository is English: code, comments, output strings, commits.
- No employer, client, internal-system or personal name, and no absolute home path, in any file. Cite paths relative to the repository root.
- The daemon has no panic paths. No `unwrap`, `expect` or indexing that can fail on a path reachable while serving a request.
- Every user-visible failure names what to do next. A silent failure is a defect of the same weight as a wrong transcript.
- Device selection by name, never by index. (Untouched here, stated because the recorder is involved.)
- Every configuration key has a default; an absent configuration file is a valid state.
- Tests live next to the code. Nothing in the suite may need a microphone, a model, a network or a live herdr.
- Four gates must pass before any commit is pushed: `cargo test`, `cargo clippy --all-targets -- -D warnings`, `cargo fmt --check`, `python3 scripts/check_manifest.py`.
- Commits end with `Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>`.
- Before committing a file you wrote with an editing tool, grep it for `</new_string>`, `<new_string>`, `</old_string>`, `<old_string>` and for merge-conflict markers. The pre-commit hook now refuses both, but find them before it does.

## Plan decisions

Three things `DESIGN_17.md` section 13 left to this plan, decided here:

**One watcher thread, not one per hold.** A hold is a singleton — the recorder serves one take at a time — so a thread per hold buys nothing and adds a lifetime to manage. The watcher lives as long as the daemon.

**The decision is a pure function; the thread is thin.** `decide(now, hold, settings)` returns what to do and touches nothing. Almost every criterion about timing is then a table test with no thread, no clock and no sleeping, and the thread itself needs one test rather than a dozen. This is what makes the backwards-clock case of AC-7 and the gap cases of AC-2 cheap.

**`Hold` lives on `Runtime`.** `answer` already receives `&Runtime`, and `serve` already holds an `Arc<Runtime>` to hand the watcher. A structure beside it would have to be threaded through `answer`'s signature and through every test that builds a runtime. `Runtime` already carries interior mutability for the same kind of reason (`told: AtomicBool`, `src/daemon.rs:88`).

**The pipeline runs on the watcher thread.** It takes seconds, and during those seconds no hold can exist — the recorder is busy with the take that just ended, and a `ptt` arriving meanwhile meets `Started::AlreadyRunning` and is refused by the collision rule of Task 6. So a watcher blocked inside the pipeline cannot miss a hold it should have been timing.

## File structure

- **Create `src/ptt.rs`** — everything new: `Hold`, `Stamp`, `Settings`, `Decision`, `decide`, the `Clock` trait with its real and test implementations, and the watcher loop. One responsibility: deciding when a hold is over. It knows nothing about audio, recognition or delivery.
- **Modify `src/config.rs`** — the `[ptt]` section, and the stale test that must move.
- **Modify `src/daemon.rs`** — `Hold` on `Runtime`, the `ptt` arm of `answer`, the collisions, the watcher's spawn and join in `serve`, the journal lines.
- **Modify `src/main.rs`** — route `ptt` to the client instead of the stub.
- **Modify `src/client.rs`** — nothing but a test, confirming `ptt` keeps the short reply bound.
- **Modify `docs/design.md`, `docs/decisions.md`, `docs/evidence.md`** — the gap, the row, the measurement.

---

### Task 1: The `[ptt]` configuration section

**Files:**
- Modify: `src/config.rs:20-28` (the `Config` struct), and a new section struct beside `Ui`
- Modify: `src/config.rs:362-378` (the test that must move)

**Interfaces:**
- Consumes: nothing.
- Produces: `config::Ptt { release_ms: u64, min_hold_ms: u64 }` with `Default` giving `release_ms: 1000`, `min_hold_ms: 300`; reachable as `Config::ptt`.

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn the_ptt_table_has_defaults_and_is_read() {
    let directory = scratch("ptt");
    std::fs::write(
        directory.join("config.toml"),
        "[ptt]\nrelease_ms = 2500\n",
    )
    .unwrap();
    let loaded = load(Some(&directory));
    assert_eq!(loaded.config.ptt.release_ms, 2500);
    // The key the file did not name keeps its default.
    assert_eq!(loaded.config.ptt.min_hold_ms, 300);
}

#[test]
fn the_ptt_defaults_are_the_documented_ones() {
    let config = Config::default();
    assert_eq!(config.ptt.release_ms, 1000);
    assert_eq!(config.ptt.min_hold_ms, 300);
}
```

- [ ] **Step 2: Run them and watch them fail**

Run: `cargo test config::tests::the_ptt`
Expected: FAIL — no field `ptt` on `Config`.

- [ ] **Step 3: Add the section**

```rust
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct Ptt {
    /// How long after the last keypress a hold counts as released.
    ///
    /// One second, derived from the auto-repeat measurements in
    /// `docs/evidence.md`: an 85 ms median repeat interval and a largest
    /// observed start-of-hold gap of 99 ms, so this survives a stall an order
    /// of magnitude worse than anything measured. It is a default rather than a
    /// constant because the case it exists to survive — the worst gap inside a
    /// hold on a loaded machine — has not been measured (issue #56).
    pub release_ms: u64,
    /// Shorter than this is a tap, not a hold: the recording is discarded
    /// without transcription and the person is told it was a tap.
    pub min_hold_ms: u64,
}

impl Default for Ptt {
    fn default() -> Self {
        Ptt {
            release_ms: 1000,
            min_hold_ms: 300,
        }
    }
}
```

and add `pub ptt: Ptt,` to `Config`.

- [ ] **Step 4: Move the test that has just stopped testing anything**

`a_key_of_a_later_stage_is_ignored_rather_than_fatal` (`src/config.rs:362-378`) proves an unknown section does not make the file fail to load, and it uses `[ptt]` to do it. `[ptt]` is no longer unknown. Change its fixture to a section that is still in the future and say why in the test:

```rust
    #[test]
    fn a_key_of_a_later_stage_is_ignored_rather_than_fatal() {
        let directory = scratch("future");
        // The section has to be one no `Config` field claims, or this test
        // silently stops testing anything. `[ptt]` was that section until
        // issue #17 gave it fields; `[indicator]` is issue #40's, unbuilt.
        std::fs::write(
            directory.join("config.toml"),
            "[indicator]\nblink_ms = 600\n\n[stt]\nmodel = \"small\"\n",
        )
        .unwrap();
        let loaded = load(Some(&directory));
        assert_eq!(loaded.config.stt.model, "small");
        assert!(
            matches!(loaded.source, Source::File(_)),
            "got {:?}",
            loaded.source
        );
    }
```

- [ ] **Step 5: Run the tests and the gates**

Run: `cargo test config` then `cargo clippy --all-targets -- -D warnings` and `cargo fmt --check`
Expected: PASS, no warnings.

- [ ] **Step 6: Commit**

```bash
git add src/config.rs
git commit -m "[ptt] release_ms and min_hold_ms, with the defaults the design names"
```

---

### Task 2: The hold, the decision, and the clock seam

**Files:**
- Create: `src/ptt.rs`
- Modify: `src/lib.rs` or `src/main.rs` module list — whichever declares the modules (`grep -n "^mod \|^pub mod " src/*.rs` to find it)

**Interfaces:**
- Consumes: `config::Ptt` from Task 1.
- Produces, all `pub` in `crate::ptt`:
  - `type Stamp = u64` — monotonic milliseconds since the clock's origin.
  - `struct Hold { target: String, cwd: Option<String>, agent: Option<String>, began: Stamp, last_poke: Stamp, pokes: u32 }`
  - `struct Settings { release_ms: u64, min_hold_ms: u64 }`
  - `enum Decision { Idle, KeepWaiting { until: Stamp }, Release { held_ms: u64 }, TooShort { held_ms: u64 } }`
  - `fn decide(now: Stamp, hold: Option<&Hold>, settings: &Settings) -> Decision`
  - `trait Clock: Send + Sync { fn now(&self) -> Stamp; fn wait_until(&self, deadline: Stamp); fn wake(&self); }`
  - `struct SystemClock` implementing it, and `tests_support::TestClock`.

- [ ] **Step 1: Write the failing tests for `decide`**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn settings() -> Settings {
        Settings { release_ms: 1000, min_hold_ms: 300 }
    }

    fn hold_at(began: Stamp, last_poke: Stamp) -> Hold {
        Hold {
            target: "w1:p1".to_string(),
            cwd: None,
            agent: None,
            began,
            last_poke,
            pokes: 2,
        }
    }

    #[test]
    fn with_no_hold_there_is_nothing_to_decide() {
        assert_eq!(decide(500, None, &settings()), Decision::Idle);
    }

    #[test]
    fn a_gap_shorter_than_the_release_keeps_the_hold() {
        // Last poke at 1000, now 1600: 600 ms of silence, under the 1000 ms gap.
        let hold = hold_at(0, 1000);
        assert_eq!(
            decide(1600, Some(&hold), &settings()),
            Decision::KeepWaiting { until: 2000 }
        );
    }

    #[test]
    fn the_deadline_releases_the_hold() {
        let hold = hold_at(0, 1000);
        assert_eq!(
            decide(2000, Some(&hold), &settings()),
            Decision::Release { held_ms: 1000 }
        );
    }

    #[test]
    fn a_hold_shorter_than_the_minimum_is_a_tap() {
        // Held 120 ms, then released.
        let hold = hold_at(0, 120);
        assert_eq!(
            decide(1120, Some(&hold), &settings()),
            Decision::TooShort { held_ms: 120 }
        );
    }

    #[test]
    fn a_clock_that_went_backwards_never_discards_a_take() {
        // last_poke precedes began: impossible from a monotonic clock, and the
        // prototype discarded a live take on exactly this arithmetic.
        let hold = hold_at(5_000, 1_000);
        match decide(9_000, Some(&hold), &settings()) {
            Decision::Release { held_ms } => assert_eq!(held_ms, 0),
            other => panic!("an impossible duration must still release: {other:?}"),
        }
    }
}
```

- [ ] **Step 2: Run them and watch them fail**

Run: `cargo test ptt::tests`
Expected: FAIL — module `ptt` does not exist.

- [ ] **Step 3: Write the module**

```rust
//! When a held key counts as released.
//!
//! Holding a key starts the client about twelve times a second, and each start
//! reaches the daemon as a request. The hold lives here, in memory: there is no
//! file to write and therefore no truncated read to defend against, which is
//! what the shell prototype lost recordings to.
//!
//! The decision is a pure function so that a one-second gap costs nothing to
//! test. The thread that calls it does nothing but wait and act.

/// Monotonic milliseconds since the clock's origin.
pub type Stamp = u64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hold {
    /// The pane pinned when the hold began, kept until delivery.
    pub target: String,
    pub cwd: Option<String>,
    pub agent: Option<String>,
    pub began: Stamp,
    pub last_poke: Stamp,
    /// How many repeats arrived. For the journal only.
    pub pokes: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    pub release_ms: u64,
    pub min_hold_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    /// No hold. Wait until one begins.
    Idle,
    /// The key is still down as far as anything here can tell.
    KeepWaiting { until: Stamp },
    /// The gap passed: end the take and deliver it.
    Release { held_ms: u64 },
    /// The gap passed and the hold was too short to be one.
    TooShort { held_ms: u64 },
}

/// What to do about the hold, if any, at `now`.
///
/// `held_ms` is saturating on purpose. A monotonic clock cannot put `last_poke`
/// before `began`, so a negative duration is not reachable here — but the
/// prototype discarded 17 live takes on exactly that arithmetic, and a take is
/// never discarded for a duration this function could not compute. An
/// impossible span releases with `held_ms` of zero and is reported, not thrown
/// away.
pub fn decide(now: Stamp, hold: Option<&Hold>, settings: &Settings) -> Decision {
    let Some(hold) = hold else {
        return Decision::Idle;
    };
    let deadline = hold.last_poke.saturating_add(settings.release_ms);
    if now < deadline {
        return Decision::KeepWaiting { until: deadline };
    }
    let held_ms = hold.last_poke.saturating_sub(hold.began);
    if held_ms < settings.min_hold_ms {
        Decision::TooShort { held_ms }
    } else {
        Decision::Release { held_ms }
    }
}
```

- [ ] **Step 4: Run the tests**

Run: `cargo test ptt::tests`
Expected: PASS, five tests.

Note on the backwards-clock test: with `saturating_sub`, `held_ms` is 0, which is below `min_hold_ms`, so `decide` returns `TooShort` and the test fails. That is the point — it forces the guard to be written rather than assumed. Add it:

```rust
    let held_ms = hold.last_poke.saturating_sub(hold.began);
    // A span that could not be computed is not evidence of a tap. The clock is
    // monotonic, so this is unreachable in the daemon; it is written because
    // the prototype threw away live takes when it was not.
    if hold.last_poke < hold.began {
        return Decision::Release { held_ms: 0 };
    }
```

placed before the `min_hold_ms` comparison.

- [ ] **Step 5: Add the clock seam**

```rust
/// Time, and waiting for it, behind an interface.
///
/// The gap is a second and the minimum hold 300 ms. A suite that waited those
/// out would pay for them on every run, and a clock that cannot be moved
/// backwards cannot exercise the guard in `decide`.
///
/// **The contract, which both implementations must satisfy exactly.**
/// `wait_until` returns when `now()` has reached `deadline` **or** when `wake`
/// has been called since the last return, whichever happens first. A `wake` is
/// consumed by the return it causes, so two waits are not freed by one call.
/// A waiter that can only be freed by time is not enough: the watcher waits an
/// hour when there is no hold, and what ends that wait is a hold beginning.
pub trait Clock: Send + Sync {
    fn now(&self) -> Stamp;
    fn wait_until(&self, deadline: Stamp);
    fn wake(&self);
}

pub struct SystemClock {
    origin: std::time::Instant,
    woken: std::sync::Mutex<bool>,
    bell: std::sync::Condvar,
}

impl Default for SystemClock {
    fn default() -> Self {
        SystemClock {
            origin: std::time::Instant::now(),
            woken: std::sync::Mutex::new(false),
            bell: std::sync::Condvar::new(),
        }
    }
}

impl Clock for SystemClock {
    fn now(&self) -> Stamp {
        // `as u64` is safe for any run shorter than 584 million years.
        self.origin.elapsed().as_millis() as u64
    }

    fn wait_until(&self, deadline: Stamp) {
        let Ok(mut woken) = self.woken.lock() else {
            // A poisoned lock must not become a busy loop in the daemon.
            std::thread::sleep(std::time::Duration::from_millis(50));
            return;
        };
        loop {
            if *woken {
                *woken = false;
                return;
            }
            let remaining = deadline.saturating_sub(self.now());
            if remaining == 0 {
                return;
            }
            let waited = self
                .bell
                .wait_timeout(woken, std::time::Duration::from_millis(remaining));
            let Ok((guard, _)) = waited else { return };
            woken = guard;
        }
    }

    fn wake(&self) {
        if let Ok(mut woken) = self.woken.lock() {
            *woken = true;
        }
        self.bell.notify_all();
    }
}
```

- [ ] **Step 6: Add the test clock**

It has to satisfy the same contract, and the reason is concrete: the watcher's
idle wait is an hour long, nothing in any test advances an hour, and what frees
it is a hold beginning. A clock whose `wake` only notifies would leave that wait
unbroken — the waiter would re-check the same unmet deadline and sleep again —
and the test that stops the daemon would hang in `join` rather than fail.

```rust
#[cfg(test)]
pub mod tests_support {
    use super::*;
    use std::sync::{Condvar, Mutex};

    #[derive(Default)]
    struct State {
        now: Stamp,
        woken: bool,
    }

    /// A clock the test moves by hand. `wait_until` returns as soon as the
    /// test's `advance` puts `now` at or past the deadline, or as soon as
    /// `wake` is called — so a one-second gap costs nothing and an hour-long
    /// idle wait is not a hang.
    #[derive(Default)]
    pub struct TestClock {
        state: Mutex<State>,
        bell: Condvar,
    }

    impl TestClock {
        pub fn advance(&self, by: u64) {
            if let Ok(mut state) = self.state.lock() {
                state.now = state.now.saturating_add(by);
            }
            self.bell.notify_all();
        }
    }

    impl Clock for TestClock {
        fn now(&self) -> Stamp {
            self.state.lock().map(|state| state.now).unwrap_or(0)
        }

        fn wait_until(&self, deadline: Stamp) {
            let Ok(mut state) = self.state.lock() else { return };
            loop {
                if state.woken {
                    state.woken = false;
                    return;
                }
                if state.now >= deadline {
                    return;
                }
                let Ok(next) = self.bell.wait(state) else { return };
                state = next;
            }
        }

        fn wake(&self) {
            if let Ok(mut state) = self.state.lock() {
                state.woken = true;
            }
            self.bell.notify_all();
        }
    }
}
```

Then pin the contract itself, so that neither implementation can drift from it:

```rust
    #[test]
    fn a_wake_frees_a_waiter_the_deadline_never_would() {
        use std::sync::Arc;
        let clock = Arc::new(tests_support::TestClock::default());
        let waiter = {
            let clock = Arc::clone(&clock);
            // An hour away: only `wake` can end this.
            std::thread::spawn(move || clock.wait_until(3_600_000))
        };
        // Give the waiter time to be inside `wait_until`, then free it.
        std::thread::sleep(std::time::Duration::from_millis(20));
        clock.wake();
        waiter.join().expect("a wake must free a waiter the clock never will");
    }

    #[test]
    fn the_same_holds_for_the_real_clock() {
        use std::sync::Arc;
        let clock = Arc::new(SystemClock::default());
        let waiter = {
            let clock = Arc::clone(&clock);
            std::thread::spawn(move || clock.wait_until(3_600_000))
        };
        std::thread::sleep(std::time::Duration::from_millis(20));
        clock.wake();
        waiter.join().expect("the contract is the trait's, not one implementation's");
    }
```

- [ ] **Step 7: Declare the module and run the gates**

Add `mod ptt;` beside the other module declarations (they are in `src/main.rs`).

Clippy will fail here, and it is expected: nothing outside this module's own
tests uses any of it yet. The daemon takes `Hold`, `Settings` and `Clock` in
Task 3, and `decide` is called only by the watcher in Task 4, so
`cargo clippy --all-targets -- -D warnings` reports eight dead-code errors. A
commit must never be red, so add a module-level allowance at the top of
`src/ptt.rs`, with the comment that says when it goes:

```rust
// Nothing outside this module's own tests calls any of it yet: the daemon takes
// the hold, the settings and the clock in Task 3, and the watcher is what calls
// `decide`, in Task 4. Task 4 removes this line.
#![allow(dead_code)]
```

Task 4 Step 5 deletes it. An allowance nobody is told to remove is how dead code
starts being permitted by accident.

Run: `cargo test ptt` then `cargo clippy --all-targets -- -D warnings` and `cargo fmt --check`
Expected: PASS, no warnings.

- [ ] **Step 8: Commit**

```bash
git add src/ptt.rs src/main.rs
git commit -m "The hold, the decision that ends it, and a clock a test can move"
```

---

### Task 3: The `ptt` request path

**Files:**
- Modify: `src/daemon.rs:51-89` (`Runtime`), `:101-140` (`answer`), and a new `fn ptt` beside `fn dictate` (`:151`)

**Interfaces:**
- Consumes: `crate::ptt::{Hold, Settings, Stamp, Clock}` from Task 2; `config::Ptt` from Task 1.
- Produces: `Runtime.hold: std::sync::Mutex<Option<crate::ptt::Hold>>`, `Runtime.ptt: crate::ptt::Settings`, `Runtime.clock: std::sync::Arc<dyn crate::ptt::Clock>`; and `fn ptt(recorder: &Recorder, runtime: &Runtime, pane: &str, cwd: Option<&str>, agent: Option<&str>) -> Reply`.

**Why the refresh happens before the context is examined.** A `ptt` request carries herdr's invocation context, and `answer` refuses a context that will not parse, or that names no focused pane, before any handler runs (`src/daemon.rs:108-118`). That refusal is the one signal this mechanism can fail to read. If a refused repeat left the stamp untouched, a run of them would let the deadline expire and end the hold — the prototype's failure by another road. The context answers *where* a take goes, which is pinned once at the start; the arrival of the request answers *whether the key is down*, and that is true whatever the payload says.

- [ ] **Step 1: Write the failing tests**

All five use helpers that already exist in `src/daemon.rs`'s test module —
`request(command, context)` at `:710` (note: `context` is `&[u8]`, so the
literals are `br#"..."#`) and `tone_recorder(tag)` at `:720` — plus one new
helper, `fake_runtime_with_clock`, defined in Step 3.

```rust
    const PANE_1: &[u8] = br#"{"focused_pane_id":"w1:p1"}"#;

    #[test]
    fn a_first_ptt_begins_a_hold_and_names_the_pane() {
        let (runtime, _clock) = fake_runtime_with_clock("a transcript");
        let recorder = tone_recorder("ptt-first");
        let (reply, _) = answer(&request("ptt", PANE_1), &recorder, &runtime);
        assert_eq!(reply, Reply::Ok("holding for w1:p1".to_string()));
        let held = runtime.hold.lock().unwrap();
        let hold = held.as_ref().expect("a hold");
        assert_eq!(hold.target, "w1:p1");
        assert_eq!(hold.pokes, 1);
    }

    #[test]
    fn a_repeat_refreshes_the_stamp_and_does_not_start_a_second_take() {
        let (runtime, clock) = fake_runtime_with_clock("a transcript");
        let recorder = tone_recorder("ptt-repeat");
        answer(&request("ptt", PANE_1), &recorder, &runtime);
        let first = runtime.hold.lock().unwrap().as_ref().unwrap().last_poke;
        clock.advance(90);
        answer(&request("ptt", PANE_1), &recorder, &runtime);
        let held = runtime.hold.lock().unwrap();
        let hold = held.as_ref().expect("still one hold");
        assert!(hold.last_poke > first, "the repeat must move the stamp");
        assert_eq!(hold.pokes, 2);
        assert_eq!(hold.target, "w1:p1");
    }

    #[test]
    fn a_repeat_from_another_pane_does_not_move_the_target() {
        let (runtime, clock) = fake_runtime_with_clock("a transcript");
        let recorder = tone_recorder("ptt-other-pane");
        answer(&request("ptt", PANE_1), &recorder, &runtime);
        clock.advance(90);
        let (reply, _) = answer(
            &request("ptt", br#"{"focused_pane_id":"w1:p9"}"#),
            &recorder,
            &runtime,
        );
        assert_eq!(reply, Reply::Ok("holding for w1:p1".to_string()));
        assert_eq!(
            runtime.hold.lock().unwrap().as_ref().unwrap().target,
            "w1:p1"
        );
    }

    #[test]
    fn an_unreadable_repeat_refreshes_the_hold_and_is_still_refused() {
        let (runtime, clock) = fake_runtime_with_clock("a transcript");
        let recorder = tone_recorder("ptt-unreadable");
        answer(&request("ptt", PANE_1), &recorder, &runtime);
        let first = runtime.hold.lock().unwrap().as_ref().unwrap().last_poke;
        clock.advance(90);
        // Not JSON at all: `context::parse` refuses it.
        let (reply, _) = answer(&request("ptt", b"not json"), &recorder, &runtime);
        assert!(matches!(reply, Reply::Error(_)), "the keypress itself failed");
        let held = runtime.hold.lock().unwrap();
        let hold = held.as_ref().expect("the hold survives an unreadable repeat");
        assert!(
            hold.last_poke > first,
            "a signal the daemon could not read is not evidence the key came up"
        );
    }

    #[test]
    fn an_unreadable_request_with_no_hold_open_is_refused_as_before() {
        let (runtime, _clock) = fake_runtime_with_clock("a transcript");
        let recorder = tone_recorder("ptt-unreadable-idle");
        let (reply, _) = answer(&request("ptt", b"not json"), &recorder, &runtime);
        assert!(matches!(reply, Reply::Error(_)));
        assert!(
            runtime.hold.lock().unwrap().is_none(),
            "nothing to pin a hold to"
        );
    }
```

- [ ] **Step 2: Run them and watch them fail**

Run: `cargo test daemon::tests::a_first_ptt daemon::tests::a_repeat daemon::tests::an_unreadable`
Expected: FAIL — no field `hold` on `Runtime`, and `ptt` still answers `not implemented yet`.

- [ ] **Step 3: Widen `Runtime`**

```rust
    /// The hold in progress, if a key is down. In memory on purpose: a file
    /// would bring back the truncated read the prototype lost recordings to.
    pub hold: std::sync::Mutex<Option<crate::ptt::Hold>>,
    /// `[ptt]`, read once with the rest of the configuration.
    pub ptt: crate::ptt::Settings,
    /// Behind an `Arc` because the watcher thread holds it too.
    pub clock: std::sync::Arc<dyn crate::ptt::Clock>,
```

Four places build a `Runtime`, not three. Besides `start()`, `fake_runtime` and
`runtime_with`, one test builds a literal inline —
`the_failure_reply_survives_one_read_line_when_the_transcript_and_the_reason_carry_newlines`
at roughly `src/daemon.rs:1015` — and it stops compiling without the three new
fields. It never holds a key, so give it an empty hold, the same 1000/300
settings, and `SystemClock::default()`; it needs no handle on the clock.

In `start()` (`src/daemon.rs:551`), build them from the loaded configuration:

```rust
        hold: std::sync::Mutex::new(None),
        ptt: crate::ptt::Settings {
            release_ms: loaded.config.ptt.release_ms,
            min_hold_ms: loaded.config.ptt.min_hold_ms,
        },
        clock: std::sync::Arc::new(crate::ptt::SystemClock::default()),
```

`fake_runtime` (`src/daemon.rs:647`) and `runtime_with` (`:735`) gain the same
three fields. Neither changes shape, so no existing call site moves; instead each
grows a sibling that also hands back the concrete clock, and the old name
delegates to it and drops the clock. `Runtime` gets **no** extra field for the
test clock — the concrete `Arc` is kept by the test, not by the runtime:

```rust
    /// The same runtime `fake_runtime` builds, plus the clock it was built
    /// with. Tests that move time need the concrete type; `Runtime` only ever
    /// holds the trait object, so the concrete `Arc` is handed back here rather
    /// than stored and downcast.
    fn fake_runtime_with_clock(
        text: &str,
    ) -> (Runtime, std::sync::Arc<crate::ptt::tests_support::TestClock>) {
        let clock = std::sync::Arc::new(crate::ptt::tests_support::TestClock::default());
        let runtime = Runtime {
            // ... every field `fake_runtime` already sets, unchanged ...
            hold: std::sync::Mutex::new(None),
            ptt: crate::ptt::Settings {
                release_ms: 1000,
                min_hold_ms: 300,
            },
            clock: std::sync::Arc::clone(&clock) as std::sync::Arc<dyn crate::ptt::Clock>,
        };
        (runtime, clock)
    }

    fn fake_runtime(text: &str) -> Runtime {
        fake_runtime_with_clock(text).0
    }

    /// As `runtime_with`, plus the clock. `runtime_with` keeps its signature —
    /// `FakeDeliverer` unboxed, `submit: bool` — because the existing suite
    /// calls it a dozen times.
    fn runtime_with_clock(
        deliverer: crate::delivery::tests_support::FakeDeliverer,
        submit: bool,
    ) -> (Runtime, std::sync::Arc<crate::ptt::tests_support::TestClock>) {
        let clock = std::sync::Arc::new(crate::ptt::tests_support::TestClock::default());
        let runtime = Runtime {
            // ... every field `runtime_with` already sets, unchanged ...
            hold: std::sync::Mutex::new(None),
            ptt: crate::ptt::Settings {
                release_ms: 1000,
                min_hold_ms: 300,
            },
            clock: std::sync::Arc::clone(&clock) as std::sync::Arc<dyn crate::ptt::Clock>,
        };
        (runtime, clock)
    }

    fn runtime_with(
        deliverer: crate::delivery::tests_support::FakeDeliverer,
        submit: bool,
    ) -> Runtime {
        runtime_with_clock(deliverer, submit).0
    }
```

Write the two `// ... unchanged ...` comments out in full when you implement
them: copy the field list from the function you are replacing. They are elided
here only to keep the two new helpers readable side by side.

- [ ] **Step 4: Add the test for the journalled refusal**

```rust
    #[test]
    fn an_unreadable_repeat_is_recorded_against_the_hold_it_did_not_end() {
        let fake = crate::delivery::tests_support::FakeDeliverer::ok();
        let (mut runtime, clock) = runtime_with_clock(fake, false);
        let journal = std::sync::Arc::new(RecordingJournal::default());
        runtime.journal = Box::new(TestJournal(std::sync::Arc::clone(&journal)));
        let recorder = tone_recorder("ptt-unreadable-line");
        answer(&request("ptt", PANE_1), &recorder, &runtime);
        clock.advance(90);
        answer(&request("ptt", b"not json"), &recorder, &runtime);
        let lines = journal.0.lock().unwrap();
        assert!(
            lines.iter().any(|line| line.contains("the hold continues")),
            "the refusal is recorded against the hold it did not end: got {lines:?}"
        );
    }

    #[test]
    fn an_unreadable_request_with_no_hold_writes_no_ptt_line() {
        let fake = crate::delivery::tests_support::FakeDeliverer::ok();
        let (mut runtime, _clock) = runtime_with_clock(fake, false);
        let journal = std::sync::Arc::new(RecordingJournal::default());
        runtime.journal = Box::new(TestJournal(std::sync::Arc::clone(&journal)));
        let recorder = tone_recorder("ptt-unreadable-line-idle");
        answer(&request("ptt", b"not json"), &recorder, &runtime);
        let lines = journal.0.lock().unwrap();
        assert!(
            lines.is_empty(),
            "with no hold there is nothing a refusal failed to end: got {lines:?}"
        );
    }
```

```rust
/// Move the hold's stamp forward, if there is one, and say which pane it
/// belongs to. Called before a `ptt` request's context is examined, so that a
/// request the daemon cannot read never becomes evidence that the key came up
/// (`DESIGN_17.md`, section 2a).
///
/// The returned target is what lets the refusal that follows be journalled
/// against the hold it did not end.
fn refresh_hold(runtime: &Runtime) -> Option<String> {
    let mut held = runtime.hold.lock().ok()?;
    let hold = held.as_mut()?;
    hold.last_poke = runtime.clock.now();
    hold.pokes = hold.pokes.saturating_add(1);
    Some(hold.target.clone())
}
```

Both refusal arms then write the line, and only when a hold was actually
refreshed — with no hold open there is nothing the refusal failed to end:

```rust
        command if needs_target_pane(command) => {
            // A repeat that arrives is evidence the key is down, whatever its
            // payload turns out to say. The context answers where the take
            // goes, and that was settled when the hold began.
            let refreshed = if command == "ptt" {
                refresh_hold(runtime)
            } else {
                None
            };
            match context::parse(&request.context) {
                Err(why) => {
                    if let Some(target) = &refreshed {
                        runtime
                            .journal
                            .write(&unreadable_repeat_line(target, &why.to_string()));
                    }
                    (Reply::Error(why.to_string()), Control::Continue)
                }
                Ok(invocation) => match invocation.target_pane() {
                    None => {
                        if let Some(target) = &refreshed {
                            runtime.journal.write(&unreadable_repeat_line(
                                target,
                                "the invocation context names no focused pane",
                            ));
                        }
                        (
                            Reply::Error(
                                "the invocation context names no focused pane; \
                                 invoke this from a pane running an agent"
                                    .to_string(),
                            ),
                            Control::Continue,
                        )
                    }
                    // ... the `dictate`, `ptt` and stub arms follow, unchanged
                },
            }
        }
```

```rust
/// A repeat whose context could not be read, while a hold was open. Journal
/// only: the keypress's own reply already carries the failure, and the hold
/// continuing is the correct outcome rather than something to act on.
fn unreadable_repeat_line(target: &str, why: &str) -> String {
    format!("ptt {target}: a repeat could not be read ({why}); the hold continues")
}
```

- [ ] **Step 5: Add the handler**

Replace the `Some(_) => (Reply::Ok(format!("{command}: not implemented yet")), ...)` arm with a `ptt` arm, leaving the stub for the commands still unbuilt:

```rust
                Some(pane) if command == "ptt" => (
                    ptt(
                        recorder,
                        runtime,
                        pane,
                        invocation.focused_pane_cwd.as_deref(),
                        invocation.focused_pane_agent.as_deref(),
                    ),
                    Control::Continue,
                ),
```

```rust
/// One repeat of a held key.
///
/// The stamp has already been moved by `refresh_hold`, before this request's
/// context was read. What is left to decide is whether this repeat begins a
/// hold — and beginning one is the only thing here that does real work.
fn ptt(
    recorder: &Recorder,
    runtime: &Runtime,
    pane: &str,
    cwd: Option<&str>,
    agent: Option<&str>,
) -> Reply {
    if let Ok(held) = runtime.hold.lock() {
        if let Some(hold) = held.as_ref() {
            // A repeat. The target stays what it was: the pane is pinned when
            // the hold begins, so that text does not follow the focus while
            // somebody is still speaking.
            return Reply::Ok(format!("holding for {}", hold.target));
        }
    }
    match recorder.start(pane, cwd, agent) {
        Started::CouldNotStart(why) => Reply::Error(why),
        Started::PreviousFailure(why) => Reply::Error(why),
        // `.to_string()`, not `format!`: the string interpolates nothing, and
        // `clippy::useless_format` is an error under `-D warnings`.
        Started::AlreadyRunning => Reply::Error(
            "a take is already recording for another action; \
             end it with `herdr-voice dictate` before holding the key"
                .to_string(),
        ),
        Started::Began => {
            let now = runtime.clock.now();
            if let Ok(mut held) = runtime.hold.lock() {
                *held = Some(crate::ptt::Hold {
                    target: pane.to_string(),
                    cwd: cwd.map(|c| c.to_string()),
                    agent: agent.map(|a| a.to_string()),
                    began: now,
                    last_poke: now,
                    pokes: 1,
                });
            }
            runtime.clock.wake();
            Reply::Ok(format!("holding for {pane}"))
        }
    }
}
```

- [ ] **Step 6: Run the tests**

Run: `cargo test daemon::tests`
Expected: PASS, including the five new ones and every existing one.

- [ ] **Step 7: Gates and commit**

```bash
cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt --check
git add src/daemon.rs
git commit -m "A held key begins a hold, and a repeat the daemon cannot read still refreshes it"
```

---

### Task 4: The watcher

**Files:**
- Modify: `src/ptt.rs` (the loop)
- Modify: `src/daemon.rs:571-600` (`serve` spawns and joins it)
- Modify: `src/capture.rs:397-434` (`tests_support` gains one source, so a test can lose a device without hardware)
- Modify: `src/daemon.rs:935` — an existing test calls `transcribe` directly and must be destructured when its return becomes a tuple

**Interfaces:**
- Consumes: `decide`, `Clock`, `Hold` from Task 2; `Runtime.hold`, `Runtime.ptt`, `Runtime.clock` from Task 3.
- Produces, all in `src/daemon.rs`: `enum Reported { Yes, No }` and the changed `fn transcribe(..) -> (Reply, Reported)` with its `dictate` call site updated, `fn watch(recorder: Arc<Recorder>, runtime: Arc<Runtime>, stop: Arc<AtomicBool>)`, `fn end_take(recorder: &Recorder, runtime: &Runtime, hold: &crate::ptt::Hold)`, `fn discard_take(recorder: &Recorder, runtime: &Runtime, hold: &crate::ptt::Hold, held_ms: u64)`, `fn report_failure(runtime: &Runtime, target: &str, why: &str)`, `fn toast(runtime: &Runtime, title: &str, body: &str)`; and `crate::capture::tests_support::LosingSource`.

- [ ] **Step 1: Write the failing tests**

`FakeDeliverer` is `Clone` and shares its log through an inner `Arc`
(`src/delivery.rs:76-86`), so the test keeps a clone and reads `.calls()`; there
is no constructor that takes an outside log. `runtime_with` takes the
`FakeDeliverer` unboxed (`src/daemon.rs:735`). `RecordingJournal` is a tuple
struct deriving `Default` (`src/daemon.rs:946`), read through `TestJournal`
(`:954`) exactly as `the_delivering_line_precedes_the_failure_line_and_names_the_reason`
does at `:961`.

```rust
    /// Poll the fake's log until it has a call or the bound expires. Waits on
    /// the condition rather than on a duration, so it neither slows the suite
    /// nor goes flaky on a loaded machine.
    fn wait_for_calls(
        fake: &crate::delivery::tests_support::FakeDeliverer,
        within: std::time::Duration,
    ) -> Vec<crate::delivery::tests_support::Call> {
        let deadline = std::time::Instant::now() + within;
        loop {
            let calls = fake.calls();
            if !calls.is_empty() || std::time::Instant::now() >= deadline {
                return calls;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }

    #[test]
    fn the_deadline_ends_the_hold_and_the_take_is_delivered() {
        let fake = crate::delivery::tests_support::FakeDeliverer::ok();
        let (runtime, clock) = runtime_with_clock(fake.clone(), false);
        let runtime = std::sync::Arc::new(runtime);
        let recorder = std::sync::Arc::new(tone_recorder("ptt-deadline"));
        let stop = std::sync::Arc::new(AtomicBool::new(false));

        let watcher = {
            let recorder = std::sync::Arc::clone(&recorder);
            let runtime = std::sync::Arc::clone(&runtime);
            let stop = std::sync::Arc::clone(&stop);
            thread::spawn(move || watch(recorder, runtime, stop))
        };

        answer(&request("ptt", PANE_1), &recorder, &runtime);
        // A hold of 400 ms, over the 300 ms minimum: advance, then let a repeat
        // stamp the new time the way a real one would.
        clock.advance(400);
        answer(&request("ptt", PANE_1), &recorder, &runtime);
        // The gap passes with no further repeat.
        clock.advance(1_000);

        let calls = wait_for_calls(&fake, std::time::Duration::from_secs(5));
        assert!(
            matches!(
                calls.first(),
                Some(crate::delivery::tests_support::Call::Insert(pane, _)) if pane == "w1:p1"
            ),
            "the take is inserted into the pinned pane: got {calls:?}"
        );
        assert!(runtime.hold.lock().unwrap().is_none(), "the hold is cleared");

        stop.store(true, Ordering::SeqCst);
        runtime.clock.wake();
        clock.advance(1);
        watcher.join().unwrap();
    }

    #[test]
    fn a_tap_is_discarded_and_says_so() {
        let fake = crate::delivery::tests_support::FakeDeliverer::ok();
        let (mut runtime, clock) = runtime_with_clock(fake.clone(), false);
        let journal = std::sync::Arc::new(RecordingJournal::default());
        runtime.journal = Box::new(TestJournal(std::sync::Arc::clone(&journal)));
        let runtime = std::sync::Arc::new(runtime);
        let recorder = std::sync::Arc::new(tone_recorder("ptt-tap"));
        let stop = std::sync::Arc::new(AtomicBool::new(false));

        let watcher = {
            let recorder = std::sync::Arc::clone(&recorder);
            let runtime = std::sync::Arc::clone(&runtime);
            let stop = std::sync::Arc::clone(&stop);
            thread::spawn(move || watch(recorder, runtime, stop))
        };

        // One repeat and nothing more: held for 0 ms, far under the minimum.
        answer(&request("ptt", PANE_1), &recorder, &runtime);
        clock.advance(1_000);

        // Wait for the hold to be cleared rather than for a duration.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while runtime.hold.lock().unwrap().is_some() && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(5));
        }

        assert!(
            fake.calls().is_empty(),
            "a tap delivers nothing: got {:?}",
            fake.calls()
        );
        let lines = journal.0.lock().unwrap();
        assert!(
            lines.iter().any(|line| line.contains("too short")),
            "a tap says it was a tap: got {lines:?}"
        );

        drop(lines);
        stop.store(true, Ordering::SeqCst);
        runtime.clock.wake();
        clock.advance(1);
        watcher.join().unwrap();
    }
```

A third test covers the case the design names and the other two do not: a
device that died during the hold, discovered by the stop the deadline triggers.
It needs a source that fails, which `tests_support` does not have — the scripted
`Fake` lives in `capture.rs`'s own private test module. Add one beside
`ToneSource` (`src/capture.rs:417`), in the same shape:

```rust
    /// Hears one moment and then reports the device gone, so a test can drive
    /// the mid-take failure without hardware.
    pub struct LosingSource;

    impl Source for LosingSource {
        fn start(&mut self, _device: Option<&str>, sink: Sink) -> Result<Format, String> {
            sink.push(Event::Samples(vec![0.0; 4_800]));
            sink.push(Event::Failed("the device went away".to_string()));
            Ok(Format {
                rate: 48_000,
                channels: 1,
            })
        }
        fn stop(&mut self) {}
    }
```

and a recorder built on it, beside `tone_recorder` (`src/daemon.rs:720`):

```rust
    fn losing_recorder(tag: &str) -> Recorder {
        Recorder::spawn(
            || Box::new(crate::capture::tests_support::LosingSource),
            crate::config::Audio::default(),
            std::env::temp_dir().join(format!("daemon-takes-{tag}-{}", std::process::id())),
        )
    }
```

```rust
    #[test]
    fn a_device_lost_during_a_hold_gives_a_release_line_and_a_failure_line() {
        let fake = crate::delivery::tests_support::FakeDeliverer::ok();
        let (mut runtime, clock) = runtime_with_clock(fake.clone(), false);
        let journal = std::sync::Arc::new(RecordingJournal::default());
        runtime.journal = Box::new(TestJournal(std::sync::Arc::clone(&journal)));
        let runtime = std::sync::Arc::new(runtime);
        let recorder = std::sync::Arc::new(losing_recorder("ptt-device-lost"));
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
        clock.advance(1_000);

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while runtime.hold.lock().unwrap().is_some() && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(5));
        }

        assert!(
            fake.calls().is_empty(),
            "a take whose device went away delivers nothing: {:?}",
            fake.calls()
        );
        let lines = journal.0.lock().unwrap();
        let released = lines
            .iter()
            .find(|line| line.contains("released"))
            .unwrap_or_else(|| panic!("the hold was released, and says so: {lines:?}"));
        assert!(
            !released.contains("device"),
            "the release line says why the hold ended, not why the take failed: {released}"
        );
        assert!(
            lines.iter().any(|line| line.contains("the device went away")),
            "and the failure is its own line: {lines:?}"
        );

        drop(lines);
        stop.store(true, Ordering::SeqCst);
        runtime.clock.wake();
        clock.advance(1);
        watcher.join().unwrap();
    }
```

- [ ] **Step 2: Run it and watch it fail**

Run: `cargo test daemon::tests::the_deadline_ends`
Expected: FAIL — no function `watch`.

- [ ] **Step 3: Write the loop**

```rust
/// Ends a hold when the repeats stop.
///
/// One thread for the daemon's life: a hold is a singleton, because the
/// recorder serves one take at a time. The pipeline runs here too — it takes
/// seconds, and no hold can exist during them, because a `ptt` arriving while
/// the recorder is busy is refused rather than queued.
fn watch(recorder: Arc<Recorder>, runtime: Arc<Runtime>, stop: Arc<AtomicBool>) {
    loop {
        if stop.load(Ordering::SeqCst) {
            // A bare return, on purpose, and only until Task 5. Calling
            // `finish_on_shutdown` here would make Task 5's test pass the
            // moment it was written, leaving nothing to watch fail — and a
            // test never seen red proves nothing. This return *is* the defect
            // Task 5 exists against: a daemon going away with a key down,
            // losing the recording silently.
            return;
        }
        let now = runtime.clock.now();
        let decision = {
            let held = match runtime.hold.lock() {
                Ok(held) => held,
                // A poisoned lock would otherwise spin this thread forever.
                Err(poisoned) => poisoned.into_inner(),
            };
            crate::ptt::decide(now, held.as_ref(), &runtime.ptt)
        };
        match decision {
            crate::ptt::Decision::Idle => runtime.clock.wait_until(now.saturating_add(3_600_000)),
            crate::ptt::Decision::KeepWaiting { until } => runtime.clock.wait_until(until),
            crate::ptt::Decision::Release { held_ms } => {
                let Some(hold) = take_hold(&runtime) else { continue };
                runtime.journal.write(&released_line(&hold.target, held_ms, hold.pokes));
                end_take(&recorder, &runtime, &hold);
            }
            crate::ptt::Decision::TooShort { held_ms } => {
                let Some(hold) = take_hold(&runtime) else { continue };
                runtime.journal.write(&too_short_line(&hold.target, held_ms, runtime.ptt.min_hold_ms));
                discard_take(&recorder, &runtime, &hold, held_ms);
            }
        }
    }
}

/// Clear the hold and return it, so no second path can act on the same one.
fn take_hold(runtime: &Runtime) -> Option<crate::ptt::Hold> {
    match runtime.hold.lock() {
        Ok(mut held) => held.take(),
        Err(poisoned) => poisoned.into_inner().take(),
    }
}
```

```rust
/// A hold that was released: stop the recording and run the take through the
/// pipeline the toggle already runs (`src/daemon.rs:165-178`).
///
/// `recorder.stop()` failing here is how a device that died during the hold is
/// discovered — the watcher has no way to learn it sooner (`DESIGN_17.md`,
/// section 3). That is a second fact, not a different reason the hold ended:
/// the release line is already written, and this adds the failure beside it.
fn end_take(recorder: &Recorder, runtime: &Runtime, hold: &crate::ptt::Hold) {
    match recorder.stop() {
        Ok(take) => {
            let collected = take_bias(
                runtime,
                &take.target,
                take.cwd.as_deref(),
                take.agent.as_deref(),
            );
            // `transcribe` reports its own delivery failure, and only that one.
            // Reporting again here would put two journal lines and two toasts
            // on one failure; saying nothing would leave the other two silent,
            // because a hold has no keypress waiting to be told.
            match transcribe(runtime, &take, &collected.bias) {
                (_, Reported::Yes) => {}
                (Reply::Error(why), Reported::No) => report_failure(runtime, &hold.target, &why),
                (Reply::Ok(_), Reported::No) => {}
            }
        }
        Err(why) => report_failure(runtime, &hold.target, &why.to_string()),
    }
}

/// A hold too short to be one: stop the recording, remove it, and say so.
///
/// Only the `Ok` path has a file to remove. When the recorder refuses a take
/// itself — a lost device, or a level under the floor — it removes the file
/// before returning the error (`src/capture.rs:347-370`).
fn discard_take(
    recorder: &Recorder,
    runtime: &Runtime,
    hold: &crate::ptt::Hold,
    held_ms: u64,
) {
    match recorder.stop() {
        Ok(take) => {
            let _ = std::fs::remove_file(&take.path);
            // The journal line was written by the caller; this is the part the
            // person sees without going to look.
            toast(
                runtime,
                "Too short to be a hold",
                &format!(
                    "{}: held {held_ms} ms, and a hold starts at {} ms",
                    hold.target, runtime.ptt.min_hold_ms
                ),
            );
        }
        Err(why) => report_failure(runtime, &hold.target, &why.to_string()),
    }
}

/// A failure the person has to know about: recorded, and raised where they are
/// looking when `[ui] toasts` is on.
fn report_failure(runtime: &Runtime, target: &str, why: &str) {
    runtime.journal.write(&take_failed_line(target, why));
    toast(runtime, "Dictation failed", &format!("{target}: {why}"));
}

/// `[ui] toasts` decides whether the person is interrupted. It never decides
/// whether a failure is recorded — the journal line is written by the caller in
/// every case, including this one.
fn toast(runtime: &Runtime, title: &str, body: &str) {
    if !runtime.delivery_settings.toasts {
        return;
    }
    if let Err(why) = runtime.deliverer.notify(title, body) {
        runtime.journal.write(&toast_failed_line(title, &why.to_string()));
    }
}
```

**Why `transcribe` has to say whether it reported.** It journals and toasts on
a delivery failure (`src/daemon.rs:386-393`) and on nothing else: recognition
being unavailable, and recognition failing, each return a `Reply::Error` and
write nothing (`:318-334`). For the toggle that is right — the client prints the
reply and the person reads it. For a hold there is no reply reader at all, so
those two failures would pass in silence. Telling them apart by looking at the
message text would be guessing; the return says it instead.

```rust
/// Whether the failure inside a `Reply::Error` has already been journalled and
/// toasted by the code that produced it.
///
/// Only the delivery branch of `transcribe` reports its own failure. This is
/// how a caller with nobody waiting for the reply — the watcher, ending a hold
/// — knows which failures it still has to announce, without reading the
/// message to guess.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Reported {
    Yes,
    No,
}
```

`transcribe`'s signature becomes
`fn transcribe(runtime: &Runtime, take: &crate::capture::Take, bias: &str) -> (Reply, Reported)`.
Each of its four returns gains the flag: the two recognition errors return
`Reported::No`, the success returns `Reported::No`, and the delivery-failure
branch — the one that writes `delivery_failed_line` and raises its toast —
returns `Reported::Yes`. The free function `transcribe` has exactly two call sites, and both must be
changed or the tree will not compile. (`grep -rn "transcribe(" src/` also
matches `engine.transcribe`, a different function on the `Engine` trait; those
are untouched.)

**`dictate`, `src/daemon.rs:177`** — takes the reply and drops the flag, so the
toggle behaves exactly as it does today:

```rust
                transcribe(runtime, &take, &collected.bias).0
```

**An existing test, `src/daemon.rs:935`**, inside
`the_pane_and_the_path_are_collapsed_too_although_they_precede_the_transcript`.
It calls `transcribe` directly and matches on the result, so the tuple breaks
it. Destructure, and use the opportunity to pin the flag where it is set — this
test already builds the delivery failure that sets it:

```rust
        let (reply, reported) = transcribe(&runtime, &take, "");
        assert_eq!(
            reported,
            Reported::Yes,
            "the delivery branch reports its own failure; that is what lets a \
             hold know which failures it still has to announce"
        );
        let text = match reply {
            Reply::Error(text) => text,
            other => panic!("expected Reply::Error, got {other:?}"),
        };
```

The test that pins it, in Task 4:

```rust
    #[test]
    fn a_delivery_failure_during_a_hold_is_reported_once_and_not_twice() {
        let fake = crate::delivery::tests_support::FakeDeliverer::failing(
            crate::delivery::DeliveryError::Rejected("pane_not_found".into()),
        );
        let (mut runtime, clock) = runtime_with_clock(fake.clone(), false);
        runtime.delivery_settings.toasts = true;
        let journal = std::sync::Arc::new(RecordingJournal::default());
        runtime.journal = Box::new(TestJournal(std::sync::Arc::clone(&journal)));
        let runtime = std::sync::Arc::new(runtime);
        let recorder = std::sync::Arc::new(tone_recorder("ptt-delivery-failure"));
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
        clock.advance(1_000);

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while runtime.hold.lock().unwrap().is_some() && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(5));
        }

        let notifies = fake
            .calls()
            .into_iter()
            .filter(|call| matches!(call, crate::delivery::tests_support::Call::Notify(_, _)))
            .count();
        assert_eq!(notifies, 1, "one failure, one toast: {:?}", fake.calls());
        let lines = journal.0.lock().unwrap();
        let failures = lines
            .iter()
            .filter(|line| line.contains("pane_not_found"))
            .count();
        assert_eq!(failures, 1, "one failure, one journal line: {lines:?}");

        drop(lines);
        stop.store(true, Ordering::SeqCst);
        runtime.clock.wake();
        clock.advance(1);
        watcher.join().unwrap();
    }
```

`toast_failed_line` already exists (`src/daemon.rs:461`); `take_failed_line` is
new and belongs to Task 8 with the other lines. Neither `end_take` nor
`discard_take` introduces a pipeline: both reuse what `dictate`
(`src/daemon.rs:151-181`) already calls.

- [ ] **Step 4: Spawn and join it in `serve`**

```rust
    let stop = Arc::new(AtomicBool::new(false));
    let watcher = {
        let recorder = Arc::clone(&recorder);
        let runtime = Arc::clone(&runtime);
        let stop = Arc::clone(&stop);
        thread::spawn(move || watch(recorder, runtime, stop))
    };
```

and, after the accept loop breaks and before `serve` returns:

```rust
    // The watcher owns the hold, and a hold open here means a recording is
    // still running. Waking it and waiting is what keeps that take rather than
    // losing it to the daemon going away.
    runtime.clock.wake();
    if let Err(e) = watcher.join() {
        eprintln!("the watcher thread ended badly: {e:?}");
    }
```

- [ ] **Step 5: Remove the dead-code allowance, then run the tests and the gates**

Every item in `src/ptt.rs` now has a caller outside its own tests, and so does
every field the daemon gained: the hold and the clock were read from Task 3, and
this task adds the watcher, which is what reads `Runtime.ptt` and calls `decide`.

**There are two allowances to delete, in two files.** Both were added because a
commit must never be red, and both name this step as where they go:

- `#![allow(dead_code)]` with its comment at the top of `src/ptt.rs`, from Task 2
  Step 7. It is module-level and covers only that file.
- `#[allow(dead_code)]` with its comment on the `ptt` field of `Runtime`, at
  roughly `src/daemon.rs:84`, added in Task 3. A module-level allowance in
  `src/ptt.rs` does not reach a field declared in `src/daemon.rs`, which is why
  it needed its own.

Run: `cargo test` then `cargo clippy --all-targets -- -D warnings` and
`cargo fmt --check`.
Expected: PASS, no warnings — including with the allowance gone. If clippy names
something still unused, that item has no caller and the task is not finished;
find out which of the plan's pieces was not written rather than putting the
allowance back.
If a test hangs, the watcher is waiting on a clock nobody advances — check the
`Idle` branch is woken by `wake()`, and that both `Clock` implementations
consume the `woken` flag as the contract in Task 2 requires.

- [ ] **Step 6: Commit**

```bash
git add src/ptt.rs src/daemon.rs
git commit -m "A watcher ends the hold when the repeats stop, and keeps the take when they were a tap"
```

---

### Task 5: The daemon stopping with a hold open

**Files:**
- Modify: `src/daemon.rs` (`finish_on_shutdown`, and the ordering in `serve`)

**Interfaces:**
- Consumes: `take_hold`, the journal lines from Task 4.
- Produces: `fn finish_on_shutdown(recorder: &Recorder, runtime: &Runtime)`.

**Why this is its own task.** `serve` keeps its stop flag as a local `Arc<AtomicBool>` (`src/daemon.rs:577`) and returns as soon as the accept loop breaks. Without this, stopping the daemon while a key is held loses the recording with nothing said — the failure class this whole issue exists against.

This task writes `finish_on_shutdown` **and** replaces Task 4's bare `return`
with the call to it. Task 4 left the defect in place so that this task's test
has something to fail against.

- [ ] **Step 1: Write the failing test**

```rust
    #[test]
    fn stopping_the_daemon_with_a_hold_open_keeps_the_take_and_names_it() {
        let fake = crate::delivery::tests_support::FakeDeliverer::ok();
        let (mut runtime, clock) = runtime_with_clock(fake.clone(), false);
        let journal = std::sync::Arc::new(RecordingJournal::default());
        runtime.journal = Box::new(TestJournal(std::sync::Arc::clone(&journal)));
        let runtime = std::sync::Arc::new(runtime);
        let recorder = std::sync::Arc::new(tone_recorder("ptt-shutdown"));
        let stop = std::sync::Arc::new(AtomicBool::new(false));

        let watcher = {
            let recorder = std::sync::Arc::clone(&recorder);
            let runtime = std::sync::Arc::clone(&runtime);
            let stop = std::sync::Arc::clone(&stop);
            thread::spawn(move || watch(recorder, runtime, stop))
        };

        // A key is down, well inside the gap, when the daemon is told to stop.
        answer(&request("ptt", PANE_1), &recorder, &runtime);
        clock.advance(400);
        answer(&request("ptt", PANE_1), &recorder, &runtime);
        stop.store(true, Ordering::SeqCst);
        runtime.clock.wake();
        clock.advance(1);
        watcher.join().unwrap();

        assert!(
            fake.calls().is_empty(),
            "a daemon going away delivers nothing: got {:?}",
            fake.calls()
        );
        assert!(
            runtime.hold.lock().unwrap().is_none(),
            "the hold is cleared rather than left for nobody"
        );
        let lines = journal.0.lock().unwrap();
        let kept = lines
            .iter()
            .find(|line| line.contains(".wav"))
            .unwrap_or_else(|| panic!("the kept take is named: got {lines:?}"));
        assert!(
            !kept.contains("released"),
            "stopping is not a release: nothing here says the key came up: {kept}"
        );
    }
```

- [ ] **Step 2: Run it and watch it fail** — `cargo test stopping_the_daemon_with_a_hold`

- [ ] **Step 3: Write it**

```rust
/// The daemon is going away with a key still down. This is not a release: no
/// repeat stopped arriving, so nothing here says the key came up. The recording
/// is stopped and kept, and the journal names where it is.
fn finish_on_shutdown(recorder: &Recorder, runtime: &Runtime) {
    let Some(hold) = take_hold(runtime) else { return };
    match recorder.stop() {
        Ok(take) => runtime.journal.write(&kept_on_shutdown_line(
            &hold.target,
            &take.path.display().to_string(),
        )),
        Err(why) => runtime
            .journal
            .write(&shutdown_lost_line(&hold.target, &why.to_string())),
    }
}
```

- [ ] **Step 4: Run the tests and the gates. Commit.**

```bash
git add src/daemon.rs
git commit -m "A daemon stopping with a key down keeps the recording and says where it is"
```

---

### Task 6: The two collisions

**Files:**
- Modify: `src/daemon.rs` (`fn dictate`, and the `AlreadyRunning` arm of `fn ptt`)

**Interfaces:**
- Consumes: `Runtime.hold` from Task 3.
- Produces: nothing new; two error replies.

**What this is not.** Making `dictate` end a hold would be an action that ends a hold immediately, which is issue #55 and deliberately outside this task. Both directions refuse and explain.

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn a_dictate_during_a_hold_is_refused_and_says_what_is_running() {
    let runtime = fake_runtime("a transcript");
    let recorder = tone_recorder("collide-dictate");
    answer(&request("ptt", PANE_1), &recorder, &runtime);
    let (reply, _) = answer(&request("dictate", PANE_1), &recorder, &runtime);
    match reply {
        Reply::Error(why) => {
            assert!(why.contains("holding"), "names what is running: {why}");
            assert!(why.contains("w1:p1"), "names the pane: {why}");
        }
        other => panic!("a hold must not be ended by dictate: {other:?}"),
    }
    assert!(runtime.hold.lock().unwrap().is_some(), "the hold survives");
}

#[test]
fn a_ptt_while_a_toggle_take_runs_is_refused_and_says_how_it_ends() {
    let runtime = fake_runtime("a transcript");
    let recorder = tone_recorder("collide-ptt");
    answer(&request("dictate", PANE_1), &recorder, &runtime);
    let (reply, _) = answer(&request("ptt", PANE_1), &recorder, &runtime);
    match reply {
        Reply::Error(why) => assert!(why.contains("dictate"), "names how it ends: {why}"),
        other => panic!("a toggle take must not be taken over by a hold: {other:?}"),
    }
    assert!(runtime.hold.lock().unwrap().is_none(), "no hold was begun");
}
```

- [ ] **Step 2: Run them and watch them fail** — `cargo test daemon::tests::a_dictate_during daemon::tests::a_ptt_while`

- [ ] **Step 3: Guard `dictate`**

At the top of `fn dictate`, before it asks the recorder anything:

```rust
    // A hold is not ended by the toggle. An action that ends a hold at once is
    // issue #55; doing it here would be that feature under another name.
    if let Ok(held) = runtime.hold.lock() {
        if let Some(hold) = held.as_ref() {
            // The test asserts this message contains "holding", and the word
            // is not decoration: it is the same word the `ptt` success reply
            // uses ("holding for {pane}"), so the two say the same thing about
            // the same state.
            return Reply::Error(format!(
                "holding for {}: a key is being held, and the recording ends \
                 on its own when the key comes up",
                hold.target
            ));
        }
    }
```

The `ptt` side is already written in Task 3's `Started::AlreadyRunning` arm; this task adds its test and, if the wording there does not name `dictate` as the way the running take ends, fixes it so it does.

- [ ] **Step 4: Run the tests and the gates. Commit.**

```bash
git add src/daemon.rs
git commit -m "Neither ptt nor dictate takes the other's recording; both refuse and say why"
```

---

### Task 7: Routing `ptt` in the client

**Files:**
- Modify: `src/main.rs:112-124` (`IMPLEMENTED`, `USAGE`), `:150-168` (the dispatch arms)
- Modify: `src/main.rs`, the test `the_commands_this_issue_implements_are_not_in_the_unimplemented_arm` — it lists `"ptt"` among the commands that must **not** be implemented, so the suite is red until `ptt` moves to the other list. Moving it is what makes this task's red state
- Modify: `src/client.rs` — the assertion `assert_eq!(timeout_for("ptt"), REPLY_TIMEOUT)` already exists **inside** `the_command_that_waits_on_work_gets_the_long_bound`. Move it out into its own test under the name below rather than writing a second copy of it
- Check: `herdr-plugin.toml` needs no change — `ptt` is already declared

**Interfaces:**
- Consumes: the daemon's `ptt` handler from Task 3.
- Produces: `herdr-voice ptt` reaching the daemon instead of exiting 69.

- [ ] **Step 1: Write the failing tests**

Note which of these is worth writing. Once `ptt` moves between the two lists in
`IMPLEMENTED`, both halves of a `ptt`-specific test are already asserted
generically — membership by `the_commands_this_issue_implements_are_not_in_the_unimplemented_arm`,
and the usage line by `the_usage_text_names_every_implemented_command`, which
loops over `IMPLEMENTED`. So do not add a third test that repeats them; the
generic pair is what keeps the next command honest too.

```rust
// src/client.rs — this one should already pass; it pins what must not change.
#[test]
fn ptt_keeps_the_short_reply_bound() {
    // Every repeat answers at once: a hold does not wait for recognition, so
    // the long bound `dictate` needs would only hide a wedged daemon here.
    assert_eq!(timeout_for("ptt"), REPLY_TIMEOUT);
}
```

- [ ] **Step 2: Run them** — `cargo test ptt_is_implemented ptt_keeps_the_short`
Expected: the first FAILS, the second PASSES.

- [ ] **Step 3: Move `ptt` across**

```rust
const IMPLEMENTED: &[&str] = &["daemon", "doctor", "cancel", "dictate", "model", "ptt"];
```

In `USAGE`, after the `dictate` line:

```
  herdr-voice ptt        one keypress of hold-to-talk; bind it to a key
```

In `main`, move `Command::Ptt` out of the stub arm and into the client arm:

```rust
        other @ (Command::Cancel | Command::Dictate | Command::Ptt) => {
```

leaving `other @ (Command::Setup | Command::Status | Command::Mic)` on the stub.

- [ ] **Step 4: Run every gate, including the manifest check**

Run: `cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt --check && python3 scripts/check_manifest.py`
Expected: PASS. The manifest check matters here: it is the only thing that catches a command the manifest names and the binary rejects.

- [ ] **Step 5: Commit**

```bash
git add src/main.rs src/client.rs
git commit -m "ptt reaches the daemon instead of exiting 69"
```

---

### Task 8: What the person sees

**Files:**
- Modify: `src/daemon.rs` (the journal-line functions beside `delivering_line` at `:448`, and the toast calls)

**Interfaces:**
- Consumes: the `Decision` handling from Task 4, `finish_on_shutdown` from Task 5.
- Produces: the tests. The line functions themselves — `released_line`, `too_short_line`, `take_failed_line`, `kept_on_shutdown_line`, `shutdown_lost_line` — are **already written**, in Tasks 4 and 5: the watcher and `finish_on_shutdown` call them, and neither task compiles without them. This task adds the tests that pin their wording, and the two toast tests. Expect the wording tests to pass as soon as they are written; that is not a reason to skip them, because what they pin is that a line names the pane and the numbers, which nothing else checks. `unreadable_repeat_line` is **not** here: it belongs to Task 3, where the behaviour it records lives and where its two tests are.

**The split.** Success is silent — the text appearing in the input box is the message. Every failure writes a journal line, and raises a toast when `[ui] toasts` is on. Two lines are journal-only because neither is a failure the person must act on: the repeat that could not be read while a hold continued — written in Task 3, at the refusal — and the reason a hold ended.

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn every_ptt_line_names_what_happened_and_what_to_do() {
    let lines = vec![
        too_short_line("w1:p1", 120, 300),
        kept_on_shutdown_line("w1:p1", "/takes/1789-1-2.wav"),
        shutdown_lost_line("w1:p1", "the device went away"),
    ];
    for line in lines {
        assert!(!line.is_empty());
        assert!(line.contains("w1:p1"), "names the pane: {line}");
    }
    assert!(
        too_short_line("w1:p1", 120, 300).contains("120"),
        "a tap names its own length, so the minimum can be judged"
    );
    assert!(
        kept_on_shutdown_line("w1:p1", "/takes/1789-1-2.wav").contains("/takes/1789-1-2.wav"),
        "a kept take names where it is, or it is lost in practice"
    );
}

    #[test]
    fn a_tap_raises_a_toast_so_it_is_not_mistaken_for_a_broken_plugin() {
        let fake = crate::delivery::tests_support::FakeDeliverer::ok();
        let (mut runtime, clock) = runtime_with_clock(fake.clone(), false);
        // `runtime_with` builds with toasts off; this case is about them being on.
        runtime.delivery_settings.toasts = true;
        let runtime = std::sync::Arc::new(runtime);
        let recorder = std::sync::Arc::new(tone_recorder("ptt-tap-toast"));
        let stop = std::sync::Arc::new(AtomicBool::new(false));
        let watcher = {
            let recorder = std::sync::Arc::clone(&recorder);
            let runtime = std::sync::Arc::clone(&runtime);
            let stop = std::sync::Arc::clone(&stop);
            thread::spawn(move || watch(recorder, runtime, stop))
        };

        answer(&request("ptt", PANE_1), &recorder, &runtime);
        clock.advance(1_000);
        let calls = wait_for_calls(&fake, std::time::Duration::from_secs(5));

        assert!(
            calls
                .iter()
                .any(|call| matches!(call, crate::delivery::tests_support::Call::Notify(_, _))),
            "a tap is announced, or it is indistinguishable from a broken plugin: {calls:?}"
        );
        assert!(
            !calls
                .iter()
                .any(|call| matches!(call, crate::delivery::tests_support::Call::Insert(_, _))),
            "and it delivers nothing: {calls:?}"
        );

        stop.store(true, Ordering::SeqCst);
        runtime.clock.wake();
        clock.advance(1);
        watcher.join().unwrap();
    }

    #[test]
    fn a_tap_with_toasts_off_still_writes_the_journal_line() {
        let fake = crate::delivery::tests_support::FakeDeliverer::ok();
        let (mut runtime, clock) = runtime_with_clock(fake.clone(), false);
        // Toasts off is the default this helper builds; stated rather than
        // implied, because the whole point of the test is that key's value.
        runtime.delivery_settings.toasts = false;
        let journal = std::sync::Arc::new(RecordingJournal::default());
        runtime.journal = Box::new(TestJournal(std::sync::Arc::clone(&journal)));
        let runtime = std::sync::Arc::new(runtime);
        let recorder = std::sync::Arc::new(tone_recorder("ptt-tap-quiet"));
        let stop = std::sync::Arc::new(AtomicBool::new(false));
        let watcher = {
            let recorder = std::sync::Arc::clone(&recorder);
            let runtime = std::sync::Arc::clone(&runtime);
            let stop = std::sync::Arc::clone(&stop);
            thread::spawn(move || watch(recorder, runtime, stop))
        };

        answer(&request("ptt", PANE_1), &recorder, &runtime);
        clock.advance(1_000);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while runtime.hold.lock().unwrap().is_some() && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(5));
        }

        assert!(
            fake.calls().is_empty(),
            "no toast when toasts are off: {:?}",
            fake.calls()
        );
        let lines = journal.0.lock().unwrap();
        assert!(
            lines.iter().any(|line| line.contains("too short")),
            "`[ui] toasts` decides interruption, never whether a failure is \
             recorded at all: got {lines:?}"
        );

        drop(lines);
        stop.store(true, Ordering::SeqCst);
        runtime.clock.wake();
        clock.advance(1);
        watcher.join().unwrap();
    }
```

- [ ] **Step 2: Run them, watch them fail, write the lines, run again**

Each line is one `format!`. Follow the wording of the existing ones (`src/daemon.rs:448-470`): what happened, then what to do about it.

- [ ] **Step 3: Gates and commit**

```bash
git add src/daemon.rs
git commit -m "Every way a hold can end says so, and a tap says it was a tap"
```

---

### Task 9: The documents

**Files:**
- Modify: `docs/design.md:175-200` (section 5) and `:250-256` (the `[ptt]` sketch in section 7)
- Modify: `docs/decisions.md` (one row)

- [ ] **Step 1: Correct section 5**

Replace the 250 milliseconds with one second and the reason for it: a gap in the repeat stream is not proof of release; the cost of a gap too long is a tail on every take, and of one too short a hold split in two whose second fragment can fall below the minimum and be discarded. State that the value is configuration and that the measurement which would settle it — the worst gap inside a hold on a loaded machine — is issue #56.

- [ ] **Step 2: Correct section 7's sketch** so `[ptt]` shows `release_ms = 1000` and `min_hold_ms = 300`, matching `config::Ptt`.

- [ ] **Step 3: Add the row to `docs/decisions.md`**

| Decision | Basis | Where |
|---|---|---|
| The push-to-talk release gap is one second, and the hold lives in the daemon's memory rather than in a file | One second is about twelve times the 85 ms median auto-repeat interval and ten times the largest observed start-of-hold gap, so it survives a stall an order of magnitude worse than anything measured; the cost of a longer gap is a tail on every take. The file the ticket describes belongs to a shell prototype with no resident process: holding the stamps in the daemon removes the truncated read that cost the prototype 22 takes rather than defending against it | 2026-09-14, #17 |

- [ ] **Step 4: Grep the files you just wrote for editing debris, then commit**

```bash
grep -nE '^(<<<<<<< |>>>>>>> |=======$)' docs/design.md docs/decisions.md
grep -nE '(^|[^`])</?(new|old)_string>' docs/design.md docs/decisions.md
git add docs/design.md docs/decisions.md
git commit -m "The release gap is one second, and the design says why"
```

---

### Task 10: The measured repeat rate

**Files:**
- Modify: `src/daemon.rs` tests (one test)
- Modify: `docs/evidence.md` (one section)

**Why measured rather than asserted.** AC-11 asks for a number, not a claim. A repeat that cannot be served twelve times a second turns a hold into a queue, and the client's two-second bound would start timing out under the person's own key.

- [ ] **Step 1: Write the measurement**

```rust
#[test]
fn a_repeat_is_served_fast_enough_to_sustain_twelve_a_second() {
    let runtime = fake_runtime("a transcript");
    let recorder = tone_recorder("ptt-rate");
    let request = request("ptt", PANE_1);
    answer(&request, &recorder, &runtime);
    let started = std::time::Instant::now();
    let repeats = 120;
    for _ in 0..repeats {
        answer(&request, &recorder, &runtime);
    }
    let each = started.elapsed() / repeats;
    // Twelve a second is one every ~83 ms. A bound of 8 ms is an order of
    // magnitude of headroom and still fails loudly if a repeat ever starts
    // doing real work — a file write, an allocation that grows with the hold.
    assert!(
        each < std::time::Duration::from_millis(8),
        "a repeat took {each:?}; twelve a second needs one every 83 ms"
    );
    eprintln!("repeat served in {each:?}");
}
```

- [ ] **Step 2: Run it, read the number it prints, and write the section**

Add to `docs/evidence.md`, beside the auto-repeat table, a short section naming the platform, what was measured (the daemon's own handling of a repeat, with a scripted source and a fake engine — not a real keypress), the number, and what it does not establish: that this is the daemon in process, not the client's round trip through the socket, which the hand verification of Task 11 exercises instead.

- [ ] **Step 3: Gates and commit**

```bash
git add src/daemon.rs docs/evidence.md
git commit -m "Measure what a repeat costs the daemon, rather than asserting it is cheap"
```

---

### Task 11: Verify it by hand, and write down what happened

**Files:**
- Modify: `docs/evidence.md`
- Modify: `tasks/17/RUN_17.md`

**This task writes no production code.** It is AC-15, and it is the only criterion that cannot be met without a microphone, a real key and a live herdr.

- [ ] **Step 1: Build and install the checkout**

```bash
cargo build --release
herdr plugin link .
```

- [ ] **Step 2: Bind a key by hand**

`setup` is still a stub (#41), so the binding goes into the user's herdr configuration by hand, pointing at this plugin's `ptt` action. Note in the run file that the keys already bound on the development machine run an older shell prototype, so a new binding is needed rather than a reused one.

- [ ] **Step 3: Hold the key, speak, release**

Say something in Russian carrying this repository's own English terms — file names, flags, commands. **Nothing that names an employer, a client, an internal system or a person.** The recording lands in the state directory and is not covered by the leak gate.

- [ ] **Step 4: Record what happened**

In `docs/evidence.md`, with the platform named as that file does throughout: whether the hold recorded for as long as the key was down, how long after release the text appeared, which pane it landed in, whether it was inserted rather than submitted, and the level reported. Describe the phrase in English; do not reproduce it.

- [ ] **Step 5: Gates, then the pull request**

```bash
cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt --check && python3 scripts/check_manifest.py
```

Open the pull request with `Closes #17`, the checklist from the template, and the description ending in the generated-with line. Do not merge.

---

## Self-review

**Spec coverage.** Section 1 → Task 3. Section 2 → Task 3. Section 2a → Task 3, with its own test for the unreadable repeat. Section 3 → Tasks 4 and 5. Section 4 → Task 2. Section 5 → Task 4, reported in Task 8. Section 6 → Task 2, the backwards-clock test. Section 7 → Task 6. Section 8 → Task 8. Section 9 → Task 1. Section 10 → Task 9. Section 11 → spread across the tasks that make each claim, plus Task 10 for the measured one. Section 12 is what is deliberately absent and needs no task. Section 13's three questions are answered under "Plan decisions" above.

**Reporting exactly once.** `transcribe` reports its own delivery failure and no other, so `end_take` reports the ones it did not — told apart by a returned flag rather than by reading the message. `a_delivery_failure_during_a_hold_is_reported_once_and_not_twice` counts both the toasts and the journal lines, because a defect here is a duplicate rather than an absence and only a count catches it.\n\n**The device-lost case.** `DESIGN_17.md` section 3 requires a release line and a failure line rather than one line claiming the hold ended because the device went, and section 11 requires the test. Task 4 carries both: `end_take` and `discard_take` handle the `Err` branch of `recorder.stop()` through `report_failure`, and `a_device_lost_during_a_hold_gives_a_release_line_and_a_failure_line` asserts the two lines and that neither is the other.\n\n**Criteria coverage.** AC-1 Tasks 3, 4, 11. AC-2 Task 2 and Task 4. AC-3 Task 1. AC-4 Tasks 4, 8. AC-5 Task 3. AC-6 Tasks 4, 5, 8. AC-7 Task 2. AC-8 met by construction — no file is written, stated in Task 2's module comment. AC-9 Task 6, and no code path in this plan ends a process it did not start. AC-10 no lock exists, so nothing to own. AC-11 Task 10. AC-12 Task 6. AC-13 every task. AC-14 Task 9. AC-15 Task 11.

**Type consistency.** `Stamp = u64` throughout. `Hold` fields named identically in Tasks 2, 3, 4 and 5. `Decision`'s four variants used in Task 4 exactly as Task 2 defines them. `Settings { release_ms, min_hold_ms }` matches `config::Ptt`'s field names, which is why the conversion in Task 3 is field-for-field.

**One thing an implementer must not silently change.** The refresh in Task 3 happens before `context::parse`, not after. Moving it for tidiness re-introduces the defect the whole issue exists against, and the test `an_unreadable_repeat_refreshes_the_hold_and_is_still_refused` is the thing that catches it.
