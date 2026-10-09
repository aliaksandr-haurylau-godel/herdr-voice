# Evidence for DESIGN_103

Facts the design stands on that are not in `AC_103_evidence.md`. Paths are relative to
the repository root, read on 2026-10-07 on macOS at `e92f0b2`. Nothing here is a claim
about herdr's popup behaviour; those are marked UNVERIFIED in the design.

**D1. `setup` replaces herdr's configuration by rename, safely.** `src/setup.rs:646`
`let target = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());`
resolves a symbolic link to the file it points at; the comment above it (`:637-645`)
gives the reason ("Renaming over the link itself would leave a regular file in its
place and the real file unchanged, while the run reported success"). `:673` copies the
original's permissions onto the candidate (`set_permissions`); `:692` renames the
candidate over the target. Tests with modes exist at `src/setup.rs:2100` and `:2201`
(unix).

**D2. The `toml` crate renders a string with a line break over several lines.** A
scratch crate outside the repository, `toml = "1"`, run with
`toml::Value::String(s).to_string()`, for five names:

```
"MacBook Pro Microphone" -> "MacBook Pro Microphone" | round trip equal: true
"Mic \"A\" \\ B" -> 'Mic "A" \ B' | round trip equal: true
"Микрофон é 🎙" -> "Микрофон é 🎙" | round trip equal: true
"line\nbreak\ttab" -> """
line
break\ttab""" | round trip equal: true
"ctl\u{7}" -> "ctl\u0007" | round trip equal: true
```

The crate chooses the form itself (a literal string with single quotes for a quote and a
backslash; a multi-line string for a line break). The fourth output is three lines long,
which a one-line replacement cannot overwrite. The probe shows the library's behaviour,
not this repository's code.

**D3. A reply travels as one line.** `src/proto.rs:190-203` (`Reply::read_from`) reads a
reply with a single `read_line`. `src/daemon.rs:2418-2432` (a test) asserts that a
reply carrying line breaks is written as exactly one newline.

**D4. Lessons from the one popup that already asks a question.** `docs/evidence.md`,
section "The pane opens, and the question in it was invisible" and the one after it: a
prompt with no newline stayed in the line buffer until flushed, and an answer typed
without Enter looked like an answer given. The fixes were a flush before the read and
the words "then Enter" in the prompt (`src/setup.rs:898` records the second).

**D5. The client's bounds.** `src/client.rs:17` `REPLY_TIMEOUT` is 2 seconds;
`src/client.rs:32-35` `timeout_for` returns the 120-second bound for `dictate` and
`REPLY_TIMEOUT` for every other command.

**D6. Running another program with a bound, and finding herdr.** `src/outward.rs:115`
`pub fn run(command: &mut Command, bound: Duration) -> Result<Output, RunError>`: closes
standard input, captures output, stops the program at the bound. `src/delivery.rs:178`
`herdr_binary()` returns `HERDR_BIN_PATH` or `herdr`. The environment variable
`HERDR_PLUGIN_ID` appears among the strings of the herdr binary and in the installed
navigator plugin's `open_plugin_pane` (`AC_103_evidence.md`, E17).

**D7. A finished take is judged by the recorder's current `[audio]`.**
`src/capture.rs:376` and `:382`: `stop_one` compares the take's level with
`audio.silence_db`.

**D8. The manifest check reads known commands from `main.rs`.**
`scripts/check_manifest.py` collects `Some("...")` strings from `src/main.rs` and fails
if a manifest command's first argument after `herdr-voice` is not among them; `mic`
is already one.

**D10. The client's exchange and who calls it.** `src/client.rs:109-140` (`send_to`) connects, writes the request, reads the reply on another thread with a bound, and turns every result into `Outcome { code, message }` through `outcome` (`:53-95`), which gives `NoDaemon`, `Timeout`, `Transport`, `Protocol` and an error reply the same `code: 1`. `ClientError` is `pub` (`src/client.rs:40`). `src/doctor.rs:193` calls `send_to` with `ping`; `src/client.rs:97` (`send`) calls it for the three key commands; both keep working because `send_to` stays.

**D9. `Config` can be parsed as the type `config::load` uses.** `src/config.rs:19-21`
`#[derive(Debug, Clone, Default, PartialEq, Deserialize)] #[serde(default)] pub struct Config`.
