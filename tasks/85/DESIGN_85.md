# DESIGN_85

All changes are in `src/doctor.rs`. Nothing in the daemon, `toast`, the manifest
or herdr's configuration changes.

## Shape

One new finding, `notifications`, placed after `daemon` in `run()`. It is built
by a pure function so every case is a unit test without a file, a herdr or an
environment:

```rust
fn notifications_finding(
    plugin_toasts: bool,                          // [ui] toasts in the plugin's own configuration
    location: &Location,                          // where herdr's configuration is expected
    file: Result<String, ReadFailure>,            // what reading it gave
) -> Finding
```

- `Location` is either `Path(PathBuf)` or `Unknown`. `run()` fills it from
  `herdr_config_path_from(explicit, xdg, home, appdata, windows)`, a pure function
  over the five values. On every platform `HERDR_CONFIG_PATH` wins. On Windows the
  next is `%APPDATA%\herdr\config.toml`; elsewhere `XDG_CONFIG_HOME/herdr/config.toml`,
  then `~/.config/herdr/config.toml` (the order `src/setup.rs:66` states). It
  returns `Unknown` when none of the needed values is set.
- `ReadFailure` is `Absent` (file not found) or `Other(String)` (the operating
  system's message).
- The TOML is parsed with the `toml` crate already in use. The value is read from
  `ui.toast.delivery`; anything present but not a string, or a string outside
  `off | herdr | terminal | system`, is an unknown value and is quoted back.

`State` gains `Warning`, whose word is `warning`. `exit_code` is unchanged:
only `Missing` returns 1. `render`'s name column widens from 8 to 13 for every
line so `notifications` fits and the columns stay aligned.

## The finding, by case

Checked in this order; the first that applies wins.

| Case | State | Detail (one line) |
|---|---|---|
| `plugin_toasts` is false | `unused` | `[ui] toasts = false in the plugin's configuration: the plugin raises no toasts, so herdr's delivery setting does not matter` |
| location unknown | `missing` | `cannot tell where herdr's configuration is: none of HERDR_CONFIG_PATH, XDG_CONFIG_HOME, HOME (APPDATA on Windows) is set, so it is not known where notifications go; set HERDR_CONFIG_PATH to the file; <tail>` |
| other read failure | `missing` | `cannot read <path> (<reason>), so it is not known where notifications go; once it can be read: <tail>` |
| file does not parse | `missing` | `<path> is not valid TOML (<reason>), so it is not known where notifications go; once it parses: <tail>` |
| `delivery = "herdr"` | `ok` | `set to "herdr" in <path>: herdr draws the toast itself` |
| `terminal` or `system` | `warning` | `set to "<v>" in <path>: herdr hands the message on and cannot tell whether it appeared, so this plugin's failure messages may not reach you; for `"herdr"` set [ui.toast] delivery = "herdr" there and run `herdr server reload-config`; the plugin log also holds them` |
| `off` | `missing` | `set to "off" in <path>: this plugin's failure messages will not appear; set [ui.toast] delivery = "herdr" there and run `herdr server reload-config`; the plugin log also holds them` |
| key absent, or file `Absent` | `missing` | `no [ui.toast] delivery in <path> (herdr's default is "off"): ...` then the `off` sentence |
| unknown value | `missing` | `set to <value quoted> in <path>, which herdr does not list (off, herdr, terminal, system): ...` then the `off` sentence |

`<tail>` is one shared string, used by every `missing` and `warning` line:
`[ui.toast] delivery = "herdr" and `herdr server reload-config` make herdr show
the plugin's messages itself; they are also written to the plugin log, read with
`herdr plugin log list --plugin herdr-voice``. In the `off`, absent-key and
unknown-value rows it follows the sentence written there; for `terminal` and
`system` the row's own sentence already ends with it. Every line that is not
`ok` and not `unused` therefore contains the key, the value, the reload command
and the log command, which is what requirement 5 and AC-1 ask for.

The wording says "set to", never "is in effect": `doctor` reads the file, and a
server that has not reloaded can differ. The `unused` line carries neither the
setting nor the reload command, so it does not press a change on someone who
turned toasts off.

`"the plugin log also holds them"` is followed by the command that reads it,
`herdr plugin log list --plugin herdr-voice`, in the same line.

## Decisions

**Read the file, not ask herdr.**
- Context: `doctor` must say where notifications go.
- Problem: herdr 0.9.1 has no command or API method reporting the setting, and
  the reply of `notification show` is the same for all four values once a client
  is attached (`AC_85.md`, as-is 1 and 2).
- Decision: read `[ui.toast] delivery` from herdr's configuration file.
- Why: it is the only source that exists.

**`terminal` and `system` are a warning, `off` and unknown are `missing`.**
- Context: the line has to tell "will not arrive" from "cannot be confirmed".
- Problem: a single `ok` or `missing` hides that difference, and the exit code
  should not fail a machine on which messages may well arrive.
- Decision: a new `warning` state that leaves the exit code alone; `missing` for
  what is known not to arrive or not known at all.
- Why: it matches the issue's "a delivery of `off`, or one that cannot be
  confirmed, is named there" without failing `doctor` for a terminal that does
  surface them.

**An absent key is `off`.**
- Context: most machines never write `[ui.toast]`.
- Problem: treating absence as `ok` would repeat the silent failure.
- Decision: absent key and absent file both report `off`, naming herdr's default.
- Why: measured — both answer `disabled` with no client attached (`AC_85.md`,
  as-is 3).

**Windows resolves its own path.**
- Context: the issue is filed for all platforms, and the machine on which it was
  seen is a Windows one (`AC_90.md`).
- Problem: `src/setup.rs::config_path` looks at `HOME` and `~/.config`, which is
  not where herdr's documentation puts the file on Windows.
- Decision: `doctor` uses its own resolver with `%APPDATA%\herdr\config.toml` on
  Windows; `src/setup.rs` is left as it is.
- Why: changing `setup` would change another command's behaviour. The Windows
  path is taken from herdr's documentation and is not verified on Windows here;
  `docs/evidence.md` says so.

## Not in this change

- `config_path` in `src/setup.rs` on Windows (noticed above, not touched).
- Any check of the running server, and any message at daemon start.
