# RUN_26

| field | value |
|---|---|
| issue | #26 — Pass the bias string to the engine, so the terms actually come back |
| input | GitHub issue, read with `gh issue view 26` |
| stage | S1 |
| branch | feat/26-bias-to-engine |
| opened | 2026-09-02 |

## Stages

<!-- One block per stage, appended, never rewritten. -->

### S1 Assess
- artifact: `AC_26.md`
- produced: 2026-09-02

Written directly from the issue body and the current state of `src/stt.rs`,
`src/stt/command.rs` and `src/daemon.rs` — no ADO ticket exists for this
repository; the GitHub issue is the trigger (`CLAUDE.md`, "Trigger and run
root"). No `Risk` field exists to check.

One reading was chosen rather than left open, and flagged in `AC_26.md` at
AC-3: the issue's "a configured list that names no placeholder for it must
still work" is read as "does not break", not as "the string is force-appended
the way the audio path is when `{audio}` is absent" — the design stage can
revisit this if it disagrees, but a criterion has to pick one reading to be
checkable at all.

```yaml
gate:
  stage: S1
  artifact: AC_26.md
  reviewer: designer
  verdict: null
  date: null
```
