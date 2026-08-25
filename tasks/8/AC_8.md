# AC_8 — Capture: record from a named device at 16 kHz mono, and refuse a silent take

Input: GitHub issue 8, `gh issue view 8`, no comments. `docs/design.md` section 4
(capture) and section 7 (configuration), `docs/evidence.md`, the prototype in
`spike/`.

## as-is

The frame is merged and the pipeline is empty. `src/daemon.rs` answers one frame
per connection and knows two commands: `cancel`, which needs no target pane, and
`stop`, which exists for the tests. `needs_target_pane` already names `dictate` and
`ptt` as pane-needing, and `answer` replies `"<command>: not implemented yet"` for
them when a pane is present. The dispatch in `src/main.rs` never sends them: they
fall into the arm that exits 69.

Configuration reads three keys — `[stt] model`, `[rewrite] engine`, `[rewrite]
agent` — and ignores every other key rather than failing, so a file already
carrying `[audio]` parses today and the values are dropped. There is no `[audio]`
table in `src/config.rs`.

`doctor` reports five things and none of them is a microphone: permission cannot be
established without capture code, which is why issue 3 left it out.

The prototype recorded through `ffmpeg` with `-f avfoundation -ar 16000 -ac 1`
(`spike/spike.sh:254`), measured loudness with `volumedetect` and read
`mean_volume` out of its output (`spike/spike.sh:171`), and refused a take quieter
than `SILENCE_DB`, default `-60` (`spike/spike.sh:63`, `spike/spike.sh:175`). It
resolved the input by name rather than index, and its comment records why: the
indices shift and a recording once landed on a foreign input in silence
(`spike/spike.sh:55`).

`docs/evidence.md` holds the numbers behind that: a take from the wrong input
measured −91 dB over 4.37 seconds and recognition returned a single period, while
a real take of comparable length measured −46.9 dB.

**Two facts about `cpal` established by running it, not by reading about it.**
Version 0.18.2, Apache-2.0, minimum Rust 1.78, below this crate's 1.82.

1. **No input device on the development machine offers 16 kHz.** Enumerating the
   CoreAudio host gave three input devices — a USB device, the built-in
   microphone, and a virtual device installed by a conferencing application. Every
   one reports a single supported input configuration of 48 kHz, one channel,
   `f32`; the built-in microphone additionally offers 44.1, 88.2 and 96 kHz. None
   offers 16 kHz at all. The prototype never met this because `ffmpeg` resampled
   for it.
2. **A device's name comes from `Display`, not from a `name()` method.** In 0.18
   `DeviceTrait` requires `Display` and offers `description()` and a stable
   `id()`; the `name() -> Result<String>` of earlier versions is gone.

## to-be

The daemon can take a recording. It opens the input the configuration names, or
the system default when the name is empty, records until told to stop, and leaves
a 16 kHz mono file that the recognition stage can read. A take whose mean volume
is below the configured threshold is refused, and the refusal names the device it
came from and the level it measured. Nothing looks at the audio beyond that.

## Requirements

Asked for by the issue:

- R1 A take starts, stops, and produces 16 kHz mono audio in a file.
- R2 The input device is selected by name, never by index.
- R3 The mean volume of a take is measured after recording, and a take below the
  threshold is refused before anything else reads it.
- R4 The recording file is unique per run, not a fixed path.
- R5 The device name and the threshold are configuration: `[audio] input` and
  `[audio] silence_db`, default `-60`.

Implied, and in scope only as far as R1 to R5 need them:

- R6 Conversion to 16 kHz mono inside this plugin. R1 cannot be met otherwise: the
  hardware does not offer that rate, and the plugin has no `ffmpeg`.
- R7 A container the recognition stage can read. The prototype wrote a WAV file and
  handed its path to a Whisper command-line tool.
- R8 No panic paths, and every failure names what to do next.
- R9 Tests that need neither a microphone nor a model, which is what the interfaces
  in `docs/design.md` section 4 exist for.

## Chosen readings

1. **What drives a take.** The issue says "a plain subcommand", which would be a
   new user-visible command name — and names are the owner's decision, not this
   stage's. The take is therefore driven by the `dictate` action the manifest
   already declares: the first invocation starts a take, the second stops it. No
   new name is introduced. `dictate` stops reporting "not implemented yet" and
   starts reporting where the recording went.
2. **How loud is loud enough.** `silence_db` is compared against the mean volume of
   the whole take, as the prototype did, not against a peak and not against a
   sliding window. A quieter measure of the same audio would change which takes are
   refused, and nothing asks for that.
3. **Which rate is authoritative.** The file is 16 kHz mono whatever the device
   gives. The device is opened at a configuration it actually supports and the
   samples are converted; opening it at 16 kHz is not attempted, because no device
   here offers it.

## Acceptance criteria

- **AC-1** `[audio] input` and `[audio] silence_db` are read from the
  configuration, each with a default: an empty name and `-60`. An absent file
  leaves both at their defaults, and an unknown key in `[audio]` does not fail.
- **AC-2** With `input` empty, the system default input device is used. With a name
  set, the device whose name equals it is used.
- **AC-3** A configured name that matches no device fails with a message that names
  what was configured and lists the input devices that do exist. It does not fall
  back to the default silently.
- **AC-4** No code path selects a device by index, and none exists that could: the
  selection takes a name.
- **AC-5** A take produces a file of 16 kHz mono 16-bit PCM audio in a WAV
  container, whatever sample rate and format the device delivered.
- **AC-6** The conversion is covered by a test that feeds known samples at a device
  rate and asserts the rate, the channel count and the sample count of the result,
  without opening a device.
- **AC-7** Each take writes to its own path. Two takes in one process, and two
  processes, never write to the same file.
- **AC-8** After a take, its mean volume in decibels is computed and reported.
- **AC-9** A take whose mean volume is below `silence_db` is refused, and the
  refusal names the device and the measured level. The audio is not handed on.
- **AC-10** A take above the threshold is accepted and its path is reported to the
  caller.
- **AC-11** The loudness measurement is covered by tests over synthesised samples:
  digital silence, a quiet signal below the threshold, and a normal one above it.
- **AC-12** The first `dictate` starts a take and the second stops it. Neither
  reports "not implemented yet" any more.
- **AC-13** A failure to open the device, a device that disappears mid-take, and a
  stop with no take running each produce a message naming the next action and no
  panic.
- **AC-14** `cargo test --all`, `cargo clippy --all-targets -- -D warnings`,
  `cargo fmt --check` and `python3 scripts/check_manifest.py` pass, and the five CI
  checks pass, including the suite on Windows.

## Out of scope / noticed

- Recognition, rewrite, delivery, indicator, push-to-talk timing — the issue puts
  them out of bounds. `ptt` keeps exiting 69.
- Microphone permission in `doctor`. Capture code makes it possible, and the issue
  does not ask for it.
- Real capture on Linux and Windows. Windows is issue 1.
- `cpal` 0.18 offers a stable device `id()`, which addresses the very failure the
  name rule was written against. `CLAUDE.md` says selection is by name, so name it
  is; moving to an identifier would be a change to a documented rule and belongs to
  whoever owns that rule, not to this issue.
- The recording is written to disk rather than kept in memory. The prototype wrote
  a file, the recognition stage in `docs/design.md` section 4 takes audio from a
  file, and nothing here asks for a change.

## Risk

GitHub issues carry no risk field. None was set; nothing is inferred.
