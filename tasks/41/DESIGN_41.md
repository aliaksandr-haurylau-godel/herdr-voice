# DESIGN_41 — `setup` prints the keybinding snippet and offers to append it

## What is added

One new module, `src/setup.rs`, holding the snippet, the reader of the user's
herdr configuration and the writer. One new `[[panes]]` entry in
`herdr-plugin.toml`, id `setup`. `src/main.rs` routes `Command::Setup` into the
module instead of the not-implemented arm, and `setup` joins the `IMPLEMENTED`
list and the usage text. `README.md` is corrected where it describes this action.

Nothing else moves. The daemon is not involved: `setup` neither records nor
delivers anything, so it never opens the socket.

## 1. One subcommand, two roles, chosen by the process itself

**Context.** A person reaches `setup` through herdr, as
`herdr plugin action invoke haurylau.voice.setup` or a key bound to that action.
The process herdr starts for an action has no terminal: its output is captured
and appears only in `herdr plugin log list`. The process herdr starts for a
declared pane does have one — all three standard descriptors of a pane process
are the same terminal device.

**Problem.** The offer needs a terminal to be seen and answered in, and the entry
point does not have one.

**Decision.** `herdr-voice setup` looks at its own standard input. When it is a
terminal, it does the work: prints the snippet, asks the question, writes. When it
is not, it runs `herdr plugin pane open --plugin haurylau.voice --entrypoint setup`
and exits. The manifest's new pane runs the same `herdr-voice setup`, which then
finds a terminal and takes the first branch.

**Why.** One subcommand and one code path, selected by a fact about the process
rather than by a flag. No new user-visible name is introduced, and
`herdr-voice setup` typed into an ordinary shell does the work directly instead of
bouncing through herdr.

## 2. A pane that will not open says so where a person is looking

**Context.** Opening can be refused. herdr allows one popup at a time and answers
`{"code":"ui_busy","message":"a popup pane is already open"}` when another is up;
it also refuses when there is no active pane. The caller of a global action sees
none of this.

**Problem.** A keypress that produces nothing on screen is the failure this
project weighs the same as a wrong transcript.

**Decision.** The branch that could not open a pane reports through
`herdr notification show` — the same command `src/delivery.rs` already sends, run
through this module's own seam, which section 12 defines — and writes the same
sentence to standard error, where the plugin log keeps it. The sentence names what to do: close the
popup that is open, or run `herdr-voice setup` in a terminal.

**Why.** The toast is the only surface that reaches somebody who just pressed a
key; the plugin log is the only one that survives for whoever reads afterwards.

## 3. The snippet is one table, and a test binds it to the manifest

**Context.** The snippet is three `[[keys.command]]` blocks: `ptt` on `ctrl+g`,
`dictate` on `prefix+i`, `cancel` on `ctrl+shift+g`. Each block carries four
fields — `key`, `type = "plugin_action"`, a `command` of
`haurylau.voice.<action id>`, and a `description`, which is what herdr shows for
the binding and what AC-1 requires.

**Problem.** The action ids are written twice — once in the manifest, once in the
snippet — and two copies of one name drift.

**Decision.** A constant array of three entries in `src/setup.rs`, rendered into
the blocks. A unit test parses `herdr-plugin.toml` and fails when the plugin id or
any of the three action ids is not declared there.

**Why.** The manifest is the only contract with herdr; `scripts/check_manifest.py`
exists for exactly this class of drift. A snippet naming an action the manifest
does not declare installs a key that does nothing when pressed.

## 4. Parse to decide, append text to write

**Context.** The file being changed is hand-written and commented: the bindings in
it carry explanations of why each key was chosen.

**Problem.** Deciding what is already present needs structure. Writing a parsed
document back destroys comments, ordering and formatting.

**Decision.** Read the file and parse it with `toml`, already an allowed
dependency, to answer two questions — which of the three actions is already bound,
and which of the three keys is already taken. Then write the original bytes
unchanged, followed by a blank line, a comment naming this plugin, and the blocks
that are missing.

