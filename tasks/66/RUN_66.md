# RUN_66

| field | value |
|---|---|
| issue | #66 — An indicator test loses its fixture script and fails intermittently on Linux |
| bundled | #101 — setup, the indicator and doctor report every failure to start herdr as not found; #62 — six download tests fail intermittently under load; #48 — nothing checks that the model download uses the pinned revision |
| input | GitHub issues, read with `gh issue view <number> --comments` for each of the four; the issue bodies and their comments are the ticket |
| stage | S1 |
| branch | fix/66-101-spawned-fixtures |
| opened | 2026-10-06 |

One run root and one pull request for the four issues, in this order of work:
#101, #66, #62 and #48. #62 and #48 edit the same fixture, `serve` in
`src/stt/fetch.rs`.

## Stages

<!-- One block per stage, appended, never rewritten. -->

### S1 Assess
- artifact: `AC_66.md`
- produced: 2026-10-06

The input is GitHub, not a ticket tracker. The Risk field does not exist on a
GitHub issue and is not set.

Two things measured on this machine while assessing, both in `AC_66.md` under
"as-is":

- Executing a script while a write descriptor on it is open succeeds on macOS
  (Python `subprocess`, Darwin). `ETXTBSY` therefore cannot be produced here,
  and a loop on this machine cannot show that the flake is gone. No container
  runtime is installed here, so a Linux run is possible only on CI.
- `serve` in `src/stt/fetch.rs` stops on a 750 ms quiet deadline and drops its
  listener. That is a second way for a test's own port to refuse connections,
  independent of the port reuse #62 names. It is unverified and is for S2 to
  establish by measurement.

```yaml
gate:
  stage: S1
  artifact: AC_66.md
  reviewer: designer
  verdict: READY
  date: 2026-10-06
  blocker: null
```

Reviewer's note, not a question: AC-8 requires the `ETXTBSY` case to be waited
out and not returned, so a design that avoids the error by another route does
not satisfy it, and the AC-8 test must release its write handle while the
shared code is still waiting.

S5 evidence for #66, in addition to AC-15 (requested by the run's
orchestrator after S1 closed; `AC_66.md` is not edited): the error failed in
roughly 5 of about 12 `ubuntu-latest` runs on 2026-09-30 and 2026-10-05, so two
clean runs are too few. After the fix is pushed, rerun the `ubuntu-latest` job
of one run until there are at least 6 clean attempts in total, and record every
attempt's id and result in `docs/evidence.md` with that before rate. One failure
among them is a finding to investigate, not a retry. `DESIGN_66.md` states why
the fix closes the window by construction, since no test on this machine can
open it.

### S2 Design
- artifact: `DESIGN_66.md`, with its evidence in `DESIGN_66_evidence.md`
- produced: 2026-10-06

Measured while designing, on macOS, in a scratch copy of the crate (deleted):

- A client that first connects 1200 ms after `serve` returns gets
  `Connection refused (os error 61)` on its own address: the 750 ms deadline in
  `serve` closes the listener. That error text is the one in #62; port reuse
  cannot produce it on a listener that is still bound.
- Connecting to `http://127.0.0.1:0` fails at once and is reported as
  `FetchError::Http` (0.02 s).
- A load probe of 60 busy loops on this 15-core machine, run for about 34
  minutes, made the unmodified `stt::fetch` tests fail with the same
  `Connection refused`. The number of completed runs was not recorded, so it is
  not a rate. It was stopped by hand with the load average at about 178, and was
  heavier than the brief allows. Load runs in S5 use at most 6 jobs and a hard
  timeout.

S2 was not approved interactively: the person who owns the repository was not at
the pane. The gate below and the orchestrator are the review.

```yaml
gate:
  stage: S2
  artifact: DESIGN_66.md
  reviewer: planner
  verdict: READY
  date: 2026-10-06
  blocker: null
```

The reviewer's three notes, each a statement in `DESIGN_66.md` that did not
match the code. They were corrected in the design after this verdict, without a
second gate, because each correction states what the reviewer already assumed:

