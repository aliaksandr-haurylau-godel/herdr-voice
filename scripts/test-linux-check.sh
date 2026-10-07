#!/bin/sh
#
# Runs the whole of scripts/linux-check.sh against stub commands, one case for each
# way it used to report a pass it had not earned or lose what it had borrowed.
#
# There is no container here and no herdr, and the script's mistakes were in its
# decisions, not in what herdr does: which record it accepts, when it puts the
# configuration back, what it does about a daemon that will not stop. So the
# script runs as it is, against stubs that answer the way herdr, the plugin and the
# package tools do, and each case sets one stub to misbehave.
#
# What this does not show is a Linux run: no container, no real herdr, no real
# daemon. docs/evidence.md says so.
#
# The script under test runs `pkill -f 'herdr-voice daemon'`. A machine that has a
# daemon of its own running matches that pattern. The stub directory is first on
# PATH and holds a `pkill` that signals nothing, and the test refuses to start if
# that is not what `pkill` resolves to. Everything else the test kills is a pid a
# stub recorded for itself.

set -eu

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SCRIPT="${ROOT}/scripts/linux-check.sh"
BASH_UNDER_TEST="${HERDR_VOICE_TEST_BASH:-bash}"
CASE_LIMIT=60
SCRATCH="$(mktemp -d)"
STUBS="${SCRATCH}/stubs"
failures=0

case "${SCRATCH}" in
    *[[:space:]]*)
        printf 'FAIL  the scratch directory %s has whitespace in it; set TMPDIR to one that has none\n' "${SCRATCH}"
        exit 1
        ;;
esac

