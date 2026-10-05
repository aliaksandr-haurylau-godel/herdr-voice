# PLAN_85

Everything below is in `src/doctor.rs` (code and its `#[cfg(test)] mod tests`,
which already exists at the bottom of the file) except task 7. Run
`cargo test doctor::` after each task; a task is done when its listed tests fail
before the code and pass after.

## Fixed strings

```rust
const TAIL: &str = "to have herdr show this plugin's messages itself, set \
    `[ui.toast] delivery = \"herdr\"` in herdr's configuration and run \
    `herdr server reload-config`; they are also written to the plugin log: \
    `herdr plugin log list --plugin herdr-voice`";
```

Every detail below is `format!` of the given text. `{path}` is
`path.display()`. A `; {TAIL}` ending means the string `"; "` then `TAIL`.

| Case | State | Detail |
|---|---|---|
| plugin toasts off | `NotUsed` | `[ui] toasts = false in the plugin's configuration: the plugin raises no toasts, so herdr's delivery setting does not matter` |
| location unknown | `Missing` | `cannot tell where herdr's configuration is: none of HERDR_CONFIG_PATH, XDG_CONFIG_HOME, HOME (APPDATA on Windows) is set, so it is not known where notifications go; set HERDR_CONFIG_PATH to the file; {TAIL}` |
| read failed | `Missing` | `cannot read {path} ({reason}), so it is not known where notifications go; once it can be read, {TAIL}` |
| not TOML | `Missing` | `{path} is not valid TOML ({reason}), so it is not known where notifications go; once it parses, {TAIL}` |
| `herdr` | `Ok` | `set to "herdr" in {path}: herdr draws the toast itself` |
| `terminal` / `system` | `Warning` | `set to "{v}" in {path}: herdr hands the message on and cannot tell whether it appeared, so this plugin's failure messages may not reach you; {TAIL}` |
| `off` | `Missing` | `set to "off" in {path}: this plugin's failure messages will not appear anywhere in herdr; {TAIL}` |
| key absent | `Missing` | `no [ui.toast] delivery in {path} (herdr's default is "off"): this plugin's failure messages will not appear anywhere in herdr; {TAIL}` |
| file absent | `Missing` | `no file at {path} (herdr's default delivery is "off"): this plugin's failure messages will not appear anywhere in herdr; {TAIL}` |
| other value | `Missing` | `set to {shown} in {path}, which herdr does not list (off, herdr, terminal, system): this plugin's failure messages may not appear; {TAIL}` |

`{reason}` is passed through `fn one_line(text: &str) -> String`, which returns
the first line of `text` with surrounding whitespace trimmed (empty text gives
an empty string): the `toml` crate's error text spans several lines and a
detail is one line.

`{v}` is the lowercase value. `{shown}` is `toml::Value`'s `Display` of the
value, so a string arrives quoted and a number or table arrives as TOML text.

## Tasks

**1. `State::Warning`, wider name column.** Input: `src/doctor.rs:23-42,79-90`.
Add variant `Warning` with word `"warning"`. Change `"{:<8} {:<8} {}\n"` to
`"{:<13} {:<8} {}\n"`. `exit_code` is not touched. Tests: `Warning` renders the
word `warning`; a finding named `notifications` renders as
`"notifications ok       x\n"` (name padded to 13, state to 8); an existing
test in the file still passes. `the_exit_code_follows_the_worst_line` gets a
case: a list of `Ok` and `Warning` returns 0. Depends on: nothing.

**2. Path resolver.** Add
```rust
#[derive(Debug, PartialEq, Eq)]
enum Location { Path(PathBuf), Unknown }
fn herdr_config_path_from(explicit: Option<String>, xdg: Option<String>,
    home: Option<String>, appdata: Option<String>, windows: bool) -> Location
```
An empty string counts as unset. `explicit` set: `Path(explicit)`. Else if
`windows`: `appdata` -> `Path(appdata/herdr/config.toml)`, otherwise `Unknown`.
Else `xdg` -> `Path(xdg/herdr/config.toml)`, else `home` ->
`Path(home/.config/herdr/config.toml)`, else `Unknown`. Tests (all with
`PathBuf::join` for expected values): explicit wins over all others on both
platforms; xdg beats home; home alone; nothing -> `Unknown`; empty strings ->
`Unknown`; windows uses appdata and ignores xdg and home; windows without
appdata -> `Unknown`. Depends on: nothing.

