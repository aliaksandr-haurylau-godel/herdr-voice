# RUN_17

Push-to-talk: holding a key records, releasing it stops, and the text lands in
the pane the key was held over.

Input: GitHub issue #17, read with `gh issue view 17`. The issue body and its one
comment are the ticket; there is no other tracker. The comment carries the
release gap the plugin must use.

Run root: `tasks/17/`. Branch `feat/17-push-to-talk`, cut from `main` at
`b8a07a8`.

## Stages

### S1 Assess

Scope settled before the criteria were written, because it changes what they
have to cover: **push-to-talk only**. The issue's comment names two mitigations
for the five-second tail — trimming trailing silence before transcription, and
an explicit key that ends a hold at once — and says they are worth considering
in this task. They are not in it. Each becomes its own issue, so that the task
that closes the hold-to-talk loop is not held open by an audio-processing change
and by a new user-visible name.

Two things the issue leaves in conflict, and how this stage resolves them:

**The release gap is five seconds, not 250 milliseconds.** `docs/design.md`
section 5 states 250 ms, three times the measured 85 ms repeat interval. The
issue's comment supersedes it: a gap in the poke stream is not proof of release,
repeat delivery stutters under load, and a hold observed splitting in two cost a
take. `docs/design.md` is corrected by this task rather than contradicted by it.

**Everything the five-second gap costs is now visible to the person, or it is
not.** A hold carries up to five seconds of tail, and during that tail the
recording is still running with nothing on screen saying so — the indicator of
`docs/design.md` section 6 does not exist, and is issue #40. This task does not
build the indicator, and must not depend on it; it must also not make its absence
worse silently.

Artifact: `tasks/17/AC_17.md`, 15 criteria.

Gate, round 1:

```yaml
gate:
  stage: S1
  artifact: AC_17.md
  reviewer: designer
  verdict: READY
  date: 2026-09-14
  blocker: null
```

Closed on the first round. Two conflicts in the ticket were resolved by this
stage rather than passed on: the release gap is five seconds and `docs/design.md`
is corrected by this task, and the ticket's file-and-watchdog vocabulary is
recorded as the prototype's architecture rather than as a requirement, with the
invariant underneath it stated so that it survives whichever mechanism design
chooses — a signal the plugin could not read is never evidence that the key was
released.

Two exclusions became issues rather than criteria: #54, trimming the silence the
five-second gap puts at the end of every take, and #55, an action that ends a
hold at once.

### S2 Design

Artifact: `tasks/17/DESIGN_17.md`.

Two decisions were taken before it was written, both of which change what the
ticket says:

**The mechanism is in memory, with no poke file.** The ticket's rules are
written in the vocabulary of a shell prototype that had no resident process: a
file the client writes, a watchdog that polls it, a lock. This plugin has a
daemon that already receives every keypress as a socket request. Holding the
stamps in the daemon removes the truncated-read failure entirely rather than
defending against it, and the rules about atomic writes and unreadable stamps
are satisfied by construction. The invariant underneath them is what carries
over and is written into the design: a signal the plugin could not read is never
evidence that the key was released.

**The release gap is one second, not five.** The five seconds in the ticket's
comment were not derived from the auto-repeat measurements; they came from one
observation that a hold split in two on a loaded machine. One second is roughly
twelve times the 85 ms median repeat interval in `docs/evidence.md` and about
ten times the largest gap observed at the start of a hold, so it survives a
stall an order of magnitude worse than anything measured. The cost of a gap that
is too long is linear and predictable — a tail on every take, later text, more
silence into recognition. The cost of one that is too short is a hold split in
two, whose second fragment can fall below the minimum hold and be discarded as a
tap, which is speech lost rather than merely divided. The number is due to be
refined by measuring the worst gap under load, which is why it is a
configuration default rather than a constant.

Gate, round 1:

```yaml
gate:
  stage: S2
  artifact: DESIGN_17.md
  reviewer: planner
  verdict: QUESTIONS
  date: 2026-09-14
  questions:
    - "AC-5 has no section. The in-memory mechanism does have a signal the
       plugin can fail to read: a `ptt` request whose invocation context will
       not parse. It is refused at `src/daemon.rs:108-118` before any handler
       runs, so under §2 and §3 as written a run of unreadable repeats lets the
       deadline expire and ends the hold."
    - "AC-6 is not answered. §3 has a single end path, and nothing distinguishes
       a signal that went stale from one that disappeared, nor declares the two
       collapsed."
  note: "§6 cited requirement 7 of AC_17.md, the lock rule, where it meant
         requirement 3 and AC-8, the file rule."
```

