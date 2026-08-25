#!/usr/bin/env bash
#
# Verify this plugin against a live herdr on Linux, inside a throwaway container.
#
# The whole check is one non-interactive run: it installs what it needs, builds
# the plugin, links it into herdr, and exercises the daemon, `doctor`, an action
# through herdr and the case a container is uniquely good at — no capture device
# at all. Every step records its exit code and a one-line outcome, and the run
# prints them as a table at the end, whether it succeeded or failed.
#
# Run it inside a clean Linux container as root, with network access:
#
#   bash scripts/linux-check.sh
#
# Environment it honours:
#
#   HERDR_VOICE_CHECKOUT  a checkout to use instead of cloning
#   HERDR_VOICE_REPO      the repository to clone when there is no checkout
#   HERDR_VOICE_REF       the branch or tag to clone (default: the default branch)
#   HERDR_VOICE_WORK      where the clone and the logs go (default: /tmp/herdr-voice-check)
#
# It is safe to re-run: it unlinks a plugin it linked before, stops a daemon and
# a server it started before, and reuses the work directory.

set -eu

PLUGIN_ID="haurylau.voice"
REPO_URL="${HERDR_VOICE_REPO:-https://github.com/aliaksandr-haurylau-godel/herdr-voice.git}"
REF="${HERDR_VOICE_REF:-}"
WORK="${HERDR_VOICE_WORK:-/tmp/herdr-voice-check}"
REPORT="${WORK}/report.tsv"
LOGS="${WORK}/logs"

# The daemon answers a few bytes, so anything that takes longer than this is the
# hang the check exists to catch, not slowness.
CLIENT_TIMEOUT=20
# Starting a server or a daemon is allowed to take longer, but not forever.
START_TIMEOUT=30

mkdir -p "${WORK}" "${LOGS}"
: >"${REPORT}"

# ---------------------------------------------------------------------------
# Reporting
# ---------------------------------------------------------------------------

record() {
    # step, exit code, one-line outcome
    printf '%s\t%s\t%s\n' "$1" "$2" "$3" >>"${REPORT}"
}

print_report() {
    printf '\n== report ==\n'
    printf '%-14s %-5s %s\n' "step" "exit" "outcome"
    while IFS=$'\t' read -r step code outcome; do
        printf '%-14s %-5s %s\n' "${step}" "${code}" "${outcome}"
    done <"${REPORT}"
    printf '\nlogs: %s\n' "${LOGS}"
}

# A step that failed. Names what to do next, prints the report, and stops the
# run: a later step's result would be about a machine that is already wrong.
fail() {
    # step, exit code, outcome, what to do next
    record "$1" "$2" "$3"
    printf '\nFAILED at step %s (exit %s): %s\n' "$1" "$2" "$3" >&2
    printf 'next: %s\n' "$4" >&2
    print_report
    exit 1
}

# A step that could not run at all. Recorded as a failure, never as a pass.
skip_as_failure() {
    # step, reason, what to do next
    fail "$1" "-" "could not run: $2" "$3"
}

cleanup() {
    # Best effort, and quiet: this runs after the report has been printed.
    if command -v herdr >/dev/null 2>&1; then
        herdr plugin unlink "${PLUGIN_ID}" >/dev/null 2>&1 || true
    fi
    if [ -n "${DAEMON_PID:-}" ]; then
        kill "${DAEMON_PID}" >/dev/null 2>&1 || true
    fi
    if [ -n "${SERVER_PID:-}" ]; then
        herdr server stop >/dev/null 2>&1 || true
        kill "${SERVER_PID}" >/dev/null 2>&1 || true
    fi
}
trap cleanup EXIT

# ---------------------------------------------------------------------------
# Step 1: the machine itself
# ---------------------------------------------------------------------------

printf '== step: os ==\n'
OS_NAME="unknown"
if [ -r /etc/os-release ]; then
    # shellcheck disable=SC1091
    . /etc/os-release
    OS_NAME="${PRETTY_NAME:-${NAME:-unknown}}"
