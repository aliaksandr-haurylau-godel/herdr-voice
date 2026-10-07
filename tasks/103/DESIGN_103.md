# DESIGN_103 — The microphone popup, the configuration writer, the reload request

Covers `AC_103.md` (criteria AC-1 to AC-22, AC-39 to AC-41) for #103 and #49. It
introduces nothing beyond them. Facts are in `DESIGN_103_evidence.md` as [D1], [D2]
and so on; facts about the existing code are in `AC_103_evidence.md` as [E1], [E2].
Anything marked UNVERIFIED was not established and is settled by the step named.

## 1. What changes

| Where | Change |
|---|---|
| `src/config_edit.rs` (new) | The writer: edits one or more keys of the configuration file, refuses a result that would not load, replaces the file whole |
| `src/chooser.rs` | `set_model_key`, `table_name` and `write_model_key` move out; the chooser calls the writer for `[stt] model` |
| `src/mic.rs` (new) | The popup: list, answer, write, tell the daemon; and the action that opens the popup |
| `src/capture/cpal_source.rs` | `input_names()`: the machine's input names |
| `src/capture.rs` | A `Reconfigure` order for the recorder's thread, and `Recorder::reconfigure` |
| `src/reload.rs` (new) | The reply of a reload: one function writes it, one reads it, and a type holds what it says |
| `src/client.rs` | `exchange`: the body of `send_to` up to the point where it is turned into an `Outcome`, returning the reply or the `ClientError`; `send_to` becomes `outcome(exchange(..))` and behaves as before |
| `src/daemon.rs` | The `reload` request; the configuration the daemon is running, kept in `Runtime` |
| `src/main.rs` | `mic` is built: `--choose`, `--open`, and the list when neither is given; usage text; the list of implemented commands |
| `herdr-plugin.toml` | One action, `mic`, whose command opens the popup |
| `src/setup.rs` | A fourth binding for that action; wording that counts bindings counts them from the list |
| `docs/decisions.md` | Two entries (section 3, decisions 2 and 4) |

No new dependency.

## 2. Components

### 2.1 The writer, `src/config_edit.rs`

```
pub struct Edit<'a> { pub table: &'a str, pub key: &'a str, pub value: String }
pub fn quote(text: &str) -> String                         // a TOML basic string, one line
pub fn set_keys(existing: &str, edits: &[Edit]) -> String  // pure
pub fn write_keys(directory: Option<&Path>, edits: &[Edit]) -> Result<PathBuf, WriteError>
pub enum WriteError { NoDirectory, Unreadable { path, why }, AlreadyInvalid { path, why },
                      Refused { path, why }, Io { path, why } }
```

`value` is TOML text, built by `quote` for a string. The second pull request adds
helpers for booleans and numbers; the shape does not change.

`set_keys` is the existing line-oriented `set_model_key` generalised: for each edit, in
order, it replaces the first `key =` line inside `[table]`, or adds the line at the end
of that table, or adds the table at the end of the file. The tests of
`set_model_key` move with it and gain the table and key as parameters [E10]. The edit
touches one line of the file and nothing else.

`write_keys` does, in order:

1. no directory known: `NoDirectory`;
2. create the directory;
3. resolve a symbolic link to the file it points at, as `src/setup.rs` does, so the link
   survives and the real file changes [D1];
4. read the file: absent is the empty text; any other read failure is `Unreadable`, and
   nothing is written. The old code treated that failure as an empty file and replaced
   the file [E9];
5. if the original text does not load as the configuration, `AlreadyInvalid`, and
   nothing is written: the daemon is already running on defaults, and an edit would be
   blamed for what it did not break;
6. build the edited text with `set_keys`;
7. parse the edited text **as `Config`**, the type `config::load` parses [E6, E8, E8a];
   a failure is `Refused`, and nothing is written;
8. write the text to a candidate file beside the target, give it the target's
   permissions, rename it over the target. A failed step removes the candidate and is
   `Io`.

Step 8 is a replace, not a write in place, because a configuration file can hold
tokens and a truncating write that fails halfway leaves an empty file, which
`config::load` reads as "use every default" [E6]. The permissions are copied for the
same reason: a new file would be readable by others where a token file was not [D1].

`quote` writes one line: backslash and double quote escaped, control characters as
escapes. The `toml` crate's own rendering is not used because it writes a string with a
line break as a multi-line string, which the line-oriented edit cannot replace [D2].

Every error names the file and what to do. `Refused`, `AlreadyInvalid` and `Unreadable`
say the file was not touched; `Io` and `NoDirectory` add the line to put in the file by
hand (the caller knows the edit, and prints `[table]` and `key = value`).

