# DESIGN_23

Changes are in `scripts/linux-check.sh`, a new `scripts/test-linux-check.sh`, one
line in `.github/workflows/check.yml`, and one section of `docs/evidence.md`.
Nothing in the Rust crate changes. What each step verifies does not change.

## Shape

### Records (requirement 1)

`newest_log_id` is replaced by

```bash
# Every log id herdr holds for the plugin now, as a JSON array. Fails when herdr
# cannot be asked or does not answer the shape the script relies on.
log_ids() {
    herdr plugin log list --plugin "${PLUGIN_ID}" --limit 30 2>/dev/null \
        | jq -ce '[.result.logs[].log_id]'
}
```

`jq -e` exits non-zero on no output and on invalid input, so an unreadable log is
a failure and never an empty set. `await_new_log ACTION SEEN SECONDS` takes that
array as `SEEN` and accepts a record only when its `action_id` is `ACTION`, its
`status` is not `running`, and its `log_id` is not equal to any element of
`SEEN` (`select(.log_id as $id | $seen | any(. == $id) | not)`). Exact equality: jq's
`inside` and `contains` match substrings of strings, so an id that is a prefix or
substring of an older one would be taken as seen.

The snapshot is taken immediately before each of the three invocations that are
waited for: the `cancel` of the `action` step, the `dictate` of `no-device`, and
the `dictate` of `named-device`. Each caller reads
`SEEN="$(log_ids)" || fail STEP - "could not read herdr's plugin log before invoking …" "…"`,
at top level, so `fail` ends the run. A window of 30 records is enough because
`--limit N` returns the newest N and the log only grows: every record that
existed at snapshot time and is still in the newest 30 is in `SEEN`, and one that
has slid out cannot come back.

### Configuration (requirement 2)

Two files beside `config.toml` in the plugin's configuration directory:
`herdr-voice-config-backup`, a copy of the original, and
`herdr-voice-config-was-absent`, an empty marker for when there was no original.
`restore_config` puts the directory back:

```bash
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

`CONFIG_BACKUP`, `CONFIG_MARKER` and `CONFIG_FILE` are empty until the link step has
learnt the directory (and left empty when herdr printed none). Three callers:

1. The link step, as soon as the directory is known: anything a killed earlier
   run left is put back before the configuration is used for anything, and the
   script prints that it did.
2. `cleanup`, quietly, so every way out restores.
3. The end of `named-device`, in place of the inline `rm`/`mv`.

`named-device` borrows only when neither file exists (it always is so after the
first caller, and the check keeps it true): it copies `config.toml` to the backup
when there is one, or creates the marker when there is none, and then writes its
configuration. An existing backup is therefore never overwritten.

`trap 'exit 130' INT`, `trap 'exit 143' TERM` and `trap 'exit 129' HUP` are added
beside `trap cleanup EXIT`, so a signal ends the script through `cleanup`.

A run killed so hard that no trap runs (SIGKILL, power loss) leaves the
configuration borrowed; the next run restores it at the link step, from the backup
or the marker. That covers a killed run whose original was absent, which the
marker exists for.

### A daemon that will not stop (requirement 3)

`STOP_TIMEOUT="${HERDR_VOICE_STOP_TIMEOUT:-10}"`, documented with the other
environment variables in the header. After `daemon_is_up` is defined:

```bash
wait_daemon_gone() {
    _waited=0
    while [ "${_waited}" -lt "${STOP_TIMEOUT}" ]; do
        if ! daemon_is_up; then return 0; fi
        sleep 1
        _waited=$((_waited + 1))
    done
    ! daemon_is_up
}
```

In `named-device`, the wait loop after `pkill` becomes
`wait_daemon_gone || fail "named-device" "-" "the old daemon was still answering
${STOP_TIMEOUT}s after it was told to stop" "…"`, so no second daemon is started over
it. In `no-daemon`, the script today runs `kill "${DAEMON_PID}"` and then
`wait "${DAEMON_PID}"`; `wait` blocks for as long as the process lives, so a daemon
that ignores `TERM` would hang the run there, before any bounded wait could report
it. The `wait` is replaced by `disown "${DAEMON_PID}" 2>/dev/null || true`, which
neither blocks nor lets bash print a job notice, and the script then relies on the
bounded check: the loop becomes `if wait_daemon_gone; then <the checks> else
fail "no-daemon" "-" "a daemon was still answering …" "…"; fi`; the recorded
"skipped" row is gone. The stub daemon that ignores `TERM` sleeps in a loop and
leaves its state file in place, so `daemon_is_up` stays true; the test kills it by
its recorded pid in teardown.

