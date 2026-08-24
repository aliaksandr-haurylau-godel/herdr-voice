# Shell prototype

The prototype the design came from. macOS only, not built, not shipped, not
maintained. It is kept because every number in `../docs/evidence.md` was produced
with it, and because it is the executable form of the pipeline the plugin
reimplements.

- `run.sh` — POSIX launcher. A detached multiplexer binding starts with a minimal
  environment, where `env bash` resolves to a bash old enough that it cannot parse
  the main script; this repairs `HOME` and `PATH` and hands over to a modern bash.
- `spike.sh` — the pipeline: record, transcribe, rewrite with context, insert.
  Modes: hold-to-talk poke and watchdog, toggle, cancel, status, microphone
  selection, and a measurement mode that prints three transcription variants.
- `context.sh` — context collection: the agent's conversation, recently touched
  file names, branch, pane title.
- `keyprobe.sh` — the key auto-repeat experiment.

Requires a modern bash, ffmpeg, jq, a whisper command-line tool with a model, and a
coding-agent command-line tool for the rewrite stage.