fi
ARCH="$(uname -m)"
KERNEL="$(uname -sr)"
printf 'image:  %s\n' "${OS_NAME}"
printf 'kernel: %s\n' "${KERNEL}"
printf 'arch:   %s\n' "${ARCH}"
record "os" "0" "${OS_NAME}, ${ARCH}, ${KERNEL}"

# ---------------------------------------------------------------------------
# Step 2: system packages
#
# `interprocess` needs nothing beyond a C toolchain today. `cpal` arrives with
# issue #8 and pulls `alsa-sys`, which needs the ALSA development headers on
# Linux, so they are installed here already: a check that installs them only
# after the crate needs them proves nothing about the machine the crate will
# meet.
# ---------------------------------------------------------------------------

printf '\n== step: packages ==\n'
if command -v apt-get >/dev/null 2>&1; then
    PM="apt-get"
    export DEBIAN_FRONTEND=noninteractive
    apt-get update -qq
    apt-get install -y -qq \
        ca-certificates curl git build-essential pkg-config libasound2-dev \
        procps coreutils >"${LOGS}/packages.log" 2>&1
elif command -v dnf >/dev/null 2>&1; then
    PM="dnf"
    dnf install -y -q \
        ca-certificates curl git gcc gcc-c++ make pkgconf-pkg-config \
        alsa-lib-devel procps-ng coreutils >"${LOGS}/packages.log" 2>&1
elif command -v apk >/dev/null 2>&1; then
    PM="apk"
    apk add --no-cache \
        ca-certificates curl git build-base pkgconf alsa-lib-dev \
        procps coreutils >"${LOGS}/packages.log" 2>&1
elif command -v pacman >/dev/null 2>&1; then
    PM="pacman"
    pacman -Sy --noconfirm --needed \
        ca-certificates curl git base-devel pkgconf alsa-lib procps-ng \
        >"${LOGS}/packages.log" 2>&1
else
    skip_as_failure "packages" \
        "no apt-get, dnf, apk or pacman on this image" \
        "run this on an image with one of those package managers, or install curl, git, a C toolchain, pkg-config and the ALSA development headers by hand first"
fi

for tool in curl git cc pkg-config timeout; do
    if ! command -v "${tool}" >/dev/null 2>&1; then
        skip_as_failure "packages" \
            "${tool} is still missing after installing with ${PM}" \
            "install ${tool} by hand and re-run; the package names in this script may be wrong for this image"
    fi
done

ALSA_HEADERS="absent"
if pkg-config --exists alsa 2>/dev/null; then
    ALSA_HEADERS="alsa $(pkg-config --modversion alsa)"
fi
printf 'package manager: %s\n' "${PM}"
printf 'cc:              %s\n' "$(cc --version 2>&1 | head -n 1)"
printf 'git:             %s\n' "$(git --version)"
printf 'curl:            %s\n' "$(curl --version 2>&1 | head -n 1)"
printf 'ALSA headers:    %s\n' "${ALSA_HEADERS}"
record "packages" "0" "${PM}; ALSA headers: ${ALSA_HEADERS}"

# ---------------------------------------------------------------------------
# Step 3: the Rust toolchain
# ---------------------------------------------------------------------------

printf '\n== step: rust ==\n'
export PATH="${HOME}/.cargo/bin:${HOME}/.local/bin:${PATH}"
if ! command -v cargo >/dev/null 2>&1; then
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
        | sh -s -- -y --default-toolchain stable --profile minimal \
        >"${LOGS}/rustup.log" 2>&1 \
        || fail "rust" "$?" "rustup install failed" \
            "read ${LOGS}/rustup.log; the usual cause is no network or no CA certificates"
fi
if ! command -v cargo >/dev/null 2>&1; then
    fail "rust" "-" "cargo is not on PATH after installing rustup" \
        "check that ${HOME}/.cargo/bin exists and re-run"
