# RUN_indicator-glyphs

| field | value |
|---|---|
| issue | none yet — the issue text is prepared in this run and filed by the owner; the directory and branch carry no number until then |
| input | the owner's decision relayed by another session on 2026-11-16, not a GitHub issue |
| stage | S5 |
| branch | feat/indicator-glyphs |
| opened | 2026-11-16 |

## Stages

<!-- One block per stage, appended, never rewritten. -->

### S1 Assess
- artifact: `AC_indicator-glyphs.md`
- produced: 2026-11-16

```yaml
gate:
  stage: S1
  artifact: AC_indicator-glyphs.md
  reviewer: designer
  verdict: READY
  date: 2026-11-16
  blocker: null
```

Reviewer notes, recorded as returned (not gate questions): AC-8 can only mean herdr's file changing mid-take, since the plugin's own `[ui]` already needs a restart (`plan_reload`, `src/daemon.rs:306`); equal display width holds because every alternating pair stays in one East Asian Width class; a text-look suffix has no marker, so the sweep will rename a tab a person named like "notes FIX 0:02" — the requirements imply it.

### S2 Design
- artifact: `DESIGN_indicator-glyphs.md`
- produced: 2026-11-16
- The look, the glyph set and the key were settled by the owner on a written proposal before this run, so the design records them rather than reopening them through `superpowers:brainstorming`.

```yaml
gate:
  stage: S2
  artifact: DESIGN_indicator-glyphs.md
  reviewer: planner
  verdict: READY
  date: 2026-11-16
  blocker: null
```

Reviewer notes, recorded as returned (not gate questions): the AC-6 tests check character count but not display width — plan a width assertion with the column helper in `src/indicator.rs` tests; a test at `src/daemon.rs:5548` calls `crate::indicator::value` and must move with its removal. While the review ran, section 7 was edited to place the colour snippet in `docs/design.md` rather than the README (the README has no configuration reference); no decision changed.

### S3 Plan
- artifact: `PLAN_indicator-glyphs.md`
- produced: 2026-11-16
- Both S2 reviewer notes are planned (task 3, steps 5 and 6).

```yaml
gate:
  stage: S3
  artifact: PLAN_indicator-glyphs.md
  reviewer: implementer
  verdict: QUESTIONS
  date: 2026-11-16
  questions:
    - "Tasks 3 and 4 cannot reach a passing `cargo test` in the order written: task 3 changes `State` and removes `value` while `tick` and `paint_tab` still use them until task 5. The plan has to say how tasks 3-4 compile before task 5."
    - "Task 5 step 2 must state which side of the `finish` call in the other-tab check the resolve goes on."
    - "Task 5 step 3: `paint_tab` has no `look` parameter and `self.look` is an `Option`; the plan must say whether to add a parameter or handle `None`."
    - "Task 5 step 6 names no tests or lines; the new assertions and whether `MARKER` stays are not determinable."
  blocker: null
```

### S3 Plan, revised
- artifact: `PLAN_indicator-glyphs.md`, tasks 3-6 rewritten
- produced: 2026-11-16
- Answers: task 3 keeps `value` compiling until task 5; resolve goes after the other-tab check; `paint_tab` takes `look: Look`; task 5 step 7 lists every assertion that changes and what it becomes, and `MARKER` becomes `LEGACY_MARKER`, private to the sweep.

## Notes

- Base: `d68b2a2`.

```yaml
gate:
  stage: S3
  artifact: PLAN_indicator-glyphs.md
  reviewer: implementer
  verdict: QUESTIONS
  date: 2026-11-16
  questions:
    - "Task 6 step 3 expects `\"api ○ REC 0:05\"` unchanged, but the grammar of step 1 matches the glyph-less suffix after the space and returns `\"api ○\"`. State the rule or change the expected result."
  blocker: null
```

### S3 Plan, revised again
- produced: 2026-11-16
- Answer: the expected result was wrong; the case is removed from the unchanged list and task 6 says the grammar is step 1 and nothing more.

```yaml
gate:
  stage: S3
  artifact: PLAN_indicator-glyphs.md
  reviewer: implementer
  verdict: READY
  date: 2026-11-16
  blocker: null
