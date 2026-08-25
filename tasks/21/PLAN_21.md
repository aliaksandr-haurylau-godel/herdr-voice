# PLAN_21 — Context: bias recognition with what the agent is talking about

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Assemble a capped bias string per take — recent conversation
content plus recently touched file and directory names — behind an
interface a later issue can hand to recognition, logging only metadata
about it.

**Architecture:** A new `bias` module: `Source` and `collect` in
`src/bias.rs`, one submodule per source of failure — `source`,
`transcript`, `pane`, `files`. `src/daemon.rs` resolves `[context] source`
once at start, calls `bias::collect` per take, logs `Collected`'s counts
and flags, never the string.

**Tech Stack:** Rust, `serde`/`toml`, `git` and `herdr` run as external
processes. No new dependencies.

**Spec:** `tasks/21/DESIGN_21.md` (READY), argued from `tasks/21/AC_21.md`
(READY). Both sit beside this plan; task steps cite sections and AC
numbers rather than repeating their text — read them alongside this file.

## Global Constraints

- No panics on the context-assembly path (AC-8).
- A miss never fails a take; recognition proceeds on whatever was
  collected (design §8).
- `[context] source` is exactly `auto` (default), `transcript` or `pane`;
  an unrecognised value is refused, naming all three, and the take still
  runs on file names alone (AC-3, AC-7, design §2a).
- `Collected.bias` — the finished string — is never logged, on a hit or a
  miss (AC-9, design §8).
- An absent configuration file, or one that omits `[context]` or a key in
  it, is valid and yields defaults (AC-7).
- Configuration is resolved once at daemon start, not per take
  (`docs/decisions.md:38`).
- No test needs a microphone, a live herdr, a model or a network.
  `bias::pane` tests run a small script under an explicit `binary`
  argument. `bias::files` tests run real `git` against a scratch
  directory. `bias::transcript` tests use a scratch directory as the
  transcript root.
- English only; no absolute home path or personal/employer name in any
  file, comment, fixture or commit message. Cite paths relative to the
  repository root.

**Keep these verbatim — a test asserts on each:**

- `[context]` keys and defaults: `source = "auto"`, `conversation_turns =
  6`, `file_names = 40`, `prompt_chars = 600`.
- Service-turn markers (AC-2): `<task-notification>`, `<system-reminder>`,
  `<cross-session-message>`, `<local-command>`, `<command-name>`.
- The pane-read contract (design §5, `spike/context.sh:42-45`): `herdr
  pane read "$pane" --source recent --lines "$CTX_LINES" --format text`.

### Dependency on issue #22

