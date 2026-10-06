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
