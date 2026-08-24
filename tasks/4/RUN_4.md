# RUN_4

| field | value |
|---|---|
| issue | #4 — Complete the development setup: crate skeleton, manifest, CI, release |
| input | GitHub issue, read with `gh issue view 4` |
| stage | S4 |
| branch | feat/4-dev-setup |
| opened | 2026-08-24 |

## Stages

<!-- One block per stage, appended, never rewritten. -->

### S1–S3

Skipped deliberately. This run lands infrastructure whose shape is already fixed
by `docs/design.md` sections 2 and 8; there is nothing for acceptance criteria, a
design or a plan to decide. The exemption applies to infrastructure only — a run
that changes behaviour goes through every stage.

### S4 Implement
- artifact: crate skeleton, plugin manifest, check and release workflows, run scaffolding
- produced: 2026-08-24
- verification: `cargo fmt --check`, `cargo clippy -D warnings`, `cargo test`,
  `python3 scripts/check_manifest.py` all pass locally; CI runs them on macOS,
  Linux and Windows.

## Notes

The manifest check earned its place immediately: it caught the manifest naming
four subcommands the binary did not accept. Left alone, that ships as a plugin
that installs and then does nothing when an action is invoked.