1. D1 said no existing test changes; the tests of `start_failure` in
   `src/delivery.rs:579-616` read its result as a `DeliveryError` and change to
   read a `StartFailure`, assertions unchanged.
2. AC-4 needs doctor reachable from a test: `herdr_finding_at(binary)` is added,
   after `daemon_finding_at` (`src/doctor.rs:156`).
3. `wait_until_executable` takes a deadline, so the Linux-only test can hold the
   handle past a short one.

### S3 Plan
- artifact: `PLAN_66.md`
- produced: 2026-10-06

D1 in `DESIGN_66.md` was reworded before the S3 gate: the three sentences are
three functions in `src/delivery.rs` that the three error types print, rather
than the `Display` of `StartFailure`. Behaviour is the same; the plan follows the
reworded text.

```yaml
gate:
  stage: S3
  artifact: PLAN_66.md
  reviewer: implementer
  verdict: QUESTIONS
  date: 2026-10-06
  questions:
    - Task 5, Step 2, test `a_script_still_open_for_writing_is_waited_for_and_not_reported_as_busy`, contradicts Step 5 on macOS. The test asserts `released.load(...)` is true right after `wait_until_executable` returns. The test's own `else` branch and the module doc say macOS executes a file that is open for writing. So on macOS the probe succeeds on its first try with 0 retries and the call returns at once. At that moment the releaser thread is still inside its 400 ms sleep, so `released` is false and the assertion fails. Step 5 expects `test result: ok. 4 passed` on macOS, which this test makes impossible. I cannot tell which of the two is wrong. Either the `released` assertion is meant to be Linux-only, like the `retries >= 1` branch, or the expected macOS result is not 4 passed.
  blocker: null
```

Answer, from the design: the `released` assertion is Linux-only. `DESIGN_66.md`
says the test asserts the call returned only after the release on Linux, and
asserts zero retries on macOS. `PLAN_66.md` Task 5 was corrected to read
`released` into a variable and assert it inside the Linux branch only. The
expected macOS result stays 4 passed.

```yaml
gate:
  stage: S3
  artifact: PLAN_66.md
  reviewer: implementer
  verdict: READY
  date: 2026-10-06
  questions: []
  blocker: null
```

Reviewer's note, not a question: Task 5 Step 2 says to include only the tests
module and two `use` lines first; the full file content shown below that
sentence is what to write.

### S4 Implement
- started: 2026-10-06
- executing: `PLAN_66.md`, Tasks 0 to 10, by the run itself with
  `superpowers:executing-plans` and `superpowers:test-driven-development`.
  One cargo process at a time, `CARGO_BUILD_JOBS=6`.

Ledger (S4):
- Task 0: complete (e0e0410, run artifacts committed).
- Task 1: complete (5e263e5; `cargo test --bin herdr-voice delivery` → 34 passed; RED before: `cannot find type StartFailure`).
- Noticed, not changed: `bias::pane::read` (`src/bias/pane.rs:67`, `Err(_) => PaneError::NotFound`) reports every failure to start herdr as "install herdr, or set HERDR_BIN_PATH". Same defect as #101, in a place #101 and `AC_66.md` do not name. To be raised with the orchestrator for a separate issue.
- Environment: macOS has no `timeout` binary; loops in S5 use a `perl -e 'alarm N; exec @ARGV'` wrapper.
- Task 2: complete (43fead5; `cargo test --bin herdr-voice setup::` → 78 passed; RED before: no variant NotExecutable/StartFailed).
- Task 3: complete (4eb9791; `cargo test --bin herdr-voice indicator::` → 40 passed; RED before: no variant NotExecutable/StartFailed).
- Task 4: complete (3c1ab35; `cargo test --bin herdr-voice doctor::` → 80 passed; RED before: herdr_finding_at and cannot_run_herdr undefined).
- Task 5: complete (b76cb11; `cargo test --bin herdr-voice script_fixture` → 4 passed on macOS, the Linux-only deadline test is not compiled here; RED before: write_executable and wait_until_executable undefined).
- Task 6: complete (cceb8b1; `cargo test` → 719 passed, 1 ignored; the five unix fixtures call write_executable, no set_mode(0o755) left in them).
- Task 7: complete (30a249a; `cargo test --bin herdr-voice stt::fetch` → 10 passed; RED before: no method `finish` on JoinHandle).
- Tasks 8 and 9: complete (f04d6e7; `cargo test --bin herdr-voice stt::fetch` → 11 passed).
  Ruling: Tasks 8 and 9 were committed together, not as two commits — both edit the same test module and were applied in one edit; cost if wrong: one commit to split.
  Mutation of #48 (production line `entry.repo, entry.revision, file.name` in `one` changed to `entry.repo, "main", file.name`, nothing else): `cargo test --bin herdr-voice stt::fetch` → exit 101, 9 passed, 2 failed: `every_file_is_requested_at_the_revision_the_entry_pins` and `a_request_the_table_does_not_hold_is_answered_404_and_recorded`, left `/openai/whisper-fixture/resolve/main/model.safetensors`, right `.../resolve/0000000000000000000000000000000000000000/model.safetensors`; mutation reverted.
  First attempt of that mutation used `sed` over the whole file, which also rewrote the test's own expected-path line; the pin test then passed because the test was mutated along. Caught by reading the per-test output; redone on line 156 only.
