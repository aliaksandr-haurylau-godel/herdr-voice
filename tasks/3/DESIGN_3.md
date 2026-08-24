# DESIGN_3 — the daemon, the client, and doctor

Covers `AC_3.md` in full. Decides five things the acceptance criteria leave open:
the transport, the message format, the daemon's lifecycle, what `doctor` checks,
and which dependencies the crate takes. It decides nothing about capture,
recognition, rewrite, delivery, indication or push-to-talk; those have their own
issues and `docs/design.md` already fixes their shape.

## 1 Transport

**Context.** The daemon and the client are the same binary. On macOS and Linux the
two ends meet over a Unix domain socket; on Windows, over a named pipe
(`docs/design.md` section 2). CI runs the test suite on `windows-latest`, so the
Windows side has to compile and pass its tests whatever is verified by hand.

**Problem.** `std` has Unix domain sockets but no named pipes: the server side of a
Windows pipe needs `CreateNamedPipeW`, which means hand-written `winapi` calls and
unsafe code. Two separate implementations also invite two separate bugs.

**Decision.** One module, `transport`, is the only place that knows how the two
ends meet. It exposes a listener and a stream, and one function that derives the
name to listen on. Everything above it — the daemon, the client, `doctor` — sees
byte streams and nothing else.

The name is derived per platform, because the platforms' namespaces differ:

- macOS and Linux: the filesystem path `<state>/voice.sock`, where `<state>` is
  `HERDR_PLUGIN_STATE_DIR` when herdr sets it, and otherwise
  `$XDG_STATE_HOME/herdr/plugins/haurylau.voice` or, failing that,
  `$HOME/.local/state/herdr/plugins/haurylau.voice`.
- Windows: the namespaced name `haurylau.voice`, which resolves to
  `\\.\pipe\haurylau.voice`.

The derivation reads only the environment, never the working directory: herdr
starts plugin commands in directories this binary does not choose.

**Why.** Keeping the crate's one dependency behind a single module means replacing
it later is a change to one file rather than to every call site. Deriving the name
from the environment satisfies the requirement that the daemon and the client
agree on where to meet without either of them depending on where it was started.

**Consequence, recorded rather than solved.** A namespaced pipe name is
machine-wide, so on Windows two users on one machine, or two herdr instances
belonging to one person, reach one daemon. The Unix side has no such problem,
because the state directory is per user. Fixing the name is issue `#6`; this issue
implements the transport on macOS first and leaves the name as it is.

## 2 Message format

**Context.** The client is not a long-running program. It is started by herdr on
every action, and holding a key produces roughly twelve invocations per second
(`docs/evidence.md`: three holds, 62 and 50 events over 5.05 s and 4.10 s, median
gap 85 ms). The invocation context arrives as JSON in the environment variable
`HERDR_PLUGIN_CONTEXT_JSON`.

**Problem.** Anything the client does per invocation is paid twelve times a
second, on top of process startup. Parsing the context in the client would be the
largest of those costs, and it would be pure waste: the client has no use for the
fields.

**Decision.** A frame of a header line and a body:

```
voice/1 <command> <entrypoint> <body-length>\n
<body-length bytes of body>
```

Four tokens, all on one line. `<entrypoint>` is `HERDR_PLUGIN_ENTRYPOINT_ID`, and
it is on the wire because the daemon has to name it in the line it records and the
variable reaches the client only: it is an environment variable, not a field of the
invocation context. A single `-` stands for a variable that was not set, so the
token count never varies.

The body is the bytes of `HERDR_PLUGIN_CONTEXT_JSON`, copied without inspection; a
length of zero means the variable was absent. The reply is one line: `ok` followed
by optional text, or `error` followed by a message. The client writes the frame,
reads the reply line, prints an `error` message to standard error, and exits — 0
for `ok`, non-zero for `error`.

The daemon parses the body, because it is the end that needs the fields and the
end that is already resident.

**A body that will not parse is not by itself a failure.** Each command declares
whether it needs a target pane. The daemon parses the body once, keeps the outcome
— fields, or the reason they are missing — and then:

- a command that needs a target pane and has none answers `error` naming what was
  missing, and the client exits non-zero;
- a command that needs none proceeds and answers `ok`.

`cancel`, the one command wired in this issue, needs no target pane: it stops
whatever is running and clears what a dead run left. So an absent or malformed
context leaves `cancel` working and exiting 0, which is what the criteria require,
while the daemon still records one line saying the context could not be read. The
distinction lives in one place — a property of the command — rather than in each
command's code.

**Why.** The client's whole run becomes: read an environment variable, connect,
write a header and copy a buffer, read one short line. The header parses by
splitting one line on spaces. Nothing in that path grows with the size of the
context.

