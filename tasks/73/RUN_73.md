# RUN_73

| field | value |
|---|---|
| issue | #73 — the plugin id carries a surname: `haurylau.voice` becomes `herdr-voice` |
| input | GitHub issue, read with `gh issue view 73`; no comments on it |
| stage | S1 |
| branch | feat/73-plugin-id |
| opened | 2026-09-18 |

## Stages

### S1 Assess
- artifact: `AC_73.md`
- produced: 2026-09-18

## Notes

Every stage runs: the id is what a user types and reads, so this changes
behaviour and nothing is skipped.

Facts established by running rather than by reading, against herdr 0.9.1 on
macOS, with a throwaway plugin of this run's own that was unlinked afterwards and
left no registry entry, no configuration directory and no state directory behind:

1. A dotless plugin id is accepted — linked, listed, and `plugin config-dir`
   answers for it.
2. Relinking one checkout under a changed id leaves the old registry entry in
   `plugins.json`. `herdr plugin list` shows one entry per checkout, so the old
   one is invisible until the new one is unlinked, at which point it reappears
   marked `1 warning(s)`. `herdr plugin unlink <old id>` removes it.
3. herdr creates a plugin's configuration directory when the plugin is linked
   and does not remove it when it is unlinked.
4. The running daemon has parent process 1 — reparented to the init process, not
   a child of the herdr server — so no herdr restart ends a daemon started under
   the old id.

The owner's live setup was not touched: nothing was linked from this worktree, no
daemon was restarted, and no plugin action was invoked.

## S1 gate, round 1

```yaml
gate:
  stage: S1
  artifact: AC_73.md
  reviewer: designer
  verdict: QUESTIONS
  round: 1
  date: 2026-09-18
  questions:
    - "AC-5 and AC-7 contradict each other on the old id string. AC-5 requires `setup` to detect bindings whose commands are `haurylau.voice.*`, name the keys and the file, and point at the old configuration directory, with a test asserting on that output — all of which needs the literal `haurylau.voice` (or `haurylau` plus `.voice`) in `src/` and in tests. AC-7 requires that `grep -rn \"haurylau\\.voice\" . --exclude-dir=target --exclude-dir=tasks --exclude-dir=.git --exclude=evidence.md` print nothing, and `src/` and `tests/` are not among its two exceptions. I cannot design the detection until the AC says which is true: either the old id is kept in one named place (a `LEGACY_PLUGIN_ID` constant, with AC-7's exception list extended to it) or the notice must not name the old id at all, which changes what AC-5's three sentences can say. The two readings are materially different designs."
    - "AC-6 does not say what `setup` may do to the old `[[keys.command]]` blocks, and appending alone cannot satisfy it. The as-is states `setup` only ever appends, and the append is gated on `herdr config check` of the candidate file (`src/setup.rs:417-488`); two `[[keys.command]]` blocks on one key are rejected by herdr as `config: issues found` with the later one disabled (measured in `docs/evidence.md:1009-1012` and `tasks/41/DESIGN_41.md:298`). So appending `ctrl+g` while a block naming `haurylau.voice.ptt` holds `ctrl+g` makes the whole candidate fail and nothing is added. Meeting AC-6 therefore needs a capability no requirement grants — rewriting, removing or commenting out the user's existing blocks — and AC-5 asks only that the user be told about them. I need the AC to state whether `setup` modifies the old blocks in the user's file, or whether AC-6's outcome is something else (for example the old blocks being reported as this plugin's own predecessors and left in place, with the new bindings not added)."
  blocker: null
