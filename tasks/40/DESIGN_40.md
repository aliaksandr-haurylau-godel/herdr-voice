# DESIGN_40

The indicator: while a take is recording, something on screen says so. Built
against `tasks/40/AC_40.md`.

## 0. What this builds

Two places say what the take is doing: a token on the pinned pane's row in the
sidebar, and the tab's label. Both are drawn by the daemon, which already owns
the take and its stages. The token renews itself and expires; the tab label is
decorated once per stage and restored at the end.

Nothing about capture, recognition, rewrite or delivery changes. This adds a
second consumer of state the daemon already has.

## 1. What "now" is

**Context.** Since #17 the daemon knows a hold's stage — opening, live, ending
— but the take's own stages after that live only in the order of calls inside
`end_take`: bias, then recognition, then rewrite, then delivery.

**Problem.** The indicator has to name a stage that nothing currently names.

**Decision.** `Runtime` gains one more piece of state, an `Activity`, behind its
own mutex:

```
enum Activity {
    Idle,
    Recording { target: String, tab: Option<String>, since: Stamp },
    Working { target: String, tab: Option<String>, stage: Stage },
}

enum Stage { Transcribing, Fixing }
```

Set by the code that enters each stage, read by the drawing thread. It is
display only: nothing decides anything from it, so a stale or missed update
costs a wrong label for one interval and never a wrong take.

**Why.** The alternative is deriving the stage from the hold state plus a guess
about where the pipeline is, which is how a display starts disagreeing with the
thing it displays. One value, written where the work actually starts.

**Who writes it, exactly.** These sites, and they cover both the hold and the
toggle, because `transcribe` is shared by them:

| site | writes |
|---|---|
| `ptt`, on `Started::Began` | `Recording` |
| `dictate`, on `Started::Began` | `Recording` |
| `end_take` and `dictate`'s second half, before `take_bias` | `Working { stage: Transcribing }` |
| `transcribe`, before rewrite | `Working { stage: Fixing }` |
| `transcribe`, after delivery returns | `Idle` |
| the take path, when the take is finished, failed, discarded or abandoned | `Idle` |

A missed write costs one interval of a stale label. A missed `Idle` costs
nothing either: the token lapses on its own, and the restore of section 4 does
not depend on seeing `Idle` — see below.

## 2. Who draws

**Context.** The watcher thread runs the pipeline, and the pipeline takes
seconds — exactly the seconds whose stages need showing.

**Problem.** A thread blocked inside recognition cannot also renew a token every
few hundred milliseconds.

**Decision.** A second thread, started with the daemon and living as long as it
does, that wakes on the renewal interval, reads `Activity`, and draws. It never
writes `Activity` and never touches the recorder.

**Why.** Renewal is periodic and the pipeline is not; putting them on one thread
means the indicator stops exactly when it is most wanted. The drawing thread
also cannot hold anything the take path needs — it takes the `Activity` guard,
copies, drops it, and spends the rest of its time in subprocesses.

**The thread keeps its own memory, and `Activity` stays publish-only.** What it
decorated, what the label was before it did, what it last wrote there, and
whether drawing has failed for this take are the drawing thread's own locals —
not fields of `Activity`, which only the take path writes and only the drawing
thread reads. That is what keeps `herdr tab get` off the keypress path: the
label is read by the drawing thread on its first tick of a take, not by `ptt`,
which answers twelve times a second and must not run a subprocess (AC-6).

The cost is one interval of lag: the tab is decorated on the first tick after a
take begins, not at the instant it begins. At the default that is under a
second, against a take that lasts seconds.

**Its lifetime.** Spawned in `serve` beside the watcher, given the same stop
flag, woken and joined before `serve` returns — the same shape, for the same
reason: a thread that can still draw after the daemon has decided to stop would
leave a decoration behind.

**It waits on its own clock and measures with the take's.** The two are
different jobs and only one of them conflicts.

