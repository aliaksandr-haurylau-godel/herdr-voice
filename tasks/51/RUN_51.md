# RUN 51 — repaired terms come back wrapped in backticks

Input: GitHub issue #51, read with `gh issue view 51`. The issue body is the
ticket; it has no comments.

Run root: `tasks/51/`. Branch `fix/51-reply-backticks`, cut from `main` at
`d68b2a2` (`0.1.0-beta.6`).

Every stage runs. The change alters the text delivered to a person's pane.

## Stages

### S1 Assess

- artifact: `AC_51.md`
- produced: 2026-10-21

The model the issue names, `google/gemma-4-e4b`, is not loaded in the local
LM Studio and no request naming it was sent. On the loaded model,
`google/gemma-4-e2b`, a take with three mangled terms came back without
backticks (two requests, a different take from the issue's), so the defect has
not been reproduced in this run. The strip is specified from the issue's
description and pinned by recorded calls; the live series on the model the issue
names is pending the owner loading it.

The ticket's goal: corrected terms arrive in the pane as the words themselves,
with no markup wrapped around them. Out of bounds: the shortening the same reply
showed (#50) and the default engine or model (#43). Done when: a reply carrying
markup around a repaired term is delivered without it, and a test pins it
through a recorded call; whatever the chosen mechanism, it does not strip
characters the speaker actually dictated.

Gate, round 1:

```yaml
gate:
  stage: S1
  artifact: AC_51.md
  reviewer: designer
  verdict: QUESTIONS
  date: 2026-10-21
  questions:
    - "AC-1, AC-2 and AC-4 conflict with AC-6 on whether the reply is trimmed again after the strip. For a fenced reply with a newline at each edge, removing only the backticks leaves the newlines; trimming again removes non-backtick characters, which AC-4 forbids. The tests for AC-2 and AC-6 depend on which."
  blocker: null
```

Answer, written into `AC_51.md`: the reply is trimmed again after the strip, as
it is trimmed today; edge whitespace is not dictated content. AC-4 and AC-6 say
so. The same round corrected "unsubmitted" in the as-is, since submitting is a
setting.

Gate, round 2:

```yaml
gate:
  stage: S1
  artifact: AC_51.md
  reviewer: designer
  verdict: READY
  date: 2026-10-21
  questions: []
  blocker: null
```

The reviewer confirmed that the round-1 answer creates no new contradiction and
that every cited path and line range matches the code.

### S2 Design

- artifact: `DESIGN_51.md`
- produced: 2026-10-21

```yaml
gate:
  stage: S2
  artifact: DESIGN_51.md
  reviewer: planner
  verdict: READY
  date: 2026-10-21
  questions: []
  blocker: null
```

The reviewer noted one imprecise citation: the call through the `Engine` trait is
at `src/daemon.rs:1227`, inside `resolve_rewrite` which starts at `:1216`.
Corrected in `DESIGN_51.md`.

### S3 Plan

- artifact: `PLAN_51.md`
- produced: 2026-10-21

Gate, round 1:

```yaml
gate:
  stage: S3
  artifact: PLAN_51.md
  reviewer: implementer
  verdict: QUESTIONS
  date: 2026-10-21
  questions:
    - "T4 ends with the four gates and the Windows dead-code check on a scratch copy, and its Done-when requires that the Windows check passes. The plan gives no command for the Windows check and no meaning for 'passes'."
  blocker: null
```

Answer, written into `PLAN_51.md`: the four gate commands and the Windows check
command are now spelled out in T4, and passing means the last command exits 0
with no warning. The reviewer's two small points were taken as well: T3 uses
`.expect("text")` because `EngineError` has no `PartialEq`, and the unreadable
table cell for the all-backticks reply now says what it is.

Gate, round 2:

```yaml
gate:
  stage: S3
  artifact: PLAN_51.md
  reviewer: implementer
  verdict: READY
  date: 2026-10-21
  questions: []
  blocker: null
```

The reviewer confirmed that T4 gives commands and a pass condition, that the two
small defects are gone, and that every symbol the plan names exists.

### S4 Implement

Tests first, in the order of `PLAN_51.md`: the nine direct tests failed to compile
(function not found), then passed with `without_added_backticks`; the four tests
that need the wiring failed (`3 failed; 46 passed` for the module), then passed
with it. The four gates and the Windows dead-code check on a scratch copy pass;
the full suite is `1039 passed; 0 failed; 1 ignored`.

Code review of the diff by a reviewer that did not write it (no cargo, read-only):
no real defect. It checked that no dictated character can be removed, that the
evidence matches the tests, that each test fails against a wrong implementation,
that there is no panic path and that the command engine is untouched. It raised two
items, both taken: `docs/evidence.md` gave the command and a one-line result but
not the output, so the filtered output of the whole module run is now beside the
claim; and a doc comment said "carried one first" where the rule is "contains
one", now corrected. It also noted that a fence with a language tag leaves the tag
in the reply, which `AC_51.md` records under "Out of scope / noticed".
