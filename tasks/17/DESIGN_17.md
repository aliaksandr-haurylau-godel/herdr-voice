# DESIGN_17

Push-to-talk: hold a key, speak, release, and the text lands in the pane the key
was held over. Built against `tasks/17/AC_17.md`.

## 0. What this builds

The `ptt` action stops being a stub. Holding a bound key records; the recording
ends one second after the last keypress; the transcript goes through the pipeline
`dictate` already uses and is inserted into the pinned pane, unsubmitted.

Nothing about capture, recognition, bias, rewrite or delivery changes. This
design adds one thing — knowing that a key is still down — and connects it to
machinery that exists.

## 1. Where the hold lives

**Context.** A take today is driven by `dictate` as a toggle. The daemon keeps
no state between the two halves: it asks the recorder to start, and the
recorder's answer — `Started::Began` or `Started::AlreadyRunning` — tells the
daemon which half this was (`src/daemon.rs:158-163`). All per-take state lives
on the recorder thread as locals (`src/capture.rs:197`).

**Problem.** A hold is not a toggle. Nothing in the current arrangement can
answer "when did the last keypress arrive", and the recorder thread is the wrong
place to ask: it owns the audio device and knows nothing about requests.

**Decision.** The daemon gains one piece of state, a `Hold`, guarded by a mutex
and held for as long as a key is down:

```
Hold {
    target: String,        // the pane pinned when the hold began
    cwd: Option<String>,
    agent: Option<String>,
    began: Stamp,          // from the clock seam, §4
    last_poke: Stamp,
    pokes: u32,            // how many repeats arrived; for the journal
}
```

**Why.** A hold is a fact about the arrival of requests, and the daemon is what
receives them. Putting it beside the recorder would mean teaching the audio
thread about the shape of requests, and would put the hold behind the same
channel that serves start and stop.

## 2. One keypress

**Context.** Holding a key starts the client about twelve times a second
(`docs/decisions.md:18`), each time as a whole process that connects, writes a
frame and waits. The client's reply bound for anything that is not `dictate` is
two seconds (`src/client.rs:32-36`).

**Problem.** Any real work performed per repeat is performed twelve times a
second and is wasted eleven of them.

**Decision.** A `ptt` request does one of four things and answers at once:

| state | what happens | reply |
|---|---|---|
| no hold, no take running | the recorder is asked to start; a `Hold` is created | `holding for <pane>` |
| a hold for the same pane | `last_poke` is updated, `pokes` incremented | `holding` |
| a hold for a different pane | `last_poke` is updated; the pane is **not** changed; a journal line names both panes | `holding for <original pane>` |
| a `dictate` take is open | nothing starts | an error naming the open take and how it ends |
| the request cannot be read (§2a) | `last_poke` is updated if a hold is open; nothing else | an error naming what could not be read |

No file is written, nothing is allocated that grows with the hold, and no work
is done on the repeat path beyond taking a mutex and storing a stamp. `ptt`
keeps the two-second client bound, because every reply is immediate.

### 2a. A repeat the daemon cannot read

**Context.** A `ptt` request carries herdr's invocation context, and the daemon
refuses the request before any handler runs when that context will not parse or
names no focused pane — `src/daemon.rs:108-118`, shared with `dictate` through
`needs_target_pane` (`src/daemon.rs:41`).

**Problem.** That refusal is the one signal in this mechanism that the plugin
can fail to read. If a refused repeat leaves `last_poke` untouched, a run of
them lets the deadline expire, and the hold ends because the plugin could not
read its own input — the exact failure the prototype's truncated read produced,
arrived at by another road.

**Decision.** While a hold is open, a `ptt` request refreshes `last_poke`
**before** its context is examined, and is then refused on its own merits. The
refusal is reported to that keypress and written to the journal, naming what
could not be read; the hold continues on the last good pane. With no hold open,
an unreadable request is refused exactly as it is today, because there is
nothing to pin a hold to and nothing to continue.

**Why.** The context answers *where* a take goes, and that question is settled
once at the start of a hold and pinned. It does not answer *whether the key is
down* — the arrival of the request answers that, and a request that arrives is
evidence the key is down whatever its payload says. Reading the invariant the
other way round is what makes an unreadable signal into a release.

**Why.** The target pane is pinned when a take begins and held until delivery —
a decision this repository already took, because a target chosen later follows
the focus while the person is still speaking. The same reasoning makes a repeat
arriving from another pane a continuation of the hold, not a change of target.

## 3. Ending the hold

**Context.** A hold ends when the keypresses stop. No request corresponds to
that moment: the last repeat's reply has already gone out.

**Problem.** Something has to notice the absence of the next repeat, and it
cannot be a request handler.

**Decision.** A watcher thread, started with the daemon and living as long as it
does. It waits until `last_poke + release_ms`, wakes, and re-reads the hold:

- more repeats arrived — recompute the deadline and wait again;
- the deadline has passed — take the hold, clear it, stop the recorder, and run
  the take through the path `dictate` already uses: `take_bias`, `transcribe`,
  `deliver` (`src/daemon.rs:165-178`, `:367`);
- no hold — wait until there is one.

**A hold ends for one of two reasons, and the journal names which.** The
prototype distinguished a stamp that had gone stale from a stamp file that had
disappeared; without a file, nothing can disappear, and what remains is:

| reason | what happened | the take |
|---|---|---|
| released | the deadline passed with no further repeat — the ordinary case | goes through the pipeline |
| daemon stopping | the daemon is shutting down with a hold open | the take is stopped and kept on disk, and the journal names its path |

Only the first is a release. The second ends a hold with no evidence that the
key came up, and the journal line says so in those words, because "the hold
ended" and "the key was released" are different facts and only one of them is
ever inferred.

**A device that dies mid-hold is not a third reason, and cannot be.** The
recorder reports `CaptureError::DeviceLost` from `stop_one`
(`src/capture.rs:347-351`) and from the next `start_one`
(`src/capture.rs:291-299`); the live failure flag, `Sink::failure`
(`src/capture.rs:66`), is private and lives on the recorder thread. Nothing the
watcher can call would tell it mid-hold, and this design does not add such a
call. So a lost device is discovered by the `recorder.stop()` the deadline
already triggers: the hold ended because it was released, and the take then
failed because the device was gone. Those are one reason and one failure, and
the journal writes them as two lines, not as one confused one.

