# RUN_3

| field | value |
|---|---|
| issue | #3 — Skeleton: manifest, daemon, socket client, doctor |
| input | GitHub issue, read with `gh issue view 3` |
| stage | S3 |
| branch | feat/3-daemon-client-doctor |
| opened | 2026-08-24 |

## Stages

<!-- One block per stage, appended, never rewritten. -->

### S1 Assess
- artifact: `AC_3.md`
- produced: 2026-08-24

Written against the repository as it was before `#5`: no crate, no manifest, no
CI. Manifest format, the invocation-context field names and the herdr version were
read from the installed herdr 0.8.2 and from an installed third-party plugin
rather than assumed. Three ambiguous points were resolved narrowly and recorded in
the artifact.

```yaml
gate:
  stage: S1
  artifact: AC_3.md
  reviewer: designer
  verdict: READY
  date: 2026-08-24
  questions: []
  blocker: null
  notes:
    - "Not a question, no answer needed: AC-10 requires `doctor` to report whether a transcription model is present while recognition is out of bounds, and neither the issue nor `docs/design.md` fixes where a model lives. I will define the model-store location and the presence test in the design and mark it as a contract the transcription stage inherits."
    - "Not a question: AC-3 and AC-6 name `herdr plugin list --json`, `herdr plugin link` and `herdr plugin action invoke cancel`; the as-is section verified only that the subcommands `link`, `enable`, `list` and `action` exist. I will confirm exact invocation and whether `link` leaves the plugin enabled while designing, and add `enable` to the install path if it does not."
```

### S1 Assess — revised
- artifact: `AC_3.md`, rewritten
- produced: 2026-08-24

`#5` landed the crate, the manifest, the workflows and the run scaffolding while
this run sat at the gate, so roughly half of what the first version of the
acceptance criteria required was already merged. The artifact was rewritten to
cover the remainder only: the daemon as a long-lived process, the client that
reaches it over a Unix socket on macOS and Linux and a named pipe on Windows, and
`doctor`. The as-is section now rests on the merged code, with line references
into `src/main.rs`, `herdr-plugin.toml`, `scripts/check_manifest.py` and the two
workflows.

Both notes the designer left on the first version survive into the rewrite: the
model location is still undecided and named as a design obligation, and the exact
`herdr plugin` invocations are no longer asserted as acceptance criteria.

The rewrite also removed absolute home paths from the citations, which the leak
gate rejects.

A revised artifact needs a new verdict; the gate runs again.

```yaml
gate:
  stage: S1
  artifact: AC_3.md
  reviewer: designer
  verdict: READY
  date: 2026-08-24
  questions: []
  blocker: null
  notes:
    - "As-is section checked against main and holds: `Cargo.toml:15` has an empty `[dependencies]`; every subcommand falls into the single `NOT_IMPLEMENTED = 69` arm (`src/main.rs:109`, `src/main.rs:118`); `herdr-plugin.toml` declares actions `dictate`, `ptt`, `cancel`, `setup` and panes `status`, `model`, `mic` with no `doctor` entry while `parse` accepts `doctor` (`src/main.rs:73`); `scripts/check_manifest.py:50` reads known subcommands with `re.findall(r'Some\(\"([a-z-]+)\"\)', source)`, so the manifest-to-binary coupling described is real; the five checks are the three-OS matrix (`.github/workflows/check.yml:19`), the manifest job (`.github/workflows/check.yml:33`) and the leak gate (`.github/workflows/leak-gate.yml:9`). Nothing in the as-is section is asserted beyond what the merged code shows."
    - "Not a question, no answer needed: the previous verdict's model-location note survives unchanged, and the artifact now states the obligation itself under 'Out of scope / noticed'. I will fix the model-store location and the presence test in the design and mark them as a contract the transcription stage inherits."
    - "Not a question: `RUN_3.md:53` says the exact `herdr plugin` invocations are no longer asserted as acceptance criteria, but AC-5 still names `herdr plugin action invoke cancel`. I read AC-5 as requiring that an action invocation reach the daemon, with the exact command line a verification detail I will confirm while designing. No revision needed for me to proceed."
    - "Not a question: AC-3 fixes the socket path derivation only for the Unix side, while R5 and AC-12 require a named-pipe implementation on Windows. The pipe-name derivation, the wire format of the request, the shutdown signal for AC-1, how `doctor` locates the herdr binary when `HERDR_BIN_PATH` is absent, and where the daemon records a received request for AC-5 are all mine to decide — both ends of the transport are this binary, so no external contract is missing."
    - "Separate note on scope, not a reason to withhold READY: chosen reading 4 gives behaviour only to `cancel`, and with no pipeline there is nothing for `cancel` to cancel, so AC-5 reduces to a round-trip proof. That matches the issue's 'proves the shape' goal; I mention it only so the plan does not later read AC-5 as real cancellation semantics."
```

