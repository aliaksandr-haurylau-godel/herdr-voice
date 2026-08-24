# AC_3 — Skeleton: manifest, daemon, socket client, doctor

Rescoped after `#5` landed on `main`. The first version of this document was
written against an empty repository and claimed work that is now merged; the
criteria below cover only what is left.

## as-is

The crate exists and builds. `Cargo.toml:1` declares `herdr-voice` 0.0.0 with an
empty `[dependencies]` table (`Cargo.toml:15`) — nothing has been chosen yet for
the socket, the configuration format or the command line.

`src/main.rs` is a command-line front with no behaviour behind it. `parse`
(`src/main.rs:68`) maps every subcommand the manifest names, and every one of them
falls into a single arm that prints `"<name>: not implemented yet"` and exits with
`NOT_IMPLEMENTED = 69` (`src/main.rs:87`, `src/main.rs:109`). Only `help` and
`--version` do real work. Four unit tests live beside the code
(`src/main.rs:129`), one of which asserts that the binary accepts every subcommand
the manifest calls.

`herdr-plugin.toml` is complete and accepted: `id = "haurylau.voice"`,
`min_herdr_version = "0.8.0"`, `platforms = ["macos", "linux", "windows"]`. Its
`[[startup]]` entry is `["target/release/herdr-voice", "daemon"]`
(`herdr-plugin.toml:20`), so herdr already tries to start a daemon and gets exit
69. Actions declared: `dictate`, `ptt`, `cancel`, `setup`. Panes declared:
`status`, `model`, `mic`. **The manifest has no `doctor` entry**, although the
binary accepts the subcommand (`src/main.rs:73`) and the usage text advertises it
(`src/main.rs:94`).

`scripts/check_manifest.py:47` reads the set of known subcommands out of
`src/main.rs` with a regular expression over `Some("…")`, so a subcommand is
callable from the manifest only once it appears in `parse`.

CI runs five required checks: `cargo fmt --check`, `cargo clippy --all-targets -D
warnings` and `cargo test --all` on `macos-latest`, `ubuntu-latest` and
`windows-latest` (`.github/workflows/check.yml:12`), the manifest check
(`.github/workflows/check.yml:33`), and the leak gate
(`.github/workflows/leak-gate.yml:10`). Tests therefore run on Windows on every
pull request, whatever is or is not verified on a real Windows machine.

Facts about the herdr side, read from the installed herdr 0.8.2 rather than
assumed:

- The invocation context arrives in the environment variable
  `HERDR_PLUGIN_CONTEXT_JSON`. Every field `docs/design.md` section 3 relies on is
  present in the 0.8.2 binary: `workspace_id`, `workspace_label`, `workspace_cwd`,
  `worktree`, `tab_id`, `tab_label`, `focused_pane_id`, `focused_pane_cwd`,
  `focused_pane_agent`, `focused_pane_status`, `selected_text`,
  `invocation_source`, `correlation_id`.
- Other variables herdr sets for plugin commands: `HERDR_BIN_PATH`,
  `HERDR_SOCKET_PATH`, `HERDR_PLUGIN_ID`, `HERDR_PLUGIN_ROOT`,
  `HERDR_PLUGIN_CONFIG_DIR`, `HERDR_PLUGIN_STATE_DIR`,
  `HERDR_PLUGIN_ENTRYPOINT_ID`.
- herdr runs plugin commands with a minimal `PATH`, and a pane command resolves
  against the pane's working directory rather than the plugin root — a relative
  path in a `[[panes]]` entry fails. The three panes in the manifest use relative
  paths today; none of them is implemented yet, so nothing is broken, but the
  daemon must not depend on the working directory it is started in either.

## to-be

herdr's startup entry produces a process that stays alive and listens. Every
other invocation of the binary is a short client run that writes a request to that
listener and exits, carrying the invocation context herdr gave it. `doctor` names
each precondition the plugin needs and, for every one that is missing, what to do
about it. The pipeline is still absent: the actions that need it keep reporting
that they are not implemented.

## Requirements

Asked for by the issue, and not yet done:

