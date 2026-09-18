# DESIGN_73 — the plugin id becomes `herdr-voice`

Written from `AC_73.md`, which the designer gate passed on round 2. Every section
below answers one or more of AC-1 to AC-10 and adds nothing they do not ask for.

## 1. Two constants, and what derives from each

### Context

`PLUGIN_ID` in `src/transport.rs:18` is the single source of the id. Four things
derive from it — the state directory, the socket or pipe name, the configuration
directory, and the `<id>.<action>` command a keybinding names — and the sidebar
token is written with it after `--source`.

### Problem

The migration has to talk about two ids at once: the one the plugin now uses, and
the one the person's machine still carries. A second literal spelling of the old
id in every place that needs it is the drift the file already warns against.

### Decision

`src/transport.rs` declares both:

```rust
pub const PLUGIN_ID: &str = "herdr-voice";
pub const LEGACY_PLUGIN_ID: &str = "haurylau.voice";
```

`LEGACY_PLUGIN_ID` has exactly two kinds of use: the superseded-binding command
strings in `src/setup.rs`, and the two legacy locations `setup` reports on.

The legacy locations are **not** computed by running the existing derivation with
the other constant. Established by running, against herdr 0.9.1: a plugin action
is given `HERDR_PLUGIN_CONFIG_DIR` and `HERDR_PLUGIN_STATE_DIR`, and both end in
the id herdr knows the plugin by — after this change, the new one. The first
branch of `config::directory` (`src/config.rs:250-253`) and of
`state_directory` (`src/transport.rs:113-117`) returns that override verbatim and
never looks at an id, so a call with `LEGACY_PLUGIN_ID` would hand back the
directory the plugin reads today and the socket the running daemon owns.

The legacy location is derived from the current one, by its last component:

```rust
/// The sibling of `current` under the legacy id, or `None` when `current` is not
/// this plugin's own directory.
pub fn legacy_sibling(current: &Path) -> Option<PathBuf> {
    if current.file_name().and_then(|n| n.to_str()) != Some(PLUGIN_ID) {
        return None;
    }
    Some(current.parent()?.join(LEGACY_PLUGIN_ID))
}
```

`setup` applies it to the configuration directory the plugin is actually using,
and to the state directory the socket sits in. When the last component is not
`PLUGIN_ID` — herdr pointed the plugin somewhere of its own choosing, or a test
did — there is no legacy location to speak of and nothing is reported.

**The invariant.** `legacy_sibling(current)` is never equal to `current`: it is
returned only when the last component is `PLUGIN_ID`, and it replaces that
component with `LEGACY_PLUGIN_ID`, which is a different string. A test asserts it
over both branches, including the one where herdr's override is in force. That
property is what makes it impossible for `setup` to report the live configuration
as orphaned or to tell a person to kill the daemon they are running.

### Why

One spelling of each id, and one derivation. Deriving the legacy location from the
current one also survives herdr putting plugin directories somewhere this plugin
does not predict: whatever root herdr chose, the old id's directory is its sibling.

## 2. What `setup` does about blocks naming the old id

### Context

`inspect` reads every `[[keys.command]]` block into `(key, command)` pairs
(`src/setup.rs:122-152`). `decide` sorts this plugin's three bindings into
`to_add`, `already` and `blocked` by comparing command strings exactly
(`src/setup.rs:166-203`). `run` reports each list and then offers `to_add`
(`src/setup.rs:499-620`).

### Problem

A block naming `haurylau.voice.ptt` is, to `decide`, a stranger holding `ctrl+g`.
The person is told that something else owns their key and that they should pick
another one by hand — about a binding they added themselves, on this plugin's
own instruction, one version ago. Appending is not an escape: two
`[[keys.command]]` on one key make herdr answer `config: issues found` and
disable the later one (`docs/evidence.md:1004-1008`), and `append` refuses a
candidate herdr refuses.

### Decision

`Decision` gains a fourth list, `superseded: Vec<(&'static Binding, String)>` —
the binding and the key its old block sits on. `decide` fills it before it tests
for `already` and before it tests for a blocking holder: a command equal to
`format!("{LEGACY_PLUGIN_ID}.{action}")` is this plugin's own past, so its key is
never reported as held by a stranger.