fi
RUSTC_VERSION="$(rustc --version)"
CARGO_VERSION="$(cargo --version)"
printf 'rustc: %s\n' "${RUSTC_VERSION}"
printf 'cargo: %s\n' "${CARGO_VERSION}"
record "rust" "0" "${RUSTC_VERSION}; ${CARGO_VERSION}"

# ---------------------------------------------------------------------------
# Step 4: herdr
# ---------------------------------------------------------------------------

printf '\n== step: herdr ==\n'
if ! command -v herdr >/dev/null 2>&1; then
    curl -fsSL https://herdr.dev/install.sh | sh >"${LOGS}/herdr-install.log" 2>&1 \
        || fail "herdr" "$?" "the herdr installer failed" \
            "read ${LOGS}/herdr-install.log; if the image is musl-based (Alpine), try a glibc image instead"
fi
export PATH="${HOME}/.local/bin:${PATH}"
if ! command -v herdr >/dev/null 2>&1; then
    fail "herdr" "-" "herdr is not on PATH after the installer ran" \
        "read ${LOGS}/herdr-install.log and add the directory it installed into to PATH"
fi
HERDR_VERSION="$(herdr --version 2>&1 | head -n 1)"
printf 'herdr: %s\n' "${HERDR_VERSION}"
record "herdr" "0" "${HERDR_VERSION}"

# ---------------------------------------------------------------------------
# Step 5: the repository
# ---------------------------------------------------------------------------

printf '\n== step: repo ==\n'
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
CANDIDATE="$(dirname "${SCRIPT_DIR}")"
if [ -n "${HERDR_VOICE_CHECKOUT:-}" ]; then
    ROOT="${HERDR_VOICE_CHECKOUT}"
    SOURCE="checkout named by HERDR_VOICE_CHECKOUT"
elif [ -f "${CANDIDATE}/herdr-plugin.toml" ]; then
    ROOT="${CANDIDATE}"
    SOURCE="the checkout this script was run from"
else
    ROOT="${WORK}/herdr-voice"
    SOURCE="clone of ${REPO_URL}"
    if [ -d "${ROOT}/.git" ]; then
        git -C "${ROOT}" fetch --quiet --all
        git -C "${ROOT}" reset --hard --quiet "$(git -C "${ROOT}" rev-parse '@{u}')"
    elif [ -n "${REF}" ]; then
        git clone --quiet --branch "${REF}" "${REPO_URL}" "${ROOT}"
    else
        git clone --quiet "${REPO_URL}" "${ROOT}"
    fi
fi
if [ ! -f "${ROOT}/herdr-plugin.toml" ]; then
    fail "repo" "-" "no herdr-plugin.toml under ${ROOT}" \
        "point HERDR_VOICE_CHECKOUT at a checkout of this repository, or unset it so the script clones one"
fi
REVISION="$(git -C "${ROOT}" rev-parse --short HEAD 2>/dev/null || echo 'not a git checkout')"
printf 'root:     %s\n' "${ROOT}"
printf 'source:   %s\n' "${SOURCE}"
printf 'revision: %s\n' "${REVISION}"
record "repo" "0" "${SOURCE}, revision ${REVISION}"

# ---------------------------------------------------------------------------
# Step 6: the build
# ---------------------------------------------------------------------------

printf '\n== step: build ==\n'
BIN="${ROOT}/target/release/herdr-voice"
if ! (cd "${ROOT}" && cargo build --release) >"${LOGS}/build.log" 2>&1; then
    tail -n 40 "${LOGS}/build.log" >&2 || true
    fail "build" "$?" "cargo build --release failed" \
        "read ${LOGS}/build.log; a missing ALSA header here means the package step installed the wrong name for this image"
