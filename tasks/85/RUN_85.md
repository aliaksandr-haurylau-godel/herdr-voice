# RUN_85

| field | value |
|---|---|
| issue | #85 — Everything the plugin has to say goes through a channel that can be silently off |
| input | GitHub issue, read with `gh issue view 85` |
| stage | S1 |
| branch | fix/85-notification-channel |
| opened | 2026-09-30 |

## Stages

<!-- One block per stage, appended, never rewritten. -->

### S1 Assess
- artifact: `AC_85.md`
- produced: 2026-09-30

The issue requires that the means of asking herdr where notifications go is
established by running herdr. It was, on herdr 0.9.1, on an isolated server with
its own `HERDR_CONFIG_PATH` and socket; the commands, the outputs and the
conclusion are in `AC_85.md` under "as-is". The owner's running server and
configuration were only read, never changed.

herdr exposes no command or API method that reports the setting. This was
established by running it, so the configuration file is the means, and that is
what "rests on what herdr actually reports" comes down to. The four probe results
go into `docs/evidence.md` in S5.

The isolated server writes its log to the owner's herdr log regardless of
`HERDR_CONFIG_PATH`, so a few lines from the probes are in it. They are
harmless and were not cleaned out.

Name `notifications`, states and wording were proposed to the orchestrator and
answered: `terminal` and `system` are a warning, `off` and unknown are `missing`;
the wording says what is configured, never what is in effect. They are put to the
owner and may change; only the strings and their tests would.

## Notes

- Input came from GitHub: the issue has no comments.
- Toolchain and baseline for S4 are recorded when S4 starts.

```yaml
gate:
  stage: S1
  artifact: AC_85.md
  reviewer: designer
  verdict: READY
  date: 2026-09-30
  blocker: null
```

Reviewer notes, recorded as returned (none blocks the design):

1. The issue text was not among the files the reviewer was given; it checked the
   AC against the source only, including the reading of the out-of-bounds clause
   that requirement 5 rests on. The orchestrator answered the states and the
   wording; the reading of the clause is written into `AC_85.md` and goes to the
   owner with the name and wording.
2. Requirement 5 says every non-`ok` line names `[ui.toast] delivery = "herdr"`
   and `herdr server reload-config`; requirement 4 says herdr's setting does not
   matter under `unused`. The design resolves it: the `unused` line carries
   neither string.
3. The as-is claim "every message goes through `toast`" is inaccurate: the
   refused-delivery report (`src/daemon.rs:1003`) and "Rewrite unavailable"
   (`src/daemon.rs:1279`) call `notify` directly. Both check `[ui] toasts` first,
   so requirement 4's precedence holds, and the change stays in `src/doctor.rs`.
4. `warning` is a new `State`; `render` pads the name to 8 characters and
   `notifications` has 13; `config_path()` returns `None` when no variable is
   set. The design settles all three.

### S2 Design
- artifact: `DESIGN_85.md`
- produced: 2026-09-30

```yaml
gate:
  stage: S2
  artifact: DESIGN_85.md
  reviewer: planner
  verdict: QUESTIONS
  date: 2026-09-30
  questions:
    - >-
      The design's wording for three `missing` lines (location unknown, unreadable
      file, unparsable file) does not contain what AC-1 and requirement 5 of
      AC_85.md require -- `[ui.toast]`, `delivery = "herdr"`, `herdr server
      reload-config` and the plugin log -- and the design gives no reason for
      leaving it out. The test task for AC-1 and the task that writes these
      strings would contradict each other. The design must either give full
      wording for these three lines that meets AC-1 and requirement 5, or record
      with a reason that they leave the strings out, and change the acceptance
      criterion with it.
  blocker: null
```

Answer: the strings belong on those lines too. A person who cannot get herdr's
file read still needs to know what to set once it is. `DESIGN_85.md` is revised
to give full wording for all three, built from one shared tail, and the
`unused` line remains the one recorded exception. The note on requirement 1
(own resolver instead of `config_path()`) is recorded in the design already.

```yaml
gate:
  stage: S2
  artifact: DESIGN_85.md
  reviewer: planner
  verdict: READY
  date: 2026-09-30
  blocker: null
```

