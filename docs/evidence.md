# Evidence

Measurements the design rests on. Every number here was produced on one machine —
Apple silicon, macOS 25.6, herdr 0.8.2 — with the shell prototype kept in `spike/`,
except the last section, which was produced on Linux in a container and says so.

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
identical before and after. That gap is closed on Linux rather than on macOS —
see "Linux, in a container" at the end of this file.

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

## Linux, in a container

Run twice on 2026-08-25 by `scripts/linux-check.sh`, which performs the whole
check in one non-interactive pass and prints the table below itself. The two runs
agreed step for step; the second exists to show the script is re-runnable on a
machine it has already touched. The plugin was built from revision `68fbe89`, the
tip of `main`, which is the first revision that records anything — capture landed
there. The script itself carries the changes described below, which are not in
`68fbe89`.

This is the second run of this check. The first, on revision `e32a9b7`, could not
touch capture at all, because capture did not exist: its `no-device` step was
recorded as pending. Everything that run established about installation, the
socket path and the `[[startup]]` entry was re-observed here and is stated below
as one result.

The machine: a throwaway Debian GNU/Linux 12 (bookworm) container, `aarch64`,
kernel 6.18.15, no sound hardware of any kind — no `/dev/snd`, no PipeWire, no
`arecord`. The host ran Apple's `container` 1.1.0 and nothing was installed on it.
Versions the script installed and printed: `rustc 1.98.0 (88d9e12ae 2026-08-18)`,
`cargo 1.98.0 (797e8a9bc 2026-08-05)`, `herdr 0.8.2`, `cc (Debian 12.2.0-14+deb12u1)
12.2.0`, ALSA development headers `alsa 1.2.8` from `libasound2-dev`.

| step | exit | outcome |
|---|---|---|
| packages | 0 | `apt-get`; ALSA headers `alsa 1.2.8`, which `cpal` needs to build at all |
| rust | 0 | stable toolchain installed by `rustup` |
| herdr | 0 | installed by `curl -fsSL https://herdr.dev/install.sh \| sh` |
| build | 0 | `cargo build --release` produced `target/release/herdr-voice`, `cpal` and `alsa-sys` included |
| link | 0 | `herdr plugin link .`; herdr computes the config directory `<config>/herdr/plugins/config/haurylau.voice` |
| server | 0 | `herdr server`, the headless server, runs without a terminal |
| pane | 0 | `herdr workspace create --focus` gives herdr a focused pane, `w1:p1` |
| daemon | 0 | herdr started the daemon itself, through the manifest's `[[startup]]` entry |
| doctor | 1 | five lines in the fixed order; `herdr ok`, `daemon ok`, `config default`, `model missing`, `rewrite missing` |
| action | 0 | `herdr plugin action invoke cancel` returned 0 |
| herdr-log | 0 | herdr recorded that invocation as `succeeded` with `exit_code 0`, and the `[[startup]]` daemon has a record too |
| no-device | 1 | a take through the `dictate` action named the input it could not open and exited |
| named-device | 1 | `[audio] input` set to a name no device has was refused, with the names that do exist |
| no-daemon | 1 | with nothing listening, the client named the socket and exited without hanging |

**A take with no capture device names the input and exits.** This is what the
issue was left open for. A `dictate` action invoked through herdr, against the
focused pane `w1:p1`, on a machine with no `/dev/snd`, ended with exit code 1 and
this on standard error:

```
cannot read what "Default Audio Device" supports: The requested audio device is
not available. It may have been disconnected.
```

No hang, no signal, no panic, no silence and no zero exit — the five outcomes the
step now fails on. The daemon stayed up and answered the invocations that came
after.

**With no sound hardware, ALSA still offers a device.** The container has no card,
and `cpal` nevertheless enumerated one input, called `Default Audio Device`, whose
supported configurations could not be read. So the branch that reports "no input
devices at all" is not the one a headless Linux machine reaches: the refusal comes
one step later, at the point the device is interrogated. The daemon's standard
error carries the ALSA library's own complaint next to it — `cannot find card '0'`,
`Unknown PCM default` — which is noise from the library rather than from the
plugin, and it does not reach the person: what herdr shows is the single line
above.

**A configured input that no device answers to is refused with the list.** With
`[audio] input` set to a name nothing has, the take exited 1 and said:

```
no input device named "No Such Microphone 6133"; the ones that exist are
"Discard all samples (playback) or generate zero samples (capture)". Set [audio]
input to one of them, or leave it empty for the default
```

The one name in that list is ALSA's null device, which is all this container
offers. The refusal repeats the configured name and does not fall back to the
default, which is the behaviour "by name, never by index" exists to produce.

