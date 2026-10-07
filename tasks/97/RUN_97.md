# RUN_97

| field | value |
|---|---|
| issue | #97 — The leak gate skips the last entry of .leakwords when the file has no final newline |
| input | GitHub issue, read with `gh issue view 97 --comments` (no comments; the body carries the reproduction) |
| stage | S4 |
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

Code review (a fresh general-purpose subagent over `3dd45b8..3aba8b6`, read-only, no cargo;
it ran `sh scripts/test-pre-commit.sh` and probes in a temporary directory): no Critical,
no Important issue; verdict "Yes". It ran the loop under `sh`, `dash`, `bash`,
`bash --posix`, `ksh` and `zsh --emulate sh`, under `set -e`, on ten inputs (unterminated
last line, terminated, empty file, lone newline, trailing blank line, unterminated comment,
spaces only, CR line endings, a last line `-n`, a last line `!`), with the same result in
all six; `shellcheck -s sh` passes on both files; no other reader of `.leakwords` exists.
Minor findings and what was done:
- This file's header said `S1`. Updated to the current stage.
- The comment of case 11 described case 12 as well. Narrowed.
- Case 12 did not check that standard output is empty. Added.
- The S5 section in `docs/evidence.md` must say that it closes the gap listed in the section
  for issue #64. It will.

Mutation test (a second fresh subagent after the review, in a scratch copy, running only
`sh scripts/test-pre-commit.sh`): mutations of the loop condition, of the skip line, of the
four new cases, and five alternative fixes. Killed: removing `|| [ -n "$pattern" ]` (cases
11 and 12, four failures), `||` to `&&`, removing the `'#'*` alternative (case 14 among
others), flipping any expected exit code, and the fixes that check the leftover line after
the loop without the comment skip (case 14 alone). Mutations that loop forever
(`-n` to `-z`, `[ -n "${pattern}x" ]`, an unquoted `[ -n $pattern ]`) are ended only by the
outer time limit; the script has none of its own, which is a suggestion outside this diff.
Survivors, and what was done:
- H6, `-r` removed from `read`: not killed. Added case 16, an entry `ordin\(a\)ry`
  (literal parentheses), which matches nothing as written and matches the staged line once
  `read` strips the backslashes. Re-applied by hand: it fails with `exit 1, expected 0`.
- H8, `[ ${#pattern} -gt 1 ]` in place of `[ -n "$pattern" ]`: a one-character unterminated
  entry is missed. Added case 15 (`x`, which the staged `file.txt` contains). Re-applied by
  hand: it fails with `exit 0, expected 1`.
- H5, `IFS=` removed: not killed. Explained: without `IFS=`, `read` strips leading and
  trailing whitespace, so an entry of spaces only becomes blank and `  #x` becomes a
  comment. That is a decision about what whitespace in an entry means, and it is not what
  this issue asks; the line existed before this change and is left alone.
- A3 (`grep -v` into a `while`) and A5 (`{ cat; echo; } | while`): equivalent fixes, so
  nothing should kill them.

S4 is closed. Next is S5 Verify.

### S5 Verify
- artifact: section "The leak gate checks an unterminated last line of `.leakwords`, for issue #97" in `docs/evidence.md`
- platform: macOS (Darwin), this machine, 2026-10-06, gitleaks 8.30.1 installed
- result: pass. Real `git commit` in a scratch repository, hook "before" (`3dd45b8`) and
  "after" named per command with `git -c core.hooksPath`: an unterminated matching last
  entry, alone or after another entry, was committed before and is refused after; a
  terminated entry is refused by both; an unterminated entry that matches nothing passes
  in both. `sh scripts/test-pre-commit.sh`: 36 `ok`, no `FAIL`.
- not exercised: the Ubuntu runner, CRLF files (out of bounds).