Reviewer notes, none blocking: the `terminal`/`system` row could be built from its
own sentence plus the log command or plus the full tail (the plan fixes it to the
full tail); an absent file counts as `off` without checking that its directory
exists, which is a stated choice.

### S3 Plan
- artifact: `PLAN_85.md`
- produced: 2026-09-30

```yaml
gate:
  stage: S3
  artifact: PLAN_85.md
  reviewer: implementer
  verdict: QUESTIONS
  date: 2026-09-30
  questions:
    - "Task 6 says 'the Windows dead-code check of `BRIEF.md`'. There is no BRIEF.md in the repository and no task or file gives the command. Please state the exact command, or the file and section that holds it."
    - "Task 4 requires that 'no detail contains a newline'. The `Unparsable` detail interpolates `{reason}`, which is `toml::de::Error::to_string()` from Task 3. The toml crate's error text is multi-line: it includes the source line and a caret. Following Task 3 as written, the not-TOML row would fail this test. Please say what Task 3 or Task 4 must do with a multi-line reason."
    - "Task 4 requires that 'no detail contains ... the word \"now\"'. Every `Missing` row contains 'not known where' or 'not known at all', and 'known' contains the substring 'now'. It is unclear whether the assertion is a substring check (which fails on the plan's own fixed strings) or a whole-word check."
    - "Task 4 says 'Depends on: 1, 3', but its signature uses `Location`, which Task 2 defines. Please add 2 to the dependency list."
```

Answers, all made in `PLAN_85.md`: (1) the command is written out in task 6;
(2) the reason is cut to its first line by a helper in task 4; (3) the "now"
assertion is dropped and replaced by the phrase "in effect"; (4) the dependency
list is corrected.

```yaml
gate:
  stage: S3
  artifact: PLAN_85.md
  reviewer: implementer
  verdict: QUESTIONS
  date: 2026-09-30
  questions:
    - "The plan names the state `Unused`; `src/doctor.rs:30` has no `Unused`. The variant is `State::NotUsed`, whose word is already `unused`. Please state which to use."
    - "Task 7 says to record 'the four probe results of `AC_85.md` as-is 2 and 3'. It does not say which four results are meant, and gives no commands for them; they need an isolated herdr server. Please say which results go into `docs/evidence.md`, and give the commands."
```

Answers, made in `PLAN_85.md`: the existing `State::NotUsed` is used and no
variant is added; task 7 now lists the results and carries the probe script and
the commands.

```yaml
gate:
  stage: S3
  artifact: PLAN_85.md
  reviewer: implementer
  verdict: QUESTIONS
  date: 2026-09-30
  questions:
    - "Task 5 says to `push` the notifications finding 'right after `daemon_finding()`'. In `src/doctor.rs:416`, `daemon_finding()` is an element inside the `vec![herdr_finding(), daemon_finding(), config_finding(&loaded)]` literal, so a `push` puts `notifications` last, which contradicts 'after daemon' in Task 7. Please say which is meant. The plan also uses `&location` but never says where `let location = herdr_config_location();` goes, and the snippet has no trailing semicolon."
    - "Task 3's test inputs are ambiguous. `delivery = \"Herdr\"` and `delivery = 3` are written without a `[ui.toast]` header, so the lookup would return `Absent`. The same question applies to the 'each of the four values' tests. Task 4 also feeds `read_delivery(\"delivery = \")` and expects `Unparsable`; please confirm that it works only because the text is invalid TOML."
```

Answers, made in `PLAN_85.md`: the finding goes into the `vec!` literal between
`daemon_finding()` and `config_finding(&loaded)`, with `location` bound before
the literal; every Task 3 input carries the `[ui.toast]` header; the broken input
is `"[ui.toast]\ndelivery = \n"`, invalid TOML by design.

```yaml
gate:
  stage: S3
  artifact: PLAN_85.md
  reviewer: implementer
  verdict: READY
  date: 2026-09-30
  blocker: null
