# RUN_13

| field | value |
|---|---|
| issue | #13 — Recognition: transcribe a take with candle, and let the person choose the model |
| input | GitHub issue, read with `gh issue view 13` |
| stage | S1 |
| branch | feat/13-recognition |
| opened | 2026-08-25 |

## Stages

<!-- One block per stage, appended, never rewritten. -->

### S1 Assess
- artifact: `AC_13.md`
- produced: 2026-08-25

Two facts were checked on the machine rather than assumed, and they decided the
shape of the work: `whisper-cli` is installed, and the model the prototype used —
`ggml-large-v3-turbo.bin`, 1.5 GB — has been on disk since April. An engine that
runs an external command therefore reaches a working chain tonight without a
download and without inference code of ours.

The issue is rescoped to the two thin engines plus the model contract. The built-in
`candle` engine becomes issue #15. This is a **scope cut that touches a request the
owner made himself** — a plugin that needs nothing else installed — so it is
flagged in `docs/decisions.md` for his confirmation rather than recorded as settled.
Two things follow from it and are in the criteria: no release is tagged while the
built-in engine is missing, and the default stays `candle`, which reports that it is
not built rather than quietly becoming a different engine.

## Notes

The prototype passed a bias prompt to `whisper-cli`. That prompt is context, and
context is its own task, so nothing here passes one — which `docs/evidence.md` says
costs punctuation and capitalisation but not terms.