This also settles where an unparsable context is reported: the daemon finds it and
answers `error`, and the client prints what the daemon said. A malformed context
therefore produces a message rather than a panic, and the message names the field
that could not be read.

## 3 Daemon lifecycle

**Context.** The manifest's `[[startup]]` entry already runs
`target/release/herdr-voice daemon`, and herdr may run it more than once — on
every start, and again after a plugin is re-enabled.

**Problem.** Two hazards sit next to each other. A second daemon must not start
beside a live one, and a socket file left behind by a daemon that died must not
stop the next one from starting. The obvious fix for the second — always overwrite
the name — causes the first, by taking the name away from a running daemon.

**Decision.** Start in this order, which is the reverse of the intuitive one:

1. Try to connect to the name. If the connection succeeds, a daemon is already
   alive: print one line saying so and exit 0.
2. Otherwise create the listener, reclaiming a stale name.

Then accept connections in a loop, handling each in its own thread. Each accepted
request is reported as one line on standard error, naming the command and the
entrypoint it came from; herdr captures a plugin's standard error, so
`herdr plugin log list --plugin haurylau.voice` shows it. The daemon runs until it
is signalled, which is how herdr stops it. There is a `stop` request on the wire
for the tests, which need to shut a listener down without signals; no action or
subcommand sends it.

**Why.** Connecting first is the only order that distinguishes a live daemon from
a dead one's leftovers, because a stale socket file refuses connections while a
live one accepts them. A thread per connection is the cheapest thing that works
for a stream of commands from one person; an asynchronous runtime would add a
dependency and startup cost to a process whose entire job is to answer a few bytes.

Reporting to standard error rather than to a file of our own means the record is
visible through the tool the author already uses to see what herdr ran. The run
journal of `docs/design.md` section 6 stays what it is there: off by default, and
somebody else's issue.

## 4 What doctor checks

**Context.** The prototype's worst failure was silence: a parse error produced no
output at all and looked like a hang for a morning (`CLAUDE.md`, rules for the
code). `doctor` is the answer to "why is nothing happening".

**Problem.** A check that reports a state without saying what to do about it moves
the question rather than answering it.

**Decision.** Five checks, one line each, in a fixed order: a name, a state, and a
detail. Every line whose state is not `ok` names the next action.

```
herdr    ok       0.8.2, manifest needs 0.8.0 or newer
daemon   ok       listening at <name>
config   default  no file at <path>, defaults used
model    missing  put a speech model in <dir>; the chooser is not built yet
rewrite  ok       found "<name>" in PATH
```

- **herdr** — the binary from `HERDR_BIN_PATH`, or `herdr` on `PATH` when that
  variable is unset. Its `--version` output is compared against the minimum the
  manifest declares. The comparison is three integers, written here rather than
  taken from a crate.
- **daemon** — whether connecting to the derived name succeeds, and the name.
- **config** — the file, or the fact that defaults were used, and the path that was
  looked at. The path is `<config>/config.toml`, where `<config>` is
  `HERDR_PLUGIN_CONFIG_DIR` when herdr set it, and otherwise
  `$XDG_CONFIG_HOME/herdr/plugins/config/haurylau.voice` or, failing that,
  `$HOME/.config/herdr/plugins/config/haurylau.voice`. The fallback is not a guess:
  `herdr plugin config-dir haurylau.voice` prints exactly that directory, and it
  prints it for a plugin that is not installed, which is the case `doctor` has to
  survive when it is run from a plain terminal.
- **model** — whether `<state>/models/` holds a file whose name contains the
  configured `[stt] model`.
- **rewrite** — the engine from `[rewrite] engine`. For `agent`, whether the
  configured program is on `PATH`; when it is `auto`, the first name on the
  candidate list that is on `PATH`. The candidate list has one entry, `claude`,
  because that is the only agent command-line tool the prototype used
  (`spike/spike.sh:103` refuses to start without it) and the only one every rewrite
  measurement in `docs/evidence.md` was made with. A second name goes on the list
  when somebody measures a second tool, not before. For `http`, whether an endpoint
  is configured. For `command`, whether the program exists.

  This fixes what `doctor` prints on a clean machine, which is the point of asking:
  with no configuration file the defaults are `engine = "agent"` and
  `agent = "auto"`, so the line is `ok` on a machine that has `claude` and
  `missing` on one that does not, and the exit code follows from that rather than
  from an invented list.

`doctor` exits 0 when every line is `ok` and 1 otherwise.

The minimum herdr version lives as a constant in the code, and
`scripts/check_manifest.py` gains a check that the constant and the manifest's
`min_herdr_version` agree.