fi
if [ ! -x "${BIN}" ]; then
    fail "build" "-" "no binary at target/release/herdr-voice after a successful build" \
        "read ${LOGS}/build.log and check the [[bin]] name in Cargo.toml"
fi
printf 'binary:  %s\n' "${BIN}"
printf 'version: %s\n' "$("${BIN}" --version 2>&1 | head -n 1)"
record "build" "0" "cargo build --release produced target/release/herdr-voice"

# ---------------------------------------------------------------------------
# Step 7: link the plugin
#
# Linking comes before the server is started, and does not need one: the
# manifest's [[startup]] entry is what starts the daemon on a real machine, and
# a server that finds the plugin already linked is the only arrangement in which
# it can run that entry here.
# ---------------------------------------------------------------------------

printf '\n== step: link ==\n'
herdr plugin unlink "${PLUGIN_ID}" >/dev/null 2>&1 || true
if ! (cd "${ROOT}" && herdr plugin link .) >"${LOGS}/link.log" 2>&1; then
    cat "${LOGS}/link.log" >&2 || true
    fail "link" "$?" "herdr plugin link . failed" \
        "read ${LOGS}/link.log; a manifest that does not parse is the usual cause"
fi
herdr plugin list --plugin "${PLUGIN_ID}" >"${LOGS}/plugin-list.log" 2>&1 || true
if ! grep -q "${PLUGIN_ID}" "${LOGS}/plugin-list.log"; then
    fail "link" "-" "herdr plugin list does not show ${PLUGIN_ID} after linking" \
        "read ${LOGS}/plugin-list.log and ${LOGS}/link.log"
fi
CONFIG_DIR="$(herdr plugin config-dir "${PLUGIN_ID}" 2>/dev/null || echo 'not printed')"
printf 'linked, config dir: %s\n' "${CONFIG_DIR}"
record "link" "0" "linked; herdr computes config dir ${CONFIG_DIR}"

# ---------------------------------------------------------------------------
# Step 8: a live herdr server
#
# herdr's own TUI needs a terminal; the headless server does not, which is what
# makes this check possible in a container at all.
#
# `herdr status server` exits 0 whether or not a server is running and says which
# in its output, so the state is read out of --json. Trusting the exit code here
# reports a server that does not exist, and every later step then fails on a
# socket that was never there.
# ---------------------------------------------------------------------------

printf '\n== step: server ==\n'
SERVER_PID=""
server_is_up() {
    herdr status server --json 2>/dev/null | grep -q '"running":true'
}
if server_is_up; then
    printf 'a server was already running\n'
else
    herdr server >"${LOGS}/herdr-server.log" 2>&1 &
    SERVER_PID="$!"
    waited=0
    while [ "${waited}" -lt "${START_TIMEOUT}" ]; do
        if server_is_up; then
            break
        fi
        sleep 1
        waited=$((waited + 1))
    done
fi
if ! server_is_up; then
    tail -n 20 "${LOGS}/herdr-server.log" >&2 || true
    fail "server" "-" "no herdr server after ${START_TIMEOUT}s" \
        "read ${LOGS}/herdr-server.log; without a server no plugin action can be invoked"
fi
herdr status server --json >"${LOGS}/herdr-status.log" 2>&1 || true
record "server" "0" "herdr server running and answering on its socket"

# Step 9: the daemon
#
# Two ways in, and which one happened is recorded rather than smoothed over:
# herdr may have started the daemon through the manifest's [[startup]] entry, and
# if it did not, the script starts it by hand so the rest of the check can run.
# ---------------------------------------------------------------------------

printf '\n== step: daemon ==\n'
DAEMON_PID=""
daemon_is_up() {
    "${BIN}" doctor 2>/dev/null | grep -Eq '^daemon +ok'
}
waited=0
while [ "${waited}" -lt 5 ]; do
    if daemon_is_up; then
        break
    fi
    sleep 1
    waited=$((waited + 1))
done
if daemon_is_up; then
    DAEMON_SOURCE="started by herdr through the manifest [[startup]] entry"
