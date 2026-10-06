# DESIGN_27

Design for issue #27, against `tasks/27/AC_27.md` (gate S1: READY,
`tasks/27/RUN_27.md`).

Classified **bounded**: one match arm split in two, two sentences, tests. The live
approval `superpowers:brainstorming` asks for is replaced by the project's own gate,
`octoflow-reviewer-planner`, because the run is not a chat. The wording is a
user-visible name; it is sent to the orchestrator before S4 and recorded here as
proposed.

## 1. What the engine line says for `command` and `http`

**Context.** `engine_finding_from` in `src/doctor.rs` maps the result of
`stt::check_with` to the engine line. `check_with` returns `Ready::Candle`,
`Ready::Command` or `Ready::Http`. The candle arm names the device it will run on. A
wildcard arm `Ok(_)` handles the other two with `"{:?} is ready"` of the engine name.

**Problem.** `check_with` established that `[stt] command` is non-empty (and the model
resolves), or that `[stt] url` is non-empty. It did not look for the program or contact
the endpoint, and must not: the first element of the list is often a shell, and running
a configured program to test it can hang. "is ready" claims more than that, and the
quoted token is the engine kind, which reads as a program name.

**Decision.** The wildcard becomes two arms, one per engine, each with its own
sentence, both with state `ok`:

| engine | detail |
|---|---|
| `command` | `[stt] command is set; its program is not looked for until a take starts` |
| `http` | `[stt] url is set; the endpoint is not contacted until a take starts` |

The candle arm, every `missing` finding and the exit code are untouched.

**Why.** Each sentence has an established half and a not-established half, which is what
the issue asks for, in the register of the rewrite line (`configured to post to ...`).
Two named arms instead of a wildcard mean that a future engine variant fails to compile
until someone decides what its line may claim. The state stays `ok` because the
configuration is complete and refusing it would change the exit code of a report that is
true; the unverified half is in the words.

## 2. What is not printed

The first element of `[stt] command` is not printed: for a shell wrapper it would be true
and say nothing about the transcriber. The program is not checked, with or without
`--version`, for the reasons in section 1.

## 3. Tests

In `src/doctor.rs`, next to the existing engine-line tests:

- `command` with a program that is not on `PATH` (`["hv27-no-such-program", "{audio}"]`):
  state `Ok`, detail equals the `command` sentence exactly.
- `http` with a url set: state `Ok`, detail equals the `http` sentence exactly.
- Both: detail contains neither `is ready` nor a quoted engine kind (`"command"`,
  `"http"`).
- The existing `candle` line test and the `missing` tests are unchanged and keep passing.
- By hand, S5: the built binary's `doctor` with `engine = "command"`, once with a program
  absent from `PATH` and once with one that is present; the line is the same in both,
  which is the point.

## 4. The design document

`docs/design.md` is searched for the engine line's old text. If it quotes it, it is
updated; otherwise nothing is added, because the sentence itself says what it claims.

## Out of scope

Checking the program; the candle and missing lines; #21 and #22; the quotations of the old
line in earlier `docs/evidence.md` sections, which record what those runs printed.
