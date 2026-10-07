# RUN_18

| field | value |
|---|---|
| issue | #18 — cancel answers "nothing to cancel" and stops nothing |
| input | GitHub issue, read with `gh issue view 18`; the issue has no comments |
| stage | S4 |
| branch | fix/18-cancel |
| opened | 2026-10-06 |

## Stages

<!-- One block per stage, appended, never rewritten. -->

### S1 Assess
- artifact: `AC_18.md`
- produced: 2026-10-06

## Notes

- The branch started as `fix/18-30-23-takes` and was renamed with `git branch -m`
  before any push.
- Base: `3dd45b8` (`0.1.0-beta.5`).

```yaml
gate:
  stage: S1
  artifact: AC_18.md
  reviewer: designer
  verdict: QUESTIONS
  date: 2026-10-06
  questions:
    - >-
      The "remembered failure" case (fourth to-be bullet, requirement 5, and the test "cancel with
      a remembered failure" in requirement 8) depends on an as-is claim the code does not support.
      In src/capture.rs nothing ever sets `remembered` to Some. It is declared
      `let mut remembered: Option<String> = None;` (src/capture.rs:204), set back to None at
      src/capture.rs:312, and only ever read with `.take()` at src/capture.rs:316 and :356. A take
      that dies with its device stays in `running` with `sink.failure()` set, and the next `Start`
      or `Stop` reports it at once. That case is requirement 3, not requirement 5. As things stand,
      the state requirement 5 describes cannot happen, and no test can create it through `Recorder`.
      I cannot design this part until the AC say which of two things is meant. (a) Requirement 5
      and its test are about a state that never occurs, so they should be dropped or folded into
      requirement 3. (b) This ticket should also make the recorder start filling `remembered`.
      Then I need to know when it gets filled, because that changes behaviour beyond `cancel`.
      The two readings lead to different recorder changes and different test sets.
  blocker: null
```

Reviewer notes, recorded as returned (none blocks the design):

1. The audio file only exists after `stop`; while a take runs its samples sit in memory, so "deletes the take's audio file" passes even if `cancel` does nothing to files.
2. `discard` is at `src/capture.rs:399`, not `:386`.
3. Requirement 6 can be read two ways; the reviewer reads it as applying only when a take was discarded.

Author's answer (round 1): reading (a). Checked against the branch: `remembered` is assigned only `None` and read with `.take()`, so it is never `Some`. Requirement 5 is replaced by "cancel does not touch `remembered`", the device-failure case stays as requirement 3, the file note and the line number are corrected, and requirement 6 states the reading the reviewer used. Neither the issue nor the assignment asks for `remembered` to be filled.

```yaml
gate:
  stage: S1
  artifact: AC_18.md
  reviewer: designer
  verdict: READY
  date: 2026-10-06
```

Reviewer notes, recorded as returned: the reviewer checked the revised claims against the code and could not run `gh issue view`, so the "Observed result, from the issue" paragraph was not checked against the issue; the design does not depend on it.

### S2 Design
- artifact: `DESIGN_18.md`
- produced: 2026-10-06

```yaml
gate:
  stage: S2
  artifact: DESIGN_18.md
  reviewer: planner
  verdict: QUESTIONS
  date: 2026-10-06
  questions:
    - >-
      The design does not say whether the hold check (step 1 of the cancel arm) and
      `recorder.cancel()` (step 2) happen under one lock. The task that builds the cancel arm
      would have to make that choice without the design saying so. Decision 1 says the order is
      safe because the recorder thread serialises `Start`, `Stop` and `Cancel`. The hold check
      is not on that thread: it reads `runtime.hold` through `hold_of`, and in `dictate` that
      guard is a temporary released at the end of the `match` (src/daemon.rs:274). `ptt` sets
      `Opening` and then calls `recorder.start` with the guard released (src/daemon.rs:441-454).
      So this order can happen: `cancel` sees `Idle` and drops the guard, `ptt` sets `Opening`,
      `Start` answers `Began`, then `Cancel` arrives and discards the hold's take. The hold then
      becomes `Live`, and the watcher later gets `NothingRunning` from `stop`. Decision 2 says
      this is the result refusing during a hold prevents, and the AC to-be ("A hold is in
      progress: cancel does not touch it") rules it out. There are three possible answers, and
      each gives a different task with a different done-criterion: (a) keep the hold guard
      across `recorder.cancel()`; (b) give `Cancel` a way to tell a hold's take apart on the
      recorder thread; (c) accept the window, the same one `dictate` already has, and say so.
      The design has to pick one before the cancel-arm task and its test (cancel during each
      hold state leaves the take running) can be written.
  blocker: null
```

Reviewer notes, recorded as returned (none blocks planning):

