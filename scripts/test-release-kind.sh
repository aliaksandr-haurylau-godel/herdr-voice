#!/bin/sh
#
# Drives scripts/release-kind.sh over every tag shape that matters. A wrong
# answer here publishes a throwaway tag as the repository's latest release,
# where anyone's `herdr plugin install` finds it.

set -eu

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SCRIPT="${ROOT}/scripts/release-kind.sh"
failures=0

expect() {
    # tag, expected answer
    actual="$(sh "${SCRIPT}" "$1")"
    if [ "${actual}" = "$2" ]; then
        printf 'ok    %-16s -> %s\n' "$1" "${actual}"
    else
        printf 'FAIL  %-16s -> %s (expected %s)\n' "$1" "${actual}" "$2"
        failures=$((failures + 1))
    fi
}

# 0.0.0 is this repository's placeholder for "unreleased": both the manifest and
# the crate sit there, and a tag cut from it is never a version of the plugin.
expect v0.0.0        prerelease
# A hyphen is semver's own prerelease marker.
expect v1.0.0-rc.1   prerelease
expect v0.2.0-test   prerelease
# Everything else is a real release.
expect v0.1.0        release
expect v1.0.0        release
expect v10.20.30     release

if [ "${failures}" -ne 0 ]; then
    printf '\n%s assertion(s) failed\n' "${failures}" >&2
    exit 1
fi
printf '\nall release-kind assertions passed\n'
