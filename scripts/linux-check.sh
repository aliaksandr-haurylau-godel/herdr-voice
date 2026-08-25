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
#   HERDR_VOICE_REVISION  the revision to record when the checkout is not a git
#                         one — a worktree mounted into a container has its `.git`
#                         pointing outside the mount, so `git rev-parse` fails there
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
    # Every daemon, not only one this script started. herdr starts one from the
    # manifest's [[startup]] entry, and a run that leaves it behind poisons the
    # next one: the second run finds a daemon that is on its way out, connects to
    # it, and gets a closed connection instead of a reply.
    pkill -f 'herdr-voice daemon' >/dev/null 2>&1 || true
    if [ -n "${SERVER_PID:-}" ]; then
        herdr server stop >/dev/null 2>&1 || true
        kill "${SERVER_PID}" >/dev/null 2>&1 || true
    fi
}
trap cleanup EXIT

# ---------------------------------------------------------------------------
# What herdr recorded about a plugin command
#
# `herdr plugin action invoke` returns 0 for "the action was started", never the
# plugin's own exit code: the record herdr keeps is the only place that code and
# the plugin's standard error appear. So an assertion about what a command did is
# an assertion about that record, and a record that never leaves `running` is the
# hang.
# ---------------------------------------------------------------------------

# The id of the newest record herdr holds for an action, or nothing.
newest_log_id() {
    herdr plugin log list --plugin "${PLUGIN_ID}" --limit 30 2>/dev/null \
        | jq -r --arg id "$1" \
            '[.result.logs[] | select(.action_id == $id)] | last | .log_id // empty'
}

# Wait for a record that is newer than the one named and has finished. Prints it
# as one JSON object on success; prints nothing and returns 1 on a timeout.
await_new_log() {
    # action id, the log id to ignore, seconds to wait
    _waited=0
    while [ "${_waited}" -lt "$3" ]; do
        _record="$(herdr plugin log list --plugin "${PLUGIN_ID}" --limit 30 2>/dev/null \
            | jq -c --arg id "$1" --arg seen "$2" \
                '[.result.logs[]
                  | select(.action_id == $id)
                  | select(.log_id != $seen)
                  | select(.status != "running")] | last // empty')"
        if [ -n "${_record}" ]; then
            printf '%s\n' "${_record}"
            return 0
        fi
        sleep 1
        _waited=$((_waited + 1))
    done
    return 1
}

field() {
    # a record, a jq path
    printf '%s' "$1" | jq -r "$2 // empty"
}

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
        procps coreutils jq >"${LOGS}/packages.log" 2>&1
elif command -v dnf >/dev/null 2>&1; then
    PM="dnf"
    dnf install -y -q \
        ca-certificates curl git gcc gcc-c++ make pkgconf-pkg-config \
        alsa-lib-devel procps-ng coreutils jq >"${LOGS}/packages.log" 2>&1
elif command -v apk >/dev/null 2>&1; then
    PM="apk"
    apk add --no-cache \
        ca-certificates curl git build-base pkgconf alsa-lib-dev \
        procps coreutils jq >"${LOGS}/packages.log" 2>&1
elif command -v pacman >/dev/null 2>&1; then
    PM="pacman"
    pacman -Sy --noconfirm --needed \
        ca-certificates curl git base-devel pkgconf alsa-lib procps-ng jq \
        >"${LOGS}/packages.log" 2>&1
else
    skip_as_failure "packages" \
        "no apt-get, dnf, apk or pacman on this image" \
        "run this on an image with one of those package managers, or install curl, git, a C toolchain, pkg-config and the ALSA development headers by hand first"
fi

for tool in curl git cc pkg-config timeout jq; do
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
REVISION="$(git -C "${ROOT}" rev-parse --short HEAD 2>/dev/null || echo '')"
if [ -z "${REVISION}" ]; then
    # A worktree mounted into a container keeps its `.git` outside the mount, so
    # git cannot answer here. The revision still has to reach the report.
    REVISION="${HERDR_VOICE_REVISION:-not a git checkout}"
