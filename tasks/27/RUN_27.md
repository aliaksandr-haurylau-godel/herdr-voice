# RUN_27

| field | value |
|---|---|
| issue | #27 — doctor says "command" is ready without having checked anything about the program |
| input | GitHub issue, read with `gh issue view 27 --comments` (no comments; the body carries the argument) |
| stage | S1 |
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
