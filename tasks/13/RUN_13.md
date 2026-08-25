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

```yaml
gate:
  stage: S1
  artifact: AC_13.md
  reviewer: designer
  verdict: QUESTIONS
  date: 2026-08-25
  questions:
    - "AC-3 and AC-12: how do the take's path, the language and the model reach the external program's argument list, and under which configuration keys? `whisper-cli` takes the file behind `-f` and needs `-m` and `-l` too, and `[stt]` has no key for a program or its arguments. A fixed 'append the path' contract and a configured argument template are two different engines with two different schemas."
    - "AC-9: is `[stt] model` the file name or an identifier mapped to one? The default is `large-v3-turbo` and the file on disk is `ggml-large-v3-turbo.bin`, so under an exact-name rule the default can never be present."
    - "AC-10: what is an integrity check compared against? Nothing holds an expected size or digest, and chosen reading 3 removes the download that would carry one."
    - "AC-8 refuses a name that is 'none of the four' while everything else names three. Which is the fourth?"
  blocker: null
  notes:
    - "Separate note, not a reason to withhold READY: the scope cut to `command` and `http` is stated, reasoned and flagged for the owner, and I can design against it. The as-is claims about `src/config.rs` and `src/doctor.rs` check out against the code on the branch."
```

### S1 Assess — answers to the gate
- artifact: `AC_13.md`, a new section and five criteria
- produced: 2026-08-25

All four are answered in the artifact, and one of them found a plain error.

The external program takes a configured argument list with `{audio}`, `{model}`
and `{language}` placeholders, under a new key `[stt] command`. That key is a
user-visible name that `docs/design.md` section 7 does not contain, so it is in
`docs/decisions.md` as the owner's to confirm, alongside the scope cut.

`[stt] model` is an identifier and the file is `ggml-<model>.bin`. The reviewer
caught that an exact-name rule against the identifier could never match, since the
default is `large-v3-turbo` and the file is `ggml-large-v3-turbo.bin`.

Integrity is the exact name, a plausible size, and the four leading bytes
`6C 6D 67 67` — the `ggml` magic, read off the model on disk rather than assumed.
That catches a truncated download without a digest nobody has yet; a `.sha256`
sidecar is checked when present, and #15 writes it.

There is no fourth engine. The criterion said four and was wrong.

A revised artifact needs a new verdict; the gate runs again.

## Notes

The prototype passed a bias prompt to `whisper-cli`. That prompt is context, and
context is its own task, so nothing here passes one — which `docs/evidence.md` says
costs punctuation and capitalisation but not terms.
