# RUN_52

| field | value |
|---|---|
| issue | #52 — The http rewrite engine reports every failure as an unreachable server, and discards the reason. This run also closes #78 — Every failure to start herdr is reported as "not on the PATH" |
| input | GitHub issues, read with `gh issue view 52` and `gh issue view 78`; neither has comments |
| stage | S5 |
| branch | fix/52-78-failure-causes |
| opened | 2026-09-30 |

## Stages

### S1 Assess
- artifact: `AC_52.md`
- produced: 2026-09-30

```yaml
gate:
  stage: S1
  artifact: AC_52.md
  reviewer: octoflow-reviewer-designer
  verdict: READY
  date: 2026-09-30
  blocker: null
```

Reviewer notes that did not hold back READY, and how the run answers them:
1. The reviewer could not run `gh issue view`; R1 to R6 were checked against the code only. The issue text was read by the author of the AC, and both bodies are quoted in `AC_52.md` where they matter.
2. AC-14 names the Windows dead-code check. Its command, so the plan can carry it:
   ```sh
   find src -name '*.rs' -exec sed -i '' \
     -e 's/#\[cfg(all(test, unix))\]/#[cfg(any())]/g' \
     -e 's/#\[cfg(unix)\]/#[cfg(any())]/g' \
     -e 's/#\[cfg(not(unix))\]/#[cfg(not(any()))]/g' \
     -e 's/#\[cfg(windows)\]/#[cfg(not(any()))]/g' {} +
   cargo clippy --all-targets -- -D warnings
   git checkout -- src
   ```
   Its edits are never committed.
3. A test double that never answers waits out the full 30-second bound. The design must let a test set a shorter bound without changing the shipped value.
4. The same defect exists in `src/indicator.rs` (`PaintError::NotFound`, line 227) and `src/setup.rs` (`HerdrError::NotFound`, line 493), besides the three places #78 lists. Both stay out of scope and are named in the pull request as remaining.

### S2 Design
- artifact: `DESIGN_52.md` (evidence in `DESIGN_52_evidence.md`)
- produced: 2026-09-30

```yaml
gate:
  stage: S2
  artifact: DESIGN_52.md
  reviewer: planner
  verdict: READY
  date: 2026-09-30
  blocker: null
```

### S3 Plan
- artifact: `PLAN_52.md`
- produced: 2026-09-30

```yaml
gate:
  stage: S3
  artifact: PLAN_52.md
  reviewer: implementer
  verdict: QUESTIONS
  date: 2026-09-30
  questions:
    - "Task 2 'Done when' says `grep -n \"Refused\" src/stt/http.rs` must print nothing. Task 2 Step 1 adds `cause: Cause::ConnectionRefused` to `a_connection_that_refuses_is_named_by_address` in that same file, and the substring `Refused` is part of `ConnectionRefused`. The grep will print that line, so I cannot meet the done condition as written. Which check is meant? For example `grep -n 'HttpError::Refused'`, or a word-boundary match."
    - "Task 3 Step 1 builds `std::io::Error::new(ErrorKind::Other, \"Text file busy\")` in two tests: `start_failure_reads_the_kind_the_operating_system_gave` and `another_failure_to_start_carries_...`. Current stable clippy warns on this pattern with `clippy::io_other_error`, which tells you to use `io::Error::other`. The plan requires `cargo clippy --all-targets -- -D warnings` to print nothing after every task. Under `-D warnings`, Step 4 of Task 3 would fail on those two lines, and Task 4 Step 1 would fail with it. The repository has no other `ErrorKind::Other` use and `Cargo.toml` has `rust-version = \"1.82\"`. I cannot tell whether the plan wants `io::Error::other(..)`, or an `#[allow]`, or a pinned toolchain that lacks the lint. Please state which, so I do not have to decide how to change the test code."
  blocker: null
```

Answers, applied to `PLAN_52.md`: the done check is now
`grep -n "HttpError::Refused" src/stt/http.rs`; both tests use `io::Error::other`,
which `rust-version = "1.82"` supports (stable since 1.74).

```yaml
gate:
  stage: S3
  artifact: PLAN_52.md
  reviewer: implementer
  verdict: READY
  date: 2026-09-30
  blocker: null
