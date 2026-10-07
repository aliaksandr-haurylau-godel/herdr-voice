# RUN_23

| field | value |
|---|---|
| issue | #23 — The Linux check grades the previous run, and loses the configuration on failure |
| input | GitHub issue, read with `gh issue view 23`; the issue has no comments |
| stage | S4 |
| branch | fix/23-linux-check |
| opened | 2026-10-07 |

## Stages

<!-- One block per stage, appended, never rewritten. -->

### S1 Assess
- artifact: `AC_23.md`
- produced: 2026-10-07

## Notes

- Base: `e92f0b2`.
- The issue was written against `662508a`; every point was read against this branch and still holds (line numbers in `AC_23.md` are this branch's).
- No container runtime or herdr is available here, so verification is by stub (assignment).
- `scripts/linux-check.sh` runs `pkill -f 'herdr-voice daemon'`. The owner's live daemon matches that pattern, so no test may let a real `pkill` run: the test script puts a stub `pkill` first on `PATH`.

```yaml
gate:
  stage: S1
  artifact: AC_23.md
  reviewer: designer
  verdict: READY
  date: 2026-10-07
  blocker: null
```

Reviewer note, recorded as returned: requirement 2 brings back a backup left by a killed run, but not a killed run whose original `config.toml` was absent, since nothing then marks the file as borrowed; the ticket asks only for restore on exit paths and for an existing backup never to be overwritten, and the design will say whether this case is handled.

### S2 Design
- artifact: `DESIGN_23.md`
- produced: 2026-10-07

```yaml
gate:
  stage: S2
  artifact: DESIGN_23.md
  reviewer: planner
  verdict: QUESTIONS
  date: 2026-10-07
  questions:
    - >-
      The case "daemon still up at the end" (STUB_DAEMON_IGNORES_TERM) cannot reach its
      stated result with the script changes the design lists. In `no-daemon`, the script
      runs `kill "${DAEMON_PID}"` and then `wait "${DAEMON_PID}" 2>/dev/null || true`
      (scripts/linux-check.sh:766-767), and only after that calls the wait loop the design
      replaces with `wait_daemon_gone`. The design's stub daemon "sleeps" and ignores TERM,
      so bash's `wait` blocks until that sleep ends. That happens before `wait_daemon_gone`
      runs, so the run does not end with "exit 1, no skipped row"; it hangs, both on
      today's script and on the changed one. A real daemon that ignores TERM would hang
      the real script at the same line, and that daemon is what requirement 3 is about.
      The design does not say which of two things happens: the script's `wait` changes
      (bounded, or removed in favour of `wait_daemon_gone`), which would be a script
      change the design does not list; or the stub's sleep is bounded, and the state file
      stays behind when the stub exits, so `daemon_is_up` is still true. This stops me
      because I cannot give the task for that case, or the task for the `no-daemon` change,
      an input, an output and a done-criterion someone else can check without making this
      choice myself. It also decides whether the `no-daemon` task touches line 767.
  blocker: null
```

Reviewer note, recorded as returned: every requirement from 1 to 8 has a section that answers it, and each case except this one traces to fail on today's script and pass on the changed one.

Author's answer (round 1): the script changes. Checked against the script: `wait` at line 767 blocks while the process lives, so an unbounded `wait` before the bounded check defeats requirement 3 for exactly the daemon it is about. The `wait` is replaced by `disown`, which neither blocks nor prints a job notice; the design says so and says what the stub does.

```yaml
gate:
  stage: S2
  artifact: DESIGN_23.md
  reviewer: planner
  verdict: QUESTIONS
  date: 2026-10-07
  questions:
    - >-
      The design does not say how the test script bounds a case that never ends, and
      without that the case "daemon still up at the end" has no done-criterion I can
      write for the run against today's script. Requirement 7 says each case fails on
      the script as it is today, and the design's table says so too ("every one but the
      first must fail then"). With STUB_DAEMON_IGNORES_TERM set, the stub daemon "sleeps
      in a loop" and ignores TERM. Today's `no-daemon` runs `kill "${DAEMON_PID}"` and
      then `wait "${DAEMON_PID}"` (scripts/linux-check.sh:766-767), so today's script
      blocks there for good. It never exits, so the case never reports a failure, and
      neither the test script nor the by-hand S5 step ("each case shown to fail on the
      unmodified script") would return. The design's `timeout` stub drops its first
      argument and gives no bound, and the design gives the test script no per-case
      time limit. A hang is not "a failure", and nothing in the design turns it into
      one. This stops me because the task for the test script needs a stated result for
      a case whose script does not end: a per-case limit (how long, how it is enforced
      without `timeout` on macOS, and that a case hitting it counts as failed and kills
      only its own process group), or some other stated way this case shows it fails on
      today's script. Choosing one would put a harness interface into the plan that the
      design does not have.
  blocker: null
```

Reviewer note, recorded as returned: the round 1 answer settles the round 1 question; replacing `wait` with `disown` and then calling `wait_daemon_gone` gives the `no-daemon` task a checkable done-criterion. The question above is only about this case against today's script.

Author's answer (round 2): a per-case limit of 60 seconds, enforced with `perl -e 'alarm shift; exec @ARGV' 60 bash scripts/linux-check.sh`; exit code 142 is recorded as a failed case ("hit the 60-second limit") and the test goes on. Processes a case leaves behind are the stubs', killed by their recorded pids; the interrupt case is bounded by `python3` at 30 seconds and kills only its own process group. Written into the design. The limit is also what lets a hung case be reported when run against today's script.

```yaml
gate:
  stage: S2
  artifact: DESIGN_23.md
  reviewer: planner
  verdict: READY
  date: 2026-10-07
  blocker: null
```

Reviewer note, recorded as returned (not a gate question): `select(([.log_id] | inside($seen)) | not)` does not do what decision 1 says, because for strings jq's `inside` matches substrings, so a new id that is a substring of an old id would be counted as seen; a filter that tests exact membership does what decision 1 says.

Author's change after the gate: the filter is exact equality (`select(.log_id as $id | $seen | any(. == $id) | not)`) and the design gains a case, "substring id", that guards it. That case passes on today's script (which excludes one id by `!=`), so the design now says every case but the first and that one fails on today's script.

### S3 Plan
- artifact: `PLAN_23.md`
- produced: 2026-10-07

```yaml
gate:
  stage: S3
  artifact: PLAN_23.md
  reviewer: implementer
  verdict: QUESTIONS
  date: 2026-10-07
  questions:
    - 'Task 1 and Task 3 disagree on the output text, so the case `leftover-backup` cannot pass. The case asserts `expect_text leftover-backup "the script says it restored" "restored" "${CASE}/out"` (PLAN_23.md:453). Task 3 step 4 (PLAN_23.md:583-585) tells me to print `an earlier run left the configuration borrowed; restoring it first`. "restoring" does not contain the substring "restored", and a grep of `scripts/linux-check.sh` finds no other "restored" in its output. After I implement task 3 exactly as written, that assertion prints FAIL and the run ends non-zero, but task 3 requires the case to pass. Which is it: should the test assert "restoring", or should the printed message contain "restored"? I cannot pick one without changing either the verbatim test text in task 1 or the verbatim message in task 3.'
```

Author's answer (round 1): the message changes, not the test. The script prints it after `restore_config` has run, so the past tense is the true one: `an earlier run left the configuration borrowed; restored it before this run uses it`. Task 3, step 4 of the plan now says so.

```yaml
gate:
  stage: S3
  artifact: PLAN_23.md
  reviewer: implementer
  verdict: READY
  date: 2026-10-07
  questions: []
  blocker: null
```

Reviewer notes, recorded as returned (do not stop execution): under `env -i` with `PATH=<stubs>:/usr/bin:/bin`, `bash` resolves to `/bin/bash` (3.2 on macOS), so the "bash on PATH" run and the `/bin/bash` run are the same interpreter on this machine; the harness checks for `jq`, `perl`, `bash` and `python3` on the outer `PATH`, but the script under test sees only `/usr/bin:/bin`.

### S4 Implement
- baseline: `e92f0b2`.
- To run the test under a second, newer bash, `HERDR_VOICE_TEST_BASH` names it by absolute path.

Task 1, run against the unmodified `scripts/linux-check.sh` (red phase), macOS 27.0.1, `/bin/bash` 3.2: `clean` and `substring-id` pass; every other case fails, 22 `FAIL` lines in all. Failures by case:

- `stale-dictate`: exit 1, and the report holds the earlier run's `STALE` text.
- `stale-cancel`: exit 1, no `exit_code 0` in the `herdr-log` row.
- `unreadable-log`: exit 1, but not the message the plan names.
- `config-restored`, `config-absent`, `leftover-backup`, `leftover-marker`, `interrupt`: the configuration is not the original, a backup or marker is left, the script says nothing about restoring.
- `daemon-will-not-stop`: exit 0 (the run goes on over a daemon that did not stop). `daemon-still-up-at-the-end`: hit the 60-second limit (the script blocks in `wait`).
- `hung-invoke`: the failure says "never finished".
- `tab`: a report line has four fields.

The `interrupt` case's `exit 130` line passes on the unmodified script only because the child was killed by SIGINT; its two restore assertions fail.

Green phase, after tasks 2 to 5: `sh scripts/test-linux-check.sh` prints 41 `ok` lines and `all cases passed`, under `/bin/bash` 3.2.57 and under `bash` 5.3.20 (`HERDR_VOICE_TEST_BASH`). `shellcheck` is clean on both scripts. After the run no stub process is left and the owner's running `herdr-voice daemon` is still there (read with `pgrep -fl`, nothing signalled); the stub `pkill` was the one used in every case.

### S4 code review (round 1)

Reviewer: a fresh general-purpose subagent, over `e92f0b2..7e3ab85`. No Critical findings. Verdict: ready to open a pull request, with fixes. Findings and how each was settled:

1. Important — the evidence section for this change, with the plain statement that no Linux run was made, was not in the diff. **Open until S5:** it is written after the last test run and before the pull request (AC requirement 8).
2. Important — the rewritten "Linux, in a container" intro said "one run on each revision", while the passage on a daemon outliving its server, later in the same section, records several attempts on `68fbe89` and "both re-runs after that change were clean". **Fixed.** The intro now reads from that passage: an attempt that found the leaked daemon, and after the cleanup change two re-runs that were clean and agreed, the second made to show re-runnability on a machine already touched. The count of attempts in all, and which re-run the table was copied from, are stated as not recorded. The hazard paragraph now says the second re-run was on a touched machine, so its three rows could have been satisfied by the first re-run's records, and that whether they were is not recorded.
3. Minor — `teardown` sent `kill -9` to every pid ever recorded, and a pid can be reused. **Fixed:** it kills only a pid that is still running and whose command line names the test's scratch directory.
4. Minor — `cleanup` discarded `restore_config`'s result silently. **Fixed:** `restore_config` returns non-zero when the move or the removal fails; `cleanup` prints one warning that names where the original is; the end of `named-device` now fails the step with the same next action. New case `restore-fails` (skipped, and said so, when run as root).
5. Minor — the signal paths skipped the report and two comments were untrue; a second signal during cleanup could cut it short. **Fixed:** `on_signal` prints that the run was interrupted and the table so far, then exits; `cleanup` begins with `trap '' INT TERM HUP`; the comments say what happens. **Accepted:** a `TERM` sent to the script alone (not its process group) is handled when the current foreground command ends, which is how bash behaves; a Ctrl-C or a signal to the group reaches the foreground command too. The `interrupt` case now also asserts the message.
6. Minor — the script under test saw only `/usr/bin:/bin` and needs `jq`. **Fixed:** the directory `jq` is in is appended after them; the stub directory stays first.
7. Minor — weak assertions: `leftover-backup`'s restore property was guarded only by the final content. **Fixed:** the stub records, at every `dictate`, whether the borrowed file was present; `leftover-backup` and `leftover-marker` assert the first take ran with it absent. `hung-invoke` now says in a comment that it shows the 124 branch only.
8. Minor — `--limit 30` returning the newest records oldest-first is assumed, not checked against herdr. **Goes into the evidence's list of what is unproven.**
9. Minor — `invoke_dictate` fails only on 124; another non-zero return from `herdr plugin action invoke` in `named-device` is reported as "never finished" after the wait, as before. **Out of scope** (the issue lists 124 only), and goes into the same list.

After these: `sh scripts/test-linux-check.sh` prints 47 `ok` lines and `all cases passed`; `shellcheck` is clean on both scripts.

### S4 mutation testing

Tester: a fresh general-purpose subagent, in a scratch copy, one run at a time (`sh scripts/test-linux-check.sh`, about 70 seconds each). First run, over `477bd28`: 54 mutations, 38 killed, 16 survived. How each survivor was settled:

- Killed by new cases (re-checked by a second tester run on `9149779`, 14 of 14 killed, and by me for the last two below):
  - tab in the exit-code field: case `tab-in-exit-code`;
  - backup and marker both present, and a restore that cannot write the directory: `leftover-backup-and-marker`, `leftover-unwritable`;
  - the `TERM` and `HUP` traps: `interrupt-term`, `interrupt-hup`;
  - a failing `log list` at the no-device and named-device snapshots: `unreadable-log-before-no-device`, `unreadable-log-before-named-device`;
  - `HERDR_VOICE_STOP_TIMEOUT`'s name and default: `daemon-will-not-stop` asserts "2s", `default-stop-timeout` asserts "10s".
- Two survived the second run and were real gaps, and are now killed, re-checked by me one at a time in the foreground:
  - marker branch checked before the backup branch: the case only compared the end state, and the mutant reaches the same end state. The stub now records the first line of the configuration at every `dictate` (or `absent`), and `leftover-backup`, `leftover-marker` and `leftover-backup-and-marker` assert the sequence (`ORIGINAL`, then `[audio]`; `absent`, then `[audio]`). Mutant now fails `leftover-backup-and-marker`.
  - `trap '' INT TERM HUP` removed from `cleanup`: the second signal was sent after a fixed second and the first run could not tell. It is now sent once the stub `pkill` that cleanup runs says it has started (a marker file). Mutant now fails `interrupt-twice` (two assertions) under bash 5. **Under bash 3.2 this mutant cannot be killed**: bash 3.2 does not run a trap again while it is running the `EXIT` trap, which I confirmed with a two-line experiment outside the repository (`with.sh` and `without.sh`, both reached `cleanup-end` under 3.2; only `with.sh` did under 5). So the case tells the two apart under bash 4 and later (the ubuntu job, or `HERDR_VOICE_TEST_BASH` naming a newer bash) and says so in a comment.
- Equivalent or unreachable, left as they are:
  - tab replacement for the first field (the step name): step names are literals in the script;
  - `[ ! -f "${CONFIG_BACKUP}" ] &&` in the borrow guard: the link step restores or fails first, so a backup cannot be present there; it is kept because AC requirement 2 says an existing backup is never overwritten and the guard is what states it at the place it would happen;
  - `-lt` to `-le` in `wait_daemon_gone`: one extra second of waiting, nothing observable;
  - `&& [ -n "${CONFIG_DIR}" ]` in the link step: an empty directory gives names rooted at `/`, which do not exist;
  - `[ -n "${CONFIG_BACKUP}" ] || return 0` at the top of `restore_config`: with empty names both tests are false and the function returns 0 anyway. **The line was deleted.**

After these: `sh scripts/test-linux-check.sh` prints 75 `ok` lines and `all cases passed` under `/bin/bash` 3.2.57 and under bash 5.3.20; `shellcheck` is clean on both scripts.
