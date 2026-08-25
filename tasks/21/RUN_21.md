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

## Gate S1

```yaml
gate:
  stage: S1
  artifact: AC_21.md
  reviewer: designer
  verdict: QUESTIONS
  date: 2026-08-26
  questions:
    - "AC-3 requires that the conversation source — transcript file on disk versus pane contents read through herdr — be set by configuration and not hard-wired to the file, but AC-7 enumerates the [context] table as exactly three keys, and docs/design.md documents only those three. No name, no permitted values and no default are given for the key that selects the source. The config schema and the module's entry point cannot be written without inventing all three, and the default alone decides what a machine with no configuration file does."
    - "AC-3's second sentence reads two ways with a different design behind each: (a) a mode the person sets, where the configured source is used and the other is never consulted, or (b) transcript first, pane contents as an automatic fallback when no transcript is found. The as-is section supports (b), the to-be section supports (a). They differ in the module's control flow, in whether herdr is called at all on the transcript-success path, and in what the open question about having no conversation at all even means."
  blocker: null
```

Noted by the reviewer, not a gate question: nothing in `src/` can call herdr today
— there is no client for reading a pane and no invocation of the `herdr` binary —
so the pane source needs a new outward call. The prototype's exact command in
`spike/context.sh` is a sufficient contract to design against.

The reviewer had no shell and could not read the issue with `gh`; it checked the
criteria against `docs/design.md`, `docs/evidence.md`, `spike/context.sh`,
`src/stt.rs`, `src/context.rs` and `src/config.rs`, and every citation it checked
held.

### Answered

The owner named the key: `[context] source`, three values, `auto` default —
`auto` is the transcript falling back to the pane when none is found;
`transcript` and `pane` are set modes where the other source is never
consulted. That answers both questions at once rather than choosing between the
two readings the second question posed: `auto` is reading (b) from the
questions above, `transcript`/`pane` together are reading (a), and the key
carries all three. `AC_21.md` was revised on 2026-08-26 to name the key in
AC-3, AC-7 and the requirements, to state the herdr-call rule each value
implies (never called under `transcript`/`pane` on the excluded path, called
under `auto` only after the transcript search comes up empty), and to split
open question 3 ("no conversation found at all") into its three per-value
forms — the design decision itself (fail, proceed on file names, or report
once) stays open, per value or otherwise. No entry was added to
`docs/decisions.md`: the key is the owner's decision, not one taken during this
revision.