fi
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

# ---------------------------------------------------------------------------
# Step 8b: a focused pane
#
# `dictate` delivers into a pane and refuses to start a take without one, so a
# server with no workspace at all can only ever produce "the invocation context
# names no focused pane" — which says nothing about audio. One workspace is
# created here so that the capture steps below are about capture.
# ---------------------------------------------------------------------------

printf '\n== step: pane ==\n'
focused_pane() {
    herdr pane current 2>/dev/null | jq -r '.result.pane.pane_id // empty'
}
PANE="$(focused_pane)"
if [ -z "${PANE}" ]; then
    herdr workspace create --focus --cwd "${WORK}" --label check \
        >"${LOGS}/workspace.log" 2>&1 || true
    waited=0
    while [ "${waited}" -lt "${START_TIMEOUT}" ]; do
        PANE="$(focused_pane)"
        if [ -n "${PANE}" ]; then
            break
        fi
        sleep 1
        waited=$((waited + 1))
    done
fi
if [ -z "${PANE}" ]; then
    fail "pane" "-" "herdr has no focused pane after creating a workspace" \
        "read ${LOGS}/workspace.log; without a pane the dictate action cannot be exercised at all"
fi
printf 'focused pane: %s\n' "${PANE}"
record "pane" "0" "herdr names the focused pane ${PANE}"

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
# just as faithfully as one that worked. herdr writes the record when the command
# ends, not when it starts, so the record has to be waited for — reading the log
# the instant the action returns finds it still `running`.
# ---------------------------------------------------------------------------

printf '\n== step: herdr-log ==\n'
CANCEL_RECORD="$(await_new_log "cancel" "" "${START_TIMEOUT}" || true)"
herdr plugin log list --plugin "${PLUGIN_ID}" --limit 10 \
    >"${LOGS}/plugin-log.log" 2>&1 || true
cat "${LOGS}/plugin-log.log"
if [ -z "${CANCEL_RECORD}" ]; then
    fail "herdr-log" "-" "herdr kept no finished record of the cancel invocation" \
        "the action returned 0; compare ${LOGS}/action.log with ${LOGS}/plugin-log.log"
fi
CANCEL_STATUS="$(field "${CANCEL_RECORD}" '.status')"
CANCEL_EXIT="$(field "${CANCEL_RECORD}" '.exit_code')"
if [ "${CANCEL_STATUS}" != "succeeded" ]; then
    fail "herdr-log" "${CANCEL_EXIT}" \
        "herdr recorded the cancel invocation as ${CANCEL_STATUS}, not succeeded" \
        "read ${LOGS}/plugin-log.log; the status field is what herdr shows the person"
fi
LOG_NOTE="herdr recorded cancel as succeeded with exit_code ${CANCEL_EXIT}"
if grep -q '"event":"startup"' "${LOGS}/plugin-log.log"; then
    LOG_NOTE="${LOG_NOTE}; the [[startup]] daemon has a record too"
fi
record "herdr-log" "0" "${LOG_NOTE}"

# ---------------------------------------------------------------------------
# Step 13: a take with no capture device
#
# This is the step the whole exercise is for. A container has no sound hardware,
# which is the state a headless Linux machine is in and the state no macOS check
# can produce. A take is driven the way a person drives one — the `dictate`
# action, through herdr — and the requirement is that it names what is missing
# and exits: not a hang, not a panic, not silence, and not a success.
#
# The plugin's own exit code is not what `herdr plugin action invoke` returns.
# That command returns 0 for "the action was started" and herdr writes the code
# and the standard error into its plugin log when the command ends, so every
# assertion below reads that record.
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