```

The reviewer also noted, without withholding READY for it, that AC-5's second item and
both "out of scope / noticed" items are designable as handed over, and that AC-9 needs
no new code: the token lapses through its three-renewal time to live, so verifying it is
a wait-and-observe.

### How the questions were answered

Both were answered by the author rather than escalated.

**Question 1 — the old id string.** The old id is kept, in exactly one place:
`LEGACY_PLUGIN_ID` beside `PLUGIN_ID` in `src/transport.rs`. This follows the rule the
file already states for the current id — "two copies of the id could drift apart"
(`src/setup.rs:5-9`) — and the detection cannot exist without the string. AC-7's
exception list is extended to that constant and to the tests that assert on the notice.

**Question 2 — what `setup` may do to the old blocks.** The reviewer's mechanism is off
by one step and its conclusion holds. `decide` classifies a key held by any other
command as blocked (`src/setup.rs:166-183`), so `ctrl+g` would never reach the candidate
file and the append would not fail — `setup` would instead report `ctrl+g` as "already
held by `haurylau.voice.ptt`" and add nothing. Either way the three bindings do not land
on their keys. The answer is that `setup` recognises such a block as this plugin's own
predecessor and offers to rewrite those blocks in place, under the same question and the
same machinery the append already uses. The reason is in the next section.

### Decision: `setup` rewrites a predecessor's binding blocks in place, on consent

**Context.** Three `[[keys.command]]` blocks in the user's herdr configuration name
`haurylau.voice.ptt`, `haurylau.voice.dictate` and `haurylau.voice.cancel`. After the
rename those commands address a plugin id herdr does not know, and `setup` today can
only append.

**Problem.** Appending cannot put the new bindings on those keys. `decide` reports each
of the three keys as held by another command and offers nothing, so a person who runs
`setup` is told that something else holds `ctrl+g` — which is this plugin's own past —
and is left with three keys that do nothing. Telling them to delete three blocks by hand
and run `setup` again satisfies the ticket's letter and leaves the repair manual.

**Decision.** `setup` recognises a `[[keys.command]]` block whose command is
`<LEGACY_PLUGIN_ID>.<action>` as its own predecessor, reports it as such, and offers —
with the same `[y/N]` question and the same `herdr config check`, candidate-file and
atomic-rename machinery the append already uses — to rewrite those blocks so they name
the new id. Nothing is rewritten without the answer.

**Why.** The machinery that makes an edit safe already exists and is already tested:
herdr judges the candidate before anything moves, the move is one rename, a symbolic
link is resolved first and the original's permissions are carried over
(`src/setup.rs:404-487`). The orchestrating brief leaves the migration to this design,
and of the two designs it allows this is the one that does not end with the person
editing TOML by hand to recover keys the rename took from them.

## S1 gate, round 2

```yaml
gate:
  stage: S1
  artifact: AC_73.md
  reviewer: designer
  verdict: READY
  round: 2
  date: 2026-09-18
  questions: []
  blocker: null
```

The reviewer flagged, without withholding READY, that AC-7 asks the notice to name
"the one thing that ends" a daemon started under the old id, and the AC establishes
only what does not end it. The remedy is to be established and recorded in S2.

### After the gate, before S2

Two changes to `AC_73.md` that add no requirement:

- The "out of scope / noticed" section called the migration notice AC-5 in two
  places where it is AC-7. Corrected.
- AC-6 said "every other block and every other line of the file is what it was".
  It now says what that means for the case the file actually has: a comment sits
  above the `ctrl+g` block on the machine this runs on, explaining why that key
  was chosen, and a rewrite that removed the block and appended a replacement
  would leave the comment describing a binding that is no longer under it. AC-6
  now requires the fixture to carry such a comment.

`a_commented_hand_written_file_keeps_every_byte_it_had` (`src/setup.rs:1047`) is
not replaced by any of this: it tests `append`, which does not change. The rewrite
is a second operation and gets a test of its own with the same premise.

## S2 Design
- artifact: `DESIGN_73.md`
- produced: 2026-09-18

## S2 gate, round 1

```yaml
gate:
  stage: S2
  artifact: DESIGN_73.md
  reviewer: planner
  verdict: QUESTIONS
  round: 1
  date: 2026-09-18
  questions:
    - "Section 1 defines the legacy paths as `state_directory_of(vars, id)` / `config::directory_of(vars, id)` — 'the same code with the other constant'. But in both functions the first branch returns the override verbatim and the id never appears in it: `config::directory` returns `PathBuf::from(HERDR_PLUGIN_CONFIG_DIR)` (src/config.rs:250-253) and `state_directory` returns `PathBuf::from(HERDR_PLUGIN_STATE_DIR)` (src/transport.rs:114-117). herdr sets both variables when it runs a plugin action (tasks/3/DESIGN_3.md:28, :171), which is how `setup` normally runs. Under those variables `directory_of(vars, LEGACY_PLUGIN_ID)` is the directory the plugin reads today, and `address_of(vars, LEGACY_PLUGIN_ID)` is the live daemon's own socket — so section 5's two checks would report the current configuration file as the orphaned one and tell the person to `kill` the daemon they are running. The design does not say whether `_of` ignores the id in the override branch (and the false notices are accepted), or whether the legacy lookup skips the override and derives from XDG/HOME only. I cannot write the derivation task's done-criterion, nor the tests 'the report names the old configuration file' and 'says nothing about a daemon when nothing answers', without picking one."
    - "AC-7 requires the notice to name both where the old configuration file is and 'where the plugin now looks', and section 5 repeats that `setup` 'names the directory the plugin now reads'. The `Legacy` struct carries only `config_file` and `daemon_socket`, and section 5 states the rule that `run` takes such values rather than reading the environment (src/setup.rs:71-75), so `run` has no way to obtain the current configuration directory. The design does not say whether `Legacy` gains a third field, whether the sentence is built from something else, or where that value comes from. The task that builds `Legacy` in `setup::main` and the task that writes the report in `run` cannot be written independently until the struct is fixed."
  blocker: null
