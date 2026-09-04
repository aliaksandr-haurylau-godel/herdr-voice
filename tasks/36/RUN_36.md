# RUN_36

| field | value |
|---|---|
| issue | #36 — Rewrite: wire the stage into the pipeline, with the http and command engines |
| input | GitHub issue, read with `gh issue view 36` |
| stage | S1 |
| branch | feat/36-rewrite-http-command |
| opened | 2026-09-04 |

## Stages

<!-- One block per stage, appended, never rewritten. -->

### S1 Assess
- artifact: `AC_36.md`
- produced: 2026-09-04

Written directly from the issue body and the current state of `src/daemon.rs`,
`src/config.rs`, `src/doctor.rs`, `docs/design.md` and `spike/spike.sh`'s
`rewrite()` function — no ADO ticket exists for this repository; the GitHub
issue is the trigger (`CLAUDE.md`, "Trigger and run root"). No `Risk` field
exists to check.

One thing the issue's own body treats as settled turned out not to be, on
reading the prototype: the "short phrase skips the round trip" rule has no
prototype measurement behind it — `spike/spike.sh`'s `rewrite()` always calls
the rewrite model, unconditionally. `AC_36.md` demotes this from a settled
requirement to an item the design stage has to invent, alongside four others
(what counts as a foreign term, whether context reaches the prompt at all,
what "told once" means as a mechanism, and the exact new configuration keys).

Sequencing: this worktree branches from `origin/main` at `7dab991`, before
issue #26's pull request (#35) has merged — noted in `AC_36.md` so the plan
does not assume a state that may change under it.

```yaml
gate:
  stage: S1
  artifact: AC_36.md
  reviewer: designer
  verdict: null
  date: null
```