teardown() {
    for pids in "${SCRATCH}"/*/state/daemon.pids; do
        [ -f "${pids}" ] || continue
        while read -r pid; do
            kill -9 "${pid}" 2>/dev/null || true
        done <"${pids}"
    done
    rm -rf "${SCRATCH}"
}
trap teardown EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

ok() {
    printf 'ok    %s\n' "$1"
}

bad() {
    printf 'FAIL  %s: %s\n' "$1" "$2"
    failures=$((failures + 1))
}

# ---------------------------------------------------------------------------
# The stubs
# ---------------------------------------------------------------------------

mkdir "${STUBS}"

# herdr: answers what the script asks, keeps a JSON-lines log of plugin commands.
cat >"${STUBS}/herdr" <<'STUB'
#!/bin/sh
S="${STUB_STATE:?}"
CFG="${STUB_CONFIG_DIR:?}/config.toml"
LOG="${S}/logs.jsonl"
case "$1" in
    --version) echo "herdr 0.0.0-stub"; exit 0 ;;
    status) echo '{"running":true}'; exit 0 ;;
    pane) echo '{"result":{"pane":{"pane_id":"w1:p1"}}}'; exit 0 ;;
    plugin) ;;
    *) exit 0 ;;
esac
case "$2" in
    list) echo "herdr-voice  linked"; exit 0 ;;
    config-dir) printf '%s\n' "${STUB_CONFIG_DIR}"; exit 0 ;;
    log)
        if [ -n "${STUB_LOG_FAIL:-}" ]; then
            echo "stub: the plugin log cannot be read" >&2
            exit 1
        fi
        limit=30
        while [ $# -gt 0 ]; do
            if [ "$1" = "--limit" ]; then limit="$2"; fi
            shift
        done
        # A record whose delay has run out finishes on this call.
        for pending in "${S}"/pending.*; do
            [ -f "${pending}" ] || continue
            read -r left final <"${pending}"
            id="${pending##*pending.}"
            left=$((left - 1))
            if [ "${left}" -le 0 ]; then
                jq -c --arg id "${id}" --arg st "${final}" \
                    'if .log_id == $id then .status = $st else . end' "${LOG}" >"${LOG}.new"
                mv "${LOG}.new" "${LOG}"
                rm -f "${pending}"
            else
                echo "${left} ${final}" >"${pending}"
            fi
        done
        if [ -s "${LOG}" ]; then
            jq -sc --argjson n "${limit}" '{result: {logs: .[-$n:]}}' "${LOG}"
        else
            echo '{"result":{"logs":[]}}'
        fi
        exit 0
        ;;
    action)
        action="$4"
        if [ "${action}" = "dictate" ] && grep -q 'No Such Microphone' "${CFG}" 2>/dev/null; then
            if [ -n "${STUB_NAMED_INVOKE_EXIT:-}" ]; then exit "${STUB_NAMED_INVOKE_EXIT}"; fi
            if [ -n "${STUB_NAMED_BLOCK:-}" ]; then
                : >"${S}/named_started"
                i=0
                while [ "${i}" -lt 60 ]; do sleep 1; i=$((i + 1)); done
            fi
        fi
        n="$(cat "${S}/counter" 2>/dev/null || echo "${STUB_ID_START:-100}")"
        n=$((n + 1))
        echo "${n}" >"${S}/counter"
        code=0
        err=""
        if [ "${action}" = "dictate" ]; then
            code=1
            if grep -q 'No Such Microphone' "${CFG}" 2>/dev/null; then
                name="$(sed -n 's/^input = "\(.*\)"$/\1/p' "${CFG}")"
                if [ -n "${STUB_REFUSAL_OMITS_NAME:-}" ]; then
                    err='no input device with that name; the ones that exist are "Stub null device". Set [audio] input to one of them'
                else
                    err="no input device named \"${name}\"; the ones that exist are \"Stub null device\". Set [audio] input to one of them, or leave it empty for the default"
                fi
            elif [ -n "${STUB_TAB:-}" ]; then
                err="cannot read what \"Default Audio Device\" supports:$(printf '\t')no device (stub)"
            else
                err='cannot read what "Default Audio Device" supports: no device (stub)'
            fi
        fi
        final=succeeded
        if [ "${code}" != "0" ]; then final=failed; fi
        st="${final}"
        if [ "${STUB_DELAY:-0}" -gt 0 ]; then st=running; fi
        jq -cn --arg id "log-${n}" --arg a "${action}" --arg st "${st}" \
            --argjson code "${code}" --arg err "${err}" \
            '{log_id: $id, action_id: $a, status: $st, exit_code: $code, stderr: $err, stdout: ""}' >>"${LOG}"
        if [ "${st}" = "running" ]; then
            echo "${STUB_DELAY} ${final}" >"${S}/pending.log-${n}"
        fi
        echo '{"result":{"started":true}}'
        exit 0
        ;;
esac
exit 0
STUB

# The plugin binary the script drives: doctor, daemon, cancel.
cat >"${STUBS}/herdr-voice-bin" <<'STUB'
#!/bin/sh
S="${STUB_STATE:?}"
case "$1" in
    --version) echo "herdr-voice 0.0.0-stub" ;;
    doctor)
        if [ -f "${S}/daemon_up" ]; then
            d="daemon         ok       listening at stub-socket"
        else
            d="daemon         missing  nothing is listening at stub-socket"
        fi
        printf 'herdr          ok       found stub\n%s\nconfig         default  none\nmodel          missing  none\nrewrite        missing  none\n' "${d}"
        exit 1
        ;;
    daemon)
        : >"${S}/daemon_up"
        echo "$$" >>"${S}/daemon.pids"
        if [ -n "${STUB_DAEMON_IGNORES_TERM:-}" ]; then
            trap '' TERM
        else
            trap 'rm -f "${S}/daemon_up"; exit 0' TERM
        fi
        while :; do sleep 1; done
        ;;
    cancel)
        if [ -f "${S}/daemon_up" ]; then
            echo "nothing to cancel"
            exit 0
        fi
        echo "cannot reach the daemon at stub-socket; start it with herdr-voice daemon" >&2
        exit 1
        ;;
esac
STUB

# pkill: signals nothing. It stands in for the daemon dying.
cat >"${STUBS}/pkill" <<'STUB'
#!/bin/sh
echo "pkill $*" >>"${STUB_STATE:?}/pkill.log"
if [ -z "${STUB_DAEMON_IMMORTAL:-}" ]; then rm -f "${STUB_STATE}/daemon_up"; fi
exit 0
STUB

# timeout: macOS has none. Drops the duration and runs the command.
cat >"${STUBS}/timeout" <<'STUB'
#!/bin/sh
shift
exec "$@"
STUB

# The package and toolchain commands: answer, do nothing.
cat >"${STUBS}/apt-get" <<'STUB'
#!/bin/sh
exit 0
STUB
cat >"${STUBS}/pkg-config" <<'STUB'
#!/bin/sh
exit 1
STUB
for tool in cc rustc cargo; do
    cat >"${STUBS}/${tool}" <<'STUB'
#!/bin/sh
echo "$(basename "$0") 0.0.0-stub"
exit 0
STUB
done
chmod +x "${STUBS}"/*

if [ "$(PATH="${STUBS}:/usr/bin:/bin" command -v pkill)" != "${STUBS}/pkill" ]; then
    printf 'FAIL  the stub pkill is not the one that resolves; refusing to run a script that signals processes by name\n'
    exit 1
fi
for tool in jq perl bash python3; do
    if ! command -v "${tool}" >/dev/null 2>&1; then
        printf 'FAIL  %s is needed to run this test and is not on PATH\n' "${tool}"
        exit 1
    fi
done

# ---------------------------------------------------------------------------
# One case
# ---------------------------------------------------------------------------

CASE=""
code=0

new_case() {
    CASE="${SCRATCH}/$1"
    mkdir -p "${CASE}/home" "${CASE}/work" "${CASE}/state" "${CASE}/cfg" "${CASE}/root/target/release"
    # herdr started the daemon itself, through the manifest's [[startup]] entry.
    : >"${CASE}/state/daemon_up"
    : >"${CASE}/root/herdr-plugin.toml"
    cp "${STUBS}/herdr-voice-bin" "${CASE}/root/target/release/herdr-voice"
}

case_env() {
    ENVV="PATH=${STUBS}:/usr/bin:/bin HOME=${CASE}/home HERDR_VOICE_CHECKOUT=${CASE}/root"
    ENVV="${ENVV} HERDR_VOICE_WORK=${CASE}/work HERDR_VOICE_STOP_TIMEOUT=2 HERDR_VOICE_REVISION=stub"
    ENVV="${ENVV} STUB_STATE=${CASE}/state STUB_CONFIG_DIR=${CASE}/cfg"
}

# Runs the script under a limit. A script that hangs ends with exit 142 (SIGALRM)
# and the case reports it, instead of the test never returning. The remaining
# arguments are KEY=VALUE for the stubs.
run_case() {
    case_env
    code=0
    # shellcheck disable=SC2086
    env -i ${ENVV} "$@" perl -e 'alarm shift; exec @ARGV' "${CASE_LIMIT}" \
        "${BASH_UNDER_TEST}" "${SCRIPT}" >"${CASE}/out" 2>"${CASE}/err" || code=$?
}

# As run_case, but sends SIGINT to the script's process group once the stub says
# the named-device step is blocked, and waits at most 30 seconds for it to end.
run_case_interrupted() {
    case_env
    code=0
    # shellcheck disable=SC2086
    python3 -c '
import os, signal, subprocess, sys, time
marker, out, err = sys.argv[1:4]
command = sys.argv[4:]
child = subprocess.Popen(command, stdout=open(out, "w"), stderr=open(err, "w"),
                         start_new_session=True)
deadline = time.time() + 30
while time.time() < deadline and not os.path.exists(marker) and child.poll() is None:
    time.sleep(0.2)
if child.poll() is None and os.path.exists(marker):
    os.killpg(child.pid, signal.SIGINT)
try:
    child.wait(timeout=30)
except subprocess.TimeoutExpired:
    os.killpg(child.pid, signal.SIGKILL)
    child.wait()
    sys.exit(142)
sys.exit(child.returncode if child.returncode >= 0 else 128 - child.returncode)
' "${CASE}/state/named_started" "${CASE}/out" "${CASE}/err" \
        env -i ${ENVV} "$@" "${BASH_UNDER_TEST}" "${SCRIPT}" || code=$?
}

# A finished record already in herdr's log, as an earlier run would have left it.
seed_record() {
    # id, action, status, exit code, stderr
    jq -cn --arg id "$1" --arg a "$2" --arg st "$3" --argjson code "$4" --arg err "$5" \
        '{log_id: $id, action_id: $a, status: $st, exit_code: $code, stderr: $err, stdout: ""}' \
        >>"${CASE}/state/logs.jsonl"
}

# The configuration an interrupted earlier run leaves behind.
borrowed_config() {
    printf '[audio]\ninput = "No Such Microphone 4242"\n' >"${CASE}/cfg/config.toml"
}

row() {
    awk -F'\t' -v step="$1" '$1 == step' "${CASE}/work/report.tsv"
}

expect_code() {
    # case, wanted exit code
    if [ "${code}" -eq 142 ]; then
        bad "$1" "hit the ${CASE_LIMIT}-second limit"
    elif [ "${code}" -eq "$2" ]; then
        ok "$1: exit $2"
    else
        bad "$1" "exit ${code}, expected $2"
    fi
}

expect_text() {
    # case, what, the text, the file
    if grep -F -q -- "$3" "$4"; then ok "$1: $2"; else bad "$1" "$2 — no '$3' in $4"; fi
}

expect_no_text() {
    if grep -F -q -- "$3" "$4"; then bad "$1" "$2 — found '$3' in $4"; else ok "$1: $2"; fi
}

expect_true() {
    # case, what, then a test command
    name="$1"
    what="$2"
    shift 2
    if "$@"; then ok "${name}: ${what}"; else bad "${name}" "${what}"; fi
}

content_is() {
    [ "$(cat "$1" 2>/dev/null)" = "$2" ]
}

absent() {
    [ ! -e "$1" ]
}

# ---------------------------------------------------------------------------
# The cases
# ---------------------------------------------------------------------------

new_case clean
run_case
expect_code clean 0
expect_text clean "the run says it finished" "All steps ran" "${CASE}/out"
expect_true clean "the report has the last step" test -n "$(row no-daemon)"
expect_true clean "the stub pkill was the one used" test -s "${CASE}/state/pkill.log"

# An id that is a substring of an older id. Fresh ids start at log-101, which is
# inside log-1010. Guards the exact comparison; passes on the script as it was.
new_case substring-id
seed_record log-1010 dictate failed 1 "an earlier run: no device"
run_case
expect_code substring-id 0

new_case stale-dictate
seed_record log-1 dictate failed 1 "STALE one: no input device"
seed_record log-2 dictate failed 1 "STALE two: no input device"
run_case STUB_DELAY=2
expect_code stale-dictate 0
expect_true stale-dictate "the no-device row is not an earlier run's record" test -n "$(row no-device)"
expect_no_text stale-dictate "no earlier record was graded" "STALE" "${CASE}/work/report.tsv"

new_case stale-cancel
seed_record log-1 cancel failed 99 "an earlier run's cancel"
run_case STUB_DELAY=2
expect_code stale-cancel 0
expect_text stale-cancel "the herdr-log row is this run's" "exit_code 0" "${CASE}/work/report.tsv"

new_case unreadable-log
run_case STUB_LOG_FAIL=1
expect_code unreadable-log 1
expect_text unreadable-log "the failure says the log could not be read" "could not read herdr's plugin log" "${CASE}/err"

new_case config-restored
printf 'ORIGINAL\n' >"${CASE}/cfg/config.toml"
run_case STUB_REFUSAL_OMITS_NAME=1
expect_code config-restored 1
expect_true config-restored "config.toml is the original" content_is "${CASE}/cfg/config.toml" ORIGINAL
expect_true config-restored "no backup is left" absent "${CASE}/cfg/herdr-voice-config-backup"
expect_true config-restored "no marker is left" absent "${CASE}/cfg/herdr-voice-config-was-absent"

new_case config-absent
run_case STUB_REFUSAL_OMITS_NAME=1
expect_code config-absent 1
expect_true config-absent "there is no config.toml" absent "${CASE}/cfg/config.toml"
expect_true config-absent "no marker is left" absent "${CASE}/cfg/herdr-voice-config-was-absent"

new_case interrupt
printf 'ORIGINAL\n' >"${CASE}/cfg/config.toml"
run_case_interrupted STUB_NAMED_BLOCK=1
expect_code interrupt 130
expect_true interrupt "config.toml is the original" content_is "${CASE}/cfg/config.toml" ORIGINAL
expect_true interrupt "no backup is left" absent "${CASE}/cfg/herdr-voice-config-backup"

new_case leftover-backup
printf 'ORIGINAL\n' >"${CASE}/cfg/herdr-voice-config-backup"
borrowed_config
run_case
expect_code leftover-backup 0
expect_true leftover-backup "config.toml is the backup's content" content_is "${CASE}/cfg/config.toml" ORIGINAL
expect_true leftover-backup "no backup is left" absent "${CASE}/cfg/herdr-voice-config-backup"
expect_text leftover-backup "the script says it restored" "restored" "${CASE}/out"

new_case leftover-marker
: >"${CASE}/cfg/herdr-voice-config-was-absent"
borrowed_config
run_case
expect_code leftover-marker 0
expect_true leftover-marker "there is no config.toml" absent "${CASE}/cfg/config.toml"
expect_true leftover-marker "no marker is left" absent "${CASE}/cfg/herdr-voice-config-was-absent"

new_case daemon-will-not-stop
run_case STUB_DAEMON_IMMORTAL=1
expect_code daemon-will-not-stop 1
expect_text daemon-will-not-stop "the message names the daemon still answering" "still answering" "${CASE}/err"
expect_true daemon-will-not-stop "no daemon was started over it" absent "${CASE}/state/daemon.pids"
if [ -f "${CASE}/work/report.tsv" ]; then
    expect_no_text daemon-will-not-stop "no step is recorded as skipped" "skipped" "${CASE}/work/report.tsv"
fi

new_case daemon-still-up-at-the-end
run_case STUB_DAEMON_IGNORES_TERM=1
expect_code daemon-still-up-at-the-end 1
expect_text daemon-still-up-at-the-end "the message names the daemon still answering" "still answering" "${CASE}/err"
expect_no_text daemon-still-up-at-the-end "no step is recorded as skipped" "skipped" "${CASE}/work/report.tsv"

new_case hung-invoke
run_case STUB_NAMED_INVOKE_EXIT=124
expect_code hung-invoke 1
expect_text hung-invoke "the failure is herdr not returning" "herdr did not return from invoking dictate" "${CASE}/err"
expect_no_text hung-invoke "the plugin is not blamed" "never finished" "${CASE}/err"

new_case tab
run_case STUB_TAB=1
expect_code tab 0
expect_true tab "every report line has three fields" \
    awk -F'\t' 'NF != 3 { bad = 1 } END { exit bad }' "${CASE}/work/report.tsv"

if [ "${failures}" -ne 0 ]; then
    printf '\n%s case(s) failed\n' "${failures}"
    exit 1
fi
printf '\nall cases passed\n'
