# herdr-voice

Voice dictation for [herdr](https://github.com/SuperCodeAgents/herdr-terminal).
Hold a key, speak, and the text appears in the input box of the agent pane you are
looking at — corrected against what that agent is actually working on, and never
submitted for you.

> **Status: design stage.** The design is written and measured, the plugin itself is
> not implemented yet. `spike/` holds the shell prototype the measurements came from;
> it works on macOS only and is not the product.

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

Not yet installable. When the first release is tagged:

```sh
herdr plugin install aliaksandr-haurylau-godel/herdr-voice
```

herdr plugin manifests cannot declare keybindings, so two lines go into your herdr
configuration. The plugin's `setup` action prints them and offers to append them.

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

macOS, Linux and Windows are the target. Only macOS has been exercised so far;
Linux and Windows need verification on real machines and are tracked as issues.

## License

MIT
