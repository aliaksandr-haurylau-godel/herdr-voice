# RUN_21

| field | value |
|---|---|
| issue | #21 — Context: bias recognition with what the agent is talking about |
| input | GitHub issue, read with `gh issue view 21` |
| stage | S1 |
| branch | feat/21-context |
| opened | 2026-08-26 |

## Stages

<!-- One block per stage, appended, never rewritten. -->

### S1 Assess
- artifact: `AC_21.md`
- produced: 2026-08-26

No production code exists for this concern yet: `src/context.rs` is the
unrelated invocation-context parser, `src/config.rs` has no `[context]` table,
and the recognition engine the bias string would feed (issues #13, #15, #16) is
not built either. The acceptance criteria therefore stop at producing a capped
bias string through an interface a later stage can consume, rather than at
wiring a call to an engine.

Four items the issue names as genuinely unresolved were left to design rather
than answered here: how the transcript file is found reliably, which agents
beyond the one proven have a known conversation location, what happens when no
conversation can be found at all, and (already settled by the owner, not left
open) that no privacy handling beyond the prototype's is added.

```yaml
gate:
  stage: S1
  artifact: AC_21.md
  reviewer: designer
  verdict: null
  date: null
```

## Notes

<!-- Anything a later stage needs and the artifacts do not carry. -->

`docs/evidence.md` has no section titled "Recognition, by hand on macOS" as the
issue's account of the "пули квест" / "pull request" take might suggest; the
closest sections are "Context and its effect on the transcript"
(`docs/evidence.md:22-36`) and "Pane screen versus conversation transcript"
(`docs/evidence.md:62-66`). The facts given for this run rest on those sections
plus the issue text itself, not on a section that does not exist.
