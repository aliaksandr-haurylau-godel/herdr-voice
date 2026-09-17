#!/usr/bin/env bash
#
# Verify that `herdr plugin install` works on a machine with no Rust toolchain,
# inside a throwaway container.
#
# This is the check `scripts/linux-check.sh` cannot be. That one installs rustup
# and cargo and then links a local checkout, which never runs a build entry at
# all. This one asserts that no toolchain is present and installs from GitHub, so
# the build entry under test is the thing being exercised.
#
# Run it inside a clean glibc Linux container as root, with network access:
#
#   bash scripts/install-check.sh
#
# A musl image (Alpine) will not do: the published Linux archives are built
# against glibc.
#
# It installs from a tag rather than a branch. A tag pins one tree and the
# archives were built from exactly it, so the manifest the build entry reads and
# the archives it fetches cannot disagree.
#
# Environment it honours:
#
#   HERDR_VOICE_REF    the tag or branch to install (default: v0.0.0)
#   HERDR_VOICE_REPO   the owner/repo shorthand to install from
#   HERDR_VOICE_WORK   where the logs go (default: /tmp/herdr-voice-install-check)

set -eu

PLUGIN_ID="haurylau.voice"
REPO="${HERDR_VOICE_REPO:-aliaksandr-haurylau-godel/herdr-voice}"
REF="${HERDR_VOICE_REF:-v0.0.0}"
WORK="${HERDR_VOICE_WORK:-/tmp/herdr-voice-install-check}"
REPORT="${WORK}/report.tsv"
LOGS="${WORK}/logs"

mkdir -p "${WORK}" "${LOGS}"
: >"${REPORT}"

record() { printf '%s\t%s\t%s\n' "$1" "$2" "$3" >>"${REPORT}"; }

print_report() {
    printf '\n== report ==\n'
    printf '%-14s %-5s %s\n' "step" "exit" "outcome"
    while IFS=$'\t' read -r step code outcome; do
        printf '%-14s %-5s %s\n' "${step}" "${code}" "${outcome}"
    done <"${REPORT}"
    printf '\nlogs: %s\n' "${LOGS}"
}

fail() {
    record "$1" "$2" "$3"
    printf '\nFAILED at step %s (exit %s): %s\n' "$1" "$2" "$3" >&2
    printf 'next: %s\n' "$4" >&2
    print_report
    exit 1
}

cleanup() {
    if command -v herdr >/dev/null 2>&1; then
        herdr plugin uninstall "${PLUGIN_ID}" >/dev/null 2>&1 || true
    fi
    pkill -f 'herdr-voice daemon' >/dev/null 2>&1 || true
}
trap cleanup EXIT

# ---------------------------------------------------------------------------
# Step 1: no toolchain
#
# The premise of the whole check, and the easiest thing to lose: an image that
# happens to carry cargo would turn this into a slower copy of linux-check.sh
# while still printing green.
# ---------------------------------------------------------------------------

printf '\n== step: no-toolchain ==\n'
for tool in cargo rustup rustc; do
    if command -v "${tool}" >/dev/null 2>&1; then
        fail "no-toolchain" "-" "${tool} is on PATH, so this image cannot prove anything" \
            "run this in an image with no Rust toolchain; that absence is what the check is about"
    fi
done
record "no-toolchain" "0" "no cargo, no rustup, no rustc"

# ---------------------------------------------------------------------------
# Step 2: what the install itself needs
# ---------------------------------------------------------------------------

printf '\n== step: prereqs ==\n'
if command -v apt-get >/dev/null 2>&1; then
    export DEBIAN_FRONTEND=noninteractive
    apt-get update -qq >"${LOGS}/apt.log" 2>&1
    apt-get install -y -qq git curl ca-certificates tar >>"${LOGS}/apt.log" 2>&1 \
        || fail "prereqs" "$?" "could not install git, curl, ca-certificates and tar" \
            "read ${LOGS}/apt.log"
else
    fail "prereqs" "-" "no apt-get on this image" \
        "use a Debian or Ubuntu image, or install git, curl, ca-certificates and tar by hand first"
fi
record "prereqs" "0" "git, curl, ca-certificates, tar"

# ---------------------------------------------------------------------------
# Step 3: herdr
# ---------------------------------------------------------------------------

printf '\n== step: herdr ==\n'
if ! command -v herdr >/dev/null 2>&1; then
    curl -fsSL https://herdr.dev/install.sh | sh >"${LOGS}/herdr-install.log" 2>&1 \
        || fail "herdr" "$?" "the herdr installer failed" "read ${LOGS}/herdr-install.log"
