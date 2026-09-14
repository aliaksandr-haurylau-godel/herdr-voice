# RUN_40

The indicator: while a take is recording, something on screen says so.

Input: GitHub issue #40, read with `gh issue view 40`. The issue was opened by
this project rather than by a tracker, out of `docs/design.md` section 6, which
describes an indicator none of whose parts exist.

Run root: `tasks/40/`. Branch `feat/40-indicator`, cut from `main` at `914471a`.

## Stages

### S1 Assess

Two things were established by running herdr rather than by reading it, before
any criterion was written, because they decide what the criteria can ask for.

**herdr has a mechanism for the sidebar token, and it carries a time to live.**
`herdr pane report-metadata <pane> --source <id> --token <name>=<value>
--ttl-ms <n>` sets a token on a pane; `herdr pane get` then shows
`tokens: {"voice": "REC 0:05"}`, and after the time to live it shows nothing.
Verified on 2026-09-14 with a six-second token, which cleared itself.

That inverts the requirement `docs/design.md` section 6 states. The design asks
for the token to be cleared on every exit path, which is a discipline that can
be missed — and the design says why it matters, because a decorated label left
behind stays wrong forever. With a time to live there is no exit path to miss:
the token is renewed while a take runs and disappears on its own when the daemon
stops renewing it, whether it finished, failed, was killed or wedged.

**The tab label has no such mechanism.** `herdr tab rename <tab_id> <label>` is
the only way to change it, and it is permanent until changed back. So the two
halves of the indicator are not symmetrical: one is safe by construction and the
other still needs the take's own bookkeeping.

The first answer written here said the value to restore arrives in the
invocation context as `tab_label` (`src/context.rs:16`). That is wrong, and the
gate's second question is what showed it: the context arrives only on a
keypress, so it cannot see a rename made during the seconds the pipeline runs
after the key comes up, and once the plugin has renamed the tab the context
reports the plugin's own decoration back to it. `herdr tab get <tab_id>` reports
the current label and is what the restore reads — before decorating, and again
before restoring, so that a rename by the person during the take is left alone
rather than clobbered.

Artifact: `tasks/40/AC_40.md`, 11 criteria.

Gate, round 1:

```yaml
gate: {stage: S1, artifact: AC_40.md, reviewer: designer, verdict: QUESTIONS, round: 1, date: 2026-09-14,
  questions:
    - "Requirement 1 said the token's time to live is shorter than the renewal
       interval, which would make the token absent between renewals and
       contradict AC-1 and AC-3, and would pre-decide the blink question the
       artifact hands to design."
    - "AC-5 needs a way to observe a tab's current label and none is named. The
       invocation context arrives only on keypresses, so it cannot see a rename
       made while the pipeline runs — and once the plugin has renamed the tab,
       the context reports the plugin's own decoration back."}
```

The first was an outright error: longer, not shorter. The second was answered by
running herdr rather than by choosing between AC-4 and AC-5 — `herdr tab get`
reports the current label, which makes a read-back comparison possible, so
neither criterion has to yield.

Three things came out of exercising the tab commands, all now in the criteria: a
tab's `label` and `number` are different fields, and a label that looks like a
number can still be one somebody typed (the tab used reported `label = "6"` with
`number = 8`); an empty rename is accepted and leaves the label empty, so "no
label" is a value a restore must be able to write back; and the restore is only
as good as the value read before decorating — established by decorating a live
tab and putting it back, which worked only because the original had been read
first. That is this issue's own failure mode, met while establishing its
mechanism.

Gate, round 2:

```yaml
gate: {stage: S1, artifact: AC_40.md, reviewer: designer, verdict: READY, round: 2, date: 2026-09-15, blocker: null}
```

Closed with one note, acted on rather than left: two passages still said the
label to restore comes from the invocation context, which requirement 2 now
rejects. Both are corrected. An artifact that contradicts itself is the defect
that cost #17 its criteria drifting to five seconds while the code did one.

### S2 Design

Artifact: `tasks/40/DESIGN_40.md`.

Six rounds before this entry was written, which is itself the first finding
worth recording: the gate blocks below were kept in the conversation and not in
this file until the reviewer pointed out that the run file ended at S1. A run
record written after the fact is a worse record than one written as it goes, and
this note is here so the next run does not repeat it.

