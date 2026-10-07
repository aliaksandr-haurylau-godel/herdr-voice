# Issue #97 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans with superpowers:test-driven-development. Steps use checkbox (`- [ ]`) syntax.

**Goal:** The pre-commit hook checks the last entry of `.leakwords` whether or not the file ends in a newline.

**Architecture:** One changed loop condition in `.githooks/pre-commit`; four new cases in `scripts/test-pre-commit.sh`.

**Tech Stack:** POSIX `sh`. No Rust changes.

**Spec:** `tasks/97/DESIGN_97.md` (S2 READY), `tasks/97/AC_97.md`.

## Global Constraints

- Everything in the repository is English. No absolute path, employer, client or internal name in any file.
- Run `sh scripts/test-pre-commit.sh` only; it uses a scratch repository and never reads the worktree's own `.leakwords`.
- Before each commit: `cargo test`, `cargo clippy --all-targets -- -D warnings`, `cargo fmt --check`, `python3 scripts/check_manifest.py`, each run as `CARGO_BUILD_JOBS=6 CARGO_TARGET_DIR="$TMPDIR/hv-97-target" perl -e 'alarm shift; exec @ARGV' 570 cargo ...`, one at a time. The Windows dead-code check is skipped because the diff touches no Rust; `RUN_97.md` says so. Grep every written file for the four stray edit-tool tags (`new_string` and `old_string`, opening and closing) and line-start conflict markers.
- Commit messages end with `Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>`.
- Do not edit anything else in the hook.

## Review Focus

- A file that ends with a newline must behave as before (cases 1 to 10 keep passing unchanged).
- An unterminated comment line must still be skipped (case 14).

---

### Task 1: The four cases

**Files:** Modify `scripts/test-pre-commit.sh`, after case 10 and before the final `if [ "${failures}" -ne 0 ]` block.

- [ ] **Step 1: Add the cases.**

```sh
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
```

- [ ] **Step 2: Run** `sh scripts/test-pre-commit.sh` — expected: `FAIL unterminated last entry: exit 0, expected 1`, `FAIL unterminated last entry: stderr differs`, `FAIL unterminated only entry: exit 0, expected 1` and `FAIL unterminated only entry: stderr lacks ...`; every other check `ok`; the last line is `4 check(s) failed` and the exit status is 1.

### Task 2: The fix

**Files:** Modify `.githooks/pre-commit`, the line `while IFS= read -r pattern; do`.

- [ ] **Step 1: Implement.** Replace it with

```sh
  # `|| [ -n "$pattern" ]`: `read` returns non-zero on a last line with no newline
  # and has already stored its text, so the status alone would end the loop before
  # that entry is checked. After a clean end of file `pattern` is empty and the
  # body does not run again.
  while IFS= read -r pattern || [ -n "$pattern" ]; do
```

- [ ] **Step 2: Run** `sh scripts/test-pre-commit.sh` — expected: every check `ok`, no output line starting with `FAIL`, exit 0.

### Task 3: Gates and commit

- [ ] **Step 1:** Run the four gates as written in Global Constraints.
- [ ] **Step 2:** Grep the written files for stray tags and conflict markers.
- [ ] **Step 3:** `git add .githooks/pre-commit scripts/test-pre-commit.sh tasks/97` and commit `fix: the leak gate checks an unterminated last line of .leakwords (#97)`.
