# 24-bit capture Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let `CpalSource::start` open an input that delivers 24-bit integer
samples, so a take on such a device is recorded instead of refused.

**Architecture:** One pure function `i24_to_f32` and one new `match` arm in
`src/capture/cpal_source.rs`, the file that owns the sound card. The `I16` arm is
the template; the `other =>` refusal is left alone for every format still not
read.

**Tech Stack:** Rust 1.98.1 (installed for this run, MSVC target), `cpal` 0.18.2
with `dasp_sample` 0.11, no new dependency.

**Spec:** `tasks/90/DESIGN_90.md`, against `tasks/90/AC_90.md` (S1: READY,
S2: READY, `tasks/90/RUN_90.md`).

## Global Constraints

- Only `src/capture/cpal_source.rs` changes in code. `Cargo.toml` does not.
- No panic paths in production code (`CLAUDE.md`). `unwrap()` is allowed in the
  new tests only, on constants inside the 24-bit range.
- Every comment is one short line at most, and only where the reason is not
  obvious.
- Every command runs from the `herdr-voice-90` worktree in PowerShell with
  `$env:PATH = "$env:USERPROFILE\.cargo\bin;C:\programs\PortableGit\usr\bin;$env:PATH"`
  set first. The first entry is the toolchain; the second supplies the `echo` and
  `true` that five existing tests run, and sits after cargo so Git's `link.exe` is
  never the one found first.
- **The baseline is not green, and the plan does not pretend it is.** On unmodified
  `main` (`7443094`) on this machine, with the PATH above, `cargo test` gives
  `547 passed; 3 failed; 1 ignored`. The three failures are
  `bias::tests::auto_falls_back_to_the_pane_when_the_transcript_misses`,
  `bias::tests::auto_tries_both_on_a_double_miss` and
  `bias::tests::transcript_source_on_a_miss_attempts_only_transcript`: each expects
  a transcript miss and gets a hit. Without Git's tools on PATH the run is
  `545 passed; 5 failed`, the two extra being tests that run `echo` and `true`.
  Neither set touches capture, and neither is investigated here.
  "Passes" below therefore means: no test that passed at the baseline fails, the
  three new tests pass, and the total is `550 passed; 3 failed; 1 ignored` with
  the same three names.
- The public-repository rule: no account name, no absolute home path in any
  file that is committed. Commands above are instructions, not file content.

---

### Task 1: Witness the real defect on unmodified code

**Files:**
- Modify (temporary, reverted at the end of this task): `src/capture/cpal_source.rs`

**Interfaces:**
- Consumes: `CpalSource`, `Sink` (`src/capture.rs:38`), `Source::start`.
- Produces: nothing that survives the task — a recorded outcome only.

- [ ] **Step 1: Confirm the tree is clean and the baseline suite is as recorded**

Run: `git status --short` — expected exactly one line, `?? tasks/90/` (the run
artifacts, committed at the end). Nothing else may appear.
Run `cargo test` on this unmodified tree and confirm it matches the baseline in
Global Constraints: `547 passed; 3 failed; 1 ignored`, the same three names.
If it differs, stop and record what it gave.

- [ ] **Step 2: Add a temporary hardware test**

Append to the end of `src/capture/cpal_source.rs`:

```rust
#[cfg(test)]
mod live {
    use super::*;
    use crate::capture::Sink;

    #[test]
    #[ignore]
    fn live_default_input() {
        let sink = Sink::default();
        let mut source = CpalSource::new();
        let format = source.start(None, sink.clone()).expect("start");
        std::thread::sleep(std::time::Duration::from_secs(2));
        source.stop();
        let samples = sink.take_samples();
        let peak = samples.iter().fold(0f32, |m, s| m.max(s.abs()));
        println!("format={format:?} samples={} peak={peak}", samples.len());
        assert!(!samples.is_empty());
    }
}
```

- [ ] **Step 3: Run it and record the failure**

Run: `cargo test live_default_input -- --ignored --nocapture`

Expected, on the machine the issue came from: the test panics at `expect("start")`
with `"Microphone Array (Realtek(R) Audio)" delivers I24 samples, which this build
does not read`. That is the defect, reproduced from source, not from the log.
If it instead starts and prints samples, stop: the defect does not reproduce on
this checkout and the run needs the owner.