Both questions were answered in the artifact rather than escalated; both follow
from rules already settled.

**The first was a real defect in the design, not a gap in its prose.** The claim
that a mechanism without a file has no unreadable signal was wrong. The request
itself is one: it carries herdr's invocation context, and a context that will not
parse or that names no focused pane is refused before any handler runs. Under the
design as first written, such a repeat left `last_poke` untouched, so a run of
them ended the hold — the prototype's failure reached by another road. The answer
separates the two questions the request answers: the context says *where* a take
goes, which is settled once and pinned at the start; the arrival of the request
says *whether the key is down*, and that is true whatever the payload contains.
So a repeat refreshes the stamp before its context is examined, and is then
refused on its own merits.

**The second turned out to have a real answer rather than a collapse.** Without a
file there is no stamp to disappear, but a hold still ends for more than one
reason, and the journal names which. The first answer written here named three,
including the recorder losing the device mid-hold; round 2 showed that reason
does not exist, and the correction is below.


Gate, round 2:

```yaml
gate:
  stage: S2
  artifact: DESIGN_17.md
  reviewer: planner
  verdict: QUESTIONS
  date: 2026-09-14
  questions:
    - "§3 names `device lost` as a reason a hold ends, and §11 asks for a test
       of it, but the recorder cannot report anything mid-hold:
       `CaptureError::DeviceLost` is produced only in `start_one`
       (`src/capture.rs:291-299`) and `stop_one` (`:347-351`), and `Sink::failure`
       (`:66`) is private and lives on the recorder thread. Either `Recorder`
       gains a query and the watcher checks it on a schedule the design does not
       state, or device loss is discovered only by the `recorder.stop()` the
       deadline already triggers — in which case one wake matches two rows of the
       table at once and the journal line is ambiguous."
  blocker: null
```

**The second real defect in two rounds, and the same kind as the first: a path I
assumed existed and had not checked.** Removing the file removed one way to fail
to read a signal, and I did not notice the second; here I described the recorder
telling the watcher something it has no way to say.

The correction takes the second reading. A hold ends for **two** reasons — the
deadline passed, which is the only release, and the daemon stopping. A device
that dies mid-hold is not a reason and is not detectable by the watcher; it is
discovered by the `recorder.stop()` the deadline already triggers, so the hold
ended because it was released and the take then failed because the device was
gone. Two journal lines, not one confused one.

No query was added to `Recorder` and no polling interval introduced. What that
costs is written into the design rather than left to be discovered: a device
dying early in a long hold goes unnoticed until the person stops speaking. It is
not bought here because there is nowhere to show it while a hold runs — the
indicator is #40 — and the pipeline reports it one release gap later anyway.

Gate, round 3:

```yaml
gate:
  stage: S2
  artifact: DESIGN_17.md
  reviewer: planner
  verdict: READY
  date: 2026-09-14
  blocker: null
```

Every code citation in the design was verified against the worktree by the
reviewer. Two notes carried forward to the plan, neither blocking: `§8` still
lists a device lost mid-hold among the failures that produce a journal line and
a toast, which §3 and §11 make unambiguous as the failure written when
`recorder.stop()` returns `DeviceLost` at the deadline; and the design does not
name how the watcher learns the daemon is stopping — `serve` keeps its stop flag
as a local `Arc<AtomicBool>` (`src/daemon.rs:577`) and returns as soon as the
accept loop breaks, so the plan decides both the signal and that `serve` does not
return before the watcher has stopped and kept the take.

### S3 Plan

Artifact: `tasks/17/PLAN_17.md`, 11 tasks.

Six rounds. The verdicts, in order: QUESTIONS with five findings, then QUESTIONS
with two, then one, one, one, then READY.

