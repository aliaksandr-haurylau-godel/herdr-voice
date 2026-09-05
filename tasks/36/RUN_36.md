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

```yaml
gate:
  stage: S2
  artifact: DESIGN_36.md
  reviewer: planner
  verdict: READY
  date: 2026-09-04
  questions: []
  blocker: null
```

Every citation checked and held, including the `Arc<Runtime>` construction
(`src/daemon.rs:470-483`) that makes the `AtomicBool` field actually work the
way the design assumes, and the four `Runtime` test literals that need the
new fields. The skip heuristic, the `command` placeholder substitution and
the #26 rebase sequencing were each confirmed writable into a task without
guessing.

Three corrections applied to `DESIGN_36.md`, none reopening the gate:
`runtime.rewrite_settings.skip_if_plain` was never declared — replaced with a
bare `runtime.skip_if_plain: bool`, since one field needs no group of its own
the way `delivery_settings` groups two. The `Relaxed`-ordering argument
claimed `serve` handles connections sequentially; `serve` spawns a thread per
connection (`src/daemon.rs:509-513`), so the two-notices-instead-of-one race
is real, not hypothetical — the conclusion (still acceptable) stands on the
corrected reason. `rewrite_finding`'s signature is stated to change from two
`&str` parameters to `&config::Rewrite`, since §8 needs `url` and `command`
which the old shape cannot carry.

S2 is closed. Next is S3 Plan.

### S3 Plan
- artifact: `PLAN_36.md`
- produced: 2026-09-04

Ten tasks: `ureq` dependency + decision (1), `config::Rewrite`'s new keys (2),
`rewrite::command` (3), `rewrite::http` (4), the trait/`Resolution`/`resolve`
tying them together (5), the skip heuristic (6), daemon wiring (7),
`doctor::rewrite_finding` (8), S4 review (9), S5 verify plus the manual runs
(10).

Self-review found and fixed two things before this went to gate: a vague
"check how `stt::tests_support` is declared" instruction, replaced with the
actual declaration read from the file and full code for the new module's own
`tests_support::Fake`; and four daemon-level tests in Task 7 that asserted
only `Reply::Ok`, which every one of the four `Resolution` branches produces
identically and so proved nothing about which branch actually ran. Rewrote
all four to inspect `FakeDeliverer::calls()` — cloned before the fake is
passed into `runtime_with`, the same idiom
`a_toast_is_raised_on_a_failed_delivery_only_when_ui_toasts_is_on`
(`src/daemon.rs:979-1002`) already uses — so each test checks the actual
delivered text: rewritten for a working engine, unrewritten and reasoned
about a specific text for `Unavailable`/a failed engine/`Off`.

```yaml
gate:
  stage: S3
  artifact: PLAN_36.md
  reviewer: implementer
  verdict: null
  date: null
```

```yaml
gate:
  stage: S3
  artifact: PLAN_36.md
  reviewer: implementer
  verdict: QUESTIONS
  date: 2026-09-04
  questions:
    - "Task 7's Off and Unavailable tests both asserted only the delivered
       text, so nothing distinguished them from each other, and the Coverage
       table named a test Task 7 never wrote."
    - "Task 6's two context-word tests used 'worklog', a Latin word, so
       has_latin_run already returns false before shares_a_word is reached —
       a broken shares_a_word would still pass both."
    - "Task 8's signature change breaks a pre-existing test
       (the_rewrite_engine_is_looked_for_by_name) that the plan never shows
       converted."
  blocker: null
```

### Answered

All three fixed directly in `PLAN_36.md`. Task 7: the Unavailable test now
also asserts exactly one journal notice, and the Off test (renamed
`resolution_off_never_tells`) asserts zero — the two are now distinguished by
the one thing that actually differs between them. The Coverage table's
reference corrected to the real test names. Task 6: both context-word tests
now use "журнал" (no Latin letters) shared between transcript and bias
instead of "worklog", so `has_latin_run` cannot short-circuit before
`shares_a_word` runs, with a comment stating why. Task 8: the pre-existing
`the_rewrite_engine_is_looked_for_by_name` is now shown fully converted to
the new one-argument signature, with its stale comment on the third assertion
replaced.

S3 continues; re-running the gate against the revised artifact.

```yaml
gate:
  stage: S3
  artifact: PLAN_36.md
  reviewer: implementer
  verdict: QUESTIONS
  date: 2026-09-04
  questions:
    - "Task 5's Resolution enum has no Debug, but its own tests print
       {other:?} in a panic message — Box<dyn Engine + Send + Sync> cannot
       derive Debug, and nothing in the plan showed a manual impl."
  blocker: null
```

