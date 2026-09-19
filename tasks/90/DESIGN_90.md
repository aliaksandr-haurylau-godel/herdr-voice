# DESIGN_90

Design for issue #90, against `tasks/90/AC_90.md` (gate S1: READY,
`tasks/90/RUN_90.md`).

Classified **bounded** under `superpowers:brainstorming`: one match arm and one
pure function in a file that already has the shape for both. As in #83 and
#36, the live approval that skill asks for is replaced by the project's own
gate, `octoflow-reviewer-planner`, because the run is not a chat. The decisions
below are engineering choices `CLAUDE.md` delegates; none is a scope, naming or
spend decision.

## 1. The conversion is its own function

```rust
/// A 24-bit sample as a float in `-1.0..=1.0`, scaled the way `dasp_sample`
/// scales it: the whole negative range is `-1.0`, the positive range stops
/// one step short of `1.0`.
fn i24_to_f32(sample: cpal::I24) -> f32 {
    sample.inner() as f32 / 8_388_608.0
}
```

It sits in `src/capture/cpal_source.rs` next to `pick_config` and `describe`,
the file's other free functions, and takes a `cpal::I24`, which is public in
cpal 0.18 (`pub use sample_format::{..., I24, U24}` in `lib.rs`). A function of
one argument that needs no device is what makes AC-2 testable in a file that
otherwise cannot be. `inner()` is `i32`; `as f32` is exact for every value in
the 24-bit range because an `f32` carries 24 bits of significand.

The divisor is `8_388_608.0`, that is 2^23, and not `f32::from(i16::MAX)`'s
analogue `8_388_607.0`. `dasp_sample` uses 2^23 for this conversion, and the
`I16` arm's divisor of `i16::MAX` is left as it is: changing a working arm is
outside the ticket, and the difference is 0.003 % of full scale.

## 2. One new arm

In the `match sample_format` of `CpalSource::start`, before `other =>`:

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

`cpal::I24` implements `SizedSample` (`sample_format.rs`: `impl SizedSample for
I24`), which is the bound `build_input_stream` places on its sample type, and it
is `Copy`, so `*s` is fine. The arm has the same shape as the `I16` one, and the
closure moves `sink` exactly as the other two do; only one arm executes, so the
three moves do not conflict.

The `other =>` arm is untouched, so every format not read still produces the
sentence it produced before. That is AC-3, by construction.

## 3. Tests

A `#[cfg(test)] mod tests` at the end of `src/capture/cpal_source.rs`, three
tests, written first:

- `the_lowest_sample_is_minus_one` — `i24_to_f32(I24::new(-8_388_608).unwrap())`
  equals `-1.0`.
- `silence_is_zero` — `I24::new(0)` gives `0.0`.
- `the_highest_sample_is_just_under_one` — `I24::new(8_388_607)` gives
  `8_388_607.0 / 8_388_608.0` and is strictly less than `1.0`.

`I24::new` returns `Option<I24>`, `None` outside the 24-bit range, so the
`unwrap()`s are on constants inside it. Exact `f32` equality is right here: the
inputs are integers below 2^24 and the divisor is a power of two, so the result
is exact.

Red is witnessed by running the tests before the function exists, which fails to
compile; then a second time with the function present but the divisor set to
`8_388_607.0`, which fails the first and third tests; then correct. The
mutation check is what shows the tests pin the scaling and not merely the sign.

## 4. What this design does not cover, and how it is closed instead

The stream itself — that WASAPI on the affected machine accepts an `I24` stream
and delivers non-zero samples — cannot be tested without the device. It is
closed by S5: a take from the real input on the real machine, through a daemon
built from this branch, recorded in `docs/evidence.md` with its outcome. If the
take then meets the missing recognition engine, that is recorded as the next
thing and the capture claim is limited to what the level line shows.

## 5. Coverage against AC_90.md

| AC | Covered by |
|---|---|
| AC-1 | §2, verified by S5's real take |
| AC-2 | §1, §3 |
| AC-3 | §2 (the `other =>` arm is not edited) |
| AC-4 | Not a design concern; the plan's S5 task runs the four gates and reads the Windows CI job |
| AC-5 | §4 |

## What this design does not decide

The names of the three tests and the wording of the doc comment are left to the
implementer; both are mechanical once the contract above is fixed.
