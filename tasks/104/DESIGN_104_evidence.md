# Evidence for DESIGN_104

Facts the design stands on that are not in `AC_104_evidence.md`. Paths are relative to the
repository root, read on 2026-10-09 on macOS at the merge of `origin/main` `5a1ccfd` and
`feat/103-mic-popup`. Nothing here is a claim about herdr's popup behaviour; those are marked
UNVERIFIED in the design.

**D1. The sections and keys of the configuration can be listed from the type.** A scratch
crate outside the repository with `serde = "1"` (feature `derive`) and `toml = "1"`, structs
shaped like `Audio` and `Stt` under `#[serde(default)]` deriving `Serialize` and `Deserialize`,
and `toml::Value::try_from(&Config::default())`:

```
[audio]
  input = ""    (string)
  silence_db = -60.0    (float)
[stt]
  command = []    (array)
  command_timeout_seconds = 0    (integer)
  model = ""    (string)
  ok = false    (boolean)
```

Sections and keys come out alphabetical (the `toml` crate's table is ordered by key), each with
its default and its type; a float default of `-60.0` prints as `-60.0`. The probe shows the
library's behaviour, not this repository's types.

**D2. The values of the three string keys with a fixed set.** `[stt] engine`:
`src/config.rs:57-60` says "`candle`, `http` or `command`"; `src/stt.rs:46` `const ENGINES: &[&str] = &["candle", "http", "command"]` (private today) and the matches at `:131` and `:150` use exactly those.
`[rewrite] engine`: `src/rewrite.rs:77-102` matches `"off"`, `"agent"`, `"http"` and `"command"`;
`"agent"` answers that it is not invoked by this build. `[context] source`:
`src/bias/source.rs:7` `const VALUES: &[&str] = &["auto", "transcript", "pane"]`, used by
`resolve` at `:14-28`; it is private today.

**D3. What `src/mic.rs` holds, to move.** `Reached`, `reached`, `render` (the inputs list, with
the duplicate-name and unmatched-name lines), `Answer`, `parse_answer`, `prompt`, `choose` (the
flow), `save_input`, `tell_daemon`, `Mode`/`mode`, `config_note`, `run`/`run_with`/`run_inner`,
`pause`, `open_command`, `open_with`, `open` and the tests of each; `OPEN_BOUND` is 10 seconds.
The flow `choose` takes `names`, `configured`, `input`, `out`, `save` and `tell`; `reached`,
`tell_daemon`, `config_note`, `pause` and `open_with` do not depend on the microphone.

**D4. The popup heights in the manifest.** `herdr-plugin.toml`: `setup` 20, `status` 12,
`model` 16, `mic` 14 (all `placement = "popup"`). The keys menu for the largest section, `[stt]`,
has eight keys; with a heading, a blank line and a question it is thirteen lines, and the
microphone chooser with three inputs and its messages is about fourteen. The `settings` pane is
given 24 rows.

**D5. How the HTTP engine builds a request.** `src/rewrite/http.rs:105-132` builds a dedicated
`ureq::Agent` with `.timeout(timeout)` and `.max_idle_connections_per_host(0)`;
`:177-180` sets `Authorization: Bearer <token>` when the token is not empty; `:182-185` turns a
`ureq::Error` into `Cause::from_ureq(error, bound)` and `src/http_failure.rs:42` `describe(url)`
makes the sentence. The same `Cause` serves the model list.

**D6. `Loaded` is not `Clone`.** `src/config.rs:304-308` `#[derive(Debug)] pub struct Loaded
{ config, source }`; `Source` is `File`, `Defaults(Option<PathBuf>)` or `Invalid { path, why }`.
A snapshot holds one and does not copy it.

**D7. What `setup` says about the microphone.** `src/setup.rs:50-52` the binding (`mic`,
`prefix+shift+i`, "dictation: choose a microphone"); the tests that name it are
`the_bindings_are_the_ones_the_owner_chose` (`:1186`), `with_every_key_taken_it_says_nothing_was_added_rather_than_nothing_to_add`
(the fourth holder's key) and `the_snippet_for_the_microphone_carries_its_action_and_description`
(`:2782-2786`).