A file that carries both a legacy block and a current block for the same action
therefore comes out with two keys on that command. Both blocks were the person's
own, herdr accepts two keys for one action, and the alternative — leaving the
legacy block alone — leaves them a key that does nothing. Two working keys is the
better of the two.

`run` reports each one as what it is, and then asks a single question covering
both halves of the repair:

```
superseded: ctrl+g in /path/to/herdr/config.toml carries this plugin's previous
id, haurylau.voice.ptt. The id is now herdr-voice, so that key does nothing when
it is pressed.

rewrite 3 bindings to name herdr-voice in the file named above? [y/N] then Enter:
```

One line per superseded block, naming its key and the file it sits in, as AC-6
requires. The question's count covers both halves — "rewrite 3 bindings" alone
when nothing is left to append, "rewrite 3 bindings and append 1" when something
is. On `y` the rewrite and the append happen in one write, so the file is judged
by herdr once and renamed once. On anything else nothing is changed, which is
what the same question already means for the append.

The early return at `src/setup.rs:576-590` changes with this. Today an empty
`to_add` prints "nothing to add" and returns before any question is asked; it
must now do that only when `superseded` is also empty, and otherwise fall through
to the question.

### Why

The key does not change and the person's choice of it does not change; only the
name of the action behind it is now wrong. Reporting that as a stranger's key
tells the person something false, and telling them to delete three blocks by hand
leaves the repair to them for a rename they did not ask for.

## 3. How the rewrite touches the file

### Context

The file is hand-written and commented. On the machine this runs on, the
`ctrl+g` block carries three lines above it recording why that key was chosen —
that `alt+...` depends on the terminal, that `alt+v` did nothing and `alt+g`
typed a copyright sign. `a_commented_hand_written_file_keeps_every_byte_it_had`
(`src/setup.rs:1047`) exists because of that.

### Problem

A rewrite that removes a block and appends its replacement leaves the comment
above whatever follows, describing a binding that is no longer there. A rewrite
through a TOML value tree loses every comment in the file and reformats the rest.

### Decision

The rewrite is a line edit over the file's text. Walking the lines in order:

- a line whose trimmed form is `[[keys.command]]` opens a block; any other line
  whose trimmed form starts with `[` closes it;
- inside an open block, a line whose trimmed form is
  `command = "<LEGACY_PLUGIN_ID>.<action>"`, for one of this plugin's three
  actions, has that quoted value — and only that value — replaced by
  `"<PLUGIN_ID>.<action>"`. Leading whitespace, the spacing around `=` and
  anything after the value on the line, an inline comment included, are copied
  through untouched;
- every other line is copied byte for byte.

Nothing else in the file is examined and nothing else is written. A comment that
merely mentions the old command is not an assignment, so it is not rewritten:
what it says stays what it said.

`toml_edit` would do this too, and it is a dependency this repository has not
taken. `docs/decisions.md` admits `serde`, `serde_json` and `toml` and nothing
else without a decision, and a line edit over four exact forms needs neither a
parser nor a document model.

### Why

The key stays where it was, so a comment attached to the key stays true. Byte
equality everywhere else is checkable in a test rather than argued about, and it
is the same promise `append` already makes.

## 4. The one write path

### Context

`append` (`src/setup.rs:417-487`) reads the file, asks herdr to judge it, builds
the new text, writes a candidate beside the real file, carries the original's
permissions onto it, asks herdr to judge the candidate, and renames. A symbolic
link is resolved first so the file the link points at is the one that changes.

### Problem

The rewrite needs all of that and differs only in how the new text is produced.
A second copy of the sequence is a second place for the symbolic-link handling,
the permissions and the two checks to be got wrong.

### Decision

The body is extracted as

```rust
fn commit(herdr: &dyn Herdr, path: &Path, make: impl FnOnce(&str) -> String)
    -> Result<(), WriteError>
```

which does everything `append` does now and calls `make` on the original text —
empty when the file does not exist — to get the candidate's text. `append` keeps
its signature and becomes one call to `commit`; the rewrite is a second. Both
`WriteError` and its messages are unchanged.

### Why

One write path means one place where the file is judged before it is replaced and
one place where the rename happens. `append`'s tests keep testing the machinery
through `append`, and the rewrite's tests add the cases only it has.

