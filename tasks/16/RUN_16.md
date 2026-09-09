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
