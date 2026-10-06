# RUN_27

| field | value |
|---|---|
| issue | #27 — doctor says "command" is ready without having checked anything about the program |
| input | GitHub issue, read with `gh issue view 27 --comments` (no comments; the body carries the argument) |
| stage | S5 |
| branch | fix/27-doctor-command-engine |
| opened | 2026-10-06 |

## Stages

<!-- One block per stage, appended, never rewritten. -->

### S1 Assess
- artifact: `AC_27.md`
- produced: 2026-10-06

```yaml
gate:
  stage: S1
  artifact: AC_27.md
  reviewer: designer
  round: 1
  verdict: READY
  date: 2026-10-06
  blocker: null
```

Reviewer note, not a gate question: the issue's "Done when" says "a configured external
transcriber" and its context names `command` only; the AC also changes `http`, which goes
through the same `Ok(_)` arm. The reviewer reads that as inside the issue. The
orchestrator confirmed it by message: both lines change.

S1 is closed. Next is S2 Design.

### S2 Design
- artifact: `DESIGN_27.md`
- produced: 2026-10-06

```yaml
gate:
  stage: S2
  artifact: DESIGN_27.md
  reviewer: planner
  round: 1
  verdict: READY
  date: 2026-10-06
  blocker: null
```

Reviewer notes, none held back READY: the code claims hold (`Ready` has exactly three
variants, so two named arms replace the wildcard completely; `docs/design.md` has no
`is ready` text, so section 4 of the design produces no change there); the approval of
the wording has to come before the code task. The reviewer saw only one candle check in
`the_engine_line_names_what_resolve_reports_for_each_engine`; the `ok` candle line is
pinned by `the_candle_engine_line_names_the_device` (`src/doctor.rs:1068`), which the
design means by "the existing candle line test".

S2 is closed. Next is S3 Plan.

### S3 Plan
- artifact: `PLAN_27.md`
- produced: 2026-10-06

```yaml
gate:
  stage: S3
  artifact: PLAN_27.md
  reviewer: implementer
  round: 1
  verdict: READY
  date: 2026-10-06
  blocker: null
```

S3 is closed. S4 starts when the orchestrator has answered on the wording of the two
engine lines (criterion 6 of `AC_27.md`).

### S4 Implement
- wording approved: both engine lines are final as proposed in `DESIGN_27.md` section 1 and
  `PLAN_27.md`, state `ok`: `[stt] command is set; its program is not looked for until a
  take starts` and `[stt] url is set; the endpoint is not contacted until a take starts`.
  Criterion 6 of `AC_27.md` is met.
- red step: the three new tests failed against the unchanged code (`"command" is ready`,
  `"http" is ready`). Green: `cargo test` 711 unit tests and the process tests, all passing;
  clippy, fmt, manifest and the Windows dead-code check, all passing.

Code review (a fresh general-purpose subagent over `3dd45b8..46bd712`, read-only, no
cargo): no Critical issue, one Important, three Minor, verdict "With fixes".
- Important: "until a take starts" is false. `CommandEngine::new` and `HttpEngine::new`
  look for nothing; `transcribe_take` runs the program or contacts the endpoint on the
  finished recording (`src/daemon.rs`, `engine.transcribe(&take.path, bias)`), so a person
  with a missing program records the whole take and gets the error only after it ends.
  Checked against the code. The approved wording therefore stated a false thing; it was
  changed to "until a take is transcribed" for both lines, decided by the orchestrator, and
  `AC_27.md`, `DESIGN_27.md`, `PLAN_27.md`, the two literals and the two test constants
  carry the new text. The approval recorded above was of the wording as proposed.
- Minor: the code comment said `check_with` establishes only that the key is set, and put
  the shell reason above both arms. Rewritten: it names the model, and the shell reason
  is about `doctor`.
- Minor: `neither_external_engine_line_says_ready_or_quotes_the_engine_kind` overlaps the
  two exact-text tests. Kept: requirement R2 of `AC_27.md` asks for it, and it guards a
  later rewording that updates the constants.
- Minor: each sentence exists as a literal and as a test constant. Deliberate pinning; all
  four places changed together.
- Side finding, fixed here at the orchestrator's instruction: the doc comment on
  `Ready::Http` (`src/stt.rs`) said "Its address is validated here", but `check_with` only
  checks that the url is non-empty. It now says that. It is the same unverified claim this
  issue is about, in one line, and no behaviour changes.

Mutation test (a second fresh subagent, after the review, in a scratch copy, one cargo
process at a time): 40 mutations of the two new arms and of the three new tests. Every
mutation of the code was killed except one that does not change behaviour (`Ok(stt::Ready::Http)`
replaced by `Ok(_)`, which can only match Http, since the Command and Candle arms precede it
and `Ready` has three variants). Survivors, all in the tests, and what was done:
- Removing the `State::Ok` assert from the command test, and the `is ready` and
  quoted-kind asserts from the third test: no mutation of the code depends on them; the
  exact-text tests and the older tests pin the same facts. The two text asserts are kept
  because `AC_27.md` R2 asks for them.
- Removing the `State::Ok` assert from the http test: it is the only check on the http arm's
  state (shown by the four http-state mutations, all killed by that assert). It stays.
- The program set to `sh`: passes, as designed; the line makes no claim about the program.
- Next to the diff, not in it: the text of the candle line is pinned only by "metal" or
  "cpu" in `the_candle_engine_line_names_the_device`. The candle line is out of bounds
  (`AC_27.md`), so it is left as it is.

S4 is closed. Next is S5 Verify.

### S5 Verify
- artifact: section "What `doctor` says about a configured transcriber, for issue #27" in `docs/evidence.md`
- platform: macOS (Darwin), this machine, 2026-10-07
- result: pass. `doctor` run in an empty environment with scratch configuration, before
  (`3dd45b8`) and after: `"command" is ready` for a program absent from `PATH` and for one
  present, and `"http" is ready` for a url, became the two new lines, identical for the
  absent and the present program; the empty-list `missing` line is unchanged.
- caught on the way: the first "after" binary was the one a mutation run had linked last
  into the shared target directory (case C printed `missing`); rebuilt into a directory of
  its own and rerun. The scratch copies of a run share one target directory, and the file
  `target/debug/herdr-voice` there belongs to whichever package was built last, so a binary
  to be run by hand needs a directory of its own.
- not exercised: a take with a missing program through a daemon.
