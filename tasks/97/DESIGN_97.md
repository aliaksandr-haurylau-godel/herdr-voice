# DESIGN_97

Design for issue #97, against `tasks/97/AC_97.md` (gate S1: READY,
`tasks/97/RUN_97.md`). Classified **bounded**: one loop condition, three test cases. The
live approval `superpowers:brainstorming` asks for is replaced by the project's own gate,
`octoflow-reviewer-planner`, because the run is not a chat.

## 1. The loop reads an unterminated last line

**Context.** `.githooks/pre-commit` checks each line of the untracked `.leakwords`
against the staged diff, with `while IFS= read -r pattern; do ... done < file`.

**Problem.** `read` stores the text of a last line that has no newline in `pattern`, then
returns non-zero because it reached end of file. The `while` condition fails, so the body
never runs for that line and its entry is never checked. Editors often save without a
final newline, and the file is written by hand in every clone.

**Decision.** The condition becomes `while IFS= read -r pattern || [ -n "$pattern" ]; do`.

**Why.** After a failed `read` at end of file, `pattern` holds the unterminated text, or
is empty when the file ended with a newline. The second test runs the body once for the
unterminated line and not at all otherwise, so a terminated file behaves as before. The
`case` inside the loop still skips a blank or comment line, so an unterminated comment is
not read as an entry. Under `set -e` a failed command in a `while` condition does not
stop the script.

## 2. Tests

Four cases are appended to `scripts/test-pre-commit.sh`, after case 10, numbered 11 to
14, each writing the list with `printf` and no final newline (the staged file contains
the line `an ordinary line` and `#ordinary`):

| case | list | expected |
|---|---|---|
| 11 | `zzz-no-such-word` newline `ordinary`, no final newline | exit 1, the match message exactly as in case 4 |
| 12 | `ordinary`, nothing else, no final newline | exit 1, the same message |
| 13 | `zzz-no-such-word`, no final newline | exit 0, prints nothing |
| 14 | `zzz-no-such-word` newline `#ordinary`, no final newline | exit 0, prints nothing |

Cases 11 and 12 fail against the unchanged hook (exit 0 where 1 is expected); 13 and 14
pass against both, and pin that the change does not make an unterminated last line an
entry it should not be.

## 3. Out of scope

CRLF line endings, a symbolic-link list, invalid regular expressions, and the other two
checks in the hook.