### 2.2 The popup, `src/mic.rs`

`herdr-voice mic --choose` is the popup. It is written as a function over its inputs so
that it runs in a test with no device and no terminal:

```
fn choose(names: &[String], configured: &str, answer: &mut dyn BufRead,
          out: &mut dyn Write, save: &mut dyn FnMut(&str) -> Result<PathBuf, WriteError>,
          tell: &mut dyn FnMut() -> Reached) -> u8
```

`Reached` is local to `src/mic.rs`: `Applied { restart: Vec<String> }` (the daemon took the
change; `restart` lists sections that need a restart), `NoDaemon`, and `Failed(String)`.
One function maps the result of `client::exchange` onto it: `Err(ClientError::NoDaemon(_))`
is `NoDaemon`; `Ok(Reply::Ok(text))` is read by `reload::parse` into `Applied`, or
`Failed` when the text is not a reload reply; `Ok(Reply::Error(text))` and every other
`Err` are `Failed` with the text. `choose` never sees `client::Outcome` or matches on
message text.

`run(choosing)` supplies the real ones: `input_names()`, `config::load`, the writer with
`[audio] input`, and `client::exchange` with the `reload` request, mapped onto `Reached`.

What it prints and reads, in this order:

1. The list, numbered from 1, one name per line, the configured one marked `(current)`.
   Two devices with the same name are both listed and the second carries a note that the
   plugin selects by name and uses the first of them [E13, `cpal_source.rs`].
2. When the configured name is set and matches no device: a line saying
   `[audio] input is "<name>", which matches none of these inputs; choose one, or the
   plugin stops with this name when recording`. When it is empty: `[audio] input is
   not set, so the system default input is used`. When there are no devices: `no input
   devices were found; connect a microphone and open this again` and exit 1, nothing
   written.
3. The prompt: `Type the number of the input, then Enter. Esc then Enter, or an empty
   line, leaves it as it is:`. It is flushed before the read, because standard output
   is line buffered and a prompt with no newline stays unseen [E14, D4].
