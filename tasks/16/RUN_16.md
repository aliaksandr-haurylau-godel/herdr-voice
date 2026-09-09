# RUN_16

| field | value |
|---|---|
| issue | #16 — Recognition: the Whisper-compatible endpoint engine |
| input | GitHub issue, read with `gh issue view 16` |
| stage | S1 |
| branch | feat/16-http-recognition |
| opened | 2026-09-09 |

## Stages

<!-- One block per stage, appended, never rewritten. -->

### S1 Assess
- artifact: `AC_16.md`
- produced: 2026-09-09

The issue names its own key decisions ("which configuration key holds the
address, what its default is, whether an authentication token and a model
name travel with it, what exactly is posted, and where in the answer the
transcript is read from... user-visible names and a wire contract, and they
are the owner's to settle") as reserved for the owner, unlike issue #36's
`[rewrite]` keys, which were provisional and renameable. Asked directly
before writing the acceptance criteria, in this same conversation:

- Endpoint address key: `[stt] url`.
- Model name in the request: a separate key, `[stt] http_model` — not a
  reuse of `[stt] model`, which keeps its existing meaning (a local-model
  identifier for `engine = "command"`).
- Auth token: `[stt] token`, empty default, no header when empty.
- Wire contract: the OpenAI Whisper transcription API —
  `multipart/form-data` with `file`/`model`/`language`, JSON response read
  from `text`.

All four are recorded as settled in `AC_16.md`, not provisional.

```yaml
gate:
  stage: S1
  artifact: AC_16.md
  reviewer: designer
  verdict: null
  date: null
```

```yaml
gate:
  stage: S1
  artifact: AC_16.md
  reviewer: designer
  verdict: READY
  date: 2026-09-09
```

Two corrections applied before closing, both from the reviewer's notes:
requirement 2's rationale for omitting `language` when it is `"auto"`
originally claimed to mirror `command::render`'s treatment of `"auto"`
(`src/stt/command.rs:32`) — that function does the opposite, substituting
`"auto"` literally because `whisper-cli` reads it as "detect it." The rule
itself (omit the field) was already correct in AC-3; only the stated reason
was wrong, corrected to explain that the API's `language` field has no
`"auto"` meaning of its own.

The reviewer also noted the wire contract as written never sends the bias
string `Engine::transcribe`'s second parameter carries (widened by issue
#26), unlike the `command` engine, which already substitutes it into
`{prompt}` (`src/stt/command.rs:37`). Added AC-3a: the bias string travels
as the API's own `prompt` field, sent only when non-empty, matching
`{prompt}`'s existing precedent rather than inventing a new rule.

S1 is closed. Next is S2 Design.

### S2 Design
- artifact: `DESIGN_16.md`
- produced: 2026-09-09

Written directly, gated by the planner reviewer rather than a live chat
approval — this repository's established substitution for
`superpowers:brainstorming`'s approval step in an async run (`tasks/26/
RUN_26.md`, `tasks/36/RUN_36.md`), used here for the same reason: the design
decisions below are engineering choices `CLAUDE.md` already delegates, not
scope or naming decisions (those — the three configuration keys and the wire
contract — were already settled directly with the owner at S1).

One real simplification found while writing this: `doctor` needs no change.
`engine_finding_from` (`src/doctor.rs:199-215`) already delegates to
`stt::resolve_with` and reports `Ok`/`Missing` generically, unlike
`rewrite_finding`, which had to be hand-written because `rewrite::Resolution`
carries a third state a plain `Result` cannot express. AC-9 is satisfied by
`resolve_with`'s own fix alone.

One deliberate departure from `rewrite::http::HttpError`'s two-variant shape:
this engine's error type needs three variants, not two, since AC-5 asks for
a connection failure and a non-2xx response to be told apart by message,
which `rewrite::http`'s `Failed` variant collapses into one case.

```yaml
gate:
  stage: S2
  artifact: DESIGN_16.md
  reviewer: planner
  verdict: null
  date: null
```

```yaml
gate:
  stage: S2
  artifact: DESIGN_16.md
  reviewer: planner
  verdict: QUESTIONS
  date: 2026-09-09
  questions:
    - "Reusing EngineError::NotConfigured as-is for the http engine's empty
       url reaches doctor with a message naming [stt] command, not [stt]
       url — failing AC-2 and AC-6, and the design did not say how a message
       becomes engine-specific while keeping one variant."
  blocker: null
```

### Answered

`NotConfigured` gains a payload — `{ engine, key, example }` — rather than a
second variant. Both existing construction sites (`src/stt.rs:121`,
`src/stt/command.rs:126`) updated to pass the `"command"` case explicitly;
the rendered text for that case is unchanged, so the existing test
(`a_command_engine_with_nothing_to_run_names_the_key_and_shows_one`,
`src/stt.rs:234`) keeps its assertion, only through the new shape — the plan
names updating it rather than assuming it. The `"http"` case passes its own
engine/key/example. Section 4 of `DESIGN_16.md` now states this in full.

S2 is closed. Next is S3 Plan.

```yaml
gate:
  stage: S2
  artifact: DESIGN_16.md
  reviewer: planner
  verdict: QUESTIONS
  date: 2026-09-09
  questions:
    - "HttpEngine had no field and resolve_with's constructor call had no
       parameter for [stt] language, so AC-3's language field could never
       reach a request — Engine::transcribe itself carries only audio and
       bias, so the language has to arrive at construction, and nothing
       did."
  blocker: null
```

Confirmed from the same pass: the `NotConfigured` payload fix does close
AC-2 and AC-6, and the command case's existing test survives unchanged in
substance. One thing named but not treated as a question: the `EXAMPLE`
rename touches three sites, not two — the `NotBuilt` arm also reads the same
constant (`src/stt.rs:51`); a plain rename, caught by the compiler, added to
the design text for completeness.

### Answered

`HttpEngine` gains a `language: String` field, `HttpEngine::new` a fourth
parameter, and `resolve_with`'s construction passes `stt.language.clone()`
alongside `url`/`token`/`http_model` — the same way `model` and `token`
already do, since the trait method itself takes only `audio` and `bias`.

S2 continues; re-running the gate against the revised artifact.

```yaml
gate:
  stage: S2
  artifact: DESIGN_16.md
  reviewer: planner
  verdict: READY
  date: 2026-09-09
  questions: []
  blocker: null
```

Third read, hunting specifically for anything the two prior fixes left
inconsistent: struct field order, constructor signature and every call site
agree; the `NotConfigured` payload is the same shape everywhere it appears.

Three small stale spots noted for the plan, none reopening the gate — each a
determined consequence of AC-1, not a design question:

- `the_unbuilt_engines_say_so_and_name_the_one_that_works` (`src/stt.rs:204`)
  loops over `[("candle", "#15"), ("http", "#16")]`; once `"http"` stops
  returning `NotBuilt`, its half of that loop needs removing.
- The coverage table's AC-2 row said "reused" before the payload existed —
  corrected above.
- `src/config.rs:56`'s doc comment ("candle, http or command. The first two
  are not built yet.") is stale once this issue lands; `[stt] engine`'s
  comment needs updating to name only `candle` as unbuilt.

S2 is closed after three rounds. Next is S3 Plan.

### S3 Plan
- artifact: `PLAN_16.md`
- produced: 2026-09-09

Six tasks: the `NotConfigured` payload refactor (1), `[stt]`'s three new
keys (2), `stt::http` itself (3), wiring plus cleanup of the two stale spots
S2's third pass found (4), S4 review (5), S5 verify (6). Smaller in scope
than issues #21, #26 or #36 — this engine reuses `ureq` and mirrors
`rewrite::http`'s shape directly rather than introducing new infrastructure.

Self-review found two things before this went to gate: a redundant
`super::HttpError::` qualification in three test assertions (`use super::*`
already brings `HttpError` into scope) and a missing note that `src/stt/
http.rs` needs the same `use super::{Engine, EngineError};` import
`src/stt/command.rs:11` already has — both fixed.

```yaml
gate:
  stage: S3
  artifact: PLAN_16.md
  reviewer: implementer
  verdict: null
  date: null
```

```yaml
gate:
  stage: S3
  artifact: PLAN_16.md
  reviewer: implementer
  verdict: QUESTIONS
  date: 2026-09-09
  questions:
    - "No task ever adds EngineError::Http(http::HttpError) to src/stt.rs's
       EngineError enum, even though Task 3's own tests construct it and
       DESIGN_16.md §1 requires it — Task 3 cannot compile without a
       variant no step adds."
    - "Task 4 Step 7's 'all four green' is unreachable as written:
       src/doctor.rs:569 asserts finding.detail.contains(\"#16\") for the
       http engine, text only NotBuilt produces — once resolve_with's
       http arm stops returning NotBuilt, this pre-existing test fails,
       and no task lists src/doctor.rs as a file to modify."
  blocker: null
```

### Answered

Task 3 now adds the `Http` variant and its `Display` arm as its own first
sub-step, before the module it depends on is even written, with the reason
stated (the module's own tests cannot compile without it). Task 4 gains a
new step and a listed file (`src/doctor.rs:561-569`) that replaces the
`"#16"` assertion with one matching what `NotConfigured` now says
(`finding.detail.contains("url")`) — a real fix, not an optional cleanup.
`DESIGN_16.md` §6 corrected to distinguish "no change to `doctor.rs`'s
production code" from "one existing test needs updating," which is what the
gate's second question actually found.

S3 continues; re-running the gate against the revised artifact.

```yaml
gate:
  stage: S3
  artifact: PLAN_16.md
  reviewer: implementer
  verdict: QUESTIONS
  date: 2026-09-09
  questions:
    - "Task 3's steps disagreed on when the module joins the crate: the test
       file was to be written before pub mod http; existed, so the stated
       'FAIL to compile' at the verify-it-fails step would not actually
       happen — an undeclared module is invisible to rustc, so cargo test
       would report zero tests collected, not a compile failure."
  blocker: null
```

### Answered

Task 3 restructured into seven steps instead of six: a new Step 1 scaffolds
`src/stt/http.rs` with an empty `HttpError` enum and declares `pub mod
http;` plus the `EngineError::Http` variant, confirmed to compile on its own
first; Step 2 then writes the full test module, which now genuinely fails
to compile (Step 3) since `HttpEngine` does not exist and the scaffolded
`HttpError` has no variants for the tests to construct. Step 4 replaces the
scaffold with the real implementation. Steps 5-7 (verify pass, whole suite,
commit) follow unchanged in substance, renumbered. The stale Coverage-table
parenthetical about AC-9 ("no code change needed") was also corrected to
match Task 4 Step 6's actual content.

S3 continues; re-running the gate against the revised artifact.

```yaml
gate:
  stage: S3
  artifact: PLAN_16.md
  reviewer: implementer
  verdict: READY
  date: 2026-09-09
  questions: []
  blocker: null
```

Fourth read, hunting specifically for anything the three prior fixes left
inconsistent: Task 3's seven-step restructuring is internally consistent —
the empty-enum scaffold compiles legitimately (`match *self {}` on an
uninhabited enum), Step 2's test module genuinely fails against that
scaffold, and no stray reference to a pre-renumbering step count survives
anywhere else in the plan. A few file line ranges have drifted by one or
two lines against the current file but land on the right symbol every
time; the one line that must be exact (`src/stt.rs:121`, the empty-argv
`NotConfigured` site) matches exactly.

S3 is closed after three rounds. Next is S4 Implement.

## Gate S4

```yaml
gate:
  stage: S4
  artifact: the diff on feat/16-http-recognition since 851821c
  reviewer: one review, mutation-testing every field-presence rule and error variant
  verdict: QUESTIONS
  date: 2026-09-09
  blocker: null
```

517 insertions across `src/stt.rs`, `src/stt/http.rs` (new), `src/stt/
command.rs`, `src/config.rs`, `src/doctor.rs`. All four gates green, 264
tests. Seventeen mutations run; fifteen caught. No correctness defect —
both misses are coverage gaps in already-correct code, confirmed by direct
byte-level inspection of the shipped multipart body and by checking the
real `status` code is genuinely carried through, not hardcoded.

Clean: the `NotConfigured` refactor (both engines name themselves
correctly); all eight field-presence mutations across `model`/`language`/
`prompt`/`Authorization` (`language`'s "omit when auto" rule confirmed as
the deliberate opposite of `command::render`'s literal substitution); no
leak of the transcript, bias, audio bytes or request/response bodies into
any error message; no panic path, including a missing WAV file returning
`Err`; no trace of Task 3's empty-enum scaffold; `doctor`'s test fix is a
real, meaningful assertion; no absolute path or private name anywhere.

### Sent back for fixing

1. **`HttpError::Failed`'s `status` field can be hardcoded to `500` and
   nothing catches it.** `a_non_2xx_response_is_a_failure_naming_the_status`
   is the only test exercising this variant, and its fixture happens to use
   status 500 — mutating `status: code` to `status: 500` leaves every test
   green. Add a second case with a different status (e.g. 503) to the same
   test or a sibling one, so the real code path (carrying `code` through,
   not a fixed value) is what's actually proven.
2. **The multipart body's tests are substring checks only** (`request.
   contains("name=\"model\"")` and similar) and would not catch a
   malformed part — a missing blank line between a part's headers and its
   content, or a `\n` where `\r\n` belongs. Both mutations left all twelve
   tests green. Add at least one test that inspects the captured request's
   exact structure closely enough to catch these two classes (a full-body
   equality check against an expected byte string for one representative
   request is the most direct way, given the boundary is a fixed constant).
3. **The WAV-read-failure path has no dedicated test.** `std::fs::read`
   failing (a missing file) is mapped to `HttpError::Refused` without a
   panic — verified by the reviewer with a scratch test, but no equivalent
   ships in the diff. Add one.
4. **`every_key_has_a_default` doesn't assert the three new `Stt` fields.**
   Its name promises checking every key against `Config::default()`; add
   `url`/`token`/`http_model` to it rather than relying only on the
   separate partial-TOML test to cover the same ground.

None of the four is a design or scope question — all are additional tests
against code already confirmed correct by direct inspection.

All four fixed and verified fresh. `HttpError::Failed`'s status now proven
with a second case (503) alongside the original 500 fixture. The multipart
body has a byte-exact comparison test alongside the substring-based ones,
catching both the missing-blank-line and the wrong-line-ending mutations.
The missing-WAV-file path has a dedicated test, confirmed to catch a
`.unwrap()` regression by inducing a real panic and reverting. `every_key_
has_a_default` now checks `url`/`token`/`http_model` against
`Config::default()` directly. 267 tests, six consecutive full reruns here
and by the implementer, all green.

```yaml
gate:
  stage: S4
  artifact: the diff on feat/16-http-recognition since 851821c, at 9b5e0ed
  reviewer: one review plus one round of fixes, verified fresh
  verdict: READY
  date: 2026-09-09
  blocker: null
```

S4 is closed. Next is S5: verify by test suite, then by a real endpoint if
one is reachable.

## Gate S5

```yaml
gate:
  stage: S5
  artifact: docs/evidence.md, "The http recognition engine, against a real
    whisper.cpp server"
  verdict: pass
  date: 2026-09-09
  platform: macOS 26.6.2, Rust 1.97.1, ureq 2.12.1
```

Fresh run on `feat/16-http-recognition` at `9b5e0ed`: 267 passed, 0 failed;
clippy/fmt/manifest clean.

A live server was already running on this machine for an unrelated purpose
(OpenWhispr's `whisper-server-darwin-arm64`, port 8178, `/inference`).
A temporary, uncommitted test drove the real, compiled
`stt::http::HttpEngine` against it: a synthesized 16 kHz mono WAV, "Please
open the pull request and merge it," came back transcribed exactly. Caught
a second, real fact worth recording while checking: this server speaks
whisper.cpp's own `/inference` route, not a path named `/v1/audio/
transcriptions` — the engine's own `[stt] url` is fully configurable, so
this needed no code change, only the right address, and it is noted in the
evidence entry so the same distinction is not rediscovered later. The
temporary test was reverted (`git checkout -- src/stt/http.rs`) before this
commit; nothing live-server-dependent is part of the permanent suite.

A stray tool-artifact string (`</new_string>`) was caught and removed from
the evidence entry before this commit — the same class of mistake this run
made once before while writing issue #36's evidence entry
(`tasks/36/RUN_36.md`, "Rebased onto #26's merge"); swept the file
afterward and found nothing else.

S5 is closed. Next: the pull request for #16.
