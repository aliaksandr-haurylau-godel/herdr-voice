# Evidence

Measurements the design rests on. Every number here was produced on one machine —
Apple silicon, macOS 25.6, herdr 0.8.2 — with the shell prototype kept in `spike/`.
Nothing here has been reproduced on Linux or Windows.

## Key auto-repeat through herdr

A direct chord (no prefix) was bound to a command that appended a high-resolution
timestamp, then held down. Three holds were recorded in one log.

| hold | events | duration | gap before the 2nd event | median gap after |
|---|---|---|---|---|
| 1 | 62 | 5.05 s | 44 ms | 84 ms |
| 2 | 1 | single tap | — | — |
| 3 | 50 | 4.10 s | 99 ms | 85 ms |

Conclusions: herdr invokes a bound command on every auto-repeat, at roughly twelve
per second; there is no half-second pause before repeats begin; and a single tap
is distinguishable from a hold by producing exactly one event.

## Context and its effect on the transcript

Target pane: an agent working in a notes repository. Spoken phrase, in Russian,
containing one English term: "what is this worklog about, tell me more".

| stage | time | result |
|---|---|---|
| whisper, no context | 2 s | the English term came out as an unrelated Russian word, no punctuation |
| whisper, context in `--prompt` | 1 s | punctuation and capitalization appeared, the term stayed wrong |
| rewrite with context | 7 s | term restored, punctuation and word forms corrected |

Conclusion: biasing recognition improves form but does not restore terms; the
repair happens in the rewrite stage. The term was missing from the bias string
because only file basenames were collected and the term was a directory name —
fixed by including path components.

## Rewrite engines

Same phrase and context for every row.

| engine | time | outcome |
|---|---|---|
| agent CLI, mid-tier model, MCP servers disabled | 4.6 s | correct |
| agent CLI, top-tier model, MCP servers disabled | 5.4 s | correct |
| agent CLI, mid-tier model, MCP servers loaded | 7.3 s | wrong term |
| agent CLI, small model, MCP servers disabled | 11.2 s | not corrected |
| local model, 9.6 GB | 16 s | not corrected |
| local model, 18 GB | 49 s | wrong term, word forms corrected |

Conclusions: on this machine local models are both slower and worse than an agent
command-line tool; and disabling the agent's tool servers saves almost three
seconds, which is more than the difference between two model tiers.

## Prompt size limit

whisper.cpp accepts an initial prompt of about `n_text_ctx / 2` tokens, on the
order of 224. A full repository file listing produced 7277 characters — beyond the
limit, and alphabetically ordered, so the surviving prefix was noise. Replaced by
recently touched names capped at 600 characters.

## Pane screen versus conversation transcript

For a pane running a full-screen agent, 80 screen lines reduced to 59 characters
after dropping lines without letters or digits: a frame and a status line. The
same agent's session transcript yielded about 2.6 KB of content over six turns.

## Silent recording

A recording made from the wrong input measured a mean volume of −91 dB over 4.37
seconds, and recognition returned a single period. A real take of comparable
length measured −46.9 dB. Hence the loudness check before transcription.

## Daemon, client and doctor

Verified by hand on macOS 25.6, Apple silicon, herdr 0.8.2, with the release build
of the plugin. Nothing here was reproduced on Linux or Windows.

| what | command | result |
|---|---|---|
| `doctor` on a machine with no configuration file | `herdr-voice doctor` | five lines; `config default`, `model missing`, `rewrite ok found "claude" in PATH`; exit 1 |
| a client with no daemon | `herdr-voice cancel` | one line naming the socket and how to start the daemon; exit 1, no hang |
| the daemon stays alive | `herdr-voice daemon &` | the process is still there a second later; `doctor` reports `daemon ok` and the path |
| a second start | `herdr-voice daemon` | "a daemon is already listening at …", exit 0, still one process |
| a request carrying a context | `HERDR_PLUGIN_CONTEXT_JSON=… herdr-voice cancel` | exit 0; the daemon recorded `request command=cancel entrypoint=cancel context=41 bytes` |
| a request with no context | `env -u HERDR_PLUGIN_CONTEXT_JSON herdr-voice cancel` | exit 0, and the daemon recorded the context as unreadable on its own line |
| a leftover socket after `kill -9` | `kill -9`, then `herdr-voice daemon` | the socket file survived the kill; the next daemon reclaimed it and `doctor` reported `daemon ok` |
| an action through herdr | `herdr plugin link .`, `herdr plugin action invoke cancel` | exit 0; herdr logged `status: succeeded, exit_code: 0`; the daemon recorded a request of 375 context bytes |

Three things this exercise established that the design had assumed:

**A liveness probe is not a malformed request.** `doctor` and a second `daemon`
start both connect and close without sending a frame, and the first version
reported each one as `connection failed: malformed header: ""`. Three of those
lines appeared in a log that had answered two real requests. The frame reader now
distinguishes a peer that sent nothing from one that sent something wrong, and the
daemon stays quiet about the former. A log that cries failure over the normal case
is the same defect as a log that says nothing about a real one.

**herdr does not set `HERDR_PLUGIN_ENTRYPOINT_ID` for an action invoked from the
command line.** The recorded line read `entrypoint=-`. The variable is documented
and the frame carries it, but on this path it does not arrive, so the `-` that
stands for an absent token earned its place on the first real invocation rather
than in a test.

**The configuration directory the plugin derives is the one herdr computes.**
`herdr plugin list` printed
`config: ~/.config/herdr/plugins/config/haurylau.voice` for the linked plugin, which
is what `config::directory` produces from `HERDR_PLUGIN_CONFIG_DIR` or, without it,
from `$HOME`.

Not verified, and why: the daemon's own log line reaching
`herdr plugin log list` requires herdr to have started the daemon through the
manifest's `startup` entry, which means restarting herdr. The daemon was started by
hand instead, so its line was read from its own standard error. The plugin was
linked for the action check and unlinked afterwards; `herdr plugin list` was
identical before and after.

### What the Windows job established

The `windows-latest` job in CI is the only thing that compiles or runs the named
pipe: the `x86_64-pc-windows-msvc` target is not installed on the development
machine and `rustup` is absent, so nothing under `cfg(windows)` is built there.
Two platform differences came out of it, both invisible on macOS and Linux.

**A named pipe does not hold a connection whose client has already left.** A test
connected, dropped the connection at once, and expected `accept` to hand that
connection over anyway. A Unix socket queues it, so the test passed on two
platforms; on Windows `accept` went back to waiting for a client that never came,
and the job sat for ten minutes until the cap killed it. The single-threaded run
named the culprit: the output stopped at the probe test. The test now holds the
connection open until the daemon has accepted it and only then goes away, which is
what both platforms agree on.

**A closed peer is an error on Windows and zero bytes on Unix.** Nothing has been
read when the frame's first line is attempted, so a peer that goes away there sent
nothing at all — a liveness probe. The reader now treats `BrokenPipe`,
`ConnectionReset`, `ConnectionAborted` and `UnexpectedEof` at that point the same
as end of input. A disconnect further in, inside the body, stays a short body.

Neither of these was found by reading the code, and neither could have been: the
platform that shows them is the one this machine cannot build.

The crate's own source settles why, and settles two related worries as well
(`interprocess` 2.4.3, as vendored in the local registry):

- `src/os/windows/named_pipe/listener.rs:154` — `accept` loops on `ERROR_NO_DATA`,
  disconnects the instance and waits again. A connection whose client left without
  writing is what that error is, and the crate calls it an empty connection and
  discards it. That is the hang, exactly.
- `src/os/windows/named_pipe/listener.rs:72` — `accept` creates the next instance
  before it hands out the accepted one, so a request being served does not make the
  next client queue. No work of ours is needed for that.
- `src/os/windows/named_pipe/c_wrappers.rs:171` and
  `src/os/windows/named_pipe/wait_timeout.rs:13` — a client meeting a busy pipe
  waits rather than failing, but the wait is bounded: with both sides on the default
  it is 50 milliseconds. So a liveness probe on Windows cannot block indefinitely.

## What the capture library offers

Measured on macOS 25.6, Apple silicon, with `cpal` 0.18.2 in a throwaway crate
outside this repository, before capture was designed. Two facts, both of which
change what the capture stage has to do.

**No input device on this machine offers 16 kHz.** Enumerating the CoreAudio host
gave three input devices: a USB device, the built-in microphone, and a virtual
device installed by a conferencing application. Each reports a single supported
input configuration of 48 kHz, one channel, 32-bit float; the built-in microphone
additionally offers 44.1, 88.2 and 96 kHz. Not one offers 16 kHz.

The prototype never met this. It recorded through `ffmpeg` with `-ar 16000 -ac 1`
(`spike/spike.sh:254`), so the resampling happened inside a program the plugin does
not have. Producing 16 kHz mono is therefore work this plugin must do itself, and
opening a device at 16 kHz is not an option to fall back on.

**A device's name comes from `Display`.** In 0.18 `DeviceTrait` requires `Display`
and offers `description()` and an `id()` documented as stable across runs,
disconnections and reboots. The `name() -> Result<String>` of earlier versions is
gone, so selection by name reads the `Display` form.

The existence of a stable identifier is worth recording next to this, because it
addresses exactly the failure that "select by name, never by index" was written
against. Moving to it would change a rule in `CLAUDE.md` and was left alone.
