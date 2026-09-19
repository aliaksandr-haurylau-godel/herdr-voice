# RUN_90

| field | value |
|---|---|
| issue | #90 — Capture refuses a microphone that delivers 24-bit samples, so no take can start on that machine |
| input | GitHub issue, read with `gh issue view 90` |
| stage | S1 |
| branch | fix/90-capture-i24 |
| opened | 2026-09-19 |

## Stages

<!-- One block per stage, appended, never rewritten. -->

### S1 Assess
- artifact: `AC_90.md`
- produced: 2026-09-19

Written directly, on the precedent of `tasks/83/RUN_83.md`: `octoflow-assess`
is not installed in this environment and no project agent exists for the
"designer" role that gates S1, so the gate below is a self-review and says so.
That is a process gap the owner may want closed; this run does not invent a
reviewer to hide it.

The ticket was filed from a live failure, not from reading code: the error
string, the device name and the log ids in `AC_90.md` are from a real install of
`v0.1.0-beta.3` on the owner's machine. The API facts the fix rests on — that
cpal 0.18.2 exposes `cpal::I24` and that `dasp_sample` converts it to `f32` by
dividing by `8_388_608.0` — were read from the published crate sources, not
recalled.

Toolchain, recorded because it changes how S4 and S5 run: this machine had no
Rust toolchain when the run began (`cargo` and `rustc` resolved to nothing, no
`~/.cargo`). Visual Studio 2019 Build Tools with MSVC 14.29 and a Windows SDK
were present, so `rustup` was installed to the home directory with
`--profile minimal`, clippy and rustfmt, and with `--no-modify-path`; nothing
outside the home directory or the user's PATH was changed. It is removable with
`rustup self uninstall`.

```yaml
gate:
  stage: S1
  artifact: AC_90.md
  reviewer: designer (no project agent exists; self-reviewed against the issue
    body and the crate sources, per the tasks/83 precedent)
  verdict: READY
  date: 2026-09-19
  questions: []
  blocker: null
```

Self-review: every criterion traces to the issue's "Done when" or to a command
run in this session; the out-of-scope list separates the three things the person
will meet next (the invisible toast, the daemon timeouts, the missing
recognition engine) from this defect, and names the issue that owns the first.
Nothing needs the owner before design can begin.

S1 is closed. Next is S2 Design.

### S2 Design
- artifact: `DESIGN_90.md`
- produced: 2026-09-19

