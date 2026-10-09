# Evidence for AC_104

Every statement in the "as-is" part of `AC_104.md` that rests on the code or on a
running program is listed here with the place it was read or the command that
produced it. Paths are relative to the repository root. The branch is
`feat/104-settings-popup`, cut from `main` at `3dd45b8` (`0.1.0-beta.5`). herdr is
`0.9.3` (`herdr --version`). Read on 2026-10-06 on macOS.

An entry marked **UNVERIFIED** states something that was not established by
reading or running anything, and the artifact does not rely on it as fact.

## The configuration and the daemon

**E1. The configuration has eight sections.** `src/config.rs:21-30`:
`pub struct Config { audio, stt, rewrite, ui, delivery, context, ptt, record }`.

**E2. The daemon reads the configuration once.** `src/daemon.rs:1302-1306`:
"The configuration is read once, here, and handed to the recorder's thread:
re-reading it per take would put file system access on the path that runs while
somebody is speaking." followed by `let loaded = config::load(config::directory(&vars).as_deref());`.

**E3. What the daemon builds from that one reading.** `src/daemon.rs:1332-1336`
passes `loaded.config.audio` by value to `Recorder::spawn`, whose signature is
`src/capture.rs:196` `spawn<F>(make_source: F, audio: Audio, takes: PathBuf)`.
`src/daemon.rs:1327` builds the speech engine once with `stt::resolve_with(&loaded.config.stt, state)`
and stores it in `Runtime.recognition` (`src/daemon.rs:47`, `:86`). The remaining
sections go into `Runtime` fields in the struct built at `src/daemon.rs:1348-1373`. The daemon keeps the
speech model resident: `docs/design.md`, section 2, "The daemon outlives its herdr",
"Why".

**E4. The input is chosen per take from what the recorder was given.**
`src/capture.rs:304`: `let device = (!audio.input.is_empty()).then(|| audio.input.clone());`.
The device a running take uses is stored with it: `src/capture.rs:342`
`device: device.unwrap_or_else(|| "the default input".to_string())`.

**E5. Requests the daemon answers.** `src/daemon.rs:166-171`: `answer` matches on
`request.command`; `"ping"` answers `pong` "without reading a context or touching a
take"; `"stop"`, `"cancel"` and others follow. `doctor` sends `ping` with
`src/doctor.rs:193`.

**E6. An unparsable or unreadable file means every default.** `src/config.rs:319-322`:
a failed read returns `Config::default()` with `Source::Defaults`. `src/config.rs:323`
parses with `toml::from_str::<Config>`; on error `src/config.rs:339-345` returns
`Config::default()` with `Source::Invalid`.

**E7. Value types the configuration holds.** `src/config.rs`: strings (`audio.input`,
`stt.model`, `stt.engine`, `rewrite.url`, ...), booleans (`ui.toasts`,
`delivery.submit`, `rewrite.skip_if_plain`, `record.transcripts`), integers
(`ui.blink_ms`, `ptt.release_ms`, `context.prompt_chars`, `stt.command_timeout_seconds` at `:79`), a float (`audio.silence_db`,
`:41`) and arrays of strings (`stt.command` at `:65`, `rewrite.command` at `:104`).
`[stt] token` is `:70`, `[rewrite] token` is `:97`. `load` raises `ui.blink_ms` and
`stt.command_timeout_seconds` to a floor (`src/config.rs:329-333`), so the value the
daemon uses can differ from the value in the file.

**E8. A wrongly typed value passes the guard in `write_model_key` and fails
`config::load`.** The guard parses as `toml::Value` (`src/chooser.rs:205`);
`config::load` parses as `Config` (`src/config.rs:323`). Library behaviour, shown by
the probe in E8a below.

**E8a. Probe.** A scratch crate outside the repository with `serde = "1"` and
`toml = "1"` (the versions in `Cargo.toml`), struct `Ui { blink_ms: u64 }` under
`#[serde(default)]`, input `"[ui]\nblink_ms = \"fast\"\n"`:

```
as toml::Value : Ok("parses")
as Config      : Err("invalid type: string \"fast\", expected u64")
```

Both lock files resolve `toml` to `1.1.4+spec-1.1.0`. The probe shows the
library's behaviour; it does not run this repository's `write_model_key`.

## The model chooser

**E9. `write_model_key`.** `src/chooser.rs:188-213`. Reads the existing file with
`std::fs::read_to_string(&path).unwrap_or_default()` (`:198`); an existing file that
cannot be read (permissions, bytes that are not UTF-8) is therefore treated as empty,
the edit is made against nothing, and `std::fs::write` (`:211`) replaces the file
with the one `[stt]` table. The parse guard is `:205`. Directory creation is `:194`.
The caller of the function in production code is `src/chooser.rs:285`; no test calls
it (every test in `mod tests`, `:309-491`, calls `set_model_key`, `list`, `pick`,
`human` or `table_name`).

**E10. The chooser writes `[stt] model` and no other key.** `src/chooser.rs:133-184`
(`set_model_key`) builds one line `model = "<identifier>"` and only that key; nothing
in `src/chooser.rs` writes `engine`. The success text at `:287-292` and the
already-configured text at `:281-283` mention no engine. The prompt is read with
`read_line` (`:243`).

**E11. What `engine` and `model` do together.** Defaults `engine = "command"`,
`model = "large-v3-turbo"` (`src/config.rs:113,119`). `src/stt.rs:130-142`
(`locate_configured_model`): `"candle"` locates the candle store for `stt.model`;
`"command"` locates a whisper.cpp file only `if wants_our_model(&stt.command)`, that
is when the configured command uses the model; any other case is `NotUsed`. The
catalogue the chooser lists is the candle catalogue: `src/stt/catalogue.rs:31`
`pub const MODELS: [Entry; 6]` (`tiny`, `base`, `small`, `large-v3-turbo`, `medium`,
`large-v3`).