# Judges one finished plugin-log record of a take that had no device to open.
# Every branch that is not "named it and exited non-zero" is a failure. It is
# called as a plain command and never inside `$(...)`: `fail` ends the run, and a
# subshell would swallow both the exit and the report. What it wants to hand back
# — the first line the plugin printed — goes into TAKE_SAID.
TAKE_SAID=""
judge_take() {
    # step name, the record
    _step="$1"
    _record="$2"
    _code="$(field "${_record}" '.exit_code')"
    _stderr="$(field "${_record}" '.stderr')"
    _stdout="$(field "${_record}" '.stdout')"
    printf 'exit_code: %s\nstderr: %s\nstdout: %s\n' "${_code}" "${_stderr}" "${_stdout}"
    case "${_code}" in
        ''|*[!0-9]*)
            fail "${_step}" "${_code:--}" "herdr recorded no usable exit code for the take" \
                "read ${LOGS}/${_step}-record.json"
            ;;
    esac
    if [ "${_code}" -ge 128 ]; then
        fail "${_step}" "${_code}" "the take died on a signal with no device present" \
            "a panic or an abort here is a defect, not a report; read ${LOGS}/${_step}-record.json"
    fi
    case "${_stderr}${_stdout}" in
        *"panicked at"*)
            fail "${_step}" "${_code}" "the take panicked with no device present" \
                "the absence of a device is an expected state and must be reported, not asserted against"
            ;;
    esac
    if [ "${_code}" = "0" ]; then
        fail "${_step}" "${_code}" "the take succeeded with no device present" \
            "there is no microphone on this machine, so a success means the device was never opened"
    fi
    if [ "${_code}" = "69" ]; then
        fail "${_step}" "${_code}" "the take reported 'not implemented yet'" \
            "capture has landed; a command that still answers 69 here is a regression, and it is not a pass"
    fi
    if [ -z "${_stderr}" ]; then
        fail "${_step}" "${_code}" "the take failed without saying anything" \
            "silence is a defect of the same weight as a wrong transcript; the failure must name what is missing"
    fi
    case "${_stderr}" in
        *device*|*Device*|*input*)
            ;;
        *)
            fail "${_step}" "${_code}" "the failure does not name the input it could not open" \
                "the message is what the person acts on: read ${LOGS}/${_step}-record.json"
            ;;
    esac
    TAKE_SAID="$(printf '%s' "${_stderr}" | head -n 1 | cut -c 1-120)"
}

SEEN_DICTATE="$(newest_log_id 'dictate')"
set +e
timeout "${CLIENT_TIMEOUT}" herdr plugin action invoke dictate --plugin "${PLUGIN_ID}" \
    >"${LOGS}/no-device.log" 2>&1
INVOKE_CODE="$?"
set -e
cat "${LOGS}/no-device.log"
if [ "${INVOKE_CODE}" = "124" ]; then
    fail "no-device" "${INVOKE_CODE}" "herdr did not return from invoking dictate within ${CLIENT_TIMEOUT}s" \
        "read ${LOGS}/no-device.log"
fi
NO_DEVICE_RECORD="$(await_new_log "dictate" "${SEEN_DICTATE}" "${CLIENT_TIMEOUT}" || true)"
if [ -z "${NO_DEVICE_RECORD}" ]; then
    fail "no-device" "-" "the dictate take never finished within ${CLIENT_TIMEOUT}s" \
        "a hang is the worst of the outcomes: the plugin must name the missing device and exit. The record is still 'running' in herdr plugin log list"
fi
printf '%s\n' "${NO_DEVICE_RECORD}" >"${LOGS}/no-device-record.json"
judge_take "no-device" "${NO_DEVICE_RECORD}"
record "no-device" "$(field "${NO_DEVICE_RECORD}" '.exit_code')" \
    "named what is missing and exited, ${DEVICE_STATE}: ${TAKE_SAID}"

# ---------------------------------------------------------------------------
# Step 13b: a configured input that no device answers to
#
# The other half of device selection, and the one that decides where a recording
# comes from: a name in `[audio] input` that matches nothing must be refused with
# the names that do exist, never quietly replaced by the default. A fall back is
# silent, and a recording from an input nobody speaks into is the failure the
# whole "by name, never by index" rule was written against.
#
# The configuration is read once when the daemon starts, so the daemon is
# restarted here by hand. From this point on the daemon is this script's, not
# herdr's, which is also what lets the last step take it away again.
# ---------------------------------------------------------------------------