```

The reviewer also noted, without withholding READY for them: the sample report in
section 2 names the key but not the file AC-6 requires; section 2 does not mention
the early return at `src/setup.rs:576-590`, which a non-empty `superseded` must
change; and the daemon probe needs a named function, which is an implementation
choice the plan can take.

### How the questions were answered

**Question 1 — the override branch. The reviewer found a real defect, and the
measurement confirms it.** Established by running, with a throwaway plugin of this
run's own, linked and unlinked and cleaned up afterwards: invoking a plugin action
through herdr 0.9.1 gives the process
`HERDR_PLUGIN_CONFIG_DIR=<config>/herdr/plugins/config/<that plugin's id>` and
`HERDR_PLUGIN_STATE_DIR=<state>/herdr/plugins/<that plugin's id>`, both ending in
the id herdr knows the plugin by, which after this change is the new one. Passing
the legacy id to a function whose first branch returns the override verbatim would
have produced exactly the two false notices the reviewer describes.

The answer is that the legacy path is derived from the current one rather than
computed a second time, and the design now says so: when the directory herdr names
ends in `PLUGIN_ID`, the legacy directory is its sibling under `LEGACY_PLUGIN_ID`;
when it does not end in `PLUGIN_ID`, herdr has named something this plugin cannot
reason about and nothing is reported. The property that makes the whole class of
false notice impossible — the legacy path can never equal the current one, because
the two constants differ and the last component is the only thing that changes —
is stated as an invariant with a test of its own.

**Question 2 — the third field.** `Legacy` gains `current_config_dir`, so the
sentence AC-7 requires can be written from what `run` is given.

## S2 gate, round 2

```yaml
gate:
  stage: S2
  artifact: DESIGN_73.md
  reviewer: planner
  verdict: READY
  round: 2
  date: 2026-09-18
  questions: []
  blocker: null
```

Four notes, none of which withheld READY: the legacy report is placed after the
binding report on every interactive path, including the one where nothing is to be
added or rewritten; a configuration carrying both a legacy and a current block for
one action gets the legacy one rewritten, leaving two keys on the same command;
the command that moves the old configuration file is `mv`, covered by a test on
unix; and the daemon probe is a named function over the legacy state directory,
which the plan chooses. The second of these is now stated in the design itself
rather than left to be discovered.

## S3 Plan
- artifact: `PLAN_73.md`
- produced: 2026-09-18

## The escalated question, and its answer

**Asked at S2:** who tells the person to run `setup`? As the acceptance criteria
stood, every sentence was in `setup`, and nothing prompted a person whose keys had
gone dead to run it.

**Answered:** the daemon raises one notice at start. The basis is a rule already
written down rather than anyone's preference — `CLAUDE.md`, under the rules for
the code: "Every user-visible failure names what to do next. 'Silent failure' is
a defect of the same weight as a wrong transcript." Three keys that do nothing and
say nothing is that failure, and an explanation a person could find by guessing
which command holds it does not make it not silent.