- [ ] **Step 4: Revert the temporary test**

Run: `git checkout -- src/capture/cpal_source.rs` then `git status --short` —
expected exactly `?? tasks/90/`.

---

### Task 2: Write the failing tests

**Files:**
- Modify: `src/capture/cpal_source.rs` (append after `describe`)

**Interfaces:**
- Consumes: `cpal::I24` (`I24::new(i32) -> Option<I24>`).
- Produces: three tests that call `i24_to_f32(cpal::I24) -> f32`, which Task 3
  defines in the same file above the test module.

- [ ] **Step 1: Append the test module**

After the closing brace of `fn describe`, append:

```rust

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_lowest_sample_is_minus_one() {
        let lowest = cpal::I24::new(-8_388_608).unwrap();
        assert_eq!(i24_to_f32(lowest), -1.0);
    }

    #[test]
    fn silence_is_zero() {
        let silence = cpal::I24::new(0).unwrap();
        assert_eq!(i24_to_f32(silence), 0.0);
    }

    #[test]
    fn the_highest_sample_is_just_under_one() {
        let highest = cpal::I24::new(8_388_607).unwrap();
        let converted = i24_to_f32(highest);
        assert_eq!(converted, 8_388_607.0 / 8_388_608.0);
        assert!(converted < 1.0);
    }
}
```

- [ ] **Step 2: Run them and confirm they fail**

Run: `cargo test --bin herdr-voice capture::cpal_source::tests`.

Expected: the build fails with `cannot find function `i24_to_f32` in this scope`
(error E0425), three times. This is the red state: the tests exist and the
function does not.

---

### Task 3: The function, with both mutations witnessed

**Files:**
- Modify: `src/capture/cpal_source.rs` (insert between `describe` and `mod tests`)

**Interfaces:**
- Consumes: `cpal::I24::inner(self) -> i32`.
- Produces: `fn i24_to_f32(sample: cpal::I24) -> f32`, private to the module,
  used by Task 4's arm and by Task 2's tests.

- [ ] **Step 1: Add the function with a wrong divisor**

Between `fn describe`'s closing brace and `#[cfg(test)]`, insert:

```rust

/// A 24-bit sample as a float in `-1.0..=1.0`, scaled the way `dasp_sample` does.
fn i24_to_f32(sample: cpal::I24) -> f32 {
    sample.inner() as f32 / 8_388_607.0
}
```

Run: `cargo test --bin herdr-voice capture::cpal_source::tests`

Expected: `silence_is_zero` passes; `the_lowest_sample_is_minus_one` fails
(`left: -1.0000001`, `right: -1.0`) and `the_highest_sample_is_just_under_one`
fails (`left: 1.0`, `right: 0.9999999`; Rust prints the shortest form of the
`f32`). The tests pin the scale.

- [ ] **Step 2: Mutate the sign handling**

Change the body to `sample.inner().abs() as f32 / 8_388_608.0`.
Run the same command. Expected: `the_lowest_sample_is_minus_one` fails
(`left: 1.0`, `right: -1.0`). The tests pin the sign.

- [ ] **Step 3: Write the correct function**

Set the body to `sample.inner() as f32 / 8_388_608.0`.
Run the same command. Expected: `test result: ok. 3 passed`.

- [ ] **Step 4: Run the suite and the format gate, not clippy yet**

Run: `cargo test`, then `cargo fmt --check`.
Expected: `550 passed; 3 failed; 1 ignored`, the same three names as the baseline,
and a clean `cargo fmt --check`. If it reports a diff, run `cargo fmt` and re-run
it; formatting is the only permitted change.

Do not run `cargo clippy --all-targets -- -D warnings` here. Until Task 4 the
function has no caller outside `#[cfg(test)]`, so the non-test bin target reports
`dead_code`, which `-D warnings` turns into an error (`src/main.rs:117-120` records
the same effect for another item). That is expected, and is why there is **no
commit at this task**: a commit here would not pass CI. Task 4 commits both.

---

### Task 4: The match arm

**Files:**
- Modify: `src/capture/cpal_source.rs` (the `match sample_format` in `CpalSource::start`)

**Interfaces:**
- Consumes: `i24_to_f32` from Task 3, `Event::Samples(Vec<f32>)`,
  `Sink::push(&self, Event)`.