**3. Reader of the setting.** Add
```rust
#[derive(Debug, PartialEq, Eq)]
enum Configured { Value(Delivery), Absent, Other(String), Unparsable(String) }
enum Delivery { Off, Herdr, Terminal, System }   // derive Debug, PartialEq, Eq, Clone, Copy
fn read_delivery(text: &str) -> Configured
```
Parse with `toml::from_str::<toml::Value>`; `Err(e)` -> `Unparsable(e.to_string())`.
Look up `ui` -> `toast` -> `delivery` with `get`; any missing step or a non-table
step -> `Absent`. A string equal (exactly, case-sensitive) to `off`, `herdr`,
`terminal` or `system` -> `Value`. Any other value, string or not ->
`Other(value.to_string())`. Tests use a helper `fn under_toast(line: &str) -> String` returning
`format!("[ui.toast]\n{line}\n")`; every input below is passed through it unless
stated. Each of the four values (`delivery = "off"` and so on) -> the matching
`Value`; `""` (passed as is) -> `Absent`; `"[ui]\ntoasts = false\n"` (as is) ->
`Absent`; `"[ui]\ntoast = \"x\"\n"` (as is; `toast` is not a table) -> `Absent`;
`[ui.toast]\nother = 1\n` -> `Absent`; `delivery = "Herdr"` -> `Other("\"Herdr\"")`;
`delivery = 3` -> `Other("3")`; the literal text `"[ui.toast]\ndelivery = \n"`
(as is; a value is missing, so it is invalid TOML) -> `Unparsable`. Depends on:
nothing.

**4. `notifications_finding`.** Add
```rust
enum ReadFailure { Absent, Other(String) }
fn notifications_finding(plugin_toasts: bool, location: &Location,
    file: Result<String, ReadFailure>) -> Finding
```
with `name: "notifications"`, cases checked in the table's order, strings exactly
as in the table. `Location::Unknown` ignores `file`. Tests, one per table row,
each asserting state and that the detail contains: for every row except the first
and the `Ok` row, all of `[ui.toast]`, `delivery = "herdr"`,
`herdr server reload-config` and `herdr plugin log list --plugin herdr-voice`;
for every row that has a path, the path text; for `Other`/`Unparsable`/read-failure
the first line of the reason, produced by `one_line` (a test feeds
the `Unparsable` text of `read_delivery("[ui.toast]\ndelivery = \n")`, which spans several lines);
for the value rows the value. `one_line` has its own tests: multi-line input
gives the first line; leading spaces are trimmed; empty gives empty. Also: the `NotUsed` row contains none of
`reload-config`, `delivery = "herdr"`; `NotUsed` wins over `Unknown` location and
over a read failure and over `terminal`; the `Ok` row does not contain `TAIL`'s
`reload-config`; no detail contains a newline; no detail contains the phrase "in effect".
Depends on: 1, 2, 3.

**5. Wire it into `run()`.** Add `fn herdr_config_location() -> Location` that
calls `herdr_config_path_from` with `HERDR_CONFIG_PATH`, `XDG_CONFIG_HOME`,
`HOME`, `APPDATA` from `std::env::var(..).ok()` and `cfg!(windows)`. Add
`fn read_herdr_config(location: &Location) -> Result<String, ReadFailure>`, which
returns `Ok(text)` from `std::fs::read_to_string`, maps `ErrorKind::NotFound` to
`Err(ReadFailure::Absent)`, any other error to `Err(ReadFailure::Other(e.to_string()))`,
and `Err(ReadFailure::Absent)` for `Location::Unknown` (never used, because
`Unknown` ignores the file). In `run()`, on the line before
`let mut findings = vec![...]` bind
`let location = herdr_config_location();`, and change the literal to
```rust
let mut findings = vec![
    herdr_finding(),
    daemon_finding(),
    notifications_finding(loaded.config.ui.toasts, &location, read_herdr_config(&location)),
    config_finding(&loaded),
];
```
so the finding is third, between `daemon` and `config`. Test: `read_herdr_config` on a path in a
`tempfile`-style temp directory the test creates (use `std::env::temp_dir()` plus
a unique name, as `src/setup.rs` tests do) returns the text; on a missing path
returns `Absent`; on a directory returns `Other`. Depends on: 2, 4.