4. The answer, one line. Starting with Esc, or empty: `nothing was changed` and exit 0.
   A number in range: that device's name. Anything else: `"<text>" is not one of the
   numbers above; open this again and type a number between 1 and N`, exit 1, nothing
   written.
5. The write, then the request to the daemon (2.4). The popup prints what the writer
   did and what the daemon answered, each as one line; section 2.5 lists them.

An arrow key arrives as an Esc-led sequence, so an arrow followed by Enter is read as
Esc and leaves without a change. The prompt says to type a number. No terminal-mode
code is written.

`mic` with no flag prints the list and exits 0, as `model` does. `mic --open` is the
action (2.5).

### 2.3 The recorder and the input names

`input_names()` in `src/capture/cpal_source.rs` returns the names `CpalSource::start`
builds its list from, so the popup and the take agree on what a name is [E13]. It is the
only code that asks cpal for devices, so it lives with the one that already does.

The recorder's thread owns an `Audio` that it passes to `start_one` and `stop_one`
[E3]. A `Reconfigure { audio }` order replaces it. The thread handles orders one at a
time, in the order they arrive, so a reconfigure is applied after the order in flight
and before the next. A take already recording holds its device in `Running.device`
[E4]; it stops on that device whatever the new value is. `stop_one` reads
`audio.silence_db` to judge the finished take [E4]; a reconfigure that changes it between
start and stop makes that take judged by the new value, which is accepted: it is the
value in the file when the take ends.

### 2.4 The request, `reload`

Context: the daemon reads the file once, at start [E2], and answers requests [E5].

`answer` gains a `"reload"` command that needs no target pane. It runs on the
connection thread, not on the recorder's, so reading the file here does not put file
access on the path of a keypress. It does the following:

1. Load the file with `config::load`. A file that is `Source::Invalid` is not applied:
   the reply is an error saying the file does not parse, why, and that nothing changed.
   An absent file is the defaults, as at start.
2. Compare the loaded configuration with the one the daemon is running, a `Config` kept
   in `Runtime` behind a mutex, set at start.
3. For `[audio]`, when it differs: send the recorder `Reconfigure`, and record the new
   `[audio]` as running.
4. For every other section that differs: it is not applied and not recorded as running;
   the reply names it as needing a restart. It stays so in every later reply until the
   daemon restarts, because the running configuration still holds the old value.
5. Reply with one line, written by `reload::reply(applied, restart)`: `applied: audio` or
   `applied: nothing`, then, if any, `; needs a restart: <sections>`. `reload::parse` reads
   it back; the two sit together and a test round-trips them. One line, because the protocol reads a reply with a
   single `read_line` [D3].

`ping` and every other request are untouched. The request uses the short client bound
(`client::timeout_for` gives 2 s to every command but `dictate`), which a small file read meets [D5].

The second pull request widens step 3 to more sections and adds the speech-model case;
this pull request fixes the mechanism, the reply and the running configuration.

### 2.5 Opening the popup, and the messages

The action `mic` in `herdr-plugin.toml` runs `herdr-voice mic --open`. `--open` runs
`<herdr> plugin pane open --plugin <id> --entrypoint mic`, where `<herdr>` is
`HERDR_BIN_PATH` or `herdr` [E17, D6] and `<id>` is
`HERDR_PLUGIN_ID` or `herdr-voice`, through `outward::run` with a bound, as every call to
another program is [E22]. The command line is built by a function and tested without
running it. A failure prints what herdr said and `check herdr plugin log list --plugin
herdr-voice`, exit 1.

A key opens it by a `plugin_action` binding to `herdr-voice.mic`, the way `setup`
binds the other three [E16]. The binding is a fourth entry in `BINDINGS`: action `mic`,
key `prefix+shift+i`, description `dictation: choose a microphone`. The key is a
proposal until the owner names it [AC-40]; it is one line in `BINDINGS`. Where `setup`
says "three bindings" it counts from the list [`src/setup.rs:878`, its tests].

Messages, each ending in the next step. Every line is printed, then the popup waits for
Enter before it exits (decision 5):

| Situation | Says | Exit |
|---|---|---|
| Written, daemon reports `applied: nothing` | `[audio] input is now "<name>". The daemon already uses it; nothing to apply` | 0 |
| Written, daemon applied | `[audio] input is now "<name>". applied: audio` and "the next take uses it" | 0 |
| Written, daemon reports others need a restart | the same, plus `needs a restart: <sections>` and "restart herdr to apply those" | 0 |
| Written, no daemon | `[audio] input is now "<name>". No dictation daemon is running; the change applies when it starts` | 0 |
| Written, daemon answered an error or did not answer | `written, but the daemon did not take it: <reply>; restart herdr, or check herdr plugin log list --plugin herdr-voice` | 1 |
| Writer refused | the writer's error and the file untouched | 1 |
| Writer could not write | the error, then `Add this under [audio] in <file> by hand:` and `input = "<name>"` | 1 |

## 3. Decisions

Each in four parts: context, the problem, the decision, the reason.

**1. The writer replaces the file whole.** Context: the old `write_model_key` truncated
the file in place [E9]. Problem: a failed write leaves an empty file, which loads as
"every default", and a replace by rename drops permissions and replaces a symbolic
link. Decision: write a candidate beside the real file (following a link), copy the
permissions, rename it over. Reason: the file may hold tokens and may be a link into a
dotfiles repository; `src/setup.rs` does the same for herdr's file for the same reasons
[D1].

**2. The daemon learns of a change by a request, `reload`.** Context: the file is read
once and the daemon keeps the speech model resident [E2, E3]. Problem: a read before
each take puts file access on the keypress path, and could not apply a speech-model
change without replacing the engine. Decision: the popup sends `reload` after a
successful write; the daemon re-reads, applies what it can, and says what it applied
and what needs a restart. Reason: one mechanism for both popups, and the reply is what
lets the popup say truthfully whether a change applies (AC-13). A file edited by hand is
not picked up until a restart, as before. Recorded in `docs/decisions.md`.

**3. A reload applies `[audio]` between takes and leaves a take in progress alone.**
Context: the recorder pins a take's device when it starts [E4]. Problem: the take
must finish on the device it started on (AC-11). Decision: `Reconfigure` goes through
the recorder's own order queue and replaces its `Audio`; nothing touches `Running`.
Reason: the thread already serialises starts and stops, so no new lock is added.

**4. A key opens a popup through a manifest action.** Context: herdr's binding types
are `shell`, `pane`, `popup` and `plugin_action`; none opens a plugin's declared pane
[E15], and an installed plugin opens one with an action whose command runs `herdr
plugin pane open` [E17]. Problem: without an action a key has nothing to bind.
Decision: an action `mic` per popup that opens its pane, and a `plugin_action` key to
it. Reason: it is the mechanism herdr offers and the one `setup` already writes. Whether
`pane open` honours the pane's `placement = "popup"` without `--placement` is UNVERIFIED
[E18]; it is settled in an isolated herdr session (section 5), and if it does not, the
action passes the placement explicitly. Recorded in `docs/decisions.md`.

**5. The popup waits for Enter before it exits.** Context: the popup is a pane that runs
a command [E12]. Problem: whether herdr closes a popup the instant its command exits is
UNVERIFIED, and a result nobody can read is a silent failure. Decision: after the last
message the popup prints `Press Enter to close.` and reads a line, only when standard
input is a terminal. Reason: it costs one keypress where the pane stays open and saves
the message where it does not. Settled in the isolated session (section 5); if the pane
stays open on its own, the wait is removed.

**6. A name is written as one escaped line.** Context: the edit is line-oriented [E10].
Problem: the `toml` crate writes a string with a line break over several lines [D2].
Decision: `quote` writes a basic string on one line. Reason: the replacement of a
one-line value stays one line; a test round-trips quote, backslash, non-ASCII and
control characters through the parser.

## 4. Data flow

```
key -> herdr -> action mic -> herdr-voice mic --open -> herdr plugin pane open
                                                           |
                                         popup pane: herdr-voice mic --choose
  input_names() ---> list ---> answer ---> write_keys([audio] input = name)
                                              |  ok
                                              v
                              client::exchange("reload") ---> daemon.answer
                                                               |  config::load, diff, Reconfigure
                                                               v
                                   reply: "applied: audio" ---> popup prints it
