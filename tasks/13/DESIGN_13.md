# DESIGN_13 — recognition

Covers `AC_13.md` in full. Decides four things: the interface a transcript comes
through, how an external program is invoked, what makes a model usable, and what
the daemon answers when the engine cannot run.

## 1 The interface

**Context.** Three engines are named by `docs/design.md` section 4; one is built
here and two arrive later (#15, #16). Adding them must not change what the daemon
calls.

**Problem.** An interface shaped around the one engine that exists would have to be
reshaped twice.

**Decision.** One trait, one method: a take's path in, a transcript out, an error
that already knows how to describe itself.

```
trait Engine {
    fn transcribe(&self, audio: &Path) -> Result<String, EngineError>;
}
```

Everything an engine needs beyond the path — the model file, the language, the
argument list — is given to it when it is built, not per call. The daemon resolves
an engine once, from configuration, and holds it.

**Why.** The path is the only thing that changes between takes. A model that is
resolved once is also a model that is checked once, which is where the integrity
check belongs: at the point somebody can still be told what to fix, rather than in
the middle of a recording.

## 2 Running a program

**Context.** `whisper-cli` takes the audio behind `-f`, the model behind `-m` and
the language behind `-l` (`spike/spike.sh:288`).

**Problem.** Appending the path to a command line covers a program nobody uses.

**Decision.** `[stt] command` is an argument list. Each element has `{audio}`,
`{model}` and `{language}` replaced; a list with no `{audio}` gets the path
appended. `auto` is substituted like any other language, because it is what these
programs already take to mean "detect it". Standard output is the transcript,
trimmed. A non-zero exit is an error carrying what the program wrote to standard
error, truncated to something a message can hold.

**Why.** An argument list is the only shape that fits programs whose flags nobody
here chose. It also puts the omission case where it belongs: somebody whose program
has no language flag leaves it out of their own list, and no rule about dropping
neighbouring arguments has to exist.

**A detail that matters.** The program inherits this process's environment, and
herdr starts plugin commands with a minimal `PATH`. The daemon is started by herdr,
so a program found in an interactive shell may be absent from the daemon's. The
engine therefore reports "not found on PATH" with the `PATH` it actually searched.

## 3 What makes a model usable

**Context.** `[stt] model` is an identifier — `large-v3-turbo` — and the file is
`ggml-large-v3-turbo.bin`. Issue 3 accepted a substring test and recorded that this
issue must replace it.

**Problem.** A substring matches an unrelated file. A truncated download matches
its own name perfectly and fails later, somewhere less helpful.

**Decision.** A model is usable when all of this holds, checked in this order:

1. `<state>/models/ggml-<model>.bin` exists.
2. It is at least a few megabytes — smaller than any real model, larger than an
   error page saved by mistake.
3. Its first four bytes are `6C 6D 67 67`, the `ggml` magic, read off a model on
   disk rather than recalled.
4. If `<file>.sha256` exists, its digest matches. Its absence is not a failure;
   issue #15, which downloads, writes it.

`doctor` and the engine call the same function, so they cannot disagree.

**Why.** Each step catches something the next cannot: the wrong name, a truncated
or empty file, a file that is not a model at all, and a file that is the wrong model
under the right name. Only the last needs a digest, which is why its absence is
tolerated rather than fatal.

**Where the check does not apply.** A command whose argument list has no `{model}`
placeholder brings its own model, and demanding one this plugin manages would refuse
a working setup. The model is resolved and checked only when the list asks for it.

**And what `doctor` says then.** The same thing, which is the point of one function
— though this state is decided *before* the shared check runs rather than by it, and
`doctor` gains a state for it beside `ok`, `default` and `missing`:
when the configuration cannot ask for a model — the engine is not built, or the
argument list has no `{model}` — the model line says the model is not used by this
configuration, and names `[stt] model` and the directory it would be looked for in.
That is the state a fresh install is actually in, and it is a different sentence
from "missing", which would send somebody to download a file nothing would read.

## 4 When the engine cannot run

**Context.** The default engine is `candle` and it is not built. `[stt] command`
defaults to empty.

**Problem.** A fresh install transcribes nothing, and the person has to be told
which of several reasons applies.

**Decision.** Four distinct answers, each naming what to set:

- `candle` or `http`: not built in this version, and `command` is what works.
- an unknown engine name: refused, listing the three.
- `command` with an empty list: the key to set, with an example that is the
  invocation the prototype used.
- a model that fails its check: which step failed and what to do.

The daemon answers with these; `doctor` prints the same for the engine and the
model, from the same functions.

**Why.** These are four different things to fix, and a single "recognition is not
available" would make somebody guess which. Silence about the difference is what the
project treats as a defect of the same weight as a wrong transcript.

**No silent substitution.** An engine that is not built does not fall back to one
that is. Somebody who believes the built-in engine is running while an external one
is finds out at the worst possible moment.

## 4a How long the client waits

**Context.** The client gives the daemon two seconds to answer
(`src/client.rs`, `REPLY_TIMEOUT`), and a test asserts that bound stays short. That
was written when every answer was a few bytes. `docs/evidence.md` measures two
seconds of transcription for a two-second take.

**Problem.** Transcribing before replying turns a take that worked into "the
daemon did not answer within 2 seconds". The failure would land on the successful
case, which is the worst place to put one.

**Decision.** The bound belongs to the command name, not to the client and not to
which half of a toggle a press turns out to be. `cancel` — and `ptt` when it
arrives — keep two seconds. **`dictate` gets two minutes, both halves**, because
the client chooses its bound before it sends and cannot know which half it is: the
answer to that lives in the daemon (`Started::Began` against `AlreadyRunning`), and
asking would mean a second round trip, a second command name, or a protocol that
answers twice. The existing test changes from "the bound is short" to "the short
bound stays short, the long one is still bounded, and `dictate` gets the long one".

**What that costs, stated rather than hidden.** The press that *starts* a take is
answered immediately, so the long bound never elapses in normal use — but if the
daemon is wedged, that press now takes two minutes to say so instead of two
seconds. A late message about a stuck daemon is worse than a prompt one, and it is
better than a prompt lie about a take that was working; the take is the case that
happens.

**Why.** A bound exists to turn a hang into a message, not to cap work somebody
asked for. Two seconds is right for a command that only writes a few bytes and
wrong for one that runs a speech model; a single number cannot be both. Two minutes
is far past any transcription measured here and still far short of forever.

**What this does not do.** It does not make the person wait less. Delivery — the
stage that puts text into a pane — is where a take stops blocking whoever started
it, and that is another issue. Until then the wait is real and the bound only
decides whether it ends in a transcript or in a lie.

## 5 What the daemon answers

A finished take is transcribed before `dictate` replies. The reply carries the
transcript, and keeps the measured level beside it as issue 8 decided: a level next
to an accepted take is what makes a marginal input visible before it becomes a bad
transcript. A take that transcribes to nothing is reported as such, not as an empty
success — a blank reply is indistinguishable from a working one that had nothing to
say.

The take's file is left on disk. Nothing deletes takes yet, and the stage that
consumes them will decide when they stop being useful.

## 6 Modules and tests

| module | owns | tested by |
|---|---|---|
| `stt` | the trait, resolving an engine from configuration | every engine name, an unknown name, an empty command list |
| `stt::model` | the file name rule and the four checks | exact name, a name that merely contains it, an empty file, a small file, wrong magic, a matching and a mismatching sidecar |
| `stt::command` | placeholder substitution, running the program, reading it | substitution of each placeholder, an appended path, `auto`, a program that is absent, one that fails, one that prints nothing |
| `config` | `[stt] engine`, `language`, `command` | defaults, partial file, unknown key |
| `daemon` | transcribing a finished take | a fake engine returning text, and one returning an error |
| `client` | the reply bound, chosen by command name | the short bound stays short, the long one is bounded, `dictate` gets the long one and `cancel` the short one |
| `README.md` | the line that says recognition needs an external engine | read it: the section about installing names `[stt] engine` and `[stt] command` |
| `doctor` | the engine line and the model line | the same states the engine reports |

The program tests use a shell command rather than a real transcriber: `echo` for a
transcript, `false` for a failure, a name that does not exist for absence. No model,
no microphone, no network — like every stage before this one.

## 7 What this design does not decide

- The built-in engine (#15) and the endpoint engine (#16).
- Choosing a model from a list, and downloading one. Both belong with #15.
- The bias prompt the prototype passed. That is context, and context is its own
  task.
- Deleting takes.

## 8 Where each criterion is decided

| AC | decided in |
|---|---|
| AC-1 configuration keys and defaults | 6 |
| AC-2 one interface, one implementation, a fake | 1 |
| AC-3 placeholders and the appended path | 2 |
| AC-4 a program that is absent | 2 |
| AC-5 a program that fails | 2 |
| AC-6 the endpoint engine reports it is not built | 4 |
| AC-7 the built-in engine reports it is not built | 4 |
| AC-8, AC-8a an unknown name, an empty list | 4 |
| AC-9 the exact file name | 3 |
| AC-10 the damaged model | 3 |
| AC-11 the transcript in the reply | 4a, 5 |
| AC-12 the language, `auto` included | 2 |
| AC-13 `doctor` agrees with the engine | 3, 4 |
| AC-14 the README line | 6 |
| AC-15 the tests and the checks | 6 |