**What that costs, stated rather than discovered.** A device that dies early in
a long hold goes unnoticed until the person stops speaking — they keep talking
into nothing and learn at the end. The alternative is asking the recorder on a
schedule, which means a new query on `Recorder`, a polling interval, and a
second place that decides a hold is over. It is not bought here because nothing
would be done with the answer that this design can do: with no indicator (#40)
there is nowhere to show "the device went away" while a hold is running, and the
pipeline already reports it one release gap later. When #40 exists there is
somewhere to put it, and that is where the query belongs.

The watcher never touches the audio device directly. It calls `recorder.stop()`,
the same call the second half of `dictate` makes.

**Why.** The daemon is resident and already owns a thread for the recorder; a
second one costs nothing and keeps the deadline off the request path. Ending the
take through the existing call means a hold-driven take and a toggle-driven take
are the same take from that point on, and everything already proven about the
pipeline keeps applying.

## 4. The clock seam

**Context.** The release gap is one second and the minimum hold is 300
milliseconds. More than a dozen acceptance criteria turn on those durations.

**Problem.** A test that waits out a real gap costs a real second, and a test
that needs a clock to move backwards cannot use the real one at all.

**Decision.** The watcher and the request handler read time through a small
interface rather than calling `Instant::now()` and `thread::sleep` directly:
a `now()` returning a monotonic stamp, and a `wait_until(deadline)` that can be
woken early when a hold begins or ends. The real implementation wraps `Instant`
and a condition variable; the test implementation moves on command and returns
immediately.

**Why.** Without the seam, the suite either sleeps for real — and a dozen
criteria become a dozen seconds of every run — or the timing is not tested at
all, which is how the prototype's defects survived. It also makes the
backwards-clock case of §6 reachable, which a monotonic clock otherwise makes
impossible to exercise.

## 5. Too short to be a hold

**Context.** A key tapped rather than held produces one repeat, or two.

**Problem.** A tap that silently records and delivers half a word is worse than
one that does nothing; a tap that silently does nothing is indistinguishable
from a broken plugin.

**Decision.** When the watcher ends a hold it measures `began` to `last_poke`.
Below `min_hold_ms` the recording is stopped and discarded without
transcription, and the person is told it was a tap: a journal line always, and a
toast when `[ui] toasts` is on.

**Why.** `CLAUDE.md` rates a silent failure as a defect of the same weight as a
wrong transcript. "Nothing happened" is unactionable; "that was 120 ms, too
short to be a hold" is.

## 6. Impossible durations

**Context.** In the prototype a truncated read produced a hold duration of
about minus 1.79 trillion milliseconds, which fell below the minimum and
discarded a live take. 17 of 22 discarded takes had a negative duration; the
takes beside them lost 13, 34 and 41 seconds of speech.

**Problem.** The rule must hold here even though the cause cannot recur.

**Decision.** The stamps come from a monotonic clock, so `last_poke` cannot
precede `began`, and the truncated-read cause does not exist because no file is
involved — requirement 3 of `AC_17.md` and AC-8 with it, satisfied by
construction rather than by defence. AC-5 is *not* claimed by construction: the
signal this mechanism can fail to read is the request itself, and §2a states
what happens to it. The guard is written
anyway: a duration that is negative or absurd does **not** discard the take. The
take is delivered, and the journal records that the duration was unusable and
that the take was kept. A test drives the fake clock backwards and shows the
take surviving.

**Why.** The defect cost this project real recordings. A guard that costs three
lines and one test is cheaper than re-learning it, and the rule outlives the
mechanism that made it necessary.

## 7. `ptt` beside `dictate`

**Context.** Both drive the same recorder, which answers `AlreadyRunning` to a
second start.

**Problem.** Each can arrive while the other is in progress.

**Decision.** Neither interrupts the other. A `ptt` arriving while a `dictate`
take is open does not start a hold and answers with what is running and how it
ends. A `dictate` arriving during a hold does not end the hold and answers the
same way.

**Why.** Making `dictate` end a hold would be an action that ends a hold
immediately — which is issue #55, deliberately outside this task. Refusing with
an explanation is the behaviour that does not quietly implement a different
feature.

## 8. What the person sees

**Context.** A toggle take reports through the reply to its second invocation:
`delivered to <pane> [<level> dB]`. A hold has no such invocation.

**Problem.** The outcome has to arrive some other way, and the pipeline's
failures have to arrive at all.

**Decision.** Success is silent: the text appearing in the input box is the
message. Every failure produces a journal line, and a toast when `[ui] toasts`
is on — the recorder refusing to start, a device lost mid-hold, a take refused
for level, recognition or delivery failing, and the too-short case of §5. Two
lines are journal-only, because neither is a failure the person has to act on:
the repeat that could not be read while a hold continued (§2a), and the reason
the hold ended (§3). A
poke's reply reports only what that poke did.

**Why.** A notification per dictation is noise for the common case, and the
common case already announces itself where the person is looking. The journal
line is written whether or not the toast is: `[ui] toasts` is a preference about
interruption, not a switch that may turn a failure silent.

## 9. Configuration

A new section, with defaults:

```toml
[ptt]
release_ms = 1000
min_hold_ms = 300
```

`release_ms` is one second: roughly twelve times the 85 ms median repeat
interval measured in `docs/evidence.md`, and about ten times the largest gap
observed at the start of a hold. It is a default chosen from the repeat
measurements and is due to be refined by a measurement of the worst gap under
load, which is why it is configuration rather than a constant.

One existing test has to move. `src/config.rs:361-378`
(`a_key_of_a_later_stage_is_ignored_rather_than_fatal`) proves that an unknown
`[ptt]` section does not make the file fail to load. `[ptt]` stops being
unknown here, so the test must switch to a section that is still in the future,
or it silently stops testing anything.

## 10. Documents

`docs/design.md` section 5 states one second and why, replacing the 250
milliseconds now there; section 7's configuration sketch matches the keys that
exist. `docs/decisions.md` gains one row: the release gap is one second, derived
from the auto-repeat measurements and pending a worst-gap measurement under
load, superseding both the 250 ms in the design and the five seconds in the
ticket.

## 11. Tests

Everything but the live take runs with no microphone, no model and no herdr,
using what already exists: the scripted `Fake` source (`src/capture.rs:439`),
`tests_support::Fake` for transcription (`src/stt.rs:223`), `FakeDeliverer`
recording `Insert`/`Submit`/`Notify` calls in order (`src/delivery.rs:64`), and
the recording journals (`src/daemon.rs:947`). The clock seam of §4 is the only
new double.

What gets covered: a hold shorter and longer than the minimum; a gap inside a
hold that does not end it; the deadline ending it; the pane staying pinned when a
repeat arrives from elsewhere; each of the two collisions in §7; a backwards
clock; a device found dead when the deadline stops the take, which must produce
a release line and a failure line rather than one line claiming the hold ended
because the device went; and the ordering of journal line against delivery call,
which the tracing doubles already make visible.

One thing is measured rather than asserted: that a repeat is served fast enough
to sustain twelve a second. A test issues repeats at that rate against a daemon
with a fake source and records the round-trip, and the number goes into
`docs/evidence.md`.

## 12. What this design does not do

- No indicator. Nothing on screen says a hold is recording; that is #40, and
  this task neither builds it nor depends on it.
- No mid-hold device check. A device that dies during a hold is discovered when
  the deadline stops the take, not while the person is still speaking. §3 says
  why, and why the fix belongs with #40.
- No trimming of the silence the gap leaves at the end of a take (#54).
- No action that ends a hold at once (#55).
- No change to `cancel`, which still stops nothing (#18).
- No keybinding. `setup` is still a stub (#41), so the key is bound by hand.

## 13. Left to the plan

- Whether the watcher is one thread waiting on a condition variable or a thread
  per hold. The first is the obvious shape; the plan decides and says why.
- Where the pipeline runs once the watcher ends a hold — on the watcher thread,
  or handed to another. It takes seconds, and a watcher blocked inside it cannot
  time the next hold.
- Whether `Hold` lives on `Runtime` or beside it, given that `Runtime` is shared
  with request handling.