*Waiting* conflicts: `Clock`'s contract is that a wake is consumed by the return
it causes, so two waiters on one clock steal each other's wakes — and both the
hold's start and the daemon's shutdown depend on a wake reaching the watcher
(`src/daemon.rs:371`, `:1108`). So the drawing thread is constructed with its
own `Arc<dyn Clock>` and waits only on that.

*Measuring* does not conflict: reading `now()` takes nothing from anybody. And
it must not be done on the drawing clock, because `since` in
`Activity::Recording` is stamped by the take path from `runtime.clock`, and the
two clocks count from different origins — `Stamp` is milliseconds since its own
clock's origin (`src/ptt.rs:11`), and a `TestClock` starts at zero and moves
only when a test moves it. Subtracting one from the other would be arithmetic on
two different time bases.

**So: elapsed is `runtime.clock.now()` minus `since`, always; the drawing clock
is used for `wait_until` and for nothing else.** The drawing thread holds both.
A test then has two knobs that mean two different things — advance the take's
clock and the token's elapsed time grows; advance the drawing clock and the
token is redrawn — which is what lets a test show a clock that runs without the
indicator ticking, and an indicator that ticks without time passing.

## 3. The token, and why it needs no clearing

**Context.** `herdr pane report-metadata <pane> --source <id> --token <n>=<v>
--ttl-ms <n>` sets a token that disappears by itself when the time to live
expires. Verified by running it.

**Problem.** `docs/design.md` section 6 asks for the token to be cleared on
every exit path, and an exit path can be missed — the design says why that
matters.

**Decision.** The token is renewed every `blink_ms` with a time to live of three
times that, and is never cleared. When `Activity` is `Idle` the thread simply
stops renewing, and the token lapses within one time to live.

**Why.** Three renewals of headroom means an ordinary scheduling delay does not
make the token flicker, and a daemon that is killed, wedged, or finishes leaves
nothing behind within 1.8 seconds at the default. There is no clearing call, so
there is no exit path to miss — the requirement is met by construction rather
than by discipline. The cost is one subprocess per `blink_ms` while a take runs,
and nothing at all when none does.

## 4. The tab label, which has to be put back

**Context.** `herdr tab rename` is the only way to change a label and has no
time to live. `herdr tab get` reports the current one.

**Problem.** Every path that decorates must restore, and a person may rename the
tab themselves while the take runs.

**Decision.** The drawing thread owns the whole of this, start to finish, and
holds it in its own locals.

On the first tick of a take it reads the label with `herdr tab get` and keeps
it. On each tick it computes the steady form for the current state and renames only
if that differs from what it last wrote — section 5 states the rule and what it
comes out as. On the
first tick at which `Activity` is `Idle` **while it is still holding a
decoration**, it restores: read the label again; if it equals what the thread
last wrote, write the original back; if it differs, somebody renamed the tab
during the take, so leave it alone and record that. An original that was empty
is written back as empty. Then it forgets the take.

So the restore is driven by the thread noticing that it decorated something and
the take is over — not by the take path calling anything, and not by seeing a
particular transition. A take that ends without ever setting `Idle` still gets
restored, at the latest when the next take sets `Recording` for a different pane
or when the daemon stops and the thread is joined.

Drawing being disabled for a take (section 6) does not disable this. The
disable stops new decorations; the restore of a decoration already made is what
keeps a broken herdr from leaving a tab wrong forever, and it is attempted once
regardless.

**Why.** Reading before decorating is the only value that is certainly the
person's; the invocation context cannot serve, because it arrives only on
keypresses and after the key comes up the pipeline runs for seconds with no
request at all. Renaming only when the text would differ keeps the tab bar
still except when it has something new to say: once a second while a clock is
running in it, and once on entering a state that has no clock. A tab bar
rewritten on every renewal flickers; one rewritten when its content changes does
not.

## 4a. A daemon that was killed