Written directly, classified bounded, gated by `octoflow-reviewer-planner` in
place of a live approval (the #83 and #36 precedent).

```yaml
gate:
  stage: S2
  artifact: DESIGN_90.md
  reviewer: octoflow-reviewer-planner
  verdict: READY
  date: 2026-09-19
  questions: []
  blocker: null
```

First-pass READY. Every citation was checked against the real files, and every
crate claim against the published sources: `cpal::I24` is public and implements
`SizedSample`, `I24::new` returns `Option`, `inner()` returns `i32`, and dasp
divides by `8_388_608.0`. The reviewer also found, unasked, that cpal's WASAPI
backend maps 24-bit input to `SampleFormat::I24` and unpacks it
(`host/wasapi/device.rs:234`, `stream.rs:625`), so the remaining risk is the
device, which S5 addresses.

One thing learned after the gate, that changes S5 and not the design: on Windows
the daemon's pipe name is the constant `herdr-voice`
(`src/transport.rs:178-181`, "the pipe namespace is machine-wide, which is
issue #6"), so a second daemon cannot run beside the one herdr started. S5
therefore drives `CpalSource` directly from a temporary, uncommitted test rather
than through a second daemon, the way `docs/evidence.md` records its other live
checks. Reaching the fix through `herdr plugin install` needs a release archive,
which the owner cuts.

S2 is closed. Next is S3 Plan.

### S3 Plan
- artifact: `PLAN_90.md`
- produced: 2026-09-19

Six tasks: witness the defect on unmodified code with a temporary hardware test
(1), three tests first (2), the function with both the scale and the sign
mutations witnessed (3), the `match` arm and one commit for all of it (4), the
same hardware test against the real device with the fix (5), the review gate and
S5 (6).

```yaml
gate:
  stage: S3
  artifact: PLAN_90.md
  reviewer: implementer
  verdict: QUESTIONS
  date: 2026-09-19
  questions:
    - "Task 3 Step 4 expected `cargo clippy --all-targets -- -D warnings` to pass
       while `i24_to_f32` still had no caller outside `#[cfg(test)]`; the non-test
       bin target reports `dead_code` and `-D warnings` makes it an error, so the
       step could not pass and the commit after it could not be made."
    - "`git status --short` was expected to print nothing, but `tasks/90/` is
       untracked in the worktree, so a clean tree could not be told from a dirty
       one."
    - "Task 1 pointed at a baseline `cargo test` result that lives in no file the
       executor can read, and that the plan gave no number for."
  blocker: null
```

### Answered

All three were right, and the third turned out to be worse than a missing
pointer: running the baseline showed it is **not green**. On unmodified `main` at
`7443094`, `cargo test` gives 545 passed and 5 failed with only the toolchain on
PATH (five tests run `echo` or `true`, which are not on this PowerShell's PATH),
and 547 passed and 3 failed with Git's Unix tools added. The remaining three are
`bias::tests::auto_falls_back_to_the_pane_when_the_transcript_misses`,
`auto_tries_both_on_a_double_miss` and
`transcript_source_on_a_miss_attempts_only_transcript`, each expecting a transcript
miss and getting a hit. Not investigated, since they have nothing to do with
capture; the likely cause is that they find real Claude session files under this
machine's home directory. Recorded here so no later claim of "the suite passes"
can be made from this machine.

Fixed in `PLAN_90.md`: Global Constraints now state the baseline, the PATH that
produces it and what "passes" means (no baseline-passing test fails, the three new
ones pass, total 550 passed and the same three failed); Task 1 states the number
to match; Task 3 no longer runs clippy or commits (the dead-code state is
expected and explained), and Task 4 commits tests, function and arm as one; the
expected `git status` is exactly `?? tasks/90/`; the printed `f32` was corrected
to `0.9999999`. `AC_90.md` AC-4 was reworded to the same standard, because as
written it required a green `cargo test` that this machine cannot give.

S3 continues; re-running the gate against the revised plan.

```yaml
gate:
  stage: S3
  artifact: PLAN_90.md
  reviewer: implementer
  verdict: READY
  date: 2026-09-19
  questions: []
  blocker: null
```

Second pass READY. The reviewer re-checked each answer against the current file,
the arithmetic (547 + 3 new = 550 passed, the same 3 failed), the task numbering
and the cross-references, and found no leftover "two commits". Two non-gating
notes, both accepted: no baseline run of `cargo fmt --check` or clippy existed in
the plan (taken in Task 1 below), and `python3` on this PATH was not confirmed
(it resolves; `scripts/check_manifest.py` ran in #83's S5).

S3 is closed. Next is S4 Implement.

### S4 Implement

Executed task by task with the red/green cycles witnessed, on `fix/90-capture-i24`.

- **Task 1.** Baselines on unmodified `main`: `cargo fmt --check` exit 0, clippy
  with `-D warnings` exit 0, `cargo test` as recorded above. A temporary hardware
  test driving `CpalSource` against the default input panicked with
  `"Microphone Array (Realtek(R) Audio)" delivers I24 samples, which this build
  does not read`: the defect, reproduced from source and not only from the log.
  Reverted; `git status --short` showed only `?? tasks/90/`.
- **Task 2.** Three tests written first. `cargo test` failed to compile with
  `E0425: cannot find function i24_to_f32`, three times.
- **Task 3.** The function with the divisor `8_388_607.0`: `silence_is_zero`
  passed, `the_lowest_sample_is_minus_one` failed (`left: -1.0000001`, `right:
  -1.0`) and `the_highest_sample_is_just_under_one` failed (`left: 1.0`, `right:
  0.9999999`). With `.abs()` and the right divisor, `the_lowest_sample_is_minus_one`
  failed (`left: 1.0`, `right: -1.0`). With the correct body all three passed, and
  the full suite gave `550 passed; 3 failed; 1 ignored`, the baseline's three
  names, and `cargo fmt --check` was clean. Both outputs matched the plan's
  predictions exactly.
- **Task 4.** The `I24` arm compiled against real cpal on the first try. `cargo
  build` exit 0; `cargo test` `550 passed; 3 failed; 1 ignored`, the same three;
  clippy with `-D warnings` exit 0; `python scripts/check_manifest.py` exit 0; the
  diff is 40 insertions and no deletions, so the `other =>` refusal is untouched.
  Committed as `490f53b`.
- **Task 5.** The same hardware test against the real default input, with the fix:
  it passed and printed `format=Format { rate: 48000, channels: 4 } samples=382080
  peak=0.013612628`. The device is four-channel, which the recorder's `to_mono`
  averages (`src/audio/resample.rs:42`), and the peak is room tone rather than
  digital silence. Reverted; `git status --short` showed only `?? tasks/90/`.

Not done, on purpose: the take did not go through a daemon. On Windows the pipe
name is one machine-wide constant, so a second daemon cannot run beside the
installed one. That last step needs a release archive, which the owner cuts.

## Gate S4

```yaml
gate:
  stage: S4
  artifact: the diff on fix/90-capture-i24 since 7443094 (490f53b)
  reviewer: general-purpose subagent, per superpowers:requesting-code-review
  verdict: Yes
  date: 2026-09-19
```

No Critical and no Important finding. The reviewer confirmed the conversion is
identical to `dasp_sample-0.11.0/src/conv.rs:261`; that cpal's WASAPI backend
sign-extends the 24-bit value into an `i32` before the callback
(`host/wasapi/stream.rs:831-841`), so `inner()` really spans the range the
conversion assumes; that exact `f32` equality in the tests is sound (every 24-bit
integer is representable and dividing by 2^23 only moves the exponent); and that
no other code in the repository describes which sample formats capture accepts.
It ran the three new tests itself.

Minor, none acted on here:

1. The `I16` arm divides by `i16::MAX` and the new arm by 2^23, a difference of
   0.003 % of full scale. `DESIGN_90.md` §1 decided not to touch the working arm.
2. `other =>` still refuses `I32`, `U16`, `F64` and the rest, so the next device
   that delivers one will fail the same way. Out of scope in `AC_90.md`; the
   reviewer suggests one follow-up issue converting the remaining formats
   generically through `dasp_sample::Sample`, replacing the per-format arms.
   **Open for the owner: file it or not.**
3. Only the pure function is unit-testable; the arm is covered by the live run.
4. Process, closed below: `docs/evidence.md` and `tasks/90/` are committed with the
   pull request.
5. A mid-range case (`I24::new(4_194_304)` giving `0.5`) would make "the middle" of
   the commit message literal; `silence_is_zero` stands in for it.

S4 is closed. Next is S5 Verify.

## Gate S5

```yaml
gate:
  stage: S5
  artifact: docs/evidence.md, "Capture from a 24-bit microphone, for issue #90"
  verdict: pass, with one gate stated as not green
  date: 2026-09-19
  platform: Windows 11 Home, Rust 1.98.1 msvc, cpal 0.18.2
```

- `cargo fmt --check`: exit 0. `cargo clippy --all-targets -- -D warnings`: exit 0.
  `python scripts/check_manifest.py`: exit 0, 12 entries.
- `cargo test`: `550 passed; 3 failed; 1 ignored`. Not green, and not called green:
  the same three `bias::` tests fail on unmodified `main` (547 passed, 3 failed),
  so the change adds three passing tests and no failure.
- Live device with the fix: 48 000 Hz, 4 channels, 382 080 samples in two seconds,
  peak 0.0136. Recorded with what it does and does not show.
- Not through a daemon; the Windows pipe name is machine-wide. Not through
  recognition; the machine has no engine configured. Both stated in the evidence.

S5 is closed. Next: the pull request for #90.
