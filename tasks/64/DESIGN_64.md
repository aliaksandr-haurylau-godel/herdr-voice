# DESIGN_64

## What changes

Four files. Nothing else is touched.

1. `.githooks/pre-commit` — the private-word-list check (check 2) gets an `else`
   branch.
2. `scripts/test-pre-commit.sh` — new; runs the hook in a scratch git repository.
3. `.github/workflows/check.yml` — one step in the `scripts` job runs the new
   script on Ubuntu and macOS.
4. `CONTRIBUTING.md` — says the hook refuses to run without `.leakwords`.

## The hook

Check 2 today is `if [ -f "$root/.leakwords" ]; then … fi`. The `else` branch
prints to standard error and exits 1:

```
leak gate: .leakwords is missing, so the private word list was not checked
create it with: cp .leakwords.example .leakwords
then list the names that must never be committed; an empty list is allowed
```

The `if` branch is not touched, so a checkout that has the file behaves as
before (AC-3, AC-4). The exit happens before check 3, the editing-debris
check. A checkout without the file cannot commit at all, so skipping check 3
there hides nothing.

`$root` comes from `git rev-parse --show-toplevel`, which in a linked worktree is
the worktree's own directory. Each worktree therefore needs its own
`.leakwords`, which is the situation issue #64 describes. The message says
`.leakwords` without a path, so it is right in every checkout.

## The test

`scripts/test-pre-commit.sh` follows the shape of `scripts/test-release-kind.sh`:
POSIX `sh`, `set -eu`, one `expect`-style helper, a failure counter, non-zero exit
on any failure.

It creates a scratch directory with `mktemp -d`, runs `git init` in it, stages a
file, and runs `sh "$ROOT/.githooks/pre-commit"` with that directory as the
working directory. The hook derives `$root` from the scratch repository, so the
worktree's own `.leakwords` is never read, written or deleted (AC-5).

A stub `gitleaks` that exits 0 is placed first on `PATH`. Without it the result
would depend on whether the machine running the test has gitleaks installed, and
the scratch repository has no `.gitleaks.toml` for a real one to read. The test
covers check 2 and the exit codes around it, not gitleaks.

Cases, each asserting exit code, standard output and standard error:

| Case | `.leakwords` | Expected |
|---|---|---|
| absent | none | exit 1; stderr names `.leakwords` and `cp .leakwords.example .leakwords`; stdout empty |
| example copy | copy of `.leakwords.example` | exit 0; stdout and stderr empty |
| no match | one pattern that does not occur in the staged diff | exit 0; stdout and stderr empty |
| match | one pattern that occurs in the staged diff | exit 1; stderr is the two lines the hook prints today |

The "match" case pins today's messages, so a change to check 2's `if` branch is
caught.

## CI

A step is added to the `scripts` job in `.github/workflows/check.yml`, after the
three existing shell tests: `sh scripts/test-pre-commit.sh`. The job already runs
on `ubuntu-latest` and `macos-latest`.

## CONTRIBUTING.md

The setup block at `CONTRIBUTING.md:12-16` marks `cp .leakwords.example
.leakwords` as required rather than as one step among optional ones, and one
sentence after the block says that the hook fails without the file and that an
unchanged copy of the example is a valid empty list.

## Decision: fail, not warn

**Context.** Check 2 of `.githooks/pre-commit` reads the private word list from
`.leakwords`, which is untracked, so every fresh clone and worktree starts
without it.

**Problem.** Without the file the check is skipped with no output. The issue
allows either failing or succeeding after a warning; the choice had to be made
before implementation.

**Decision.** The hook exits 1 with a message that names the file and the copy
command.

**Why.** A warning leaves the commit looking like a pass to anything that does not
read standard error, and repeats on every commit until nobody reads it; the
repository counts a silent no-op that looks like a pass as a defect. Failing
costs one command per checkout, and an unchanged copy of `.leakwords.example` is
a valid empty list, so a contributor with nothing to protect is not shut out.

## Risks

- A person with an existing checkout that lacks the file is stopped on the next
  commit. That is the intended effect, and the message says what to do.
- `git commit --no-verify` bypasses the whole hook, this check included. That is
  unchanged and is what the CI job is for.

## Coverage of the acceptance criteria

| AC | Where |
|---|---|
| AC-1, AC-2 | the hook's `else` branch; test case "absent" |
| AC-3 | the `if` branch untouched; test cases "example copy" and "no match" |
| AC-4 | the `if` branch untouched; test case "match" |
| AC-5 | `scripts/test-pre-commit.sh` |
| AC-6 | the new step in `check.yml` |
| AC-7 | `CONTRIBUTING.md` |
