# DESIGN_22 — delivery

Covers `AC_22.md` in full. Decides six things: where delivery is called from, how
the pinned pane's agent reaches it, what the client is told on success and on
failure, how the outward call to herdr is made and faked in tests, how a failed
delivery is reported, and what happens to the text and the audio when delivery
fails.

## 1 Where delivery is called from

**Context.** `dictate` (`src/daemon.rs:91-101`) starts a take on the first
invocation and, on the second, stops the recorder and calls `transcribe`
(`src/daemon.rs:104-126`) with the finished `Take`. `transcribe` calls the
recognition engine and builds a `Reply` from the text; nothing after that touches
the text again. The `Take` returned by capture already carries `target`, the pane
pinned when the take began (`src/capture.rs:95-101`).

**Problem.** AC-2 requires that a finished take reach delivery from the running
daemon, not only from a test. `transcribe` is the one place that holds a
successfully transcribed string and the pinned pane at the same time; nothing else
in the call chain does.

**Decision.** `transcribe` calls delivery itself, immediately after a successful
`engine.transcribe(&take.path)` and before building its reply. The reply it builds
changes shape as a result (section 3): on a successful delivery it no longer
carries the text, since the text is now in the pane; on a failed one it carries the
text and the failure reason, since delivery is the only place besides the pane
itself the text could have gone. A failure from the engine, as today, still ends
in the existing "the take is kept at …" reply — delivery is never reached when
there is no text to deliver.

**Why.** Delivery is a step of the pipeline this take is running, not a separate
feature reached by its own command; a module with no caller does not put text in
anyone's input, which is the stated goal of the issue. Calling it from `transcribe`
keeps that stated in one place, next to the only two things delivery needs: the
text and the take.

## 2 Carrying the pinned agent to delivery

**Context.** `Invocation.focused_pane_agent: Option<String>` (`src/context.rs:14`)
is read once, when `context::parse` runs inside `answer` on the first `dictate` of
a take (`src/daemon.rs:59`). `answer` currently keeps only the pane string out of
that invocation before calling `dictate` (`src/daemon.rs:70-72`), and `Take` holds
only `path`, `level_dbfs` and `target` (`src/capture.rs:95-101`). By the second
`dictate`, which is where delivery runs, the agent name is gone.