- Task 10: complete (HEAD f04d6e7). `cargo test` → 721 passed, 1 ignored (+4, +4, +2 in the other test targets), exit 0; `cargo clippy --all-targets -- -D warnings` → exit 0; `cargo fmt --check` → exit 0; `python3 scripts/check_manifest.py` → exit 0, "12 entries, all commands known"; Windows dead-code check on a scratch copy (shared target dir under TMPDIR, copy deleted) → clippy exit 0. `.leakwords` non-empty, `core.hooksPath` = `.githooks`.

### S4 code review
- reviewer: a fresh general-purpose subagent on the most capable model, over `git diff 3dd45b8..HEAD -- src`; it ran `cargo test`, `cargo clippy --all-targets -- -D warnings`, `cargo fmt --check` on macOS, all clean. Critical: none. Important: none. Minor: 8, below. The reviewer found the "probe once, then safe" argument sound on Linux (the exec permission check precedes the write-access check, so non-executable fixtures are not exposed; `ETXTBSY` is 26 on Linux x86_64, aarch64 and on macOS).
- Re-graded by effect, then fixed in one pass (commit ead3788, whole suite 723 + 4 + 4 + 8 passed, clippy, fmt and the Windows dead-code check clean after it):
  - Final: fixed minor 1, graded Important by effect: `tests/setup_process.rs` writes and runs a script in its own test crate, the sixth file of the class #66 names (the AC searched `src/` only, so it found five). It includes `src/script_fixture.rs` by `#[path]` and calls `write_executable`; the helper's own six tests now also run in that crate. Ruling: no test can fail first for this, `ETXTBSY` cannot be produced on macOS; verified by the suite and by CI — cost if wrong: that fixture keeps a narrow window, much smaller than the unit tests' because a process start sits between its close and its exec.
  - Final: fixed minor 2, graded Important by effect: `serve` ended its thread on any `accept` error other than WouldBlock, which is the `Connection refused` symptom of #62 again. It now sleeps 5 ms and continues; only `finish` ends the loop. Ruling: no test, an `accept` error cannot be provoked deterministically — cost if wrong: a late client could still be refused after such an error, as before.
  - Final: fixed minor 3 — `a_script_for_another_interpreter_is_refused` RED→GREEN: `write_executable` accepts only `#!/bin/sh`, the guard line being POSIX shell; a one-line input now reaches the `#!` assertion instead of an unrelated panic.
  - Final: fixed minor 4 — `a_file_that_already_exists_without_the_execute_bit_is_made_executable` RED→GREEN (RED: `Permission denied (os error 13)`): the mode is set on the open descriptor.
  - Final: minor (deferred) 5: the "This is often temporary: try again" sentence of `start_failed_message` is wrong for a permanent error such as `ENOEXEC` (a herdr built for another architecture). It comes from #78's wording in `src/delivery.rs` and now reaches setup, the indicator and doctor. Wording is a user-visible text: for the orchestrator.
  - Final: minor (deferred) 6: doctor prints the state `missing` next to "was found but this process is not allowed to run it"; `AC_66.md` keeps `State::Missing` on purpose. Also doctor does not quote the binary path as the other three do. For the orchestrator.
  - Final: minor 7 handled in S5: the module doc of `src/script_fixture.rs` cites `docs/evidence.md`, "Text file busy in test fixtures"; the S5 section heading begins with exactly that text.
  - Minor 8 is information: the held-handle test asserts waiting only on Linux CI; the `any_other_failure…` tests check the mapping and the wording, not a real spawn failure.

