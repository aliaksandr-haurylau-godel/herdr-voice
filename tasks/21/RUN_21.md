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

No production code assembles a bias string yet: `src/context.rs` is the
unrelated invocation-context parser, and `src/config.rs` has no `[context]`
table. Issue #13 closed after this stage's first draft and landed the
recognition engine's interface — `Engine::transcribe(&self, audio: &Path)` in
`src/stt.rs`, and the `command` engine in `src/stt/command.rs` — but that
interface has no place for a bias string: the model and the language are baked
into `CommandEngine` once at construction, while a bias string is per-take.
The acceptance criteria therefore still stop at producing a capped bias string
through an interface a later change can consume, but for a real reason now —
widening `Engine::transcribe`'s signature is an interface decision this stage
should not make silently — rather than because no engine exists.

Five items the issue names as genuinely unresolved, or that this stage's second
pass surfaced, were left to design rather than answered here: how the transcript
file is found reliably, which agents beyond the one proven have a known
conversation location, what happens when no conversation can be found at all,
how the bias string is threaded into `Engine::transcribe` (a `{prompt}`
placeholder alongside `{audio}`/`{model}`/`{language}` is plausible but not
decided), and (already settled by the owner, not left open) that no privacy
handling beyond the prototype's is added.

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

This worktree was created from `main` before pull request #20 (issue #13,
recognition) merged. After the first `AC_21.md`/`RUN_21.md` commit, the
coordinator rebased `feat/21-context` onto the post-#20 `main` (`27f57b0`); the
S1 commit now sits on top of it as `9564b65`. `AC_21.md` and `RUN_21.md` were
revised in a second commit to reflect the recognition code and the
"Recognition, by hand on macOS" evidence section that landed with #20/#13 —
both exist now and are cited directly; no discrepancy remains.
