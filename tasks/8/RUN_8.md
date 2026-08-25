# RUN_8

| field | value |
|---|---|
| issue | #8 — Capture: record from a named device at 16 kHz mono, and check the take was not silent |
| input | GitHub issue, read with `gh issue view 8` |
| stage | S4 |
| branch | feat/8-capture |
| opened | 2026-08-24 |

## Stages

<!-- One block per stage, appended, never rewritten. -->

### S1 Assess
- artifact: `AC_8.md`
- produced: 2026-08-24

Two facts were established by running `cpal` 0.18.2 rather than by reading about
it, and both change the work. No input device on this machine offers 16 kHz — all
three report 48 kHz, and the built-in microphone adds 44.1, 88.2 and 96 — so the
conversion the prototype got free from `ffmpeg` has to happen in this plugin. And a
device's name now comes from `Display`; the `name()` method of earlier versions is
gone.

The issue's phrase "a plain subcommand" was resolved to the `dictate` action the
manifest already declares, so that no user-visible name is invented at this stage.

```yaml
gate:
  stage: S1
  artifact: AC_8.md
  reviewer: designer
  verdict: READY
  date: 2026-08-24
  questions: []
  blocker: null
  notes:
    - "Not a gate question, does not withhold READY: the two `cpal` facts in the as-is section are the only claims in the artifact with nothing in the repository behind them. `cpal` is not yet a dependency, the probe crate was deleted, and neither fact reached `docs/evidence.md`, which `CLAUDE.md` says is where hand-verified measurements go. Design is not blocked: the API surface is checkable with `cargo add` at design time, and the resampling requirement stands regardless. Every other as-is claim checked out, including the four `spike/spike.sh` citations at the lines given."
```

The reviewer is right that the two measurements had no home in the repository, and
they now have one: `docs/evidence.md`, section "What the capture library offers",
with the platform they were taken on. The device names of the machine are
deliberately not written there — a list of somebody's audio devices identifies the
machine, and the shape of the finding does not need them.

### S2 Design
- artifact: `DESIGN_8.md`
- produced: 2026-08-25

Four decisions, and two of them are the ones that matter.

The recorder owns a thread for the daemon's whole life, and the `cpal` stream is
built, held and dropped there and nowhere else. A take spans two `dictate`
invocations, each arriving on a different connection handled on a different thread,
so the stream cannot live in a connection handler. Owning a thread also makes the
question of whether a `cpal` stream can move between threads irrelevant — the crate
does not state it plainly for every platform, and this way it never has to.

The rate conversion is restricted to integer ratios: the device is opened at 32,
48, 64 or 96 kHz, preferring 48, and a windowed-sinc low-pass at 7.5 kHz runs
before every *n*-th sample is taken. A device offering only 44.1 kHz is refused by
name rather than recorded badly. That turns a rational resampler into a filter and
a stride, and the low-pass is not optional: without it everything above 8 kHz folds
back into the band and goes straight into recognition.

Decisions taken here without the owner are also in `docs/decisions.md`.

```yaml
gate:
  stage: S2
  artifact: DESIGN_8.md
  reviewer: planner
  verdict: QUESTIONS
  date: 2026-08-25
  questions:
    - "AC-13 has three clauses and one of them is undecided: a device that disappears mid-take. Section 8 maps AC-13 to sections 1 and 2, but section 1 only covers name-to-choice at Start and section 2 only covers Start-while-running and Stop-with-nothing; nothing says what happens when the stream fails between the two `dictate` invocations. To cut a task with a done-criterion somebody else can check I would have to take three decisions the design does not: whether the take ends at the moment of failure or at the next Stop, whether the partial audio and its file are kept or removed, and — the one that spreads — whether the sample-source trait of section 6 carries an error at all. Section 6 describes the fake as one that 'yields a prepared buffer' and its test list stops at stop-with-nothing, so as written the trait has no error path and the failure cannot be tested without a microphone, which R9 and AC-6/AC-11 require of everything else. That trait is the interface between the `capture` task and the `audio::*` tasks; I cannot write those tasks independently while its shape depends on an undecided error path."
  blocker: null
  notes:
    - "Not a gate question: `cpal` is not yet in `Cargo.toml`, and on `ubuntu-latest` it pulls `alsa-sys`, which needs the ALSA development headers present before anything builds. The workflow installs no system packages today, so AC-14 will need a step the design does not mention. Plannable without an answer — the done-criterion is CI green."
    - "Not a gate question: no section says when the daemon reads `[audio]`. I will plan it as read once at daemon start and passed to the recorder thread; if the author meant per-take re-reading, say so and the task changes cheaply."
    - "Everything else planned cleanly: `<state>` resolves against the existing `transport::state_directory`, the module table gives four independent tasks then `capture` then `config` and `daemon`, and the integer-ratio restriction is a decision with a reason rather than a gap."
```

