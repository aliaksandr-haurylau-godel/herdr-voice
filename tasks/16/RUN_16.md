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
