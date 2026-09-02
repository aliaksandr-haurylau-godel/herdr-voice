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