1. The conditional `Idle` publish compares only `target`. The `dictate` the design guards against is most likely for the same pane, which the condition does not protect. The choice is explicit and the tests match it.
2. The reviewer checked `discard` at `src/capture.rs:399`, the dropping of samples with `Running`, the `cancel` tests at `src/daemon.rs:3093-3140` and `:3343`, `tone_recorder`, `LosingSource`, and the refusal wording at `src/daemon.rs:276-289`; all hold.

Author's answer (round 1): option (a). Checked: nothing the recorder thread runs takes the hold guard, so holding it across `recorder.cancel()` cannot deadlock, and `ptt` waits on it before claiming `Opening`. Added as decision 5 and as a line in the cancel arm. Note 1 is accepted and written into the design: `Activity` is display state, so a same-pane `dictate` in that interval costs a wrong label until the next stage publishes.

```yaml
gate:
  stage: S2
  artifact: DESIGN_18.md
  reviewer: planner
  verdict: READY
  date: 2026-10-06
```

Reviewer notes, recorded as returned: the design says "plus one helper" but describes two new daemon functions, `hold_refusal` and the conditional publish; the plan cuts them as separate tasks.

### S3 Plan
- artifact: `PLAN_18.md`
- produced: 2026-10-06

```yaml
gate:
  stage: S3
  artifact: PLAN_18.md
  reviewer: implementer
  verdict: QUESTIONS
  date: 2026-10-06
  questions:
    - >-
      "The Windows dead-code check on a scratch copy" is a required step before every commit
      (PLAN_18.md:5 and :169), and the plan gives no command and no procedure for it. The plan
      also says "the four gates in the brief", but no brief is in the run, and the plan names
      the Windows check as a fifth step. The repository does not define the check either.
      docs/evidence.md:1820 describes it only as "the `cfg` rewrite of `src/` followed by the
      same clippy", with no script or command. tasks/73/RUN_73.md:810 calls it an
      "approximation with the windows arms enabled". I would have to invent the rewrite (which
      `cfg` attributes to flip, where the scratch copy goes, and what output counts as clean).
      The plan needs to state the exact commands and the expected result. Alternatively it can
      drop the step for this change, since the new code (`Cancelled`, `cancel_one`,
      `hold_refusal`, `publish_idle_if_recording`, `cancel`) is called from both platforms.
  blocker: null
```

Reviewer note, recorded as returned: everything else was checked against the source and could be executed as written.

Author's answer (round 1): the plan now states the four commands and the Windows check as exact commands, with what clean means. The step is kept: the new items are reachable from both platforms today, but the check is the one CI runs and a later edit in this branch could change that.

Round 2 of the S3 gate did not return: the session ended while the reviewer was running (the machine was powered off). No verdict was recorded for it. Before the gate is run again, `origin/main` (`eae0135`) was merged by fast-forward. The merge changed `src/daemon.rs` only after the lines the plan cites (the cited lines, 38, 274, 442, 3093-3343, are unchanged) and did not touch `src/capture.rs`.

```yaml
gate:
  stage: S3
  artifact: PLAN_18.md
  reviewer: implementer
  verdict: READY
  date: 2026-10-07
```

Reviewer note, recorded as returned: the `Hold` literal is at `src/daemon.rs:442-450`, not `437-445` as the plan cites; the copy is still obvious.

### S4 Implement
- baseline: `eae0135`; toolchain recorded below when the first command runs.

Implementation order, stated plainly: the tests of task 1 were written first and failed to compile for want of `Recorder::cancel` before any code existed. For tasks 2, 3 and 4 the code was written before the tests, against the plan's test list; the tests were then written and the whole suite run. The mutation test below is what checks that those tests fail when the code is wrong.

Baseline: `eae0135`, rustc 1.99.0, `cargo test` 750 passed, 1 ignored. After the four tasks: 772 passed, 1 ignored. `cargo clippy --all-targets -- -D warnings`, `cargo fmt --check` and `python3 scripts/check_manifest.py` are clean; the Windows dead-code check on a scratch copy is clean.

### S4 code review (round 1)

Reviewer: a fresh general-purpose subagent, over `eae0135..94661c5`. No Critical findings. Verdict: ready to open a pull request, with fixes. Findings and how each was settled:

1. Important — `dictate` publishes `Recording` after `recorder.start` returns, holding no lock between the two; a `cancel` in that gap discards the take, finds `Idle`, and `dictate` then publishes `Recording` for a take that is gone. **Accepted, not fixed.** Closing it needs `dictate` to hold the hold guard across the device open, which blocks every `ptt` repeat for the hundreds of milliseconds a device takes to open. `Activity` is display state only, and the window is the time between two statements. The label is corrected by the next stage that publishes. Stated in the pull request description.
2. Minor — the same window exists for a same-pane `ptt` between the guard being released and `publish_idle_if_recording`; the conditional `Idle` can erase that hold's `Recording` until the watcher publishes at the end of the hold. **Accepted**, for the reason in 1.
3. Minor — `cancel` waits on the recorder thread while holding the hold guard, so a slow device open started by `dictate` delays `ptt` repeats and the reply may pass the client's two-second bound. **Accepted**; bounded by the device-open time.
4. Minor — weak tests. `cancel_leaves_no_file_in_the_takes_directory` and `cancel_leaves_no_wav_behind` cannot exercise `discard` removing a file, because no file exists while a take runs; they guard against `cancel_one` writing one. **Kept as they are, and said so.** Added: an assertion that a refused cancel leaves the indicator on `Recording`, and a daemon-level test for the `Opening` state.
5. Minor — "nothing to cancel" is also the answer while a take is being transcribed. Already decision 4 of `DESIGN_18.md`; not changed.
6. Minor — comments: none became untrue.

### S4 mutation testing (round 1)

Tester: a fresh general-purpose subagent, in a scratch copy, 41 mutations of the lines the diff adds. 36 were killed. Five survived:

- c2 — `discard(Some(take))` in `cancel_one` replaced with `drop(take)`. **Equivalent today:** the take's file is written only in `stop_one`, so while a take runs there is no file for `discard` to remove. Kept as written; the call is there so a later change that writes the file earlier does not leave one behind. No test can fail on it without a source that creates the file, which the recorder cannot ask for.
- c5 — the send-failure fallback in `Recorder::cancel` returning `Discarded` instead of `NothingRunning`. **Killed by a new test:** `cancel_on_a_recorder_whose_thread_is_gone_finds_nothing_running`.
- c6 — the reply-lost fallback. **Killed by a new test:** `cancel_answers_nothing_running_when_the_thread_dies_before_replying`.
- d3 — the hold guard released before `recorder.cancel()`. **Killed by a new test:** `cancel_keeps_the_hold_guard_while_the_recorder_discards`.
- d8 — an unconditional `publish(runtime, Activity::Idle)` in the `Discarded` arm. **Killed by a new test:** `a_discard_leaves_a_working_indicator_for_another_pane_alone`.

The four new tests were written, and the suite ran green (25 tests with `cancel` in the name). A re-check that each of c5, c6, d3 and d8 now fails the suite was started in the background and did not finish: the machine's session ended. Before it was re-run, `git diff` on the worktree showed 118 added lines and no removed ones, and none of the mutated fragments; the uncommitted changes are the four new tests, not a mutation. The background command mutated a scratch copy only.

Re-check, run in the foreground on a scratch copy of the merged tree, one mutation at a time, each under a 300-second limit, `CARGO_BUILD_JOBS=6`:

| mutation | test that fails | result |
|---|---|---|
| c5 — send-failure fallback returns `Discarded` | `cancel_on_a_recorder_whose_thread_is_gone_finds_nothing_running` | killed |
| c6 — reply-lost fallback returns `Discarded` | `cancel_answers_nothing_running_when_the_thread_dies_before_replying` | killed |
| d3 — hold guard released before `recorder.cancel()` | `cancel_keeps_the_hold_guard_while_the_recorder_discards` | killed |
| d8 — unconditional `Idle` in the `Discarded` arm | `a_discard_leaves_a_working_indicator_for_another_pane_alone` | killed |

Two corrections found while doing it. The first run of d3 hung instead of failing: the test asserted while the recorder was still waiting inside the source's `stop`, so the failed assertion left the scope waiting for the recorder. The test now records what it saw, releases the recorder, and asserts afterwards; with d3 applied it fails in 0.16 s. And the first run of d8 used the filter `cancel_`, which does not match that test's name, and reported a false survival; it was re-run by the test's full name.

After the merge of `origin/main` (`b900db7`): `cargo test` 806 passed, 1 ignored; clippy, `cargo fmt --check` and `scripts/check_manifest.py` clean; the Windows dead-code check on a scratch copy clean.

### S4 verdict
The code review (round 1) found nothing Critical; every finding is settled above. Mutation testing killed 40 of 41 mutations, and the one left is equivalent today (c2). S4 closed on 2026-10-07 before a pull request existed.

### S5 Verify
- artifact: a section in `docs/evidence.md`, "`cancel` stops a running take, for issue #18"
- platform: macOS 27.0.1, arm64, this machine
- run: a daemon from this worktree with its own state, configuration and herdr stand-in, never the owner's session. The baseline `eae0135` reproduced the defect first (the instrument check): `cancel` answered `nothing to cancel` during a take and the next `dictate` delivered that take. The branch cancelled the take, left no file, delivered nothing, and the next `dictate` began a new take.
- not shown, and said in the evidence: the tab label restored after a cancel (no `tab_id` in the context), Linux, Windows.

```yaml
verdict:
  stage: S5
  artifact: docs/evidence.md
  result: pass
  date: 2026-10-07
```
