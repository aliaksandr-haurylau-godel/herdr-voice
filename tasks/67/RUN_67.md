# RUN_67

| field | value |
|---|---|
| issue | #67 — Cut the first release, so the plugin can actually be installed |
| input | GitHub issue, read with `gh issue view 67` |
| stage | S4 |
| branch | docs/67-readme-installable |
| opened | 2026-09-30 |

This run closes one part of #67 and does not close the issue. The pull request
references #67 without closing it: the container install run and the measurement of
the window in which GitHub answers 404 after a release is published stay open there.

## Stages

<!-- One block per stage, appended, never rewritten. -->

### S1 to S3 skipped

This is infrastructure whose shape is fixed: the README's Install section says how
to install what is published now. The installer is `scripts/install.sh` and
`scripts/install.ps1`, described by `docs/design.md` section 8; the releases are the
four prereleases `v0.1.0-beta.1` to `v0.1.0-beta.4`, listed by `gh release list`.
Nothing changes behaviour, so per `CLAUDE.md` S1 to S3 are skipped. S4 and S5 apply.

What the section says, and where each statement comes from:

| Statement | Source |
|---|---|
| four prereleases, `v0.1.0-beta.1` to `v0.1.0-beta.4`, newest `v0.1.0-beta.4` | `gh release list`, 2026-09-30 |
| installing a prerelease means naming it, with `--ref` | the notes of `v0.1.0-beta.4` (`gh release view v0.1.0-beta.4`) and `scripts/release-notes.sh:85-88` |
| the install line | the same line as in the release notes, character for character |
| fetches the archive, checks the `.sha256`, unpacks, compiles nothing | `docs/design.md` section 8; `scripts/install.sh` |
| archives for macOS arm64 and x86_64, Linux x86_64 and arm64, Windows x86_64 | the ten assets of `v0.1.0-beta.4`; `scripts/install.sh` `hv_target` |
| Linux archives are built against glibc, so not Alpine | `scripts/install-check.sh` header |
| any other platform compiles from source and needs a Rust toolchain; an unreachable archive stops the install | `scripts/install.sh` `hv_fallback` and `hv_main` |

`docs/evidence.md` records that what herdr does when no `--ref` is given was not
established, so the section does not say that an install without it works.

### S4 Implement
- artifact: `README.md`, the `## Install` section only

Sections changed: `## Install`, and, at the orchestrator's request because they
contradicted it, the `Status` note at the top and `## Platforms`. Every platform
statement names the `docs/evidence.md` section it rests on.

First review, by a fresh reviewer who did not write the change: `QUESTIONS`, no
blockers. Answered:

- The README described the source build as happening only on an unsupported platform.
  The script also builds from source when the archive answers 404 after the retries
  (`scripts/install.sh`, `hv_main`, the `missing` branch). The section now says so, and
  says that any other failure to reach the archive stops the install.
- Most rows of the `docs/evidence.md` table had no command beside them. The table now
  lists every command that was run, with its result.
- The evidence text named a `herdr --session` flag that was never tried. Removed.
- The `RUN_67.md` sentence explaining a choice was removed; the fact beside it stays.

Its remaining notes, not acted on: `docs/design.md` section 8 shows the install line
without `--ref`; whether the plain form works is not established, and that file is
outside this change.

A second fresh reviewer then read the `Status` and `Platforms` text against
`docs/evidence.md`; its verdict is recorded below.

No mutation test applies: the diff contains no code.

### S5 Verify

Run on macOS arm64. The result is the section "The README's install instructions,
checked against the published release, for issue #67" in `docs/evidence.md`, with each
command and its result. `herdr plugin install` itself was not run: no container runtime
exists on this machine and no isolated herdr was available. The orchestrator agreed that
the pull request goes up with that limit stated, and that #67 keeps the container check,
the unpinned-install question and the measurement of the 404 window.

Second review, of the `Status` note, `## Platforms` and the source-build sentences
against `docs/evidence.md` and the install scripts: `QUESTIONS`, no blockers. Answered:

- The Linux bullet read as if herdr had built and linked the plugin. In the recorded
  run the build and the link were commands of the check script and only the daemon was
  started by herdr; the run was on an early revision. The bullet now says exactly that.
- The Windows bullet now says that fetching a release archive on Windows is not
  recorded.
- The source build also needs the ALSA development headers on Linux; the Install text
  now says so.

Everything else in those three parts, including the five macOS section names, the
anchors and the absence of any production-ready wording, was found supported.

Gates run fresh in this worktree before the commit: `cargo test` (571 passed, 0
failed, 1 ignored, plus the integration test), `cargo clippy --all-targets -- -D
warnings`, `cargo fmt --check` and `python3 scripts/check_manifest.py`, all green. The
Windows dead-code check was not run: the diff changes no Rust file.
