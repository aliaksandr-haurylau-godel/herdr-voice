#!/bin/sh
#
# Drives every decision scripts/install.sh makes, with no network, no release and
# no herdr. The script is sourced as a library — see the guard at the bottom of
# it — so each function can be called directly and the fetching one replaced.

set -eu

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
failures=0

check() {
    # label, actual, expected
    if [ "$2" = "$3" ]; then
        printf 'ok    %s\n' "$1"
    else
        printf 'FAIL  %s: got [%s], expected [%s]\n' "$1" "$2" "$3"
        failures=$((failures + 1))
    fi
}

HERDR_VOICE_INSTALL_LIB=1
export HERDR_VOICE_INSTALL_LIB
# shellcheck source=./install.sh
. "${ROOT}/scripts/install.sh"

# --- the platform map -------------------------------------------------------
check "Darwin arm64"    "$(hv_target Darwin arm64)"    aarch64-apple-darwin
check "Darwin x86_64"   "$(hv_target Darwin x86_64)"   x86_64-apple-darwin
check "Linux x86_64"    "$(hv_target Linux x86_64)"    x86_64-unknown-linux-gnu
check "Linux aarch64"   "$(hv_target Linux aarch64)"   aarch64-unknown-linux-gnu
check "Linux arm64"     "$(hv_target Linux arm64)"     aarch64-unknown-linux-gnu

# A pair the release matrix has no build for prints nothing and reports failure,
# which is what sends the run to the source build.
if hv_target Linux riscv64 >/dev/null 2>&1; then
    printf 'FAIL  Linux riscv64: reported a target it has no build for\n'
    failures=$((failures + 1))
else
    printf 'ok    Linux riscv64 has no target\n'
fi
if hv_target FreeBSD x86_64 >/dev/null 2>&1; then
    printf 'FAIL  FreeBSD x86_64: reported a target it has no build for\n'
    failures=$((failures + 1))
else
    printf 'ok    FreeBSD x86_64 has no target\n'
fi

# --- the version, out of a manifest -----------------------------------------
# A fixture rather than the real manifest, so that the test still means something
# when the real version moves.
fixture="$(mktemp -d)"
trap 'rm -rf "${fixture}"' EXIT
cat >"${fixture}/herdr-plugin.toml" <<'TOML'
id = "haurylau.voice"
name = "Voice"
version = "0.4.2"
min_herdr_version = "0.8.0"

[[build]]
platforms = ["macos", "linux"]
command = ["sh", "scripts/install.sh"]
TOML
check "version from the manifest" "$(hv_manifest_version "${fixture}")" 0.4.2

# The real manifest is read too: if this stops answering, the URL the script
# builds is wrong and nothing else in the suite would notice.
if [ -z "$(hv_manifest_version "${ROOT}")" ]; then
    printf 'FAIL  the repository manifest yielded no version\n'
    failures=$((failures + 1))
else
    printf 'ok    the repository manifest yields %s\n' "$(hv_manifest_version "${ROOT}")"
fi

if [ "${failures}" -ne 0 ]; then
    printf '\n%s assertion(s) failed\n' "${failures}" >&2
    exit 1
fi
printf '\nall install assertions passed\n'