Because it is a criterion and not a design choice, it went back through S1 as
round 3 rather than into the design directly, with three constraints stated with
it: once per daemon start and never per take or per keypress; it says how many
keys and which; and a notice that cannot be raised is recorded through the path
`src/daemon.rs:691-699` already uses rather than through a second one of its own.

## S1 gate, round 3 (AC-11 only)

```yaml
gate:
  stage: S1
  artifact: AC_73.md
  reviewer: designer
  verdict: READY
  round: 3
  date: 2026-09-18
  questions: []
  blocker: null
```

Two notes, neither withholding READY. The first — whether a notice suppressed by
`[ui] toasts` is still recorded — the reviewer settled from the rule the codebase
already states at `src/daemon.rs:688-690`, and the design now states it.

The second said the daemon starts when an action runs, so a person whose only
entry points are the dead keys never causes one. That is the wrong way round, and
the record says so: herdr starts the daemon from the manifest's `[[startup]]`
entry when the server comes up (`docs/evidence.md:270-273`, measured on Linux with
the plugin linked before the server), and an action does not start it — the client
tells a person to "restart herdr so the plugin's startup entry does"
(`src/client.rs:70-71`). So the notice does reach a person who never runs `setup`:
it arrives the next time herdr starts, which is also the moment herdr first knows
the plugin by its new id.

## S2 gate, round 3 (section 5a only)

```yaml
gate:
  stage: S2
  artifact: DESIGN_73.md
  reviewer: planner
  verdict: QUESTIONS
  round: 3
  date: 2026-09-18
  questions:
    - "Section 5a's notice text and AC-11 disagree on what the body says, and the disagreement is the whole deliverable of the section. AC-11 requires the notice to say `how many such keys there are, which keys they are, that the plugin's id changed, and that setup repairs them`; the body quoted in 5a (`ctrl+g, prefix+i and ctrl+shift+g still name haurylau.voice, which no longer exists. Run the setup action to repair them.`) carries no number, and section 8's row for it asks only for `one notice naming the keys`. The quoted body also lists all three keys as a fixed sentence while the preceding paragraph says the daemon counts the blocks whose command is `<LEGACY_PLUGIN_ID>.<action>`, so it does not say what the body is when only one or two legacy blocks are present: whether the keys named are the ones found or this plugin's three, and how the sentence reads in the singular. The implementation task's done-criterion and the test's assertion are both exactly this string, so I cannot write either without choosing between the design's text and AC-11's list, which is a decision the design does not assign."
  blocker: null
```

### How the question was answered

Section 5a now pins the body as a form with its two instantiations written out in
full — the plural and the singular — states that the keys named are the ones
found rather than this plugin's three, and fixes their order as the order
`BINDINGS` declares them, so the sentence does not depend on the order somebody's
file happens to be in. The journal line is pinned the same way. Section 8's rows
were rewritten to assert those strings rather than a shape.

The reviewer also noted, without asking, that the design did not say what happens
when the configuration cannot be located, does not exist, or does not parse. Its
reading — no keys found, no notice — is now stated in the design, with the reason:
a file the daemon cannot read is not evidence that a key is dead, and `setup` is
the place that reports an unparsable configuration, because it has a terminal and
the daemon has none.

## S2 gate, round 4

```yaml
gate:
  stage: S2
  artifact: DESIGN_73.md
  reviewer: planner
  verdict: READY
  round: 4
  date: 2026-09-18
  questions: []
  blocker: null
```

## S3 gate, round 1

```yaml
gate:
  stage: S3
  artifact: PLAN_73.md
  reviewer: implementer
  verdict: READY
  round: 1
  date: 2026-09-18
  questions: []
  blocker: null
```

The reviewer checked every file and line the plan cites against the code in this
worktree and found each one as stated, and checked that every signature a task
introduces is consumed later exactly as produced.

## S4 Implement

### A correction to the plan, found by executing it

`cargo clippy --all-targets -- -D warnings` refuses a constant nothing calls:

```
error: constant `LEGACY_PLUGIN_ID` is never used
  --> src/transport.rs:23:11