### S2 Design — answers to the gate
- artifact: `DESIGN_8.md`, new section 2a and a revised section 6
- produced: 2026-08-25

The question was right and the answer is three decisions, now in the artifact.

The recorder's source of samples carries failures as well as samples, in both
implementations, so a device that goes away is testable without a microphone — this
is the one that decides the interface, and leaving it open would have forced the
`capture` task to invent it.

A take whose device fails ends at the failure, its partial audio is discarded and
its file removed, and the reason is reported at the next `dictate`. Reporting there
is not a chosen delay: it is the first moment there is anywhere to report to,
because the indicator belongs to another issue. Keeping the partial audio was
rejected — a take that lost its device halfway is the same class of input as one
from the wrong device, and it produces a confident transcript of nothing.

Both notes are answered in the artifact too: the configuration is read once at
daemon start and handed to the recorder thread, and the check workflow gains a step
installing the ALSA development headers on `ubuntu-latest`, without which nothing
builds on Linux at all.

A revised artifact needs a new verdict; the gate runs again.

```yaml
gate:
  stage: S2
  artifact: DESIGN_8.md
  reviewer: planner
  verdict: READY
  date: 2026-08-25
  questions: []
  blocker: null
  notes:
    - "The gate question is answered and the three places that had to agree now do. Section 2a makes the sample source event-carrying — a block of samples or a failure with its reason — and fixes the two consequences. Section 6 states the same interface with the same two events and both implementations, and its `capture` row now carries `a source that fails mid-take`. Section 8 maps AC-13 to 1, 2 and 2a, which is exactly its three clauses. The four `audio::*` tasks and the `capture` task can now be written independently, which is what was blocking."
    - "Not a gate question: section 2a says the next `dictate` answers with the remembered reason and clears it, without saying whether that same invocation also starts a fresh take; I will plan it as report-and-clear only. And AC-8 says the mean volume is computed and reported while section 4 only states it is named in a refusal; I will plan the accepted-take reply to carry the path and the measured level."
    - "Both earlier notes are closed in the artifact. Dependency graph comes out unambiguous: `audio::device`, `audio::level`, `audio::resample`, `audio::wav` in parallel, then `capture`, then `config` and `daemon`, with the CI step independent."
```

Both of the reviewer's readings are adopted rather than left to the plan's
discretion, and they are what the plan will say: the `dictate` that reports a
remembered failure only reports and clears it, and the reply for an accepted take
carries the path and the measured level.

### S3 Plan — a reversal withdrawn

The plan's task 7 proposed taking `dictate` out of `needs_target_pane`, on the
argument that a take needs no pane because the pane matters at delivery. That
argument is incomplete and the proposal is withdrawn.

The pane is needed while recording, for two reasons. The indicator of
`docs/design.md` section 6 blinks in the sidebar token of an agent's row and in the
tab label, and it is drawn on a specific pane from the moment recording starts.
And a target chosen at the end follows the focus: somebody speaks looking at one
agent, switches while thinking, and the text lands in another. The prototype pins
the target at the start and keeps it in the run's state for exactly that reason.

So `dictate` stays where issue 3 put it. The target is pinned when a take begins
and held until delivery, and a take does not start when no pane can be determined —
losing a take is worse than nothing, and better than delivering it to the wrong
agent.

