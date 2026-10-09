# DESIGN_18

Changes are in `src/capture.rs` (a third recorder command) and `src/daemon.rs`
(the `cancel` arm of `answer`, plus one helper). Nothing in the manifest, the
client, the wire protocol or the hold mechanism changes.

## Shape

### Recorder: `Cancel`

`Command` gains `Cancel { reply: mpsc::Sender<Cancelled> }`, where

```rust
pub enum Cancelled {
    /// A take was running and has been thrown away. `target` is its pane.
    Discarded { target: String },
    /// Nothing was running.
    NothingRunning,
}
```

`Recorder::cancel(&self) -> Cancelled` sends the command the way `start` and
`stop` do. If the recorder thread is gone it returns `NothingRunning`; a daemon
whose recorder is gone has nothing running either, and `dictate` already reports
that condition itself.

The recorder thread answers it from the same `running` value `Start` and `Stop`
use, in a new `cancel_one(source, running) -> Cancelled`:

1. `running.take()`; `None` gives `NothingRunning`.
2. `source.stop()`.
3. `discard(Some(take))` removes the file if one exists, and the take's samples
   are dropped with the `Running` value. The device failure the sink may have
   recorded is not examined: the take is gone either way.
4. Return `Discarded { target }`.

`cancel_one` never converts, measures or writes audio. `remembered` is not
passed to it.

### Daemon: the `cancel` arm

`"cancel"` calls `cancel(recorder, runtime)`:

The hold guard (`hold_of(runtime)`) is taken first and kept until
`recorder.cancel()` has returned. Steps 1 and 2 happen under it.

1. If the hold state is `Opening`, `Live` or `Ending`, answer
   `Reply::Error("holding for <pane>: a key is being held, and the recording ends on its own when the key comes up")`
   for `Opening` and `Live`, and the `Ending` wording `dictate` uses for `Ending`.
   The three messages are produced by one function shared with `dictate`
   (`hold_refusal(&HoldState) -> Option<String>`), so the two commands cannot
   drift apart. Cancel does not stop a hold; that is #55.
2. Otherwise `recorder.cancel()`:
   - `NothingRunning` answers `Reply::Ok("nothing to cancel")` and publishes
     nothing.
   - `Discarded { target }` publishes `Idle` only if the current activity is
     `Recording` for that same `target`, then answers
     `Reply::Ok("cancelled the recording for <target>; nothing was transcribed or delivered")`.

The guard is released before the publish. The conditional publish exists
because requests are served on separate threads: between the recorder discarding
the take and the publish, a `dictate` may have begun a new take and published
`Recording` for a different pane; an unconditional `Idle` would erase it. The
check and the write happen under the one lock `publish` takes. A `dictate` for
the same pane in that interval is not distinguished: the activity is display
state only (`Activity` is documented as such), so the cost is a wrong label until
the next stage publishes.

Pane names in replies are the pane the take was pinned to, the same as in the
existing `recording for <pane>` reply.

## Decisions

### 1. Where cancel reads its state
- Context: `dictate` learns whether a take runs by asking the recorder thread
  (`Started::AlreadyRunning`); `cancel` is a constant.
- Problem: a second place that tracks "is something recording" (a flag in
  `Runtime`) would be the state the issue says must not exist twice.
- Decision: `cancel` is a recorder command answered on the recorder thread from
  `running`.
- Why: the thread already serialises `Start`, `Stop` and now `Cancel`, so a
  cancel and a toggle that arrive together get one order and cannot both act on
  the same take.

### 2. Cancel during a hold
- Context: a hold's take is in the recorder, but the watcher thread owns when it
  ends and expects `recorder.stop()` to return it.
- Problem: discarding it from `cancel` would leave the hold `Live`; the watcher
  would later call `stop`, get `NothingRunning`, and report a failed take for a
  take the person cancelled. Making that coherent is ending a hold at once, #55.
- Decision: `cancel` refuses while a hold exists, with the message `dictate`
  gives, and does nothing.
- Why: it leaves #55 whole and says so instead of claiming a cancel it did not
  perform.

### 3. What the discarded-take reply says
- Context: the issue asks for a reply that says which take was discarded.
- Problem: the person has to know the audio is gone and nothing was typed.
- Decision: `cancelled the recording for <pane>; nothing was transcribed or delivered`.
- Why: it names the pane and states both consequences; a take has no other
  identifier a person recognises.

### 4. Whether cancel stops a take that is already being transcribed
- Context: after `Stop`, the take is a file in the pipeline, not in the recorder.
- Problem: interrupting transcription is a different mechanism with its own
  cleanup.
- Decision: not done. Cancel then finds the recorder idle and answers
  `nothing to cancel`; the take is delivered.
- Why: the AC bound the change to a take that is recording.

### 5. Cancel and a hold being claimed at the same moment
- Context: `ptt` claims the hold (`Opening`) under the hold guard, releases the
  guard, and only then calls `recorder.start`. A hold check that released its
  guard before `recorder.cancel()` could see `Idle`, then have `ptt` claim the
  hold and start the recorder, then discard the hold's take.
- Problem: the watcher would later find nothing to stop and report a failed take
  for a take the person never cancelled on purpose.
- Decision: the cancel arm keeps the hold guard across the hold check and
  `recorder.cancel()`. A `ptt` that arrives meanwhile waits on the guard, claims
  the hold afterwards, and starts a fresh take.
- Why: the recorder thread never takes the hold guard, so holding it across the
  call cannot deadlock, and the wait is the recorder thread's answer time. The
  guard is also not held while `publish` takes the activity lock, so the two
  locks are never nested.

## Tests

Without a microphone, with the fakes in `tests_support` (`ToneSource`,
`LosingSource`) and `tone_recorder`:

- `capture`: cancel of a running take returns `Discarded` with its pane and the
  next `start` returns `Began`, not `AlreadyRunning`; cancel with nothing running
  returns `NothingRunning`; cancel after a device failure returns `Discarded`
  and the next `start` returns `Began`; the takes directory holds no file after a
  cancel; a `stop` after a cancel returns `NothingRunning`.
- `daemon`: `cancel` with a running take replies with the pane and a `dictate`
  afterwards replies `recording for <pane>`, with nothing delivered; `cancel`
  with nothing running replies `nothing to cancel`; `cancel` during each of the
  three hold states replies with the hold refusal and leaves the take running;
  the indicator is `Idle` after a discard and unchanged after `nothing to cancel`
  and after a refusal; a `Working` activity is not overwritten by a cancel that
  found nothing; a `Recording` for a different pane is not overwritten by a
  discard.
- `daemon`: while a test holds the hold guard, a `cancel` waits and answers after
  it is released (the order is observable with a thread and a channel).
- The existing tests at `src/daemon.rs:3093-3140` stay and pass unchanged; the
  tests that assert `nothing to cancel` through a daemon with no take
  (`src/daemon.rs:3343`) keep passing for the same reason.