```

The plan had task 1 add `LEGACY_PLUGIN_ID` and task 2 add `legacy_sibling`, each
with tests and no other caller. Neither can be committed green on its own, and
`pub` does not exempt them: this is a binary crate, and the same rule is already
recorded twice in it — `HerdrCli::with_binary` is `#[cfg(all(test, unix))]` and
`main.rs`'s `IMPLEMENTED` is `#[cfg(test)]`, both "because a constant used nowhere
else would trip `dead_code`, and CI runs clippy with `-D warnings`".

So the task boundaries move, and nothing else does. Each task now lands with a
caller outside the tests:

| was | is |
|---|---|
| 1: constants and spellings | 1: `PLUGIN_ID` and every spelling of the id — no new constant |
| 2: `legacy_sibling` | folded into what is now task 3, whose `setup::main` is its only caller outside a test |
| 3: `commit` out of `append` | 2, unchanged and still independent |
| 4: `superseded` and `rewrite_commands` | 3, and it brings `LEGACY_PLUGIN_ID`, whose first caller is `decide` |
| 5: what `setup` reports | 4, and it brings `legacy_sibling` |
| 6, 7, 8 | 5, 6, 7, unchanged |

### Task 1 — done

`herdr-plugin.toml`, `PLUGIN_ID`, and the twenty-five other places the id was
spelled out. Committed as `107cd68`.

Before the constant changed, the manifest was changed alone, to see the guard
fire rather than to trust it:

```
thread 'setup::tests::every_action_the_snippet_names_is_declared_in_the_manifest'
panicked at src/setup.rs:1683:9: assertion `left == right` failed
  left: "herdr-voice"  right: "haurylau.voice"
```

Four gates after: `cargo test` 491 + 2 passed, clippy clean, `fmt --check` clean,
`check_manifest.py` 12 entries.

### Task 2 — done

`commit` is the safe write, `append` is one call to it. The four properties were
re-established by mutation rather than by trusting the move; each broke exactly
one test:

| mutation | what went red |
|---|---|
| the original is never judged by herdr | `an_original_herdr_already_complains_about_is_left_alone`, and two more |
| the rename becomes a copy | `the_file_is_replaced_by_a_rename_rather_than_written_in_place` |
| a symbolic link is not resolved | `a_configuration_that_is_a_link_keeps_the_link_and_changes_what_it_points_at` |
| the original's mode is not carried | `the_mode_the_original_had_is_the_mode_the_result_has` |

Four gates after: `cargo test` 493 + 2 passed, clippy clean, `fmt --check` clean,
`check_manifest.py` 12 entries.

### A second correction, measured rather than inferred

The plan's task 2 was to add `legacy_sibling` with its tests and no other caller.
It cannot be committed green either, and this time the compiler said so directly
rather than by analogy:

```
error: function `rewrite_commands` is never used
error: function `replaced` is never used
```

A caller inside `#[cfg(test)]` does not exempt a function in the non-test build of
a binary crate. So `legacy_sibling`, `LEGACY_PLUGIN_ID`, `rewrite_commands` and
`Decision::superseded` all land in one commit with `run` and `setup::main`, which
are their callers. The plan's tasks 3, 4 and 5 become one.

### `append` split rather than kept

Once `run` writes both halves in one `commit` call, `append` had no caller outside
the tests — the same rule again. Rather than keep a wrapper alive for the tests,
it split along the seam it already had:

- `commit(herdr, path, make)` — the safe write, which owns the four properties;
- `appended(original, addition) -> String` — the text rule, which owns the blank
  line and the missing trailing newline.

`run` composes them, and the closure that was duplicating the blank-line rule
calls `appended` instead. Every test that drove `append` now drives `commit` with
`appended` inside it, so the same properties are measured by the same assertions,
and the four mutations were run a second time after the change:

| mutation | what went red |
|---|---|
| the original is never judged by herdr | `an_original_herdr_already_complains_about_is_left_alone`, and two more |
| the rename becomes a copy | `the_file_is_replaced_by_a_rename_rather_than_written_in_place` |
| a symbolic link is not resolved | `a_configuration_that_is_a_link_keeps_the_link_and_changes_what_it_points_at` |
| the original's mode is not carried | `the_mode_the_original_had_is_the_mode_the_result_has` |

### Tasks 3 and 4 — done

Four gates after: `cargo test` 508 + 2 passed, clippy clean, `fmt --check` clean,
`check_manifest.py` 12 entries.
