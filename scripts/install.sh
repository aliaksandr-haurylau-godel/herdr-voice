#!/bin/sh
#
# The herdr [[build]] step for macOS and Linux: put the released binary at
# target/release/herdr-voice without compiling it.
#
# herdr runs this on `herdr plugin install` and never on `herdr plugin link`, and
# it runs it before the plugin is registered. A non-zero exit aborts the install
# and leaves nothing behind, so every message that ends in one says so.
#
# Build commands receive none of herdr's environment — there is no
# HERDR_PLUGIN_ROOT here — so the repository root comes from this script's own
# location. Nothing here writes to herdr-plugin.toml: changing the manifest during
# a build aborts the install.
#
# Sourcing this file with HERDR_VOICE_INSTALL_LIB=1 defines the functions and runs
# nothing. scripts/test-install.sh does that.

set -eu

HV_NAME=herdr-voice
HV_REPO=aliaksandr-haurylau-godel/herdr-voice

# ---------------------------------------------------------------------------
# What this checkout is
# ---------------------------------------------------------------------------

hv_root() {
    ( cd "$(dirname "$0")/.." && pwd )
}

hv_manifest_version() {
    # $1 = repository root. The first `version = "..."` in the manifest is the
    # package's; the tables below it have no version key of their own.
    sed -n 's/^version = "\([^"]*\)".*/\1/p' "$1/herdr-plugin.toml" | head -n 1
}

# ---------------------------------------------------------------------------
# What it is running on
#
# The pairs below are exactly the targets .github/workflows/release.yml builds.
# Anything else has no archive, which is not an error — it is the source build.
# ---------------------------------------------------------------------------

hv_target() {
    # $1 = uname -s, $2 = uname -m
    case "$1:$2" in
        Darwin:arm64)              echo aarch64-apple-darwin ;;
        Darwin:x86_64)             echo x86_64-apple-darwin ;;
        Linux:x86_64)              echo x86_64-unknown-linux-gnu ;;
        Linux:aarch64|Linux:arm64) echo aarch64-unknown-linux-gnu ;;
        *)                         return 1 ;;
    esac
}

# ---------------------------------------------------------------------------
# Fetching
#
# Two rules, not one, and they answer different questions.
#
# The retry decides whether we know: GitHub's content delivery network answers
# 404 for some minutes after a release publishes, so a single 404 is not evidence
# that a target is missing.
#
# The three words below decide what we do once we know, and the caller acts on
# them: `missing` is the source build, `error` is a stop. Collapsing the two puts
# a machine whose network hiccupped into an hour of compiling nobody asked for.
# ---------------------------------------------------------------------------

HV_RETRY_ATTEMPTS="${HV_RETRY_ATTEMPTS:-5}"
HV_RETRY_DELAY="${HV_RETRY_DELAY:-3}"

hv_fetch_once() {
    # $1 = url, $2 = destination. Prints ok, missing or error. Always exits 0:
    # the answer is the word, not the status, so that `set -e` cannot turn a
    # 404 into an abort before the caller has decided what it means.
    if ! http="$(curl -sSL -o "$2" -w '%{http_code}' "$1" 2>/dev/null)"; then
        echo error
        return 0
    fi
    case "${http}" in
        200) echo ok ;;
        404) echo missing ;;
        *)   echo error ;;
    esac
}

hv_fetch() {
    # $1 = url, $2 = destination. Prints the settled answer.
    attempt=1
    while :; do
        outcome="$(hv_fetch_once "$1" "$2")"
        if [ "${outcome}" = ok ] || [ "${attempt}" -ge "${HV_RETRY_ATTEMPTS}" ]; then
            echo "${outcome}"
            return 0
        fi
        attempt=$((attempt + 1))
        sleep "${HV_RETRY_DELAY}"
    done
}

# ---------------------------------------------------------------------------
# Entry point
# ---------------------------------------------------------------------------

hv_main() {
    root="$(hv_root)"
    version="$(hv_manifest_version "${root}")"
    tag="v${version}"
    printf '%s: %s wants the release %s\n' "${HV_NAME}" "${root}" "${tag}"
}

if [ "${HERDR_VOICE_INSTALL_LIB:-0}" != 1 ]; then
    hv_main "$@"
fi