**A link is followed, not replaced.** When the configuration file is a symbolic
link — a common arrangement, with the real file kept in a dotfiles repository —
the path is resolved to its target first, and the candidate is written beside the
target and renamed over it. Renaming over the link itself would replace the link
with a regular file and leave the real file without the bindings, while the run
reported success.

**Why.** `[[keys.command]]` is a fully qualified table header, so appending at the
end of the file is valid whatever section the file ends in. That was checked
rather than assumed: appended after the last section of a real configuration,
`herdr config check` answered `config: ok`. And nothing anybody wrote is lost.

## 5. herdr validates the candidate before it becomes the configuration

**Context.** herdr keeps the previous configuration when a new one is invalid, so
a bad write is survivable but silent. herdr does detect one class this action can
cause — two `[[keys.command]]` on one key give
`prefix+i: kept keys.command[9].key, disabled keys.command[10].key`.

**Problem.** Reimplementing herdr's acceptance rules inside the plugin means
carrying a copy of them that drifts with every herdr release.

**Decision.** The verdict is the exit status of `herdr config check`, run with
`HERDR_CONFIG_PATH` pointed at the file being judged: `0` is
`config: ok` and `1` is `config: issues found`, with the reasons on standard
output. Both were measured. The run reads that status and shows that output
verbatim; it parses neither.

The original file is checked first, before anything is written. When herdr
already reports issues with it, `setup` names them and writes nothing, because a
configuration herdr is unhappy with is one it is either ignoring outright — the
parse-error diagnostic ends `; using defaults` — or ignoring in part, and a
binding appended to it would appear to do nothing when pressed. That failure would
otherwise be blamed on this action.

When the original is clean, the new content is written to a temporary file in the
same directory, the check is run against that, and only a clean result renames it
over the original. A reported issue leaves the original untouched, is shown
verbatim, and the temporary file is removed. The temporary file is given the
original's permissions before the rename, so replacing a file does not quietly
change its mode.

**Why.** The authority on what herdr accepts is herdr. Checking the original as
well is what keeps somebody else's broken configuration from being reported as
this action's failure. The same mechanism makes the write atomic: the rename is
the only moment at which the real file changes, so an interrupted run cannot
leave a half-written configuration.

**What "cannot be written" means here, and how it is detected.** It is not
detected by asking in advance — a permission answered before the write is a
different question from the write, and it would be answered on a file the run has
not yet touched. The failure is whatever actually fails: creating the temporary
file in the configuration's directory, writing it, or renaming it into place.
Each is reported with the path it happened on and the reason the operating system
gave.

This also fixes the fixture the tests use. A configuration file with its own
write permission removed is not the case: renaming over it succeeds while its
parent directory is writable, and the run correctly reports success. The case that
must fail is a parent directory that cannot be written, and that is what the test
sets up.

## 6. What counts as already there, and what counts as a taken key

**Context.** Two different situations, and the issue asks for different answers.

**Decision.** A binding is *already there* when some `[[keys.command]]` has
`command` equal to `haurylau.voice.<action>`, on whatever key: it is not added a
second time, and the key it sits on is named in the report. A key is *taken* when
some `[[keys.command]]` carries that key with a different command, or an explicit
assignment under `[keys]` has that value: that one block is not added, and what
holds the key is named. The other blocks are still added — one collision does not
abandon the rest.

*Named* means named. A key held by a herdr action is reported with that action's
name — `goto`, `new_tab` — not with the category it belongs to, because the whole
value of the message is that the person can go and look at the thing that is in
the way. A `[[keys.command]]` block that carries a key but is missing its command,
or carries values of the wrong type, still reserves its key: the report says the
key is taken by a block it could not identify and names the file to look in. A
block that cannot be identified is still a block herdr will honour, and offering a
second binding on that key would produce an opaque refusal from
`herdr config check` instead of the diagnosis this action exists to give.

