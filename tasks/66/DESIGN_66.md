# DESIGN_66 — spawn errors, script fixtures and download fixture

Covers #101, #66, #62 and #48 against `AC_66.md`. Quotes, line numbers and
measurements are in `DESIGN_66_evidence.md`; this file does not need it to be
read.

## Summary of changes

| area | change | acceptance criteria |
|---|---|---|
| `src/delivery.rs` | the existing `start_failure` helper returns a shared `StartFailure` value instead of a `DeliveryError`; `DeliveryError`, `PaintError` and `HerdrError` each convert from it | AC-1, AC-2 |
| `src/setup.rs`, `src/indicator.rs` | the four blanket arms call the helper; `PaintError` and `HerdrError` gain the two variants `DeliveryError` already has | AC-1, AC-2, AC-5 |
| `src/doctor.rs` | `herdr_finding` words its detail from the same classification | AC-3, AC-5 |
| new `src/script_fixture.rs`, test-only and unix-only | one function writes a script and returns only when it can be executed | AC-6, AC-7, AC-8, AC-9 |
| `indicator`, `delivery`, `setup`, `bias`, `bias/pane` tests | each fixture calls that function instead of its own write-and-`chmod` | AC-6 |
| `src/stt/fetch.rs` tests | `serve` returns a `Server` that keeps its listener until told to stop and records every request path; the unreachable-address test connects to port 0 | AC-10 to AC-14 |

Nothing in production code changes except the error handling of AC-1 to AC-3.

## Decisions

### D1. One classification of "starting herdr failed", used by four callers