### S2 Design
- artifact: `DESIGN_3.md`
- produced: 2026-08-24

Decides the five things the acceptance criteria left open: the transport, the
message format, the daemon's lifecycle, the contents of `doctor`, and the
dependencies. One dependency is added, `interprocess` 2.4.3, and it is confined to
the `transport` module; the configuration and the invocation context are read by
code written here, which the design names as the risk it is taking.

The gate for this stage had no reviewer at first: the S2 role is `planner`, which
maps to the agent `octoflow-reviewer-planner`, and only
`octoflow-reviewer-designer` was installed. The reviewer is now a **project**
agent, `.claude/agents/octoflow-reviewer-planner.md`, rather than an addition to
anyone's global agent set: a project agent is visible only in this repository,
travels with it, and is reviewed in the pull request like any other file here. Its
form is taken from the installed designer reviewer and its one question is whether
the design can be cut into tasks with real dependencies without guessing. Its tools
are read-only.

### S2 Design — revised
- artifact: `DESIGN_3.md`, section 5 replaced
- produced: 2026-08-24

The first version took one dependency and wrote the JSON and TOML readers by hand,
naming that as a deliberate trade. The trade is off: a hand-written JSON reader is
a defect factory on escapes, unicode and nesting, and the cost of one such defect
exceeds the line it saves. The design now takes `serde` with `derive`, `serde_json`
and `toml` alongside `interprocess`, and the section that argued for the
hand-written readers is gone. The ban on asynchronous runtimes stands.

The machine-wide Windows pipe name is now issue `#6` and no longer attributed to
`#1`, which is about auto-repeat and audio capture. The design points at `#6`.

The model-presence contract stays a filename substring in this issue, and section
4 now records that the recognition issue has to replace it with an exact name and
an integrity check: a substring matches an unrelated file.

```yaml
gate:
  stage: S2
  artifact: DESIGN_3.md
  reviewer: planner
  verdict: QUESTIONS
  date: 2026-08-24
  questions:
    - "The frame in section 2 carries three things — `voice/1 <command> <body-length>` plus the context bytes — but section 3 requires the daemon to report each request 'naming the command and the entrypoint it came from', and AC-5 requires the recorded line to name the entrypoint. The entrypoint id is not a field of the invocation context (`AC_3.md` lists the 0.8.2 context fields; `HERDR_PLUGIN_ENTRYPOINT_ID` is a separate environment variable, visible only to the client). Nothing states whether the client puts it on the wire, and if so where in the frame. The `proto` task and the `client`/`daemon` tasks cannot be written independently without that: one of them would have to invent a fourth header token or a body wrapper, and the other would have to guess it."
    - "Section 2 decides that the daemon parses the context and answers `error` when it is unparsable, and the client exits non-zero on `error`. AC-8 requires the opposite for the only command wired in this issue: with `HERDR_PLUGIN_CONTEXT_JSON` absent or unparsable, `cancel` — which needs no target pane — still works, and only a command that needs a pane fails. Nothing says whether the parse result is per-command (unusable context tolerated for `cancel`, fatal for a pane-needing command) or a hard reject of the request. The two readings give the `daemon`/`context` task opposite done-criteria and opposite exit codes for the same test, so I cannot write the task's done-criterion."
    - "Section 4 has `doctor` print the configuration file's path (`config  default  no file at <path>`), and AC-10 requires it to state that defaults were used, but no section fixes how that path is derived: no file name, no environment variable, no fallback chain — unlike the socket name, which section 1 derives explicitly with fallbacks. `docs/design.md` section 7 says only 'a TOML file in the plugin's configuration directory'. The `config` task has no stated input and the `doctor` task has no checkable output line, so both would take the location silently."
    - "The `rewrite` check in section 4 says that for `agent` with `auto` it looks for 'any of the default candidates on `PATH`', and this is the default configuration path (`docs/design.md` section 7: `engine = \"agent\"`, `agent = \"auto\"`), so it is the line `doctor` prints on a machine with no config file. The candidate list exists nowhere — not in section 4, not in `docs/design.md`. Since `doctor` exits 0 only when every line is `ok`, the task implementing this check would have to invent the list and thereby decide `doctor`'s exit code on a clean machine, which is a product decision, not an implementation detail."
  blocker: null
  notes:
    - "Not a question: keeping the transport dependency behind one module, the thread-per-connection daemon and the connect-before-listen order are decisions with reasons, and I can plan tasks around all three. The `stop` request existing only for the tests is likewise plannable — I would give it its own task boundary inside `daemon`."
    - "Not a question: the module table in section 6 gives me eight tasks with clean inputs, outputs and dependencies (`proto` and `transport` first, then `daemon`/`client`, `context`/`config`, then `doctor`, then `main` wiring and the `check_manifest.py` constant check), and section 8 maps every criterion to a section. The four questions above are the only places where a task would have to take a decision silently."
