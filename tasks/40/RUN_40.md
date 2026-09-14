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