The two prior fixes were checked by hand and confirmed to hold: walking
`plain(\"открой журнал\", \"журнал notes.txt\", true)` through all three
checks in order confirmed `shares_a_word` alone decides the result; the
`Off`/`Unavailable` tests were confirmed to differ only in journal-notice
count, catching a swapped-branch or over-eager-`tell_once` build.

### Answered

Added a manual `Debug` impl for `Resolution` to Task 5's Step 3 — the
`Engine` variant renders as a placeholder (`"Engine(..)"`), never its
contents, since `Engine` carries no `Debug` bound (matching `stt::Engine`,
which has none either). Also corrected a small citation the same pass
noticed: Task 2's `impl Default for Rewrite` is at `src/config.rs:83-90`,
not `:79-84`.

S3 continues; re-running the gate against the revised artifact.

```yaml
gate:
  stage: S3
  artifact: PLAN_36.md
  reviewer: implementer
  verdict: READY
  date: 2026-09-04
  questions: []
  blocker: null
```

Fourth read, hunting specifically for cross-task inconsistency left by three
rounds of piecemeal fixes: the `Debug` impl compiles against `Resolution`'s
actual variants, Task 7's `Resolution` constructions match Task 5's three
variant names, and every type crossing a task boundary (`CommandEngine`,
`HttpEngine`, `EngineError`, `CommandError`, `HttpError`) is constructed and
matched consistently wherever it appears. Also confirmed: issue #26 has not
merged (`transcribe` still takes no `bias` parameter), so Task 7's "not
merged" branch is the one that applies.

S3 is closed after three rounds. Next is S4 Implement.

## Gate S4

```yaml
gate:
  stage: S4
  artifact: the diff on feat/36-rewrite-http-command since 7dab991
  reviewer: two independent reviews, one hunting defects by mutation, one
    checking conformance and real behaviour against a live local server
  verdict: QUESTIONS
  date: 2026-09-04
  blocker: null
```

1142 insertions across `src/rewrite.rs`, `src/rewrite/command.rs`,
`src/rewrite/http.rs`, `src/rewrite/skip.rs`, `src/daemon.rs`, `src/doctor.rs`,
`src/config.rs`, `src/main.rs`, `Cargo.toml`, `docs/decisions.md`. All four
gates green, 242 tests, 12 full-suite reruns plus 20 targeted `rewrite::`
reruns at 16 threads with zero flakes — the implementer's fix for the test
double's original intermittent failure (draining the request fully before
responding) holds.

No correctness defect confirmed. Both reviews mutation-tested the load-bearing
behaviors and everything held: the tell-once `AtomicBool` (both
always-notify and never-notify mutations caught, and `swap`'s own atomicity
rules out the two-threads-both-notify race the daemon's one-thread-per-
connection model would otherwise risk); `{transcript}` forced /`{bias}` never
forced in `rewrite::command`; the skip heuristic's three checks, each caught
by a distinct test with no accidental short-circuit — including confirmation
that the Latin-loanword masking bug the S3 gate found in the *plan* was
genuinely fixed in the *code*, not just the document; `doctor::rewrite_finding`
reporting `agent` as `Missing` unconditionally, by construction; no leak of
the transcript, the bias string or a rewritten transcript into any new log or
error path; no panic path in the diff's new production code; the public-
repository rule; and every pre-existing test the plan required to change was
updated meaningfully, not weakened to compile.