**Context.** `src/delivery.rs` has a private `start_failure(binary, &io::Error)`
that turns the error kind into one of three `DeliveryError` variants:
`NotFound` (the program is not on the `PATH`), `NotExecutable` (found, not
allowed to run) and `StartFailed` (anything else, carrying the operating
system's text). `setup`, the indicator and doctor start the same program and
still report every error as not found.

**Problem.** `PaintError` and `HerdrError` have only `Rejected` and `NotFound`,
and their tests match on `NotFound { binary, path }`. Copying the helper into
three modules would give four copies of one rule and three copies of the
sentences. Replacing the variants with a wrapped value would change every
existing match.

**Decision.** Move the classification into a public `StartFailure` enum in
`src/delivery.rs` with the three cases, and make `start_failure` return it. The
three sentences exist once, as three functions in `src/delivery.rs`.
`DeliveryError`, `PaintError` and `HerdrError` keep their existing variants and
gain, where missing, `NotExecutable { binary }` and `StartFailed { binary,
reason }` with the same fields as `DeliveryError`'s. Each has a
`From<StartFailure>`, and its `Display` for those three variants prints the
shared sentence.

**Why.** The rule and its wording exist once. Existing variant shapes survive,
so no existing match on `NotFound { binary, path }` changes. The tests of
`start_failure` itself (`src/delivery.rs:579-616`) read its result as a
`DeliveryError` and are rewritten to read a `StartFailure`; their assertions
(which kind gives which case) stay the same. The indicator already imports from
`crate::delivery`, so no new dependency direction appears.

### D2. Doctor words its own detail from the classification

**Context.** `herdr_finding` prints "cannot run {binary}; install herdr, or set
HERDR_BIN_PATH to it" and returns a `Missing` finding.

**Problem.** The `StartFailure` text for a missing program talks about the `PATH`
this process has, which is the right thing for a daemon started by herdr and the
wrong shape for a command a person typed.

**Decision.** `herdr_finding` calls `start_failure` and matches on the result:
not found keeps today's sentence; not executable says the file was found, this
process may not run it, and to make it executable or point `HERDR_BIN_PATH` at
herdr itself; any other case says what the operating system reported and to try
again. The state is `Missing` in all three.

`herdr_finding()` reads `HERDR_BIN_PATH` itself and takes no parameter, and no
test in `src/` sets an environment variable. So the body moves into
`herdr_finding_at(binary: &str) -> Finding`, the way `daemon_finding_at` takes
its address (`src/doctor.rs:156`), and `herdr_finding()` reads the variable and
calls it. The tests call `herdr_finding_at` with a missing path and with a file
that has no execute bit.

**Why.** The person reading `doctor` gets the cause and the next step, and the
existing sentence stays for the case it was written for. A test reaches the
function without touching a variable the parallel suite shares.

### D3. A script is written by one function that returns only when it can be executed

**Context.** Five test modules write a shell script and then run it through the
code under test, in one process whose tests are threads. On Linux an `exec` of a
file fails with `ETXTBSY` while any file description open for writing on that
inode exists. A sibling thread that forks while the writer is open gives its
child a copy of that description until the child's own `exec`.

**Problem.** The spawn happens inside production code, so the test cannot wrap
it. A lock around the writes does not stop forks from tests that write nothing
(`daemon`, `stt::command`). Writing all scripts before any thread starts is not
possible with the test harness. A retry in production code would hide the
failure #101 exists to show.

**Decision.** `src/script_fixture.rs` (`cfg(all(test, unix))`) exposes
`write_executable(path, content)`:

1. `content` must start with `#!`. The function inserts a second line
   `[ -n "$HERDR_VOICE_FIXTURE_PROBE" ] && exit 0` and writes the result with
   mode `0o755` in one open-write-close.
2. It then runs the script once with `HERDR_VOICE_FIXTURE_PROBE=1`, no
   arguments and null standard streams. If the spawn fails with `ETXTBSY` it
   sleeps 5 ms and tries again. Any other spawn error, or more than 10 seconds of
   `ETXTBSY`, panics with the path and the error.
3. It returns after the first probe that starts.

The wait is `wait_until_executable(path, deadline: Duration) -> u32` (the number
of retries), separate from the write so a test can call it on a file it holds
open and can give it a short deadline. `write_executable` passes 10 seconds.

**Why.** See "Why the window is closed by construction" below. The five
fixtures lose their own write-and-`chmod` sequence, and the production spawn
stays as it is.

### D4. The unreachable-address test connects to port 0

**Context.** `an_unreachable_address_says_so_rather_than_hanging` binds an
ephemeral port, reads the number and drops the listener.

**Problem.** The number is then free for any sibling test that binds port 0.

**Decision.** The test fetches from `http://127.0.0.1:0`.

**Why.** Nothing can listen on port 0, so no sibling can own it, and the
connection attempt fails at once and is reported as `FetchError::Http`. Measured
on macOS (`an_unreachable_address_says_so_rather_than_hanging` passes in 0.02 s
with the change); Linux and Windows are verified by the CI run. If a platform
reports something that is not `FetchError::Http`, the test is changed there, not
the fixture.

### D5. `serve` keeps its listener until the test finishes with it

**Context.** `serve` binds a listener, and its thread returns, dropping the
listener, when 750 ms pass without a connection.

**Problem.** A test whose client first connects later than that, because the
machine is loaded, gets "Connection refused" on its own address. Reproduced in
a scratch copy by sleeping 1200 ms before the fetch: the test failed with
`Connection refused (os error 61)`, the text in #62. Port reuse by a sibling
cannot produce that error on a listener that is still bound.

**Decision.** `serve` returns a `Server { base, .. }`. Its thread loops on a
non-blocking accept until a stop flag is set. `Server::finish(self) ->
Vec<String>` sets the flag, joins, and returns the recorded request paths. A
`Drop` that sets the flag covers a test that panics first. Each accepted stream
gets a 5 second read timeout so a client that never sends cannot hold the thread.

**Why.** The listener lives exactly as long as the test uses it, and no timer can
close it. Every test that already calls `handle.join()` calls `finish()` at the
same place, so none can block longer than it does today.

### D6. The request path is recorded and a test pins the revision

**Context.** `serve` matches on the file name only and keeps nothing. The
download URL is built from `entry.revision`.

**Problem.** Replacing the revision with `main` leaves every test green.

**Decision.** For every request, including ones answered 404, the thread
appends the path from the request line to the list `finish()` returns. A new test
fetches the three files through `fetch_into` and asserts the returned paths are
exactly `/<repo>/resolve/<revision>/<name>` for each file of the fixture entry,
compared without regard to order. The mutation of #48 is run again in S4/S5.

**Why.** The assertion states the contract the pin exists for and fails for any
other revision, `main` included.

## Why the window is closed by construction

`execve` fails with `ETXTBSY` only while some file description open for writing
refers to the file. The fixture file is created by `write_executable` and opened
for writing exactly once, there; nothing opens it for writing afterwards. The
descriptions that can exist are: the writer's own, which is closed before the
probe; and copies inherited by a child forked by another thread between the
writer's open and close. Rust opens files close-on-exec, so such a copy is
released when that child calls `exec`, and a child forked after the writer's
close inherits nothing.

So the set of descriptions open for writing on that file only shrinks and
reaches empty. The probe's `exec` succeeds only when the set is already empty at
that moment (otherwise it returns `ETXTBSY` and is retried). Because nothing
adds to the set, it is empty for every later `exec` too, including the one the
production code makes inside the test, and including one made while other
threads fork. The probe does not test whether the race happened to be absent; it
waits for the one state in which the race cannot happen.

No test on macOS can open the window: macOS does not return `ETXTBSY` for this
(measured). The test for this behaviour holds a write handle on a prepared
script, calls `wait_until_executable` on another thread, and releases the handle
after 150 ms. On Linux it asserts the call returned only after the release and
reported at least one retry; on macOS it asserts zero retries and says so. A
second Linux-only test holds the handle past a short deadline and expects the
panic that names the path.

## What is not changed

Production spawns of herdr and of the bias collectors; the assertions of the
indicator, delivery, setup and bias tests; `on_path` in doctor; the download
code, the catalogue and the digest check; CI configuration.

## Risks

- The probe runs each fixture script once more than before. The scripts only
  write files or print; the guard line makes the probe exit before any of that.
  A fixture that depended on running exactly once would be one that records a
  call count, and none does (`AC_66.md`, as-is).
- Port 0 may not fail fast on every platform. CI runs the test on three; a
  platform difference is handled in the test, as D4 says.
- `Server::finish` joins a thread that polls every 5 ms, so a test ends at most
  one poll interval later than the client.