**Context.** The token lapses on its own; the tab label does not. Every restore
in section 4 runs inside a living daemon — noticing `Idle`, a next take, or the
thread being joined at shutdown. A daemon killed outright does none of them.

**Problem.** The criteria say nothing decorated is left behind when the run ends
"any way at all, including the daemon being killed", and AC-10 asks for that to
be shown by hand. As sections 3 and 4 stand, the token goes and the label stays
— exactly the permanently decorated tab `docs/design.md` section 6 warns about.

**Decision.** The decoration is a suffix, and a suffix is removable without
knowing what it was attached to. At daemon start, before anything else draws,
the drawing thread sweeps: it lists every tab, and for each label that carries
the plugin's marker it renames that tab to the label with the marker and
everything after it removed, trailing separator included. No stored state, no
file, no record of what the previous daemon was doing — the decoration
identifies itself, and cutting a suffix off a string leaves the string.

The marker is the `🎙️` that every value in section 10 begins with, and the
decoration is written as the original label, one space, then the value. So the
sweep cuts from the first `🎙️` back through the space before it, and what
remains is exactly what was there before — including the empty label, which
decorates to a value with no leading space and sweeps back to empty.

**The listing is one call, and it covers everything.** `herdr tab list`, with no
`--workspace`, returns every tab in every workspace with its `tab_id` and its
`label`. Verified by running it on 2026-09-14: 50 tabs across 16 workspaces in
one answer. Scope is therefore not a question the sweep has to settle — at
daemon start there is no invocation context and so no workspace to scope to, and
none is needed.

**The marker has to be unmistakable.** People already put their own prefixes on tab labels, bracketed
ones among them — observed on the machine this was developed on, where several
tabs carry a bracketed project marker ahead of their name. A marker that
looked like an ordinary bracketed tag would make this sweep strip labels nobody
decorated. `🎙️` is not something a person types into a tab name by accident.

**The sweep runs whether or not the tab indicator is switched on.** `[ui]
tab_indicator = false` means this plugin does not decorate; it cannot mean that
decorations it left earlier stay forever. A person who turns the indicator off
because they did not like it would otherwise be left with whatever the last run
put there, and would have no way to get rid of it short of renaming tabs by
hand. Switching a thing off is when its leftovers should go, not when they
should be frozen.

The sweep runs once, at start. A daemon starting while another is already
running does not reach it — the second daemon exits at the connect check before
any of this — so it cannot strip a live decoration out from under a running
take.

**When the sweep cannot run.** herdr may be absent or refusing at that moment,
and section 6's rule does not reach here: it disables drawing for a take, and
the sweep belongs to no take. So: the failure is recorded once, the daemon
carries on serving — a plugin that will not start because a tab could not be
listed would be a worse failure than a decorated tab — and the sweep is retried
**at most once more**: at the first tick on which `Activity` is `Idle`, the
thread holds no decoration of its own, **and some draw has succeeded since the
daemon started**. After that second attempt it is not retried again, whether it
succeeded or not.

The bound matters as much as the retry. Without it — with herdr absent, where
`Activity` is `Idle` and the thread holds nothing on every single tick — the
literal rule would be a `herdr tab list` subprocess every `blink_ms` for as long
as the daemon lives, which is the flooding section 6 exists to prevent, on the
one path section 6 does not cover. Gating the second attempt on a draw having
succeeded is what makes it evidence-based rather than hopeful: a successful draw
is proof that herdr answers. And if no draw ever succeeds, nothing is being
decorated either, so there is nothing new to clean up and the leftover waits for
the next daemon start — which is honest, bounded, and quiet.

**Not during a take**, which was the first answer written here and is wrong for
the same reason the sweep is safe at start: a sweep firing while
the thread is decorating a live take would strip the decoration it had just
written, and the restore would then find a label differing from what it last
wrote and leave the tab alone, exactly as though somebody else had renamed it.
The tab would also stay bare until the next rename, which in the working states
is the next state change, because nothing else in those strings moves. Waiting for an idle tick costs nothing: a decoration left by a
killed daemon is already there, and a few seconds more changes nothing, while a
sweep that collides with a live take breaks the take's own display and its
restore.

