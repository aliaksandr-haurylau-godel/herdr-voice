# RUN_8

| field | value |
|---|---|
| issue | #8 — Capture: record from a named device at 16 kHz mono, and check the take was not silent |
| input | GitHub issue, read with `gh issue view 8` |
| stage | S1 |
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

## Notes

The probe used to establish the `cpal` facts was a throwaway crate outside the
repository, and was deleted.
