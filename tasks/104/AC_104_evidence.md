# Evidence for AC_104

Every statement in the "as-is" part of `AC_104.md` that rests on the code or on a running
program is listed here with the place it was read or the command that produced it. Paths
are relative to the repository root. The tree is `feat/104-settings-popup` after
`origin/main` at `5a1ccfd` and `feat/103-mic-popup` were merged into it. herdr is `0.9.3`
(`herdr --version`). Read on 2026-10-09 on macOS.

An entry marked **UNVERIFIED** states something that was not established by reading or
running anything, and the artifact does not rely on it as fact.

## The configuration

**E1. The configuration has eight sections, and only `Deserialize` is derived.**
`src/config.rs:19-30`: `#[derive(Debug, Clone, Default, PartialEq, Deserialize)]` on
`pub struct Config { audio, stt, rewrite, ui, delivery, context, ptt, record }`. Every
section struct derives the same set (`:32`, `:53`, `:87`, `:148`, `:190`, `:216`, `:229`,
`:235`). `Cargo.toml` has `serde = { version = "1", features = ["derive"] }`, so adding
`Serialize` needs no new dependency.

**E2. The value types.** `src/config.rs`: strings (`audio.input`, `stt.model`,
`stt.engine`, `rewrite.url`, ...), booleans (`ui.toasts`, `delivery.submit`,
`rewrite.skip_if_plain`, `record.transcripts`), integers (`ui.blink_ms`, `ptt.release_ms`,
`context.prompt_chars`, `stt.command_timeout_seconds` at `:79`), one float
(`audio.silence_db`, `:41`), and arrays of strings (`stt.command` at `:65`,
`rewrite.command` at `:104`). The secrets are `[stt] token` (`:70`) and `[rewrite] token`
(`:97`); `rewrite.model` and `stt.model` are the two keys named `model`.

**E3. An unreadable or unparsable file means every default, and some values are raised.**
`src/config.rs:310-345` (`load`): a failed read returns `Config::default()` with
`Source::Defaults`; a parse failure returns `Config::default()` with `Source::Invalid`.
`load` raises `ui.blink_ms` and `stt.command_timeout_seconds` to a floor (`:329-333`), so a
value the daemon uses can differ from the one in the file.

**E4. A wrongly typed value passes a parse as `toml::Value` and fails one as `Config`.**
Probe: a scratch crate with `serde = "1"` and `toml = "1"` (the versions in `Cargo.lock`,
`toml 1.1.4`), struct `Ui { blink_ms: u64 }` under `#[serde(default)]`, input
`"[ui]\nblink_ms = \"fast\"\n"`:

```
as toml::Value : Ok("parses")
as Config      : Err("invalid type: string \"fast\", expected u64")
```

The probe shows the library's behaviour. The writer in E5 parses as `Config`.

## What the tree already holds

**E5. The configuration writer.** `src/config_edit.rs`: `Edit` (`:10`), `quote` (`:19`),
`set_keys` (`:80`, line-oriented, keeps comments, other keys and line endings),
`WriteError` (`:162`: `NoDirectory`, `Unreadable`, `AlreadyInvalid`, `Refused`, `Io`) and
`write_keys` (`:230`). It parses the original and the edited text as `Config`, refuses a
file that cannot be read instead of treating it as empty, writes a candidate beside the
file and renames it over the target (mode and symbolic link kept), and is covered by tests
in the same file.

