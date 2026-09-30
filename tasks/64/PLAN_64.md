# PLAN_64

Four tasks. Task 1 comes first and must fail before task 2 makes it pass.

| Task | Depends on |
|---|---|
| T1 test script | none |
| T2 hook | T1 |
| T3 CI step | T1 |
| T4 CONTRIBUTING.md | none |

## T1 — `scripts/test-pre-commit.sh`

Input: `.githooks/pre-commit` as it is on `main`; the shape of
`scripts/test-release-kind.sh`.

Output: a new file `scripts/test-pre-commit.sh`, mode 755, with this content:

```sh
#!/bin/sh
#
# Runs .githooks/pre-commit in a scratch git repository, once for each state of
# .leakwords. A hook that skips its private word list without saying so looks
# exactly like a hook that passed it, so the test pins what is printed as well
# as the exit code. The worktree's own .leakwords is never read or touched: the
# hook takes its root from the scratch repository.

set -eu

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
HOOK="${ROOT}/.githooks/pre-commit"
SCRATCH="$(mktemp -d)"
trap 'rm -rf "${SCRATCH}"' EXIT
failures=0

# A gitleaks that always passes, first on PATH: the result must not depend on
# whether the machine running this has gitleaks, and the scratch repository has
# no .gitleaks.toml for a real one to read. This test covers the word list.
mkdir "${SCRATCH}/bin"
printf '#!/bin/sh\nexit 0\n' > "${SCRATCH}/bin/gitleaks"
chmod +x "${SCRATCH}/bin/gitleaks"
PATH="${SCRATCH}/bin:${PATH}"

repo="${SCRATCH}/repo"
git init -q "${repo}"
printf 'an ordinary line\n' > "${repo}/file.txt"
git -C "${repo}" add file.txt

code=0

run_hook() {
    code=0
    (cd "${repo}" && sh "${HOOK}") > "${SCRATCH}/out" 2> "${SCRATCH}/err" || code=$?
}

fail() {
    printf 'FAIL  %s: %s\n' "$1" "$2"
    failures=$((failures + 1))
}

ok() {
    printf 'ok    %s\n' "$1"
}

# name, then what the run must have produced.
expect_exit() {
    if [ "${code}" -eq "$2" ]; then ok "$1: exit ${code}"; else fail "$1" "exit ${code}, expected $2"; fi
}

expect_silent() {
    if [ ! -s "${SCRATCH}/out" ] && [ ! -s "${SCRATCH}/err" ]; then
        ok "$1: prints nothing"
    else
        fail "$1" "printed something: $(cat "${SCRATCH}/out" "${SCRATCH}/err")"
    fi
}

expect_stderr_contains() {
    if grep -q -F -- "$2" "${SCRATCH}/err"; then
        ok "$1: stderr says '$2'"
    else
        fail "$1" "stderr lacks '$2'; it is: $(cat "${SCRATCH}/err")"
    fi
}

expect_stdout_empty() {
    if [ ! -s "${SCRATCH}/out" ]; then ok "$1: stdout empty"; else fail "$1" "stdout: $(cat "${SCRATCH}/out")"; fi
}

expect_stderr_equals() {
    printf '%s\n' "$2" > "${SCRATCH}/want"
    if cmp -s "${SCRATCH}/err" "${SCRATCH}/want"; then
        ok "$1: stderr is exactly the two known lines"
    else
        fail "$1" "stderr differs; it is: $(cat "${SCRATCH}/err")"
    fi
}

# 1. No .leakwords: the hook must refuse, and say what to do.
rm -f "${repo}/.leakwords"
run_hook
expect_exit "absent" 1
expect_stdout_empty "absent"
expect_stderr_contains "absent" ".leakwords"
expect_stderr_contains "absent" "cp .leakwords.example .leakwords"

# 2. An unchanged copy of the example is a valid, empty list.
cp "${ROOT}/.leakwords.example" "${repo}/.leakwords"
run_hook
expect_exit "example copy" 0
expect_silent "example copy"

# 3. A list with an entry that the staged diff does not match.
printf 'zzz-no-such-word\n' > "${repo}/.leakwords"
run_hook
expect_exit "no match" 0
expect_silent "no match"

# 4. A list with an entry that the staged diff matches.
printf 'ordinary\n' > "${repo}/.leakwords"
run_hook
expect_exit "match" 1
expect_stdout_empty "match"
expect_stderr_equals "match" 'leak gate: staged changes match a private word-list entry
the matching pattern is in .leakwords; nothing is printed here on purpose'

if [ "${failures}" -ne 0 ]; then
    printf '%s check(s) failed\n' "${failures}"
    exit 1
fi
```

Done when: `sh scripts/test-pre-commit.sh` run against the unchanged hook exits 1
and its only failures are in the "absent" group (exit code and the two
`stderr` checks); the groups "example copy", "no match" and "match" all print
`ok`. Running it must leave the worktree's `.leakwords` byte-for-byte unchanged
(compare `shasum .leakwords` before and after).

## T2 — the hook

Input: T1 done and failing as described.

Output: in `.githooks/pre-commit`, the block that ends at line 35 with `fi`
gains an `else` branch before that `fi`:

```sh
else
  echo "leak gate: .leakwords is missing, so the private word list was not checked" >&2
  echo "create it with: cp .leakwords.example .leakwords" >&2
  echo "then list the names that must never be committed; an empty list is allowed" >&2
  exit 1
fi
```

Nothing else in the file changes. The comment above the block (lines 24-25)
gains one sentence: `The hook refuses to run without it.`

Done when: `sh scripts/test-pre-commit.sh` exits 0 and every line begins `ok`;
`git diff -- .githooks/pre-commit` shows only the `else` branch and the one
comment sentence.

## T3 — the CI step

Input: T1 done.

Output: in `.github/workflows/check.yml`, in the `scripts` job, after the step
named `the install script's decisions`, a new step:

```yaml
      - name: the pre-commit hook, with and without .leakwords
        run: sh scripts/test-pre-commit.sh
```

Done when: `git diff -- .github/workflows/check.yml` shows exactly those two
added lines plus context, with the indentation of the neighbouring steps, and
`python3 -c 'import yaml,sys; yaml.safe_load(open(".github/workflows/check.yml"))'`
exits 0 (if `yaml` is not installed, `ruby -ryaml -e 'YAML.load_file(".github/workflows/check.yml")'`).

## T4 — CONTRIBUTING.md

Input: nothing.

Output: in `CONTRIBUTING.md`, the line

```
cp .leakwords.example .leakwords      # then fill in the names that must never be committed
```

becomes

```
cp .leakwords.example .leakwords      # required; then fill in the names that must never be committed
```

and after the closing fence of that block, before the paragraph that begins
`` `.gitleaks.toml` is tracked ``, one paragraph is inserted:

```
The hook refuses to run while `.leakwords` is missing, in every checkout and every
new worktree. An unchanged copy of the example is a valid, empty list.
```

Done when: `git diff -- CONTRIBUTING.md` shows only those two changes.

## After the four tasks

The four gates from the brief, run fresh, then the Windows dead-code check, then
the review of the whole diff and the mutation test (S4), then S5. Those are
process steps, not tasks of this plan.