### A hanging invoke (requirement 4)

```bash
invoke_dictate() {
    # step, the log file name
    set +e
    timeout "${CLIENT_TIMEOUT}" herdr plugin action invoke dictate --plugin "${PLUGIN_ID}" \
        >"${LOGS}/$2" 2>&1
    INVOKE_CODE="$?"
    set -e
    cat "${LOGS}/$2"
    if [ "${INVOKE_CODE}" = "124" ]; then
        fail "$1" "${INVOKE_CODE}" "herdr did not return from invoking dictate within ${CLIENT_TIMEOUT}s" \
            "read ${LOGS}/$2"
    fi
}
```

used by both `no-device` and `named-device`, so the two cannot drift again.

### Tabs (requirement 5)

`record` writes each argument with tabs replaced by spaces
(`${1//$'\t'/ }`). The script is bash and already uses `$'\t'`.

### The test (requirement 7)

`scripts/test-linux-check.sh` (POSIX `sh`) builds, for each case, a scratch tree and
a `PATH` that starts with a directory of stubs followed by `/usr/bin:/bin`, and runs
`bash scripts/linux-check.sh` with `HERDR_VOICE_CHECKOUT` naming a fake checkout (a
`herdr-plugin.toml` and a stub plugin binary), `HERDR_VOICE_WORK`, `HOME`,
`HERDR_VOICE_STOP_TIMEOUT=2` and the case's `STUB_*` variables, with a clean
environment (`env -i`). Stubs:

- `herdr`: `--version`, `plugin unlink|link|list|config-dir`, `status server`
  (`{"running":true}`), `pane current`, `workspace create`, `server stop`, and the
  two that matter: `plugin log list` prints the newest N records of a JSON-lines
  file as `{"result":{"logs":[…]}}`, and `plugin action invoke` appends a record with
  a monotonic id, `exit_code` and `stderr` chosen by the case, finished at once or
  after `STUB_DELAY` further `log list` calls.
- the plugin binary: `doctor` (five lines, `daemon ok` iff a state file exists),
  `daemon` (creates the state file, records its pid, sleeps, removes the file on
  `TERM` unless `STUB_DAEMON_IGNORES_TERM`), `cancel`.
- `pkill`: records its arguments and removes the state file unless
  `STUB_DAEMON_IMMORTAL`. It never signals anything. This is not optional: the
  script's own `pkill -f 'herdr-voice daemon'` matches a daemon an owner may have
  running.
- `timeout` (macOS has none): drops its first argument and runs the rest;
  `apt-get`, `cc`, `cargo`, `rustc`, `pkg-config`: answer and do nothing.

Every process the test starts is its own and its pid is recorded; the test kills
those, and only those, in its teardown.

Every case runs under a limit of 60 seconds, enforced with
`perl -e 'alarm shift; exec @ARGV' 60 bash scripts/linux-check.sh` (macOS has no
`timeout`, and `perl` is on both CI runners). `exec` makes the script the process the
alarm kills, so the case ends with exit code 142 (SIGALRM) and the test records the
case as failed with "hit the 60-second limit", then goes on to the next case. A case
whose script hangs, which is what today's script does for "daemon still up at the
end", is therefore a reported failure, not a test that never returns. Processes the
case left behind are the stubs', and are killed by their recorded pids. The
interrupt case runs the script under `python3`, which waits at most 30 seconds for it
to end and then kills that process group, and reports the result the same way.

