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
