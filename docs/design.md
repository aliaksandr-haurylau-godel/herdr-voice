# herdr-voice — design

Voice dictation for [herdr](https://github.com/SuperCodeAgents/herdr-terminal): hold
a key, speak, and the text lands in the input box of the agent you are looking at.
The plugin corrects the transcript against what that agent is actually working on
before inserting it, and never presses Enter on your behalf.

This document states how the plugin is built. Measurements that back the numbers
live in `docs/evidence.md`.

## 1. What the plugin does

### Context

herdr runs several coding agents side by side in panes. Typing long prompts into
them is slow, and dictation tools that exist today either live outside the
terminal or transcribe word for word without knowing what is on screen.

### Problem

Two things are missing at once. First, a way to speak into the pane that is in
focus without touching the mouse or opening another window. Second, a transcript
that survives technical speech: file names, flags, English terms inside another
language. Plain speech recognition turns `worklog` into an unrelated word, and a
wrong word inside a prompt costs more than typing it would have.

### Decision

A herdr plugin that records while a key is held, transcribes locally, repairs the
transcript using the target agent's own conversation and repository as context,
and inserts the result into that agent's input box without submitting it.

### Why

The multiplexer already knows everything the correction needs: which agent owns
the pane, its working directory, its git branch, and its conversation. No external
dictation tool has access to that. Insertion without submission lets several takes
be stacked and edited before sending.

## 2. Structure

### Context

Holding a key produces key auto-repeat, and herdr invokes a bound command on every
repeat: measured at roughly twelve invocations per second, with 44–99 milliseconds
before the second event and a median of 85 milliseconds after that.

### Problem

At twelve invocations a second, any real work performed per keypress is wasted
twelve times over, and process startup alone would dominate.

### Decision

A daemon plus a thin client. The manifest's `startup` entry launches the daemon,
which owns the model, the recording and the whole pipeline. Every plugin action is
a short run of the same binary that writes to the daemon's socket and exits: a
Unix domain socket on macOS and Linux, a named pipe on Windows.

Manifest actions: `dictate` (toggle for long takes), `ptt` (one keypress of
hold-to-talk), `cancel`, `setup`, `mic`, `model`. Manifest panes: `status`,
`setup`, `download`. Each manifest entry carries a `platforms` list, so platform
differences are declared rather than branched in code.

### Why

The daemon keeps the speech model resident, which removes the model load from
every dictation, and it turns each keypress into a socket write of a few bytes.

## 3. Target pane

### Context

herdr passes an invocation context to every plugin command: `focused_pane_id`,
`focused_pane_agent`, `focused_pane_cwd`, `focused_pane_status`, `tab_id`,
`tab_label`, `workspace_id`, `selected_text`.

### Problem

Deriving the target from `herdr agent list` requires guessing between the focused
pane, the calling tab and the workspace, and the guess fails silently as soon as
two agents are live in the same place.

### Decision

The target is the pane herdr names in the invocation context. Nothing is inferred.
An explicit target may still be passed for scripted use.

### Why

The multiplexer knows the answer exactly; recomputing it can only introduce a
disagreement between what the user is looking at and where the text is delivered.

## 4. Pipeline

Four stages, each with an interface and more than one implementation.

### Capture

Audio comes from `cpal`, which covers the three platforms with one interface. The
input device is selected **by name**, never by index: device indices shift as soon
as a headset is connected or removed, and a shifted index sends the recording to a
different input in silence.

Recorded audio is 16 kHz mono. After each take the mean volume is checked; below
the configured threshold the take is rejected as "wrong input" rather than passed
to recognition, because a silent recording otherwise reaches the transcriber and
comes back as a single punctuation mark.

### Transcription

Three interchangeable engines:

- `candle` — the built-in engine, pure Rust, no external toolchain. The model is
  chosen by the user on first run from a list with sizes, and downloaded then.
  Models live in `<state>/models/candle/<identifier>/`, three files each, and a
  download is verified against a pinned byte count and SHA-256 before anything
  loads it: a truncated or substituted file is a named failure at that point
  rather than a confusing one later.
- `http` — any Whisper-compatible endpoint, cloud or a local server.
- `command` — an arbitrary external program that reads audio and prints text.

The model name and the spoken language are configuration, not code.

### Context

Assembled from herdr and from the repository the target agent works in:

- The last turns of that agent's conversation, read from its session transcript,
  each turn cut to its first 300 characters. Machine turns — task notifications,
  system reminders, cross-session messages — are filtered out; they carry nothing
  about speech.
- Recently touched file and directory names, taken from `git status` and the last
  commits, from the repository root rather than the agent's subdirectory.

Those two are the whole of it. The git branch, the pane title and the agent kind
are not collected.

Where the conversation comes from is `[context] source`. `transcript` reads the
session transcript and nothing else: when none is found, the take is biased on
file names alone. `pane` reads the pane's screen through `herdr pane read` and
never looks for a transcript. `auto`, the default, reads the transcript and
falls back to the pane's screen when it finds none.

The visible screen of a pane running a full-screen agent is deliberately **not**
the main source: it is mostly frame. The conversation transcript carries the
content instead.

### Rewrite

Fixes form only — file and directory names, flags, commands, foreign technical
terms, punctuation and capitalization — and never meaning, length or intent. Three
interchangeable engines:

- `agent` — a coding-agent command-line tool found in `PATH`, run in one-shot
  mode. Which agent, which model and which flags are configuration.
- `http` — an OpenAI-compatible endpoint, cloud or local server.
- `command` — an arbitrary external program.

A short phrase containing neither foreign terms nor names from the context skips
this stage, so trivial dictation is not delayed by a round trip.

If no engine is available, the transcript is inserted unchanged and the user is
told once, not on every take.

### Delivery

The text is **inserted** into the pane's input box and not submitted, which allows
several takes to be stacked and edited before sending. Submitting is opt-in.

## 5. Push-to-talk

### Context

Key release is not available. herdr's keybindings fire on press only, and the
terminal keyboard protocol that reports releases depends on the outer terminal
supporting it, which Terminal.app and others do not.

### Problem

Hold-to-talk needs to know when the key was released.

### Decision

Release is inferred from the absence of auto-repeat. Each repeat stamps the
current time; the daemon records while stamps keep arriving and stops when none
has arrived for `release_ms` (default 250 milliseconds, three times the measured
85 millisecond repeat interval). A hold shorter than `min_hold_ms` (default 300
milliseconds) is a stray tap: its recording is discarded without transcription.

### Why

Auto-repeat is produced by the operating system on all three platforms and reaches
the plugin through herdr's ordinary keybindings. It needs no accessibility
permission, no additional program and no terminal-specific protocol.

A toggle action remains for long dictation, where holding a key for a minute is
worse than pressing it twice.

## 6. Indicators

While recording, an indicator blinks in two places at once: the custom token on
the agent's row in the sidebar, and the tab label. Two places because the sidebar
can be collapsed, and then only the tab bar remains visible. The indicator carries
elapsed time, so a stuck recording is distinguishable from a working one.

After release the blinking stops and the same two places show the current stage.
When the run ends — successfully, with an error, or by cancellation — the tab
label is restored and the token is cleared. Restoring on every exit path is a
requirement, not a nicety: a renamed tab otherwise keeps a stale recording prefix
forever.

Toasts announce completion and errors. A run journal and a preview step before
insertion are available and off by default.

`doctor` reports what is missing: microphone permission, model, rewrite engine,
herdr version.

## 7. Configuration

A TOML file in the plugin's configuration directory. Every key has a default, so
an absent file means the defaults.

```toml
[audio]
input = ""                # device name; empty means the system default
silence_db = -60

[stt]
engine = "candle"         # candle | http | command
model  = "large-v3-turbo"
language = "auto"

[rewrite]
engine = "agent"          # agent | http | command | off
agent  = "auto"
model  = "sonnet"
args   = []
skip_if_plain = true
prompt_file = ""

[context]
source = "auto"            # auto | transcript | pane
conversation_turns = 6
file_names = 40
prompt_chars = 600

[ptt]
release_ms = 250
min_hold_ms = 300

[ui]
blink_ms = 600
sidebar_token = true
tab_indicator = true
toasts = true
preview = false
journal = false

[delivery]
submit = false
```

Keybindings are **not** part of this file. A herdr plugin manifest cannot declare
keys, so they live in the user's herdr configuration; the `setup` action prints
the exact snippet and offers to append it.

## 8. Distribution

Installed with `herdr plugin install aliaksandr-haurylau-godel/herdr-voice`. A
tagged release publishes archives for macOS on arm64 and x86_64, Linux on x86_64
and arm64, and Windows on x86_64; the manifest's `build` entries fetch the archive
that matches the platform.

The repository is public, and the author works on client projects. A leak gate
runs in two places on the same rule set: a pre-commit hook and a CI job. It blocks
ordinary secrets plus employer, client and internal-system identifiers, and
absolute home paths that expose an account name.

## 9. Open questions

1. Whether `herdr plugin install` can fetch from a repository that is not public.
   The herdr binary contains no GitHub token handling and does contain `rev-parse`,
   which suggests a git clone; this only matters if the repository is ever closed.
2. How the built-in `candle` engine compares with whisper.cpp in speed and
   accuracy on the same recordings. One number exists now and it is not
   flattering: on a 66-second take with `large-v3-turbo` on Metal, the built-in
   engine took 12 to 14 seconds against `whisper-cli`'s 1.65 seconds for 70
   seconds of speech — roughly eight times slower with the same model. Two
   reasons are known and both are upstream: the weights cannot be loaded as F16,
   because `candle-transformers` 0.11 mixes F32 constants into the Whisper graph,
   and its decoder derives positional embeddings from the whole prefix on every
   step, so it must be re-fed each time and decoding is quadratic in the tokens
   generated. The model chosen matters more than either: `tiny` transcribes the
   same take in about a second. A proper comparison, on the same recordings and
   for accuracy as well as speed, is still open.
3. Windows as a whole: key auto-repeat through herdr, the named pipe, audio
   capture. Tracked as a separate issue for someone with a Windows machine.
4. The built-in engine's decoder has no floor on how far a window advances. A
   model that emits a timestamp one step past the window's start moves the seek
   by two frames, so a thirty-second window can take some fifteen hundred passes
   to cross. This matches the reference implementation and is not a hang, but
   with no temperature fallback there is one fewer thing stopping a degenerate
   window.
5. Recording starts with a short delay while the capture device opens, so the
   first fraction of a second of speech can be lost. A permanently open capture
   stream would remove it at the cost of holding the microphone open.