**Why.** Adding a duplicate is what the issue forbids. Shadowing somebody's
existing key is the silent failure the action exists to prevent: on the
development machine `prefix+g` was already taken twice over, and nothing would
have said so.

**A consequence, recorded rather than smoothed over.** A collision with a herdr
built-in default that the user has not written into their file is invisible to
this check, and `herdr config check` does not report it either — a
`[[keys.command]]` on `prefix+c`, which is herdr's own default for `new_tab`,
produced no diagnostic at all. The three keys were checked by hand against the
defaults of herdr 0.9.0 and are free there. Users on another herdr, or with
their own `[keys]` block, are covered only for what their file states.

## 7. Which file is written

**Decision.** `HERDR_CONFIG_PATH` when it is set, otherwise `herdr/config.toml`
under `XDG_CONFIG_HOME`, otherwise the same path under `~/.config`. Every message
names the path that was resolved. A file that does not exist is created,
containing the three blocks, and the run says it created it; a missing parent
directory is created with it.

**Why.** That is herdr's own order — its binary states
`Env: HERDR_CONFIG_PATH overrides config file path`. The override is also what
makes the tests the issue asks for possible: they point the whole resolution at a
temporary file.

## 8. What it says when it has finished

**Decision.** After a write it names the file, lists the bindings it added and the
ones that were already present, and says that the running herdr does not see the
change until `herdr server reload-config` is run, or the `reload_config` key,
`prefix+shift+r` by default, is pressed.

**Why.** A binding written into a file the running server has not reread does
nothing when pressed, and nothing on the screen would say why.

## 9. Exit codes

`0` when the run did what was asked — including a run where every binding was
already present, and a run where the offer was declined. `1` when it named a
failure: the file could not be read, could not be written, herdr already reports
issues with it, herdr rejected the candidate, or — in the branch of section 2 that
has no terminal — the pane could not be opened. The code `69`, `not implemented yet`, disappears for this subcommand.

## 10. Testing

Covered by unit tests, against a temporary configuration file reached through
`HERDR_CONFIG_PATH`: a clean append; a second run over the result of the first; a
configuration in a directory that cannot be written; a file that does not exist;
one of the three actions already bound on a different key; one of the three keys
held by a different command; and an original that herdr already reports issues
with, which must leave the file alone. The snippet-against-manifest test stands
beside them. The calls to herdr go through this module's own seam, section 12, so
the argument lists and the two exit statuses are asserted without a live herdr.

Two claims are about the binary as a process rather than about a function, and
no unit test can reach them: that standard input being a terminal is what chooses
the interactive branch, and that `setup`'s exit code arrives at whoever invoked
it. They are covered by one integration test, `tests/setup_process.rs`, which runs
the built binary with its standard input a pipe and `HERDR_BIN_PATH` pointing at a
recorder script. `CLAUDE.md` says unit tests live next to the code; this is a
departure from that, stated here and in a comment at the top of the file, because
the alternative is leaving the two decisions that route the whole feature
unproven.

Three things cannot be covered that way and are verified by hand, then written
into `docs/evidence.md` with the platform: that the pane actually opens and takes
an answer, that `herdr config check` reports no issue on the result, and that the
three keys do what they claim after a reload. The criterion that herdr accepts the
snippet can only be met against a real herdr.

## 11. What else this makes wrong

`README.md:44-45` says "two lines go into your herdr configuration". It is three
blocks, and the action is reached through a pane. That passage is rewritten in the
same change.

## 12. The two new herdr calls, and what tests them

**Context.** This action runs herdr three times: to open its own pane, to check a
configuration file, and to raise a toast when the pane could not be opened. The
seam in `src/delivery.rs` carries none of them. `trait Deliverer` has exactly
`insert`, `submit` and `notify`; its runner `HerdrDeliverer::run` is private; and
the recorder that captures argument lists lives inside that file's private test
module, where another module's tests cannot reach it.