**A plugin's exit code is not what `herdr plugin action invoke` returns.** That
command returns 0 for "the action was started", and the plugin's own exit code,
standard output and standard error appear only in the record herdr keeps —
`herdr plugin log list --plugin haurylau.voice`. The record is written when the
command ends, not when it starts: read the instant `invoke` returns, it still says
`"status":"running"`. Every assertion about what a take did therefore reads that
record and waits for it to leave `running`, and a record that never leaves it is
how a hang is detected. The first version of the check grepped the log
immediately and failed on a `cancel` that had in fact succeeded.

**herdr starts the daemon from the manifest, and the daemon's own lines reach the
plugin log.** This is what the macOS exercise could not establish, because it
would have meant restarting herdr there. With the plugin linked before the server
came up, `herdr plugin log list --plugin haurylau.voice` showed the `[[startup]]`
record next to the action's. The daemon's standard error appears in that record
only after the process ends; while it runs the record carries none. When the
daemon was killed with a signal, the record read:

```
"event":"startup","status":"failed",
"stderr":"listening at <state>/voice.sock\nrequest command=cancel entrypoint=- context=57 bytes\n"
```

So a daemon stopped by a signal is `failed` in herdr's eyes, `entrypoint` is `-`
for a command-line invocation on Linux exactly as on macOS, and the context herdr
sends from the command line was 57 bytes there against 375 on macOS. With a
workspace open it is larger: the `dictate` invocations in this run carried a
context naming `focused_pane_id`, `focused_pane_cwd`, `tab_id`, `tab_label` and
the workspace, which is what makes a take possible at all — `dictate` refuses to
start without a pane to deliver into.

**The socket path the plugin derives is the one herdr computes.** herdr ran the
daemon with `HERDR_PLUGIN_STATE_DIR` pointing at
`<state>/herdr/plugins/haurylau.voice`, and the daemon listened at `voice.sock`
inside it. A client started from a plain shell, with no `HERDR_` variable set at
all, derived the same path from `$HOME` and reached that daemon: `doctor` reported
`daemon ok` with the same name. The two derivations agree on Linux.

**A daemon left over from an earlier run answers `doctor` and then goes away
mid-request.** Between the first and second attempt at this run, a daemon that
herdr had started from `[[startup]]` outlived the server that spawned it. The next
run's `doctor` reported `daemon ok` — the socket was there and the connection was
accepted — and the `dictate` that followed got no reply at all:
`the daemon spoke something unexpected: malformed header: ""`. That message is
what a client says when the daemon closes a connection without answering, and it
is the right shape of failure: exit 1, a named cause, no hang. The cause was the
check's own cleanup, which killed the server and the daemon it had started by
hand but not the one herdr started; it now kills every `herdr-voice daemon` it can
find, and both re-runs after that change were clean.

Two smaller things, both about herdr rather than about the plugin. `herdr status
server` exits 0 whether or not a server is running and says which in its output,
so the state has to be read from `--json`; the first version of the check trusted
the exit code and reported a server that did not exist. And `herdr plugin link`
works with no server running, which is what lets the plugin be linked first so
that the server can run its `[[startup]]` entry.

**The step was checked against a false pass.** A copy of the script judging an
action that exits 69, "not implemented yet", was run through the same assertions,
and the step failed: `the take reported 'not implemented yet'`. So the `no-device`
step is not green because nothing tests it.

Not established here, and each of these is a real gap rather than a formality:

- **Recording from a real microphone on Linux.** The container has no sound
  hardware, so every take in this run is a refusal. Nothing here shows that a
  Linux machine with a working input produces 16 kHz mono, or what its levels
  look like; that is still known only from macOS, in "Capture, by hand on macOS".
- **A machine with no input devices at all.** ALSA offers its null device even
  with no card, so the "no input devices at all" refusal was never reached.
- **`x86_64`.** Both runs were `aarch64`. The release workflow builds for both,
  and only one of them has now met a live herdr.
- **The silence floor on Linux.** `[audio] silence_db` was left at its default and
  no take ever produced samples to measure, so the floor was not exercised here.

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

## Capture, by hand on macOS

macOS 25.6, Apple silicon, herdr 0.8.2, release build. Nobody spoke: everything
below is either a refusal or a measurement of a quiet room, and a take containing
speech is still unverified.

| what | result |
|---|---|
| a take on the default input, twice through `dictate` | started and finished; refused at −100.0 dB |
| the same, with the floor lowered to −200 so the file survives | 32 426 samples, 2.03 s, and `afinfo` reports `1 ch, 16000 Hz, Int16` — but every sample is zero |
| a take on the built-in microphone | refused at −62.9 dB |
| `[audio] input` set to a name no device has | refused, listing the three names that exist, exit 1 |
| `[audio] input` set to a name that exists | opened that device and recorded from it |

**The default input is not the microphone.** The first two rows look like a broken
capture and are not: the machine's default input is a USB interface that delivers
digital silence, and the same code on the built-in microphone returns −62.9 dB of
ordinary room tone. The conversion, the container and the length were all correct
throughout — 2.03 seconds of 16 kHz mono, sixteen bits — so what the silence check
caught was exactly what it exists to catch, a take from an input nobody speaks
into.