- R1 The daemon starts through the manifest's `startup` entry and stays alive.
- R2 A plugin action reaches the running daemon over the socket.
- R3 `doctor` reports what is missing.
- R4 The invocation context herdr passes is the source of the target pane —
  nothing is derived by guessing (`docs/design.md` section 3, which the issue
  names as the replacement for the prototype's target-pane guessing).

Already done by `#5`, and therefore not criteria here: the installable plugin, the
accepted manifest, the three declared platforms, the build entries, the release
workflow.

Implied, and in scope only as far as R1 to R4 need them:

- R5 A transport with one interface and two implementations: a Unix domain socket
  on macOS and Linux, a named pipe on Windows (`docs/design.md` section 2). The
  Windows implementation must compile and pass its tests, because CI runs the
  suite on `windows-latest`.
- R6 Configuration loading where every key has a default and an absent file is a
  valid state, because `doctor` has to report which of the two it saw.
- R7 No panic paths: a missing daemon, an unparsable context or a broken socket
  end in a message that names the next step and a non-zero exit.

## Chosen readings

1. **How `doctor` is reached.** The issue names `doctor`; `docs/design.md` section
   2 does not list it among the actions, and the manifest does not declare it.
   This issue requires it as a subcommand run from a terminal. Whether it also
   becomes a manifest action or pane is a design decision — but if design adds
   one, `scripts/check_manifest.py` must still pass.
2. **What `doctor` checks.** `docs/design.md` section 6 lists microphone
   permission, model, rewrite engine and herdr version. Microphone permission
   cannot be established without capture code, which the issue puts out of
   bounds, so it is not required here.
3. **How far Windows goes.** The issue says macOS first. Windows is therefore
   required to compile and to pass unit tests in CI, and is not required to be
   verified on a real machine — that is issue `#1`.
4. **Which actions gain behaviour.** Only `cancel`, because it is the one action
   `docs/design.md` section 2 names that can be answered without the pipeline.
   `dictate`, `ptt`, `setup` and the three panes keep reporting that they are not
   implemented.

## Acceptance criteria

- **AC-1** `herdr-voice daemon` does not exit on its own: it runs until it is
  stopped, and no longer returns 69.
- **AC-2** Running the `startup` command while a daemon is already alive leaves
  exactly one daemon process and exits 0. herdr may run it repeatedly.
- **AC-3** The daemon listens on a Unix domain socket whose path is derived from
  `HERDR_PLUGIN_STATE_DIR`, with a documented fallback when that variable is
  unset, and the path does not depend on the working directory the daemon was
  started in.
- **AC-4** A socket file left behind by a daemon that died does not prevent the
  next daemon from starting.
- **AC-5** `herdr plugin action invoke cancel` exits 0, and the daemon records one
  received request naming the entrypoint it came from. The client process does no
  work beyond writing the request, reading the reply and exiting.
- **AC-6** The request the client sends carries the invocation context verbatim —
  at least `focused_pane_id`, `focused_pane_cwd`, `focused_pane_agent`, `tab_id`
  and `tab_label` — and the binary contains no call to `herdr agent list` or any
  other means of deriving the target pane.
- **AC-7** With no daemon running, a client invocation exits non-zero within a
  bounded time, printing a message that names how to start the daemon. It neither
  panics nor hangs.
- **AC-8** With `HERDR_PLUGIN_CONTEXT_JSON` absent or unparsable, nothing panics.
  A command that needs a target pane exits non-zero naming what was missing;
  `cancel`, which needs none, still works.
- **AC-9** `doctor` prints one line per checked item: the herdr binary and its
  version against the manifest's `min_herdr_version`, whether the daemon is
  reachable and at which socket path, whether a configuration file was found or
  defaults were used, whether a speech model is present, and whether a rewrite
  engine is available. Every line that reports something missing names what to do
  next. `doctor` exits 0 when nothing is missing and non-zero otherwise.
- **AC-10** With no configuration file present, the daemon starts and `doctor`
  runs, both on defaults, and `doctor` states that defaults were used.
- **AC-11** `cargo test --all`, `cargo clippy --all-targets -- -D warnings`,
  `cargo fmt --check` and `python3 scripts/check_manifest.py` all pass. The socket
  round trip and the parsing of the invocation context are each covered by a test
  that needs neither herdr nor a microphone.
- **AC-12** The five required CI checks pass on the pull request, which includes
  the suite on `windows-latest`: the named-pipe transport compiles there and its
  tests pass.
- **AC-13** `dictate`, `ptt`, `setup`, `status`, `model` and `mic` still exit 69
  with `"<name>: not implemented yet"`. This issue removes no such report except
  `cancel`'s.

## Out of scope / noticed

- Audio capture, recognition, rewrite and delivery — the issue puts them out of
  bounds.
- Microphone permission checking in `doctor` — it needs capture code.
- Verification on a real Windows machine, and key auto-repeat there — issue `#1`.
- The `dictate`, `ptt`, `setup`, `mic` and `model` actions and the `status`,
  `model` and `mic` panes beyond the report they already print.
- Where a speech model is stored is not fixed by the issue or by
  `docs/design.md`. AC-9 requires `doctor` to report its presence, so design has
  to fix the location and the presence test, and the transcription stage inherits
  that as a contract.
- The three `[[panes]]` entries use paths relative to the working directory, and a
  pane command resolves against the pane's working directory rather than the
  plugin root. None of the panes is implemented, so nothing is broken now; it will
  bite whoever implements the first one.
- The leak gate and the English-only rule apply to this change as to every change
  in this repository. They are project rules rather than criteria of this issue.

## Risk

GitHub issues carry no risk field. None was set; nothing is inferred.
