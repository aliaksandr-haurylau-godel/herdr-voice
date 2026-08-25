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

## Gate S1, second pass

```yaml
gate:
  stage: S1
  artifact: AC_21.md
  reviewer: designer
  verdict: READY
  date: 2026-08-26
  questions: []
  blocker: null
```

The ticket body was given to the reviewer in the request this time. The first pass
had no shell and could not read it, so requirements attributed to the ticket went
unchecked; this pass judged them against the ticket itself.

Notes the reviewer left for the design, none of them gate questions:

- The pane source needs an outward call that does not exist yet. `src/transport.rs`
  is the plugin's own socket and `src/client.rs` its command-line side; there is no
  herdr client. The prototype names the exact contract —
  `herdr pane read "$pane" --source recent --lines "$CTX_LINES" --format text` in
  `spike/context.sh` — and the invocation context already carries the pane id, the
  working directory and the agent kind, so the mechanism is designable without a
  new requirement.
- The `pane` branch has no budget of its own among the four keys: `conversation_turns`
  counts transcript turns, and no line count appears. The prototype's 80 lines and
  the `prompt_chars` cap bound the output, so this settles from the reference the
  ticket points at.
- `docs/design.md` also lists branch, pane title and agent kind as context
  components, and the prototype builds its hotword string from those plus file
  names, while these criteria cover conversation plus file names. The ticket does
  not ask for the other three, so their absence is scope rather than a gap.

S1 is closed. Next is S2 Design.

## Gate S2

```yaml
gate:
  stage: S2
  artifact: DESIGN_21.md
  reviewer: planner
  verdict: QUESTIONS
  date: 2026-08-26
  questions:
    - "bias::collect is stated to be pub and to return the finished, capped string, but three other decisions need more than a string to cross that boundary: a log line naming which source was tried and that it found nothing, with auto naming both attempts; an unrecognised source value refused with the three names listed rather than silently defaulted; and the logging assigned to the daemon rather than to bias. A String return carries none of that and nothing states what does, so the bias task and the daemon task cannot be cut independently — one side would invent the return type and the other would guess it. The refusal case also has no stated effect on the take: reply with an error and record nothing, or log and proceed with an empty conversation component, are different checkable outcomes."
    - "Discovery is decided as walking up to the first existing directory under the transcript root, but the transcript root is never named and no seam for it is given — while the pane seam and the files seam are both stated exactly. The done-criteria nevertheless include a fixture .jsonl found by directory and a fake transcript reaching the log line. Neither is reachable without an injection point, and the injection point is part of collect's parameter list, which the first question already shows is unstated."
    - "One section decides that the collected string is written to the per-request log so the owner can see what the bias string would have been. AC-9 says the opposite in its own words: the bias string is not written to a log or a file beyond what the take already needs. An implementer producing the full string would satisfy the design and fail the criterion, and it is not stated which a reviewer is meant to accept, nor at what granularity — the whole string, a prefix, or only its length and which sources contributed."
  blocker: null
```

### Raised by the run, not by the gate

The design widens `Engine::transcribe` to take a bias argument and adds a
`{prompt}` placeholder to the command engine — a breaking change to the trait that
shipped in `main`, which also commits the two engines that are not built yet to
that shape. In the same document, this issue does not pass a bias string to any
engine: `bias::collect` is called only so its result can be logged. So the
interface breaks here and the benefit arrives later. Either the widening belongs
to this issue together with the passing, or the trait is left alone until the
issue that uses it. The gate did not raise this; it is raised here because the
decision to open S3 is the run's.

## Gate S2, second pass

