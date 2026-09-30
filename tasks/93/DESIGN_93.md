# DESIGN_93

Design for issue #93, against `tasks/93/AC_93.md` (gate S1: READY,
`tasks/93/RUN_93.md`).

Classified **bounded** under `superpowers:brainstorming`: one small writer, one
new command, one changed finding, one design-document section. As in earlier
runs, the live approval that skill asks for is replaced by the project's own
gate, `octoflow-reviewer-planner`, because the run is not a chat. The
decisions below are engineering choices `CLAUDE.md` delegates; none is a scope,
naming or spend decision.

## 1. Standard error is written through one function that cannot panic

**Context.** The daemon writes to standard error in twelve places with
`eprintln!`, which panics when the write fails. herdr connects that stream to a
pipe it reads; once herdr is gone, every write fails.

**Problem.** A panic on the connection thread happens before the reply is
written, so the client reads an empty line. Fixing only the two lines the issue
names would leave the others one restart away from the same failure.

**Decision.** A new module `src/stderr.rs` with two functions:

```rust
/// Writes `line` and a newline to `sink`; a failed write is ignored.
pub fn write_line(sink: &mut dyn Write, line: &str)
/// The same, to the process's standard error.
pub fn line(line: &str)
```

`StderrJournal::write` calls `stderr::line`. Every `eprintln!` in
`src/daemon.rs` (`1097`, `1296`, `1316`, `1324`, `1339`, `1430`, `1443`, `1452`,
`1459`, `1478`, `1480`) and the one in `src/capture/cpal_source.rs:44` becomes a
call to `stderr::line`, or to `runtime.journal.write` where a `Runtime` is in
reach (`serve_one`, the accept loop). Command-line messages in `src/main.rs`
and `src/chooser.rs` run in a short-lived client process attached to a terminal
and are not touched. Test-only prints (`src/daemon.rs:4300`, `src/stt/candle.rs`)
are not touched.

**Why.** `writeln!` on `io::stderr()` returns the error instead of panicking,
and Rust ignores `SIGPIPE` at start-up, so a closed pipe is an `Err` that is
dropped. One function means a later print cannot be added by habit without a
grep hit for `eprintln!` in review. `write_line` takes a sink so the failing
case is a unit test with no process-level stderr redirection.

## 2. What happens to a daemon whose herdr has gone: it keeps serving

**Context.** The daemon holds the speech model in memory and is started once by
`[[startup]]`. herdr does not stop it on exit, so it is re-parented to the init
process and keeps the socket. A second start finds the socket held and exits
with `AlreadyRunning`.

**Problem.** Either the daemon keeps answering after its herdr has gone, or it
notices and exits so the next `[[startup]]` owns the socket. Exiting needs a
signal that herdr has gone. Standard error breaking is seen only when something is
written and means nothing on Windows, where the address is a named pipe; the
parent process id is not available the same way on every platform.

**Decision.** It keeps serving. The daemon does not look for its herdr and does
not exit because of it. `doctor` (section 3) is what finds a daemon that cannot
answer, and says how to end it.

**Why.** It is what the design already says about the daemon's lifetime, and it
keeps the loaded model across a herdr restart. Exiting would add a detection
mechanism for each platform, and an exit while a take is recording would lose the
take. With section 1 in place a dead standard error no longer stops a reply, so
the daemon the new herdr finds answers. Whether that daemon's `HERDR_*`
environment still points at the new herdr is not known; S5 measures it and
records the answer, and `docs/design.md` says so as an open question until then.

## 3. `doctor` sends a `ping`

**Context.** `daemon_finding` (`src/doctor.rs:138`) reports `ok` after a bare
connect. The daemon returns from a connection that sent nothing before it
writes anything.

**Problem.** The probe cannot reach anything that fails while a request is
served, so a daemon that answers no request reports as healthy. No existing
command is safe to send: `cancel` does nothing today but is named for an action
on a live take, and every other command changes state or needs a pane.

**Decision.** A new command `ping`, answered with `Reply::Ok("pong")` and
`Control::Continue`, taking no pane and reading no context, so
`needs_target_pane("ping")` is false. `daemon_finding` first connects with `transport::connect`, as it does today; when
that fails the finding is `missing` with the existing text, so "nothing is
listening" stays distinct from "did not answer". When it succeeds, the finding
calls `client::send_to(&address, "ping", None, Vec::new())` and reports `ok` when
the outcome's code is 0 and its message is `pong`. In every other case it reports
`missing`, with a detail of the form

> did not answer a request at `<address>`: `<the outcome's message>`; end the
> process and restart herdr — `pkill -f 'herdr-voice daemon'` — or run
> `herdr-voice daemon` by hand

The `pkill` half is chosen with `if cfg!(unix)` and reads "end the
`herdr-voice` process that was started with `daemon`" elsewhere. A
`cfg!` value, not an attribute, so neither text is dead code under the Windows
check. When nothing is listening the existing text stays as it is.

The body moves to `daemon_finding_at(address: &Address) -> Finding`;
`daemon_finding()` resolves the address from the environment and calls it, and
the tests call `daemon_finding_at` with an address of their own. Both are in
`src/doctor.rs` and are the only change there.

**Why.** A request that reaches `answer` goes through the same reading, writing
and journal lines a keypress does, so the probe fails where a keypress fails. The
command changes nothing, so running `doctor` while a hold is open is safe. A
daemon from a build without `ping` answers `unknown command: ping`; the finding
is then `missing` and says so, which is right after an upgrade that left the old
daemon running, the same situation section 7a of `docs/design.md` describes for
the old id. `ping` is not in the manifest, so `scripts/check_manifest.py` does
not see it, and it is not an action anyone can bind.

## 4. Tests

- `stderr::write_line` with a sink whose `write` always returns an error: returns
  without panicking. With a working sink: writes the line and a newline.
- `serve_one`, run on the existing socket pattern used by the probe test at
  `src/daemon.rs:3235`, with a `Runtime` whose journal is the recording journal of
  `tests_support`: a `cancel` request receives `Reply::Ok("nothing to cancel")`,
  and the recorded lines contain `request_line` of that request. Before this
  change the line went to the process's standard error and the recording is empty,
  so the test fails on the old code. A second case sends `ping` and receives
  `pong`. That the writer itself survives a failing sink is the `write_line` test
  above; together they cover requirement 1.
- `answer` for `ping`: `Reply::Ok("pong")`, `Control::Continue`, and the request
  carries no context.
- `daemon_finding_at`, in `src/doctor.rs`'s own tests, against listeners the test
  writes by hand with `transport::tests_support::probe_address` and
  `transport::listen`; none depends on `daemon.rs`. A listener that accepts and
  closes without replying: `missing`, the detail contains the address and the
  recovery text. A listener that reads one request and answers `Reply::Ok("pong")`
  through `proto`: `ok`. A listener that answers `Reply::Error("unknown command:
  ping")`: `missing`, and the detail contains that message. An address nothing
  listens at: `missing` with the existing text.
- A source test that no `eprintln!` remains in `src/daemon.rs` outside the
  `#[cfg(test)]` module, and none in `src/capture/cpal_source.rs`.

## 5. `docs/design.md`

A new subsection under section 2, "The daemon outlives its herdr", in the
four-part form of sections 1 and 2 above, stating the decision of section 2 here,
the `ping` command of section 3 and the rule that standard error is written
through `src/stderr.rs`. The open question about the daemon's `HERDR_*`
environment is added to section 9 and removed or answered when S5 has run.