```yaml
gate: {stage: S3, artifact: PLAN_17.md, reviewer: implementer, verdict: QUESTIONS, round: 1, date: 2026-09-14,
  findings: ["clock_for_tests given as a comment, with no field named to store the concrete clock",
             "FakeDeliverer::new(calls) does not exist; runtime_with takes the fake unboxed, not boxed",
             "RecordingJournal::new() does not exist; the helper derives Default",
             "three tests left as comments describing assertions rather than as code",
             "request_for is not the helper's name; it is request(command, &[u8])"]}
gate: {stage: S3, artifact: PLAN_17.md, reviewer: implementer, verdict: QUESTIONS, round: 2, date: 2026-09-14,
  findings: ["TestClock::wait_until could not be freed by wake(), so the watcher's hour-long idle wait would hang watcher.join() rather than fail",
             "unreadable_repeat_line declared in Task 8 with no call site and no test"]}
gate: {stage: S3, artifact: PLAN_17.md, reviewer: implementer, verdict: QUESTIONS, round: 3, date: 2026-09-14,
  findings: ["end_take and discard_take given as prose; the Err branch of recorder.stop() — the device-lost case the design names — had no code, no line wording and no test"]}
gate: {stage: S3, artifact: PLAN_17.md, reviewer: implementer, verdict: QUESTIONS, round: 4, date: 2026-09-14,
  findings: ["end_take routed transcribe's Reply::Error through report_failure, double-reporting the delivery failure transcribe already journals and toasts"]}
gate: {stage: S3, artifact: PLAN_17.md, reviewer: implementer, verdict: QUESTIONS, round: 5, date: 2026-09-14,
  findings: ["the plan named dictate as transcribe's only other call site; an existing test at src/daemon.rs:935 calls it directly and the tuple return breaks compilation"]}
gate: {stage: S3, artifact: PLAN_17.md, reviewer: implementer, verdict: READY, round: 6, date: 2026-09-14, blocker: null}
```

**The fix for round 4 is worth stating, because the obvious answer was wrong.**
`transcribe` reports its own failure in exactly one of its four return paths —
the delivery branch, which writes `delivery_failed_line` and raises a toast.
Recognition being unavailable and recognition failing each return a
`Reply::Error` and write nothing, which is right for the toggle, where the
client prints the reply. A hold has no reply reader: its last keypress was
answered a second before the take was even stopped. So removing the report
would have made two failures out of three silent, and keeping it doubled the
third. `transcribe` now returns whether it reported, the watcher announces only
what was not announced, and the test counts toasts and journal lines rather than
asserting their presence — the defect here is a duplicate, and only a count
finds it.

**What five of the six findings had in common.** Each was a place where the plan
asserted something instead of checking it: that a helper existed, that a wake
would free a waiter, that a declared function had a caller, that one failure
would produce one message, that a call site was the only one. None was
carelessness in the sense of haste; each was something that seemed too obvious
to verify. That is the shape of the thing: the claims worth checking are the
ones confident enough not to have been checked.

### S4 review of the implementation

Two independent reviews read the finished implementation. Nine findings, all
accepted; three were defects in behaviour, four were tests that could not fail,
and two were smaller. What changed:

**The hold had two states and its life has four.** `watch` cleared the hold and
only then stopped the recorder and ran the pipeline — seconds of work — and `ptt`
assigned the hold only after the blocking `recorder.start` returned. So there
were two windows in which the state said nothing was happening while a recording
existed or was being made. Three defects came out of them: a `dictate` landing in
the second window took the hold's own take, delivered it, and left the watcher to
journal and toast that the take had failed; a `ptt` landing there started a
second recording that the single watcher could not time until the first pipeline
returned; and every repeat arriving during the device open — twelve a second
against an open that takes hundreds of milliseconds — was refused with "a take is
already recording for another action", naming a cause that did not exist. The
state now covers `Idle`, `Opening`, `Live` and `Ending`; sections 1, 2, 3 and 7
of `DESIGN_17.md` say what each does. A `ptt` arriving during `Ending` is refused
rather than queued, for the reason section 7 gives.

**Four mutations stayed green, so four tests were missing.** Gutting the branch
that announces what `transcribe` did not — recognition unavailable, recognition
failed — left the suite green, and so did making those two branches claim they
had already reported; that branch is the whole of what the person gets when
`[stt] engine` is misconfigured. "The take is kept" on shutdown was proved by a
journal line naming a path, not by the file being there. A tap's recording was
never proved removed. And release-over-time was covered by the pure function
alone, with no daemon-level test running a watcher against repeats spread over
time.

**Four tests waited on the wrong thing.** They waited for the hold to clear and
then asserted on a journal line, a toast or a delivery that, under the code of
the time, came after it. One reviewer saw the suite go red about one run in
twelve; injecting 300 ms into that window failed all four every time, which was
reproduced here before the change. Each now waits for what it asserts on.

**The poisoned-lock policy was inconsistent across four sites**, and in `ptt` a
poisoned lock after `Started::Began` would have left the recorder running with
nothing to stop it and the watcher waiting an hour. All four recover with
`into_inner()`. Poison stays unreachable: nothing fallible runs under that mutex.

Every fix was driven by the test that failed first. The three window defects are
reproduced without a sleep: a source that blocks inside `start` and an engine
that blocks inside `transcribe` turn each window into two events the test
controls.

```yaml
review: {stage: S4, artifact: code, reviewers: 2, findings: 9, accepted: 9, date: 2026-09-14}
```
