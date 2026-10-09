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

### S4 code review (round 1)

Reviewer: a fresh general-purpose subagent, over `e92f0b2..b4f3467`. No Critical findings. Verdict: ready to open a pull request, with fixes. Findings and how each was settled:

1. Important — "It was delivered; check it before sending" is wrong when `[delivery] submit` is on and the pane has an agent: `delivery::deliver` then submits, so the text has already been sent. **Fixed.** The sentence now reads "It was delivered; check what reached the pane." This changes the wording sent to the orchestrator on 2026-10-07 and was re-sent. `DESIGN_30.md` still carries the old words; it is a gated artifact and is not edited.
2. Important — `docs/design.md` said a delivered take keeps neither its recording nor its text unless `[record] transcripts` is on, and that with it off "a take leaves nothing behind". **Fixed:** section 4 "Delivery" and section 7 now name the one exception, a flagged take.
3. Minor — the kept take is still removed by `record::bound` after about 50 further takes; the message says "kept" without saying so. **Accepted;** the same holds for every kept take today, and `DESIGN_30.md` states it.
4. Minor — the pane name in the journal line and the toast was not collapsed to one line. **Fixed,** with a test (`a_pane_name_with_a_newline_is_one_line_in_the_journal_and_the_toast`). The reply's `delivered to {target}` was already uncollapsed before this change and is left as it was.
5. Minor — when `[rewrite]` is on and collapses the repetition, the warning is about the raw transcript while the delivered text differs. **Accepted;** the `delivering:` line before it shows what was delivered.
6. Minor — a script written without spaces (Chinese, Japanese) is one "word" and is never flagged; the AC say "split into words". **Accepted as a known gap,** stated in the pull request. No test pins it.
7. Minor — no daemon test drives a hold to show the journal line and the toast reach a person when the reply is dropped. **Covered by hand in S5** (a `ptt` hold through a flagged transcriber).
8. Minor — with toasts on, a keypress `dictate` shows both the toast and the reply. **Intended** (`DESIGN_30.md`, decision 4).

### S4 mutation testing (round 1)

Tester: a fresh general-purpose subagent, in a scratch copy, every mutation checked with the full `cargo test`. 47 mutations of the lines the diff adds (and one no-op control); 44 were killed, 3 survived. Each survivor was a real gap and is killed by a new test, re-checked by me in the foreground one mutation at a time:

| mutation | test that now fails | result |
|---|---|---|
| R4 — `n % m == 0 &&` removed (a block repeated plus a partial copy was flagged) | `repeat::tests::a_partial_block_after_the_copies_is_not_flagged` | killed |
| R16 — `.rev()` on the block sizes (largest block reported instead of smallest) | `repeat::tests::the_smallest_block_wins_when_two_sizes_fit` | killed |
| D11 — `path.replace('\n', " ")` removed in `probably_not_speech_line` | `daemon::tests::the_journal_line_is_one_line_when_the_take_path_holds_a_newline` | killed |

Not run by the tester: swapping the order of the journal write and the toast (no test is meant to notice, the design fixes the order only to say what is written first), and running the check on the rewritten text instead of the raw transcript (needs an engine that changes the transcript). Both stay open and are not claimed.

The tester reported that two mutations of the keep condition made the suite hang after the failure. With `repetition.is_none()` dropped, I ran the full suite in the foreground: it finished in 5 seconds with one failure and did not hang. The hang was not reproduced.

After the new tests: `cargo test` 804 passed, 1 ignored; clippy, `cargo fmt --check`, the manifest check clean (clippy and the Windows dead-code check are re-run before the pull request).

### S4 verdict
The code review (round 1) found nothing Critical; every finding is settled above. Mutation testing: 3 survivors out of 47, each killed by a new test. S4 closed on 2026-10-07 before a pull request existed.

### S5 Verify
- artifact: a section in `docs/evidence.md`, "A transcript that is one phrase repeated, for issue #30"
- platform: macOS 27.0.1, arm64, this machine
- run: a daemon from this worktree with its own state, configuration and herdr stand-in, never the owner's session. The baseline `e92f0b2` delivered the repeated text with no warning and deleted the take. The branch delivered it whole, kept the take, and said so in the reply, the journal and a toast; a `ptt` hold said so in the journal and the toast; ordinary text kept nothing and said nothing.
- not shown, and said in the evidence: a real silent room through a real transcriber, `[delivery] submit`, Linux, Windows.

```yaml
verdict:
  stage: S5
  artifact: docs/evidence.md
  result: pass
  date: 2026-10-07
```

### Approved
The wording of the warning (with "check what reached the pane") and the threshold of three repeats, one word included, were approved as they stand. Nothing in the code or the tests changes for it.

After the approval: `origin/main` was still `e92f0b2`, so there was nothing to merge. `cargo test` 804 passed, 1 ignored; `cargo clippy --all-targets -- -D warnings`, `cargo fmt --check`, `scripts/check_manifest.py` and the Windows dead-code check on a scratch copy are clean (`CARGO_BUILD_JOBS=6`).
