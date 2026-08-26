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

## Gate S4

```yaml
gate:
  stage: S4
  artifact: the diff on feat/21-context since d4a2d1b
  reviewer: two independent reviews, one hunting defects by mutation, one
    checking conformance and real behaviour
  verdict: QUESTIONS
  date: 2026-08-26
  blocker: null
```

Two reviews ran against `651d071`. All four gates were green on that commit —
194 tests, clippy, format, manifest — and the reviews found thirteen things
anyway, nine of them reproduced on a scratch copy of the tree. The technique
that produced most of them is the one this run keeps: delete or invert the
behaviour a test claims to prove, then run that test.

### Sent back for fixing, in this issue

1. **The bias string is built for the pane focused when the take stops, not the
   pane the take was pinned to.** `src/daemon.rs:159` passes `dictate`'s
   current `pane`, `cwd` and `agent` to `take_bias`, while `transcribe` and
   delivery use `take.target` and `take.agent`, pinned when recording began
   (`src/capture.rs:98`). `cwd` is pinned nowhere. Reproduced by driving
   `answer` twice with two different panes: the text goes to the first, the
   bias comes from the second. This is the exact failure the comment at
   `src/daemon.rs:131` says the pinning exists to prevent.

2. **The whole `pane` source is untested.** Replacing `pane::read` with a hard
   error — the pane never read under any `[context] source` — leaves all 194
   tests passing. Every `collect` and daemon test points `herdr_binary` at a
   path that cannot exist, so no test ever has a pane read succeed and reach
   `Collected.bias`. AC-3's `pane` value and `auto`'s fallback-hit branch have
   no coverage. Counting an empty pane read as a hit also passes.

3. **AC-5's cap is proved by no test.** Replacing the truncation at
   `src/bias.rs:130` with the uncapped string leaves all 194 tests passing.
   The existing test asserts the `truncated` flag and the character count,
   never that the finished string is at most `prompt_chars` long. The
   files-only path at `src/daemon.rs:226` has the same gap.

4. **AC-9 has a second log path and it is unguarded.** `bias_refused_line`
   (`src/daemon.rs:279`) carries a `Collected` whose `bias` holds the
   file-name string; appending the string to that line leaves every test
   passing. The guarded path, `bias_line`, is real — the same mutation there
   turns its test red.

5. **`transcript::find` has the empty-and-relative working directory hole that
   `files::toplevel` guards.** `src/bias/transcript.rs:31` joins the slug
   before testing for an empty directory, so an absent `focused_pane_cwd`
   returns any `.jsonl` sitting directly in the transcript root. `"."` is
   unguarded in both modules, and there `git -C .` resolves against the
   daemon's own working directory.

6. **Discovery walks up until some ancestor's slug directory exists, and the
   home directory's usually does.** A pane in a repository with no session of
   its own is handed an unrelated conversation and reports `transcript:hit`,
   so under `auto` the pane is never consulted. Every git worktree is that
   case, and this project runs one worktree per line of work. Bound the walk
   at the repository root.

7. **The prototype's per-turn cut was dropped in the port.**
   `spike/context.sh:107` truncates each turn to 300 characters and collapses
   newlines; `src/bias/transcript.rs:106` does neither. Measured on a real
   transcript, one kept turn ran to 7000 characters — more than the whole
   600-character budget on its own.

8. **`pane::read` ignores its `lines` argument**, capping on the constant
   instead (`src/bias/pane.rs:82`). The parameter is a lie, and the test named
   for the cap proves only the filtering.

9. **`files::collect(cwd, 0)` returns one name**: the bound is checked after
   the push (`src/bias/files.rs:32`). `file_names = 0` is a valid configured
   state.

10. **A failed pane read drops the reason.** `src/bias.rs:79` matches `_ =>`
    and discards `PaneError`, whose text is the only thing that would say what
    to fix. Every other outward herdr call in the tree names `HERDR_BIN_PATH`
    in its failure.

11. **Stale suppression and stale comment.** `src/bias/source.rs:12` still says
    the function is not called from `main` yet and carries
    `#[allow(dead_code)]`; this same diff wires it in.

12. **`docs/design.md:129` still lists branch, pane title and agent kind as
    context components.** No code collects any of them. By this repository's
    own standard a documented claim the code does not meet is a defect; the
    document is what changes, since the three are out of this issue's scope.

