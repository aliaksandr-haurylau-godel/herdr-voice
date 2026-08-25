# RUN_22

| field | value |
|---|---|
| issue | #22 — Delivery: put the transcript in the pane without submitting it |
| input | GitHub issue, read with `gh issue view 22` |
| stage | S1 |
| branch | feat/22-delivery |
| opened | 2026-08-26 |

## Stages

<!-- One block per stage, appended, never rewritten. -->

### S1 Assess
- artifact: `AC_22.md`
- produced: 2026-08-26

Transcription and rewrite (issues 13, 15, 16, 21) are not part of this branch, so
the acceptance criteria treat delivery as an operation over a pane and a piece of
text — testable with a fake pane call, per `CLAUDE.md`'s own description of how
this stage is tested ("delivery through a recorded call") — rather than as an
end-to-end pipeline from a real transcript. That gap is recorded under "Out of
scope / noticed" in `AC_22.md`, not folded into an AC.

No journal subsystem exists in `src/`; every existing visible failure in the
daemon is a line on standard error, captured by herdr's own plugin log. "Journal
line" in the acceptance criteria is read as one more line in that same channel.

The take produces no separate text file, only a WAV. "The text must not be lost
silently" is met two ways: the text is written to the journal line before
delivery is attempted, and the WAV is kept rather than deleted on a failed
delivery.

## Gate S1

```yaml
gate:
  stage: S1
  artifact: AC_22.md
  reviewer: designer
  verdict: QUESTIONS
  date: 2026-08-26
  questions:
    - "The as-is is contradicted by the worktree the design will be implemented in, and the contradiction changes the design. AC_22.md says the pipeline stops after capture and that there is no transcription because issues 13, 15, 16 and 21 are unmerged. Transcription is present and wired: dictate calls transcribe on a finished take and replies with the text, the level and the target. The scope decision rests on the false half. Which design is wanted: (a) delivery is called from dictate/transcribe with the real transcript, which also decides what the reply becomes now that the text goes to the pane instead of back to the client, or (b) delivery is a module with no caller, exercised only by AC-12's tests. AC-1 to AC-12 are satisfiable either way and name no caller."
    - "AC-5 requires a delivery against a pane that no longer exists to be a failed delivery and explicitly not a silent no-op, but nothing states what herdr does against a pane id that is gone. If those commands exit non-zero, AC-5 is already AC-6. If they exit 0 and do nothing, AC-5 forces a pane-existence check against a herdr command nobody names, plus a fake for it in the tests. Two different designs and two different test surfaces."
  blocker: null
```

Note recorded by the reviewer, not a reason to withhold READY: AC-3 makes
`submit = true` always use `herdr agent prompt`, while the prototype gates that on
the target pane actually having an agent and otherwise falls back to `send-text`.
AC-6 makes a rejected call a visible failure, so the dropped condition is
designable as written; it was dropped deliberately.

### Answers

**Question two is answered by measurement, not by choice.** Both commands refuse a
pane that does not exist, with a code and a structured error:

```
$ herdr pane send-text "w99:p99" "probe"
{"error":{"code":"pane_not_found","message":"pane w99:p99 not found"},...}
exit=1
$ herdr agent prompt "w99:p99" "probe"
{"error":{"code":"agent_not_found","message":"agent target w99:p99 not found"},...}
exit=1
```

So a pane that is gone is an ordinary rejected call. AC-5 collapses into AC-6, no
existence check is needed before delivering, and the tests need no fake for one.

**Question one is answered (a): delivery is called when a take finishes.** The
goal of this issue is that the text lands in the pane's input — a module with no
caller does not reach it, and an unreached goal is not something to plan around.
What the client prints changes with it: the transcript goes to the pane, so the
reply becomes a confirmation of where it was delivered and at what level, rather
than the text itself. That is a change to what the client prints, in the same class
as the earlier decision that made it print anything at all, and it is recorded in
`docs/decisions.md` when the criteria are revised.
