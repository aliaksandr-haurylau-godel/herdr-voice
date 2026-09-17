#!/bin/sh
#
# Answers one question for .github/workflows/release.yml: does this tag publish
# as a prerelease, or as a release?
#
# It exists as a script rather than as a `case` inside the workflow so that it
# can be tested. Getting it wrong is not visible until a throwaway tag is sitting
# at the top of the releases page being installed by strangers.
#
# Usage: sh scripts/release-kind.sh v1.2.3  ->  prints `prerelease` or `release`

set -eu

case "${1:?usage: release-kind.sh <tag>}" in
    # The repository's placeholder version. Nothing cut from it is a version of
    # the plugin; herdr-plugin.toml and Cargo.toml both say 0.0.0.
    v0.0.0) echo prerelease ;;
    # Semver's own prerelease marker.
    *-*)    echo prerelease ;;
    *)      echo release ;;
esac