else
    "${BIN}" daemon >"${LOGS}/daemon.log" 2>&1 &
    DAEMON_PID="$!"
    waited=0
    while [ "${waited}" -lt "${START_TIMEOUT}" ]; do
        if daemon_is_up; then
            break
        fi
        sleep 1
        waited=$((waited + 1))
    done
    DAEMON_SOURCE="started by this script; herdr did not start it from the manifest"
fi
if ! daemon_is_up; then
    tail -n 20 "${LOGS}/daemon.log" >&2 || true
    fail "daemon" "-" "no daemon listening after ${START_TIMEOUT}s" \
        "read ${LOGS}/daemon.log; the socket path the daemon derives is in the doctor output"
fi
printf '%s\n' "${DAEMON_SOURCE}"
record "daemon" "0" "${DAEMON_SOURCE}"

# ---------------------------------------------------------------------------
# Step 10: doctor
#
# On a clean container `doctor` is expected to exit 1: there is no speech model
# and no agent command-line tool for the rewrite stage. That is a report, not a
# breakage, so 0 and 1 are both recorded outcomes. Anything else — a crash, a
# signal, no output at all — is a failure, and so is a `doctor` that does not
# name the herdr it found.
# ---------------------------------------------------------------------------

printf '\n== step: doctor ==\n'
set +e
timeout "${CLIENT_TIMEOUT}" "${BIN}" doctor >"${LOGS}/doctor.log" 2>&1
DOCTOR_CODE="$?"
set -e
cat "${LOGS}/doctor.log"
case "${DOCTOR_CODE}" in
    124)
        fail "doctor" "${DOCTOR_CODE}" "doctor hung for more than ${CLIENT_TIMEOUT}s" \
            "a check that blocks is a defect; find which line is missing from ${LOGS}/doctor.log"
        ;;
    0 | 1) ;;
    *)
        fail "doctor" "${DOCTOR_CODE}" "doctor exited with an unexpected code" \
            "read ${LOGS}/doctor.log; codes other than 0 and 1 mean a crash rather than a report"
        ;;
esac
if [ ! -s "${LOGS}/doctor.log" ]; then
    fail "doctor" "${DOCTOR_CODE}" "doctor printed nothing" \
        "silence is the failure this project treats as a defect; the five lines are not optional"
fi
for line in herdr daemon config model rewrite; do
    if ! grep -Eq "^${line} " "${LOGS}/doctor.log"; then
        fail "doctor" "${DOCTOR_CODE}" "doctor printed no '${line}' line" \
            "read ${LOGS}/doctor.log; the five lines are a fixed order and a fixed shape"
    fi
done
SOCKET_LINE="$(grep -E '^daemon ' "${LOGS}/doctor.log" | head -n 1)"
record "doctor" "${DOCTOR_CODE}" "five lines; ${SOCKET_LINE}"

# ---------------------------------------------------------------------------
# Step 11: an action through herdr
# ---------------------------------------------------------------------------

printf '\n== step: action ==\n'
set +e
timeout "${CLIENT_TIMEOUT}" herdr plugin action invoke cancel --plugin "${PLUGIN_ID}" \
    >"${LOGS}/action.log" 2>&1
ACTION_CODE="$?"
set -e
cat "${LOGS}/action.log"
if [ "${ACTION_CODE}" = "124" ]; then
    fail "action" "${ACTION_CODE}" "the cancel action did not return within ${CLIENT_TIMEOUT}s" \
        "read ${LOGS}/action.log and ${LOGS}/daemon.log; a client that blocks is the failure the two-second reply bound exists to prevent"
fi
if [ "${ACTION_CODE}" != "0" ]; then
    fail "action" "${ACTION_CODE}" "the cancel action failed" \
        "read ${LOGS}/action.log; cancel needs no target pane, so a failure here is about the socket, not the context"
