# RUN_36

| field | value |
|---|---|
| issue | #36 — Rewrite: wire the stage into the pipeline, with the http and command engines |
| input | GitHub issue, read with `gh issue view 36` |
| stage | S1 |
| branch | feat/36-rewrite-http-command |
| opened | 2026-09-04 |

## Stages

<!-- One block per stage, appended, never rewritten. -->

### S1 Assess
- artifact: `AC_36.md`
- produced: 2026-09-04

Written directly from the issue body and the current state of `src/daemon.rs`,
`src/config.rs`, `src/doctor.rs`, `docs/design.md` and `spike/spike.sh`'s
`rewrite()` function — no ADO ticket exists for this repository; the GitHub
issue is the trigger (`CLAUDE.md`, "Trigger and run root"). No `Risk` field
exists to check.

One thing the issue's own body treats as settled turned out not to be, on
reading the prototype: the "short phrase skips the round trip" rule has no
prototype measurement behind it — `spike/spike.sh`'s `rewrite()` always calls
the rewrite model, unconditionally. `AC_36.md` demotes this from a settled
requirement to an item the design stage has to invent, alongside four others
(what counts as a foreign term, whether context reaches the prompt at all,
what "told once" means as a mechanism, and the exact new configuration keys).

Sequencing: this worktree branches from `origin/main` at `7dab991`, before
issue #26's pull request (#35) has merged — noted in `AC_36.md` so the plan
does not assume a state that may change under it.

```yaml
gate:
  stage: S1
  artifact: AC_36.md
  reviewer: designer
  verdict: null
  date: null
```

```yaml
gate:
  stage: S1
  artifact: AC_36.md
  reviewer: designer
  verdict: QUESTIONS
  date: 2026-09-04
  questions:
    - "What does the wired-in take path do when [rewrite] engine = \"agent\"
       — the shipped default — or any other unrecognised value, since the
       agent engine is out of this issue's scope? AC-1 requires the step to
       run for every non-off take; AC-6/AC-7's deliver-unchanged-and-tell-once
       path was written only for a configured http/command engine failing.
       Three materially different designs were possible and nothing in the
       ticket or the AC chose one."
  blocker: null
```

Noted by the reviewer, not a gate question: `doctor::rewrite_finding`'s
`"http" | "command"` branch reports "not built yet" for exactly the two
engines this issue builds — true today, false once it lands.

The demotion of the short-phrase skip rule was checked and confirmed
justified: `spike/spike.sh`'s `rewrite()` calls `claude -p` unconditionally,
with no length, term or name test anywhere in it — the only conditional is
the empty-output fallback. AC-5 correctly hands the heuristic to design
rather than claiming a ported behavior that does not exist.

### Answered

`"agent"` and any value that is none of the four named engines are treated
exactly as a live `http`/`command` failure is — routed through the same "no
engine available" path AC-6/AC-7 already require, not as a distinct case.
`docs/design.md`'s own rule already covers it without a new one: an engine
this issue does not implement invoking is not available. This keeps the
shipped default unchanged and needs no new code path. `AC_36.md` was revised
to state this in a new "Resolved during S1's gate" section, and AC-1, AC-6 and
AC-7 were reworded accordingly. AC-11 was added for the `doctor` correction.
This is an engineering-coherence question the ticket's own stated rule
already answers, not a scope or naming decision reserved for the owner, so it
was resolved here rather than escalated.

S1 continues; re-running the gate against the revised artifact.

```yaml
gate:
  stage: S1
  artifact: AC_36.md
  reviewer: designer
  verdict: READY
  date: 2026-09-04
  questions: []
  blocker: null
```

Confirmed: the answer applies the ticket's own stated rule directly, leaves
the shipped default's delivered output unchanged, and needed no code path
beyond what AC-6/AC-7 already required — not a decision reserved for the
owner.

Noted for S2, not a gate question: treating `"agent"` as "no engine available"
on the take path leaves `doctor::rewrite_finding`'s `"agent"` branch reporting
`Ok` whenever an agent binary is on `PATH` (`src/doctor.rs:266-287`), while a
take with that same configuration is told the engine is unavailable. AC-11
covers only the `"http"`/`"command"` branch. The design should decide how
`doctor` names this divergence for the shipped default, not just for the two
built engines.

S1 is closed. Next is S2 Design.

### S2 Design
- artifact: `DESIGN_36.md`
- produced: 2026-09-04

Written directly, per this run's own precedent from #26's S2 (async/overnight
workflow substitutes the project's own gate — planner via
`octoflow-reviewer-planner` — for `superpowers:brainstorming`'s live chat
approval). This session's user is directly present, but the same substitution
is used here too for consistency with how every other stage in this run has
proceeded, and because the decisions below are engineering choices
`CLAUDE.md` already delegates ("Всё остальное решай сам"), not scope, naming,
or spend decisions reserved for the owner — except the new `[rewrite]`
configuration keys, which are marked provisional, the same footing `[stt]
command` was introduced on.

Five real decisions made, each with a stated reason: the rewrite engine
receives the bias string from the start, unlike recognition which had to be
widened after the fact (§1); `ureq` as the HTTP client, blocking, matching the
no-async-runtime rule (§6); a concrete three-part skip heuristic, safe
because of its length gate even though its foreign-term/name checks are
unmeasured (§4); an `AtomicBool` for the tell-once mechanism reusing the
existing toast path, not a new one (§3); and `doctor::rewrite_finding`'s
`"agent"` branch changes state, not just wording, closing the divergence the
S1 gate found (§8).

```yaml
gate:
  stage: S2
  artifact: DESIGN_36.md
  reviewer: planner
  verdict: null
  date: null
```
