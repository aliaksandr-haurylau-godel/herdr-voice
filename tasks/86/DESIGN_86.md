# DESIGN_86

Design for issue #86, against `tasks/86/AC_86.md` (gate S1: READY,
`tasks/86/RUN_86.md`).

Classified **bounded**: one reading function, one changed branch in `run`, one
sentence in `docs/design.md`. The live approval `superpowers:brainstorming` asks
for is replaced by the project's own gate, `octoflow-reviewer-planner`, because the
run is not a chat. The user-visible wording below is the only choice that is not
purely mechanical; it follows the issue's "Done when".

## 1. Reading the answer

**Context.** `main` in `src/setup.rs` builds the closure `run` calls to get the
person's answer. It calls `read_line` and returns `Some(line)` whenever the call
did not fail.

**Problem.** `read_line` returns `Ok(0)` at end of file, so a process that nobody
could answer returns `Some("")`, which `run` cannot tell from a person who pressed
Enter.

**Decision.** A function `read_answer(reader: &mut dyn BufRead) -> Option<String>`
returns `None` when `read_line` fails or returns zero bytes, and `Some(line)`
otherwise. `main`'s closure calls it on the locked standard input.

**Why.** `None` already means "no answer" in `run`'s signature
(`answer: &mut dyn FnMut() -> Option<String>`); only the closure failed to use it.
Taking a reader makes the zero-byte case a unit test with no terminal. A last line
without a newline (`"y"` then end of file) still returns `Some("y")`: that is an
answer.

## 2. What `run` does with no answer

**Context.** After the question, `run` takes `answer().unwrap_or_default()` and
treats anything but `y` or `yes` as a decline: it prints `nothing was changed.`,
prints the legacy report and returns 0.

**Problem.** The unanswered run and the declined run print the same text and exit
the same way, so the person cannot learn that their keystrokes never arrived.

**Decision.** `run` matches on `answer()`. For `None` it prints, after a blank line:

```text
the question could not be answered: no input reached this process, so nothing was
changed. Run `herdr-voice setup` in a terminal that passes your keystrokes on.
```

then the legacy report, and returns **1**. `Some(...)` goes through the existing
decline-or-write code unchanged, so a declined offer keeps its text and its exit
code 0.

**Why.** The text names what happened (no input arrived), what the state is
(nothing changed) and what to do (a terminal that forwards keystrokes), and shares
no sentence with the decline text. It says "no input reached this process" rather
than "end of file" because the same branch is taken when the read fails. Exit 1
because the task the person started was not done and nothing they can retry by
doing nothing differently will do it; a decline is a decision and stays 0. Every
other path in `run` where the setup could not do its job (no config path, unreadable
file, bad TOML, refused pane) already returns 1.

## 3. The design document

**Decision.** One sentence is added to section 7a of `docs/design.md`, after the
description of the question: a run that gets no answer says so and exits 1; a
declined offer exits 0.

**Why.** Section 7a is where `setup`'s question is described, and the exit code is
a behaviour a caller can rely on.

## 4. Tests

- `run` with an answer closure returning `None`: output contains the new text and
  not `nothing was changed.`; returns 1; the stand-in herdr records no call; the
  configuration file is not written.
- `run` with `Some("n\n")` in the same state: output contains `nothing was
  changed.` and not the new text; returns 0 (existing tests keep covering this; one
  assertion on the new text's absence is added).
- `run` with `None` still prints the legacy report.
- `read_answer`: empty reader gives `None`; `"\n"` gives `Some("\n")`; `"y"` with no
  newline gives `Some("y")`; a reader that fails gives `None`.
- By hand, S5: the built binary with standard input a pseudoterminal that sends end
  of file at the first read. A process test cannot do this, because `is_terminal`
  is false for a pipe and the run takes the other branch.

## Out of scope

The `is_terminal` check; the question's wording; the branch that opens the pane.