printf '\n== step: named-device ==\n'
WRONG_NAME="No Such Microphone $$"
if [ "${CONFIG_DIR}" = "not printed" ] || [ -z "${CONFIG_DIR}" ]; then
    skip_as_failure "named-device" \
        "herdr did not print a config directory at the link step" \
        "read ${LOGS}/link.log; without the directory there is nowhere to put config.toml"
fi
mkdir -p "${CONFIG_DIR}"
CONFIG_FILE="${CONFIG_DIR}/${PLUGIN_ID}-config-backup"
if [ -f "${CONFIG_DIR}/config.toml" ]; then
    cp "${CONFIG_DIR}/config.toml" "${CONFIG_FILE}"
fi
printf '[audio]\ninput = "%s"\n' "${WRONG_NAME}" >"${CONFIG_DIR}/config.toml"
printf 'wrote %s with [audio] input = %s\n' "${CONFIG_DIR}/config.toml" "\"${WRONG_NAME}\""

pkill -f 'herdr-voice daemon' >/dev/null 2>&1 || true
waited=0
while [ "${waited}" -lt 10 ]; do
    if ! daemon_is_up; then
        break
    fi
    sleep 1
    waited=$((waited + 1))
done
"${BIN}" daemon >"${LOGS}/daemon-named.log" 2>&1 &
DAEMON_PID="$!"
waited=0
while [ "${waited}" -lt "${START_TIMEOUT}" ]; do
    if daemon_is_up; then
        break
    fi
    sleep 1
    waited=$((waited + 1))
done
if ! daemon_is_up; then
    tail -n 20 "${LOGS}/daemon-named.log" >&2 || true
    fail "named-device" "-" "no daemon came back after writing config.toml" \
        "read ${LOGS}/daemon-named.log; a configuration file that does not parse is the usual cause"
fi

SEEN_DICTATE="$(newest_log_id 'dictate')"
set +e
timeout "${CLIENT_TIMEOUT}" herdr plugin action invoke dictate --plugin "${PLUGIN_ID}" \
    >"${LOGS}/named-device.log" 2>&1
INVOKE_CODE="$?"
set -e
cat "${LOGS}/named-device.log"
NAMED_RECORD="$(await_new_log "dictate" "${SEEN_DICTATE}" "${CLIENT_TIMEOUT}" || true)"
if [ -z "${NAMED_RECORD}" ]; then
    fail "named-device" "-" "the dictate take never finished within ${CLIENT_TIMEOUT}s" \
        "a configured name that matches nothing must be refused at once; the record is still 'running'"
fi
printf '%s\n' "${NAMED_RECORD}" >"${LOGS}/named-device-record.json"
judge_take "named-device" "${NAMED_RECORD}"
NAMED_STDERR="$(field "${NAMED_RECORD}" '.stderr')"
case "${NAMED_STDERR}" in
    *"${WRONG_NAME}"*) ;;
    *)
        fail "named-device" "$(field "${NAMED_RECORD}" '.exit_code')" \
            "the refusal does not repeat the name that was configured" \
            "read ${LOGS}/named-device-record.json; a person has to see which name was not found"
        ;;
esac
case "${NAMED_STDERR}" in
    *"the ones that exist are"*|*"no input devices"*) ;;
    *)
        fail "named-device" "$(field "${NAMED_RECORD}" '.exit_code')" \
            "the refusal does not say what names do exist" \
            "read ${LOGS}/named-device-record.json; the list is what turns the refusal into an action"
        ;;
esac
record "named-device" "$(field "${NAMED_RECORD}" '.exit_code')" \
    "refused and listed what exists: ${TAKE_SAID}"

# Put the configuration back the way it was found, so a re-run starts clean.
rm -f "${CONFIG_DIR}/config.toml"
if [ -f "${CONFIG_FILE}" ]; then
    mv "${CONFIG_FILE}" "${CONFIG_DIR}/config.toml"
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