#22 lands first. It replaces the `Recognition` value threaded through
`src/daemon.rs`'s `start`, `serve`, `serve_one`, `answer`, `dictate` and
`transcribe` with one per-daemon bundle (called **Runtime** below, since
its real name is #22's to choose), and extracts from `src/doctor.rs` a
helper that runs the `herdr` binary through `HERDR_BIN_PATH`. #22 does
**not** thread the working directory or the agent name into `dictate`.

Tasks 1-7 build and test the `bias` module and the `[context]` table on
their own; none touch `src/daemon.rs` and none depend on #22. **Tasks 8-10
touch `src/daemon.rs` and cannot start until #22 has merged.** Each names,
in its own text, what to check in the merged code before writing it —
this plan does not guess Runtime's field names or `dictate`'s parameter
list; read the merged `src/daemon.rs` first.

---

### Task 1: Document the fourth `[context]` key

**Files:** Modify `docs/design.md` (§7's `[context]` block; §4).

Add `source = "auto"  # auto | transcript | pane` to §7's `[context]`
block. In §4, add one sentence: when no transcript is found, or when
`[context] source = "pane"`, the conversation component is read from the
pane's screen through `herdr pane read` instead (design §10).

- [ ] Edit both sections.
- [ ] Check: `grep -n 'source = "auto"' docs/design.md` — one match.
- [ ] Commit: `git add docs/design.md && git commit -m "Document the [context] source key and the pane source"`

---

### Task 2: The `[context]` configuration table

**Files:** Modify `src/config.rs`.

**Produces:** `pub struct Context { source: String, conversation_turns:
usize, file_names: usize, prompt_chars: usize }`, defaults as listed
above; `Config.context: Context`, `#[serde(default)]` throughout, matching
`Audio`/`Stt`/`Rewrite`'s existing shape (AC-7).

- [ ] Write failing tests: defaults are `"auto"`/6/40/600; a file with only
  `[context]\nsource = "pane"` keeps the other three defaults; a file that
  omits `[context]` entirely yields all four defaults. Follow this file's
  existing `scratch(tag)` fixture pattern.
- [ ] Run: `cargo test --lib config::tests::` — verify FAIL (no `context`
  field).
- [ ] Implement `Context` and its `Default`, wire into `Config`.
- [ ] Run: `cargo test --lib config::` — PASS.
- [ ] Commit: `git add src/config.rs && git commit -m "Read the [context] configuration table, with its four defaults"`

---

### Task 3: `bias::Source` and `bias::source::resolve`

**Files:** Create `src/bias.rs`, `src/bias/source.rs`; modify
`src/main.rs` (`mod bias;`, next to `mod context;`).

**Produces:** `pub enum Source { Transcript, Pane, Auto }` (`Debug, Clone,
Copy, PartialEq, Eq`) in `src/bias.rs`; `pub fn source::resolve(value:
&str) -> Result<Source, String>` — `Err` names all three valid values,
matching `EngineError::Unknown`'s shape (`src/stt.rs:52-60`). `auto`
resolves to `Ok(Source::Auto)`, not an absence of a member (design §2a).

- [ ] Write failing tests: all three values resolve; an unrecognised value
  (e.g. `"vosk"`) is refused and the message names it plus all three valid
  values.
- [ ] Run: verify FAIL to compile (module doesn't exist).
- [ ] Implement `Source` and `resolve`; register `bias` in `src/main.rs`.
- [ ] Run: `cargo test --lib bias::` — PASS.
- [ ] Commit: `git add src/bias.rs src/bias/source.rs src/main.rs && git commit -m "Resolve [context] source into a checked Source, or refuse it"`

---

### Task 4: `bias::files::collect`

**Files:** Create `src/bias/files.rs`.

**Produces:** `pub fn files::collect(cwd: &str, max: usize) -> Vec<String>`
— file and directory names for the repository containing `cwd`,
newest-first, intermediate path components (not basenames alone), capped
at `max`; empty when `cwd` is outside a git repository (AC-4). Port
`spike/context.sh:57-62`'s two commands unchanged: `git status
--porcelain` from the repository root (`git rev-parse --show-toplevel`
against `cwd`), then `git log -30 --name-only`, deduplicated in order.

- [ ] Write failing tests, against a scratch repository created with real
  `git init`/`git commit`: a nested directory's component survives, not
  just the file's basename; outside a repository the result is empty;
  more entries than `max` are truncated to `max`.
- [ ] Run: verify FAIL.
- [ ] Implement.
- [ ] Run: `cargo test --lib bias::files::` — PASS.
- [ ] Commit: `git add src/bias/files.rs && git commit -m "Collect recently touched file and directory names for the bias string"`

---

### Task 5: `bias::pane::argv` and `bias::pane::read`

**Files:** Create `src/bias/pane.rs`.

**Produces:** `pub const PANE_LINES: usize = 80;` `pub fn pane::argv(pane:
&str, lines: usize) -> Vec<String>` — pure, the exact contract above. `pub
fn pane::read(pane: &str, lines: usize, binary: &str) -> Result<String,
PaneError>` — runs `binary` with `argv`'s arguments (minus the program
name); on success, trims trailing whitespace per line, drops lines with no
letter or digit, keeps the last `lines` lines, joins with `\n`; `PaneError`
distinguishes "not found" from "exited non-zero" (design §5). **`binary`
is an explicit parameter — this task does not resolve `HERDR_BIN_PATH`
itself**; that happens at the Task 10 call site, via #22's helper.

- [ ] Write failing tests: `argv("w1:p2", 80)` equals the exact contract
  above, element for element; a small script (write a `#!/bin/sh` file to
  a temp path, `chmod +x`) that prints mixed blank/content lines comes
  back filtered and line-capped; a nonexistent program yields `NotFound`;
  a script that `exit 3`s yields `Failed` naming the exit code.
- [ ] Run: verify FAIL.
- [ ] Implement.
- [ ] Run: `cargo test --lib bias::pane::` — PASS.
- [ ] Commit: `git add src/bias/pane.rs && git commit -m "Read a pane's screen contents through herdr, filtered"`

---

### Task 6: `bias::transcript::find` and `bias::transcript::read_turns`

**Files:** Create `src/bias/transcript.rs`.

**Produces:** a constant naming the one known agent (e.g.
`KNOWN_TRANSCRIPT_AGENT = "claude"`), deliberately separate from
`doctor::AGENT_CANDIDATES` (`src/doctor.rs:20` — same value today, answers
a different question, design §4). `pub fn transcript::find(cwd: &str,
agent: Option<&str>, root: &Path) -> Option<PathBuf>` — `None` when
`agent` isn't the known one; otherwise slugify `cwd` (`/`, `.`, `@` →
`-`), walk up to the first existing directory under `root`, return the
newest `.jsonl` there by mtime (design §3, no session-id lookup). `pub fn
transcript::read_turns(path: &Path, max: usize) -> Vec<String>` — parse
each JSONL line, keep `user`/`assistant` turns whose text (a string, or
the joined text of an array of `{"type":"text","text":...}` items) does
not start with a service-turn marker (list above), format `"{role}:
{text}"`, keep the last `max` (AC-1, AC-2).

- [ ] Write failing tests: a fixture `.jsonl` is found by directory; an
  agent other than the known one is never searched for (no directory
  touched); no project directory under `root` finds nothing; walking up
  finds the first existing directory; service turns are excluded from
  both count and content; only the last `max` filtered turns are kept; a
  missing or empty file yields no turns; array-shaped `content` is joined
  from its text items.
- [ ] Run: verify FAIL.
- [ ] Implement.
- [ ] Run: `cargo test --lib bias::transcript::` — PASS.
- [ ] Commit: `git add src/bias/transcript.rs && git commit -m "Find and read the target agent's transcript, filtered of service turns"`

---

### Task 7: `bias::Collected` and `bias::collect`

**Files:** Modify `src/bias.rs`.

**Consumes:** `Source` (3), `files::collect` (4), `pane::read`/`PANE_LINES`
(5), `transcript::find`/`read_turns` (6).

**Produces:**

```
pub struct Collected {
    bias: String, attempted: Vec<(Source, bool)>, file_count: usize,
    file_chars: usize, conversation_chars: usize, truncated: bool,
}
pub struct CollectInput<'a> {
    source: Source, cwd: &'a str, agent: Option<&'a str>, pane: &'a str,
    transcript_root: &'a Path, herdr_binary: &'a str,
    conversation_turns: usize, file_names: usize, prompt_chars: usize,
}
pub fn collect(input: CollectInput) -> Collected
```

Dispatch exactly as design §2: `Transcript` calls `transcript::find` only
(herdr never called); `Pane` calls `pane::read` only (transcript never
sought); `Auto` calls `transcript::find`, then `pane::read` only if that
came up empty. `files::collect` always runs, independent of `source`.
`attempted` entries are only ever `Transcript`/`Pane` — never `Auto`
(design §2a). Assembly: file-names line, then a newline, then conversation
turns joined by `\n`, hard-truncated to `prompt_chars` characters
(design §7); `truncated` is true iff the pre-cut length exceeded it.

**`file_chars`/`conversation_chars` — beyond `DESIGN_21.md` §2a's literal
fields.** At the defaults, a busy repository's `file_names = 40` list can
run to roughly 800 characters while `prompt_chars = 600` caps the whole
string — the conversation component can be cut away entirely, every take,
and a bare `truncated: bool` would not tell a log reader that it was
specifically the conversation that lost the cut. These two counts make
that visible without logging content. Flag this explicitly at the Task 11
(S4) review for the reviewer to judge; do not change either default and
do not invent a reserved conversation budget.

- [ ] Write failing tests, using a scratch transcript root and a
  nonexistent `herdr_binary` (so `pane::read` reliably misses without a
  live herdr): `Transcript` source produces `attempted == [(Transcript,
  false)]` on a miss; `Pane` source produces `[(Pane, false)]`; `Auto`
  tries both on a double miss; `Auto` does not try the pane when the
  transcript hits, and the bias contains the fixture's text; a miss on
  every source still yields a files-only bias from a scratch git
  repository; a conversation long enough to exceed a small `prompt_chars`
  sets `truncated` and leaves `conversation_chars` greater than the cap; a
  short result leaves `truncated` false.
- [ ] Run: verify FAIL.
- [ ] Implement `collect`.
- [ ] Run: `cargo test --lib bias::` — PASS, all submodules.
- [ ] Run: `cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt --check`.
  If clippy flags `collect` as unused (no caller until Task 10), add
  `#[allow(dead_code)]` with a one-line comment pointing at Task 10 — the
  same pattern `IMPLEMENTED` in `src/main.rs:107` already uses for the
  same reason.
- [ ] Commit: `git add src/bias.rs && git commit -m "Assemble and cap the bias string from files and conversation"`

---

### Task 8 — BLOCKED on #22: resolve `[context] source` once, at daemon start

**Files:** Modify `src/daemon.rs`.

**Before starting:** read the merged `src/daemon.rs`; confirm Runtime's
real name and whether it is built in `start()` the way `Recognition` is
today (pre-#22 `src/daemon.rs:175-178`), and whether
`config::Vars::from_env().home` already reaches that construction site.

**Produces:** Runtime gains a field holding `Result<bias::Source, String>`
via `bias::source::resolve(&loaded.config.context.source)`, resolved once
— the same shape `Recognition` already has (design §2a, "why resolved
once, not per take") — and a field holding the transcript root
(`<home>/.claude/projects`, built the way `config::directory` reads
`home`, `src/config.rs:117-121`; absent when `home` is `None`, design §3,
"`home` is `Option<String>`").

- [ ] Write a failing test on Runtime's construction: `source = "pane"`
  resolves to `Ok(Source::Pane)`; `source = "vosk"` is `Err` naming
  `"vosk"`. Write it against the real Runtime type found above.
- [ ] Run: verify FAIL.
- [ ] Add both fields and their construction in `start()`.
- [ ] Run: `cargo test --lib daemon::` — PASS.
- [ ] Commit: `git add src/daemon.rs && git commit -m "Resolve [context] source and the transcript root once, at daemon start"`

---

### Task 9 — BLOCKED on #22, depends on Task 8: thread `cwd`/agent into `dictate`

**Files:** Modify `src/daemon.rs`.

**Before starting:** confirm `answer` still parses `Invocation` before
calling `dictate` (pre-#22 `src/daemon.rs:59-72`; fields at
`src/context.rs:12-14`), and `dictate`'s real parameter list post-#22.

**Produces:** `dictate` gains `cwd: Option<&str>`, `agent: Option<&str>`,
read from the already-parsed `Invocation` in `answer` — not re-parsed
(design §9, "That call needs the working directory and the agent name...
a signature change inside one file, not a design decision").

- [ ] Widen `dictate`'s signature and its call in `answer`; update every
  test call site in `src/daemon.rs`'s test module.
- [ ] Run: `cargo test --lib daemon::` — PASS (existing dispatch tests
  unaffected by the wider signature).
- [ ] Commit: `git add src/daemon.rs && git commit -m "Thread the working directory and the agent name into dictate"`

---

### Task 10 — BLOCKED on #22, depends on Tasks 7-9: wire `bias::collect` into the take path

**Files:** Modify `src/daemon.rs`.

**Before starting:** confirm the name/signature of #22's helper for
running the `herdr` binary through `HERDR_BIN_PATH` (extracted from
pre-#22 `doctor::herdr_finding`, `src/doctor.rs:105-112`). Use it here —
do not read `HERDR_BIN_PATH` a second time in `src/daemon.rs`.

**Produces:** in `dictate`, when Runtime's resolved source is `Ok`, build
a `bias::CollectInput` from it, `dictate`'s new `cwd`/`agent`, the pane,
the transcript root, and `loaded.config.context`'s three numeric keys;
call `bias::collect`; write one line to the existing per-request stderr
channel (pre-#22 `src/daemon.rs:141-150`) naming `attempted`,
`file_count`, `file_chars`, `conversation_chars`, `prompt_chars`,
`truncated` — **never `bias`**. When the resolved source is `Err`, call
`bias::files::collect` alone, log the configuration error once, and treat
the conversation component as empty (design §2a, "the refusal's effect on
the take"; §8, uniform miss handling).

- [ ] Write failing tests: a fixture transcript containing an obviously
  distinctive sentence reaches the log line only as counts — assert the
  line contains `attempted`/`file_chars`/`conversation_chars` and does
  **not** contain the fixture sentence (the AC-9 proof design §11 asks
  for); an unresolved `source` logs the configuration error by name and
  still yields a files-only bias from a scratch repository; a miss on
  every source still lets the take succeed (reply is `Ok`, not `Error`).
- [ ] Run: verify FAIL.
- [ ] Implement the wiring.
- [ ] Run: `cargo test --lib daemon::` — PASS.
- [ ] Run: `cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt --check && python3 scripts/check_manifest.py`.
- [ ] Commit: `git add src/daemon.rs && git commit -m "Bias each take's recognition, logging only metadata about the result"`

---

### Task 11: S4 — review the diff before a pull request exists

Invoke `superpowers:requesting-code-review` against the full diff on
`feat/21-context` since it diverged from `main`, naming this plan and
`DESIGN_21.md` as what it should satisfy. Explicitly ask the reviewer to
judge Task 7's `file_chars`/`conversation_chars` addition — it goes beyond
`DESIGN_21.md`'s literal `Collected` fields; decide whether it belongs
there as written.

- [ ] Run the review.
- [ ] Append a `## Gate S4` block to `tasks/21/RUN_21.md` with the verdict
  (`READY`/`QUESTIONS`/`BLOCKED`) and any questions; if not `READY`,
  address them and re-run before proceeding — S4 is never skipped
  (`CLAUDE.md`).
- [ ] Commit: `git add tasks/21/RUN_21.md && git commit -m "Record the S4 diff review for context"`

---

### Task 12: S5 — verify and write `docs/evidence.md`

- [ ] Run `cargo test`, reading the whole output; then `cargo clippy
  --all-targets -- -D warnings`, `cargo fmt --check`,
  `python3 scripts/check_manifest.py`.
- [ ] Append a section to `docs/evidence.md`: the platform, the test
  count, and what it establishes — the bias string is assembled per
  `[context] source`, capped at `prompt_chars`, and the log carries only
  metadata, proven by a negative assertion against fixture text. State
  plainly what it does **not** establish: whether the string changes a
  real recognition output — `Engine::transcribe` is untouched by this
  issue (design §9), so there is no consumer to run a spoken take
  against; that belongs to #26.
- [ ] Append a `## Gate S5` block to `tasks/21/RUN_21.md` with the
  pass/fail verdict.
- [ ] Commit: `git add docs/evidence.md tasks/21/RUN_21.md && git commit -m "Verify the context bias string by test suite, and record what #26 still owes"`

---

## Coverage

AC-1/AC-2 → Task 6, 7. AC-3 → Task 7. AC-4 → Task 4. AC-5 → Task 7. AC-6 →
Task 7 (`collect` is `pub` and called from Task 10; `Engine::transcribe`
untouched throughout). AC-7 → Task 2. AC-8 → Tasks 4-7 (every path returns
`Option`/`Result`/an empty `Vec`, never panics). AC-9 → Task 7's counts-only
`Collected`, Task 10's log line and its negative-content test.

## What could not be cut into a checkable task

`Engine::transcribe`'s widened shape is left to #26 by `DESIGN_21.md` §9,
which declines even a non-binding sketch of it — there is nothing here to
check against. Tasks 8-10 are necessarily less precise than 1-7: they edit
`src/daemon.rs`'s post-#22 Runtime, which does not exist in this worktree,
so each names what to verify against the merged code rather than a
signature this plan cannot see.
