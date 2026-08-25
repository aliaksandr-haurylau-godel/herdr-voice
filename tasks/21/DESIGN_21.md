# DESIGN_21 — context: bias recognition with what the agent is talking about

Covers `AC_21.md` in full, and closes the five open questions it left to this
stage. Revised after the S2 gate returned `QUESTIONS` (`tasks/21/RUN_21.md`,
"Gate S2"): `bias::collect`'s return type is now named, an unrecognised
`[context] source` value has a defined effect, the transcript root and its test
seam are named, every section is checked against AC-9 rather than only the
assembly step, and `Engine::transcribe` is left untouched rather than
half-decided.

Decides nine things: the module and its boundary with `src/context.rs`, the
control flow `[context] source` selects, what `bias::collect` returns and how
an invalid `source` value behaves, how the transcript file is found, which
agents that reaches, how the pane is read, how file names are collected, what
happens when nothing is found and exactly what is logged about it, and why
`Engine::transcribe` is not touched by this design.

## 1 The module

**Context.** `src/context.rs` already owns a different concern — the invocation
context herdr passes per request (`src/context.rs:1`) — and R8 asks for a name
that does not collide with it.

**Decision.** A new module, `src/bias.rs`, with four submodules:

- `src/bias/source.rs` — resolves `[context] source` into a `Source`, or
  refuses it (section 2a).
- `src/bias/transcript.rs` — finds and reads the target agent's transcript
  file, and filters out its service turns.
- `src/bias/pane.rs` — reads the pane's screen contents through herdr.
- `src/bias/files.rs` — collects recently touched file and directory names.

`src/bias.rs` itself holds the `[context] source` dispatch (section 2), the
`Collected` type (section 2a), and the assembly and cap (section 7). This
mirrors how `src/stt.rs` holds `Engine`/`resolve` while `src/stt/command.rs` and
`src/stt/model.rs` hold one concern each.

**Why.** Four sources of failure — an unresolved `source`, a transcript miss, a
pane miss, an empty repository — and four things to fake in a test. One
file per source keeps each fake small and keeps the control flow in section 2
readable without also reading how a `.jsonl` file is parsed.

## 2 What `[context] source` controls

**Context.** AC-3 names three values and states the herdr-call rule for each;
`docs/design.md` section 4 does not yet describe the pane source at all (noted
out of scope for this stage — see section 11).

**Decision.** `bias::collect` dispatches on `[context] source` before doing
anything else:

```
transcript  -> transcript::find(...)                         (herdr never called)
pane        -> pane::read(...)                                (transcript never sought)
auto        -> transcript::find(...); if empty, pane::read(...)
```

Each branch produces a conversation component (possibly empty) and a "found"
flag, independent of the file-names component (section 5), which is always
collected regardless of `source`.

**Why.** This is a direct restatement of AC-3; writing it as three non-branching
calls, rather than one function with a fallback flag threaded through it, keeps
the rule that `pane` never even builds a session-id lookup and `transcript` never
even builds a pane-read command line — not just "doesn't call them", but doesn't
construct the arguments it would call them with.

## 2a `Collected`, and an unrecognised `source`

**Context.** `bias::collect` cannot return a bare `String`. Three decisions
elsewhere in this document need more than text to cross that boundary: the
per-take log line naming which source was tried and what it found, with `auto`
naming both attempts (section 8); an unrecognised `[context] source` refused
with the three valid names listed, rather than silently defaulted (this
section); and that refusal's own effect on the take, which has to be a
checkable outcome and not left implicit.

**Decision.** `[context] source` is resolved once, at daemon start, the same
moment and the same shape recognition already is:

```
pub enum Source { Transcript, Pane }
pub fn resolve(value: &str) -> Result<Source, String>;  // Err names the three
type ContextSource = Result<Source, String>;              // held beside `Recognition`
```

An unrecognised value — anything but `auto`, `transcript` or `pane` — is `Err`,
carrying a message that lists the three valid values, the same shape
`EngineError::Unknown` already gives an unrecognised `[stt] engine`
(`src/stt.rs`). `Source` itself has only two members: `auto` is
`bias::collect`'s own dispatch (section 2) — try `Transcript`, then `Pane` if
it found nothing — not a third value something downstream has to match on.

`bias::collect` takes an already-resolved `Source` (or the caller's instruction
to try both) and never itself fails. What it returns on success:

```
pub struct Collected {
    pub bias: String,                    // the finished, capped string — AC-6's interface
    pub attempted: Vec<(Source, bool)>,  // each source tried, and whether it found anything
    pub file_count: usize,
    pub truncated: bool,
}
```

`bias` is the only field that ever holds conversation or file content; the
other three are counts and flags a caller can log without reading it.

**Why this shape, and not a bare `String`.** A single return type that carries
the finished string alongside what a log line needs about it means the task
that builds `bias::collect` and the task that logs about it can be planned
separately without one guessing the other's shape — which is exactly what a
bare `String` would force: either `bias` grows a side channel later, or the
daemon re-derives "which source found something" by inspecting the text, which
is the kind of inspection AC-9 exists to prevent.

**The refusal's effect on the take.** An invalid `[context] source` never fails
a `dictate` request — the same answer section 8 gives when a source resolves
correctly but finds nothing, extended to the value not resolving at all. The
daemon logs the configuration error once per take (the error, never the
string), and calls `bias::files::collect` directly for the file-name
component, since collecting file names does not depend on `source`. The
conversation component is empty, exactly as if `transcript` or `pane` had been
tried and missed. `[context] source = "vosk"` therefore behaves like a
permanent miss, not like a broken take — every other config-driven gap in this
design costs accuracy, not the recording, and this is that rule applied to a
typo in a key nobody has set correctly yet.

**Why resolved once, not per take.** `stt::resolve` already establishes the
pattern this borrows: a configuration value that can be wrong is checked once,
at daemon start, and the `Result` is held and consulted per request rather
than re-validated on every `dictate` (`src/daemon.rs`, `Recognition`).
Re-parsing `source` on every take would repeat work whose answer cannot change
before a restart, since configuration is read once at daemon start
(`docs/decisions.md`).

## 3 Finding the transcript file

**Context.** The prototype finds the transcript two ways: by the session id
herdr's `agent list` reports, and — when that id is absent, or names a file that
does not exist — by the newest `.jsonl` in a project directory derived by
slugifying the pane's working directory and walking up to the first directory
that exists (`spike/context.sh:73-96`). The id was measured to change while a
session runs and to sometimes name a file that is not there yet
(`spike/context.sh:74-76`). The prototype already falls through to the directory
search whenever the id-named file is missing, so that specific failure is
already handled — but reaching it needs a second herdr call the plugin does not
otherwise make: `agent list`, to read `agent_session.kind`/`value`. Nothing else
in this issue needs that call; only the `pane` source needs a call to herdr at
all, and its contract is `pane read`, not `agent list`.

**Problem.** Adding a second herdr client, for a signal that is measured to be
unreliable and whose failure the same algorithm already routes around, buys
nothing this stage can point to.

**Decision.** Drop the session-id step. Transcript discovery is the directory
search alone: derive the project directory from the pane's working directory
(`focused_pane_cwd` in the invocation context, already available with no
additional call) the same way the prototype does — replace `/`, `.` and `@`
with `-`, walk up to the first directory under the transcript root that exists —
and take the newest `.jsonl` file in it by modification time. "Reliably found"
means exactly this: a deterministic, single-shot lookup against the filesystem
as it stands at the moment the take needs it, with no retry and no wait.

**Why.** This resolves the open question by removing the unreliable input from
the algorithm rather than working around its staleness: the id cannot mislead
discovery if discovery never reads it. It also means the `transcript` and `auto`
branches need no herdr client at all — only `pane` does — which keeps the
outward-call surface (section 4) to the one contract the S1 gate already
sanctioned.

**What this costs.** Two agents both working in the same directory, in two
different panes, cannot be told apart by this algorithm: newest-by-mtime picks
whichever session wrote most recently, which may belong to the other pane. The
prototype's id path does not reliably avoid this either, since the id is exactly
the input just removed — but it is a real, named limitation of the design taken
here, and single-agent-per-directory is the case that happens.