**Why.** A fixed order and a fixed shape make the output readable at a glance and
testable from a fixed set of findings. The version constant is checked against the
manifest because the alternative is a second copy of the same number that nothing
keeps in step — exactly the failure `scripts/check_manifest.py` was written to
prevent for subcommands.

**Contract this creates.** `<state>/models/` is where a speech model lives, and
presence is a filename containing the configured model name. The transcription
stage inherits both, and has to strengthen the second: a substring test matches an
unrelated file, which is good enough for a report that says "no model" and not
good enough to choose what to load. That issue replaces it with an exact name and
an integrity check.

## 5 Dependencies

**Context.** The crate has none (`Cargo.toml:15`).

**Problem.** Three formats meet in this issue and none of them is ours. The
transport is a Unix socket on one family of platforms and a named pipe on the
other. The invocation context is JSON, produced by herdr. The configuration is
TOML, in the schema of `docs/design.md` section 7.

**Decision.** Four dependencies, and no asynchronous runtime in this project until
a task appears that cannot be done without one:

| crate | version | what it is for |
|---|---|---|
| `interprocess` | 2.4.3 | the named pipe on Windows |
| `serde` | 1, feature `derive` | the context and configuration types |
| `serde_json` | 1 | the invocation context |
| `toml` | 1 | the configuration file |

**Why.** On Unix a Unix domain socket is in `std`; a Windows named pipe is not,
and the alternative is writing `winapi` calls by hand. `interprocess` has an empty
default feature set, so nothing asynchronous arrives with it, and its minimum Rust
version, 1.75, is below the crate's 1.82.

The other three replace parsers that would otherwise be written here. A
hand-written JSON reader is a defect factory on escapes, unicode and nesting, and
the cost of one such defect is higher than the line it saves in `Cargo.toml`. The
same argument covers TOML, which arrives in this issue rather than later because
`doctor` reads `[stt] model` and `[rewrite] engine` to report on the model and the
rewrite engine, and reports whether a file was found at all.

Both readers ignore keys and fields they do not know, so a configuration file
carrying keys for later stages does not stop the daemon, and a herdr release that
adds a context field does not stop an action.

## 6 Modules and tests

Each module owns one thing, and its tests sit beside it. The interface between
them is what makes the pipeline stages testable later without a microphone
(`CLAUDE.md`, testing).

| module | owns | tested by |
|---|---|---|
| `main` | argument parsing, dispatch, exit codes | the four tests already there, plus the new commands |
| `proto` | the frame: encode, decode, replies | round trips, zero-length body, an absent entrypoint as `-`, malformed header, a wrong token count |
| `transport` | the name, the listener, the stream | a round trip over a temporary name, on all three platforms |
| `daemon` | connect-or-listen, the accept loop, the request line, which commands need a pane | connect-first logic against a live and a stale name; a malformed body tolerated for `cancel` and fatal for a pane-needing command |
| `client` | one request, one reply, one exit code | reply handling: `ok`, `error`, a closed connection |
| `context` | the invocation context, deserialised from JSON | absent fields, unknown fields, malformed input |
| `config` | the configuration, with a default for every key | absent file, partial file, unknown keys, bad values |
| `doctor` | the five checks and their rendering | rendering and exit code from a fixed set of findings |

None of these tests needs herdr, a microphone or a model. What cannot be covered
that way — the daemon under a real herdr on macOS — is verified by hand and
written into `docs/evidence.md` with the platform.

## 7 What this design does not decide

- Where the pipeline's stages live. `dictate`, `ptt`, `setup` and the three panes
  keep reporting that they are not implemented.
- Whether `doctor` becomes a manifest action or pane. It is a subcommand here. If
  a later issue adds a manifest entry, `scripts/check_manifest.py` keeps it honest.
- The Windows pipe name being machine-wide rather than per user — issue `#6`.
- The three `[[panes]]` entries invoke the binary by a relative path, and a pane
  command resolves against the pane's working directory rather than the plugin
  root. No pane is implemented, so nothing is broken; whoever implements the first
  one has to fix the path.

## 8 Where each criterion is decided

| AC | decided in |
|---|---|
| AC-1 daemon stays alive | 3 |
| AC-2 a second start leaves one daemon | 3 |
| AC-3 the name, derived from the environment | 1 |
| AC-4 a stale name does not block a start | 1, 3 |
| AC-5 an action reaches the daemon, which records it | 2, 3 |
| AC-6 the context travels verbatim | 2 |
| AC-7 no daemon: a bounded, named failure | 2, 3 |
| AC-8 absent or unparsable context never panics | 2, 5 |
| AC-9 what doctor reports | 4 |
| AC-10 defaults when no file exists | 4, 5 |
| AC-11 the checks and the two required tests | 6 |
| AC-12 the suite passes on Windows too | 1, 6 |
| AC-13 the other commands still report 69 | 7 |