**Problem.** AC-4 needs the agent name that was true when the pane was pinned, not
one looked up again at delivery time — the same reasoning that already pins the
target pane itself (`docs/decisions.md`, "`dictate` stays in
`needs_target_pane`…"). A second herdr call at delivery time would also
reintroduce the race the pinned target already exists to close: the pane a person
was looking at when they started speaking, not whichever one has focus once they
finish.

**Decision.** The agent name is carried exactly where the pane is carried, all the
way through:

- `answer` passes `invocation.focused_pane_agent.as_deref()` into `dictate`
  alongside `pane`, at the same call site that already extracts `pane`
  (`src/daemon.rs:70-72`).
- `dictate`'s signature gains `agent: Option<&str>` and passes it to
  `Recorder::start`.
- `Recorder::start(&self, target: &str)` (`src/capture.rs:215`) gains a second
  parameter, `agent: Option<&str>`; `Command::Start` (`src/capture.rs:149-160`)
  and `Running` (`src/capture.rs:166`) each gain an `agent: Option<String>` field
  next to the `target: String` they already carry; `start_one`
  (`src/capture.rs:249`) stores it into `Running` exactly as it stores `target`.
- `Take` (`src/capture.rs:95-101`) gains `pub agent: Option<String>`, filled from
  `Running.agent` in `stop_one` (`src/capture.rs:305`) next to where `target` is
  filled today.

When the field is absent — no agent named at pin time, or an invocation herdr sent
without it — `Take.agent` is `None`, and delivery reads that as "this pane has no
agent to submit to" (AC-4's fallback, decided in section 4).

**Why.** The field already exists and is already read at the moment that matters;
the only gap is that nothing keeps it past that moment. Threading it beside
`target` costs one field in three structs already built for exactly this shape,
and it needs no new call to herdr — the reviewer's note when S1 closed READY
already established that this is possible without one
(`tasks/22/RUN_22.md:123-128`).

## 3 What the client is told

**Context.** `dictate`'s reply today embeds the transcript in both branches of
`transcribe` (`src/daemon.rs:117-124`): `"{text} [{level_dbfs:.1} dB, {target}]"`
on success, `"{why} — the take is kept at {path}"` on failure. `Reply::write_to`
writes one line (`src/proto.rs:158-165`) and `Reply::read_from` reads one line
(`src/proto.rs:167-177`); a reply with an embedded newline is truncated silently —
measured in `docs/evidence.md`, "a reply with a newline in it is truncated
silently", and filed as issue #19.

**Problem.** R11 and R12 change what each branch says once delivery, not
`transcribe`, is the thing that decides success or failure: on success the text
already reached the pane, so repeating it is noise; on failure it reached neither
the pane nor the client unless the reply itself carries it. Whatever the wording
is, it has to stay one line, since #19 is not this design's to fix.

**Decision.**

- **Success:** `format!("delivered to {target} [{level_dbfs:.1} dB]")` — for
  example `delivered to w1:p2 [-46.9 dB]`. It names the pane and the measured
  level, the two things AC-11 requires, and nothing else.
- **Failure:** `format!("could not deliver to {target} ({why}); paste the text in \
  by hand: {text} — the take is kept at {path}")` — for example `could not deliver
  to w99:p99 (pane_not_found: pane w99:p99 not found); paste the text in by hand:
  fix the worklog entry — the take is kept at
  /state/takes/1234-5678-9.wav`. It carries the full text and the reason (AC-12),
  and it names what to do next (CLAUDE.md, "Rules for the code"): paste the text by
  hand, and the audio is not lost either.

Both strings are built from values delivery itself controls or receives — the pane
name, the level, `why` from the rejected herdr call, and the transcript. Two of
those are outside this design's control: the transcript is whatever recognition
returned, and `why` is whatever herdr printed. Neither is expected to contain a
newline in ordinary operation, and this design does not add a defense against one
that does; a multi-line transcript or a multi-line herdr error would be truncated
by the same mechanism #19 already names, not by anything new here. Delivery does
not depend on a reply carrying more than herdr's one line ever tests round-trip.

**Why.** The wording is left to design by AC_22.md itself ("Out of scope /
noticed"); what is fixed is what each string must convey, and both satisfy that
without assuming the newline bug is fixed underneath them.

## 4 How the outward call is made and tested

**Context.** There is no herdr client in `src/`; `src/transport.rs` is the
plugin's own socket to its own daemon, unrelated to herdr's command line.
`src/doctor.rs:97-108` already shells out to the `herdr` binary for a version
check, naming it through `HERDR_BIN_PATH` with `"herdr"` as the fallback — the same
problem delivery has (a program that must be found and run, with a distinguishable
"not found" from "found but refused"). `src/stt/command.rs` transcribes by running
an external program behind a small `Engine` trait, with a `CommandEngine` real
implementation and a fake used by every test that does not need a live
transcriber (`src/stt/command.rs:1-115`, `src/daemon.rs:265-269`).

**Problem.** AC-14 needs the insert path, the submit path, the fallback, and a
rejected call, each testable without a live herdr. AC-6 needs a rejected call —
including the pane-gone case measured in `docs/evidence.md` — treated as a failed
delivery with no existence check first.

**Decision.** A new module, `src/delivery.rs`, mirrors the shape `src/stt.rs`
already uses for the same problem:

```rust
pub trait Deliverer: Send + Sync {
    fn insert(&self, pane: &str, text: &str) -> Result<(), DeliveryError>;
    fn submit(&self, pane: &str, text: &str) -> Result<(), DeliveryError>;
    fn notify(&self, title: &str, body: &str) -> Result<(), DeliveryError>;
}
```

`insert` runs `herdr pane send-text <pane> <text>`; `submit` runs `herdr agent
prompt <pane> <text>`; `notify` runs `herdr notification show <title> --body
<body>` — the same call the prototype makes in `note()`
(`spike/spike.sh:96`). One trait carries all three because all three are "run
herdr and read whether it refused," and one fake covers the whole surface AC-14
and AC-8 need.

The real implementation, `HerdrDeliverer`, is built once, at daemon start, from
`HERDR_BIN_PATH` with `"herdr"` as the fallback — the same rule `doctor.rs`
already uses (`src/doctor.rs:105`). Each method runs `std::process::Command`,
checks `output.status.success()`, and on failure reads whatever herdr printed
(stdout or stderr, whichever is non-empty) into `DeliveryError::Rejected`,
extracting the JSON `"code"` field when the text parses as the shape measured in
`docs/evidence.md` (`pane_not_found`, `agent_not_found`, …) and falling back to the
raw text otherwise. `Command::new` itself failing to spawn — herdr not on the
`PATH` this process has — is `DeliveryError::NotFound`, on the same reasoning
`CommandError::NotFound` already states for the transcriber
(`src/stt/command.rs:70-79`): herdr starts plugin commands with a minimal `PATH`,
so a program that works in a shell can still be absent here.

A free function carries the branching AC-3 and AC-4 state, independent of which
`Deliverer` it is given:

```rust
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
```

`submit` comes from `[delivery] submit` (section 7); `agent` from `Take.agent`
(section 2). No existence check runs before either call — AC-6 already gives
delivery a single failure mode, whatever herdr's reason, and the pane-gone case is
one instance of it rather than a second check.

`daemon::start` builds one `HerdrDeliverer` and the `[delivery]`/`[ui]` settings
once, the same moment it resolves `Recognition` (`src/daemon.rs:175-178`), and
hands them to `serve` the same way — `Arc`, threaded through `serve_one`,
`answer`, `dictate`, into `transcribe`, which is where `delivery::deliver` is
finally called (section 1).

Tests exercise `deliver` and the failure/toast paths against a `FakeDeliverer` in
`delivery::tests_support`, built the way `stt::tests_support::Fake` is
(`src/stt.rs:168` area): a struct that records which method was called with which
arguments, and can be told in advance to return `Ok` or a given `DeliveryError`.
That fake proves, with no live herdr:

- **insert-only** — `submit = false` calls `insert`, never `submit`.
- **submit** — `submit = true` and `agent = Some(_)` calls `submit`.
- **submit falls back to insert** — `submit = true` and `agent = None` calls
  `insert`.
- **a rejected call** — the fake returns `DeliveryError::Rejected`, and `deliver`
  propagates it unchanged; no dedicated "pane does not exist" fake is needed, per
  AC-14's own note, since the fake's `Err` stands for any rejected call including
  that one.

A second layer, in `daemon.rs`'s own tests, wires the fake through `answer` and
`transcribe` to prove AC-2 (a finished take really reaches delivery, not only a
unit test of `deliver`), AC-9 (the journal line appears before the fake is
called), and AC-11/AC-12 (the reply's shape on each outcome).

**Why.** The same shape recognition already uses — a trait, a real implementation
behind an external program, a fake that records calls — is proven in this
codebase to make an external-program stage testable without the program. Building
delivery to the same shape means the daemon carries one pattern for "call
something outside the process and read whether it refused," not two.

## 5 Failure reporting: journal and toast

**Context.** The daemon's only visible-failure channel today is a line on
standard error, which herdr captures and shows through `herdr plugin log list`
(`src/daemon.rs:143-150`, `request_line`; `src/daemon.rs:134-139`,
`context_note`). No journal or toast subsystem exists beyond that. `docs/design.md`
documents `[ui] toasts = true` but `src/config.rs` reads no `[ui]` table at all
today — `Config` has only `audio`, `stt` and `rewrite` (`src/config.rs:20-23`). The
decision recorded 2026-08-26 in `docs/decisions.md` makes the toast obey
`[ui] toasts` while the journal line stays unconditional.

**Problem.** AC-7 and AC-8 ask for a journal line and a toast on a failed
delivery. Nothing today raises a toast from inside `src/`.

**Decision — the mechanism exists, outside `src/`, and this design uses it rather
than building one.** `herdr notification show` is a real herdr subcommand, already
proven by the prototype (`spike/spike.sh:96`, `note()`). Raising a toast is one
more call through the same `Deliverer` trait (section 4), `notify(title, body)`,
run through the same `HerdrDeliverer` that already knows how to find and run
`herdr`. Nothing needs to be invented; what was missing was only that `src/`
never called it.

On a failed delivery, `transcribe` does two things, in this order:

1. **Journal line, unconditional.** `eprintln!("delivery failed: pane={target} \
   reason={why}")` — one more line in the channel `request_line` and
   `context_note` already write to, exactly as `tasks/22/RUN_22.md` reads AC-7's
   "journal line."
2. **Toast, gated on `[ui] toasts`.** When `settings.toasts` is `true`,
   `deliverer.notify("Dictation failed", &format!("{target}: {why}"))` is called.
   Its own failure — herdr itself unreachable, say — is logged with one more
   `eprintln!` and nothing else; a toast that could not be shown must not stop the
   journal line or the client's reply from getting through, and must not panic
   (CLAUDE.md, "no panic paths").

`[ui] toasts` is added to `src/config.rs` as a new `Ui` table, `toasts: bool`,
default `true` — the value `docs/design.md:242` already documents. No other `[ui]`
key is added; the rest are out of scope for this issue.

**Why.** The decision recorded 2026-08-26 already settled that the toast obeys the
documented key rather than ignoring it, and that the journal line stays
unconditional so a person who turned toasts off still has the failure recorded.
This section only settles how a toast is raised at all, which the decision left
open: through the same outward call delivery already makes, to a command herdr
already exposes.

## 6 The text's survival

**Context.** A take produces one file, the WAV (`Take.path`); there is no separate
text file. Nothing in `src/` deletes a take's file after a successful `transcribe`
today — `capture.rs` removes a take's file only when a take is discarded before it
is ever handed to recognition: a device that dies mid-take
(`src/capture.rs:353`) or one that measures below the silence floor
(`src/capture.rs:476`, `485`). A take that reaches `transcribe` keeps its file
regardless of what happens next, by the simple fact that nothing after capture
ever calls `std::fs::remove_file` on it.

**Problem.** AC-9 and AC-10 ask for two things to be true on a failed delivery: the
text was written down before the attempt, and the audio was not deleted.

**Decision.**

- **The text.** Before `delivery::deliver` is called, `transcribe` writes
  `eprintln!("delivering: {text}")` — one more journal line, before the attempt,
  so the text is visible in `herdr plugin log list` even if the process were to
  die between that line and the call. This is in addition to, not instead of, the
  text appearing again in full in the client's failure reply (section 3).
- **The audio.** Delivery adds no call that deletes `Take.path`, on success or on
  failure. The file already survives every path through `transcribe` today; this
  design keeps that true rather than introducing cleanup, which stays future work
  (`tasks/8/DESIGN_8.md`, section 5: "Nothing deletes old takes yet. That belongs
  with the stage that consumes them").

A person recovering from a failed delivery has three independent ways to the
text — the client's own reply, the journal line written before the attempt, and
the WAV itself, which can be re-transcribed by hand — matching the three-way
reading `AC_22.md`'s "Chosen readings" already commits to.

**Why.** Three independent copies is what "must not be lost silently" asks for
when there is no dedicated text-storage facility to add: cheaper than building
one, and each copy survives a different way the other two could fail to reach the
person (a client that was piped to `/dev/null`, a journal nobody is tailing, a
terminal that was closed).

## 7 Configuration

Two tables are added to `Config` (`src/config.rs:20-23`), both already documented
in `docs/design.md:238-247` and neither read today:

```toml
[ui]
toasts = true

[delivery]
submit = false
```

`Ui` gets only `toasts` for now — the rest of `docs/design.md`'s `[ui]` table
(`blink_ms`, `sidebar_token`, `tab_indicator`, `preview`, `journal`) belongs to the
indicator and preview work this issue puts out of bounds. `Delivery` gets only
`submit`, the one key `AC_22.md` (R7) asks for. An absent file, or a file that
omits either table, leaves both at their documented defaults — the same rule every
other table in `Config` already follows.

## 8 Modules and tests

| module | owns | tested by |
|---|---|---|
| `delivery` | the `Deliverer` trait, `HerdrDeliverer`, `deliver()`'s branching, `DeliveryError` | insert-only, submit, submit-falls-back-to-insert, a rejected call — all against `FakeDeliverer`, no live herdr |
| `config` | `[delivery] submit`, `[ui] toasts`, and their defaults | defaults, partial file, an absent `[delivery]`/`[ui]` table |
| `capture` | `Take.agent`, carried the way `Take.target` already is | a take started with an agent name reports it; one started without reports `None` |
| `daemon` | calling `delivery::deliver` from `transcribe`, the journal lines, the toast gate, the two reply shapes | a finished take reaches delivery (AC-2); the journal line precedes the attempt (AC-9); a successful delivery's reply names the pane and the level and not the text (AC-11); a failed one's reply carries the text and the reason (AC-12); a toast is raised only when `[ui] toasts` is `true` (AC-8) |

## 9 What this design does not decide

- The blinking indicator and push-to-talk timing — out of bounds per the issue.
- Rewriting the text before delivery — out of bounds per the issue.
- Deleting old take files at all, on any outcome — deferred since #8, unchanged
  here.
- Fixing issue #19, the truncated multi-line reply. Section 3 states that this
  design does not depend on it being fixed, not that it fixes it.
- The rest of `[ui]` beyond `toasts` — belongs to the indicator work this issue
  puts out of bounds.
- Whether `herdr notification show`'s own failure should itself be visible beyond
  a journal line. Nothing in `AC_22.md` asks for a second failure channel behind
  the toast; a toast that could not be raised still leaves the unconditional
  journal line and the client's reply intact.

## 10 Where each criterion is decided

| AC | decided in |
|---|---|
| AC-1 `[delivery] submit`, default `false`, absent table or file | 7 |
| AC-2 delivery reached from a finished take in the running daemon | 1 |
| AC-3 `submit = false` inserts, never submits | 4 |
| AC-4 `submit = true` submits when the pane has an agent, falls back otherwise | 2, 4 |
| AC-5 the pane is the one pinned at take start | 1 (unchanged: `Take.target` already does this) |
| AC-6 any rejected call is a failed delivery, no existence check | 4 |
| AC-7 a journal line names the pane and the reason | 5 |
| AC-8 a toast on a failed delivery | 5 |
| AC-9 the text is journalled before the attempt | 6 |
| AC-10 the take's audio is kept on a failed delivery | 6 |
| AC-11 a successful reply names the pane and the level, not the text | 3 |
| AC-12 a failed reply carries the text and the reason | 3 |
| AC-13 no panic path; every failure names what to do next | 4 (`DeliveryError`), 3 (reply wording), 5 (toast failure does not propagate) |
| AC-14 insert, submit, fallback and rejected-call tests, no live herdr | 4, 8 |
