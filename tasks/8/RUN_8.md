# RUN_8

| field | value |
|---|---|
| issue | #8 — Capture: record from a named device at 16 kHz mono, and check the take was not silent |
| input | GitHub issue, read with `gh issue view 8` |
| stage | S2 |
| branch | feat/8-capture |
| opened | 2026-08-24 |

## Stages

<!-- One block per stage, appended, never rewritten. -->

### S1 Assess
- artifact: `AC_8.md`
- produced: 2026-08-24

Two facts were established by running `cpal` 0.18.2 rather than by reading about
it, and both change the work. No input device on this machine offers 16 kHz — all
three report 48 kHz, and the built-in microphone adds 44.1, 88.2 and 96 — so the
conversion the prototype got free from `ffmpeg` has to happen in this plugin. And a
device's name now comes from `Display`; the `name()` method of earlier versions is
gone.

The issue's phrase "a plain subcommand" was resolved to the `dictate` action the
manifest already declares, so that no user-visible name is invented at this stage.

```yaml
gate:
  stage: S1
  artifact: AC_8.md
  reviewer: designer
  verdict: READY
  date: 2026-08-24
  questions: []
  blocker: null
  notes:
    - "Not a gate question, does not withhold READY: the two `cpal` facts in the as-is section are the only claims in the artifact with nothing in the repository behind them. `cpal` is not yet a dependency, the probe crate was deleted, and neither fact reached `docs/evidence.md`, which `CLAUDE.md` says is where hand-verified measurements go. Design is not blocked: the API surface is checkable with `cargo add` at design time, and the resampling requirement stands regardless. Every other as-is claim checked out, including the four `spike/spike.sh` citations at the lines given."
```

The reviewer is right that the two measurements had no home in the repository, and
they now have one: `docs/evidence.md`, section "What the capture library offers",
with the platform they were taken on. The device names of the machine are
deliberately not written there — a list of somebody's audio devices identifies the
machine, and the shape of the finding does not need them.

### S2 Design
- artifact: `DESIGN_8.md`
- produced: 2026-08-25

Four decisions, and two of them are the ones that matter.

The recorder owns a thread for the daemon's whole life, and the `cpal` stream is
built, held and dropped there and nowhere else. A take spans two `dictate`
invocations, each arriving on a different connection handled on a different thread,
so the stream cannot live in a connection handler. Owning a thread also makes the
question of whether a `cpal` stream can move between threads irrelevant — the crate
does not state it plainly for every platform, and this way it never has to.

The rate conversion is restricted to integer ratios: the device is opened at 32,
48, 64 or 96 kHz, preferring 48, and a windowed-sinc low-pass at 7.5 kHz runs
before every *n*-th sample is taken. A device offering only 44.1 kHz is refused by
name rather than recorded badly. That turns a rational resampler into a filter and
a stride, and the low-pass is not optional: without it everything above 8 kHz folds
back into the band and goes straight into recognition.

Decisions taken here without the owner are also in `docs/decisions.md`.

## Notes

The probe used to establish the `cpal` facts was a throwaway crate outside the
repository, and was deleted.
