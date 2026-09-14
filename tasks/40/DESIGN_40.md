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

enum Stage { Context, Recognising, Rewriting, Delivering }
```

Set by the code that enters each stage, read by the drawing thread. It is
display only: nothing decides anything from it, so a stale or missed update
costs a wrong label for one interval and never a wrong take.

**Why.** The alternative is deriving the stage from the hold state plus a guess
about where the pipeline is, which is how a display starts disagreeing with the
thing it displays. One value, written where the work actually starts.

**Who writes it, exactly.** Four sites, and they cover both the hold and the
toggle, because `transcribe` is shared by them:

| site | writes |
|---|---|
| `ptt`, on `Started::Began` | `Recording` |
| `dictate`, on `Started::Began` | `Recording` |
| `end_take` and `dictate`'s second half, before `take_bias` | `Working { stage: Context }` |
| `transcribe`, before recognition, before rewrite, before delivery | `Working` with each stage |
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
it. On each tick where the stage has changed since the last, it renames. On the
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
request at all. Renaming per stage rather than per renewal keeps a take to four
or five renames instead of one every few hundred milliseconds — a tab bar that
flickers is worse than one that does not move.

## 4a. A daemon that was killed

**Context.** The token lapses on its own; the tab label does not. Every restore
in section 4 runs inside a living daemon — noticing `Idle`, a next take, or the
thread being joined at shutdown. A daemon killed outright does none of them.

**Problem.** The criteria say nothing decorated is left behind when the run ends
"any way at all, including the daemon being killed", and AC-10 asks for that to
be shown by hand. As sections 3 and 4 stand, the token goes and the label stays
— exactly the permanently decorated tab `docs/design.md` section 6 warns about.

**Decision.** The decoration is a prefix, and a prefix is removable without
knowing what it was attached to. At daemon start, before anything else draws,
the drawing thread sweeps: it lists every tab, and for each label that begins
with the plugin's prefix it renames that tab to the label without it. No stored
state, no file, no record of what the previous daemon was doing — the decoration
identifies itself, and removing a prefix from a string leaves the string.

**The listing is one call, and it covers everything.** `herdr tab list`, with no
`--workspace`, returns every tab in every workspace with its `tab_id` and its
`label`. Verified by running it on 2026-09-14: 50 tabs across 16 workspaces in
one answer. Scope is therefore not a question the sweep has to settle — at
daemon start there is no invocation context and so no workspace to scope to, and
none is needed.

**The prefix has to be unmistakable, and that is a constraint on the answer to
section 10.** People already put their own prefixes on tab labels, bracketed
ones among them — observed on the machine this was developed on, where several
tabs carry a bracketed project marker ahead of their name. A decoration that
looks like an ordinary bracketed prefix would make this sweep strip labels
nobody decorated. Whatever string is chosen must be one a person would not type.

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
the thread is decorating a live take would strip its own fresh prefix, and then
— because section 4 renames only when the stage changes — the tab would stay
undecorated until the next stage, and the restore would find a label differing
from what it last wrote and leave the tab alone, exactly as though somebody else
had renamed it. Waiting for an idle tick costs nothing: a decoration left by a
killed daemon is already there, and a few seconds more changes nothing, while a
sweep that collides with a live take breaks the take's own display and its
restore.

An idle tick is also the moment the same argument as at start applies again:
there is no live decoration to strip, because the thread holds none.

**Why.** The alternative is remembering the decoration across a kill, which
means a file, and a file is the thing #17 removed from this plugin for good
reasons. Self-identifying decoration costs one listing at start and closes the
case the criteria actually name.

**What this ties to section 10.** It works because the decoration is a prefix.
If the owner chooses a replacement instead, the original is not recoverable from
the label, and the killed-daemon case needs stored state or has to be given up —
so that decision is no longer only about how the tab bar reads.

## 5. What the two places show

**Context.** The criteria ask that both places show the same thing, so that a
collapsed sidebar loses only the place and not the information.

**Problem.** The token can carry a clock cheaply, because it is rewritten every
interval anyway. The label cannot, because rewriting it every interval is the
flicker of section 4.

**Decision.** Both carry the same **stage**. The token also carries elapsed
time, which the label does not. So the label says what is happening and the
token says what is happening and for how long.

**Why.** This is a departure from the criteria's requirement 3 read strictly,
and it is named rather than hidden: the information that matters when a sidebar
is collapsed is which stage the take is in, and the second of the two — how long
— is the one that costs a rename every interval to show.

## 6. Failure

**Context.** Drawing means running `herdr`, which can be absent, slow or
refuse.

**Decision.** A failed draw is recorded once per take and drawing is then
disabled for the rest of that take. The take continues untouched. The restore in
section 4 is still attempted at the end, because a tab decorated before the
failure would otherwise stay decorated.

**Why.** An indicator is an optimisation; the take is not. Recording once rather
than per interval is what keeps a broken herdr from filling the journal at two
lines a second.

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
stage and the elapsed time; the tab is decorated once per stage; the restore
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

## 10. Open, and the owner's rather than this stage's

**What the token is called and what it says.** Both are visible in the sidebar.
The shape is fixed — a name and a short value, rewritten every interval — and
the strings are not. Proposed, pending the owner: name `voice`, value
`REC 0:05` while recording and the stage's own word after that.

**What the decorated label looks like.** Answering this decides more than how
the tab bar reads: section 4a recovers from a killed daemon by stripping the
prefix, which only works because a prefix is removable. A replacement would need
stored state to recover, or would give that case up.

 `AC_40.md` hands this stage the choice
between a prefix, a suffix and a replacement, and what to do with a label that
is already long — and every one of those is text a person reads in their own tab
bar, which makes it the same kind of decision as the token's wording rather than
a mechanism. The restore's comparison and the per-stage rename both need the
exact string, so it cannot be left to whoever implements it. Proposed, pending
the owner: a prefix, so that a tab named by its owner keeps its name visible,
and no truncation, because a tab bar already truncates and doing it twice loses
more than it saves.

**Whether it blinks at all.** `docs/design.md` section 6 says it blinks. A
steady token with a running clock says more and is quieter. Proposed, pending
the owner: steady, on the grounds that the clock already proves it is alive and
a blinking token in a sidebar is harder to ignore than to read.

This one is not a flag, and that is worth knowing before answering it. The token
outlives three renewal intervals by design, so skipping a renewal does not make
it disappear — blinking would need a clearing call every other interval, which
section 3 removes on purpose and AC-2 forbids in as many words. Answering
"blink" therefore reopens section 3 rather than flipping a switch: the token
would have to be cleared and reset, and the property that a killed daemon leaves
nothing behind would have to be re-established some other way.
