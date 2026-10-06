# RUN_28

| field | value |
|---|---|
| issue | #28 — A wedged herdr or transcriber leaks a thread and misnames the failure; this run also closes #94 — A long non-ASCII error from a command engine panics the daemon instead of being reported |
| input | GitHub issues, read with `gh issue view 28 --comments` and `gh issue view 94 --comments`; the body and comments of each are the ticket |
| stage | S1 |
| branch | fix/28-94-outward-calls |
| base | `origin/main` at `3dd45b8` (0.1.0-beta.5) |
| opened | 2026-10-06 |
| pull request | one, closing both issues |

## Stages

<!-- One block per stage, appended, never rewritten. -->

### S1 Assess
- artifact: `AC_28.md`
- produced: 2026-10-06

```yaml
gate:
  stage: S1
  artifact: AC_28.md
  reviewer: designer
  round: 1
  verdict: QUESTIONS
  date: 2026-10-06
  questions:
    - >-
      The as-is section says "A take is delivered on that thread" (meaning the
      connection thread, src/daemon.rs:1445). That is true only for the `dictate`
      toggle. A push-to-talk take runs bias, transcription, rewrite and delivery on
      the single `watch` thread that lives as long as the daemon: src/daemon.rs:513
      ("One thread for the daemon's life ... The pipeline runs here too"), through
      `end_take` at src/daemon.rs:630-655. No client waits on that path, and its
      failures reach the person through `report_failure` (journal line and toast,
      src/daemon.rs:706). A wedge there does not leak a thread. It stalls the
      watcher, which leaves the hold in `Ending`, so every later `ptt` and
      `dictate` is refused with "still being transcribed" (src/daemon.rs:434,
      :283). It also makes the shutdown `watcher.join()` at src/daemon.rs:1455
      wait forever.
      AC-6 (the client reply names herdr), AC-7 (measured against "the client's own
      bound") and AC-8 ("the connection thread returns") say what done looks like
      only on the toggle path. The AC needs to say whether the hold path is in
      scope. If it is, the AC needs to say what done looks like there: what the
      person must see when a bounded call times out, and what a "no bound" policy
      for transcription must say about a watcher that never comes back.
      This blocks me because AC-2's central decision, whether transcription gets a
      bound or a recorded reason for having none, has different consequences on the
      two paths. On the toggle path "no bound" ends with the client's 120-second
      message. On the hold path it ends with no message and a daemon that cannot
      take dictation or shut down. Without the answer I would have to invent the
      hold-path requirement myself.
  blocker: null
```

Reviewer note, not a gate question: on the `dictate` path the pane read, transcription, rewrite and delivery run one after another inside the single 120-second `WORKING_TIMEOUT` (`src/daemon.rs:327-333`, `src/client.rs:23`), so AC-7 holds only if the bounds added together stay under 120 seconds.

Author's answer (checked against `src/daemon.rs:513`, `:630-655`, `:706`, `:283`, `:434`, `:1455`): the hold path is in scope. #28's goal is that a wedge becomes a message and not a stuck daemon; on the hold path the stuck thing is the watcher. `AC_28.md` is revised: as-is corrected, AC-6 to AC-8 split by path, AC-14 and AC-15 added.

```yaml
gate:
  stage: S1
  artifact: AC_28.md
  reviewer: designer
  round: 2
  verdict: READY
  date: 2026-10-06
  blocker: null
```

Reviewer note, not a gate question: AC-14 asks that a journal line and a toast name the program on the hold path. If the call that timed out is herdr delivery, the toast goes out through the same herdr binary (`report_failure` → `toast` → `runtime.deliverer.notify`, `src/daemon.rs:706-722`), so with herdr wedged the toast cannot appear, the watcher waits a second bound for it, and then writes `toast_failed_line`. The design states this case explicitly; the AC-14 test uses a fake herdr that hangs only on the delivery subcommand.

S1 is closed.

### S2 Design
- artifact: `DESIGN_28.md`

```yaml
gate:
  stage: S2
  artifact: DESIGN_28.md
  reviewer: planner
  round: 1
  verdict: READY
  date: 2026-10-06
  blocker: null
```

Reviewer notes, none held back READY, all three answered in `DESIGN_28.md`:
1. "Two bounds" does not hold for the second AC-14 test: with herdr fully wedged `take_bias` runs the pane read first, so the watcher waits three bounds, 5 + 10 + 10 = 25 seconds. Corrected in sections 4 and 6.
2. On the hold path a delivery timeout is reported by the failure arm itself (`src/daemon.rs:1006-1027`: journal line, its own toast, `Reported::Yes`), not by `report_failure`; that toast's text does not name herdr. The arm does not change and the journal line carries the timeout text. Section 4 now says so.
3. `PaneError::TimedOut` gets a `bound` field so the message names the bound used. Corrected in section 4.

Author's note on `AC_28.md`: AC-1 says the entries use the four-part form "`CLAUDE.md` requires". That form is the author's own standing rule, not a line of this repository's `CLAUDE.md`; `docs/decisions.md` is a table of Decision | Basis | Where, so the four parts go into those columns, as `DESIGN_28.md` section 3 states.

S2 is closed.

### S3 Plan
- artifact: `PLAN_28.md`
- produced: 2026-10-06

Deviation from `DESIGN_28.md` section 2 step 5, decided while planning: after a
timeout the reader threads are not given one second to finish. The output of a
timed-out call is discarded, so nothing needs them, and they end by themselves when
the killed process group closes its pipes. After a normal exit the pipes are still
given `max(time left, 1 second)`, which is where the design's one second is kept.
The key name `[stt] command_timeout_seconds` is with the owner; the plan carries it
as not final.

```yaml
gate:
  stage: S3
  artifact: PLAN_28.md
  reviewer: implementer
  round: 1
  verdict: READY
  date: 2026-10-06
  questions: []
  blocker: null
```

Reviewer notes, none held back READY, all three fixed in `PLAN_28.md`: the dependency diagram swapped the numbers of the docs and gates tasks; Task 4 step 8 was worded confusingly (it removes `#[allow(dead_code)]` from `shorten` only); the Task 8 commit listed `src/rewrite/command.rs`, which Task 8 does not touch.

S3 is closed. Next is S4.

### S4 Implement

Decisions confirmed before S4: the key `[stt] command_timeout_seconds` (default 60,
floor 1) and the per-call policy (delivery 10 seconds, pane read 5 seconds, rewrite
command 30 seconds, all fixed) stand as designed, and the key name is final. The two
scope additions, the push-to-talk watcher path and the rewrite command, stand.

The plan's Windows dead-code check found one defect that the macOS gates did not:
`with_bound` in `src/delivery.rs` and `src/rewrite/command.rs` was `#[cfg(test)]` but
used only by Unix tests, so a Windows test build would have failed on dead code.
Both are now `#[cfg(all(test, unix))]` (commit "Keep the test-only bound setters off
the Windows build").