**No retry, stated as a decision.** A transcript written moments after the take
begins is still "not found" for that take. Waiting would put a filesystem poll
on the path that runs while somebody is speaking, which the project already
avoids doing with configuration (`docs/decisions.md`, "configuration is read
once at daemon start").

**The root, named.** `transcript::find` takes the transcript root as a
parameter, not something it computes for itself:

```
pub fn find(cwd: &str, agent: Option<&str>, root: &Path) -> Option<PathBuf>;
```

In production, `src/daemon.rs` computes `root` once, at daemon start, next to
the rest of the configuration: `<home>/.claude/projects` — the directory the
prototype searches (`spike/context.sh:79`) — built from
`config::Vars::from_env().home`, the same environment capture
`config::directory` already reads, rather than a second one. A test never
touches the real home directory: it builds a small directory tree shaped like
a project directory under a temporary path and passes that as `root` directly
— the same kind of seam `bias::pane::read`'s `binary` parameter and
`bias::files`'s repository directory (section 6) already give a test. AC-8's
"a fixture `.jsonl` found by directory" and "a fake transcript reaching the log
line" (section 11) are both exactly this: a scratch `root`, not a mock of the
filesystem.

**The service-turn filter (AC-2).** Once a transcript file is read, each line is
a JSON record; `user`/`assistant` turns whose text begins with
`<task-notification>`, `<system-reminder>`, `<cross-session-message>`,
`<local-command>` or `<command-name>` — a command wrapper, in AC-2's phrasing —
are dropped before counting toward `conversation_turns` and before their text
reaches the bias string, matching `spike/context.sh:98-108` exactly. The last
`conversation_turns` turns that pass the filter are kept.

## 4 Which agents this reaches

**Context.** Where a conversation lives on disk is known for exactly one agent —
the one measured in `docs/evidence.md`, "Recognition, by hand on macOS" and
"Context and its effect on the transcript". `doctor.rs` already carries a
similar one-name list for a different concern, the rewrite stage's command-line
tool (`src/doctor.rs:17-19`, `AGENT_CANDIDATES`).

**Decision.** `transcript::find` takes the invocation's `focused_pane_agent` and
compares it against one constant naming the one agent whose transcript location
is known. When the field names anything else, or is absent, the function returns
"not found" without touching the filesystem — no directory is derived, no search
runs. The constant is declared in `src/bias/transcript.rs`, separate from
`doctor::AGENT_CANDIDATES`: the two lists answer different questions (which tool
to run for rewrite; whose transcript layout is known) and happen to name the
same value today only because the same agent is the only one measured for both.

**Why.** This is what AC-1's "the proven agent" already says; the decision here
is to make the check explicit and total, so a second agent's turns are never
silently searched for using a layout that was never verified. Supporting a named
second agent is out of bounds for this issue (R2 in the issue's own scope
note); this decision is what "out of bounds" means in code — one comparison,
one constant, nothing built on the other side of it.

## 5 Reading the pane

**Context.** No herdr client exists in `src/`: `src/transport.rs` is the
plugin's own socket to itself, not a connection to herdr. `src/doctor.rs`
already runs the `herdr` binary directly, though — `herdr --version`, resolved
through `HERDR_BIN_PATH` with `herdr` as the default (`src/doctor.rs:106`) — so
running it for `pane read` follows an established shape rather than inventing
one. The reviewer's note names the exact contract:
`herdr pane read "$pane" --source recent --lines "$CTX_LINES" --format text`
(`spike/context.sh:42-45`), and the invocation context already carries the pane
id.

**Decision.** `src/bias/pane.rs` splits into two functions, the same way
`src/stt/command.rs` splits `render` from execution:

- `argv(pane, lines) -> Vec<String>` — pure, builds
  `["herdr", "pane", "read", pane, "--source", "recent", "--lines",
  &lines.to_string(), "--format", "text"]`. No process, no I/O.
- `read(pane, lines, binary) -> Result<String, PaneError>` — runs `binary`
  (`HERDR_BIN_PATH`, defaulting to `herdr`, factored out of `doctor.rs` rather
  than duplicated) with `argv`'s output minus the program name, and returns
  standard output on a zero exit.

Output filtering matches the prototype: trim trailing whitespace per line, drop
lines with no letter or digit — a frame or a status line, not content — and keep
the last `PANE_LINES` lines after filtering. `PANE_LINES` is a constant, `80`,
not a fifth `[context]` key: the S1 gate's reviewer noted the pane branch has no
budget of its own among the four keys, and the prototype's 80 lines plus the
overall `prompt_chars` cap already bound the result — the reference the ticket
points at, not a new requirement. "The pane read returned nothing usable"
(AC-3) means the process failed to run, exited non-zero, or the filtered
result is empty.

**Why the split.** `argv` is what a test checks — the exact command line, byte
for byte, against the contract the reviewer named — without a live herdr.
`read` is exercised the same way `command.rs`'s `CommandEngine` is: pointed at a
real, small program during a test (a script that prints fixed text or fails),
never at herdr itself, matching `CLAUDE.md`'s testing rule directly.

## 6 Collecting file names

**Context.** AC-4 asks for names from the repository root, newest first, with
path components rather than basenames, capped at `file_names`. The prototype's
method — `git status --porcelain` for working-tree changes, `git log -30
--name-only` for recent commits, split into path components, de-duplicated,
capped — is already measured to matter: the term evidence restored was a
directory name, missing only because an earlier version collected basenames
(`docs/evidence.md`, "Context and its effect on the transcript").

**Decision.** `src/bias/files.rs` runs the same two `git` commands, from the
repository root found by `git rev-parse --show-toplevel` against the pane's
working directory — not the pane's own subdirectory, which is what AC-4 asks
for and what the prototype does today (`spike/context.sh:57-62`). When the
directory is not inside a git repository, `git rev-parse` fails and the
file-names component is empty — not an error, and not a reason to skip the
rest of the bias string.

**Why.** This is a direct port of a mechanism already measured to matter, with
no change to what it collects. Re-deriving it from first principles would be
inventing a second version of something already shown to work.

**Tested how.** A scratch git repository, built by the test with real `git
init`/`git commit` calls against a temporary directory — the same shape
`src/config.rs`'s tests use real files for, not a fake. `git` is already a
build-time dependency of this checkout (it is a git repository), so this needs
nothing beyond what CI already has.

## 7 Assembling and capping the string

**Decision.** The bias string is file names, then conversation content, joined
by a newline, then hard-truncated to `prompt_chars` characters:

```
<file and directory names, space-separated, newest first>
<conversation turns, one per line, "role: text">
```

File names come first because they are what the one restored-term measurement
(section 6) ties directly to a fix, and because six turns already run to about
2.6 KB — many times the 600-character default cap — so conversation content is
truncated in every ordinary case regardless of where it sits, while file names,
capped structurally at `file_names` entries, usually are not. Putting them
first means the entries most likely to hold an exact term a person is about to
say survive the cut; conversation fills whatever the cap leaves over.

**Why not something more elaborate.** AC-9 forbids adding anything beyond the
filter and the two caps already named. A smarter interleaving — sentence-aware
truncation, weighting recent turns over older ones beyond what
`conversation_turns` already does — is exactly the kind of cleverness AC-9
rules out; a hard character cut on a fixed order is the version that adds
nothing.

## 8 What happens when nothing is found

**Context.** AC-8 requires that none of `transcript`, `pane` or `auto` panics on
a miss; RUN_21's S1 gate flags that a silent difference between the three would
be the worst outcome; the issue lists three candidates — fail the take, proceed
on file names alone, report once.

**Decision.** All three sources, on a miss, take the same outcome: recognition
is never blocked. `Collected.bias` (section 2a) is assembled from whatever was
found — file names alone, when the conversation component came back empty —
and the daemon writes one line to its existing per-request stderr log (the
channel `request_line` and `context_note` already write to, read with `herdr
plugin log list`), built from `Collected`'s other three fields: `attempted`
(which sources were tried and whether each found anything — both entries,
under `auto`), `file_count`, and `truncated`. **That line never contains
`Collected.bias` itself — on a miss or on a hit.** AC-9 says the bias string is
not written to a log beyond what the take needs, and a take does not need its
own bias string echoed back at it; "report once" is satisfied by the line
existing and naming what happened, not by what it quotes.

**Why this, and not failing the take.** The bias string biases recognition; it
is not required for it to run. Failing a take over a missing transcript would
turn an enhancement's absence into a lost recording, which costs the person
more than a slightly less accurate transcript does — and `docs/evidence.md`
already shows recognition producing readable, if imperfect, text with no
context at all. Reporting nothing was the option this stage's owner explicitly
ruled out by asking that a silent difference be avoided; reporting on every
take, rather than once, would repeat CLAUDE.md's rule about the doctor's model
line and issue #13's decision for `cancel`'s liveness probe in the other
direction — noisy on the routine case is its own failure to communicate. This
design reports once per take, not once ever: every miss is logged, on the same
line the request itself already is, so nothing is suppressed and nothing is
duplicated into a second channel.

**Why the outcome is uniform across the three.** The gate's own words — "a
silent difference between the three would be the worst outcome" — argue against
each value inventing its own recovery. `transcript` and `pane` reaching "not
found" by a different route than `auto` does is already a structural
difference (section 2); giving each a different *consequence* on top of that
would be a second, needless one.

## 9 `Engine::transcribe`, left alone

**Context.** `AC_21.md`'s reading of AC-6 calls the wiring's shape "an open
question for design" — an invitation for this stage to decide it.
`Engine::transcribe(&self, audio: &Path)` has no place for a bias string today;
`CommandEngine` bakes model and language in at construction, once per daemon
start, while a bias string depends on the take's target pane and cannot be
known until then (`src/stt.rs:118-130`, `src/stt/command.rs:44-53`). AC-6
itself forbids this issue from widening the trait or `CommandEngine`. And after
section 8's correction, nothing in this design passes the bias string to
anything: it is built, capped, and logged about only in aggregate —
`Collected`'s counts and flags, never `Collected.bias`. `Engine` never sees it.

**Problem.** Deciding a shape for `Engine::transcribe` here would commit a
trait issue #13 already shipped, and with it the `candle` (#15) and `http`
(#16) engines, neither built, to a new parameter that this same document never
once passes to anything. Whoever plans #21 would cut a task to change the
signature and every call site, and the take still would not transcribe with
context afterward — the very next issue would have to touch that signature
again, either to use it or to discover the shape chosen here does not fit what
it needs.

**Decision.** `Engine::transcribe` is not touched by this design. The shape of
the widening — whether the bias string is a second call argument, a value set
before the call, or something `CommandEngine`'s per-take state does not have
room for yet — is decided by the issue that also passes it to the engine, not
by this one. `bias::collect` and the `Collected` type (section 2a) are the
whole of what #21 exposes; AC-6 is satisfied by that exposure existing and
being callable, not by a trait signature nothing in this issue calls.

**Why not decide the shape anyway, as a non-binding note.** A shape written
down here is still a decision, whether or not this issue's own code follows
it: the next issue either follows it or reopens it, and reopening a choice its
own predecessor made reads as the predecessor having been wrong, not as the
question having been legitimately left open. A design is only honest about an
interface if it also exercises it; this one does not, so the accurate account
of what this stage actually knows is that the shape is undecided, not that it
is decided-but-dormant. AC-6's invitation to settle this "as an open question
for design" is answered here by declining, with the reason written down,
rather than left unaddressed.

**What #21 itself does.** `bias::collect` is `pub`, tested, and called from
exactly one place: `src/daemon.rs`'s `dictate` handler, to produce the log
line in section 8. This is what keeps the module reachable from `main` rather
than only from tests — the same reason `IMPLEMENTED` in `src/main.rs:107` is
`#[cfg(test)]` instead of simply unused, since CI runs clippy with
`-D warnings` and an unreached `pub` item in a binary crate is flagged. Nothing
about that call requires `Engine` to change: it is a plain function call whose
`Collected` is inspected for its metadata (section 8) and then dropped.

**What the next issue inherits.** A working, tested `bias::collect` to call,
and no interface commitment to work around or unwind. It decides, with a
concrete consumer in hand — the recognition it is trying to improve — whether
the bias string becomes a second `transcribe` argument or something else, and
it makes that decision the way #13 made its own: settled once, for all three
engines, in one place, rather than reopened per engine as each is built.

## 10 Configuration

`[context]` gains a fourth key beyond the three `docs/design.md` section 7
already lists:

```toml
[context]
source = "auto"            # auto | transcript | pane
conversation_turns = 6
file_names = 40
prompt_chars = 600
```

Read the way every other table in `src/config.rs` is: `#[serde(default)]` on
the struct and on every field, so a file that omits `[context]` entirely, or
omits one key inside it, yields defaults for what it omits — the same shape
`Audio`, `Stt` and `Rewrite` already have (`src/config.rs:27-70`). `source`
stays a `String` in `Config`, deserialized the same permissive way
`Stt::engine` is, rather than an enum `serde` would refuse to parse on a typo
— `bias::source::resolve` (section 2a) is where the string becomes a
`Source` or an error, once, at daemon start, exactly where `stt::resolve`
turns `Stt::engine` into an `Engine` or an error.

`docs/design.md` section 7's `[context]` block gains the `source` line; this is
a document edit this stage makes, since the key did not exist when that
section was written and a plan cannot be cut against a document that omits a
key it reads.

## 11 Modules and tests

| module | owns | tested by |
|---|---|---|
| `bias` | `[context] source` dispatch, assembling `Collected`, the character cap | each of the three `source` values against fakes for transcript and pane, the character cap on a string built to exceed it, `Collected.truncated` set exactly when the cap actually cut something |
| `bias::source` | resolving `[context] source` into `Source`, refusing an unrecognised value | `auto`, `transcript`, `pane` each resolve; an unrecognised name is refused with all three listed |
| `bias::transcript` | directory-derived discovery against an injected root, the one-agent gate, the service-turn filter | a fixture `.jsonl` found under a scratch `root` (section 3), a `focused_pane_agent` that is not the known one, a working directory outside any project directory under `root`, an empty file, service turns excluded from both the count and the content |
| `bias::pane` | the argument list, running the program, filtering the output | `argv`'s exact output against the contract in `spike/context.sh:42-45`; `read` against a script that prints text, one that fails, one that is absent; the alnum-line filter and the 80-line cap |
| `bias::files` | the two `git` commands, path-component splitting, the cap | a scratch repository with staged and committed changes, a working directory outside any repository, more entries than `file_names` |
| `config` | `[context] source`/`conversation_turns`/`file_names`/`prompt_chars` and their defaults | defaults, a partial file, `source` round-tripping as whatever string was written (validity is `bias::source`'s job, not `config`'s) |
| `daemon` | resolving `[context] source` once at start; per take, calling `bias::collect` (or `bias::files::collect` alone, on an unresolved `source`) and logging `Collected`'s metadata | a fake transcript and a fake pane read reaching the log line with the right `attempted` entries; an unresolved `source` logging the configuration error and still producing a file-names-only `bias`; **the log line, in every case above, checked for the absence of the fixture content it was built from** — not merely for the presence of the counts |

No test needs a microphone, a live herdr or a running model. `bias::pane`'s
tests run a real, small program under `HERDR_BIN_PATH`, never `herdr` itself,
following `src/stt/command.rs`'s tests exactly. `bias::files`'s tests run real
`git` against a scratch directory, following `src/config.rs`'s tests. Nothing
here needs a network. The last `daemon` test is the one AC-9's gate failure
argues for directly: a positive assertion that logging happened is not
evidence the string wasn't in it.

## 12 What this design does not decide

- `Engine::transcribe`'s widened shape, and everything downstream of it —
  `CommandEngine`'s placeholder substitution, and the `candle`/`http` engines'
  own handling — belong to the issue that also passes the bias string to the
  engine, per AC-6 and section 9's reasoning for declining to guess the shape
  here.
- Branch, pane title and agent kind, which `docs/design.md` section 4 also
  lists as context components: out of scope for this issue, per the S1 gate's
  note and the issue's own requirements, which cover conversation and file
  names only.
- The rewrite stage's own prompt, which stays a separate concern per the issue.
- A second agent's transcript layout: section 4 names the gate; adding a second
  entry to it is future work, not designed here.

## 13 Where each criterion is decided

| AC | decided in |
|---|---|
| AC-1 conversation turns from the transcript, capped at `conversation_turns` | 3, 4, 7 |
| AC-2 service turns excluded from count and content | 3 (the filter itself), 1 (module boundary) |
| AC-3 `[context] source`, three values, herdr-call rule per value | 2, 2a |
| AC-4 file and directory names, repository root, newest first, path components, capped | 6 |
| AC-5 the finished string capped at `prompt_chars` | 7 |
| AC-6 exposed through an interface, `Engine::transcribe` untouched by this issue | 2a, 9 |
| AC-7 the four `[context]` keys and their defaults | 10 |
| AC-8 no panic on a missing transcript, an unreadable pane, a pane outside a repository | 3, 5, 6, 8 |
| AC-9 no filtering, redaction or persistence beyond AC-2/AC-4/AC-5; nothing logged beyond metadata | 2a, 7, 8, 11 |
