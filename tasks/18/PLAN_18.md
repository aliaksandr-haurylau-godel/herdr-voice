# PLAN_18

Four tasks, each test-first. Names and wording are those in `DESIGN_18.md`.

Before each commit run these four commands from the worktree root; all must
succeed with no warnings:

```sh
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt --check
python3 scripts/check_manifest.py
```

And the Windows dead-code check. CI compiles Windows with `-D warnings`, and the
macOS build cannot see an item reachable only from a `#[cfg(unix)]` path. It runs
on a scratch copy, never in the worktree. Clean means clippy exits 0 with no
warning:

```sh
W=$(mktemp -d) && rsync -a --exclude target --exclude .git ./ "$W/" && cd "$W" &&
find src -name '*.rs' -exec sed -i '' \
  -e 's/#\[cfg(all(test, unix))\]/#[cfg(any())]/g' \
  -e 's/#\[cfg(unix)\]/#[cfg(any())]/g' \
  -e 's/#\[cfg(not(unix))\]/#[cfg(not(any()))]/g' \
  -e 's/#\[cfg(windows)\]/#[cfg(not(any()))]/g' {} + &&
CARGO_TARGET_DIR="$W/target" cargo clippy --all-targets -- -D warnings
```

Before each commit also grep every file written for `<new_string>`,
`</new_string>`, `<old_string>`, `</old_string>` and line-start conflict markers.

## Task 1 — recorder `Cancel` (`src/capture.rs`) — depends on nothing

Tests first, in the `tests` module of `src/capture.rs`, using `recorder_with`,
`tone`, `takes_dir`, and `Fake`. Add one new source for the stop count:

```rust
struct CountingSource {
    stops: Arc<std::sync::atomic::AtomicUsize>,
}
impl Source for CountingSource {
    fn start(&mut self, _d: Option<&str>, sink: Sink) -> Result<Format, String> {
        sink.push(Event::Samples(tone(0.3, 0.1)));
        Ok(Format { rate: 48_000, channels: 1 })
    }
    fn stop(&mut self) {
        self.stops.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    }
}
```

Tests (names exact):

1. `cancel_discards_a_running_take_and_names_its_pane` — start for `"w1:p2"`,
   `cancel()` equals `Cancelled::Discarded { target: "w1:p2".into() }`.
2. `a_start_after_a_cancel_begins_a_new_take` — start, cancel, `start` is
   `Started::Began` (not `AlreadyRunning`).
3. `a_stop_after_a_cancel_finds_nothing_running` — start, cancel, `stop()` is
   `Err(CaptureError::NothingRunning)`.
4. `cancel_with_nothing_running_says_so` — `cancel()` equals
   `Cancelled::NothingRunning` on a recorder that never started.
5. `cancel_twice_discards_once` — start, cancel, cancel: second is `NothingRunning`.
6. `cancel_after_the_device_failed_still_discards_the_take` — script
   `[Samples(tone(0.3, 0.1)), Failed("gone".into())]`; start, cancel is
   `Discarded`; the next `start` is `Began`.
7. `cancel_stops_the_source_exactly_once` — a `CountingSource`; start, cancel;
   the counter is 1.
8. `cancel_leaves_no_file_in_the_takes_directory` — start, cancel; the takes
   directory has no `.wav` (a missing directory counts as none).
9. `cancel_never_touches_the_remembered_failure` is not written: `remembered` is
   never set (AC requirement 5), so there is nothing to observe.

Then the code, in `src/capture.rs`:

- `pub enum Cancelled { Discarded { target: String }, NothingRunning }` with
  `#[derive(Debug, PartialEq, Eq)]`, next to `Started`.
- `Command::Cancel { reply: mpsc::Sender<Cancelled> }`.
- In the thread loop: `Command::Cancel { reply } => { let _ = reply.send(cancel_one(source.as_mut(), &mut running)); }`.
- `fn cancel_one(source: &mut dyn Source, running: &mut Option<Running>) -> Cancelled`:
  `let Some(take) = running.take() else { return Cancelled::NothingRunning };`
  `source.stop();` `let target = take.target.clone();` `discard(Some(take));`
  `Cancelled::Discarded { target }`. No sink, format or resample call.
- `pub fn cancel(&self) -> Cancelled` modelled on `stop`: lock, send, `recv`; when
  the lock fails, the send fails or the reply channel is closed, return
  `Cancelled::NothingRunning`.

Done when the eight tests pass and the whole suite is green.

## Task 2 — `hold_refusal` out of `dictate` (`src/daemon.rs`) — depends on nothing

Pure refactor, no behaviour change. Add

```rust
fn hold_refusal(state: &crate::ptt::HoldState) -> Option<String>
```

