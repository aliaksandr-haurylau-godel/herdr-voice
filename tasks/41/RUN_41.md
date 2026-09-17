# RUN_41

| field | value |
|---|---|
| issue | #41 — setup answers "not implemented yet", so the keybindings must be written by hand |
| input | GitHub issue, read with `gh issue view 41`; no comments on it |
| stage | S4 |
| branch | feat/41-setup |
| opened | 2026-09-16 |

## Stages

### S1 Assess
- artifact: `AC_41.md`
- produced: 2026-09-16

```yaml
gate:
  stage: S1
  artifact: AC_41.md
  reviewer: designer
  verdict: QUESTIONS
  round: 1
  date: 2026-09-16
  questions:
    - "AC-4 requires that invoking `setup` opens a visible popup pane, but nothing in the AC names the mechanism by which that happens, and the as-is section says the opposite about the only path it establishes: a `global` action has no terminal and its output reaches only `herdr plugin log list` (AC_41.md:40-47). I cannot choose between the two designs this allows. Reading one: the manifest keeps the `setup` action (herdr-plugin.toml:48-53) and the binary, when run as that action, asks herdr to open a declared `[[panes]] id = \"setup\"` — that needs a herdr capability (a command or protocol by which a running plugin process opens a declared pane) which no fact in the AC establishes and which I cannot assume. Reading two: the pane is the entry point on its own, the action becomes a thin or removed thing, and the person reaches setup by a binding or a launcher — but AC-1 fixes the snippet at exactly three blocks (`ptt`, `dictate`, `cancel`), so under this reading nothing in the AC says how a freshly installed plugin's setup pane is reached at all. The two readings differ in what the binary does under `setup`, what goes into the manifest, and whether a fourth binding exists; I cannot write DESIGN_41.md without the answer."
  blocker: null
```

The reviewer also noted, without withholding READY for it, that `docs/design.md:61-62`
already lists `setup` among the manifest panes, which supports the pane route but
does not state how the pane is opened.

## Decisions taken with the owner, 2026-09-16

**Which keys the snippet names.** The issue puts this outside the stage's bounds
and names it the owner's. He chose three bindings: `ptt` on `ctrl+g`, `dictate` on
`prefix+i`, `cancel` on `ctrl+shift+g`. Two earlier candidates were withdrawn on
evidence: `prefix+g`, which is herdr's default for `goto` and is already bound to
another plugin's action on the development machine, and `prefix+v`, which is
herdr's default for `split_vertical`. `ctrl+g` is the chord that was proven to
reach the binding on 2026-09-10, after `alt+v` did nothing and `alt+g` typed `©`.

**Where the offer is made.** A `global` action has no terminal: invoking
`haurylau.voice.setup` through herdr showed the caller nothing, and the plugin's
own output surfaced only in `herdr plugin log list`. The owner chose a popup pane
declared in the manifest over an interactive terminal prompt and over a
`--write` flag with no question at all.

## Notes

- The worktree is `wt-41`, cut from `main` at `c64697c`.
- `HERDR_CONFIG_PATH` is what makes the tests the issue asks for possible: it
  overrides the configuration file's path, so a test can point herdr's
  configuration at a temporary file.
- `herdr config check` validates a configuration file and prints diagnostics; it
  is the check AC-3 rests on.

## S1 round 1 — how the question was answered

The reviewer could not choose between two designs because nothing established
that a running plugin process can get a pane onto the screen. It was answered by
running herdr, not by picking a reading.

`herdr plugin pane open --plugin haurylau.voice --entrypoint status` answered
`{"id":"cli:plugin","result":{"type":"ok"}}`, so a manifest-declared pane is
opened on request by anything that can run herdr. The pane's process left no
record in `herdr plugin log list`, unlike an action, which is how a pane differs:
it runs in a terminal of its own. And `herdr plugin action list` and
`herdr plugin action invoke` are command-line commands with no counterpart inside
herdr's own interface, so on a machine with no key bound yet, the command line is
the only reach.

Both facts went into the as-is section. AC-4 was restated to fix the outcome — a
person who invokes `setup` and never opens a terminal gets the snippet and the
question — while leaving to the design which process does which half. AC-8 was
widened at the same time to cover a failure that happens before any pane exists,
where `herdr plugin log list` is the only surface left.

```yaml
gate:
  stage: S1
  artifact: AC_41.md
  reviewer: designer
  verdict: READY
  round: 2
  date: 2026-09-17
  questions: []
  blocker: null