## 5. What else `setup` reports

### Context

Two more things survive the rename on a machine that had the plugin: the
configuration file under `<config>/herdr/plugins/config/haurylau.voice/`, which
the plugin no longer reads, and a daemon started under the old id, which is
reparented to process 1 and which no herdr restart ends.

### Problem

A person who repairs the three keys and stops there runs the plugin on defaults —
the local speech model and the rewrite endpoint gone from under it — and leaves a
process alive holding the old socket. Neither says anything about itself.

### Decision

After the binding report, and whether or not anything is rewritten, `setup`
reports what it finds:

- **The old configuration file.** When the plugin's own configuration directory
  has a legacy sibling and a `config.toml` exists in it, `setup` names that file,
  names the directory the plugin now reads, and gives the one command that moves
  it. It does not copy it. The plugin does not own that file, and
  folding a copy of a person's configuration into the same `[y/N]` that rewrites
  their keybindings asks two consents with one answer.
- **The old daemon.** On macOS and Linux, `setup` connects to `voice.sock` inside
  the legacy sibling of the state directory. A connection that succeeds
  means a daemon is alive there; `setup` names the socket path and the one way to
  end it, `kill $(lsof -t <path>)` — established by running: `lsof -t` on the
  live socket returned exactly the pid `pgrep` reports for the daemon. Nothing is
  sent on the connection and nothing is killed by `setup`. On Windows the legacy
  address is a name in the pipe namespace with no path and no `lsof`, so the
  sentence is not printed there; Windows as a whole is unverified and is issue #3's
  third open question.

### How the two paths reach `run`

`run` today takes the herdr configuration's path and nothing else about the
environment (`src/setup.rs:499-506`). The two legacy paths are derived from
environment variables, and `src/setup.rs:71-75` already states why this module
takes such values rather than reading them: the tests run in parallel and must
not mutate an environment they share.

So `run` gains one parameter, a small owned struct:

```rust
pub struct Legacy {
    /// The old configuration file, when one is there.
    pub config_file: Option<PathBuf>,
    /// The directory the plugin reads now, which the same sentence names.
    pub current_config_dir: Option<PathBuf>,
    /// The old socket, on unix, when a daemon answers on it.
    pub daemon_socket: Option<String>,
}
```

`setup::main` builds it from the two directories the plugin is actually using —
`config::directory(&config::Vars::from_env())` and
`transport::state_directory(&transport::Vars::from_env())` — by taking each one's
legacy sibling, testing the file and attempting the connection. The tests build
the struct directly. Both the filesystem check and the connection attempt happen
once, outside `run`; `run` reports only what it is given, and so has no branch
that depends on the environment.

### Why

The issue names three things the rename breaks. Repairing one of them and saying
nothing about the other two is the silent failure this repository weighs the same
as a wrong transcript.

## 5a. The notice at daemon start

### Context

`setup` tells a person everything the rename left behind, once they run it.
Nothing tells them to run it. herdr starts the daemon from the manifest's
`[[startup]]` entry when the server comes up — measured, `docs/evidence.md:270-273`
— and an action does not start it: the client's own message tells a person to
"restart herdr so the plugin's startup entry does" (`src/client.rs:70-71`). So the
daemon starts exactly once per herdr start, which is also the first moment herdr
knows this plugin by its new id.

### Problem

A person who has relinked and restarted herdr presses `ctrl+g` and nothing
happens. `CLAUDE.md` has already priced that: "Every user-visible failure names
what to do next. 'Silent failure' is a defect of the same weight as a wrong
transcript." An explanation that exists only inside a command they would have to
guess at does not name what to do next.

### Decision

`daemon::start`, after the configuration is read and the runtime exists, reads the
herdr configuration through the functions `setup` already has — `config_path`,
then `inspect` — and collects the keys of the `[[keys.command]]` blocks whose
command is `<LEGACY_PLUGIN_ID>.<action>` for one of the three actions. The keys
are collected in the order `BINDINGS` declares them, so the sentence does not
depend on the order somebody's file happens to be in.

When the list is empty, nothing is raised and nothing is written. When it is not,
exactly one notice is raised, with this title and this body:

```
Dictation: the plugin id changed
<n> <key|keys> still <names|name> haurylau.voice, which no longer exists: <list>.
Run the setup action to repair <it|them>.
```

where `<n>` is the number found, `<list>` is those keys joined by `, ` with ` and `
before the last, and the three alternatives are chosen by `<n> == 1`. The two
forms in full, so the test asserts a string rather than a shape:

- three found —
  `3 keys still name haurylau.voice, which no longer exists: ctrl+g, prefix+i and ctrl+shift+g. Run the setup action to repair them.`
- one found —
  `1 key still names haurylau.voice, which no longer exists: prefix+i. Run the setup action to repair it.`

The keys named are the ones found, never this plugin's three: a person who has
bound one action and not the others is told about the one that is broken.

It names them because a notice that says only that something is wrong leaves the
person exactly where they were. It is raised through `toast`
(`src/daemon.rs:691-699`), which is the existing path and which writes a journal
line when herdr refuses the call. When `[ui] toasts` is off, `toast` returns
without raising and without recording, so the journal line for this notice —
`rename: <n> key(s) still name(s) haurylau.voice: <list>`, in the same number as
the body — is written by the caller before the toast in every case, which is the rule the file already states:
"`[ui] toasts` decides whether the person is interrupted. It never decides whether
a failure is recorded" (`src/daemon.rs:688-690`).

When the herdr configuration cannot be located, does not exist, or does not parse,
the list is empty and nothing is raised. A file the daemon cannot read is not
evidence that a key is dead, and `setup` is the place that reports a configuration
it cannot parse — it has a terminal and this has none.

The notice is raised on a thread of its own. `toast` runs
`herdr notification show` through `Command::output()`, which has no timeout, and
this is the only place a toast would be raised before the daemon is serving: a
herdr slow to answer — it is starting this process as it starts itself — would
otherwise hold the listener bound and accepting nothing, which reads as a hang
rather than as a late notice. Nothing waits for that thread, and it is spawned
only when there is something to say: if the daemon stops before the thread
finishes, the toast is lost and the journal line, written first inside
`announce_rename`, is not.

Once per daemon start, and nowhere near a take: reading a configuration file on
the path that runs while somebody is speaking is what `start` already avoids by
reading configuration once (`src/daemon.rs:1133-1137`).

### What it does not reach

A person who has not relinked still has the old plugin, the old daemon and three
keys that work. The notice is for the state after the relink, which is the state
the issue describes. Nothing here reaches somebody who relinks and never restarts
herdr; for them the old daemon is still listening and the old keys still work,
because herdr still has the old plugin registered until it restarts.

## 5b. Saying only what was actually done

### Context

`decide` finds a superseded block by parsing the file with `inspect`, and
`rewrite_commands` edits one spelling of the value: `command`, `=`, blank space,
`"<legacy id>.<action>"`.

### Problem

TOML has other spellings of the same value — a literal string in single quotes, a
multi-line string — and `inspect` accepts all of them. A block written that way is
found by the detector, offered for rewrite, left alone by the rewrite, and would
be reported as repaired. The person is told their key works while the file still
names an id that does not exist, and the notice at every daemon start has nothing
to explain it. That is this repository's own silent-failure rule inverted: a
failure that announces success.

### Decision

The text that is about to be written is read back through `superseded_keys`
before it is written. Every key still named there is reported as left as it was,
with the line to change by hand, and the run exits non-zero — a key that is still
dead is a failure whatever else landed. A key that is no longer named is reported
as rewritten.

`rewrite_commands` also tracks multi-line strings and reads nothing inside one as
structure, so a `[[keys.command]]` line inside somebody's prose opens no block and
a `command =` line inside it is not a binding.

The scanner agrees with `toml` on documents `toml` accepts, which is what it was
checked against and also the whole of what it has to do: a file that does not
parse never reaches the rewrite. `run` reads the file with `inspect` first and
stops with "cannot read … as TOML, so nothing was changed" — and a configuration
half-edited, which is the common way to have one that does not parse, is a
configuration herdr is already ignoring.

### Why

The check costs one pass over the text the code already holds, and it makes the
report a statement about the file rather than about the intention. The
alternative — teaching the line edit every TOML spelling — is a parser, and the
one thing worse than not editing a line is editing the wrong one.