**Problem.** Either `Deliverer` grows methods that have nothing to do with
delivering text — which changes the trait, its public fake, and every
`Box<dyn Deliverer>` built in `src/daemon.rs` — or `setup` brings its own.

**Decision.** `src/setup.rs` declares its own trait with three methods:
`open_pane`, `check_config` taking the path to judge, and `notify`. The real
implementation runs whatever `delivery::herdr_binary()` names, which is already
public and is how every outward call to herdr in this codebase resolves the
program. Its test double records the argument lists and returns a chosen exit
status, and lives in `src/setup.rs`'s own test module. `trait Deliverer` is not
touched and no construction of it changes.

One thing is shared rather than copied: `delivery::notify_args`, the builder for
`herdr notification show <title> --body <body>`, becomes `pub(crate)` so that
command has one definition in the codebase instead of two.

**Why.** A seam stops meaning anything once it grows a method because an unrelated
caller needed somewhere to sit; `Deliverer` is about putting text into a pane, and
neither opening a pane nor validating a file is that. The argument builder is
shared because it really is the same command being sent.

---

## Appendix — where the facts above came from

Run on macOS against herdr 0.9.0 on 2026-09-16 and 2026-09-17.

| Claim | How it was established |
|---|---|
| An action's process has no terminal and its output reaches only the plugin log | `herdr plugin action invoke haurylau.voice.setup` printed nothing of the plugin's own output; `herdr plugin log list` then held `"stderr":"setup: not implemented yet\n"`, `"exit_code":69` |
| A pane's process has a terminal on all three descriptors | A throwaway linked plugin declaring one popup pane; `lsof` on the running pane process reported `0u CHR /dev/ttys060`, `1u CHR /dev/ttys060`, `2u CHR /dev/ttys060`. An earlier `[ -t 1 ]` inside a `$( )` substitution answered "no" and was wrong — the substitution replaces descriptor 1 with a pipe |
| A declared pane is opened on request | `herdr plugin pane open --plugin <id> --entrypoint <id>` answered `{"id":"cli:plugin","result":{"type":"ok"}}` |
| Only one popup at a time | A second open, with one already up, answered `{"code":"ui_busy","message":"a popup pane is already open"}` |
| A popup pane is neither listed nor logged | It did not appear in `herdr pane list`, and `herdr plugin log list` for that plugin stayed empty |
| herdr has no menu of plugin actions | `herdr plugin action list` and `herdr plugin action invoke` are command-line commands; nothing in the binary offers the actions in its own interface |
| Appending after the file's last section is accepted | A `[[keys.command]]` block appended after a configuration's final `[theme]` section: `herdr config check` answered `config: ok` |
| `herdr config check` reports its verdict as an exit status | `0` with `config: ok` on a valid configuration; `1` with `config: issues found` on a duplicate key, on an unknown section and on a parse error |
| A configuration file that does not exist is reported as `config: ok`, exit `0` | `HERDR_CONFIG_PATH` pointed at a path with no file there |
| An unrelated defect in the file is also exit `1` | An added `[nonsense]` section gave `unknown config section [nonsense]; ignoring section`; a malformed file gave a parse error ending `; using defaults` |
| herdr detects two commands on one key | Two `[[keys.command]]` on `prefix+i`: `config: issues found`, `prefix+i: kept keys.command[9].key, disabled keys.command[10].key` |
| herdr does not detect shadowing of its own default | A `[[keys.command]]` on `prefix+c`, herdr's default for `new_tab`, added no diagnostic |
| The configuration path and its override | The herdr binary states `Env: HERDR_CONFIG_PATH overrides config file path` |
| `type` accepts `plugin_action` | The binding type enum in the herdr binary reads `shellpanepopupplugin_action`, with fields `key command type description width height` |
