# RUN_97

| field | value |
|---|---|
| issue | #97 — The leak gate skips the last entry of .leakwords when the file has no final newline |
| input | GitHub issue, read with `gh issue view 97 --comments` (no comments; the body carries the reproduction) |
| stage | S1 |
| branch | fix/97-leakwords-last-line |
| opened | 2026-10-06 |

## Stages

<!-- One block per stage, appended, never rewritten. -->

### S1 Assess
- artifact: `AC_97.md`
- produced: 2026-10-06

```yaml
gate:
  stage: S1
  artifact: AC_97.md
  reviewer: designer
  round: 1
  verdict: READY
  date: 2026-10-06
  blocker: null
```

Reviewer note, none held back READY: every as-is claim matches the files; the issue's
suggested fix does not conflict with R3, because an unterminated comment line still
reaches the `continue` branch.

S1 is closed. Next is S2 Design.

### S2 Design
- artifact: `DESIGN_97.md`
- produced: 2026-10-06

```yaml
gate:
  stage: S2
  artifact: DESIGN_97.md
  reviewer: planner
  round: 1
  verdict: READY
  date: 2026-10-06
  blocker: null
```

Reviewer notes, none held back READY: R5 is S5 work and has no planning task; the AC's
"a blank last line without a newline" cannot exist, since a file that ends in a blank
line ends in a newline, so nothing needs a task for it.

S2 is closed. Next is S3 Plan.

### S3 Plan
- artifact: `PLAN_97.md`
- produced: 2026-10-06

```yaml
gate:
  stage: S3
  artifact: PLAN_97.md
  reviewer: implementer
  round: 1
  verdict: READY
  date: 2026-10-06
  blocker: null
```

Reviewer notes, none held back READY: the `fail` helper prints two spaces after `FAIL` and
longer messages than the plan quotes (which checks fail, and the count of four, are right);
the Windows dead-code check is skipped because the diff touches no Rust, and this file
says so in S4.

S3 is closed. Next is S4 Implement.

### S4 Implement
- code: `.githooks/pre-commit` (the loop condition), `scripts/test-pre-commit.sh` (cases 11 to 14)
- red step: against the unchanged hook, cases 11 and 12 printed four `FAIL` lines
  (`exit 0, expected 1` and the missing match message, for each) and the script ended with
  `4 check(s) failed`, exit 1; cases 13 and 14 passed, as designed.
- green step: `sh scripts/test-pre-commit.sh` prints 32 `ok` lines and no `FAIL`, exit 0.
- gates: `cargo test` (708 unit tests and the process tests, all passing), `cargo clippy
  --all-targets -- -D warnings`, `cargo fmt --check`, `python3 scripts/check_manifest.py`,
  all passing. The Windows dead-code check is skipped: the diff touches no Rust.
