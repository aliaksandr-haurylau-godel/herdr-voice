# DESIGN_104 — One settings popup

Covers `AC_104.md` (criteria AC-1 to AC-44) and introduces nothing beyond them. Facts about
the existing code are in `AC_104_evidence.md` as [E1], [E2]; facts this design adds are in
`DESIGN_104_evidence.md` as [D1], [D2]. Anything marked UNVERIFIED was not established and is
settled by the step named.

The writer (`src/config_edit.rs`), the reload request (`src/reload.rs`, the `"reload"` arm of
the daemon, `Recorder::reconfigure`, `client::exchange`) and the input list
(`cpal_source::input_names`) are in the tree and are used as they are [E5, E6, E7].

## 1. What changes

| Where | Change |
|---|---|
| `src/config.rs` | `Serialize` is derived on the configuration types, so the list of sections, keys and defaults is read from the type and not kept by hand |
| `src/popup.rs` (new) | What a popup of this plugin needs: `Io` (the terminal), `Answer` and `parse_answer`, `prompt`, `pause`, `Reached` and `reached`, `tell_daemon`, `config_note`, `change_note`, `open_command` and `open_with`, and the trait `World` that every flow talks to. Most of it moves here from `src/mic.rs` |
| `src/settings.rs` (new) | The popup: the sections menu, the keys menu, the three choosers (microphone, speech model entry, rewrite model), the editor for the other keys, `Real` (the `World` of a real run), `run` and `open` |
| `src/chooser.rs` | `choose_speech_model` (the flow of `model --choose`, over `World`) replaces the body of `run`; `list` takes the state of each model from the `World`; `run` becomes a thin wrapper, so the pane `model` and the popup share one function |
| `src/rewrite_models.rs` (new) | `models_url` (the list's address from `[rewrite] url`) and `fetch` (one bounded `GET`) |
| `src/mic.rs` | Removed: what is reusable moved to `popup.rs` and `settings.rs`, the command goes |
| `src/main.rs` | `Command::Settings` replaces `Command::Mic`: `settings` is the popup, `settings --open` is the action; usage text and the lists of implemented commands name `settings` |
| `src/bias/source.rs`, `src/stt.rs` | `VALUES` and `ENGINES` become `pub(crate)`, so the popup offers the values the code accepts and keeps no second copy |
| `herdr-plugin.toml` | The action and the pane `mic` go; an action `settings` (global; command `settings --open`) and a pane `settings` (popup, 70% wide, 24 high; command `settings`) come |
| `src/setup.rs` | The fourth binding is `settings` on `prefix+shift+s` (description "herdr-voice: settings"); the tests that name the binding follow |
| `docs/decisions.md` | Three rows (section 3, decisions 1, 4 and 5) |
| `README.md` | One paragraph that says how to open the settings |

No new dependency. `Cargo.toml` and `Cargo.lock` do not change.

## 2. Components

### 2.1 The list of keys, `src/settings.rs`

`sections()` serialises `Config::default()` with `toml::Value::try_from` and returns, in the
fixed order `audio, stt, rewrite, ui, delivery, context, ptt, record`, each section with its
keys (alphabetical, as the serialiser gives them) and each key's default value [D1]. A test
fails when the serialised defaults hold a section that the fixed order does not name, and when
the order names one the type lacks: a field added to the configuration is listed with no other
change, and a section added fails the test until it is placed.

A key is **set** when the file, parsed as a table, has it, whatever the value (reading C3); the
value shown is then the file's, and otherwise the default with `(default)`. A file that does not
parse is shown as all defaults, preceded by `config_note`.

Three kinds of key are not changed by typing:

- **secrets**, `[stt] token` and `[rewrite] token` (`SECRETS`): shown as `set` or `not set`
  (an empty string is not set), never as a value; choosing one says they are changed in the file
  and names the key. A test asserts that `SECRETS` holds every key named `token` in the
  serialised defaults, so a new token cannot be shown by accident.
- **arrays**, `[stt] command` and `[rewrite] command`: shown on one line (cut to 60 characters
  with `...`); choosing one says it is changed in the file and names the key.
- **keys with a chooser**: `[audio] input`, `[stt] model`, `[rewrite] model` (reading C7).

Every other key is a scalar and is changed by typing (2.4).

### 2.2 The seam, `src/popup.rs`

```
pub struct Io<'a> { pub input: &'a mut dyn BufRead, pub out: &'a mut dyn Write, pub failed: bool }
impl Io<'_> { pub fn say(&mut self, text: &str); pub fn ask(&mut self, question: &str) -> Option<String>; }

pub struct Snapshot { pub text: String, pub loaded: crate::config::Loaded }

pub trait World {
    fn snapshot(&self) -> Snapshot;                                   // the file as it stands, read again each call
    fn input_names(&self) -> Result<Vec<String>, String>;
    fn save(&mut self, edits: &[Edit]) -> Result<PathBuf, WriteError>;
    fn tell(&mut self) -> Reached;
    fn models_dir(&self) -> Option<PathBuf>;
    fn model_state(&self, entry: &catalogue::Entry) -> store::Glance;
    fn install(&mut self, entry: &catalogue::Entry, out: &mut dyn Write) -> Result<(), String>;
    fn rewrite_models(&mut self, url: &str, token: &str) -> Result<Vec<String>, ListFailure>;
}
```

`ask` writes the question, flushes (standard output is line buffered and a question has no
newline), reads one line and returns `None` at the end of input or on an error. `say` writes a
line. `failed` is set by any flow that ends in a failure the person has to act on; the popup's
exit code is 1 when it is set. Tests implement `World` with a fake that records every call; the
real one is `settings::Real`.

`Reached`, `reached` and `tell_daemon` are the ones `src/mic.rs` has (`reached` maps the result
of `client::exchange` onto `Applied`, `NoDaemon` or `Failed`). `parse_answer` and `prompt` are
the ones it has, generalised: `parse_answer(line, count)` gives `Leave` for an empty line or one
that begins with Esc (which is what an arrow key is as a line), `Pick(i)` for a number in range,
and `Invalid(text)` otherwise.

`change_note(section, &Reached) -> String` is the one place that says what a saved change does
(2.6).

### 2.3 The menus

`run_menu(world, io)` shows the sections, numbered, each with how many of its keys are set; reads
an answer; and for a number runs `keys_menu` for that section, which shows the keys numbered with
their values and runs the editor for the one chosen. An empty line or Esc leaves one level up; at
the sections level it ends the popup. A number outside the list is said ("is not one of the
numbers above; type a number between 1 and N") and the question is asked again; it is not a
failure. The end of input is said once ("nothing was read from the terminal") and ends the popup
with `failed` set.

The lists are numbered from 1, one line each, and the longest (`[stt]`, eight keys) with its
question fits the popup's 24 rows [D4]. After every change the file is read again with
`world.snapshot()` and the keys menu is drawn from that, so the list shows what was written.

### 2.4 The editor and the choosers

**Scalars.** The type comes from the default: boolean takes `true` or `false`; integer takes a
whole number; float takes a number; string takes the text as typed. A value that does not read as
the type is said ("`x` is not true or false") and nothing is written; an empty line leaves. The
value is written through the writer (`write_keys`) as TOML text, with `quote` for a string, and
the writer refuses a result that does not load (a negative number for an unsigned key), which the
popup prints as the writer says it. Three string keys with a fixed set of values are checked
before anything is written and refused with the set named: `[stt] engine` (`stt::ENGINES`: `candle`,
`http`, `command`), `[rewrite] engine` (`off`, `agent`, `http`, `command`) and `[context] source`
(`bias::source::VALUES`) [D2]; `[stt] engine` is checked against `stt::ENGINES`, and `[rewrite] engine` against the four names in `src/rewrite.rs`, which the popup holds as one constant.

**Microphone.** `[audio] input`: `input_names()`, the list `render` has today (numbered, the
configured one marked, a note for two inputs of one name, a line when the configured name matches
none and when the input is unset, and "No input devices were found" with nothing to choose),
a number, then the write and the daemon is told. The duplicate note is repeated after the choice.

**Speech model.** `[stt] model`: `chooser::choose_speech_model(world, io, &catalogue::MODELS)`
lists the catalogue with size, mel bins and the state `world.model_state` gives (installed, not
installed, installed but the wrong size), marks the configured model, asks for a number, and then
by the engine in the file:

- `candle`: when the chosen model is installed and is the configured one, it says so and stops;
  when it is installed and is not the configured one, it does not download; otherwise
  `world.install` downloads and verifies it. Then `[stt] model` is written, the daemon is told, and
  the change is reported (needs a restart of herdr).
- `command` with `{model}` in the command: nothing is downloaded or written. It says that this
  engine looks for `ggml-<model>.bin` in the models directory (named), that the catalogue installs
  weights for the built-in engine, and the two ways on: set `[stt] engine` to `candle` and choose
  the model again, or put that file in the models directory.
- `command` without `{model}`: nothing is written; the command brings its own model and
  `[stt] model` is not used.
- `http`: nothing is written; the server's model is `[stt] http_model` and `[stt] model` is not used.
- any other value: nothing is written; it names the three engines.

An empty catalogue says "no speech models are on offer" and that this is a build defect to report.

**Rewrite model.** `[rewrite] model`: with no `[rewrite] url`, it says there is no server to ask
and asks for the name to type. Otherwise `rewrite_models::models_url` derives the list's address
(a url that ends in `/chat/completions` has that replaced by `/models`; any other has no list
address), and `world.rewrite_models` makes one `GET`, with `Authorization: Bearer <token>` when the
token is set, bounded to ten seconds. A list with names is shown numbered with the configured one
marked, and a number chooses. A failure (unreachable, refused, no answer in time, an error status,
a body that is not `{"data":[{"id": ...}]}`, no entries, or no list address) is a `ListFailure`:

```
pub enum ListFailure {
    Server(String),   // the sentence http_failure::Cause::describe gives: unreachable, refused, silent, an error status
    NoAddress(String),// [rewrite] url does not end in /chat/completions, so there is no list address
    BadBody(String),  // the answer is not {"data":[{"id": ...}]}
    Empty,            // the list has no entries
}
```

each with a sentence that names the address and ends in what to do, followed by "type the model's
name instead"; the name typed is written. `ListFailure` is defined in `src/rewrite_models.rs` and
carries its sentence (`Display`). The token is never printed.

### 2.5 The real world, `settings::Real`

`snapshot` reads `config.toml` in the configuration directory and calls `config::load`. `save` is
`write_keys` over the directory. `tell` sends `reload` with `client::exchange` to the address
`transport::address` gives. `models_dir` is the state directory's `models`. `model_state` is
`store::glance`. `install` is `fetch::model` with a progress line that overwrites itself, then
`store::locate` (the real check, the one `chooser::run` runs today). `rewrite_models` is
`rewrite_models::fetch`. `input_names` is `cpal_source::input_names`.

`settings::run()` builds `Real`, runs the menu over standard input and output and waits for Enter
once at the end when standard input is a terminal (`pause`). `settings --open` runs
`herdr plugin pane open --plugin <id> --entrypoint settings` through `outward::run` with a
ten-second bound, as the microphone action did [E12]; `open_command` takes the entrypoint as a
parameter.

### 2.6 What a change says

`change_note(section, reached)`:

| Reached | Section `audio` | Any other section |
|---|---|---|
| `Applied`, the reply lists the section under "applied" | "The daemon applied it: the next take records from it." | (cannot happen: only `audio` is applied) |
| `Applied`, nothing applied and the section is not listed to restart | "The daemon already uses it; nothing to apply." | "The daemon already runs with this value; nothing to apply." |
| `Applied`, the section is listed to restart | (cannot happen) | "This needs a restart of herdr to apply: `<section>`." |
| `Applied`, other sections are listed to restart | those are added: "Other changes in the file also need a restart of herdr: `<list>`." | the same |
| `NoDaemon` | "No dictation daemon is running, so nothing was told; the change applies when it starts." | the same |
| `Failed(why)` | "The file was changed, but the daemon did not take it: `<why>`. Restart herdr, or check `herdr plugin log list --plugin herdr-voice`." and `failed` is set | the same |

A write that is refused or fails prints the writer's error; only an error that means the file could
not be written (`needs_hand_edit`) is followed by "Add this under `[<section>]` in your
configuration file by hand:" and the line.

## 3. Decisions

Each in four parts: context, the problem, the decision, the reason.

**1. The list of keys is read from the configuration type.** Context: the popup lists every
section and key and a test must fail when one is missing [E1]. Problem: a table kept by hand drifts
from the type the day a key is added. Decision: derive `Serialize` on the configuration types and
list the keys of the serialised defaults, with a fixed order of sections and a test that checks the
order against them. Reason: the type is the list; an added field appears with no other change, and
only a new section needs the order to name it. Recorded in `docs/decisions.md`.

**2. One seam, `World`.** Context: the popup touches the file, the daemon, the sound devices, the
models directory, a download and a server, and has to be tested with none of them [E8]. Problem:
passing six closures through every flow makes each signature unreadable and every test setup long.
Decision: one trait, `World`, which `settings::Real` implements for a run and a fake implements for
a test; every flow takes `&mut dyn World` and `&mut Io`. Reason: a flow is a function of what it
reads, what it prints and what it asks the world to do, so it is tested by scripting the input and
reading the output and the recorded calls.

**3. Keys with a chooser are not changed by typing.** Context: `[stt] model` typed freely could be
set to a model the engine will not use, which is #46 again [E9]. Decision: `[audio] input`,
`[stt] model` and `[rewrite] model` open their own flow. Reason: the flow is where the engine, the
installed state and the server's list are known.

**4. The speech model is written only for an engine that uses it, and the engine is never switched
by the popup.** Context: the catalogue installs weights for the engine `candle`; the default engine
is `command`, which looks for another file [E9]. Problem: writing `[stt] model` with a download for
the wrong engine breaks a setup that worked (#46); writing `[stt] engine` as well would move a
`whisper-cli` setup to an engine that measured about ten times slower for the same model [E9].
Decision: for `candle`, install and write; for any other engine, change nothing and say what to do.
Reason: the person decides the engine, in the same popup, with `[stt] engine`. Recorded in
`docs/decisions.md`.

**5. A change outside `[audio]` is applied by a restart of herdr, and the popup says so.** Context:
the speech engine and the rewrite engine are built once at start and kept [E6]. Problem: replacing
a running engine adds a failure mode for a change made a few times in a year. Decision: the daemon
does not swap an engine; the reload reply names the section and the popup says "needs a restart of
herdr". Reason: the daemon is started by herdr, so herdr's restart is the only restart there is.
Recorded in `docs/decisions.md`.

**6. The pane `mic` goes and the pane `model` stays.** Context: the settings popup lists the inputs
itself, and nothing opens a second pane [E7]. Decision: `mic` (command, action, pane, binding) is
removed; the pane `model` is kept, because the owner's change names the microphone only and the
pane keeps working through the shared function. Reason: no entry in the manifest is left that
nothing opens.

**7. A number outside the list asks again.** Context: the earlier popup ended on one, which suited
a single question. Decision: in a menu an answer that is not a number in range is said and asked
again; only an empty line, Esc or the end of input leaves. Reason: leaving a menu on a typo costs
the person a reopen.

## 4. What each criterion rests on

| Criterion | Where | How it is checked |
|---|---|---|
| AC-1 to 9 | `src/config_edit.rs` (unchanged) | its tests, which stay |
| AC-10, 11 | `Recorder::reconfigure`, `reload_from` (unchanged); the microphone flow | the existing tests; a flow test that a choice saves `[audio] input` and tells the daemon; by hand |
| AC-12 | `docs/decisions.md` | read |
| AC-13, 14 | `change_note` | a test per row of the table in 2.6 |
| AC-15 to 20 | the microphone flow | tests over a fake `World` (the tests of `mic::choose` move and adapt) |
| AC-21, 42 | `src/main.rs`, manifest, `setup` | the existing tests of `main`, `setup` and `check_manifest`, adjusted |
| AC-22, 39, 40 | manifest, `setup`, `settings --open` | check_manifest; a test of the command line; by hand in AC-39's terms |
| AC-23, 24 | `sections()`, the keys menu | tests: every serialised key appears in the menu; set keys carry the file's value and defaults carry `(default)`; the order test |
| AC-25 | the scalar editor | tests per type, a refusal for a wrong type and for a fixed-set value, an empty line leaves |
| AC-26 | the keys menu and every flow | a test with a known token value in the file and in the server's request: no output contains it |
| AC-27 | `Real::save` | the writer writes only `config.toml` in the configuration directory (its tests) |
| AC-43 | the array and secret branches | tests |
| AC-44 | `settings::run` | by hand: the popup ends by itself after Enter; a test that `run_with` pauses once |
| AC-28 to 31 | `chooser::choose_speech_model` | tests over a fake `World` for each engine, an empty catalogue, installed and not, and the configured model; one of them has the fake save through `config_edit::write_keys` to a temporary configuration file with the download replaced, which is what AC-31 asks of `run()` since `run()` is only a wrapper that builds the real `World` |
| AC-32, 38 | by hand | after the restart named, a take uses the model; recorded as not verified where it cannot be run |
| AC-33 to 37 | `rewrite_models` and the rewrite flow | tests with a local fake server that records every request: one `GET`, the bearer header, the failures |
| AC-41 | manifest | `scripts/check_manifest.py` |

## 5. Risks and what is not established

- **UNVERIFIED** whether `herdr plugin pane open --entrypoint settings` opens the popup placement;
  whether herdr closes a popup the moment its command exits (the popup waits for Enter once at the
  end); whether the popup's environment carries `HERDR_PLUGIN_STATE_DIR` [E15]. The step on the
  owner's machine is `herdr plugin action invoke herdr-voice.settings`.
- The keys menu fits 24 rows for the largest section [D4]; whether herdr gives a popup that height
  on a small terminal is not known. A longer list scrolls and the top is lost; the sections menu
  and the keys menus are short by design.
- A value the editor accepts and the daemon later rejects (an engine name is checked; a language
  code, a command, a context source count are not) shows up in `doctor` and the daemon's journal,
  as it does for a value typed into the file.
- Windows is checked for dead code with the project's scratch-copy method; nothing here is
  platform-specific.

## 6. Not done here

Arrow keys without Enter (#115); changing a token or an array in the popup; replacing a running
engine; any setting that does not exist in `src/config.rs`.