```

Two notes the reviewer attached without withholding READY, both for the design to
carry: AC-3 can only be satisfied against a real herdr, so it becomes a by-hand
verification written into `docs/evidence.md`; and `herdr-plugin.toml:55-77`
declares the panes `status`, `model` and `mic` but no `setup` pane, while
`docs/design.md:61` already lists one — the design adds it.

### S2 Design
- artifact: `DESIGN_41.md`
- produced: 2026-09-17

The design was settled against a live herdr rather than against the documentation.
A throwaway plugin was linked, opened as a popup pane, inspected with `lsof` and
unlinked again; the machine was left with the five plugins it had before. What that
established, and what it changed:

- A pane's process holds a real terminal on all three descriptors, so the offer can
  be asked and answered there. This is what the whole shape rests on.
- Only one popup exists at a time; a second open is refused with `ui_busy`. So the
  entry point needs a way to report a refusal, which became the toast in section 2.
- herdr itself reports two commands bound to one key, but reports nothing when a
  `[[keys.command]]` shadows one of its own built-in defaults. So the collision
  check is ours for the file's contents, and blind beyond them — recorded in
  section 6 rather than smoothed over.
- Appending after a configuration's last section is accepted, which is why the
  writer appends text instead of rewriting a parsed document and losing comments.

One earlier statement of mine was wrong and is corrected in the appendix: `[ -t 1 ]`
inside a `$( )` substitution reported that a pane has no terminal on standard
output. The substitution replaces that descriptor with a pipe; the test was
measuring itself.

```yaml
gate:
  stage: S2
  artifact: DESIGN_41.md
  reviewer: planner
  verdict: QUESTIONS
  round: 1
  date: 2026-09-17
  questions:
    - "The seam for the two new herdr calls is named but has no symbol I can point a task at. Section 2 and section 10 say the pane-open call (`herdr plugin pane open --plugin haurylau.voice --entrypoint setup`) and the `herdr config check` call 'go through the recorded-call seam src/delivery.rs already uses', but nothing in src/delivery.rs can carry them: `trait Deliverer` (src/delivery.rs:30-34) has exactly `insert`, `submit`, `notify`; `HerdrDeliverer::run` (src/delivery.rs:182) is private; and the recorder script that records argv lives in the private `mod tests` (src/delivery.rs:357-452), so src/setup.rs tests cannot reach it — only `tests_support::FakeDeliverer` with its three `Call` variants is `pub` (src/delivery.rs:63-73). The design does not say whether `Deliverer` grows two methods (which changes `FakeDeliverer`, `HerdrDeliverer` and every `Box<dyn Deliverer>` construction, for example src/daemon.rs:102 and src/daemon.rs:1186), or whether src/setup.rs gets its own runner and its own fake. Until that is stated I cannot split 'extend the herdr-call seam' from 'implement setup' as two tasks with an interface between them, and I cannot say which task creates the symbol the setup tests name."
    - "Section 5 makes the write conditional on 'a clean result' from `herdr config check`, and says a reported issue is 'shown verbatim', but nothing states how clean is distinguished from not clean or where the verbatim text comes from. The appendix records the output strings (`config: ok`, `config: issues found` plus the per-key line) and not an exit code; the existing runner returns `Ok(())` on a successful exit status and discards stdout, and on failure returns only `extract_reason` — a JSON `error.code` or the trimmed text (src/delivery.rs:145-160, src/delivery.rs:190-198). So a task implementing the validate-then-rename step has to decide on its own whether the verdict is read from the exit status or from the output text, and what the fake returns in the 'herdr rejected the candidate' test. That decision sets the interface of the seam in the previous question, so it also decides whether the two tasks can be written independently."
    - "AC-8 and AC-12 require a case where the configuration file cannot be written, but section 5's scheme — write a temporary file beside the real one, then rename over the original — has no stated rule for detecting that case, and under it the obvious fixture does not trigger a failure: renaming over a read-only regular file succeeds when the parent directory is writable, so the run would report success where the criterion demands a named file, a named reason and exit 1. The design does not say whether unwritability is established by a pre-check on the target, by the temporary write failing, by the rename failing, or by a read-only parent directory. Without that I cannot write the done-criterion for the unwritable-file test task — I would have to invent both the fixture and the expected behaviour."
  blocker: null
```

All three were answered without going to the owner, and two of them changed the
design rather than only explaining it.

**The seam.** `Deliverer` is about putting text into a pane; opening a pane and
validating a file are not that job, and adding them would change the trait, its
public fake and every construction of it in the daemon. So `src/setup.rs` brings
its own trait with `open_pane`, `check_config` and `notify`, its own recording
double, and reuses only the public `delivery::herdr_binary()`. The one thing
shared rather than copied is the argument builder for
`herdr notification show`, promoted to `pub(crate)` so that command has one
definition. Written as section 12.

**How clean is decided.** Measured rather than chosen: `herdr config check` exits
`0` with `config: ok` and `1` with `config: issues found`, the reasons on standard
output. The run reads the status and shows the output; it parses neither.

The measurement also found something the design had wrong. An unrelated defect in
the user's own configuration — an unknown section, a parse error — is also exit
`1`, so checking only the candidate would have reported somebody else's breakage
as this action's failure. The original is now checked first, and when herdr
already reports issues with it nothing is written: a configuration herdr is
ignoring, wholly or in part, would swallow an appended binding silently.

**What "cannot be written" means.** The reviewer was right that the obvious
fixture proves nothing: renaming over a file whose write permission is removed
succeeds while its parent directory is writable. There is no pre-check — a
permission answered before the write is a different question from the write.
The failure is whichever step actually fails, creation, write or rename, reported
with its path and the reason the system gave, and the test fixture is a parent
directory that cannot be written. The temporary file also takes the original's
permissions before the rename, so replacing a file does not change its mode.

```yaml
gate:
  stage: S2
  artifact: DESIGN_41.md
  reviewer: planner
  verdict: READY
  round: 2
  date: 2026-09-17
  questions: []
  blocker: null
