# RUN_22

| field | value |
|---|---|
| issue | #22 — Delivery: put the transcript in the pane without submitting it |
| input | GitHub issue, read with `gh issue view 22` |
| stage | S1 |
| branch | feat/22-delivery |
| opened | 2026-08-26 |

## Stages

<!-- One block per stage, appended, never rewritten. -->

### S1 Assess
- artifact: `AC_22.md`
- produced: 2026-08-26

Transcription and rewrite (issues 13, 15, 16, 21) are not part of this branch, so
the acceptance criteria treat delivery as an operation over a pane and a piece of
text — testable with a fake pane call, per `CLAUDE.md`'s own description of how
this stage is tested ("delivery through a recorded call") — rather than as an
end-to-end pipeline from a real transcript. That gap is recorded under "Out of
scope / noticed" in `AC_22.md`, not folded into an AC.

No journal subsystem exists in `src/`; every existing visible failure in the
daemon is a line on standard error, captured by herdr's own plugin log. "Journal
line" in the acceptance criteria is read as one more line in that same channel.

The take produces no separate text file, only a WAV. "The text must not be lost
silently" is met two ways: the text is written to the journal line before
delivery is attempted, and the WAV is kept rather than deleted on a failed
delivery.

## Gate S1

```yaml
gate:
  stage: S1
  artifact: AC_22.md
  reviewer: designer
  verdict: QUESTIONS
  date: 2026-08-26
  questions:
    - "The as-is is contradicted by the worktree the design will be implemented in, and the contradiction changes the design. AC_22.md says the pipeline stops after capture and that there is no transcription because issues 13, 15, 16 and 21 are unmerged. Transcription is present and wired: dictate calls transcribe on a finished take and replies with the text, the level and the target. The scope decision rests on the false half. Which design is wanted: (a) delivery is called from dictate/transcribe with the real transcript, which also decides what the reply becomes now that the text goes to the pane instead of back to the client, or (b) delivery is a module with no caller, exercised only by AC-12's tests. AC-1 to AC-12 are satisfiable either way and name no caller."
    - "AC-5 requires a delivery against a pane that no longer exists to be a failed delivery and explicitly not a silent no-op, but nothing states what herdr does against a pane id that is gone. If those commands exit non-zero, AC-5 is already AC-6. If they exit 0 and do nothing, AC-5 forces a pane-existence check against a herdr command nobody names, plus a fake for it in the tests. Two different designs and two different test surfaces."
  blocker: null
```

Note recorded by the reviewer, not a reason to withhold READY: AC-3 makes
`submit = true` always use `herdr agent prompt`, while the prototype gates that on
the target pane actually having an agent and otherwise falls back to `send-text`.
AC-6 makes a rejected call a visible failure, so the dropped condition is
designable as written; it was dropped deliberately.

### Answers

**Question two is answered by measurement, not by choice.** Both commands refuse a
pane that does not exist, with a code and a structured error:

```
$ herdr pane send-text "w99:p99" "probe"
{"error":{"code":"pane_not_found","message":"pane w99:p99 not found"},...}
exit=1
$ herdr agent prompt "w99:p99" "probe"
{"error":{"code":"agent_not_found","message":"agent target w99:p99 not found"},...}
exit=1
```

So a pane that is gone is an ordinary rejected call. AC-5 collapses into AC-6, no
existence check is needed before delivering, and the tests need no fake for one.

**Question one is answered (a): delivery is called when a take finishes.** The
goal of this issue is that the text lands in the pane's input — a module with no
caller does not reach it, and an unreached goal is not something to plan around.
What the client prints changes with it: the transcript goes to the pane, so the
reply becomes a confirmation of where it was delivered and at what level, rather
than the text itself. That is a change to what the client prints, in the same class
as the earlier decision that made it print anything at all, and it is recorded in
`docs/decisions.md` when the criteria are revised.

### S1 Assess — revised after the gate
- artifact: `AC_22.md`, `docs/decisions.md`
- produced: 2026-08-26

Both questions are answered in the artifact, with facts rather than choices where
a fact settled it.

The as-is section is rewritten against `main` as it stands after pull request
#20: recognition is in, `dictate` already produces real text on a finished take,
and delivery is now written as a called stage (AC-2), not only a tested module.
That changes what `dictate`'s reply carries: a successful delivery is confirmed by
where the text went, not repeated (AC-11); a failed one carries the text itself
and the reason (AC-12), since neither the pane nor the client's own output has
the text otherwise. Recorded in `docs/decisions.md`.