**6. The four gates.** `cargo test`, `cargo clippy --all-targets -- -D warnings`,
`cargo fmt --check`, `python3 scripts/check_manifest.py`, then the Windows
dead-code check, run from the repository root:
```sh
find src -name '*.rs' -exec sed -i '' \
  -e 's/#\[cfg(all(test, unix))\]/#[cfg(any())]/g' \
  -e 's/#\[cfg(unix)\]/#[cfg(any())]/g' \
  -e 's/#\[cfg(not(unix))\]/#[cfg(not(any()))]/g' \
  -e 's/#\[cfg(windows)\]/#[cfg(not(any()))]/g' {} +
cargo clippy --all-targets -- -D warnings
git checkout -- src
```
It edits `src/` in place, so `git checkout -- src` on the last line is
mandatory, and it discards every uncommitted change under `src/`: commit the
work of tasks 1 to 5 first. The clippy result is the outcome of the check; any
warning is a defect in the diff. Depends on: 5.

**7. S5 (after S4's reviews).** Two parts, both written into `docs/evidence.md`
with herdr 0.9.1 and macOS, commands and complete output beside each result.

*Part A: `doctor` on this machine.* Run `cargo run -- doctor` (a) with the
owner's configuration, read only, and no `HERDR_CONFIG_PATH`; (b) with
`HERDR_CONFIG_PATH` naming a scratch file that holds, one run each,
`[ui.toast]` `delivery = "terminal"`, `"off"`, `"system"`, `"herdr"`, an empty
file, and a path that does not exist. Each output must show the `notifications`
line after `daemon`.

*Part B: the probe results behind `AC_85.md` "as-is" 2 and 3.* These are five
results: (1) with no client attached, `delivery` set to `off`; (2) with no
client, `terminal`, `herdr`, `system`; (3) with a focused client, each of the
four values; (4) an empty configuration file with no client; (5) an absent
configuration file with no client. They need an isolated server that never
touches the owner's: a short scratch directory (a Unix socket path is limited in
length; use `/tmp/h85`), its own `HERDR_CONFIG_PATH` and `HERDR_SOCKET_PATH`, and
`HERDR_ENV`, `HERDR_PANE_ID`, `HERDR_TAB_ID`, `HERDR_WORKSPACE_ID` removed from the
environment of every command. Procedure:

```sh
mkdir -p /tmp/h85 && cd /tmp/h85
cat > h.sh <<'EOS'
#!/bin/sh
exec env -u HERDR_ENV -u HERDR_PANE_ID -u HERDR_TAB_ID -u HERDR_WORKSPACE_ID \
  HERDR_CONFIG_PATH=/tmp/h85/config.toml HERDR_SOCKET_PATH=/tmp/h85/h.sock herdr "$@"
EOS
chmod +x h.sh
: > config.toml
(env -u HERDR_ENV -u HERDR_PANE_ID -u HERDR_TAB_ID -u HERDR_WORKSPACE_ID \
  HERDR_CONFIG_PATH=/tmp/h85/config.toml HERDR_SOCKET_PATH=/tmp/h85/h.sock \
  nohup herdr server > server.out 2>&1 &)
sleep 3
# results 4 and 5, no client
./h.sh notification show probe --body empty
rm config.toml; ./h.sh server reload-config
./h.sh notification show probe --body absent
# results 1 and 2, no client
for v in off terminal herdr system; do
  printf '[ui.toast]\ndelivery = "%s"\n' $v > config.toml
  ./h.sh server reload-config; ./h.sh notification show probe --body $v
done
```

For result 3 a client must be attached: a small Python program forks a
pseudo-terminal running `herdr` with the same environment, sets its window size
(`TIOCSWINSZ`, 40 rows by 120 columns), writes the focus-in report `ESC [ I`,
waits two seconds, and for each value rewrites `config.toml`, runs
`./h.sh server reload-config`, waits one second, runs
`./h.sh notification show probe --body <value>`, and finally kills the child.
The program's text goes into the evidence entry. Stop the isolated server with
`kill` on the pid of the process that holds `/tmp/h85/h.sock` (find it with
`lsof -U | grep /tmp/h85`), and check with `ps` that the owner's server, which
was started earlier and holds a different socket, is still running. State that
the Windows path in `herdr_config_path_from` is from herdr's documentation and is
not verified on Windows. Depends on: 6.