### Deferred, with the reason

- **No time limit on the pane read.** `src/bias.rs:80` spawns herdr with no
  deadline, on the take path, before recognition. A herdr that accepts and
  does not answer blocks the daemon — `cancel` included — until the client's
  120 second timeout. Not reproduced. This belongs to issue #28, which exists
  for time limits, rather than growing this diff a timeout mechanism of its
  own.
- **The order of assembly inside the cap.** File names come first and the cut
  falls at the tail, so the conversation is what is lost, and among the turns
  the newest goes first. Reproduced: a repository of forty long directory
  names produces 1529 characters of file names, and the conversation
  contributes nothing while the log still reads `transcript:hit`. Whether the
  conversation should be cut instead, or the newest turn kept, is a design
  question and not a defect in the port.

### Not defects

The three modified journal tests all still prove what they proved: moving the
delivering line after the delivery call turns its test red, so filtering the
bias line out does not mask a reordering. The four new `Runtime` fields are
resolved once at start and no path through them can panic or abort the daemon.
`delivery::herdr_binary` being public adds no third read of `HERDR_BIN_PATH`
and changes no delivery behaviour. The character cap is genuinely
character-based: Russian text truncated mid-string does not panic. No absolute
path, home directory or private name appears anywhere in the diff.

## Gate S4, second pass

```yaml
gate:
  stage: S4
  artifact: the diff on feat/21-context since d4a2d1b, at 643e30a
  reviewer: one review, re-running each first-pass mutation and examining the
    surfaces the fixes introduced
  verdict: QUESTIONS
  date: 2026-08-26
  blocker: null
```

All twelve findings of the first pass are fixed. Eleven are proved: the
mutation that caught each one now turns a test red. The twelfth, the stale
suppression, is settled by inspection because no mutation applies to it.

The pane-read reason, added this round to satisfy the rule that a failure names
what to do next, does not leak the screen into the journal: `PaneError` has two
variants, neither touches the called program's output, and folding stdout into
one of them turns the guard test red. The bound on the walk did not break
ordinary discovery — a pane at a repository root whose session directory exists
still finds it, and a pane deeper inside still walks up to it. Pinning the
working directory into the take changed no capture or delivery behaviour, and a
take started with no working directory still records, recognises and delivers.

### Sent back, six small things

1. **The per-turn newline collapsing has no test.** Removing `.replace('\n',
   " ")` at `src/bias/transcript.rs:143` leaves all 209 tests green. The test
   written for it puts its newline at character 1000, and the 300-character cut
   removes it before the collapse is reached, so the assertion passes on the
   cut alone. This is the accidental-pass shape the fixture rename in the same
   round was meant to end.

2. **`docs/design.md:135` states behaviour the code does not have.** It says
   the conversation is read from the pane's screen when no transcript is found
   or when the source is `pane`. Under `source = "transcript"` the pane is
   never consulted on a miss, and a test pins that. Only `auto` falls back. The
   line entered on this branch, so it is inside the diff under review, and it
   is the same section that was just corrected for the same kind of error.

3. **A second `git rev-parse --show-toplevel` runs per take**, with the same
   argument as the first and microseconds after it —
   `src/bias/transcript.rs:43` asks for the ceiling that `files::collect`
   already computed. Measured with a shim on the path: four git subprocesses
   where there were three. On the take path, before recognition.

4. **The leak guard covers stdout but not stderr.** `src/bias.rs:317`. Folding
   the called program's stdout into `PaneError::Failed` turns the test red;
   folding its stderr in leaves everything green. Pane contents arrive on
   stdout, so the likely leak is covered, but the guard should close both.

5. **`why.contains('3')` at `src/bias.rs:329` proves nothing about the exit
   code.** Removing the code from `PaneError::Failed`'s rendering leaves every
   test green: the reason string still holds a `3` from the scratch script's
   path. Nothing covers the rendering a person actually reads.

6. **The module comment overstates the bound.** `src/bias/transcript.rs:39`
   says the walk stops at the repository root. Outside a repository there is no
   root, and the walk climbs to `/` as before — which is the recorded decision,
   but the comment states the bound unconditionally.

### Noted, not sent back

`bias_counts` collapses newlines in the pane reason although neither variant of
`PaneError` can produce one, and the `Auto` arm of the attempted-source
rendering is unreachable by construction. Both are harmless and both are
commented as such.
