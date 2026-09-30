# RUN_64

| field | value |
|---|---|
| issue | #64 — The leak gate skips its private word list in silence when .leakwords is absent |
| input | GitHub issue, read with `gh issue view 64 --comments` (no comments exist) |
| stage | S1 |
| branch | fix/64-leak-gate-silence |
| opened | 2026-09-30 |

## Stages

<!-- One block per stage, appended, never rewritten. -->

### S1 Assess
- artifact: `AC_64.md`
- produced: 2026-09-30

The issue leaves the choice between failing and warning to the run. The chosen
reading is written into `AC_64.md` (the hook fails) and the reasoning is in
`DESIGN_64.md`, where the four-part decision record belongs.

Precondition replaced. The orchestration brief expected the pre-commit hook to
print that it checked `.leakwords`. On `13733c5` the hook prints nothing on
either path — that silence is the defect this issue describes — so the
expectation could not hold. The orchestrator confirmed the brief was wrong and
replaced the check with: `test -s .leakwords` succeeds and
`git config core.hooksPath` prints `.githooks`. Both were checked before the
first commit: `.leakwords` is non-empty and the hooks path is `.githooks`.

```yaml
gate:
  stage: S1
  artifact: AC_64.md
  reviewer: designer
  verdict: QUESTIONS
  date: 2026-09-30
  questions:
    - >-
      Where the fail-or-warn choice is decided, and on what basis. The issue's
      "Done when" gives two outcomes for a checkout with no .leakwords. It also
      says the choice is made "in the issue rather than left to the implementation".
      AC_64.md picks failing (R1, AC-1, AC-7) but gives no reason and points to
      DESIGN_64.md, which does not exist yet. The one requirement that decides the
      design would have to be justified by the designer, which means inventing the
      requirement and then defending it. Point to where the choice was made, or
      state in AC_64.md why failing was chosen over warning, so that DESIGN_64.md
      records a decision instead of making one.
  blocker: null
```

Answer to the questions: the choice is made in `AC_64.md` (section "requirements",
"Reading chosen where the issue is open"), with the basis stated there; it follows
from `CLAUDE.md`. Also corrected in `AC_64.md`: the gitleaks fallback is at
`.githooks/pre-commit:17-22`, not 20-25, as the reviewer noted.

```yaml
gate:
  stage: S1
  artifact: AC_64.md
  reviewer: designer
  verdict: READY
  date: 2026-09-30
  blocker: null
```

The reviewer's note, recorded as given: the issue asks for the choice "in the
issue"; `AC_64.md` records it in itself. Whether it should also be written into
issue #64 on GitHub is left to the owner. The design is the same under either
reading.

S1 is closed. Next is S2 Design.

### S2 Design
- artifact: `DESIGN_64.md`
- produced: 2026-09-30

Written directly rather than through an interactive brainstorm: the acceptance
criteria fix the behaviour, so there is nothing to ask the owner. The one real
choice, failing against warning, was made in S1.

```yaml
gate:
  stage: S2
  artifact: DESIGN_64.md
  reviewer: planner
  verdict: READY
  date: 2026-09-30
  blocker: null
```

S2 is closed. Next is S3 Plan.

### S3 Plan
- artifact: `PLAN_64.md`
- produced: 2026-09-30

```yaml
gate:
  stage: S3
  artifact: PLAN_64.md
  reviewer: implementer
  verdict: READY
  date: 2026-09-30
  questions: []
  blocker: null
```

The reviewer traced the plan by hand and did not run it (Bash was unavailable to
it). S3 is closed. Next is S4 Implement.

### S4 Implement
Tasks T1 to T4 of `PLAN_64.md` were done in order. T1 was run against the
unchanged hook first and failed exactly where the plan said: three checks in the
"absent" group, everything else `ok`. After T2 all of it passed.

Review and mutation test, by two fresh subagents that did not write the code:

- Code review of the whole diff: `READY`, no blocking findings. Applied from its
  low-severity findings: `unset GIT_DIR GIT_INDEX_FILE GIT_WORK_TREE` at the top
  of `scripts/test-pre-commit.sh`, and `INT TERM` on its `trap`. Not applied:
  a message on the `ok` line naming the compared text (cosmetic).
- Not applied, and not part of this issue: a `.leakwords` whose last line has no
  trailing newline has that line silently skipped, because `read` returns
  non-zero at end of file (`.githooks/pre-commit`, the `while … read` loop). It is
  the same kind of silent skip, but changing it changes the present-file path,
  which AC-3 requires to stay as it is. Reported to the orchestrator as a
  candidate follow-up issue.
- Mutation test, against the script as first written (11 checks): 30 mutations
  of the added and changed lines and of the behaviour AC-3 and AC-4 pin. Survivors
  and what killed them afterwards:

  | Survivor | Killed by |
  |---|---|
  | echo 1 or echo 3 of the new message deleted or shortened | the exact-stderr check in the "absent" case |
  | `-f` changed to `-s`, `-e` or `-r` | the "empty file" and "directory" cases |
  | `$root/.leakwords` replaced by `.leakwords` (working directory) | the "subdirectory" case |
  | `-i`, `-E` removed from the `grep`; `-E` changed to `-F` | the "upper-case entry" and "regular expression" cases |
  | blank-line or comment skip removed | the "comment and blank lines" case (the staged file has a line `#ordinary` for this) |
  | `break` after the first entry | the "second entry" case |

  All twelve of those mutations were re-run against the extended script in a
  scratch copy and each one now fails it.

  Survivors accepted without a new test, all in the test script's own machinery,
  where the hook is correct and a weakened helper still passes: a helper changed
  to always report `ok`, `fail` not counting, the final summary `exit 1` changed
  to `exit 0`, `set -eu` weakened, and `read -r` or `IFS=` dropped from the hook's
  loop with entries that contain no backslash or leading space. Killing the first
  four needs a test of the test against a deliberately broken hook, which is out
  of proportion for a 120-line script; the last needs an entry with a backslash.

The Windows dead-code check was not run. The command was refused by the
permission classifier because it contained `git checkout -- src`; it was not
retried in another form. This diff changes no Rust file (`git status` shows no
change under `src/`), so the check has nothing to see, and the orchestrator
accepted skipping it for that reason.

Incident before the first commit. `core.hooksPath` lives in the shared
`.git/config` of the main checkout. At the pre-commit check it printed an
absolute path to the main checkout's `.githooks` instead of `.githooks`; the file
had been modified at 12:42 the same day, by a session other than this one. That
hook is byte-identical to the one at `13733c5`, so a commit would have run
without the fix. The run stopped and told the orchestrator; the value was
restored to the relative `.githooks`, so each worktree runs its own hook. The
commit was made after that, with the value read again from this worktree.

Gates run fresh before the commit: `cargo test` (571 + 2 passed), `cargo clippy
--all-targets -- -D warnings`, `cargo fmt --check`, `python3
scripts/check_manifest.py`, `sh scripts/test-pre-commit.sh` — all green.

### S5 Verify
Run in a scratch clone of the branch with real `git commit` and real gitleaks,
in four states: no file, an unchanged copy of the example, an entry that matches,
and a new worktree. The result is the section "The leak gate refuses to run
without `.leakwords`, for issue #64" in `docs/evidence.md`. Verdict: the criteria
AC-1 to AC-5 and AC-7 are met by the recorded runs; AC-6 is met by the diff and is
observed only when CI runs.

Follow-up, not done here and not part of this issue: the last line of a
`.leakwords` with no trailing newline is skipped by the hook's `read` loop. The
orchestrator confirmed that the owner's file ends in a newline, so nothing is being
skipped today, and will file the issue after the pull request is open. The pull
request names it.