```

### S4 Implement
- started: 2026-11-16, executed inline from the plan, test first per task.
- code committed as `be32710`; diff `d68b2a2..be32710` reviewed by an independent subagent with `superpowers:requesting-code-review`.

```yaml
gate:
  stage: S4
  artifact: diff d68b2a2..be32710
  reviewer: code (superpowers:requesting-code-review, independent subagent)
  verdict: READY with fixes
  date: 2026-11-16
  blocker: null
```

Review findings, as returned, and what was done:
- Important 1 — the colour snippet in `docs/design.md` coloured recording only in the glyph look, while herdr's own panel gets the text look under `auto`: fixed, a `starts_with = "REC"` rule added; the snippet passes `herdr config check`, and a misspelled rule key fails it, so the check does validate rule keys.
- Important 2 — the evidence entry is missing: that is S5, below.
- Minor 1 — a label ending in a lone `●` or `⋯` that then carried a text-look suffix is stripped by the sweep: documented beside the `release FIX 1:00` cost in `docs/design.md` and the `suffix_start` comment.
- Minor 2 — the legacy branch cuts any label containing `🎙️`, decorated or not: behaviour unchanged from before this run; not changed here.
- Minor 3 — no daemon-level test asserts the `since` values: not added; covered by reading the three publish sites and by the indicator's per-stage clock test.
- Minor 4 — the transcribing clock includes context collection: documented in `docs/design.md`.
- Minor 5 — two takes with less than one `blink_ms` between them and no tab change share a look and frame count: not changed; noted here.
- Minor 6 — braille and `⋯` are Neutral, not Narrow, and one sentence used change-history wording: both fixed in `docs/design.md`.
- Minor 7 — `radar_owns_sidebar` requires the exact start line: accepted as written in AC-2.

### S5 Verify
- section: `docs/evidence.md`, "The glyph look and the text look of the indicator, by hand on macOS"
- platform: macOS 27.2, arm64, herdr 0.9.5, release build of this branch with its own configuration and state directories; a scratch tab created for the check and closed afterwards.
- checks: `cargo test` (1031 + 4 + 4 + 9 passed, 1 ignored), `cargo clippy --all-targets -- -D warnings` clean, `cargo fmt --check` clean, `python3 scripts/check_manifest.py` "13 entries, all commands known".

```yaml
verdict:
  stage: S5
  result: PASS
  date: 2026-11-16
  not_verified:
    - how the forms look to a person on screen (values read from herdr, not seen)
    - real recognition and rewrite engines
    - Linux and Windows
```

## Revision 2 — the glyph look without the word (2026-11-17)

Input: the owner watched the live demonstration and asked for a mark or a word, not
both, and distinct marks for transcribing and fixing; the owner chose the
filled/hollow pairs `●/○`, `✎/✐`, `✦/✧`. AC, DESIGN and PLAN each gained a revision
section; the three gates below review those sections.

```yaml
gate:
  stage: S1
  artifact: AC_indicator-glyphs.md, revision 2
  reviewer: designer
  verdict: READY
  date: 2026-11-17
  blocker: null
```

Reviewer notes (not gate questions): AC-9' does not list the revision-1 glyph forms, which only this unreleased branch ever wrote — the design should say so; a hand-named label ending in a mark and a clock is now stripped too; the colour snippet must change, which AC-11 covers.

```yaml
gate:
  stage: S2
  artifact: DESIGN_indicator-glyphs.md, revision 2
  reviewer: planner
  verdict: READY
  date: 2026-11-17
  blocker: null
```

Reviewer notes (not gate questions): the forms table, width paragraph and sweep paragraph in `docs/design.md` and the README example also show the old glyph forms and are rewritten under AC-11; read literally, the grammar drops the optional mark before a word, so a revision-1 tab such as `x ● REC 0:05` would strip to `x ●` — those forms were never released.

```yaml
gate:
  stage: S3
  artifact: PLAN_indicator-glyphs.md, revision 2
  reviewer: implementer
  verdict: READY
  date: 2026-11-17
  blocker: null
```

### S4 Implement, revision 2
- started: 2026-11-17.