```

The reviewer ran under a stand-in: the project agent had just been created and this
session's agent registry was fixed when the session started, so the reviewer's own
definition file was handed to a read-only agent instead. That weakens the
guarantee that a reviewer cannot edit what it reviews from structural to verified,
so it was verified: the working tree, both artifacts' checksums and `HEAD` were
identical before and after the review.

### S2 Design — answers to the gate
- artifact: `DESIGN_3.md`, sections 2, 4 and 6
- produced: 2026-08-24

All four questions are answered in the artifact; three of them by a fact rather
than by a preference.

1. The entrypoint goes on the wire as a fourth header token, with `-` standing for
   an unset variable so the token count never varies.
2. A command now declares whether it needs a target pane. The daemon parses the
   body once and keeps the outcome; a command that needs a pane and has none
   answers `error`, a command that needs none proceeds. `cancel` needs none, which
   is what AC-8 requires, and the daemon still records that the context could not
   be read.
3. The configuration file is `<config>/config.toml`, where `<config>` is
   `HERDR_PLUGIN_CONFIG_DIR` or the same directory herdr itself computes.
   `herdr plugin config-dir haurylau.voice` prints
   `~/.config/herdr/plugins/config/haurylau.voice` even for a plugin that is not
   installed, which is the case `doctor` has to survive from a plain terminal.
4. The candidate list for `agent = "auto"` has one entry, `claude`: it is the only
   agent command-line tool the prototype used (`spike/spike.sh:103`) and the only
   one the rewrite measurements in `docs/evidence.md` were made with. A second name
   goes on the list when a second tool is measured.

```yaml
gate:
  stage: S2
  artifact: DESIGN_3.md
  reviewer: planner
  verdict: READY
  date: 2026-08-24
  questions: []
  blocker: null
  notes:
    - "All four earlier questions are closed in the artifact, and closed in a way I can plan from, not just answered in prose. (1) The entrypoint is a fourth header token with `-` for an unset variable, so `proto` has a fixed token count and `client`/`daemon` can be written against it independently. (2) 'needs a target pane' is now a property of the command, so the `daemon`/`context` task has one done-criterion instead of two contradictory ones: malformed body tolerated for `cancel` and exit 0, fatal and named for a pane-needing command — which is AC-8 as written. (3) The config path is `<config>/config.toml` with `HERDR_PLUGIN_CONFIG_DIR` then the two XDG fallbacks, so the `config` task has a stated input and the `doctor` config line has a checkable output. (4) The `agent = \"auto\"` candidate list is exactly `claude`, with the reason, so the `doctor` task no longer decides the clean-machine exit code by inventing a list."
    - "Task cut I would make from section 6, and it holds: `proto` and `transport` with no dependencies; `context` and `config` with none; `daemon` on `proto`+`transport`+`context`; `client` on `proto`+`transport`; `doctor` on `transport`+`config`; `main` wiring last (only `cancel` leaves the 69 arm, AC-13); and the `min_herdr_version` constant plus its `scripts/check_manifest.py` check as its own task. Section 8 maps every AC to a section and I found no criterion without an owner and no ordering constraint that contradicts another."
    - "Not a question, and the thinnest place in the artifact — AC-7. Section 2 describes the client's reply handling but never the connect-failure path, and the `client` row in section 6 lists `ok`, `error` and a closed connection but not 'no daemon'. Neither the message text nor any read/connect bound is stated. I can still write the task: its done-criterion comes straight from AC-7 — non-zero exit, no panic, no hang, and a message naming how to start the daemon — and the exact wording and timeout value are implementation choices whose outcome that criterion already checks. I would rather it were in the design than in my plan, but it does not stop me."
    - "Not a question: section 1 says one module hides the transport and section 5 lists `interprocess` as 'the named pipe on Windows', which leaves it open whether Unix uses `std` sockets or `interprocess` too, and therefore whether the dependency is target-specific in `Cargo.toml`. Both readings sit entirely inside the `transport` task and neither changes the interface anyone above it sees, so no other task has to guess."
    - "Separate note on a choice, not a gap: `toml = \"1\"` in the dependency table. If no 1.x of that crate is published, the version pin — not the decision to take the crate — is what the implementing task will have to correct; the argument for taking a TOML parser at all is made and I accept it."
