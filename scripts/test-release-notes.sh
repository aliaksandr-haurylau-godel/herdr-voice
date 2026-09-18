#!/bin/sh
#
# Drives scripts/release-notes.sh. The notes are the first thing a reader meets
# on the releases page, and the last release published notes that described a
# different tag entirely — one that was disposable, while the published one was
# not. So what is asserted here is that the text belongs to the tag it is
# published with, and that nothing in it calls the tag disposable.

set -eu

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SCRIPT="${ROOT}/scripts/release-notes.sh"
failures=0

fail() {
    printf 'FAIL  %s\n' "$1"
    failures=$((failures + 1))
}

contains() {
    # tag, substring
    if printf '%s' "$(sh "${SCRIPT}" "$1")" | grep -qF -- "$2"; then
        printf 'ok    %-16s contains %s\n' "$1" "$2"
    else
        fail "$1 does not contain: $2"
    fi
}

lacks() {
    # tag, substring
    if printf '%s' "$(sh "${SCRIPT}" "$1")" | grep -qF -- "$2"; then
        fail "$1 still contains: $2"
    else
        printf 'ok    %-16s lacks    %s\n' "$1" "$2"
    fi
}

# The tag itself, and the version the manifest and `herdr plugin list` show,
# which is the tag without its leading `v`.
contains v0.1.0-beta.1 'version 0.1.0-beta.1 of the plugin'
contains v0.1.0-beta.1 'v0.1.0-beta.1'
contains v1.0.0-rc.1   'version 1.0.0-rc.1 of the plugin'

# The install line is the reason the notes exist for a reader, and `--ref` is the
# part that makes it install this version rather than the default branch's.
contains v0.1.0-beta.1 'herdr plugin install aliaksandr-haurylau-godel/herdr-voice --ref v0.1.0-beta.1'
contains v1.0.0-rc.1   '--ref v1.0.0-rc.1'

# The sentences that were published with v0.1.0-beta.1 and were false of it.
lacks v0.1.0-beta.1 'Not a version of the plugin'
lacks v0.1.0-beta.1 'It is deleted'
lacks v0.1.0-beta.1 'install-path check'

# A tag belongs to exactly one set of notes: nothing generated for one tag may
# mention another.
lacks v1.0.0-rc.1 'v0.1.0-beta.1'
lacks v1.0.0-rc.1 'v0.0.0'

# An unsubstituted `${...}` means the heredoc lost an expansion, which reads as
# an unfinished sentence on the releases page rather than as a failure.
lacks v0.1.0-beta.1 '${'

# v0.0.0 is the one prerelease that really is disposable, and its notes are the
# only ones allowed to say so. It must not claim to be a version of the plugin,
# and it must not hand anyone an install line.
contains v0.0.0 'Not a version of the plugin'
contains v0.0.0 'It is deleted once the result is recorded'
lacks v0.0.0 'herdr plugin install'
lacks v0.0.0 'version 0.0.0 of the plugin'

# No tag, no notes: publishing an empty body is worse than a failed job, because
# the release exists afterwards either way.
if sh "${SCRIPT}" >/dev/null 2>&1; then
    fail 'called with no tag, the script succeeded'
else
    printf 'ok    %-16s called with no tag, the script fails\n' '(none)'
fi

if [ "${failures}" -ne 0 ]; then
    printf '\n%s assertion(s) failed\n' "${failures}" >&2
    exit 1
fi
printf '\nall release-notes assertions passed\n'