**E21. Why `command` is the default engine.** `src/config.rs:114-118`, a comment on
the default: "`command` — whisper-cli — is the default because it is the fast path:
measured at 1.65 s for a 70-second take against the built-in engine's 12-14 s for
66 seconds with the same model (`docs/evidence.md`)". The measurement itself was not
re-read for this run.

## The microphone

**E12. `mic` is declared and not built.** `herdr-plugin.toml:82-88` declares pane
`mic`, title "Choose a microphone", `placement = "popup"`, command
`herdr-voice mic --choose`. `src/main.rs:187-190`: `Command::Status | Command::Mic`
print `not implemented yet` and exit 69. `src/main.rs:127-130` (`IMPLEMENTED`)
lacks `mic`; `src/main.rs:244-249` asserts `mic` is not in it; `src/main.rs:132` (`USAGE`) names neither `mic` nor `status`.

**E13. Input devices are listed in one place only.** `src/capture/cpal_source.rs:32`,
inside `CpalSource::start`. `grep -rn "input_devices" src` finds that one line.
`src/audio/device.rs:61` `choose(configured, available)` selects by name from a list
of names.

**E14. No code reads a key without Enter.** `grep -rn "termios\|crossterm\|raw mode\|enable_raw" src`
finds nothing. Every interactive read is a line read: `src/chooser.rs:243`,
`src/setup.rs:1046-1053` (`read_answer`); `src/setup.rs:898` says why ("then Enter" is the contract).
`Cargo.toml` `[dependencies]` lists no terminal crate.

## Keys, actions and panes in herdr

**E15. The binding types herdr accepts.**
`strings -a "$(command -v herdr)" | grep -o "CommandKeybindTypeshellpanepopupplugin_action"`
prints `CommandKeybindTypeshellpanepopupplugin_action`: the type has four values,
`shell`, `pane`, `popup`, `plugin_action`. `herdr --default-config` documents
`shell` ("runs detached in the background"), `pane` ("opens a temporary pane and
closes it when the command exits") and `popup` ("a session-modal terminal"), each
with a free-form `command`. No value names a plugin's declared pane.

**E16. What `setup` writes today.** `src/setup.rs:32` (`BINDINGS`, three entries:
`ptt`, `dictate`, `cancel`), each rendered by `render` (`src/setup.rs:51`) as
`[[keys.command]]` with `type = "plugin_action"` and
`command = "herdr-voice.<action>"`. `herdr-plugin.toml`
declares four actions (`dictate`, `ptt`, `cancel`, `setup`, lines 30-56) and four
panes (`setup`, `status`, `model`, `mic`, lines 58-88). `herdr plugin action list --plugin herdr-voice`
lists the four actions.

**E17. How an installed plugin opens a pane from a key.** The plugin
`herdr-navigator` v0.3.6 declares an action `open` and a pane `picker`
(`herdr-plugin.toml` in its checkout, lines 11-15 and 29-33); its action's command
runs `herdr plugin pane open --plugin <id> --entrypoint <pane id> [extra]`
(`src/main.rs:117-130` in its checkout, where `open_plugin_pane` builds exactly that
argument list). The key is a `plugin_action` binding naming the action.

**E18. `herdr plugin pane open` options.** `herdr plugin pane open --help` lists
`--plugin`, `--entrypoint`, `--placement` with possible values `overlay`, `split`,
`tab`, `zoomed`, `--workspace`, `--target-pane`, `--direction`, `--cwd`, `--env`,
`--focus`, `--no-focus`. The value `popup` is not listed. The binary contains the
messages "a popup pane is already open" and "overlay and popup plugin panes target
the active pane" (`strings -a`). **UNVERIFIED:** whether `herdr plugin pane open
--entrypoint mic` without `--placement` opens the pane with the manifest's
`placement = "popup"`. Establishing it needs a running herdr; it must be an
isolated session (`herdr --session <name>`), never the owner's.

## The rewrite server

**E19. `[rewrite] url` is the whole endpoint.** `src/rewrite/http.rs:177`
`self.agent.post(&self.url)`; the token is sent as `Authorization: Bearer <token>`
when not empty (`:178-180`). `docs/evidence.md:1908` shows a configured address as
`.../v1/chat/completions`. The shipped sample in `src/doctor.rs:437` is
`http://127.0.0.1:1234/v1/chat/completions`.

**E20. What the model list returns.** Read-only request, nothing loaded or unloaded:

```
$ curl -s -m 5 http://127.0.0.1:4000/v1/models | python3 -c "..."   # counts and field names only
count 9
entry keys: ['id', 'object', 'owned_by']
```

The nine ids include `text-embedding-nomic-embed-text-v1.5`, which is not a model
for rewriting text. The entries carry no field that tells a chat model from an
embedding model. Issue #104 says the server listed 8; it answered 9 on
2026-10-06.

## Bounds on outward calls

**E22. Calls to other programs and servers carry a time bound.** `src/outward.rs:1-2`:
"Running somebody else's program with a bound on how long it may take ...
`docs/decisions.md` holds the bound for each call." `docs/decisions.md` records 10
seconds for delivery's herdr calls, 5 for the pane read, 60 for the transcriber
command (raised by `[stt] command_timeout_seconds`) and 30 for the rewrite command.
The HTTP rewrite engine has `const TIMEOUT: Duration = Duration::from_secs(30)`
(`src/rewrite/http.rs:93`). These entries were added on `main` after `0.1.0-beta.5`.
