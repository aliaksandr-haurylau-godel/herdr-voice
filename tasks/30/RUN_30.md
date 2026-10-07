# RUN_30

| field | value |
|---|---|
| issue | #30 — A take of silence comes back as confident invented text |
| input | GitHub issue, read with `gh issue view 30`; the issue has no comments |
| stage | S4 |
| branch | fix/30-repeated-phrase |
| opened | 2026-10-07 |

## Stages

<!-- One block per stage, appended, never rewritten. -->

### S1 Assess
- artifact: `AC_30.md`
- produced: 2026-10-07

## Notes

- Base: `e92f0b2`.
- The run assignment says the warning's wording goes to the orchestrator before S4.

```yaml
gate:
  stage: S1
  artifact: AC_30.md
  reviewer: designer
  verdict: READY
  date: 2026-10-07
  blocker: null
```

Reviewer notes, recorded as returned (none is a gate question): the design will check the raw transcript (the `transcript` variable) rather than the rewritten text; a kept take is still subject to `crate::record::bound` (`src/record.rs:96`), as takes kept by `[record]` are today; the wording is planned to go to the owner before S4.

### S2 Design
- artifact: `DESIGN_30.md`
- produced: 2026-10-07
- The warning wording was sent to the orchestrator the same day, before S4.

```yaml
gate:
  stage: S2
  artifact: DESIGN_30.md
  reviewer: planner
  verdict: READY
  date: 2026-10-07
  blocker: null
```

Reviewer notes, recorded as returned (not gate questions): the S1 note calls the raw transcript "the `transcript` variable" while the design checks `text` right after `engine.transcribe`; both are the same value, renamed at `src/daemon.rs:940`. The success reply uses `take.target` as it is, and the design follows the existing reply.

### S3 Plan
- artifact: `PLAN_30.md`
- produced: 2026-10-07

```yaml
gate:
  stage: S3
  artifact: PLAN_30.md
  reviewer: implementer
  verdict: READY
  date: 2026-10-07
  questions: []
  blocker: null
```

Reviewer note, recorded as returned: every symbol and signature the plan relies on exists as described, and the eleven task 1 tests were traced by hand through the function with the expected results.

### S4 Implement
- baseline: `e92f0b2`.
- The warning wording was sent to the orchestrator before S2 closed; no change had arrived when S4 started. The strings live in `repetition_sentence` and `probably_not_speech_line` and in the tests that name them, so a change is a few lines.

Implementation order: the tests of tasks 1 and 2 were written first and failed to compile for want of `one_phrase_repeated` and `repetition_sentence`; the code was written after. Baseline `cargo test` was not run separately on `e92f0b2` for this branch; after the change: 800 passed, 1 ignored (11 new tests in `src/repeat.rs`, 10 new in `src/daemon.rs`). `cargo clippy --all-targets -- -D warnings`, `cargo fmt --check`, `scripts/check_manifest.py` and the Windows dead-code check on a scratch copy are clean.
