# RUN_93

| field | value |
|---|---|
| issue | #93 — After herdr restarts, the daemon it left behind fails every command while doctor says it is fine |
| input | GitHub issue, read with `gh issue view 93 --comments` (no comments; the body carries the evidence) |
| stage | S1 |
| branch | fix/93-daemon-stderr |
| opened | 2026-09-30 |

## Stages

<!-- One block per stage, appended, never rewritten. -->

### S1 Assess
- artifact: `AC_93.md`
- produced: 2026-09-30

```yaml
gate:
  stage: S1
  artifact: AC_93.md
  reviewer: designer
  verdict: READY
  date: 2026-09-30
  blocker: null
```

Reviewer notes, answered by the author (none held back READY):
- The AC named a brief that is not in the repository; it now points at the four gates
  listed in `CLAUDE.md`, "Local development".
- Four `eprintln!` calls sit between `listen()` and the accept loop
  (`src/daemon.rs:1296`, `:1316`, `:1324`, `:1339`). Requirement 2 now says "from the
  moment the socket is bound", so they go through the same writer.
- No daemon command is free of side effects today, so S2 will add one for the probe.
- "The client bound" in requirement 4 is `REPLY_TIMEOUT` (`src/client.rs:17`, 2 seconds).

S1 is closed. Next is S2 Design.

### S2 Design
- artifact: `DESIGN_93.md`
- produced: 2026-09-30

```yaml
gate:
  stage: S2
  artifact: DESIGN_93.md
  reviewer: planner
  round: 1
  verdict: QUESTIONS
  date: 2026-09-30
  questions:
    - >-
      Section 4 plans three tests for `daemon_finding`: a listener that closes
      without replying, "a listener running `serve` with a fake runtime", and no
      listener. The design does not say how `daemon_finding` is pointed at a test
      address, or where the fake serving daemon comes from (`daemon_finding()` takes
      no argument and reads the environment; `serve`, `fake_runtime` and
      `silent_recorder` are private to `daemon`). `send_to` returns only `code` and
      `message`, so "nothing is listening" cannot be told from "did not answer",
      yet section 3 keeps the existing no-listener text.
  blocker: null
```

Answer, by the author, from the design: `daemon_finding` becomes
`daemon_finding_at(&Address)` with the environment lookup left in `daemon_finding()`;
the doctor tests use listeners written by hand in `src/doctor.rs`'s tests and touch
nothing in `daemon.rs`; the no-listener case is told apart by the bare connect that
runs before the `ping`. `DESIGN_93.md` sections 3 and 4 say so now. The reviewer's
note on requirement 1 (two tests together instead of one with a failing journal) is
kept: `Journal::write` returns nothing, so a journal whose write fails cannot be
expressed, and the writer that swallows the failure is tested on its own.

```yaml
gate:
  stage: S2
  artifact: DESIGN_93.md
  reviewer: planner
  round: 2
  verdict: READY
  date: 2026-09-30
  blocker: null
```

Reviewer note, for S3: each doctor test listener first receives the empty connection
of the bare `transport::connect`, then the `ping` connection, so a listener that
answers must accept past the first one. The plan's task for those tests says so.

S2 is closed. Next is S3 Plan.

### S3 Plan
- artifact: `PLAN_93.md`
- produced: 2026-09-30

```yaml
gate:
  stage: S3
  artifact: PLAN_93.md
  reviewer: implementer
  round: 1
  verdict: QUESTIONS
  date: 2026-09-30
  questions:
    - >-
      Task 2 Step 5 (and Task 4 Step 4 and Task 6) say to run the Windows
      dead-code check "as in CLAUDE.md's neighbouring brief"; CLAUDE.md contains no
      such command, and the plan gives no sed expressions, no target files and no way
      to restore them.
    - >-
      Task 3 Step 1, `ping_is_answered_while_a_hold_is_open`, opens a hold with
      `silent_recorder()` and asserts `Reply::Ok(_)`; every existing ptt test uses
      `tone_recorder(tag)` and `PANE_1`, and nothing shows `SilentSource` lets the
      recorder begin.
    - >-
      Task 2 Step 3, the `eprintln!("{why}")` at `src/daemon.rs:1339`: the plan gives
      `&why.to_string()` and also says to keep what `Display` printed; the plan should
      pick one form.
  blocker: null
```

Answers, by the author, from the code: the plan carries the exact command sequence
in Task 2 Step 5 and the later tasks point at that step; the hold test uses
`tone_recorder("ping-hold")` and `PANE_1` as `a_first_ptt_begins_a_hold_and_names_the_pane`
does, and asserts the hold is still open with `pokes == 1`; the conversion at `:1339`
is `crate::stderr::line(why)`.

```yaml
gate:
  stage: S3
  artifact: PLAN_93.md
  reviewer: implementer
  round: 2
  verdict: READY
  date: 2026-09-30
  blocker: null
```

Reviewer note: the reviewer ran no command, so the Windows dead-code sequence in
Task 2 Step 5 is unexecuted until S4 runs it.

S3 is closed. Next is S4 Implement.