## 6. What does not change

- **The tab-label sweep.** It cuts from the microphone marker and lists tabs
  without naming a plugin (`src/indicator.rs:15`, `:73-82`, `:152-158`), so a
  daemon under the new id strips a decoration left under the old one with no
  change at all. AC-10 is verified, not implemented.
- **The sidebar token.** It carries `--source PLUGIN_ID` already
  (`src/indicator.rs:128-140`); the constant's new value is the whole change.
  A token written under the old source needs no sweep: it has a time to live of
  three renewals and lapses within 1.8 seconds once nothing renews it.
- **`tasks/*` and `docs/evidence.md`.** They record what was true when they were
  written.

## 7. The mechanical half

`PLUGIN_ID`'s new value reaches the manifest, the four installation checks, the
two documents that print a command, the two module comments, the client's timeout
message and sixteen test assertions. The assertions that are about the id itself
take the constant where the surrounding test allows it; those that pin an
argument list herdr receives keep a literal, because a test that builds its
expectation from the same constant the code uses asserts nothing.

`scripts/check_manifest.py` is unaffected: it checks that every command the
manifest names is one the binary accepts, and no command name changes.

## 8. Tests

| what | where | AC |
|---|---|---|
| the manifest's id and `PLUGIN_ID` agree | exists already, `src/setup.rs:1683` | AC-1 |
| the rendered snippet names `herdr-voice.*` and nothing else | `src/setup.rs` | AC-3 |
| the token argument list carries `herdr-voice` after `--source` | `src/indicator.rs` | AC-4 |
| a block naming the legacy command is reported as superseded, not as a stranger's key, naming its key and the file | `src/setup.rs` | AC-6 |
| `legacy_sibling` never answers with the directory it was given, including under herdr's override | `src/transport.rs` | AC-7 |
| a directory whose last component is not `PLUGIN_ID` has no legacy sibling, so nothing is reported | `src/transport.rs` | AC-7 |
| `y` rewrites the three blocks; the keys are unchanged, every other byte is unchanged, comments above and inside blocks survive | `src/setup.rs`, on a commented fixture | AC-6 |
| anything but `y` leaves the file untouched | `src/setup.rs` | AC-6 |
| a comment that mentions the old command is not rewritten | `src/setup.rs` | AC-6 |
| the report names the old configuration file and the directory now read | `src/setup.rs` | AC-7 |
| the report names a live old daemon and how to end it | `src/setup.rs`, against a socket the test listens on | AC-7 |
| the report says nothing about a daemon when nothing answers | `src/setup.rs` | AC-7 |
| three legacy blocks give exactly the plural body above, counted and listed in `BINDINGS` order | `src/daemon.rs` | AC-11 |
| one legacy block gives exactly the singular body above, naming only the key found | `src/daemon.rs` | AC-11 |
| a configuration carrying none, and one that cannot be located, read or parsed, raise nothing | `src/daemon.rs` | AC-11 |
| the journal line is written whether the toast is raised, refused or switched off | `src/daemon.rs` | AC-11 |
| a notice herdr refuses is recorded and does not stop the daemon | `src/daemon.rs` | AC-11 |

AC-2, AC-10 and the second half of AC-9 are verified by running, in S5, and
recorded in `docs/evidence.md` with the platform.

## 9. Noticed, and not done here

- **The state directory, and the models in it, are orphaned.** 1.5 GB on the
  machine this runs on. The issue does not list it among what breaks and
  `model --choose` fetches a model again; `AC_73.md` leaves it out deliberately.
- **The stale registry entry.** After a relink under the new id,
  `<config>/herdr/plugins.json` still holds the old entry; `herdr plugin list`
  shows one entry per checkout, so it is invisible until the new entry is
  unlinked. `herdr plugin unlink haurylau.voice` removes it. Not reported by
  `setup`: the AC does not ask for it, and it is the one leftover that costs
  nothing while it sits there.
- **The version.** A rename is a breaking change for anyone who installed
  `v0.1.0-beta.1`, and the manifest's version is what `scripts/install.sh` asks
  the release for. Whether this run bumps it is not in the issue.