Cases, each run against the script as it is today before the script is changed;
every one but the first and the "substring id" case must fail then (that one guards
the new filter and passes on today's script, which does not use one):

| case | what it sets | what it asserts |
|---|---|---|
| clean | nothing | exit 0, a report with every step, the stub `pkill` was the one used |
| stale dictate | two finished `dictate` records already in the log, `STUB_DELAY=2` | the `no-device` row carries this run's stderr, not the old records' |
| substring id | a finished `dictate` record with id `log-1010` already in the log, and fresh ids starting at `log-101` | the run passes: a fresh id that is a substring of an old one is still accepted |
| stale cancel | a finished, failed `cancel` record already in the log, `STUB_DELAY=2` | the run passes and `herdr-log` reports exit 0 |
| unreadable log | `log list` fails | exit 1, the message says the log could not be read |
| config restored | an original `config.toml`, a `named-device` refusal that omits the name | exit 1, `config.toml` is the original, no backup, no marker |
| config absent | no `config.toml`, the same failure | exit 1, no `config.toml`, no marker |
| interrupt | an original `config.toml`, `named-device` blocked, SIGINT to the process group (started with `python3`) | the original is back |
| leftover backup | a backup and a borrowed `config.toml` from a killed run | exit 0, `config.toml` is the backup's content, no backup left, the script said it restored |
| leftover marker | a marker and a borrowed `config.toml` | exit 0, no `config.toml`, no marker |
| daemon will not stop | `STUB_DAEMON_IMMORTAL` | exit 1, the message names the daemon still answering, no daemon was started, no "skipped" row |
| daemon still up at the end | `STUB_DAEMON_IGNORES_TERM` | exit 1, no "skipped" row |
| hung invoke | `named-device` invoke returns 124 | exit 1, the message says herdr did not return from invoking dictate |
| tab | stderr with a tab | exit 0, every line of `report.tsv` has three fields |

CI runs it on `ubuntu-latest` and `macos-latest` in the existing `scripts` job.

### The evidence (requirements 6 and 8)

`docs/evidence.md`, section "Linux, in a container": the first two paragraphs are
replaced by one that states, by revision, what the section can support (see
decision 6), and a short paragraph states what the grading defect means for the
second run. A new section records this change's verification and says no Linux
run was made.

## Decisions

### 1. How a record is known to be this run's
- Context: herdr keeps one record per plugin command, and a run leaves finished
  `dictate` and `cancel` records that look like the next run's.
- Problem: remembering the newest id leaves the older records of the previous run
  matching; an empty id matches everything.
- Decision: snapshot every id before invoking, and accept only a finished record
  that is not in the snapshot.
- Why: it is true regardless of how many records a run leaves, and needs no
  assumption about the id's format or about clocks.

### 2. Where the configuration is restored
- Context: the script borrows `config.toml` for one step and exits through many
  paths.
- Problem: restoring inline restores on one path only; a hard kill restores on none.
- Decision: one function, called by `cleanup`, at the end of the step, and at the
  link step for what an earlier run left; a marker stands for "there was none".
- Why: `cleanup` is the only code every exit path runs, and the link-step call is
  the only place a kill that ran no code can be repaired.

### 3. A daemon that will not stop
- Context: the step restarts the daemon to read a new configuration.
- Problem: a daemon that survived `pkill` keeps the socket, so the new one cannot
  bind and the step tests a daemon that never read the configuration.
- Decision: wait, and fail the run when it is still answering.
- Why: continuing produces a result about the wrong thing, which is the defect
  this issue is about.

### 4. The wait time
- Context: ten seconds is right for a real daemon and wasteful in a test of the
  failure.
- Problem: a test that waits ten seconds per case is a test nobody runs.
- Decision: `HERDR_VOICE_STOP_TIMEOUT`, default ten.
- Why: it follows the script's other environment variables and changes nothing by
  default.

### 5. Tests with stubs
- Context: no container runtime, no herdr on the machine; the script's failures are
  in its decisions, not in what herdr does.
- Problem: a function-level test would not show that restoration happens on every
  exit path, which is a property of the whole script.
- Decision: run the whole script against stubs, one case per defect.
- Why: each case fails on today's script, which shows it tests the defect.

### 6. How many runs the evidence records
- Context: the section says "Run twice ... on revision `68fbe89`" and, a few lines
  later, "This is the second run of this check. The first, on revision `e32a9b7`".
- Problem: the commit that wrote it, the commit before it and the run artifacts do
  not settle whether the table comes from one run or two on `68fbe89`; deciding
  would be inventing a fact.
- Decision: state one run on each revision, say that the record does not settle
  whether `68fbe89` was run once or twice, and claim no agreement between runs.
- Why: it contradicts nothing the record shows and does not say more than it knows;
  the owner, who ran them, can correct the count.
