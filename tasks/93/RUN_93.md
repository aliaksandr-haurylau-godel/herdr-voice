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

### S4 Implement
- code: `src/stderr.rs` (new), `src/daemon.rs`, `src/doctor.rs`, `src/capture/cpal_source.rs`,
  `tests/daemon_dead_stderr.rs` (new), `docs/design.md`, `docs/decisions.md`.
- Rulings on the plan: the Windows dead-code sequence in Task 2 Step 5 ends in
  `git checkout -- src`, which discards uncommitted work; it was run once with
  uncommitted edits, `src` was restored and the edits re-applied from the same
  script, and later runs were made on a clean tree and then on a scratch copy of the
  repository outside the worktree. Result each time: clippy with `-D warnings`, no
  warning.

Review of the whole diff (fresh subagent, read only), findings and what was done:

- Important: `docs/evidence.md` had no entry (AC requirement 5). That is S5, below.
- Important: no test ran the real writer against a dead standard error. Fixed:
  `tests/daemon_dead_stderr.rs` starts the built binary with standard error on a
  pipe whose reader is closed, sends `cancel` through the built client and runs
  `doctor`. With `write_line` changed to `unwrap()` both tests fail; with it as
  written they pass.
- Minor, fixed: every `doctor` ping journalled "context unreadable". The daemon no
  longer writes the note for `ping`; `a_ping_journals_its_request_and_no_note_about_a_missing_context`
  failed first and passes.
- Minor, deferred: a silent daemon (accepts, never replies) is not tested in
  `doctor`; the two-second bound is `send_to`'s and is tested in `src/client.rs`.
- Minor, deferred: the no-print scan covers `src/daemon.rs` and
  `src/capture/cpal_source.rs` only. The reviewer found no other production print or
  inherited-stdio child process.
- Minor, deferred: `src/main.rs` still uses `eprintln!` for the `daemon` command's own
  start-up failures, before the socket is bound; out of the AC's scope.
- Minor, deferred: a journal write blocks, without panicking, if herdr is alive and stops
  draining the pipe; outside the AC (a dead reader gives a broken pipe, which is handled).
- Ruling: the bare `connect` before the `ping` stays. On Windows a named pipe may be
  busy between the two connects and `doctor` could then say "nothing is listening" for
  a healthy daemon; not reproduced, not testable on this machine, recorded as
  unverified in `docs/evidence.md`. Windows as a whole is `docs/design.md`, section 9,
  question 3.
- Ruling: `docs/design.md`'s Problem paragraph states what failed. The rule against
  describing removed material applies to material the decision removes; the failure is
  the problem the decision answers.

Mutation testing (fresh subagent, 44 mutations of the added lines, one at a time,
full suite each): 25 killed, 19 survived. Handled:

- Killed by new tests, and re-run by hand after the tests were added (each turned the
  suite red): dropping the missing-context note (`a_request_that_needs_a_context_and_has_none_journals_the_note`),
  dropping the "connection failed" line (`a_frame_that_cannot_be_read_is_journalled_as_a_failed_connection`),
  `line()` writing nothing, and `StderrJournal::write` emptied, the "listening at" line, the
  "recognition unavailable" line and the refused-source line (all through
  `the_start_up_and_request_lines_reach_standard_error` and
  `a_refused_context_source_is_named_on_standard_error`), the `pkill` text and the
  "restart herdr" text (`a_daemon_that_closes_without_answering_is_missing`), dropping
  the `pong` comparison (`a_daemon_that_answers_something_else_is_missing`,
  `a_daemon_that_answers_with_nothing_is_missing_and_says_what_it_wanted`), and
  widening the ping exemption to `cancel` (the note assertion added to
  `a_served_request_is_journalled_and_answered`).
- `line()` writing to standard output instead of standard error: not killed by name;
  `the_start_up_and_request_lines_reach_standard_error` sends standard output to the null
  device and reads the file, so the mutation fails it.
- Not killed, explained: replacing `continue` by `break` on a failed accept, and dropping
  the "accept failed" line, need a listener whose accept fails once, which
  `transport::Listener` does not allow to be substituted; the reply write's `?` replaced by
  `let _ =` changes only the text journalled for a client that hung up; dropping
  `outcome.code == 0` from the doctor condition is equivalent, because a non-zero code
  never carries `pong`; `&address.clone()` is equivalent; the three mutations of the
  duplicate-device notice in `src/capture/cpal_source.rs` need a sound card and are
  verified by hand or not at all, as `CLAUDE.md` says of capture.

Gates on the last commit of S4, run fresh: `cargo test` 587 passed, 4 passed and
2 passed (unit, `daemon_dead_stderr`, `setup_process`), 1 ignored; `cargo clippy
--all-targets -- -D warnings` clean; `cargo fmt --check` clean; `python3
scripts/check_manifest.py` prints `manifest: 12 entries, all commands known`; the
Windows dead-code check on a scratch copy is clean. No test in `src/stt/fetch.rs`
failed.

S4 is closed. Next is S5 Verify.

### S5 Verify
- artifact: section "A daemon whose herdr has gone, for issue #93" in `docs/evidence.md`
- produced: 2026-09-30
- method: the second one the orchestrator allowed — the built daemon started by hand with
  standard error on a pipe whose reader was closed. An isolated herdr could not be
  arranged: the plugin registry that `herdr plugin link` writes to is shared with the
  person's own herdr and no documented setting moves it, and the brief forbids `link`.
- result: on `main` the run reproduces the issue's own line and `doctor` says `ok`; on the
  fixed build the request is answered; the fixed `doctor` against a daemon from `main`
  says `missing` and names the recovery. Negative results and what the run does not show
  are in the evidence section.

```yaml
verdict:
  stage: S5
  artifact: docs/evidence.md
  verdict: recorded
  date: 2026-09-30
```
