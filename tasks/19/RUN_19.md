# RUN_19

| field | value |
|---|---|
| issue | #19 — A reply with a newline in it arrives truncated, silently |
| input | GitHub issue, read with `gh issue view 19` |
| stage | S1 |
| branch | fix/19-multiline-reply |
| opened | 2026-09-30 |

## Stages

<!-- One block per stage, appended, never rewritten. -->

### S1 Assess
- artifact: `AC_19.md`
- produced: 2026-09-30

Input came from GitHub: `gh issue view 19 --json ...` (issue #19, open, no
comments; the plain `--comments` form printed nothing because there are none).
The per-issue triage report (`## #19`) was checked against the code: the line
numbers hold, but its claim about the success format does not — the success reply
on `main` does not contain the transcript (see `AC_19.md`, as-is). `octoflow-assess`
ran as the skill states; the assessment was made from reading the code, not from
running it.

```yaml
gate:
  stage: S1
  artifact: AC_19.md
  reviewer: designer
  verdict: READY
  date: 2026-09-30
  blocker: null
```

Reviewer's note, not a gate question: AC-7 refers to "the Windows dead-code
check of the brief", and the run holds no brief. The plan (S3) will spell the
command out in full instead of editing `AC_19.md` after the verdict. The reviewer
had no `gh` access and checked the criteria against the triage report, not the
issue body; the issue body was read by the author with `gh issue view 19 --json`.

### S2 Design
- artifact: `DESIGN_19.md` (evidence for its claims in `DESIGN_19.evidence.md`)
- produced: 2026-09-30

The choice between carrying a multi-line reply whole and refusing to send one was
settled in the design: carried whole, in a frame that announces its length. The
orchestrator asked for it to be settled in S2 with the reasoning, so it was not
put to the owner.

```yaml
gate:
  stage: S2
  artifact: DESIGN_19.md
  reviewer: planner
  verdict: READY
  date: 2026-09-30
  blocker: null
```

Reviewer's note, not a gate question: the command of the Windows dead-code check
named in AC-7 is not spelled out in the design; the plan (S3) spells it out.

### S3 Plan
- artifact: `PLAN_19.md`
- produced: 2026-09-30

```yaml
gate:
  stage: S3
  artifact: PLAN_19.md
  reviewer: implementer
  verdict: READY
  date: 2026-09-30
  blocker: null
```

Reviewer's remarks, neither a gate question: the interface list of Task 2 names
`takes_dir(tag)`, which no Task 2 test uses; and a formatting fix found in Task 3
is committed as its own commit, because Tasks 1 and 2 are already committed by
then.

### S4 Implement
- artifact: code in `src/proto.rs`, `src/client.rs` (comment only) and tests in `src/daemon.rs`
- produced: 2026-09-30

Commits: `8664cd8` (framing, unit and socket tests, run artifacts), `da1299a`
(tests through the real client and the connection handler), then a separate
formatting commit.

Deviations from `PLAN_19.md`, none of them changing what it asks for:

- The plan gave `cargo test proto::tests client::tests`; `cargo test` takes one
  filter before `--`, and the package is a binary (no `--lib`). The command run was
  `cargo test -- proto::tests client::tests`. The plan text was corrected.
- The plan's marker grep held the literal editing-tool tags, and the pre-commit
  hook refused the commit for that ("editing debris"). The grep was rewritten with
  a regular expression that does not contain them. The hook was not bypassed.
- The red step of Task 1 was a failed build (`ProtoError::NotText` did not exist),
  not a failed assertion. Task 2 gave the behavioural red: against `src/proto.rs`
  from `main`, `ac3_...` ended at `...For example:` and `ac4_...` at
  `...model not found`; `ac2_...` passed, as the plan predicted, because the success
  reply on `main` does not carry the transcript.
- `cargo fmt --check` found three places in the added tests; `cargo fmt` changed only
  those, and they were committed on their own.

Gates, run fresh after the last commit (macOS, this machine):

```
cargo test                                   584 passed, 0 failed, 1 ignored; 2 passed (integration)
cargo clippy --all-targets -- -D warnings    no warning
cargo fmt --check                            fmt-ok
python3 scripts/check_manifest.py            manifest: 12 entries, all commands known
Windows dead-code check (brief)              clippy no warning; src restored, git status clean
```

No test failed intermittently, so no rerun was needed.

## Notes

<!-- Anything a later stage needs and the artifacts do not carry. -->
