#!/bin/sh
#
# Runs .githooks/pre-commit in a scratch git repository, once for each state of
# .leakwords. A hook that skips its private word list without saying so looks
# exactly like a hook that passed it, so the test pins what is printed as well
# as the exit code. The worktree's own .leakwords is never read or touched: the
# hook takes its root from the scratch repository.

set -eu

# Run from inside a git hook, these would point the scratch repository's git
# commands at the outer repository.
unset GIT_DIR GIT_INDEX_FILE GIT_WORK_TREE

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
HOOK="${ROOT}/.githooks/pre-commit"
SCRATCH="$(mktemp -d)"
trap 'rm -rf "${SCRATCH}"' EXIT INT TERM
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
printf 'an ordinary line\n#ordinary\n' > "${repo}/file.txt"
git -C "${repo}" add file.txt

code=0

# Runs the hook from the repository root, or from the subdirectory named by $1.
run_hook() {
    code=0
    (cd "${repo}/${1:-.}" && sh "${HOOK}") > "${SCRATCH}/out" 2> "${SCRATCH}/err" || code=$?
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
        ok "$1: stderr is exactly the expected lines"
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
expect_stderr_equals "absent" 'leak gate: .leakwords is missing, so the private word list was not checked
create it with: cp .leakwords.example .leakwords
then list the names that must never be committed; an empty list is allowed'

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

# 5. The root is the repository's, not the working directory's: the file is found
# when the commit is made from a subdirectory.
mkdir "${repo}/sub"
cp "${ROOT}/.leakwords.example" "${repo}/.leakwords"
run_hook sub
expect_exit "subdirectory" 0
expect_silent "subdirectory"

# 6. An empty file is a valid, empty list.
: > "${repo}/.leakwords"
run_hook
expect_exit "empty file" 0
expect_silent "empty file"

# 7. Something that is not a file does not count as the word list.
rm -f "${repo}/.leakwords"
mkdir "${repo}/.leakwords"
run_hook
expect_exit "directory" 1
expect_stderr_contains "directory" "cp .leakwords.example .leakwords"
rmdir "${repo}/.leakwords"

# 8. Entries match without regard to case, and are regular expressions.
printf 'ORDINARY\n' > "${repo}/.leakwords"
run_hook
expect_exit "upper-case entry" 1
printf 'ordin(a|x)ry\n' > "${repo}/.leakwords"
run_hook
expect_exit "regular expression" 1

# 9. Comment and blank lines are skipped. The comment would match the staged
# line "#ordinary" if it were read as an entry, and a blank entry matches anything.
printf '#ordinary\n\nzzz-no-such-word\n' > "${repo}/.leakwords"
run_hook
expect_exit "comment and blank lines" 0
expect_silent "comment and blank lines"

# 10. Every entry is read, not only the first.
printf 'zzz-no-such-word\nordinary\n' > "${repo}/.leakwords"
run_hook
expect_exit "second entry" 1

# 11. The last entry is checked when the file does not end in a newline: after
# another entry, and as the only line. `read` returns non-zero on such a line,
# so a loop that tests only its status never runs the body for it.
printf 'zzz-no-such-word\nordinary' > "${repo}/.leakwords"
run_hook
expect_exit "unterminated last entry" 1
expect_stdout_empty "unterminated last entry"
expect_stderr_equals "unterminated last entry" 'leak gate: staged changes match a private word-list entry
the matching pattern is in .leakwords; nothing is printed here on purpose'

# 12. The same, with the entry as the only line.
printf 'ordinary' > "${repo}/.leakwords"
run_hook
expect_exit "unterminated only entry" 1
expect_stderr_contains "unterminated only entry" "match a private word-list entry"

# 13. An unterminated last line that matches nothing passes without a word.
printf 'zzz-no-such-word' > "${repo}/.leakwords"
run_hook
expect_exit "unterminated, no match" 0
expect_silent "unterminated, no match"

# 14. An unterminated comment is still a comment: read as an entry, `#ordinary`
# would match the staged line.
printf 'zzz-no-such-word\n#ordinary' > "${repo}/.leakwords"
run_hook
expect_exit "unterminated comment" 0
expect_silent "unterminated comment"

if [ "${failures}" -ne 0 ]; then
    printf '%s check(s) failed\n' "${failures}"
    exit 1
fi
