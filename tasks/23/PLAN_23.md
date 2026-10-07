# PLAN_23

Five tasks. Names, messages and the stub interface are those in `DESIGN_23.md`.
The work is tests first, and the tests are cases of one script, so task 1 builds the
harness and all cases, and tasks 2 to 4 each turn the failing cases green with one
group of edits. No Rust changes, so the Windows dead-code check does not apply;
`cargo` is not run.

Checks before each commit, from the worktree root:

```sh
sh scripts/test-linux-check.sh                # after task 1
bash -n scripts/linux-check.sh
shellcheck scripts/linux-check.sh scripts/test-linux-check.sh   # if shellcheck is installed; record that it was or was not
python3 scripts/check_manifest.py
sh scripts/test-pre-commit.sh
```

and grep every file written for `<new_string>`, `</new_string>`, `<old_string>`,
`</old_string>` and line-start conflict markers. The test script is run under the
`bash` on `PATH` and also under `/bin/bash` (`HERDR_VOICE_TEST_BASH=/bin/bash sh
scripts/test-linux-check.sh`); on macOS that is bash 3.2, and the script must work
under it.

## Task 1 — the harness and every case — depends on nothing

Create `scripts/test-linux-check.sh` with exactly the content below, `chmod +x`, and
run it against the unmodified `scripts/linux-check.sh`. Expected result on the
unmodified script: the cases `clean` and `substring id` pass; every other case
prints a `FAIL` line; the run ends non-zero and prints a summary. Record the
`ok`/`FAIL` lines in `RUN_23.md`.

```sh
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
```

## Task 2 — records (requirement 1) — depends on task 1

Edit `scripts/linux-check.sh`. The cases `stale-dictate`, `stale-cancel` and
`unreadable-log` must pass afterwards, `substring-id` must still pass, and no case
that passed before may fail.

1. Replace the function `newest_log_id` and its comment with `log_ids`, exactly:

   ```bash
   # Every log id herdr holds for the plugin now, as a JSON array. Fails when herdr
   # cannot be asked, or does not answer the shape the script relies on.
   log_ids() {
       herdr plugin log list --plugin "${PLUGIN_ID}" --limit 30 2>/dev/null \
           | jq -ce '[.result.logs[].log_id]'
   }
   ```

2. Replace `await_new_log` with this (same header comment, reworded for the set):

   ```bash
   # Wait for a record that is not in the set of ids taken before the invocation and
   # has finished. Prints it as one JSON object on success; prints nothing and
   # returns 1 on a timeout.
   await_new_log() {
       # action id, a JSON array of the log ids to ignore, seconds to wait
       _waited=0
       while [ "${_waited}" -lt "$3" ]; do
           _record="$(herdr plugin log list --plugin "${PLUGIN_ID}" --limit 30 2>/dev/null \
               | jq -c --arg id "$1" --argjson seen "$2" \
                   '[.result.logs[]
                     | select(.action_id == $id)
                     | select(.log_id as $i | $seen | any(. == $i) | not)
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
   ```

3. In the `action` step, before the `timeout … herdr plugin action invoke cancel`
   command, add
   `SEEN_LOGS="$(log_ids)" || fail "action" "-" "could not read herdr's plugin log before invoking cancel" "run 'herdr plugin log list --plugin ${PLUGIN_ID} --limit 30' by hand; the check compares records before and after each invocation and cannot go on without the first list"`.
   In the `herdr-log` step change `await_new_log "cancel" "" "${START_TIMEOUT}"` to
   `await_new_log "cancel" "${SEEN_LOGS}" "${START_TIMEOUT}"`.
4. In `no-device` and `named-device`, replace each `SEEN_DICTATE="$(newest_log_id 'dictate')"`
   with the same `SEEN_LOGS="$(log_ids)" || fail "<step>" "-" "could not read herdr's plugin log before invoking dictate" "<the same next-step text>"`
   with `<step>` the step's own name, and change each
   `await_new_log "dictate" "${SEEN_DICTATE}" …` to pass `"${SEEN_LOGS}"`.

## Task 3 — configuration (requirement 2) — depends on task 1

The cases `config-restored`, `config-absent`, `interrupt`, `leftover-backup` and
`leftover-marker` must pass afterwards.

1. Before `cleanup`, declare `CONFIG_FILE=""`, `CONFIG_BACKUP=""`, `CONFIG_MARKER=""`.
   Define, before `cleanup`:

   ```bash
   # Puts the plugin's configuration directory back the way the script found it:
   # the original from the backup, or no config.toml when there was none. Does
   # nothing until the link step has learnt the directory, and nothing when there is
   # nothing to put back. Safe to call more than once.
   restore_config() {
       [ -n "${CONFIG_BACKUP}" ] || return 0
       if [ -f "${CONFIG_BACKUP}" ]; then
           mv -f "${CONFIG_BACKUP}" "${CONFIG_FILE}"
           rm -f "${CONFIG_MARKER}"
       elif [ -f "${CONFIG_MARKER}" ]; then
           rm -f "${CONFIG_FILE}" "${CONFIG_MARKER}"
       fi
   }
   ```

2. In `cleanup`, after the `pkill` line, add `restore_config >/dev/null 2>&1 || true`.
3. After `trap cleanup EXIT` add
   `trap 'exit 130' INT`, `trap 'exit 143' TERM`, `trap 'exit 129' HUP`.
4. In the `link` step, after `CONFIG_DIR` is computed and printed: when it is neither
   `not printed` nor empty, set
   `CONFIG_FILE="${CONFIG_DIR}/config.toml"`,
   `CONFIG_BACKUP="${CONFIG_DIR}/${PLUGIN_ID}-config-backup"`,
   `CONFIG_MARKER="${CONFIG_DIR}/${PLUGIN_ID}-config-was-absent"`; if either the
   backup or the marker exists, call `restore_config` and then print
   `an earlier run left the configuration borrowed; restored it before this run uses it`
   (the case `leftover-backup` looks for the word `restored`).