An idle tick is also the moment the same argument as at start applies again:
there is no live decoration to strip, because the thread holds none.

**Why.** The alternative is remembering the decoration across a kill, which
means a file, and a file is the thing #17 removed from this plugin for good
reasons. Self-identifying decoration costs one listing at start and closes the
case the criteria actually name.

**Why this works at all.** Because the decoration is appended rather than
substituted. A replacement would leave the original unrecoverable from the label
and would need stored state — a file, which #17 removed from this plugin — or
would give the killed-daemon case up.

## 5. What the two places show

**Context.** The criteria ask that both places show the same thing, so that a
collapsed sidebar loses only the place and not the information.

**Decision.** Every state's value has two forms, identical but for the second
character: the **steady** form and the **blink** form, in which that character
is replaced by a space. The token alternates between them on every renewal —
that is the blink. The tab label always carries the steady form.

That gives one rule for each surface, and no third:

- **The token is written on every tick**, alternating the two forms.
- **The tab is renamed only when its steady form differs from the one last
  written.** In `REC` that is once a second, because the clock is in it. In
  `TRANSCR` and `FIX` it is once, on entering the state, because nothing in
  those strings changes.

**Why.** The earlier answer here took the clock off the tab to avoid renaming
it every renewal; the answer before that had three different triggers in three
sections. One rule — write when the text would differ — produces the right rate
on each surface without anybody choosing a rate, and it is the same rule for all
three states. The tab does not blink, and that is deliberate: a tab bar that
flashes a character twice a second is noise in the corner of the eye, while the
same animation in the sidebar sits where somebody looks on purpose.

## 6. Failure

**Context.** Drawing means running `herdr`, which can be absent, slow or
refuse.

**Decision.** A failed draw is recorded once per take, and drawing carries on
being attempted. The take continues untouched. The restore in section 4 is
attempted at the end regardless, because a tab decorated before the failure
would otherwise stay decorated.

**Why.** An indicator is an optimisation; the take is not, so a failure must
never reach it. But giving up for the rest of the take is worse than it sounds:
the token is kept alive by being renewed, so a thread that stopped renewing
would let it lapse within three intervals, and the sidebar would then say the
take was over while the person was still speaking. A missing indicator is a
nuisance; an indicator that says "finished" mid-sentence is a lie. Retrying
costs one subprocess per interval against a herdr that is refusing, bounded by
the length of the take — and if herdr is refusing, the delivery at the end of
that take is going to fail too.

Recording once rather than per interval is what keeps a broken herdr from
filling the journal at two lines a second.

This reverses the first answer written here, which disabled drawing for the
take. It was changed when the plan's own tests turned out to forbid it and the
reason above came out of asking why.

## 7. Configuration

```toml
[ui]
sidebar_token = true
tab_indicator = true
blink_ms = 600
```

Either half off independently; `blink_ms` is the renewal interval, and the time
to live is derived from it rather than configured, so the two cannot be set into
a combination that flickers.

## 8. Tests

Drawing goes through a trait with a recorded implementation, the way delivery
already does (`src/delivery.rs:64`), so every test runs with no herdr. The clock
seam from #17 drives elapsed time and the renewal interval, so no test waits.

Covered: a token is renewed while recording and not after; its value carries the
stage and the elapsed time; the tab is renamed exactly when its steady form changes — once a second while recording, once per state otherwise — and never on a blink; the restore
writes the original back; a label changed by somebody else is left alone; an
empty original is restored as empty; a draw failure is recorded once and
disables drawing for that take but not the take itself; both halves obey their
configuration keys; and no draw happens on the keypress path.