```

## 5. What each criterion rests on

| Criterion | Where | How it is checked |
|---|---|---|
| AC-1, 9 | `set_keys`, `write_keys` | unit tests on text and on a temporary directory |
| AC-2, 3, 4 | step 7 of `write_keys` | a test with a wrong-typed value; a test per refusal; removing step 7 turns one red |
| AC-5 | steps 1, 2, 8 | temporary directory with a read-only parent, and a path that is a directory |
| AC-6 | step 4, 2 | temporary directory with nothing in it |
| AC-7 | step 4 | a file with bytes that are not UTF-8, and (unix) mode 000 |
| AC-8 | `set_keys` over several edits, step 7 once | a test with model and engine; one failing edit leaves the file unchanged |
| AC-10 | `reload`, `Reconfigure` | a recorder with a fake source: the second start asks the fake for the new name |
| AC-11 | `Reconfigure` after `Start` | a fake source that records the name it was opened with; a reconfigure while running does not change that take |
| AC-12, AC-39 | `docs/decisions.md`, `docs/evidence.md` | read; the isolated herdr session |
| AC-13 | `reload` reply | tests: `[audio]` changed; `[stt]` changed; both; file invalid; file absent |
| AC-14 | `mic::choose` with `tell` returning `NoDaemon`; the mapping from `client::exchange` | a test of the message; a test that each `ClientError` and `Reply` maps to the right `Reached` |
| AC-15 to 20 | `mic::choose` | tests over `choose` with a list, answers and a recorded write |
| AC-21 | `main.rs` | the test of implemented commands and the usage text |
| AC-22 | manifest, `setup`, `--open` | check_manifest; a test of the command line; by hand in the isolated session; then on the owner's machine |
| AC-40 | `BINDINGS` | the key, title and action id are one line each, marked as proposals |
| AC-41 | manifest | `scripts/check_manifest.py` |

By-hand verification (AC-10, AC-22, decisions 4 and 5): an isolated herdr server under
a session of its own, with its own state and configuration directories and a plugin
link to this worktree's build, never the owner's session [brief]. Recorded in
`docs/evidence.md` with the platform. If a safe isolated setup cannot be built, the run
stops and says so.

## 6. Risks and what is not established

- **UNVERIFIED** that `herdr plugin pane open --entrypoint mic` opens the popup
  placement [E18], and whether a popup closes when its command exits (decision 5).
- **UNVERIFIED** that a popup pane's environment carries `HERDR_PLUGIN_STATE_DIR`, so
  that the popup derives the same socket address as the daemon. The derivation falls
  back to the home directory the same way for both [E3, `src/transport.rs`]; the
  isolated session shows whether they agree.
- Listing devices in the popup process while the daemon records is not known to
  disturb the recording; it is observed in the isolated session.
- A value spanning several lines in the file cannot be replaced by a line edit; the
  parse check turns the damage into a refusal, not a corrupt file. This pull request
  edits only one-line string values.
- The Windows build is checked for dead code with the project's scratch-copy method;
  nothing here is platform-specific except the permission copy and the unix-only tests.

## 7. Not done here

Arrow keys without Enter; choosing the system default again; the settings popup, the
speech model and the rewrite model (the second pull request); more than `[audio]` in a
reload.