5. In `named-device`, replace the `CONFIG_FILE=…` line and the `if [ -f …config.toml ]; then cp …; fi`
   block with: `mkdir -p "${CONFIG_DIR}"`, then, only when neither `${CONFIG_BACKUP}`
   nor `${CONFIG_MARKER}` exists, `if [ -f "${CONFIG_FILE}" ]; then cp "${CONFIG_FILE}" "${CONFIG_BACKUP}"; else : >"${CONFIG_MARKER}"; fi`.
   The existing `skip_as_failure` for an unknown directory stays above it, and
   `CONFIG_BACKUP` is empty in that case, so `restore_config` does nothing.
   The write of the bogus configuration uses `${CONFIG_FILE}`.
6. Replace the inline restore (`rm -f …config.toml`, `if [ -f "${CONFIG_FILE}" ]; … mv`)
   at the end of `named-device` with a call to `restore_config`.

## Task 4 — a daemon that will not stop, a hanging invoke, tabs (requirements 3 to 5) — depends on task 1

The cases `daemon-will-not-stop`, `daemon-still-up-at-the-end`, `hung-invoke` and `tab`
must pass afterwards.

1. In the header comment's list of environment variables add
   `HERDR_VOICE_STOP_TIMEOUT  seconds to wait for a daemon to stop (default: 10)`.
   After `START_TIMEOUT=30` add `STOP_TIMEOUT="${HERDR_VOICE_STOP_TIMEOUT:-10}"`.
2. In `record`, write each argument with tabs replaced:
   `printf '%s\t%s\t%s\n' "${1//$'\t'/ }" "${2//$'\t'/ }" "${3//$'\t'/ }" >>"${REPORT}"`.
3. After the definition of `daemon_is_up` add `wait_daemon_gone` exactly as in
   `DESIGN_23.md` ("A daemon that will not stop").
4. In `named-device`, replace the `waited=0` … `done` loop after
   `pkill -f 'herdr-voice daemon' …` with
   `wait_daemon_gone || fail "named-device" "-" "the old daemon was still answering ${STOP_TIMEOUT}s after it was told to stop" "find it with 'pgrep -fl herdr-voice' and stop it by hand, then re-run; a second daemon started over it could not bind and the take would run against a daemon that never read the new configuration"`.
5. In `no-daemon`, replace `wait "${DAEMON_PID}" 2>/dev/null || true` with
   `disown "${DAEMON_PID}" 2>/dev/null || true`; replace the wait loop and the
   `if daemon_is_up; then record "no-daemon" "-" "skipped: …"; else … fi` with
   `if wait_daemon_gone; then … (the existing else-branch body) … else fail "no-daemon" "-" "a daemon was still answering ${STOP_TIMEOUT}s after it was told to stop" "find it with 'pgrep -fl herdr-voice' and stop it by hand; the client's behaviour with no daemon cannot be checked while one answers"; fi`.
6. Add `invoke_dictate` exactly as in `DESIGN_23.md` ("A hanging invoke"), before
   `judge_take`'s first use, and replace the `set +e … INVOKE_CODE … cat … if 124`
   blocks of both `no-device` and `named-device` with `invoke_dictate "no-device" "no-device.log"`
   and `invoke_dictate "named-device" "named-device.log"`.

## Task 5 — CI and the evidence — depends on tasks 1 to 4

1. In `.github/workflows/check.yml`, job `scripts`, after the pre-commit step add:

   ```yaml
         - name: the Linux check's decisions, with stubs
           run: sh scripts/test-linux-check.sh
   ```

2. In `docs/evidence.md`, section "Linux, in a container": replace the first two
   paragraphs (from "Run twice on 2026-08-25" through "stated below as one result.")
   with:

   > `scripts/linux-check.sh` ran on 2026-08-25 against two revisions of the plugin.
   > On `e32a9b7` it could not touch capture, because capture did not exist: its
   > `no-device` step was recorded as pending. On `68fbe89`, the tip of `main` and the
   > first revision that records anything, it ran with capture, and produced the table
   > below. The record does not settle whether the `68fbe89` table comes from one run
   > or two; this section claims one run on each revision and no agreement between
   > runs. What the run on `e32a9b7` established about installation, the socket path
   > and the `[[startup]]` entry was re-observed on `68fbe89` and is stated below as
   > one result. The script itself carries the changes described below, which are not
   > in `68fbe89`.
   >
   > A re-run on a machine an earlier run had touched could have been graded against
   > that run's herdr log records: the script remembered one earlier `dictate` record
   > and no earlier `cancel` record (`tasks/23/AC_23.md`). If the `68fbe89` table
   > comes from a second run, its `no-device`, `named-device` and `herdr-log` rows are
   > not independent evidence of that run.

   Keep every other line of the section as it is.
3. At the end of `docs/evidence.md` add a section "The Linux check's decisions, with
   stubs, for issue #23" that gives: the platform (macOS, this machine, date), the
   command, the `ok`/`FAIL` output of the unmodified script and of the modified one,
   `bash --version` of both interpreters used, the shellcheck result, and a plain
   statement: "No Linux run, no container and no real herdr were involved. What this
   leaves unproven: that the changed script passes on a real container with a real
   herdr, and that herdr's `log_id` values and record fields are what the stubs
   assume (the issue's review verified `--plugin`, `--limit`, `.result.pane.pane_id`
   and the empty `logs` array against herdr 0.8.2)."