```yaml
gate: {stage: S2, artifact: DESIGN_40.md, reviewer: planner, verdict: QUESTIONS, round: 1, date: 2026-09-15,
  findings: ["who reads the original tab label was stated three incompatible ways, and one of them put a herdr subprocess on the keypress path AC-6 forbids",
             "nothing said who performs the restore, and Idle carries no target, tab or original label to restore from",
             "the decorated label's form — prefix, suffix or replacement — was neither decided nor listed as the owner's",
             "the drawing thread's clock was unstated; sharing runtime.clock would steal the wakes the watcher's hold timing depends on",
             "whether the toggle path draws at all was unstated, and transcribe is shared by both paths"]}
gate: {stage: S2, artifact: DESIGN_40.md, reviewer: planner, verdict: QUESTIONS, round: 2, date: 2026-09-15,
  findings: ["giving the drawing thread its own clock left elapsed time computed across two time bases: since is stamped from runtime.clock and now() was read from the drawing clock"]}
gate: {stage: S2, artifact: DESIGN_40.md, reviewer: planner, verdict: QUESTIONS, round: 3, date: 2026-09-15,
  findings: ["a killed daemon leaves the tab decorated: the token lapses by its time to live, but every tab restore runs inside a living daemon"]}
gate: {stage: S2, artifact: DESIGN_40.md, reviewer: planner, verdict: QUESTIONS, round: 4, date: 2026-09-15,
  findings: ["the start-up sweep named no herdr call and no scope — the one mechanism in the design asserted rather than verified by running it",
             "what the sweep does when it cannot run was unstated, and section 6's once-per-take rule does not reach an event belonging to no take"]}
gate: {stage: S2, artifact: DESIGN_40.md, reviewer: planner, verdict: QUESTIONS, round: 5, date: 2026-09-15,
  findings: ["retrying the sweep on the first draw of a take contradicted the section's own safety argument: it would strip its own fresh prefix, leave the tab undecorated until the next stage, and make the restore see a label it had not written",
             "whether the sweep runs with [ui] tab_indicator off was unstated"]}
gate: {stage: S2, artifact: DESIGN_40.md, reviewer: planner, verdict: QUESTIONS, round: 6, date: 2026-09-15,
  findings: ["the retry was unbounded: with herdr absent the thread is idle on every tick, so the literal rule is a tab listing subprocess every blink_ms for the daemon's life — the flooding section 6 exists to prevent, on the path section 6 does not cover"]}
```

**What the six rounds have in common.** Every finding after the first round was
introduced by the previous round's fix. That is not a reviewer being pedantic:
the design reached a density where the mistakes stopped being in what a section
says and started being in how two sections combine. Separating the clocks fixed
wake-stealing and broke elapsed time. Adding the sweep fixed the killed daemon
and introduced a retry that collided with a live take. Bounding the collision
introduced an unbounded retry. Only reading the whole document each round finds
that class, which is why the reviewer is asked to do exactly that.

**Two things were settled by running herdr rather than by arguing**, and both
changed the design rather than confirming it: a pane token carries a time to
live and disappears on its own, which removed the clearing path entirely; and
`herdr tab list` with no workspace returns every tab in every workspace, which
removed the question of scope from the sweep. A third came from looking at what
the listing returned: people already put their own prefixes on tab labels, so
the plugin's prefix has to be one nobody would type, or the sweep strips names
it did not write.

```yaml
gate: {stage: S2, artifact: DESIGN_40.md, reviewer: planner, verdict: READY, round: 7, date: 2026-09-15, blocker: null,
  note: "the label-form decision in section 10 is more than a string for the plan: a prefix makes the sweep a task, a replacement removes it or replaces it with stored state. The plan will carry the sweep with an explicit dependency on the owner's answer."}
```

### Waiting on the owner

Three decisions, all user-visible, none of them this stage's to take. The design
carries a proposal for each and states what the alternative costs, so S3 can be
written against the proposals and only a literal changes — except the third,
which changes a task.

1. **What the token is called and what it reads.** Proposed: name `voice`, value
   `REC 0:05` while recording and the stage's word after that.
2. **Whether it blinks.** Proposed: steady. Answering "blink" is not a flag: the
   token outlives three renewal intervals, so skipping a renewal does not hide
   it, and blinking would need the clearing call section 3 removes and AC-2
   forbids.
3. **What the decorated tab label looks like.** Proposed: a prefix, no
   truncation. This one decides whether section 4a exists: recovering from a
   killed daemon works by stripping the prefix, and a replacement would need
   stored state — a file, which #17 removed from this plugin — or would give the
   case up. The prefix must also be a string nobody would type, because people
   already put their own prefixes on tab labels.