fi
record "action" "0" "herdr invoked cancel and it returned 0"

# ---------------------------------------------------------------------------
# Step 12: what herdr logged
#
# The interesting field is `status`, not the fact that a line exists: herdr keeps
# one record per plugin command, and a `cancel` that ran but failed is recorded
# just as faithfully as one that worked. The daemon's own standard error reaches
# the same place, but only once the process has finished — while it runs, its
# record carries no stderr at all.
# ---------------------------------------------------------------------------

printf '\n== step: herdr-log ==\n'
set +e
timeout "${CLIENT_TIMEOUT}" herdr plugin log list --plugin "${PLUGIN_ID}" --limit 10 \
    >"${LOGS}/plugin-log.log" 2>&1
LOG_CODE="$?"
set -e
cat "${LOGS}/plugin-log.log"
if [ "${LOG_CODE}" != "0" ]; then
    fail "herdr-log" "${LOG_CODE}" "herdr plugin log list failed" \
        "read ${LOGS}/plugin-log.log"
fi
if ! grep -q '"action_id":"cancel"' "${LOGS}/plugin-log.log"; then
    fail "herdr-log" "0" "herdr logged no cancel invocation" \
        "the action returned 0 but herdr kept no record of it; compare with ${LOGS}/action.log"
fi
if ! grep -q '"status":"succeeded"' "${LOGS}/plugin-log.log"; then
    fail "herdr-log" "0" "herdr recorded the cancel invocation as something other than succeeded" \
        "read ${LOGS}/plugin-log.log; the status field is what herdr shows the person"
fi
LOG_NOTE="herdr recorded cancel as succeeded"
if grep -q '"event":"startup"' "${LOGS}/plugin-log.log"; then
    LOG_NOTE="${LOG_NOTE}; the [[startup]] daemon has a record too"
fi
record "herdr-log" "0" "${LOG_NOTE}"

# ---------------------------------------------------------------------------
# Step 13: no capture device
#
# This is the step the whole exercise is for. A container has no ALSA, no
# PipeWire and no microphone, which is the state a headless Linux machine is in
# and the state no macOS check can produce. The requirement is that the plugin
# names what is missing and exits: not a hang, not a panic, not silence.
#
# Capture is issue #8 and is not on main, so today the capture-facing commands
# exit 69, "not implemented yet". That is recorded as pending, never as a pass.
#
# ONCE ISSUE #8 LANDS, extend this step:
#   - invoke the `dictate` action through herdr rather than running `mic`, since
#     a take is driven by that action;
#   - require a non-zero exit, output naming the device or the host that is
#     missing, and no line matching "panicked at";
#   - drop 69 from the accepted codes, so "not built yet" stops passing here.
# ---------------------------------------------------------------------------

printf '\n== step: no-device ==\n'
DEVICE_STATE="no /dev/snd"
if [ -d /dev/snd ]; then
    DEVICE_STATE="/dev/snd exists: $(find /dev/snd -mindepth 1 -maxdepth 1 -printf '%f ' 2>/dev/null)"
fi
PIPEWIRE_STATE="no pipewire binary"
if command -v pipewire >/dev/null 2>&1; then
    PIPEWIRE_STATE="pipewire present"
fi
ARECORD_STATE="no arecord"
if command -v arecord >/dev/null 2>&1; then
    ARECORD_STATE="arecord: $(arecord -l 2>&1 | head -n 1)"
fi
printf 'devices:  %s\n' "${DEVICE_STATE}"
printf 'pipewire: %s\n' "${PIPEWIRE_STATE}"
printf 'alsa cli: %s\n' "${ARECORD_STATE}"

set +e
timeout "${CLIENT_TIMEOUT}" "${BIN}" mic >"${LOGS}/no-device.log" 2>&1
MIC_CODE="$?"
set -e
cat "${LOGS}/no-device.log"
if [ "${MIC_CODE}" = "124" ]; then
    fail "no-device" "${MIC_CODE}" "the capture path hung with no device present" \
        "a hang is the worst of the three outcomes: the plugin must name the missing device and exit"