## 8a. One piece of plumbing the write sites need

`tab_id` is parsed from the invocation context (`src/context.rs:15`) but is
carried neither by `Hold` nor by `Take`. `transcribe` is named in section 1 as a
writer of `Working { target, tab, stage }` and has no tab in hand today. So the
tab has to be carried alongside the pane, the same way the pane already is,
from the moment a take is pinned through to delivery. It is ordinary plumbing
with a checkable outcome, named here so the plan does not discover it.

## 9. What this does not do

- The take's level (#59), though the token is where it would go.
- `--title`, `--display-agent` and `--state-label`, the rest of what
  `report-metadata` offers.
- Anything about `cancel` (#18), which still stops nothing.

## 10. What the indicator says, and where

Settled by the owner on 2026-09-16, after a probe that drew all three
mechanisms at once on a live pane and tab.

**Three states, one line each:**

| state | value |
|---|---|
| recording | `🎙️🔴 REC 0:05` — the red dot blinks |
| transcribing | `🎙️📝 TRANSCR` — the memo blinks |
| fixing | `🎙️🪄 FIX` — the wand blinks |

Bias assembly is part of `TRANSCR` rather than a state of its own: it takes
fractions of a second, and a state that flickers past in a hundred milliseconds
cannot be read.

**Delivery is not a displayed state either, for the same reason and one more.**
It is a single call that inserts the text, and the moment it succeeds the text
is in the input box — which is a louder signal than any token. So there are
three states and not four: `Activity` goes from `Fixing` straight to `Idle` when
delivery returns, and the token lapses. `AC_40.md` AC-3 named recognition,
rewrite and delivery; it is corrected to the two that are worth naming, and the
correction is recorded there.

**It blinks by alternating the second character, not by disappearing.** The
value is rewritten every renewal anyway, so blinking costs nothing and takes
nothing back: the token is present on every renewal, and only the icon
alternates. This is why the blink question turned out not to reopen section 3 —
nothing is ever cleared.

**The blink form is the same width as the steady form.** The icon is replaced,
not removed: where the steady form has the coloured glyph the blink form has
U+3000 IDEOGRAPHIC SPACE, so the blank covers the same two columns and nothing
to the right of it moves. Removing the glyph instead makes the rest of the label
slide left and back twice a second, which reads as the label shaking rather than
as one character blinking. U+3000 is chosen because a terminal lays its cells
out by East Asian Width and it is the one blank that is Wide there, as the icons
are; an ordinary space is Narrow and would move the text by half a cell.

**It is a suffix, after the name**, written as the label, one space, then the
value. That is what section 4a's recovery from a killed daemon cuts back off. Every value begins with `🎙️`,
which is the marker the sweep cuts from — see section 4a — and is not something
a person types into a tab name by accident.

**Two mechanisms paint three surfaces, and the third mechanism is not used.**

| surface | painted by |
|---|---|
| the tab bar at the top | the tab's label |
| the sidebar's tab row | the tab's label — the same rename paints both |
| the sidebar's agent row | the pane's token |

`herdr pane report-metadata --display-agent` is deliberately **not** used.
Drawn alongside the other two it produces a third copy of the same status in the
same row, and it displaces the agent's real name, which is then truncated. The
probe that established this showed all three at once and the row read as three
repetitions of one fact.

**Both surfaces carry the same text; only the token animates.** One rule governs
each, and the rates follow from it rather than being chosen: the token is
written on every tick, alternating the steady and blink forms; the tab is
renamed when its steady form differs from the one last written. In `REC` that
comes out as one rename a second, because the clock is in the string. In
`TRANSCR` and `FIX` it comes out as one rename on entering the state, because
nothing in those strings changes. None at all when no take is running.

The cost of putting the clock on the tab is therefore one rename a second during
a take, and it is stated rather than hidden. Renaming on every renewal instead
would move the tab bar several times a second for a character nobody reads
there.