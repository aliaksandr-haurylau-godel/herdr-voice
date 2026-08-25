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