- Produces: `CpalSource::start` returning `Ok` for a 24-bit input.

- [ ] **Step 1: Add the arm**

In the `match sample_format`, between the `cpal::SampleFormat::I16 => ... ,` arm
and `other => {`, insert:

```rust
            cpal::SampleFormat::I24 => picked.build_input_stream::<cpal::I24, _, _>(
                stream_config,
                move |data, _| {
                    sink.push(Event::Samples(
                        data.iter().map(|s| i24_to_f32(*s)).collect(),
                    ))
                },
                on_error,
                None,
            ),
```

- [ ] **Step 2: Build and run the gates**

Run: `cargo build`, `cargo test`, `cargo fmt --check`,
`cargo clippy --all-targets -- -D warnings`. Expected: the build succeeds;
`cargo test` gives `550 passed; 3 failed; 1 ignored` with the baseline's three
names; fmt and clippy are clean, the function now being used.
If the build rejects a `move` of `sink` or `on_error` in more than one arm, the
compiler names it; each arm already moves them and only one runs, so this is not
expected.

- [ ] **Step 3: Check the `other =>` arm was not edited**

Run: `git diff -- src/capture/cpal_source.rs` and read it. Expected: the `other =>`
arm and its sentence appear only as unchanged context lines.

- [ ] **Step 4: Commit tests, function and arm together**

```bash
git add src/capture/cpal_source.rs
git commit -m "fix: read a microphone that delivers 24-bit samples

CpalSource::start refused every format but f32 and i16, so an input such as the
Realtek array on the machine issue #90 came from ended each take with 'delivers
I24 samples, which this build does not read'. Add i24_to_f32, scaled by 2^23 the
way dasp_sample scales it and pinned by three tests at the bottom, the middle and
the top of the range, and build an i24 stream with it. Every other format still
reaches the same refusal."
```

---

### Task 5: The real device, with the fix

**Files:**
- Modify (temporary, reverted at the end of this task): `src/capture/cpal_source.rs`

**Interfaces:**
- Consumes: the commit from Task 4.
- Produces: the numbers for `docs/evidence.md`, and nothing else in the tree.

- [ ] **Step 1: Re-add the temporary hardware test**

Append the same `mod live` block as Task 1, Step 2.

- [ ] **Step 2: Run it against the real input**

Run: `cargo test live_default_input -- --ignored --nocapture`

Expected: the test passes and prints `format=Format { rate: 48000, channels: .. }`,
a sample count close to two seconds at that rate, and a peak. Record all three,
the device name, and whether anyone spoke. A peak of `0` means digital silence,
which is a finding, not a pass; say so.

- [ ] **Step 3: Revert the temporary test**

Run: `git checkout -- src/capture/cpal_source.rs`, then `git status --short` —
expected exactly `?? tasks/90/`.

---

### Task 6: Code review gate, then S5 evidence

Not a code task. Invoke `superpowers:requesting-code-review` on the diff since
`7443094` and record the verdict in `tasks/90/RUN_90.md`. Then S5: run
`cargo test` (expecting the `550 passed; 3 failed` result of Global Constraints, and
saying so in the evidence rather than "green"), `cargo clippy --all-targets -- -D
warnings`, `cargo fmt --check` and `python3 scripts/check_manifest.py` fresh, and
write the section into
`docs/evidence.md` with the platform and Task 5's numbers, saying plainly that the
take did not go through a daemon and why. Read the Windows CI job on the pull
request and record whether it compiled the change.

## Self-review

**Spec coverage.** `DESIGN_90.md` §1 → Task 3. §2 → Task 4. §3 → Tasks 2 and 3,
both mutations witnessed. §4 → Tasks 1 and 5. AC-1 → Task 5; AC-2 → Tasks 2–3;
AC-3 → Task 4 Step 3; AC-4 → Tasks 4 and 6; AC-5 → Task 6.

**Placeholder scan.** No "TBD", no "handle edge cases"; every code step is the
literal text, and every expected output is a real message or a stated shape.

**Type consistency.** `i24_to_f32(cpal::I24) -> f32` is spelled the same in
Tasks 2, 3 and 4; the temporary test module is the same block in Tasks 1 and 5.
