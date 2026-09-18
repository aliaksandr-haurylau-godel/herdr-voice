# RUN 74 — prerelease notes describe the release they are attached to

Input: GitHub issue #74, read with `gh issue view 74`. The issue body and its
comments are the ticket.

Branch: `fix/74-release-notes`, cut from `main` at `acbe320`.

## Stages

S1 to S3 skipped. This is an infrastructure run: the issue fixes the shape —
which text is published, for which tags, and what it has to say — and nothing
the change touches alters what the plugin does. Only the release workflow and
two shell scripts change; no Rust code, no manifest, no command surface.

S4 and S5 are not skipped.

### S4 Implement

`scripts/release-notes.sh` is new and writes the notes for a prerelease from the
tag. `.github/workflows/release.yml` calls it and publishes the result with
`--notes-file` instead of the fixed `--notes` string, and titles the release
with the tag alone rather than "— install-path check".

`v0.0.0` keeps the old text. It is the one prerelease of which that text is
true: `scripts/release-kind.sh` answers `prerelease` for it because it is the
repository's placeholder version, not because of the hyphen rule, and a tag cut
from it really is thrown away after the install path has been exercised.

`scripts/test-release-notes.sh` is new and drives the notes script; the
`scripts` job in `.github/workflows/check.yml` runs it beside the release-kind
tests.

Gates before the commit, all green: `cargo test` (491 + 2 passed),
`cargo clippy --all-targets -- -D warnings`, `cargo fmt --check`,
`python3 scripts/check_manifest.py`.

Diff review: recorded below when it closes.

### S5 Verify

Recorded in `docs/evidence.md`.

The already-published `v0.1.0-beta.1` is corrected in place with
`gh release edit`, notes only — not the tag, not the assets, not the prerelease
flag. Nothing in this run pushes a tag, cuts a release or re-runs the release
workflow.