The pane-gone question is answered by the measurement in `docs/evidence.md`,
"Delivering into a pane that is gone": both `herdr pane send-text` and `herdr
agent prompt` refuse a nonexistent pane with exit 1 and a machine-readable error
code. The old AC-5 (pane gone treated as a failure, not a silent no-op) is now
one instance of AC-6 (any rejected call is a failed delivery); no existence check
and no dedicated fake are needed.

The reviewer's note on AC-3 is acted on rather than left as a note: the
prototype's fallback — `submit = true` still inserts without submitting when the
pinned pane has no agent — is restored as AC-4, using `Invocation.
focused_pane_agent`, already captured at pin time, so no new herdr call is
needed to keep the condition.

## Gate S1, second pass

```yaml
gate:
  stage: S1
  artifact: AC_22.md
  reviewer: designer
  verdict: READY
  date: 2026-08-26
  questions: []
  blocker: null
```

The reviewer checked the one field a criterion depends on: `focused_pane_agent`
exists in `src/context.rs` as `Option<String>`, is parsed from the invocation body
and is covered by a test. So the agent name is available when the take is pinned,
without a new call to herdr. Carrying it from the first `dictate` to delivery on
the second is work the design adds — `answer` currently keeps only the pane string,
and `Take` holds only the path, the level and the target.

### One decision taken here, so the design does not have to guess

`docs/design.md` documents `[ui] toasts = true`, a key that governs toasts, and
AC-8 requires a toast without mentioning it. The toast obeys `[ui] toasts`: a
documented key that some code ignores is a key that lies, and the journal line
required by AC-7 is unconditional anyway, so a person who turned toasts off still
has the failure recorded rather than lost. This is not a new user-visible name —
the key already exists — and it is recorded in `docs/decisions.md`.

S1 is closed. Next is S2 Design, which by its own rule stops for the owner's
approval of the intent before anything is implemented.

## Gate S2

```yaml
gate:
  stage: S2
  artifact: DESIGN_22.md
  reviewer: planner
  verdict: QUESTIONS
  date: 2026-08-26
  questions:
    - "The daemon test that proves delivery is called needs transcribe's success path to be reachable, which needs recorder.stop() to return a Take. The only source daemon.rs can reach is capture::tests_support::SilentSource, which pushes zero samples, so the take is refused as too quiet and transcribe is never entered — which is why the existing test asserts on 'below ... dB'. The audible source that would produce a real Take lives in capture.rs's private test module, not in tests_support. No section says who exports an audible source, and the capture row of the task table scopes capture to Take.agent only. The daemon-test task therefore has an input nobody produces and an undeclared dependency on the capture task."
    - "The journal lines are specified as eprintln! inside transcribe, while the task table claims a daemon test proving that the line with the text appears before delivery is called. Standard error written by eprintln! is not readable from inside the test process, and no existing test asserts on it: this codebase makes journal text testable by returning it from a pure function and calling eprintln! at the call site, the way request_line and context_note do. Which shape delivery's journal lines take is not stated, and it changes transcribe's signature — a returned line or a passed writer rather than a bare eprintln!. That is the interface between the daemon-wiring task and the task that verifies the journal-line criteria."
  blocker: null
```

Both citation checks the gate was asked to run hold: `spike/spike.sh` really calls
`herdr notification show` with a `--body`, and the agent-name threading matches the
shape of `src/capture.rs` — `Take`, `Command::Start`, `Running` and
`Recorder::start` all take one more field beside `target` without resistance. The
design drops the prototype's `--sound`, which no criterion asks for.

Three citations in the design have drifted by a few lines each: `Command::Start`,
`HERDR_BIN_PATH` in `src/doctor.rs`, and `stt::tests_support`.

## Gate S2, second pass

```yaml
gate:
  stage: S2
  artifact: DESIGN_22.md
  reviewer: planner
  verdict: READY
  date: 2026-08-26
  questions: []
  blocker: null
```

Both claims the gate was asked to check hold. `capture::tests_support` exports only
`SilentSource`, which pushes 4 800 zero samples; the scriptable fake and its tone
generator are private to the test module, so the `ToneSource` the design adds is
the minimal audible slice of them. It clears the floor with room to spare — the
silence floor defaults to −60 dB and a 0.3-amplitude sine measures near −13 dB,
the same amplitude an existing capture test already asserts stays above the floor.
The step after the level check survives too: the WAV writer creates the take
directory itself, so a daemon test whose takes directory does not exist still
reaches transcription, which with the fake engine never opens the file.

The `Runtime` change matches the code: `transcribe` is reached through `dictate`,
`answer`, `serve_one`, `serve` and `start` — the five-function chain that threads
the recognition engine today and that the design replaces with one bundle.

Citations drift by a few lines in three more places, the same class as before. It
changes no task boundary.

S2 is closed. Next is S3 Plan.