**E6. The reload request.** `src/daemon.rs:185` (the `"reload"` arm), `:298`
(`plan_reload`), `:333` (`reload_from`); `src/reload.rs` (`reply`, `parse`, `Applied`);
`src/client.rs` (`exchange`, and a ten-second bound for `reload`, `RELOAD_TIMEOUT`).
`reload_from` re-reads the file, applies `[audio]` through `Recorder::reconfigure`
(`src/capture.rs:330`, handled in the recorder's own order queue at `:257`), and answers
one line: `applied: audio` or `applied: nothing`, then `; needs a restart: <sections>` for
every other section whose values differ from what the daemon started with. The daemon
reads the file once at start (`src/daemon.rs:1454`) and keeps that configuration in
`Runtime.running`.

**E7. A microphone popup, its action and its key are built.** `src/mic.rs` holds the
list (`render`), the answer reading (`parse_answer`, an Esc or empty line leaves), `choose`
(the flow over injected input, output, `save` and `tell`), `reached` (the mapping of what
`client::exchange` returned to `Reached::{Applied, NoDaemon, Failed}`), `save_input`,
`tell_daemon`, `config_note`, `run`, `open_command` and `open_with`. `src/main.rs:112,191-194`
dispatches `mic`, `mic --choose` and `mic --open`; `src/main.rs:132,246` lists `mic` as
implemented; `src/main.rs:145` names it in the usage text. `herdr-plugin.toml:58-63`
declares the action `mic` (title "Dictation: choose a microphone", command
`herdr-voice mic --open`) and `:89-95` the pane `mic` (popup, 60% wide, height 14).
`src/setup.rs:32` (`BINDINGS`, four entries) has `mic` on `prefix+shift+i`.
`src/capture/cpal_source.rs:31` `input_names()` lists the inputs by the names a take
matches against.

**E8. The speech-model chooser.** `src/chooser.rs`: `list` (`:34`, six catalogue models
with size, mel bins and installed or not, marking the configured one), `pick` (`:63`, a
number), `run` (`:101`, lists, reads a number, downloads with `fetch::model`, verifies with
`store::locate`, then writes `[stt] model` through the writer and prints "the daemon still
holds the previous model"). Nothing in it writes `[stt] engine`; `run` has no test (the
tests in the file cover `human`, `list` and `pick`). The pane `model` ("Choose a speech
model", popup, 70% wide, height 16) is `herdr-plugin.toml:81-87`. `src/stt/fetch.rs:17`
`Progress` and `:96` `model(identifier, models, progress)` are the download;
`src/stt/candle/store.rs:176-182` `Glance` and `glance` say whether a model is there without
hashing it.

**E9. What `[stt] engine` and `[stt] model` do together.** Defaults `engine = "command"`,
`model = "large-v3-turbo"` (`src/config.rs:113,119`). `src/stt.rs:130-142`
(`locate_configured_model`): `"candle"` locates the candle store for `stt.model`;
`"command"` locates a whisper.cpp file named for the model only `if wants_our_model(&stt.command)`
(`src/stt.rs:87`, true when an argument contains `{model}`) and then looks for the file
`<models dir>/ggml-<model>.bin` (`src/stt/model.rs:18-19` `file_name`, `:97-101` `locate`); any
other case uses no model.
The catalogue is the candle one: `src/stt/catalogue.rs:31` `MODELS: [Entry; 6]` (`tiny`,
`base`, `small`, `large-v3-turbo`, `medium`, `large-v3`). `src/config.rs:114-118` records
that `command` is the default because it measured 1.65 s for a 70-second take against the
built-in engine's 12 to 14 s for 66 seconds with the same model (the measurement itself was
not re-read).

## The rewrite server

**E10. `[rewrite] url` is the whole endpoint, and the engine's bound is thirty seconds.**
`src/rewrite/http.rs:177` `self.agent.post(&self.url)`; the token is sent as
`Authorization: Bearer <token>` when not empty (`:178-180`); `const TIMEOUT` is 30 seconds
(`:93`). `docs/evidence.md` shows a configured address as `.../v1/chat/completions`.
`src/http_failure.rs:14-60` (`Cause` and `describe`) turns an unreachable, refusing or silent
server into a sentence that names the address and ends in what to do.

**E11. What the model list returns.** Read-only request, nothing loaded or unloaded:

```
$ curl -s -m 5 http://127.0.0.1:4000/v1/models | python3 -c "..."   # counts and field names only
count 8
entry keys: ['id', 'object', 'owned_by']
```

The entries carry no field that tells a chat model from an embedding model; on 2026-10-06
the same server served nine entries, one of them `text-embedding-nomic-embed-text-v1.5`.
The number varies with what is loaded.

**E12. Calls to other programs and servers carry a time bound.** `src/outward.rs:1-2`:
"Running somebody else's program with a bound on how long it may take ...
`docs/decisions.md` holds the bound for each call." The decisions record 10 seconds for
delivery's herdr calls, 5 for the pane read, 60 for the transcriber command and 30 for the
rewrite command.

## Keys, actions and panes in herdr

**E13. The binding types herdr accepts.**
`strings -a "$(command -v herdr)" | grep -o "CommandKeybindTypeshellpanepopupplugin_action"`
prints `CommandKeybindTypeshellpanepopupplugin_action`: the type has four values, `shell`,
`pane`, `popup`, `plugin_action`. `herdr --default-config` documents `shell`, `pane` and
`popup` as running a free-form `command`. No value names a plugin's declared pane.

**E14. How an installed plugin opens a pane from a key.** The plugin `herdr-navigator`
v0.3.6 declares an action `open` and a pane `picker` (its `herdr-plugin.toml`, lines 11-15
and 29-33); the action's command runs `herdr plugin pane open --plugin <id> --entrypoint
<pane id> [extra]` (`open_plugin_pane`, `src/main.rs:117-130` in its checkout). The key is a
`plugin_action` binding naming the action.

**E15. `herdr plugin pane open` options.** `herdr plugin pane open --help` lists
`--plugin`, `--entrypoint`, `--placement` with possible values `overlay`, `split`, `tab`,
`zoomed`, `--workspace`, `--target-pane`, `--direction`, `--cwd`, `--env`, `--focus`,
`--no-focus`; `popup` is not listed. **UNVERIFIED:** whether `herdr plugin pane open
--entrypoint <pane>` without `--placement` opens a pane whose manifest says
`placement = "popup"`; whether herdr closes a popup the moment its command exits; whether a
popup pane's environment carries `HERDR_PLUGIN_STATE_DIR`. Each needs a running herdr that
links this build, and the registry of plugins is shared with the herdr in use.

**E16. `setup` writes `plugin_action` bindings.** `src/setup.rs:32` (`BINDINGS`), rendered
by `render` as `[[keys.command]]` with `type = "plugin_action"` and
`command = "herdr-voice.<action>"`. Nine of its tests depend on how many bindings there are.

**E17. Every interactive read is a line read that needs Enter.**
`grep -rn "termios\|crossterm\|enable_raw" src` prints nothing; the reads are
`src/chooser.rs`, `src/setup.rs` (`read_answer`) and `src/mic.rs` (`choose`). `src/setup.rs`
records why the prompt says "then Enter" and flushes before the read; `src/mic.rs` does the
same.