An earlier reading of this attributed the zeros to a denied microphone permission,
which the built-in microphone then disproved. The message a refused take prints
does name permission among the things to check, because on macOS a denied
permission also delivers silence rather than an error, and the two are
indistinguishable from inside the process.

**A quiet room measures −62.9 dB against a −60 floor.** The threshold sits just
above room tone on this machine, which is what it is for — but it means the margin
between "nobody spoke" and "somebody spoke quietly" is not large, and the first
person to be refused mid-sentence will be the one to say so.

**Found by running it, not by reading it:** the configured device name never
reached the device. The name was a per-call argument and the daemon passed nothing,
so `[audio] input` was ignored and every take came from the default — in silence,
which is the precise failure that selecting by name exists to prevent. The
recorder now takes the name from the configuration it was given, and a test asserts
the name reaches the source; that test fails against the old behaviour.

**Also found by running it:** a successful take printed nothing at all. The daemon
answered with the path and the level, and the client discarded everything it was
told on success, so a take that worked was indistinguishable from a take that did
nothing. Results now go to standard output and failures to standard error.

## Recognition, by hand on macOS

macOS 26.6.2 build 25G83, Apple silicon, herdr 0.8.2, release build. The
transcriber is `whisper-cli` from Homebrew against `ggml` 0.20.2 on Metal, with
`ggml-large-v3-turbo.bin` in the models directory. `[stt] engine = "command"`,
`language = "ru"`, and the argument list the code suggests as an example.

**A spoken sentence went through the whole chain for the first time.** A 70-second
take of Russian speech containing English technical terms, measured at −50.9 dB,
came back as:

> я бы смержал этот пули квест без ревью в остальном все ок можем двигаться
> дальше это была тестовая запись я думаю а

The chain works: the microphone, the conversion, the file, the transcriber and the
reply all did their part. What it produced is not usable, and the reason is the one
thing left out.

| the same take, transcribed three ways | time | "pull request" came back as |
|---|---|---|
| `-l ru`, no bias prompt | 1.65 s | "пули квест" |
| `-l ru`, `--prompt` naming the terms | 1.53 s | **"pull request"**, and "review" in Latin too |
| `-l auto`, no bias prompt | 1.81 s | "пули квест" |

**The terms need the bias prompt, and nothing else supplies them.** Automatic
language detection changes nothing; the prompt changes the terms and brings back
commas as well. This narrows what the section above concluded from the prototype —
that biasing improves form but does not restore terms. It does restore them, when
the term is in the string; there it was absent because only file basenames were
collected. So the chain cannot produce a usable transcript for this kind of speech
until context is collected and passed, and recognition without it renders every
English term as the Russian word it sounds like.

**Recognition is not the slow stage.** 1.5 to 1.8 seconds for 70 seconds of speech,
on Metal. The seven seconds the earlier measurement attributes to the rewrite stage
belong to the agent it calls, not to the model that transcribes.

**A minute of room tone produced confident text out of nothing.** Room tone
measured −54.4 dB — above the −60 floor, so the take was accepted — and the
transcriber returned "Продолжение следует..." four times over. The silence floor
answers "is anything arriving", not "did anybody speak", and a hallucinated
sentence delivered into an agent's prompt is worse than an empty one. Nothing is
changed here yet; the number is written down because it decides where a floor
should sit.

| refusal paths, each with `doctor` beside it | what it did |
|---|---|
| `[stt] command` empty | `doctor`: `engine missing` with an example list, `model unused`, exit 1; the take refused with the same sentence |
| `engine = "candle"` | `doctor`: `engine missing`, naming issue #15 and what to set instead, `model unused`, exit 1; the take refused likewise |
| the working configuration | `doctor`: `engine ok "command" is ready`, `model ok`, exit 0 |

**Found by running it: a reply with a newline in it is truncated silently.** A
transcriber printing two lines delivered only the first, with exit 0, and the level
and the target pane vanished with the remainder — they are formatted after the
text. The same cuts the guidance off refusals: the message promises an example and
the client prints everything up to "for example:". The reply protocol is one line
by construction and the messages grew multi-line. Recorded as issue #19.

**Found by running it: `cancel` stops nothing.** It answers `nothing to cancel`
with exit 0 while a take is recording, and the next `dictate` stops that same take
and transcribes it. There is currently no way to abandon a take without delivering
it. Recorded as issue #18.

**Found by running it: a deep state directory stops the daemon from starting.** The
socket path is bound by `sun_path`, 104 bytes on macOS, and the daemon refused with
`local socket name length exceeds capacity of sun_path of sockaddr_un`. The message
names the cause and the path, so nothing is silent about it, but the default state
directory is already long and a longer account name would reach the limit on a real
installation. This run used a short state directory for that reason.
