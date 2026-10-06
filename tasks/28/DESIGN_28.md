# DESIGN_28 — a bound on every outward call, and a safe cut of a program's error

Issues: #28 and #94, one pull request. Acceptance criteria: `tasks/28/AC_28.md`.
Everything below is checked against the code at `3dd45b8`.

## 1. What is built

One new module, `src/outward.rs`, holds the two things every site needs:

- `run(command, bound) -> Result<Output, RunError>` runs a program with standard
  input closed and standard output and error captured, and waits at most `bound`.
  `RunError` is `Start(io::Error)` or `TimedOut`.
- `shorten(text, limit) -> String` cuts `text` to at most `limit` bytes, at a
  character boundary, and never panics. It replaces both `stderr.truncate(...)`
  calls (#94) and is the only way text from a program is cut in `src/`.

Five call sites switch from `Command::output()` to `outward::run`:

| Site | Today | After |
|---|---|---|
| `HerdrDeliverer::run`, `src/delivery.rs:220` | `.output()` | `run` with the delivery bound |
| `pane::read`, `src/bias/pane.rs:64` | `.output()` | `run` with the pane-read bound |
| `CommandEngine::transcribe`, `src/stt/command.rs:132` | `.output()` | `run` with the transcription bound |
| `CommandEngine::rewrite`, `src/rewrite/command.rs:96` | `.output()` | `run` with the rewrite bound |
| both `stderr.truncate`, `src/stt/command.rs:152`, `src/rewrite/command.rs:116` | panics off a boundary | `shorten` |

The indicator's herdr calls (`src/indicator.rs:224`), the `git` calls in
`src/bias/files.rs` and the calls made by `doctor` and `setup` are not changed;
`AC_28.md` lists them as out of scope.

## 2. How `run` waits

1. Spawn with standard input null and standard output and error piped. On Unix the
   child is placed in a process group of its own (`CommandExt::process_group(0)`).
2. One thread per pipe reads it to the end and sends the bytes down a channel. Two
   threads, because one reader would block on a full error pipe while the other
   pipe fills, which is the deadlock `.output()` itself avoids.
3. The calling thread polls `Child::try_wait` every 10 milliseconds until the child
   has exited or the deadline has passed.
4. On the deadline: kill the child, reap it, and return `TimedOut`. On Unix the whole
   group is killed by running `kill -KILL -<group>` — the standard library has no
   group kill, and `libc` is not a dependency, which `docs/decisions.md` keeps small.
   The group kill is needed because the documented transcriber command in
   `docs/evidence.md` is `sh -c "whisper-cli ..."`: killing only `sh` would leave
   `whisper-cli` running with a take's audio open.
5. After a kill, the reader threads are given one second to finish and are then
   abandoned. A program that handed its pipe to a process outside the group would
   otherwise keep this call waiting for the very thing the bound exists to avoid.

Windows kills only the direct child, because the platform difference is in the
operating system and not in the manifest. The decision entry for the transcriber
says so.

## 3. The policy, one decision per call

Each is recorded in `docs/decisions.md` in the table's own columns: the decision in
the first, and context, problem and reason in the basis column.

**Delivery's herdr calls: bounded, 10 seconds, fixed.**
*Context:* `send-text`, `agent prompt` and `notification show` each hand a few
bytes to herdr. *Problem:* a herdr that stops answering holds the thread that
called it — a connection thread on `dictate`, the one daemon-lifetime watcher on a
hold, where it keeps the hold in `Ending` so every later press is refused and
shutdown waits for ever. *Decision:* each call may run 10 seconds, then is stopped
and reported as `DeliveryError::TimedOut`. *Why:* nobody waits for these; the value
is a long multiple of what a working herdr needs and short enough that the person
hears about a failure while still at the keyboard. It is not a configuration key:
nobody has a reason to want a longer wait, and a key nobody needs is a key to
document and to get wrong.

**Pane read: bounded, 5 seconds, fixed.**
*Context:* `herdr pane read` runs after the take ends and before recognition, so
the person is already waiting on audio they have spoken, and nothing consumes its
result until the engine uses the bias string. *Problem:* a wedge delays every
take by the whole wedge. *Decision:* 5 seconds, then the read fails like any other
read failure: the bias string carries file names only and the take goes on.
*Why:* it is on the delivery side of the line, and shorter than delivery because it
delays recognition and a miss costs only bias, not the take.

**Transcriber command: bounded, 60 seconds, configurable.**
*Context:* a bound exists to turn a hang into a message, not to cap work somebody
asked for (`docs/decisions.md`, the entry on the client's two bounds). The measured
transcriber answers a 70-second take in 1.65 seconds and the built-in engine takes
12 to 14 seconds for 66 seconds (`docs/evidence.md`). *Problem:* "no bound" leaves
the hold path with no message at all and a daemon that refuses dictation and cannot
shut down; on the toggle path it ends with the client's 120-second "did not answer".
Only a bound can release either thread. *Decision:* 60 seconds by default, raised
by `[stt] command_timeout_seconds` for a slower transcriber, and a timeout names the
program, the bound and the key. *Why:* 60 seconds is 36 times the measured time for
the longest take recorded and below the client's 120 seconds, so the toggle path
still gets the program's name and not the daemon's. A value below 1 is raised to 1,
as `blink_ms` is raised to its floor, because an out-of-range value is not a reason
to stop the daemon starting.

**Rewrite command: bounded, 30 seconds, fixed.**
*Context:* the HTTP rewrite engine is already bounded at 30 seconds
(`src/rewrite/http.rs:93`) and the shipped agent measurements ran 6.2 to 28.7
seconds a call against it (`docs/evidence.md`). *Problem:* the same wedge as the
transcriber, on the same threads. *Decision:* 30 seconds, no key; a timeout is the
rewrite's existing failure, so the transcript is delivered unrewritten and the
person is told once why. *Why:* one bound for the two ways of reaching a rewrite
program, and the rewrite is optional work, so waiting longer for it costs the take.

**The sum on the toggle path.** Defaults add to 5 + 60 + 30 + 10 = 105 seconds, and
a failed delivery adds one more 10-second call for the toast, 115 seconds: under the
client's 120. A person who raises `command_timeout_seconds` above about 60 accepts
that on the toggle path the client's report can come first; the take still finishes,
delivers, and is journaled. The hold path has no client and is not affected.

## 4. What the person sees

New variants carry the program, the bound and what to do:

- `DeliveryError::TimedOut { binary, bound }`: `"herdr" did not answer within 10
  seconds, so the plugin stopped it. If herdr is not responding, restart it, then
  dictate again`. It flows into the existing failure arm, which writes the
  `delivery failed` journal line and a reply naming the pane, this text and the
  kept take (`src/daemon.rs:1006`). Nothing in that arm changes.
- `CommandError::TimedOut { program, bound }` in the transcriber and in the rewrite
  engine: `"<program>" did not finish within 60 seconds, so it was stopped. A long
  take can need more: raise [stt] command_timeout_seconds, or run the program by hand
  on the take to see where it stops`. The rewrite text has no key and says "run the
  command by hand".
- `PaneError::TimedOut { program, bound }`: the same shape, naming the bound the call was given, recorded where a pane-read
  failure already is.

**Hold path.** The watcher returns from `end_take` and the hold goes back to
`Idle`. A delivery failure is reported by the failure arm itself, not by
`report_failure` (`src/daemon.rs:1006-1027`): it writes the `delivery failed` journal
line, raises its own toast, and returns `Reported::Yes` so nothing reports twice. That
arm does not change. When the call that timed out was herdr delivery, its toast goes
through the same herdr, is a second call with its own 10-second bound, and if it also
times out `toast_failed_line` is written. So with herdr wedged the journal line,
which carries the timeout text, is the report the person can rely on, and the toast is
best effort. A timeout in transcription or rewrite reaches the person through
`report_failure` or the rewrite's own once-only notice, which are toasts through the
same herdr and bounded the same way. The worst case for the watcher with herdr fully
wedged is the pane read, the delivery and its toast: 5 + 10 + 10 = 25 seconds.

## 5. Configuration key

`[stt] command_timeout_seconds`, an integer, default 60, read only by
`engine = "command"`. Absent means 60 and an absent file stays valid. Documented in
`docs/design.md` section 7 beside the other `[stt]` keys. The name is proposed to the
run's orchestrator before S4 and is not final until it answers.

## 6. Tests, by acceptance criterion

All run without a live herdr. A program that sleeps is `sh -c "sleep 30"` wherever
the call site takes an argument list, which writes no script file and so is not
exposed to the `Text file busy` failure of issue #66; the herdr deliverer takes a
single binary and uses the script-writing recorder `src/delivery.rs` already has.
Bounds are passed in, so a test uses 200 milliseconds and asserts the call returned
within the bound plus two seconds.

- AC-3: each constant is asserted in the module that owns it, next to the number in
  the decision entry.
- AC-4, AC-5: for each of the five sites, a sleeping program returns `TimedOut`
  within the limit, and the message contains the program, the bound and the next step.
- AC-6: `transcribe`-level test with a `HerdrDeliverer` over a fake herdr that
  sleeps on `send-text`: the journal holds `delivery failed`, the reply names herdr.
- AC-7: a transcriber and a rewrite program that sleep produce a reply and a
  recorded rewrite failure naming the program.
- AC-8: `run` is given `sh -c "sleep 30 & wait"` with a marker the sleeping child
  would write; after the timeout the child is gone, checked by the process group
  having no members.
- AC-9, AC-10: `shorten` on strings with a two-byte, three-byte and four-byte
  character straddling the limit, and the two engines run with a failing program
  whose standard error is 399 ASCII bytes followed by a multi-byte character; the
  reported error holds the 399 bytes and does not panic.
- AC-11: a test reads the sources of `src/` and fails on `.truncate(` outside this
  design's single allowed use of a byte vector in a test fixture. Not a grep in a
  shell, so the gate survives the next change.
- AC-12: the config test for an absent key and for `0`.
- AC-14: `end_take` run with a fake herdr that hangs only on `send-text`: the
  call returns, the hold is `Idle`, a following `ptt` is accepted, the journal has the
  line and the toast was attempted. A second test hangs every subcommand and asserts
  `toast_failed_line` is written and the watcher is back within the sum of the
  three bounds it can meet, 25 seconds at the defaults, which the test shortens by
  passing small bounds.
- AC-15: not applicable, the transcriber has a bound.

## 7. Alternatives not taken

- **No bound for the transcriber, a recorded reason.** Rejected: the hold path would
  have no message and no recovery, and the toggle path would end with the wrong
  cause after 120 seconds.
- **A bound scaled to the take's length.** Rejected: it needs the recording's
  duration at the call site and a second key for the factor, for a cost the measured
  numbers do not show.
- **One bound for every herdr call.** Rejected: the pane read delays recognition and
  can afford a shorter one.
- **A crate for the wait** (`wait-timeout`, `command-group`). Rejected: about forty
  lines of standard library replace it, and `docs/decisions.md` allows no addition
  without a task that cannot be done otherwise.

## 8. Risks

- `kill` is run as a program on Unix. If it is missing the group is not killed, the
  direct child still is, and the call still returns; the test for AC-8 would fail in
  such an environment, which is the signal.
- Abandoned reader threads live until the process outside the group closes its pipe.
  Bounded in practice by the kill and recorded as a limitation, not hidden.
- 10, 5, 60 and 30 seconds are chosen from measurements of working calls, not of
  wedged ones. S5 records the time a normal herdr call takes on this machine so the
  margin is a number and not a belief.
