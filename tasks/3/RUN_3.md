# RUN_3

| field | value |
|---|---|
| issue | #3 — Skeleton: manifest, daemon, socket client, doctor |
| input | GitHub issue, read with `gh issue view 3` |
| stage | S1 |
| branch | feat/3-daemon-client-doctor |
| opened | 2026-08-24 |

## Stages

<!-- One block per stage, appended, never rewritten. -->

### S1 Assess
- artifact: `AC_3.md`
- produced: 2026-08-24

Written against the repository as it was before `#5`: no crate, no manifest, no
CI. Manifest format, the invocation-context field names and the herdr version were
read from the installed herdr 0.8.2 and from an installed third-party plugin
rather than assumed. Three ambiguous points were resolved narrowly and recorded in
the artifact.

```yaml
gate:
  stage: S1
  artifact: AC_3.md
  reviewer: designer
  verdict: READY
  date: 2026-08-24
  questions: []
  blocker: null
  notes:
    - "Not a question, no answer needed: AC-10 requires `doctor` to report whether a transcription model is present while recognition is out of bounds, and neither the issue nor `docs/design.md` fixes where a model lives. I will define the model-store location and the presence test in the design and mark it as a contract the transcription stage inherits."
    - "Not a question: AC-3 and AC-6 name `herdr plugin list --json`, `herdr plugin link` and `herdr plugin action invoke cancel`; the as-is section verified only that the subcommands `link`, `enable`, `list` and `action` exist. I will confirm exact invocation and whether `link` leaves the plugin enabled while designing, and add `enable` to the install path if it does not."
```

### S1 Assess — revised
- artifact: `AC_3.md`, rewritten
- produced: 2026-08-24

`#5` landed the crate, the manifest, the workflows and the run scaffolding while
this run sat at the gate, so roughly half of what the first version of the
acceptance criteria required was already merged. The artifact was rewritten to
cover the remainder only: the daemon as a long-lived process, the client that
reaches it over a Unix socket on macOS and Linux and a named pipe on Windows, and
`doctor`. The as-is section now rests on the merged code, with line references
into `src/main.rs`, `herdr-plugin.toml`, `scripts/check_manifest.py` and the two
workflows.

Both notes the designer left on the first version survive into the rewrite: the
model location is still undecided and named as a design obligation, and the exact
`herdr plugin` invocations are no longer asserted as acceptance criteria.

The rewrite also removed absolute home paths from the citations, which the leak
gate rejects.

A revised artifact needs a new verdict; the gate runs again.

```yaml
gate:
  stage: S1
  artifact: AC_3.md
  reviewer: designer
  verdict: READY
  date: 2026-08-24
  questions: []
  blocker: null
  notes:
    - "As-is section checked against main and holds: `Cargo.toml:15` has an empty `[dependencies]`; every subcommand falls into the single `NOT_IMPLEMENTED = 69` arm (`src/main.rs:109`, `src/main.rs:118`); `herdr-plugin.toml` declares actions `dictate`, `ptt`, `cancel`, `setup` and panes `status`, `model`, `mic` with no `doctor` entry while `parse` accepts `doctor` (`src/main.rs:73`); `scripts/check_manifest.py:50` reads known subcommands with `re.findall(r'Some\(\"([a-z-]+)\"\)', source)`, so the manifest-to-binary coupling described is real; the five checks are the three-OS matrix (`.github/workflows/check.yml:19`), the manifest job (`.github/workflows/check.yml:33`) and the leak gate (`.github/workflows/leak-gate.yml:9`). Nothing in the as-is section is asserted beyond what the merged code shows."
    - "Not a question, no answer needed: the previous verdict's model-location note survives unchanged, and the artifact now states the obligation itself under 'Out of scope / noticed'. I will fix the model-store location and the presence test in the design and mark them as a contract the transcription stage inherits."
    - "Not a question: `RUN_3.md:53` says the exact `herdr plugin` invocations are no longer asserted as acceptance criteria, but AC-5 still names `herdr plugin action invoke cancel`. I read AC-5 as requiring that an action invocation reach the daemon, with the exact command line a verification detail I will confirm while designing. No revision needed for me to proceed."
    - "Not a question: AC-3 fixes the socket path derivation only for the Unix side, while R5 and AC-12 require a named-pipe implementation on Windows. The pipe-name derivation, the wire format of the request, the shutdown signal for AC-1, how `doctor` locates the herdr binary when `HERDR_BIN_PATH` is absent, and where the daemon records a received request for AC-5 are all mine to decide — both ends of the transport are this binary, so no external contract is missing."
    - "Separate note on scope, not a reason to withhold READY: chosen reading 4 gives behaviour only to `cancel`, and with no pipeline there is nothing for `cancel` to cancel, so AC-5 reduces to a round-trip proof. That matches the issue's 'proves the shape' goal; I mention it only so the plan does not later read AC-5 as real cancellation semantics."
```

## Notes

The pipeline is untouched. Every action except `cancel` keeps exiting 69 with
`"not implemented yet"`, and `scripts/check_manifest.py` keeps that honest: a
subcommand is callable from the manifest only after it appears in `parse`
(`src/main.rs:68`).