```

The second round's one wording remark, which did not hold back READY: in Task 1 Step 2 two
tests pass against the stub, because the stub returns `ConnectionRefused`. The
implementation makes the stub return `Cause::Other` so that every test in the module
fails first.

## Notes

- Scope: `src/rewrite/http.rs` (#52), `src/delivery.rs` (#78) and `src/stt/http.rs`,
  which carries the same message as #52 and is named by no issue. The remaining
  places #78 lists — `src/stt/command.rs`, `src/rewrite/command.rs`,
  `src/doctor.rs` — stay out of scope and are named in the pull request as
  remaining.
- #66 (an indicator test that fails intermittently on Linux) is not part of this
  run. If the change makes that failure readable, this file says so at S5.
- `.claude/agents/` holds the planner and implementer reviewers but no file for
  `octoflow-reviewer-designer`, which `CLAUDE.md` names for S1. The session's agent
  list offers an agent of that name, and the S1 gate uses it.
- After the S2 verdict, one sentence of `DESIGN_52.md` was trimmed: the advice for a
  status below 500 no longer ends with "; the explanation says which", because it
  would follow "the server gave no explanation" for an empty body. No behaviour
  or acceptance criterion depends on the removed words; the gate is not re-run.
- 2026-09-30, during S5 against a live endpoint: the server reports a wrong path as
  HTTP 200 with `{"error":"Unexpected endpoint or method. …"}`, and the engine dropped
  that sentence. The orchestrator decided the fix belongs in this pull request. The
  acceptance criteria got a dated amendment (AC-17 to AC-20); the amendment, and the
  design and plan additions for it, go through the gates again below.

### Amendment 2026-09-30 (AC-17 to AC-20): gate rounds

```yaml
gate:
  stage: S1
  artifact: AC_52.md (Amendment 2026-09-30, AC-17 to AC-20 and R10)
  reviewer: octoflow-reviewer-designer
  verdict: READY
  date: 2026-09-30
  blocker: null
```

Reviewer notes that did not hold back READY: a body of only whitespace is treated as
empty, because the existing excerpt already turns it into an empty string; the design
reads the body as text before parsing it so that a long successful completion still
comes through whole; the timeout while reading a 2xx body stays out of scope and is
named in the pull request.

Planner note on the design amendment, checked in the `ureq` 2.12.1 source
(`src/response.rs`): `into_json` reads without a limit, and `into_string` stops at 10 MB
with an error. The design and the plan therefore read the body with
`into_reader().read_to_string`, which has no limit, so that a large successful body is
read exactly as before (AC-20). The gate is not re-run for this wording.

```yaml
gate:
  stage: S2
  artifact: DESIGN_52.md (Amendment 2026-09-30, AC-17 to AC-20)
  reviewer: planner
  verdict: READY
  date: 2026-09-30
  blocker: null
```

```yaml
gate:
  stage: S3
  artifact: PLAN_52.md (Amendment 2026-09-30, Task 5)
  reviewer: implementer
  verdict: READY
  date: 2026-09-30
  questions: []
  blocker: null
```

### S4 Implement
- code: commits `ef3c688`, `b414835`, `7f7a68c` (Tasks 1 to 3), `a5fb539` (review fixes), `3686e4e` (Task 5, the amendment)
- tests: 624 unit tests and 2 integration tests, all passing; clippy with `-D warnings`, `cargo fmt --check` and `scripts/check_manifest.py` clean; the Windows dead-code check clean, run on a scratch copy of the tree

**Review of the whole diff** (a fresh subagent that did not write the code, over `13733c5..7f7a68c`, before the fix pass and the amendment):

```yaml
review:
  stage: S4
  reviewer: general-purpose subagent, opus, fresh context
  range: 13733c5..7f7a68c
  verdict: no Critical, no Important
  date: 2026-09-30
  findings:
    - {grade: Minor, n: 1, what: "timeout test doubles block in accept with no bound; a timeout that fires before the connect would hang the suite at join"}
    - {grade: Minor, n: 2, what: "408 and 429 get the advice for a wrong address, model or token"}
    - {grade: Minor, n: 3, what: "ErrorKind::NotFound for a program named by an absolute path still says it is not on the PATH"}
    - {grade: Minor, n: 4, what: "a timeout while reading a 2xx body is still reported as unreadable"}
    - {grade: Minor, n: 5, what: "some acceptance criteria not pinned as written: excerpt length (AC-4), transcriber status other than 400 with its body (AC-3/AC-8), refusal not worded as an answer (AC-6), URL in transcriber tests (AC-7)"}
    - {grade: Minor, n: 6, what: "wav_path leaves files in the temp directory (older pattern, four new tests use it)"}
    - {grade: Minor, n: 7, what: "Cause::Other names the URL twice (ureq's own text; older)"}
