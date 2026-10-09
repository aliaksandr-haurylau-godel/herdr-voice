# herdr-voice

Voice dictation for [herdr](https://github.com/SuperCodeAgents/herdr-terminal).
Hold a key, speak, and the text appears in the input box of the agent pane you are
looking at — corrected against what that agent is actually working on, and never
submitted for you.

> **Status: prerelease.** Version `0.1.0-beta.4` is published as a prerelease; see
> [Install](#install). What has been run, and on which platform, is listed under
> [Platforms](#platforms). `spike/` holds the shell prototype the measurements came
> from; it works on macOS only and is not the product.

## What it does

- **Hold to talk.** Holding a key records; releasing it transcribes and inserts.
  A separate toggle exists for long dictation.
- **Knows the context.** The transcript is repaired using the target agent's own
  conversation, the repository's recently touched file names and the git branch, so
  technical terms and file names survive.
- **Inserts, never sends.** The text lands in the input box. Stack several takes,
  edit them, send when you are ready.
- **Swappable engines.** Speech recognition runs locally by default, or through any
  Whisper-compatible endpoint, or through a command you name. The rewrite stage uses
  a coding-agent command-line tool found in `PATH`, an OpenAI-compatible endpoint, or
  a command you name.
- **Says what it is doing.** A blinking indicator with elapsed time shows in the
  sidebar and in the tab label, so a collapsed sidebar does not hide the state.

## Why it exists

Terminal dictation tools transcribe what you said. They do not know that the word
you just spoke is a directory in the repository the agent is working in. The
multiplexer does know, and this plugin uses that: the same phrase that comes back
as an unrelated word from plain recognition comes back correct when the agent's
context is part of the request. The numbers are in [docs/evidence.md](docs/evidence.md).

## Install

Four prereleases are published, `v0.1.0-beta.1` to `v0.1.0-beta.4`; the newest is
`v0.1.0-beta.4`. A prerelease is not offered as the current release, so installing one
means naming it:

```sh
herdr plugin install aliaksandr-haurylau-godel/herdr-voice --ref v0.1.0-beta.4
```

The install fetches the archive built for your machine, checks it against the
`.sha256` file published beside it and unpacks it, so nothing is compiled and no Rust
toolchain is needed. Archives exist for macOS on arm64 and x86_64, Linux on x86_64 and
arm64 (built against glibc, so not Alpine) and Windows on x86_64. On any other platform
the install compiles from source and needs a Rust toolchain (on Linux also the ALSA
development headers, `libasound2-dev` on Debian), and it does the same when
the archive is not found at the release (an answer of 404 that lasts through five
attempts). If the archive cannot be reached for any other reason, it stops and says so
instead of compiling. Every release is listed, with its
own install line, at
[github.com/aliaksandr-haurylau-godel/herdr-voice/releases](https://github.com/aliaksandr-haurylau-godel/herdr-voice/releases).

herdr plugin manifests cannot declare keybindings, so four blocks go into your
herdr configuration. Invoke the plugin's `setup` action — from a terminal,
`herdr plugin action invoke herdr-voice.setup` — and it opens a pane that
shows the exact blocks, offers to append them, and says what it changed. Running
it twice changes nothing the second time.

The settings are one popup. Press the key `setup` offered for it (`prefix+shift+s`),
or run `herdr plugin action invoke herdr-voice.settings`. It lists every section and
key of `config.toml` with its value and marks the ones you have not set; you change a
key by typing its number and the new value. The microphone is chosen there by name
and the next take uses it at once. The speech model is chosen from the catalogue, and
the rewrite model from the list your server serves. A change to anything but the
microphone takes effect when herdr is restarted, and the popup says so. Tokens are
never shown, only whether they are set, and the two `command` lists are changed in
the file.

If you used this plugin before its id became `herdr-voice`, your bindings name
an id that no longer exists and the keys do nothing. The daemon says so once
when herdr starts it, naming the keys, and `setup` offers to repair them: it
rewrites those blocks where they are, so the keys and anything you wrote around
them stay as they were. It also tells you where your old configuration file is,
and how to end a daemon left running under the old id.

Speech recognition has three engines. The default is `command`: give
`[stt] command` the program and arguments to run — for example a local
`whisper-cli` invocation — because that is the fastest of the three on the
machine this was measured on.

It can also run with no external program at all. Set `[stt] engine = "candle"`
and install a model with `herdr-voice model --choose`, which lists the models
with their sizes, downloads the one you pick and verifies it against a pinned
byte count and digest before anything loads it. `herdr-voice model` lists them
without installing anything. That engine is slower — see `docs/evidence.md` —
and needs nothing on the machine but this plugin.

## Building it

```sh
cargo test
herdr plugin link .    # install this checkout as a plugin
```

The binary accepts every subcommand the plugin manifest names; the ones that are
not built yet exit with a distinct code and say so, so "not yet" is never confused
with "unknown".

## Documentation

- [docs/design.md](docs/design.md) — how the plugin is built and why
- [docs/evidence.md](docs/evidence.md) — the measurements behind the design
- [CLAUDE.md](CLAUDE.md) — how work is run in this repository
- [CONTRIBUTING.md](CONTRIBUTING.md) — what a contribution has to satisfy

## Platforms

macOS, Linux and Windows are the target. What has been run on each, with the section
of [docs/evidence.md](docs/evidence.md) that records it:

- **macOS** (Apple silicon): capture, recognition, push-to-talk, the indicator and
  `setup` were each run by hand — "Capture, by hand on macOS", "Recognition, by hand
  on macOS", "Push-to-talk, by hand on macOS", "The indicator, by hand on macOS" and
  "`setup`, by hand on macOS".
- **Windows 11**: the install script's tests pass under Windows PowerShell 5.1 and
  PowerShell 7 ("The Windows install script's tar/PATH fix, for issue #83"), and
  capture opens the machine's default input, a 4-channel 24-bit device, and receives
  samples ("Capture from a 24-bit microphone, for issue #90"). Recognition of speech
  through that input, and fetching a release archive on Windows, are not recorded there.
- **Linux**: in an `aarch64` Debian container with no sound hardware, on an early
  revision, the plugin was built with `cargo build --release`, linked with
  `herdr plugin link`, and its daemon was started by herdr through the manifest's
  `[[startup]]` entry ("Linux, in a container"). Capture from a real microphone on
  Linux, and `x86_64`, are not established.

## License

MIT