A live check found something worth recording separately from the gate: Ollama
is installed and running on this machine right now, with `gemma4:latest`
loaded, and a reviewer's ad hoc, uncommitted check drove the real compiled
`HttpEngine` and `CommandEngine` against it — both restored "мерж реквест" /
"пул реквест" to "merge request" / "pull request" in a real round trip. Not
recorded as evidence (ad hoc, not committed, not the owner's own run per
AC-10's wording) — but it means AC-10's live check can happen on this machine
directly, without needing separate audio/microphone access the way #21's and
#26's manual steps did.

### Sent back for fixing

1. **AC-5 (skip means the engine is never called) has no test at the
   `daemon::transcribe` level.** `rewrite::skip::plain` is well tested in
   isolation, but every daemon-level test either uses a long string
   ("so the skip heuristic never applies here", by its own comment) or the
   canned recognition text `"fix the worklog entry"`, which is all-Latin and
   so never actually skips regardless of whether the wiring is even present.
   Add a daemon-level test with a short, plain, no-context-name transcript and
   a spy `Engine` that panics or records a call if `.rewrite()` is ever
   invoked, proving the skip really does bypass the engine, not just that
   `plain()` returns `true` in isolation.
2. **Two `rewrite::http` branches have zero test coverage.** A non-2xx
   response (`ureq::Error::Status`) and a 200 response whose JSON is
   well-formed but lacks `choices`/`message`/`content` are both handled
   correctly in the code (neither panics) but neither is exercised by any
   test. Add one test per branch using the same scratch-`TcpListener` double
   the existing tests already use.

Neither finding blocks on a design or scope question — both are additional
tests against already-correct code, assignable back to the implementer
directly.

Both fixed and verified. `a_short_plain_transcript_with_a_configured_engine_still_skips_it`
(`src/daemon.rs`) uses `"открой файл"` (verified against `rewrite::skip::plain`'s
three checks) with a `Fake` engine that would return an observably different
string if wrongly invoked; the implementer confirmed it directly by breaking
the skip check and watching the new test fail before reverting. The two
`rewrite::http` tests (`a_non_2xx_response_is_a_failure`,
`a_response_with_no_readable_content_is_a_failure`) extend the existing
scratch-listener double with a status-line parameter, no existing call site
changed. 245 tests, 3 more full-suite reruns done here plus the implementer's
own 5+15, all green, no flake.

```yaml
gate:
  stage: S4
  artifact: the diff on feat/36-rewrite-http-command since 7dab991, at fb883c1
  reviewer: two independent reviews plus one round of fixes, verified fresh
  verdict: READY
  date: 2026-09-05
  blocker: null
```

S4 is closed. Next is S5: verify by test suite, then by a real local server —
which, unusually for this project, this session can actually run itself,
since Ollama is already live on this machine.

## Gate S5

```yaml
gate:
  stage: S5
  artifact: docs/evidence.md, "The rewrite stage, against a real local server
    and a real program"
  verdict: pass
  date: 2026-09-05
  platform: macOS 26.6.2, Rust 1.97.1, ureq 2.12.1
```

Fresh run on `feat/36-rewrite-http-command` at `fb883c1`: 245 passed, 0
failed; clippy/fmt/manifest clean.

AC-10 is now genuinely satisfied, not by a reviewer's ad hoc check but by a
run done for this record: Ollama was already running on this machine with
`gemma4:latest` loaded — nothing was started for the occasion. A temporary,
uncommitted test drove the real, compiled `rewrite::http::HttpEngine`
against it and the real, compiled `rewrite::command::CommandEngine` against
a real external program; both restored "мерж реквест"/"пул реквест" to
"merge request"/"pull request" in the same sentence. Both temporary tests
were reverted (`git checkout -- src/rewrite/http.rs src/rewrite/command.rs`)
before this commit — nothing live-server-dependent is part of the permanent
suite, and `git status --short` was empty before writing the entry.

What remains open, stated in the entry itself: the daemon's own take path,
end to end through a live herdr pane on a spoken take, is not this
measurement — the engines were driven directly. That gap is the same class
already open for #21 and #26, and is unaffected by this issue landing.

S5 is closed. Next: the pull request for #36.

## Rebased onto #26's merge

The owner merged issue #26's pull request (#35, commit `8ce1fd8`) while #36's
pull request (#37) was open. Rebased onto the new `origin/main` — two
conflicts, both exactly where `DESIGN_36.md`'s sequencing note said they
would be:

- `docs/decisions.md`: two rows appended near the same place, one per issue.
  Kept both.
- `src/daemon.rs`: `dictate`'s comment above `take_bias`'s call conflicted in
  wording only — #26's merged code already binds `collected` and passes
  `&collected.bias` into `transcribe(runtime, &take, &collected.bias)`,
  exactly the shape this branch independently arrived at; the code itself
  needed no change, only a comment naming both issues instead of one
  written before the other had landed. `transcribe`'s call to
  `engine.transcribe(&take.path, bias)` already carries #26's widened
  signature after the merge — confirmed by reading it after resolving,
  not assumed.

A third conflict, in `docs/evidence.md`, surfaced something worth recording
on its own: this run's own S5 entry had a stray `</new_string>` — a tool
artifact from an earlier edit — committed into the file, unnoticed through
S4 and S5 because no fresh reviewer pass read the evidence prose itself
after it was written. Found and removed while resolving the conflict, along
with keeping both branches' evidence sections. Swept the whole tree for the
same class of artifact afterward (`</new_string>`, `</old_string>`,
`<old_string>`, `<new_string>`) — nothing else found.

Fresh run after rebase, `feat/36-rewrite-http-command` at `dc45b4f`: 249
tests (up from 245 — #26's own additions), 0 failed, three consecutive full
reruns clean; clippy, fmt, manifest all clean.
