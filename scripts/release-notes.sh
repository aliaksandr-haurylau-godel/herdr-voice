#!/bin/sh
#
# Writes the notes .github/workflows/release.yml publishes a prerelease with.
#
# Only prereleases need this. A full release is published with
# `gh release create --generate-notes`, and GitHub writes that text itself.
#
# It exists as a script rather than as a `--notes` string inside the workflow for
# two reasons. The text is the only thing a reader meets before the archives, and
# a string inlined in a workflow is only ever read when someone opens the
# workflow; and the tag has to appear inside the notes, so the text is built
# rather than fixed, and anything built can be wrong. Both are testable here and
# neither is testable in a `run:` block.
#
# Usage: sh scripts/release-notes.sh v1.2.3-rc.1  ->  prints the notes

set -eu

tag="${1:?usage: release-notes.sh <tag>}"
# The archives and the manifest carry the version without the leading `v`, which
# is what a reader compares against `herdr plugin list`.
version="${tag#v}"
repo="aliaksandr-haurylau-godel/herdr-voice"

# v0.0.0 is the repository's placeholder version, and `scripts/release-kind.sh`
# answers `prerelease` for it on that ground rather than on the hyphen rule. A
# tag cut from it is not a version of the plugin and is thrown away once it has
# been used, so it is the one prerelease the text below would be false of.
if [ "${tag}" = v0.0.0 ]; then
    cat <<'PLACEHOLDER'
Not a version of the plugin. The manifest and the crate both sit at 0.0.0, which
is this repository's placeholder for "unreleased".

This tag exists so the install path can be exercised end to end: the manifest's
build entries fetch the archives below and check each against the `.sha256` file
published beside it. It is deleted once the result is recorded in
`docs/evidence.md`. Nothing here is meant to be installed for use.
PLACEHOLDER
    exit 0
fi

cat <<NOTES
A prerelease of herdr-voice: version ${version} of the plugin, built from the
commit \`${tag}\` points at.

It is marked as a prerelease because the tag carries semver's prerelease marker —
the hyphen in \`${tag}\`. That is the whole rule; \`scripts/release-kind.sh\` is
where it lives. A prerelease here means the version is published for use and
stays published, but is not offered as the current one: GitHub does not mark it
latest, and installing it takes naming it.

## Install

\`\`\`sh
herdr plugin install ${repo} --ref ${tag}
\`\`\`

Without \`--ref\`, herdr installs from the default branch and fetches the archive
for whatever version the manifest names there, which is not necessarily this one.

The install fetches the archive below that matches the machine, checks it against
the \`.sha256\` file published beside it, and compiles from source only when there
is no archive for that platform.

## Before you rely on it

Which platforms this has actually been exercised on, and with what result, is in
[\`docs/evidence.md\`](https://github.com/${repo}/blob/${tag}/docs/evidence.md).
NOTES
