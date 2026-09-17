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
# Verifying
# ---------------------------------------------------------------------------

hv_digest() {
    # $1 = file. macOS has shasum and not sha256sum; Linux has both or the
    # first. .github/workflows/release.yml makes the same choice when it writes
    # the sidecar this is compared against.
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$1" | cut -d' ' -f1
    else
        shasum -a 256 "$1" | cut -d' ' -f1
    fi
}

# ---------------------------------------------------------------------------
# How this ends
#
# A non-zero exit from a build command aborts the install and registers no
# plugin, so a person reading any of these messages has a machine with nothing
# new on it. Each one says that, and says what to do next.
# ---------------------------------------------------------------------------

hv_die() {
    # $1 = what went wrong, $2 = what to do about it.
    printf '%s: %s\n' "${HV_NAME}" "$1" >&2
    printf '%s: nothing was installed. %s\n' "${HV_NAME}" "$2" >&2
    exit 1
}

hv_have_cargo() {
    # A function rather than an inline `command -v` so the suite can answer it
    # without emptying PATH, which would also take away the mktemp and tar this
    # script needs before it ever reaches the fallback.
    command -v cargo >/dev/null 2>&1
}

hv_fallback() {
    # $1 = why there is no archive to use.
    printf '%s: %s\n' "${HV_NAME}" "$1" >&2
    printf '%s: building from source instead, which compiles the candle crates and takes a while.\n' \
        "${HV_NAME}" >&2
    if ! hv_have_cargo; then
        hv_die "there is no archive for this platform and no cargo to build one" \
               "install a Rust toolchain from https://rustup.rs and run the install again, or install on a platform a release archive is published for."
    fi
    cargo build --release
}

# ---------------------------------------------------------------------------
# Entry point
# ---------------------------------------------------------------------------

hv_main() {
    root="$(hv_root)"
    cd "${root}"
    version="$(hv_manifest_version "${root}")"
    tag="v${version}"

    if ! target="$(hv_target "$(uname -s)" "$(uname -m)")"; then
        hv_fallback "no release archive is built for $(uname -s) $(uname -m)"
        return 0
    fi

    archive="${HV_NAME}-${tag}-${target}.tar.gz"
    base="https://github.com/${HV_REPO}/releases/download/${tag}"
    tmp="$(mktemp -d)"
    trap 'rm -rf "${tmp}"' EXIT

    case "$(hv_fetch "${base}/${archive}" "${tmp}/${archive}")" in
        ok) ;;
        missing)
            # A 404 here is what a missing release and a missing target both look
            # like, and nothing else fetched tells them apart. Telling them apart
            # needs a release-level API request, which brings its own retry, its
            # own failure mode and a rate limit a shared address can exhaust — a
            # fifth way to fail, bought for one clause. So the message names both.
            hv_fallback "no archive at ${base}/${archive} — either no release is tagged ${tag}, or that release has no build for ${target}"
            return 0 ;;
        error)
            hv_die "could not reach ${base}/${archive} after ${HV_RETRY_ATTEMPTS} attempts" \
                   "check the network and run the install again." ;;
    esac

    case "$(hv_fetch "${base}/${archive}.sha256" "${tmp}/${archive}.sha256")" in
        ok) ;;
        missing)
            hv_die "the release ${tag} publishes ${archive} but no ${archive}.sha256, so its bytes cannot be checked" \
                   "report it against the release; an unverified archive is not installed." ;;
        error)
            hv_die "could not reach ${base}/${archive}.sha256 after ${HV_RETRY_ATTEMPTS} attempts" \
                   "check the network and run the install again." ;;
    esac

    expected="$(cut -d' ' -f1 <"${tmp}/${archive}.sha256")"
    actual="$(hv_digest "${tmp}/${archive}")"
    if [ "${expected}" != "${actual}" ]; then
        hv_die "${archive} does not match the digest published with it (expected ${expected}, got ${actual})" \
               "the download is damaged or the release was changed after it was published; run the install again, and report it if it repeats."
    fi

    tar -xzf "${tmp}/${archive}" -C "${tmp}"
    mkdir -p "${root}/target/release"
    cp "${tmp}/${HV_NAME}-${tag}-${target}/${HV_NAME}" "${root}/target/release/${HV_NAME}"
    chmod 0755 "${root}/target/release/${HV_NAME}"
    printf '%s: installed %s from %s, verified against its published digest\n' \
        "${HV_NAME}" "target/release/${HV_NAME}" "${tag}"
}

if [ "${HERDR_VOICE_INSTALL_LIB:-0}" != 1 ]; then
    hv_main "$@"
fi