### S4 mutation testing
- tester: a second fresh general-purpose subagent, run after the reviewer had finished, in an rsync scratch copy with one shared target directory, `CARGO_BUILD_JOBS=6`, one cargo process at a time (its driver ran in the background but sequentially; the scratch copy was deleted when S4 ended). Baseline green. 109 mutations over the lines the diff adds or changes: 89 killed, 20 survived, none failed to compile.
- Survivors, and what became of each:
  - Production code, killed by new tests in commit 7919dc0 (each re-applied by hand against the new tests: killed by the test named): I08 `PaintError::NotFound` printed the wrong sentence → `indicator::tests::a_herdr_that_is_not_found_says_so_and_names_the_path_and_the_variable`; S04 `HerdrError::NotFound` dropped `path` → `setup::tests::herdr_cli::a_binary_that_cannot_be_started_is_reported_as_not_found` (now asserts the path); O11 doctor named the wrong program → the two doctor herdr tests (now assert the program is named).
  - Test helpers, killed the same way: F07 guard `exit 0` → `exit 1` (`wait_until_executable` now panics when the probe does not exit successfully; killed by the fixture and call-site tests); F18 `truncate(false)` → `a_file_that_already_exists_without_the_execute_bit_is_made_executable` (the stale file is now longer than the script); F20 retry condition inverted → `a_script_that_cannot_be_executed_for_another_reason_panics_at_once` (now bounds the time to 5 s with `catch_unwind`); H02 `Server` drop → `dropping_the_server_closes_the_listener`; H05 read timeout removed → `a_client_that_connects_and_never_sends_does_not_hold_finish_forever` (`serve_with` takes the timeout); H14 timeout 1 ms → `a_client_that_sends_late_is_still_answered_and_recorded`.
  - Ruling: O10 (`herdr_finding()` ignoring `HERDR_BIN_PATH`) is left alive — a test would have to set an environment variable the parallel suite shares, which this crate's tests avoid on purpose; `herdr_finding` is one line of glue to `herdr_finding_at`, which is tested — cost if wrong: a regression that stops doctor honouring `HERDR_BIN_PATH` is not caught by a test.
  - Ruling: H06 (the accept-error arm of `serve`) is left alive — an `accept` error cannot be provoked deterministically — cost if wrong: that arm could regress to ending the loop.
  - Equivalent or timing-only, left alive: F09 (`.mode(0o755)` on creation, overridden by `set_permissions` on the descriptor; kept so the file is never visible without the execute bit), F05 (the 5 ms sleep in the retry loop, no observable effect), F02 (`<` against `<=` at a one-nanosecond boundary), H16 (a mutation of the port-0 test itself: ports 0 and 1 are both refused).
  - Killable on Linux only, not on this machine: F01 (`ETXTBSY` = 27), F03, F04, F13 — the Linux branch of `a_script_still_open_for_writing_is_waited_for_and_not_reported_as_busy` kills them on CI; F12 and F19 (`write_executable` not waiting, or a zero deadline) — the new `write_executable_waits_for_a_descriptor_held_elsewhere_on_the_file` asserts the wait on Linux only. None of these can be shown killed until the Linux CI run.
- After the fixes: `cargo test` → 728 + 4 + 4 + 9 passed, 1 ignored; clippy, fmt, `check_manifest.py` and the Windows dead-code check clean.
