# PLAN_8 — capture

> Execute with `superpowers:executing-plans` and `superpowers:test-driven-development`:
> the failing test first, then the code, then the commit, one task at a time.

**Goal:** the daemon records a take from a named device, writes it as 16 kHz mono,
and refuses one that captured nothing.

**Spec:** `tasks/8/DESIGN_8.md`, built from `tasks/8/AC_8.md`. Read both.

## Global constraints

- Rust 2021, `rust-version = "1.82"`. `cpal` 0.18.2 needs 1.78.
- One new dependency, `cpal` 0.18.2, default features. No asynchronous runtime.
- English everywhere inside the repository; paths cited relative to the root.
- No panic paths. Every failure names what to do next.
- Four local checks and five CI checks stay green: `cargo test --all`,
  `cargo clippy --all-targets -- -D warnings`, `cargo fmt --check`,
  `python3 scripts/check_manifest.py`.
- Run `cargo fmt` before every commit; the format check is not forgiving of line
  width and it is cheaper to run it than to amend.

## Order

`audio::level`, `audio::wav`, `audio::device` and `audio::resample` have no
dependencies on each other and can be done in any order. `capture` needs the four.
`config` is independent. `daemon` and `main` need `capture` and `config`. The CI
step is independent of all of it and should be done first, because without it the
Linux job fails on every push from the moment `cpal` enters `Cargo.toml`.

---

### Task 0: the ALSA headers in CI

**Input:** `.github/workflows/check.yml`.
**Output:** a step, before the build, that installs the ALSA development headers on
`ubuntu-latest` only.
**Done when:** the workflow file has the step guarded by `runner.os == 'Linux'`, and
a push shows the Linux job still green.
**Depends on:** nothing. Do this first: `cpal` breaks the Linux job the moment it
lands in `Cargo.toml`, and a red Linux job hides everything after it.

---

### Task 1: `audio::level`

**Input:** a slice of `f32` samples.
**Output:** `pub fn mean_dbfs(samples: &[f32]) -> f32` — the root mean square of the
take expressed in decibels relative to full scale, and a floor value for digital
silence rather than an infinity.
**Done when:** tests cover digital silence, a tone below `-60` dBFS, and a tone
above it, each asserting the value within a tolerance rather than exactly.
**Depends on:** nothing.
**Watch for:** `20 · log₁₀(rms)`, not `10 · log₁₀`. An empty slice is silence, not a
division by zero.

---

### Task 2: `audio::wav`

**Input:** 16 kHz mono `f32` samples.
**Output:** `pub fn write(path: &Path, samples: &[f32], rate: u32) -> io::Result<()>`
writing 16-bit PCM in a WAV container, and a private encoder returning the bytes so
the header can be tested without a file.
**Done when:** tests assert the `RIFF`/`WAVE` magic, the sample rate, one channel,
16 bits per sample, and that the data chunk length matches the sample count; and one
test writes to a temporary path and reads the header back.
**Depends on:** nothing.
**Watch for:** clamping `f32` to `i16` rather than wrapping. A sample outside
`[-1, 1]` must saturate.

---

### Task 3: `audio::device`

**Input:** the configured name and the list of device names the host offers.
**Output:** `pub fn choose<'a>(configured: &str, available: &'a [String]) -> Result<Choice<'a>, DeviceError>`
where `Choice` is either the host default or a named device, and `DeviceError`
carries the list of names that exist.
**Done when:** tests cover an empty name, an exact match, a name matching nothing
with every available name present in the message, and a duplicate name resolving to
the first with the ambiguity reported.
**Depends on:** nothing. This function never touches `cpal`; it takes names.
**Watch for:** no index anywhere in the signature or the body.

---

### Task 4: `audio::resample`

**Input:** interleaved `f32` samples at a device rate and channel count.
**Output:** three functions — `to_mono`, `ratio_for(rate: u32) -> Option<u32>`
returning the integer ratio to 16 kHz, and `to_16k(samples: &[f32], rate: u32)
-> Result<Vec<f32>, ResampleError>` applying a windowed-sinc low-pass at 7.5 kHz and
taking every *n*-th sample.
**Done when:** tests cover channel averaging; `ratio_for` accepting 32, 48, 64 and
96 kHz and refusing 44 100 with a message naming it; a 1 kHz tone at 48 kHz
surviving decimation with the expected sample count and amplitude; and a 12 kHz tone
coming out attenuated rather than folded down to 4 kHz.
**Depends on:** nothing.
**Watch for:** the aliasing test is the point of this task. Write it first and make
sure it fails without the filter.

---

### Task 5: `capture`

**Input:** the choice from Task 3 and the configuration from Task 6.
**Output:**
- `pub enum Event { Samples(Vec<f32>), Failed(String) }` and a source that produces
  them, with two implementations: `cpal`, and a fake driven by a script.
- `pub struct Recorder` owning one thread for the daemon's life, with
  `start()`, `stop() -> Result<Take, CaptureError>`, and a remembered failure.
- `pub struct Take { pub path: PathBuf, pub level_dbfs: f32 }`.
- The take's path: `<state>/takes/<milliseconds>-<pid>-<counter>.wav`.
**Done when:** tests drive the fake source through start, stop, start-while-running,
stop-with-nothing, a source that fails mid-take, and a take below the threshold —
and none of them opens a device. The refusal names the device and the level; the
refused and the failed take both have their files removed.
**Depends on:** Tasks 1, 2, 3, 4.
**Watch for:** the `cpal` stream is built, held and dropped on the recorder's own
thread and nowhere else. If the compiler complains about `Send`, the fix is to move
work onto that thread, never to wrap the stream.

---

### Task 6: `config`

**Input:** `src/config.rs`.
**Output:** an `[audio]` table with `input: String` defaulting to empty and
`silence_db: f32` defaulting to `-60.0`.
**Done when:** tests cover the defaults, a partial file, and an unknown key inside
`[audio]` being ignored. The existing test that proves `[audio]` parses and is
dropped is updated rather than deleted.
**Depends on:** nothing.

---

### Task 7: `dictate`, and the wiring

**Input:** `src/daemon.rs`, `src/main.rs`.
**Output:** the daemon owns a `Recorder`, `dictate` toggles a take, and `main`
dispatches `dictate` to the client instead of exiting 69.
**Done when:**
- the first `dictate` starts a take and answers that recording began;
- the second stops it and answers with the path and the level, or with the refusal;
- a `dictate` after a mid-take device failure reports the remembered reason and
  clears it, and does not start a take;
- `ptt`, `setup`, `status`, `model` and `mic` still exit 69, and the guard test in
  `src/main.rs` is updated to say so;
- `cargo test --all`, clippy, fmt and the manifest check all pass.
**Depends on:** Tasks 5 and 6.
**Watch for:** `needs_target_pane` already lists `dictate`. A take needs no pane —
the pane matters at delivery — so `dictate` moves off that list, and the test that
asserts it is on the list changes with it. Say so in the commit message: it reverses
a decision from issue #3, which took it on the assumption that dictation delivers
in one step.

---

### Task 8: verify by hand, and record it

**Input:** a built release binary and a real microphone.
**Output:** a section in `docs/evidence.md`, with the platform.
**Done when:** these are run and their results written down as they happened —
a take from the default device; a take with `input` set to a name that exists; a
name that does not exist; a take made with the microphone muted, which must be
refused with the device and the level in the message; and the resulting file
inspected to confirm 16 kHz, one channel, 16-bit.
**Depends on:** Task 7.
**Watch for:** the muted-microphone take is the one that matters. It is the failure
the whole silence check exists for, and it is worth more than the successful one.