```

### S4 Implement
- started: 2026-09-30

Baseline on `main` at `13733c5`: `cargo test` 571 passed, 0 failed, 1 ignored.
After the change: 616 unit tests and 4 + 2 integration tests passed, 0 failed;
`cargo clippy --all-targets -- -D warnings`, `cargo fmt --check` and
`python3 scripts/check_manifest.py` clean; the Windows dead-code check, run on a
scratch copy of the tree, clean. No intermittent test (#62, #66) failed in any run.

Code was written in the plan's task order with the tests first and watched to
fail. Tasks 2 and 3 were written as one red step, tasks 4 and 5 each on their own.

### S4 review of the diff (fresh subagent, did not write the code)

Reported, and what was done:

1. An unlisted value spanning lines (`delivery = """a<newline>b"""`, an array
   or a table) produced a detail spanning lines. Reproduced by a test that failed;
   fixed: a string is shown escaped, any other value cut to its first line.
2. Wording: the `off` and absent-key lines stated an outcome ("will not appear")
   that the probes contradict (with a focused client herdr answers `shown` for
   `off`); now "are not expected to appear". The `ok` line now says "is configured
   to draw". The unknown-value row says "may not appear" (kept, more careful than
   the design table) and the absent-file row says "no file at" (as in the plan).
3. A malformed parent key (`ui = 3`, `toast = "x"`) reads as an absent key. Kept:
   the plan specifies it and a test pins it.
4. The path resolver repeats `src/setup.rs::config_path_from` for non-Windows. Kept,
   as the design decides, because `setup` has no Windows branch.
5. `scratch_dir` leaves its directory in the temporary directory. Kept, the same
   pattern as the tests in `src/setup.rs`.

### S4 mutation test (fresh subagent, own worktree of `b41e5f9`)

113 mutations ran: 83 killed, 30 survived. The survivors were: a column width
(1); strings of `notifications_finding` and `warning` that no test asserted
(about 15); `herdr_config_location`, which no test called (6); two branches of
`read_herdr_config` (2); the wiring in `run()`, which no test covered (6); one
equivalent mutation (a no-op line); and `cfg!(windows)` replaced by `false`.

Killed by new tests: `herdr_config_location` was split into
`herdr_config_location_from(lookup, windows)` and tested with a lookup;
`tests/doctor_process.rs` runs the built binary and asserts the order of the eight
lines, the state for each herdr value, and `[ui] toasts = false` from the plugin's
configuration; and unit tests assert each sentence the survivors changed. Seven of
the survivors were re-applied one by one to the final code (`!toasts`, the order,
the file read replaced, the `nowhere` sentence, `one_line` dropped from a read
failure, an environment name, the column width) and each was killed.

Not killed, and why: the equivalent mutation changes no behaviour; `cfg!(windows)`
replaced by `false` cannot be told apart from the original on macOS, where it is
already false, and needs a Windows run.

The mutation worktree registered in the shared repository (`git worktree add`, at a
scratch path outside this checkout) was removed by `git worktree remove` when the
run ended; `git worktree list` no longer shows it and its build directory is gone.

### S5 Verify
- artifact: section "Where herdr sends notifications, and what `doctor` says about
  it, for issue #85" in `docs/evidence.md`
- verdict: recorded. `doctor` ran on this machine (macOS, herdr 0.9.1) against the
  owner's configuration read only (`delivery = "herdr"`, line `ok`, exit 0) and
  against scratch files holding `terminal`, `system`, `off`, `herdr`, an empty file
  and no file. The probes behind the design were re-run cleanly on an isolated
  server: five results, in the same section.
- not verified: a toast actually appearing under any value; Windows; the running
  server's own setting (`doctor` reads the file).
- the isolated server was stopped with `kill` on its own process; the owner's
  server (started earlier, other socket) was checked afterwards and was running.

### CI after the pull request was opened
- run `36707438104` (push): job `ubuntu-latest` failed with
  `delivery::tests::insert_runs_pane_send_text_not_agent_prompt` panicking at
  `src/delivery.rs:483` with `NotFound` for `record.sh`, a freshly written fixture
  script reported as not found (615 passed, 1 failed). This diff does not touch
  `src/delivery.rs`; it is the known intermittent failure of issue #78.
- The status first reported for the pull request as green was wrong: only the tail
  of `gh pr checks` had been read, and the failing line was above it.
- rerun once with `gh run rerun 36707438104 --failed`; the job then passed and the
  run's conclusion is `success`. The run for the pull_request event
  (`36707464227`) passed on its first attempt.