```

Two loose ends the reviewer named without withholding READY were closed in the
artifact rather than left for the implementer: section 3 now lists all four fields
of a block, `description` among them, and section 9 names the refused-pane case
among the ones that exit 1.

One forced edit the reviewer found that the design does not name, and the plan
must: `src/main.rs:229-234` holds a test asserting that `setup` is *not*
implemented. Routing the subcommand breaks it, so that test changes alongside the
`IMPLEMENTED` list at `src/main.rs:119` and the usage text.

### S3 Plan
- artifact: `PLAN_41.md`
- produced: 2026-09-17

```yaml
gate:
  stage: S3
  artifact: PLAN_41.md
  reviewer: implementer
  verdict: READY
  round: 1
  date: 2026-09-17
  questions: []
  blocker: null
```

One defect was found in the plan before the gate, by its author: `BINDINGS` was
declared `const`, while `decide` hands out `&'static Binding`. A `const` is
inlined at every use site, so those references would point into a temporary and
nothing could hold them. It is `static` now, with the reason written beside it.

One citation the reviewer found loose and which was corrected rather than left:
`scripts/check_manifest.py` reads the subcommands with a whole-file regular
expression, not from a fixed line of `src/main.rs`. What the plan asserts about
it is true; where it said to look was not.

### S4 Implement
- artifact: code, commits `4126485..2178e28` on `feat/41-setup`
- produced: 2026-09-17

The four gates were run fresh by the orchestrator on the finished state, not taken
on the implementer's word: 473 tests pass, clippy clean with `-D warnings`,
`cargo fmt --check` silent, `scripts/check_manifest.py` reports 12 entries.

## The mutation review, and what it found

A separate agent, in a throwaway worktree, deleted or inverted the production code
behind each thing a test claims to prove. Nineteen mutations: nine died, **ten
survived**, and three more defects were found by reading. The pattern is one line:
everything above the seam onto herdr is tested carefully and the mutations bounce
off it; everything at or below the seam — `HerdrCli`, `setup::main()`, the
`main.rs` arm, the manifest's pane declaration — had no test at all, and that is
where four of the five worst findings sit. The suite proved that the reasoning is
right and never that the binary reaches it.

Two of the findings were verified independently before anything was acted on,
because a reviewer's report is not evidence:

- Renaming the manifest's pane from `id = "setup"` to anything else leaves the
  suite green and `scripts/check_manifest.py` satisfied — it validates the
  `command` of every entry and never an `id`. The only path by which the offer
  reaches a person would be dead, silently.
- A rename over a symbolic link replaces the link. Checked directly with a link
  and a rename: afterwards the path is a regular file and the file it pointed at
  still holds the old content.

Three findings changed behaviour rather than only test coverage, and the design
was amended to state the new behaviour: a symbolic link is now followed to its
target instead of being replaced; a key held by a herdr action is reported with
that action's name rather than with the category; and a `[[keys.command]]` block
that carries a key but is missing its command still reserves that key.

One departure from `CLAUDE.md` is taken deliberately and written where the next
reader meets it. That file says unit tests live next to the code. Two claims are
about the binary as a process — that a terminal on standard input is what chooses
the interactive branch, and that the exit code reaches the caller — and a unit
test cannot reach either. They are covered by `tests/setup_process.rs`, which runs
the built binary against a recorder script.

## A gap in the leak gate, outside this issue

`.leakwords` is never committed, so a freshly cut worktree has none, and
`.githooks/pre-commit` skips its second check with `if [ -f "$root/.leakwords" ]`
and no `else`. The commit proceeds looking exactly as it does when the check ran.
This worktree had no `.leakwords` for all eight commits; the branch was rescanned
afterwards with the list in place and with `gitleaks` over the commit range, and
both were clean. The hook itself is not this issue's to change.

### S5 Verify
- artifact: `docs/evidence.md`, section "`setup`, by hand on macOS"
- produced: 2026-09-17

The three criteria no test can reach were checked against a live herdr, with the
plugin temporarily linked from this checkout and returned to the owner's own
afterwards. The by-hand check found two defects that everything else had passed
over, both of the same shape: a refusal that looks like success.

The question never reached the screen. It is written without a newline of its own
and standard output is line buffered, so it sat in the buffer while the process
blocked on the answer; the person saw a cursor on an empty line. 492 tests, four
gate rounds and a mutation review had all passed over it, and none of them could
have seen it, because every one writes into a buffer that needs no flushing.

Then the answer was pressed twice and nothing was written either time. The answer
is read a line at a time, and the terminal echoes the keystroke — so the screen
showed the answer given and the run standing still. Confirmed by sending `y` with
no newline over a pseudoterminal: the process stays alive and the file is
untouched. The question now says `then Enter`.

Both are recorded in `docs/evidence.md` with what established them, alongside the
measurements of what herdr accepts.