fi
export PATH="${HOME}/.local/bin:${PATH}"
command -v herdr >/dev/null 2>&1 \
    || fail "herdr" "-" "herdr is not on PATH after the installer ran" \
        "read ${LOGS}/herdr-install.log and add the directory it used to PATH"
HERDR_VERSION="$(herdr --version 2>&1 | head -n 1)"
printf 'herdr: %s\n' "${HERDR_VERSION}"
record "herdr" "0" "${HERDR_VERSION}"

# ---------------------------------------------------------------------------
# Step 4: the install under test
# ---------------------------------------------------------------------------

printf '\n== step: install ==\n'
printf 'installing %s at %s\n' "${REPO}" "${REF}"
set +e
herdr plugin install "${REPO}" --ref "${REF}" --yes >"${LOGS}/install.log" 2>&1
INSTALL_CODE=$?
set -e
cat "${LOGS}/install.log"
if [ "${INSTALL_CODE}" -ne 0 ]; then
    fail "install" "${INSTALL_CODE}" "herdr plugin install failed" \
        "read ${LOGS}/install.log; the build entry's own message is in it"
fi
record "install" "0" "installed ${REPO} at ${REF}"

# ---------------------------------------------------------------------------
# Step 5: it fetched rather than compiled
#
# With no toolchain present a compile could not have happened, so this is a
# second, cheaper witness rather than the proof: it catches a build entry that
# quietly fell back and somehow succeeded.
# ---------------------------------------------------------------------------

printf '\n== step: no-compile ==\n'
if grep -qiE '^[[:space:]]*(Compiling|Downloaded) |cargo build' "${LOGS}/install.log"; then
    fail "no-compile" "-" "the install log shows a source build" \
        "read ${LOGS}/install.log; the fetch path did not run, and the fallback's message says why"
fi
if ! grep -q "verified against its published digest" "${LOGS}/install.log"; then
    fail "no-compile" "-" "the install log does not say an archive was verified" \
        "read ${LOGS}/install.log; the build entry may not have run at all"
fi
record "no-compile" "0" "fetched and verified, nothing compiled"

# ---------------------------------------------------------------------------
# Step 6: the plugin is registered
# ---------------------------------------------------------------------------

printf '\n== step: registered ==\n'
herdr plugin list >"${LOGS}/list.log" 2>&1
cat "${LOGS}/list.log"
grep -q "${PLUGIN_ID}" "${LOGS}/list.log" \
    || fail "registered" "-" "${PLUGIN_ID} is not in herdr plugin list" \
        "read ${LOGS}/list.log"
grep -E "${PLUGIN_ID}.*enabled" "${LOGS}/list.log" >/dev/null \
    || fail "registered" "-" "${PLUGIN_ID} is installed but not enabled" \
        "read ${LOGS}/list.log"
record "registered" "0" "${PLUGIN_ID} installed and enabled"

# ---------------------------------------------------------------------------
# Step 7: the binary it fetched actually runs
#
# A binary built for the wrong architecture is the failure this catches, and it
# fails by saying nothing at all rather than by exiting non-zero, so the check is
# on the output and not only on the code.
# ---------------------------------------------------------------------------

printf '\n== step: runs ==\n'
# herdr stores a GitHub-installed plugin under a directory named for the plugin
# id plus a hash, so the name is found rather than known.
PLUGIN_ROOT="$(find "${HOME}/.config/herdr/plugins/github" \
    -maxdepth 1 -type d -name "${PLUGIN_ID}-*" 2>/dev/null | head -n 1)"
if [ -z "${PLUGIN_ROOT}" ] || [ ! -x "${PLUGIN_ROOT}/target/release/herdr-voice" ]; then
    fail "runs" "-" "no executable at target/release/herdr-voice under the installed plugin" \
        "look under ${HOME}/.config/herdr/plugins/github for the plugin directory"
fi
set +e
"${PLUGIN_ROOT}/target/release/herdr-voice" doctor >"${LOGS}/doctor.log" 2>&1
DOCTOR_CODE=$?
set -e
cat "${LOGS}/doctor.log"
if [ ! -s "${LOGS}/doctor.log" ]; then
    fail "runs" "${DOCTOR_CODE}" "the fetched binary produced no output at all" \
        "a binary for the wrong architecture fails like this; check the target in the install log"
fi
record "runs" "${DOCTOR_CODE}" "the fetched binary runs and answers"

print_report
printf '\nthe install path works with no Rust toolchain present\n'
