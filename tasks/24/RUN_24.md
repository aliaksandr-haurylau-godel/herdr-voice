# Run 24

Input: GitHub issue #24, read with `gh issue view 24`. The issue body and its
comments are the ticket.

## Stages

S1 to S3 skipped. The shape was fixed before the run: issue #24 states the three
changes and the reason for each, and both candidate processes were read off disk
on 2026-08-25 — the project's `CLAUDE.md`, the three reviewers in
`.claude/agents/`, the octoflow gate and its shared stage and verdict
definitions, the seven superpowers skills the stages name, and the manifest,
dependencies, skills and agents of the five ai-first-marketplace kits. Nothing
here changes plugin behaviour; it changes how work on the plugin is run.

The owner chose octoflow with superpowers over the kits on 2026-08-25. The kits
were refused on facts: no Rust anywhere in them, `cargo test` not among the suites
`run-tests` detects, a tracker that is Azure DevOps or Jira with no GitHub-issues
mode, browser-driven test automation with nothing to drive in a terminal binary,
and a second gate mechanism with its own artifact formats that would not compose
with this one.

## What changed

`CLAUDE.md`, the "How work is run" section:

- S4 is closed by `superpowers:requesting-code-review` against the diff, before a
  pull request exists. `/code-review` stays, as a second pass after the pull
  request is open, because it answers with a comment rather than a verdict and
  cannot close a stage.
- S5 Verify is a stage: run it, read the output, write what happened into
  `docs/evidence.md` with the platform. A negative result recorded passes it; an
  assertion with no command beside it does not.
- Every verdict, S4's and S5's included, is recorded in `RUN_<issue>.md`.
- The six octoflow stage skills that are not on disk — `octoflow-take`,
  `octoflow-design`, `octoflow-plan`, `octoflow-implement`, `octoflow-verify`,
  `octoflow-ship` — are named as not applying here. They are not created and
  nothing outside this repository is edited, for the same reason the stage
  reviewers are project agents: this repository does not change a personal
  environment to suit itself.
- Infrastructure runs keep their permission to skip S1 to S3, and lose any
  reading that let them skip S4 and S5.

## Why S4 needed a gate

Two defects went through S4 unreviewed on 2026-08-25 and were found by hand
afterwards: `cancel` answers "nothing to cancel" while recording continues
(#18), and a reply carrying a newline arrives truncated, taking the rest of the
transcript, the measured level and the target pane with it (#19). Nothing looked
at the code between "the tasks are done" and "the pull request is open"; the
run file for issue #13 has no S4 gate block, because there was no gate to run.