returning `None` for `Idle`, the "holding for {target}: a key is being held, and
the recording ends on its own when the key comes up" text for `Opening` and
`Live`, and the "holding for {target}: the take that key produced is being
transcribed, and lands in that pane on its own" text for `Ending`: the strings
currently in `dictate` (`src/daemon.rs:274-290`), copied unchanged. In `dictate`,
replace the `match` with
`if let Some(why) = hold_refusal(&hold_of(runtime)) { return Reply::Error(why); }`.

Test first: `hold_refusal_has_one_text_per_state` builds a `Hold` for `"w1:p1"`
(copy the literal from the `ptt` function, `src/daemon.rs:437-445`) and asserts
`None` for `Idle` and the exact text for each of the other three states. The
existing `dictate`-during-a-hold tests must pass unchanged; run
`cargo test dictate` before and after and compare the pass counts.

## Task 3 — conditional idle publish (`src/daemon.rs`) — depends on nothing

```rust
fn publish_idle_if_recording(runtime: &Runtime, target: &str)
```

Takes the activity lock the way `publish` does (poison recovered with
`into_inner`); if the held value is `Activity::Recording { target: t, .. }` with
`t == target`, sets it to `Activity::Idle`; otherwise leaves it.

Tests first (build `Runtime` with `fake_runtime("x")`, set the activity through
`publish`): `idle_is_published_when_recording_for_the_same_pane`,
`a_recording_for_another_pane_is_left_alone`, `a_working_activity_is_left_alone`,
`an_idle_activity_stays_idle`.

## Task 4 — the `cancel` arm (`src/daemon.rs`) — depends on tasks 1, 2 and 3

Tests first (use `tone_recorder("<tag>")`, `fake_runtime`, `request`,
`dictate_request()`, `PANE_1`, `activity_of`, `takes_dir`, `wavs_in`; each test
has its own tag):

1. `cancel_discards_a_running_take_and_names_the_pane` — `dictate`, then `cancel`
   replies `Reply::Ok("cancelled the recording for w1:p2; nothing was transcribed or delivered")`.
2. `a_dictate_after_a_cancel_starts_a_new_take_and_delivers_nothing` — with a
   `FakeDeliverer` that records calls: `dictate`, `cancel`, `dictate`; the second
   `dictate` replies `Reply::Ok("recording for w1:p2")` and the deliverer has
   recorded no delivery.
3. `cancel_with_nothing_running_replies_nothing_to_cancel` — fresh recorder.
4. `cancel_leaves_the_indicator_idle_after_a_discard` — `dictate` (activity is
   `Recording`), `cancel`; `activity_of` is `Activity::Idle`.
5. `cancel_with_nothing_running_does_not_overwrite_working` — `publish` a
   `Working` for `"w1:p9"`, `cancel` with an idle recorder; the activity is
   unchanged.
6. `cancel_leaves_no_wav_behind` — `dictate`, `cancel`; `wavs_in(takes_dir(tag))`
   is empty.
7. `cancel_during_a_hold_is_refused_and_leaves_the_take_running` — as
   `ping_is_answered_while_a_hold_is_open_and_leaves_it_alone` (`ptt` with
   `PANE_1` on `fake_runtime_with_clock`); `cancel` replies
   `Reply::Error("holding for w1:p1: a key is being held, and the recording ends on its own when the key comes up")`;
   the hold is still `Live`, and `recorder.stop()` still returns a take. Repeat
   for `Ending` by writing `HoldState::Ending(hold)` into `runtime.hold` directly
   and asserting the transcribing text.
8. `cancel_waits_for_the_hold_guard` — in a thread scope, lock `runtime.hold`,
   spawn `cancel` in a second thread, assert (after a short sleep of 100 ms) that
   it has not answered, drop the guard, join, and assert its reply.
9. The existing `cancel` tests (`src/daemon.rs:3093-3140`, `3217-3240`, `3343`)
   pass unchanged.

Then the arm:

```rust
"cancel" => (cancel(recorder, runtime), Control::Continue),
```

```rust
fn cancel(recorder: &Recorder, runtime: &Runtime) -> Reply {
    let cancelled = {
        let state = hold_of(runtime);
        if let Some(why) = hold_refusal(&state) {
            return Reply::Error(why);
        }
        recorder.cancel()
    };
    match cancelled {
        Cancelled::NothingRunning => Reply::Ok("nothing to cancel".to_string()),
        Cancelled::Discarded { target } => {
            publish_idle_if_recording(runtime, &target);
            Reply::Ok(format!(
                "cancelled the recording for {target}; nothing was transcribed or delivered"
            ))
        }
    }
}
```

`state` stays in scope across `recorder.cancel()` and is dropped at the end of the
block, before `publish_idle_if_recording`. Import `Cancelled` beside `Started`.
Update the comments at `src/daemon.rs:38-39` only if they became untrue (they say
`cancel` "clears what a dead run left behind"; it does not, because
`remembered` is never set, so reword to "stops whatever is recording").

Done when every test above passes, the whole suite is green, and the four commands
and the Windows check are clean.
