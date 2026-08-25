# DESIGN_8 — capture

Covers `AC_8.md` in full. Decides four things: how the input device is chosen, how
a take is owned while it runs, how 48 kHz becomes 16 kHz, and what happens to a
take that captured nothing.

## 1 The device

**Context.** `[audio] input` names an input device, and an empty name means the
system default. `cpal` 0.18 exposes a device's name through `Display`; the
`name()` of earlier versions is gone (`docs/evidence.md`, "What the capture library
offers").

**Problem.** The prototype's rule — by name, never by index — has to survive
contact with a list that can hold two devices of the same name, and with a
configured name that matches nothing.

**Decision.** One function turns a name and a device list into a choice, and it is
pure: it takes names, not devices, so it is tested without a microphone. An empty
name yields the host default. A name that matches exactly one device yields that
device. A name that matches nothing is an error carrying the list of names that do
exist. A name that matches more than one yields the first, and the daemon records
that it was ambiguous.

**Why.** The failure that motivated the rule is silent: the recording goes
somewhere else and nothing says so. Every branch here either produces the device
the person named or says out loud why it could not. Falling back to the default on
a name that matches nothing would reintroduce exactly the silence the rule exists
to prevent.

## 2 Who owns a take

**Context.** A take spans two `dictate` invocations, and each invocation is a
separate client process reaching the daemon over a separate connection, handled on
its own thread. A `cpal` stream must stay alive between them.

**Problem.** The stream cannot live in a connection handler, which ends in
milliseconds. Whether a stream can be moved between threads at all is not
something the crate states plainly for every platform.

**Decision.** A recorder owns one thread for the whole life of the daemon. The
daemon sends it `Start` and `Stop` over a channel and receives the outcome back
over a reply channel. The stream is built, held and dropped entirely on that
thread, and nothing else ever touches it. The recorder holds at most one take:
`Start` while a take is running is an error, and so is `Stop` with none.

**Why.** It makes the question of whether a stream is `Send` irrelevant, which is
better than answering it per platform. It also gives the take a home that outlives
the connection that started it, and it makes "one take at a time" a property of the
structure rather than a rule someone has to remember.

## 2a A device that goes away in the middle

**Context.** A take spans two `dictate` invocations. Between them the person is
talking and nothing is watching: a headset can be unplugged, a virtual device can be
torn down by the application that owns it, and `cpal` reports that through the error
callback of a running stream.

**Problem.** Three things are undecided at once, and a task cannot be written while
they are: when the take ends, what happens to the audio recorded before the failure,
and whether the recorder's source of samples can report an error at all. The last
one decides the shape of the interface every other module is written against.

**Decision.**

- **The source carries errors.** The recorder does not read samples from a stream
  directly. It receives *events* — a block of samples, or a failure with its
  reason — and both implementations produce both: the `cpal` stream from its data
  and error callbacks, and the fake from a script that can be told to fail after a
  given number of samples. A failure is therefore testable without a microphone,
  which is what the rest of the criteria demand of everything else.
- **The take ends at the failure, not at the next `Stop`.** The recorder stops the
  stream, drops the take and remembers the reason.
- **The audio recorded before the failure is discarded and its file removed.** The
  next `dictate` answers with the remembered reason, naming the device and what
  happened, and clears it.

**Why.** Reporting at the next `dictate` is not a delay anybody chose; it is the
first moment there is somewhere to report to. The daemon has no way to reach the
person on its own until the indicator exists, and that is another issue. Keeping the
partial audio would be worse than useless: a take that lost its device halfway is
the same class of input as a take from the wrong device, and sending it on produces
a confident transcript of nothing. The alternative — ending the take at the next
`Stop` and keeping what was captured — was rejected for that reason.

## 3 From what the device gives to what recognition needs

**Context.** Recognition needs 16 kHz mono. No input device on the development
machine offers 16 kHz: all three report 48 kHz, and the built-in microphone adds
44.1, 88.2 and 96 (`docs/evidence.md`). The prototype never met this because
`ffmpeg` resampled for it.

**Problem.** Converting an arbitrary rate to 16 kHz is a rational resampler, which
is a real piece of signal processing. Dropping samples without filtering first
folds everything above 8 kHz back into the band as noise, and that noise goes
straight into recognition.

**Decision.** Three steps, each its own module and each tested on synthesised
samples:

1. **Channels.** More than one channel is averaged to one.
2. **Rate.** The device is opened at a rate that is an integer multiple of 16 000 —
   32, 48, 64 or 96 kHz, whichever it supports, preferring 48 kHz. A device that
   supports no such rate is refused with a message naming the rates it does
   support. Nothing attempts 44.1 kHz.
3. **Filter and decimate.** A windowed-sinc low-pass at 7.5 kHz is applied before
   taking every *n*-th sample, where *n* is the ratio.

The result is written as 16-bit PCM in a WAV container.

**Why.** Restricting to integer ratios turns a rational resampler into a filter and
a stride, which is a few dozen lines that can be tested against a known tone rather
than a dependency and a configuration surface. The restriction costs nothing on the
hardware that has been looked at, and where it costs something it says so instead
of quietly producing aliased audio. The low-pass is not optional: without it the
step is not a conversion, it is a corruption that recognition then has to survive.

**The limit, stated rather than hidden.** A device that offers only 44.1 kHz
cannot be recorded from. If one turns up, the answer is a rational resampler, and
that is a task of its own.

## 4 A take that captured nothing

**Context.** A recording from the wrong input measured −91 dB over 4.37 seconds and
recognition returned a single period; a real take measured −46.9 dB
(`docs/evidence.md`).

**Problem.** Silence is indistinguishable from a working system until the
transcript comes back wrong, and by then the person has already spoken.

**Decision.** After every take, its mean volume is computed as
`20 · log₁₀(rms)` over the whole take, in decibels relative to full scale, and
compared with `[audio] silence_db`, default −60. Below it the take is refused: the
audio is not handed on, its file is removed, and the refusal names the device it
came from and the level it measured. Digital silence reports −∞ as a floor value
rather than an error.

**Why.** It is the prototype's measure, and the numbers that set the threshold were
taken with it; a peak or a sliding window would refuse a different set of takes for
no reason anybody asked for. Naming the device and the level is what turns "nothing
happened" into "you are recording from the wrong input".

## 5 Where a take goes

Each take writes to `<state>/takes/<milliseconds>-<pid>-<counter>.wav`, created
when the take starts. The prototype's fixed path produced six simultaneous
recordings into one file; the daemon's single-take rule is the real guard now, but
two daemons, or a daemon and a test, still must not collide.

Nothing deletes old takes yet. That belongs with the stage that consumes them.

## 6 Modules and tests

| module | owns | tested by |
|---|---|---|
| `audio::device` | name to choice, over a list of names | empty name, exact match, no match with the list in the message, duplicate names |
| `audio::level` | mean volume in dBFS | digital silence, a quiet tone below the threshold, a normal one above |
| `audio::resample` | channel averaging, ratio selection, filter and decimate | a tone at 48 kHz decimates to 16 kHz with the expected length; a tone above 8 kHz is attenuated rather than folded; a rate that is not a multiple is refused |
| `audio::wav` | 16-bit PCM WAV bytes | a known buffer round-trips through the header fields; the rate and channel count in the header are what was asked for |
| `capture` | the recorder thread, the stream, the take's file | start, stop, start-while-running, stop-with-nothing, a source that fails mid-take — all with a fake source instead of `cpal` |
| `config` | `[audio] input` and `silence_db` | defaults, partial file, unknown key |
| `daemon` | `dictate` toggling a take | first call starts, second stops, a refused take answers with the reason |

The `capture` module takes its samples through a small interface with two
implementations: the `cpal` stream, and a fake driven by a script. Both deliver the
same two events, a block of samples and a failure, so every path above — including
the device that goes away — runs without a microphone. It is the same shape
`docs/design.md` section 4 gives the other stages.

**The configuration is read once, when the daemon starts, and handed to the
recorder thread.** Re-reading it per take would mean a person could change the
input device without restarting, which nothing asks for, and it would put file
system access on the path that runs while somebody is speaking.

**Linux needs a system package before any of this compiles.** `cpal` pulls
`alsa-sys` there, which needs the ALSA development headers present before `cargo
fmt`, `clippy` or `test` will build at all. The check workflow installs no system
packages today, so it gains a step that installs them on `ubuntu-latest` only.

## 7 What this design does not decide

- Recognition, and therefore what happens to the file after it is accepted.
- Push-to-talk timing: `ptt` keeps exiting 69.
- Microphone permission in `doctor`.
- Deleting old takes.
- Real capture on Linux and Windows. The tests run there; a microphone is not
  opened in CI.

## 8 Where each criterion is decided

| AC | decided in |
|---|---|
| AC-1 configuration keys and defaults | 6 |
| AC-2 default and named device | 1 |
| AC-3 a name that matches nothing | 1 |
| AC-4 nothing selects by index | 1 |
| AC-5 16 kHz mono 16-bit WAV | 3 |
| AC-6 conversion tested without a device | 3, 6 |
| AC-7 a unique path per take | 5 |
| AC-8 the mean volume is computed | 4 |
| AC-9 a silent take is refused, naming device and level | 4 |
| AC-10 an accepted take reports its path | 2, 5 |
| AC-11 loudness tested on synthesised samples | 4, 6 |
| AC-12 `dictate` starts and stops | 2, 6 |
| AC-13 device failures name the next action | 1, 2, 2a |
| AC-14 the four checks and the five CI checks | 6 |
