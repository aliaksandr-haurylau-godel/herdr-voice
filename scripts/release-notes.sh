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
here="$(cd "$(dirname "$0")" && pwd)"

# The tag is expanded inside the heredoc below, so it is checked first. A tag may
# legally carry `$`, backticks and parentheses, and a heredoc that expands is a
# shell that runs whatever those spell. Only someone with push access can create
# a tag, so this closes a small hole rather than a large one — but it costs a
# line, and the alternative is a workflow with a token in its environment.
case "${tag}" in
    v[0-9]*) ;;
    *)  echo "release-notes.sh: not a tag this repository cuts: ${tag}" >&2
        exit 2 ;;
esac
case "${tag}" in
    *[!0-9A-Za-z.+-]*)
        echo "release-notes.sh: the tag carries a character a version does not: ${tag}" >&2
        exit 2 ;;
esac

# Only prereleases get notes from here; a full release is published with
# `--generate-notes`. Asking the rule rather than repeating it means the two
# cannot drift, and it keeps the script from writing "it is marked as a
# prerelease" over a tag that is not one.
if [ "$(sh "${here}/release-kind.sh" "${tag}")" != prerelease ]; then
    echo "release-notes.sh: ${tag} is not a prerelease; a release is published with --generate-notes" >&2
    exit 2
fi

# The archives and the manifest carry the version without the leading `v`, which
# is what a reader compares against `herdr plugin list`.
version="${tag#v}"
repo="aliaksandr-haurylau-godel/herdr-voice"

# v0.0.0 is the repository's placeholder version, and `scripts/release-kind.sh`
# answers `prerelease` for it on that ground rather than on the hyphen rule. A
# tag cut from it is not a version of the plugin and is thrown away once it has
# been used, so it is the one prerelease the text below would be false of. No
# such tag exists now — the one there was went away when the version became
# 0.1.0-beta.1 — and this branch costs a line against the version returning
# there.
if [ "${tag}" = v0.0.0 ]; then
    cat <<'PLACEHOLDER'
Not a version of the plugin. 0.0.0 is this repository's placeholder for
"unreleased", and nothing cut from it is a version to install.

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

\`--ref\` is what pins the install to this tag; the install reads the manifest at
the ref it is given, and that manifest names the version whose archives are
fetched.

The install fetches the archive below that matches the machine, checks it against
the \`.sha256\` file published beside it, and compiles from source only when there
is no archive for that platform.

## Before you rely on it

This is published to be used, and not everything in it has been exercised on
every platform it claims. What was verified by hand, and on which platform, is
what [\`docs/evidence.md\`](https://github.com/${repo}/blob/${tag}/docs/evidence.md)
records at this tag.
NOTES
