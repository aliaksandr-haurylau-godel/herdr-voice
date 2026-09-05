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

```yaml
gate:
  stage: S1
  artifact: AC_26.md
  reviewer: designer
  verdict: READY
  date: 2026-09-02
```

Every as-is citation checked and held, with two line-range corrections applied
below: `CommandEngine` is `src/stt/command.rs:36-40`, not `34-38`; `render` is
`src/stt/command.rs:15-34`, not `14-31`. Content behind both is as described.

Noted by the reviewer, not a gate question: the AC does not require the
rendered argument list to stay off the journal, but nothing in the daemon
journals argv today — `bias_line`/`bias_refused_line` are the only lines built
from a `Collected`, and `CommandError` carries the program name and stderr,
never argv — so the design can uphold #21's rule without a new requirement.

S1 is closed. Next is S2 Design.

### S2 Design
- artifact: `DESIGN_26.md`
- produced: 2026-09-02

`superpowers:brainstorming` names three paths, each ending in a live
chat-approval gate; this run is async and overnight, so the design was written
directly and closed the way S1 was — by the project's own gate (planner,
`octoflow-reviewer-planner`) rather than a chat approval. The decision itself
(the trait widens rather than the string arriving some other way) is exactly
the kind `CLAUDE.md` delegates: "Всё остальное решай сам и записывай в
`docs/decisions.md` с основанием."

```yaml
gate:
  stage: S2
  artifact: DESIGN_26.md
  reviewer: planner
  verdict: null
  date: null
```

```yaml
gate:
  stage: S2
  artifact: DESIGN_26.md
  reviewer: planner
  verdict: READY
  date: 2026-09-02
  questions: []
  blocker: null
```

Every citation checked and held except one type name, corrected below:
`Runtime.recognition` is a `Recognition`, `type Recognition = Result<Box<dyn
Engine + Send + Sync>, String>` (`src/daemon.rs:47`), not
`Result<..., EngineError>` — the argument made from it is unaffected.

Noted by the reviewer, folded into §4: `Fake` is constructed at three call
sites in `src/daemon.rs`'s test module (565, 653, 775), and a fourth site,
`src/daemon.rs:842`, calls the private `transcribe` directly — all four are
mechanical once the signature is fixed, and the plan assigns them to the task
that changes the daemon rather than splitting them out.

The reviewer confirmed every AC maps to a task with a checkable
done-criterion, including AC-7 (the manual-take entry `AC_26.md` itself
specifies), and that the dependency order is unambiguous: the trait widens
first, `command::render` and the daemon threading follow in parallel, the docs
entries after, the manual take last.

S2 is closed. Next is S3 Plan.

### Correction to DESIGN_26.md, found while preparing S3

`docs/design.md` does not document the `{audio}`/`{model}`/`{language}`
placeholder set at all — its §7 configuration table has no `[stt] command` key
in it, and the placeholders are recorded only in `docs/decisions.md`'s entry
for `[stt] command`'s introduction (2026-08-25, #13). `DESIGN_26.md`'s §2 table
named `docs/design.md` as where `{prompt}` gets documented; corrected to name
`docs/decisions.md`'s own entry for this issue instead, which is where the
other three actually live. The decision itself (trait widens, no forced
append) is unchanged, so this does not reopen the S2 gate — the same class of
harmless correction as the `Recognition` type name fixed above.

### S3 Plan
- artifact: `PLAN_26.md`
- produced: 2026-09-02

Written directly at `tasks/26/PLAN_26.md`, per this repository's convention
(not `docs/superpowers/plans/`, the skill's own default). Five tasks: widen
the trait and `render` (Task 1), thread the string through the daemon (Task
2), record the decision (Task 3), S4 review (Task 4), S5 verify plus the
manual take (Task 5).

Self-review caught one real gap before this went to gate: the first draft
undercounted `src/stt/command.rs`'s existing test call sites needing an added
argument — three `render(...)` calls and six `engine.transcribe(...)` calls,
not four and two. Corrected and every call site named explicitly, by test
function name, before committing.

```yaml
gate:
  stage: S3
  artifact: PLAN_26.md
  reviewer: implementer
  verdict: null
  date: null
```

```yaml
gate:
  stage: S3
  artifact: PLAN_26.md
  reviewer: implementer
  verdict: READY
  date: 2026-09-02
  questions: []
  blocker: null
```

Every citation checked and held, including the corrected call-site count from
self-review (three `render(...)` calls, six `engine.transcribe(...)` calls,
each verified by line number). `CapturingFake`'s spec is fully self-contained;
nothing to invent.

S3 is closed. Next is S4 Implement.

## Gate S4

```yaml
gate:
  stage: S4
  artifact: the diff on feat/26-bias-to-engine since 7dab991
  reviewer: one review, mutation-testing both load-bearing behaviors
  verdict: READY
  date: 2026-09-03
  blocker: null
```

185 lines across `src/daemon.rs`, `src/stt.rs`, `src/stt/command.rs` and one row
in `docs/decisions.md`. Both behaviors the design depends on were mutated and
caught: force-appending the bias string when `{prompt}` is absent turns
`a_list_with_no_prompt_placeholder_does_not_gain_one` red, and having `dictate`
pass an empty string instead of `collected.bias` turns
`the_collected_bias_string_reaches_the_engine` red.

Ten checks, all clean: no forced append anywhere else in the diff; the string
reaches the engine from the same take `take_bias` and `transcribe` share
inside one `Ok(take) => { ... }` arm, with no staleness path; no new place logs
a rendered argument list or the bias string, and the pre-existing
called-program's-own-`stderr` risk stays exactly as scoped out in
`DESIGN_26.md` §5; an empty bias substitutes as an ordinary no-op; every
existing test call site updated for the widened signatures kept its original
assertion rather than being weakened to compile; the reworded comment above
`take_bias`'s call site states what the code now does and drops nothing that
made it inaccurate; no new panic path outside test code; no absolute path or
private name anywhere in the diff; `docs/decisions.md`'s new row matches the
mutation-tested behavior; every behavioral change has a direct test.

Mutations run on a disposable worktree, reviewed worktree confirmed untouched
before and after, full suite green throughout.

S4 is closed. Next is S5: verify by test suite, then by a spoken take.

## Gate S5

```yaml
gate:
  stage: S5
  artifact: docs/evidence.md, "The bias string reaches the engine, in argument
    lists a test can inspect"
  verdict: pass, with one criterion pending
  date: 2026-09-03
  platform: macOS 26.6.2, Rust 1.97.1
```

Fresh run on `feat/26-bias-to-engine` at `8275dcd`: `cargo test` — 213 passed,
0 failed; `cargo clippy --all-targets -- -D warnings` — clean; `cargo fmt
--check` — clean; `python3 scripts/check_manifest.py` — 11 entries, all
commands known.

AC-1 through AC-6 and AC-8 are established by the test suite, stated in the
evidence entry with what each test proves and what mutation catches its
absence. AC-7 — the spoken take — is recorded as **pending, owned by the
owner**: this session has no microphone. The entry names exactly what to run
and what result to compare against.

S5 is closed for what a test suite can establish. Next: the pull request for
#26, with AC-7 left open in the PR description for the owner to close.
