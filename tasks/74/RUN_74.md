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
from it really is thrown away after the install path has been exercised. No such
tag exists — `git ls-remote --tags origin` lists only `v0.1.0-beta.1` — so this
branch costs a line against the version returning to 0.0.0, and nothing in the
notes or the tests says that tag is published.

`scripts/test-release-notes.sh` is new and drives the notes script; the
`scripts` job in `.github/workflows/check.yml` runs it beside the release-kind
tests.

`scripts/release-notes.sh` refuses a tag `scripts/release-kind.sh` does not call
a prerelease, and a tag whose name carries a character a version does not. The
first keeps it from writing "the tag carries semver's prerelease marker" over a
tag that has none; the second keeps a tag name out of a heredoc that expands it.

Gates before the commit, all green: `cargo test` (491 + 2 passed),
`cargo clippy --all-targets -- -D warnings`, `cargo fmt --check`,
`python3 scripts/check_manifest.py`.

### The diff review that closes S4

Run against `acbe320..f076b54` before the pull request. Verdict: ready with
fixes. What it found and what was done:

- The notes said "Without `--ref`, herdr installs from the default branch". The
  reviewer could not find that in `herdr plugin install --help` or anywhere in
  the repository. The sentence now says only what is established: `--ref` pins
  the install to this tag, and the install reads the manifest at the ref it is
  given.
- "Before you rely on it" pointed at `docs/evidence.md` for which platforms the
  install path had been exercised on, and that file records nothing about the
  install path at this tag. The sentence now points at what the file does hold —
  what was verified by hand and on which platform.

  Both sentences were removed for being unestablished, not for being wrong.
  Neither was shown to be false; there was nothing in the repository, in
  `herdr plugin install --help` or in a run that made them true, and a published
  note is the one place where a reader can check a claim and find it empty. That
  is why the notes are shorter than they could be, and it is the bar for
  anything added to them later.
- The script accepted a tag with no prerelease marker and produced a sentence
  claiming one, and it expanded the tag inside a heredoc. Both are refused now,
  with tests.
- Nothing in CI asserted that the workflow calls the script. Three assertions
  now read `release.yml` directly.
- The run document claimed the published release had already been corrected
  while it had not. It says below what actually happened and when.

### S5 Verify

Recorded in `docs/evidence.md`, section "The prerelease notes, driven as the
workflow drives them".

### The published release

`v0.1.0-beta.1` was corrected on 2026-09-18, after the owner approved the exact
text in this session. `gh release edit` with the title and the notes file and
nothing else: the tag, the ten assets and the prerelease flag were not arguments
to it. Read back from GitHub afterwards rather than trusted to the exit code; the
result is in `docs/evidence.md`, section "The notes of `v0.1.0-beta.1`, corrected
in place".

That approval covered this release's notes and nothing further.

Nothing in this run pushes a tag, cuts a release or re-runs the release workflow.