```yaml
gate:
  stage: S3
  artifact: PLAN_8.md
  reviewer: implementer
  verdict: READY
  date: 2026-08-25
  questions: []
  blocker: null
  notes:
    - "Not a gate question: Task 5 gives `stop()` a full signature but never states `start()`'s return type, while Task 7 needs `start()` to distinguish three outcomes — a fresh take began, a take is already running, and a remembered mid-take failure to report and clear without starting. The behaviour is fully determined by DESIGN_8 section 2a, so it is a signature I choose, not a decision I invent."
    - "Not a gate question: adding an `f32` field to `Config` breaks its `#[derive(Eq)]`. One line to fix, and the compiler names it immediately."
    - "Checked against the code: `transport::state_directory` exists and matches what section 5 relies on; `needs_target_pane`, the `IMPLEMENTED` guard test and `answer`'s dispatch are where task 7 says; the manifest needs no change since `dictate` is already declared; the workflow has no ALSA step, matching task 0's premise. Every module tasks 1 to 6 name is new work."
    - "The gap the S2 gate raised is resolved in the artifact I read, and nothing in the plan reopens it."
```

### S3 Plan — revised after the verdict
- artifact: `PLAN_8.md`, tasks 5, 6 and 7
- produced: 2026-08-25

Three changes, none of which grows the work.

Task 7 no longer proposes taking `dictate` out of `needs_target_pane`; the reason
is above, under "a reversal withdrawn". The task gets smaller: the guard test in
`src/main.rs` stays as it is.

Task 5 now states `start`'s return type, which the reviewer said it would otherwise
choose while writing: three outcomes, because task 7 has to tell them apart. The
target pane is passed in at `start` and kept with the take, which is what pinning
means in code.

Task 6 records that `f32` costs `Config` its `Eq` derive.

The verdict above was given on the version before these changes. Task 0 — the ALSA
step in CI — is untouched by all three and is independent of every other task, so
it proceeds while the gate runs again on the revised plan. Nothing else starts
before that verdict.

### S4 Implement — task 0
- artifact: `.github/workflows/check.yml`
- produced: 2026-08-25
- verification: the `ubuntu-latest` job ran the step and installed
  `libasound2-dev` 1.2.11-1ubuntu0.3; all five checks green on the branch

Done before the dependency it exists for, so that the Linux job never goes red for
a reason unrelated to the change that turned it red.

### S4 Implement — tasks 1 to 8
- artifact: `src/audio/{level,wav,device,resample}.rs`, `src/capture.rs`,
  `src/capture/cpal_source.rs`, `[audio]` in `src/config.rs`, the `dictate` wiring
  in `src/daemon.rs` and `src/main.rs`
- produced: 2026-08-25
- verification: 93 tests, clippy, format and the manifest check clean; hand
  verification in `docs/evidence.md`, "Capture, by hand on macOS"

Two defects were found by running it, and neither could have been found by reading
it.

**The configured device name never reached the device.** It was a per-call
argument and the daemon passed nothing, so `[audio] input` was ignored and every
take came from the default input — silently, which is the precise failure that
selecting by name exists to prevent. A configured name that matched no device
started recording quite happily. The recorder now takes the name from the
configuration it holds, and a test asserts the name reaches the source; it fails
against the old behaviour, which was checked.

**A successful take printed nothing.** The daemon answered with the path and the
level; the client kept only failures and discarded everything else, so a take that
worked looked exactly like a take that did nothing.

One reading was wrong and is corrected in the evidence rather than quietly
dropped: a take of digital silence from the default input was attributed to a
denied microphone permission, and the built-in microphone then returned −62.9 dB
of ordinary room tone from the same code. The default input on this machine is a
USB interface that hears nothing. The refusal message still names permission,
because on macOS a denied permission also delivers silence and the two cannot be
told apart from inside the process.

Not verified: a take containing speech. Nobody spoke into a microphone at three in
the morning, so what is proven is that a take is recorded, converted, measured and
refused correctly — not that words survive the trip.

## Notes

The probe used to establish the `cpal` facts was a throwaway crate outside the
repository, and was deleted.