```

The stand-in reviewer changed nothing again: working tree, both artifacts'
checksums and `HEAD` were identical before and after.

Three of the reviewer's notes are carried into S3 rather than back into the design,
because the design has a verdict and reopening it for material the reviewer
explicitly did not block on would cost a third gate for no decision:

- AC-7's client path — the message and the bound — is stated in the plan.
- Whether the Unix side uses `std` or `interprocess` sits inside the `transport`
  task, which decides it.
- `toml = "1"` needs no correction: version 1.0.7 is published, and 1.1.4 is the
  latest. The reviewer flagged it conditionally and the condition does not hold.

### S3 Plan
- artifact: `PLAN_3.md`
- produced: 2026-08-24

Eight tasks, each with its own test cycle and its own commit. The plan carries the
code rather than descriptions of it, and states the interface each task produces so
a task's implementer learns the names the neighbouring tasks use without reading
them.

Five defects were found in the plan before it was handed over, each of which would
have cost the executor a red build: `write!` with a trailing newline where clippy
demands `writeln!`; a `Default` implementation clippy calls derivable; a test
helper introduced in a task later than the one that uses it; a models directory
derived from the socket name, which on Windows is a pipe name with no parent
directory; and a constant used only by tests, which trips `dead_code` under
`-D warnings`.

One fact was verified by running it rather than recalled:
`GenericNamespaced::is_supported()` is **true on macOS**, where a namespaced name
resolves to a file under the temporary directory. The plan therefore selects the
platform by `cfg(windows)`, and says why in its global constraints. Taken from
memory, this would have moved the socket out of the state directory on macOS.

The reviewer for this stage is a project agent as well,
`.claude/agents/octoflow-reviewer-implementer.md`, and it runs on the smaller model
deliberately: by the stage definition, a plan the implementer cannot execute
without guessing is rewritten, and the model is never escalated.

```yaml
gate:
  stage: S3
  artifact: PLAN_3.md
  reviewer: implementer
  verdict: READY
  date: 2026-08-24
  questions: []
  blocker: null
  notes:
    - "Verified against the actual `interprocess` 2.4.3 source cached at ~/.cargo/registry (not from memory): `ListenerOptions::new().name(...).try_overwrite(true).create_sync()`, `Stream::connect(name)` via the `traits::Stream` trait brought into scope by `local_socket::prelude::*`, and `to_fs_name::<GenericFilePath>()` / `to_ns_name::<GenericNamespaced>()` on `&str` all exist with the signatures the plan uses. Nothing in Tasks 1-8 names a symbol the crate does not have."
    - "Every task ships complete, non-placeholder code for both the failing test and the implementation; the two hedges in the plan (`toml::de::Error::message()` may not exist, use `.to_string()` instead; `check_manifest.py`'s `source` variable needs moving up one block) are stated as explicit fallback instructions, not blanks."
    - "Cross-checked `src/main.rs`, `Cargo.toml`, `herdr-plugin.toml`, `scripts/check_manifest.py` and `.github/workflows/check.yml` against every path and line the plan cites; all exist as described, and `MIN_HERDR_VERSION`/`Some(\"...\")` regex targets both match what Task 7/8 add."
    - "Not a question, no answer needed: `daemon::answer`'s `cancel` arm never calls `context::parse`, so it never produces the 'daemon still records that the context could not be read' line DESIGN_3.md section 2 describes. The plan gives the exact code and exact tests either way, so I have nothing to guess — this is a design-fidelity gap, not something that stops me from executing Task 5 as written."
    - "Task ordering, dependency direction and the AC-to-task mapping are explicit ('Notes for the executor'); I found no task that consumes something a later task produces."
```

The stand-in reviewer changed nothing: working tree, both artifacts' checksums and
`HEAD` were identical before and after.

### S3 Plan — revised
- artifact: `PLAN_3.md`, Task 5
- produced: 2026-08-24

The reviewer's fourth note is a real divergence and was closed rather than carried
into the code: `answer`'s `cancel` arm did not touch the body, so the line the
design promises — the daemon records that the context could not be read, even for a
command that needs no pane — would never have been printed. Task 5 now produces
`context_note(&Request) -> Option<String>`, which the accept loop logs after the
request line, with three tests: an absent body, a malformed body and a good one.
Splitting it out of `answer` keeps it testable without capturing standard error.

The body a command ignores is the same body the next pane-needing command will get,
and a silent skip is the failure this project treats as a defect of the same weight
as a wrong transcript.

Revising the artifact means a new verdict, so the gate runs again.

## Notes

The pipeline is untouched. Every action except `cancel` keeps exiting 69 with
`"not implemented yet"`, and `scripts/check_manifest.py` keeps that honest: a
subcommand is callable from the manifest only after it appears in `parse`
(`src/main.rs:68`).