```

Rulings on the findings:
- 1, 2, 5: fixed in `a5fb539`. Finding 2 is graded by effect, not by the spec's silence: a rate-limited person is told to edit a correct configuration, which is the defect of this issue. It has a test that failed first (`a_request_to_try_again_later_is_told_to_wait_not_to_edit_the_configuration`); `DESIGN_52.md` was updated to say so. Finding 1 has no failing test, because the hang cannot be produced on demand; the change is a bounded accept. Finding 5 adds assertions that pass at once, because they pin behaviour that was already right.
- 3: not fixed. The wording for an absolute path predates this change and is within the letter of AC-10. Named in the pull request as remaining.
- 4: not fixed; out of scope by the amendment. Named in the pull request as remaining.
- 6, 7: not fixed. Both predate this change; deferred.

**Flaky tests during this run:** none failed.

**On #66 and the delivery flake** (orchestrator's evidence, 2026-09-30: an `ubuntu-latest`
run of another branch failed in `insert_runs_pane_send_text_not_agent_prompt` with
`NotFound { binary: "<tmp>/record.sh", … }` for a script the test had just written).
- The word `NotFound` in that output comes from the old blanket `Err(_)` arm, not from the
  operating system, so the output could not say what the error was. That is the defect
  this change closes.
- With this change the same failure is reported by the kind the system gave. An error of
  kind `NotFound` still prints the `PATH` sentence; `PermissionDenied` prints the
  not-executable message; anything else prints `cannot run "<binary>": the operating
  system reported "<text>"`, so a file still open for writing would read as `Text file
  busy (os error 26)` on Linux. That last sentence is an expectation, not a measurement:
  exec of a script held open for writing did not fail on this machine (macOS, tried with
  a script scratch file), so `ETXTBSY` could not be produced here, and the Linux run was
  not repeated. The next flaky failure of that test on `main` after this lands will name
  its cause in the log; until one does, which kind it is stays unknown.
- The fixture was not touched, and #66 was not chased.

**Mutation test** (a second fresh subagent that did not write the code; it mutated the lines of the diff one at a time and ran `cargo test`): 107 mutations, 91 killed, 16 survived — 83 tried on `7f7a68c` (71 killed, 12 survived) and 24 on the commits added afterwards, `a5fb539` and `3686e4e` (20 killed, 4 survived). Every survivor, and what was done:

| Survivor | Outcome |
|---|---|
| `BODY_READ_BYTES` 1024 or 4096, and the `.take` dropped | killed by `a_refusal_body_is_read_only_up_to_its_byte_bound` (both engines) |
| `io_kind` looks only at the first source | killed by `an_io_error_deeper_in_the_chain_is_found` |
| `io_kind` starts at the error itself | not killed, on purpose: the top-level `ureq::Transport` is never an `io::Error`, so the mutation changes nothing a caller can see |
| the transport detail becomes empty | killed by the added assertion in `a_server_that_closes_without_answering_is_neither_a_timeout_nor_a_refusal` |
| the agent built with `TIMEOUT` instead of the passed bound (both engines) | killed by the elapsed-time assertion in both `a_reply_that_never_comes_is_a_timeout_naming_the_bound` |
| `AudioUnreadable` path and detail swapped; its detail dropped from the message | killed by the start and `os error 2` assertions in `a_missing_wav_file_is_named_as_the_file_not_as_the_server` |
| `NotFound` with an empty `path` | killed by the `PATH` assertion in `a_program_that_does_not_exist_keeps_the_path_sentence` |
| a parse error without its text (both engines) | killed by the `line 1 column` assertion in both `a_2xx_body_that_is_not_json_is_quoted` |
| a read error without its text (both engines) | killed by `a_body_cut_off_mid_read_names_the_read_failure` (both engines) |
| `Unreadable`'s Display dropping its detail (survived on `7f7a68c` only) | killed by the `a_2xx_*` tests already |

Each of these killing tests was checked by applying the mutation again after the tests were written, and each failed; the source was restored after every mutation. The tests are in commit `3d99627`. The mutation tester's first run stalled in the background, so it was resumed twice; its one unreliable reading (`rw-parse-noe`, reported killed with no failing test named) was counted as a survivor and is among the parse-error ones above.

**Final state of S4:** 630 unit tests and 2 integration tests pass; clippy with `-D warnings`, `cargo fmt --check`, `scripts/check_manifest.py` and the Windows dead-code check (on a scratch copy) are clean.

### S5 Verify
- section: "Failure causes named by the HTTP engines and by the start of herdr, for issues #52 and #78" in `docs/evidence.md`
- platform: macOS 26.6.2 (Darwin 25.6.0, arm64), this machine
- verdict: **passed**. Commands and outputs are beside each claim there: the old and the new build were both run against a refused port, a local server answering 400 with a body, a local server that never answers (the real 30-second bound), the owner's LM Studio on port 4000 for a wrong path (read only; the server's own sentence now reaches the person), and three kinds of `herdr` program that cannot be started.
- what it did not show, written in the section: a 4xx from the owner's LM Studio (it answered a wrong model name with 200 and a wrong path with 200, so a local server stood in for the 400); a timeout while reading a 2xx body; `ETXTBSY`; the transcriber against a live endpoint; Windows and Linux.

## Remaining, named in the pull request
- `src/stt/command.rs`, `src/rewrite/command.rs`, `src/doctor.rs` (#78 lists these), and `src/indicator.rs` and `src/setup.rs` (the same blanket mapping to `NotFound`, found by the S1 reviewer).
- A program named by an absolute path that does not exist, or whose interpreter is missing, still prints the `PATH` sentence.
- A timeout while the body of a 2xx response is being read is still reported as an unreadable answer.
- `wav_path` in the transcriber's tests leaves files in the temporary directory (older pattern); the transport's own text in `Cause::Other` repeats the URL (from `ureq`).
