# DESIGN_66 — evidence

Facts `DESIGN_66.md` relies on, each with where it was read or how it was
measured. Read on branch `fix/66-101-spawned-fixtures` at `3dd45b8`.

## Code

| claim | where |
|---|---|
| `start_failure` is private, takes `(&str, &io::Error)`, returns `DeliveryError` with `NotFound`, `NotExecutable`, `StartFailed` | `src/delivery.rs:183-197` |
| `DeliveryError` has `Rejected`, `NotFound { binary, path }`, `NotExecutable { binary }`, `StartFailed { binary, reason }` | `src/delivery.rs:5-18` |
| `PaintError` has only `Rejected` and `NotFound { binary, path }` | `src/indicator.rs:86-93` |
| `HerdrError` has only `NotFound { binary, path }` and `Rejected` | `src/setup.rs:398-404` |
| existing matches on `NotFound { binary, path }` | `src/indicator.rs:1232`, `src/setup.rs:2290`; `src/daemon.rs:2631` constructs `DeliveryError::NotFound` |
| the indicator already depends on `crate::delivery` | `src/indicator.rs:205` (`crate::delivery::herdr_binary()`), `src/setup.rs:549` (`crate::delivery::notify_args`) |
| the four blanket arms and the doctor arm | `src/setup.rs:512`, `:532`, `:549`; `src/indicator.rs:227`; `src/doctor.rs:112-116` |
| the five fixtures that write and run a script | `src/indicator.rs:1050-1058`, `src/delivery.rs:436-445` and `:679-689`, `src/setup.rs:2160-2183`, `src/bias.rs:259-271`, `src/bias/pane.rs:121-129` |
| `serve` returns `(String, JoinHandle<()>)`, stops on a 750 ms quiet deadline, matches `first.contains(suffix)`, records nothing | `src/stt/fetch.rs:265-322` (`:274`, `:280`, `:303`) |
| the URL is `"{base}/{repo}/resolve/{revision}/{name}"` | `src/stt/fetch.rs:154-157` |
| the fixture entry's revision is forty zeros | `src/stt/fetch.rs:363` |
| the unreachable-address test binds and drops a port | `src/stt/fetch.rs:475-481` |
| callers of `serve` | `src/stt/fetch.rs:396`, `:416`, `:448`, `:460`, `:546`, `:563` |

## Measurements

All on macOS (Darwin), this machine, 2026-10-06.

1. **`exec` of a file still open for writing succeeds.** Python `subprocess`
   after `open(p, "w")`, write, flush, `chmod 0o755`, with the file still open:
   the run started and returned. After `close()` it also started. `ETXTBSY` is
   not returned. No container runtime (`docker`, `podman`, `colima`, `orb`,
   `lima`) is installed.
2. **A client that connects after 750 ms is refused.** In a scratch copy of the
   crate, `a_good_transfer_leaves_three_files_and_no_part` with
   `std::thread::sleep(1200 ms)` between `serve(...)` and `fetch_into(...)`:
   `a_good_transfer_leaves_three_files_and_no_part ... FAILED`, with
   `Connection Failed: Connect error: Connection refused (os error 61)` on the
   address `serve` had returned. The other eight tests in the module passed.
3. **The unmodified tests fail under heavy load with the same error.** Scratch
   copy, unmodified `src/stt/fetch.rs`, 60 busy loops started on a 15-core
   machine, the module's test binary run in a loop until it was stopped. Roughly
   86 `FAILED` lines and 29 `refused` lines were printed over the runs that
   completed; the number of runs that completed was not recorded, so this is not
   a rate. The load was far above the six concurrent suites of #62 and was
   stopped by hand; it is evidence that the failure appears and has this text,
   not a "before" figure.
4. **Port 0 fails at once.** Scratch copy with the test's address replaced by
   `http://127.0.0.1:0`: `an_unreachable_address_says_so_rather_than_hanging ...
   ok`, `finished in 0.02s`.

## From the issues

- #66 comment, 2026-10-05: `ubuntu-latest`, run 37275092322,
  `delivery::tests::a_program_that_starts_and_fails_is_still_a_rejection` got
  `StartFailed { reason: "Text file busy (os error 26)" }`; the same run's
  `setup::tests::herdr_cli::open_pane_asks_herdr_for_this_plugin_s_setup_entrypoint`
  got `NotFound` through the blanket arm.
- Orchestrator's count of the failure rate: roughly 5 failures in about 12
  `ubuntu-latest` runs on 2026-09-30 and 2026-10-05.
- #62: three failures in 42 full-suite runs under six concurrent suites.