fi
if [ "${MIC_CODE}" -ge 128 ]; then
    fail "no-device" "${MIC_CODE}" "the capture path died on a signal with no device present" \
        "read ${LOGS}/no-device.log; a panic or an abort here is a defect, not a report"
fi
if grep -q "panicked at" "${LOGS}/no-device.log"; then
    fail "no-device" "${MIC_CODE}" "the capture path panicked with no device present" \
        "read ${LOGS}/no-device.log; the absence of a device is an expected state and must be reported, not asserted against"
fi
if [ ! -s "${LOGS}/no-device.log" ]; then
    fail "no-device" "${MIC_CODE}" "the capture path printed nothing with no device present" \
        "silence is a defect of the same weight as a wrong transcript; the failure must name what is missing"
fi
if [ "${MIC_CODE}" = "69" ]; then
    record "no-device" "${MIC_CODE}" \
        "pending: capture is not built yet (issue #8); binary said so and exited, ${DEVICE_STATE}"
elif [ "${MIC_CODE}" = "0" ]; then
    fail "no-device" "${MIC_CODE}" "the capture path succeeded with no device present" \
        "there is no microphone in this container, so a success means the device was never opened"
else
    record "no-device" "${MIC_CODE}" \
        "named what is missing and exited, ${DEVICE_STATE}"
fi

# ---------------------------------------------------------------------------
# Step 14: a client with no daemon
#
# The other bounded failure, and the one the prototype got wrong: with nothing
# listening, the client must say where it tried to reach and how to start the
# daemon, then exit non-zero. A hang here looks exactly like a broken keybinding.
# ---------------------------------------------------------------------------

printf '\n== step: no-daemon ==\n'
if [ -n "${DAEMON_PID}" ]; then
    kill "${DAEMON_PID}" >/dev/null 2>&1 || true
    wait "${DAEMON_PID}" 2>/dev/null || true
    DAEMON_PID=""
else
    herdr plugin unlink "${PLUGIN_ID}" >/dev/null 2>&1 || true
    pkill -f "herdr-voice daemon" >/dev/null 2>&1 || true
fi
waited=0
while [ "${waited}" -lt 10 ]; do
    if ! daemon_is_up; then
        break
    fi
    sleep 1
    waited=$((waited + 1))
done
if daemon_is_up; then
    record "no-daemon" "-" "skipped: a daemon is still listening and could not be stopped"
else
    set +e
    timeout "${CLIENT_TIMEOUT}" "${BIN}" cancel >"${LOGS}/no-daemon.log" 2>&1
    CANCEL_CODE="$?"
    set -e
    cat "${LOGS}/no-daemon.log"
    if [ "${CANCEL_CODE}" = "124" ]; then
        fail "no-daemon" "${CANCEL_CODE}" "the client hung with no daemon listening" \
            "read ${LOGS}/no-daemon.log; the client waits two seconds for a reply and must then give up"
    fi
    if [ "${CANCEL_CODE}" = "0" ]; then
        fail "no-daemon" "${CANCEL_CODE}" "the client reported success with no daemon listening" \
            "nothing was listening, so a zero exit means the client never noticed"
    fi
    if [ ! -s "${LOGS}/no-daemon.log" ]; then
        fail "no-daemon" "${CANCEL_CODE}" "the client failed without saying anything" \
            "the message must name the socket it tried and how to start the daemon"
    fi
    record "no-daemon" "${CANCEL_CODE}" \
        "named the socket and exited: $(head -n 1 "${LOGS}/no-daemon.log" | cut -c 1-80)"
fi

# ---------------------------------------------------------------------------
# Done
# ---------------------------------------------------------------------------

print_report
printf '\nAll steps ran. Copy the table and the versions above into docs/evidence.md.\n'