```yaml
gate:
  stage: S2
  artifact: DESIGN_21.md
  reviewer: planner
  verdict: QUESTIONS
  date: 2026-08-26
  questions:
    - "The design names `Source` with two members, `resolve(value) -> Result<Source, String>` held beside the recognition engine, and says `auto` is not a member because it is the dispatch's own concern — while `collect` takes an already-resolved Source or the caller's instruction to try both. Nothing names what `resolve(\"auto\")` returns, nor the type of that `or`. With two members the resolved value cannot tell the default from `transcript`, yet the criteria make that distinction observable — `transcript` never reads the pane even on a miss, `auto` does — and the tests require all three to resolve and all three to dispatch. Three separate tasks meet at that boundary and would each have to invent the same third state: whether `Source` grows an `Auto` member, whether `collect` takes a different parameter, or whether the daemon passes something else."
  blocker: null
```

Both claims the gate was asked to verify hold. `Recognition` is
`Result<Box<dyn Engine + Send + Sync>, String>` resolved once inside the daemon's
`start` and consulted per request, so resolving the context source the same way
matches what is already there. And the transcript root's origin is reachable:
`config::Vars` carries `home` and `config::directory` already reads it.

Two notes from the gate, neither a question: `home` is optional while
`transcript::find` takes a path, and the design's rule that a miss never blocks
recognition already covers an absent home directory. And `dictate` currently
receives only the pane string although the parsed invocation with the working
directory and the agent name is available one frame up — plan-level work, not a
design gap.

## Gate S2, third pass

```yaml
gate:
  stage: S2
  artifact: DESIGN_21.md
  reviewer: planner
  verdict: READY
  date: 2026-08-26
  questions: []
  blocker: null
```

S2 is closed.

## Sequencing against issue 22

Issue 22 lands first, and this run builds on what it leaves behind. Both runs
change how the daemon carries per-take state, in opposite directions: 22 replaces
the recognition engine threaded through `start`, `serve`, `serve_one`, `answer`,
`dictate` and `transcribe` with one bundle, while this design holds the resolved
context source beside that same engine — the parameter 22 removes. Whichever
landed second would rewrite its own threading work.

Three consequences for the plan of this run:

- The resolved context source becomes a field of 22's bundle rather than a value
  held beside the recognition engine.
- The helper that runs the `herdr` binary through `HERDR_BIN_PATH` is 22's to
  extract from `doctor`. This run consumes it and does not extract it again.
- The working directory and the agent name reach `dictate` by 22's threading. This
  run needs both — the first to derive the project directory, the second to decide
  whether a conversation is sought at all — and adds neither itself.

This is sequencing between two runs rather than a change to either design, so
neither artifact is reopened.

## Gate S3

```yaml
gate:
  stage: S3
  artifact: PLAN_21.md
  reviewer: implementer
  verdict: BLOCKED
  date: 2026-08-26
  questions: []
  blocker: "Tasks 8, 9 and 10 edit the daemon's shared state bundle and the herdr-binary helper, neither of which exists in this worktree until issue 22 merges. Tasks 11 and 12 depend on them transitively. Owner: the run for issue 22."
```

Everything the gate checked in the executable portion holds: every citation in
tasks 1 to 7 matches the code, the prototype's four contracts are quoted as they
stand, and the two character counts added beyond the design are pinned precisely
enough to implement and test while staying counts rather than content, so the rule
against putting the collected string in a log is not weakened by them.

One citation is wrong and does not stop anything: a task points at a constant in
`src/main.rs` as precedent for an attribute it does not carry. The instruction
around it is self-contained; only the analogy is weaker than claimed.

### How this run proceeds

Tasks 1 to 7 start now. They are the whole of the collection work — the three
sources, resolving the source key, assembly and the cap — and the gate found them
executable start to finish without inventing anything. They touch no file the
delivery run edits except the configuration reader, where the two runs add
different tables.

Tasks 8 to 10 wait for issue 22 to merge, and the blocker is recorded above rather
than worked around: the daemon work is written against the merged code, not against
a guess at its shape.

Holding tasks 1 to 7 idle until the blocker clears would buy nothing. The verdict
stands as BLOCKED for the run as a whole, and the part that can move, moves.
