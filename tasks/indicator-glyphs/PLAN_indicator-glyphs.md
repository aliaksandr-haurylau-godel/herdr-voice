# PLAN_indicator-glyphs

Built from `DESIGN_indicator-glyphs.md`. Work in the worktree `worktrees/glyphs` on
branch `feat/indicator-glyphs`. Each task: write the failing test first, see it
fail, make it pass, then `cargo test` for the touched module. One cargo at a time,
`CARGO_BUILD_JOBS=6`.

## Task 1 — `[ui] style` (AC-1)

Files: `src/config.rs`, `src/settings.rs`.

1. Tests in `src/config.rs`: `[ui]\nstyle = "glyphs"` loads `Style::Glyphs`; same
   for `auto`, `text`; no key → `Style::Auto`; `style = "neon"` → `Source::Invalid`.
   Extend the existing defaults test (`src/config.rs:777` area) with
   `ui.style == Style::Auto`.
2. Add `#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, Default)]
   #[serde(rename_all = "lowercase")] pub enum Style { #[default] Auto, Glyphs, Text }`
   with a doc comment, and `pub style: Style` on `Ui` with a doc comment; default in
   `impl Default for Ui`.
3. `src/settings.rs` `allowed`: `("ui", "style") => Some(vec!["auto", "glyphs", "text"])`.
   Update the test that counts `[ui]` keys (`"1 of 4 keys set"`, `src/settings.rs:702`)
   to 5.

## Task 2 — a stamp on `Working` (design §3)

Files: `src/daemon.rs`, `src/indicator.rs` (match arm only).

1. Add `since: crate::ptt::Stamp` to `Activity::Working` with a doc line saying it
   is stamped when the stage is published, from `Runtime.clock`.
2. At `src/daemon.rs:430`, `:783`, `:1085` add `since: runtime.clock.now()`.
3. Every other construction (tests near `src/daemon.rs:3786`, `:3850`, `:3999`, and
   `src/indicator.rs` test helper near `:1619`) gets `since: 0` or the test clock's
   value; matches with `{ target, tab, stage }` (`src/daemon.rs:5475`, `:5535`) get `..`.
4. `cargo test` must pass unchanged in behaviour.

## Task 3 — forms (AC-3, AC-4, AC-6, AC-7 arithmetic)

File: `src/indicator.rs`, plus one assertion in `src/daemon.rs`. The crate compiles
at the end of every step.

1. `State` variants become `Recording { elapsed_ms }`, `Transcribing { elapsed_ms }`,
   `Fixing { elapsed_ms }`. In `tick` (`src/indicator.rs:444-450`) the `Working`
   arm becomes `Working { target, tab, stage, since }` and both stages get
   `elapsed_ms: runtime.clock.now().saturating_sub(since)`. `value` keeps its
   current output by matching `Transcribing { .. }` and `Fixing { .. }`; it is
   removed in task 5, not here. Existing tests that build `State::Transcribing` or
   `State::Fixing` get `{ elapsed_ms: 0 }`; the assertion at `src/daemon.rs:5548`
   becomes `value(&State::Fixing { elapsed_ms: 0 }, false)`.
2. Tests first, then code: `pub enum Look { Glyphs, Text }` (`Debug, Clone, Copy,
   PartialEq, Eq`), `const SPIN: [&str; 8] = ["⣷","⣯","⣟","⡿","⢿","⣻","⣽","⣾"];`,
   `pub fn token(state: &State, look: Look, frame: u64) -> String`,
   `pub fn label(state: &State, look: Look) -> String`, exactly per the table in
   design §2. Clock text from `elapsed`; for text-look odd frames the `:` becomes a
   space.
3. Tests for step 2: exact forms for each state and look at frames 0..8 (AC-3,
   AC-4); for every state and look, all frames of `token` have equal
   `chars().count()` and equal `columns`, the first char is not whitespace, and no
   form of `token` or `label` contains `🎙`, `\u{FE0F}`, `\u{3000}`, `🔴`, `📝`,
   `🪄` (AC-6). `columns` is a new test helper next to the old one: ASCII, `●`, `○`,
   `⋯` and the eight braille cells count one column, any other character fails the
   test as unexpected.

## Task 4 — resolving the look (AC-2)

Files: `src/indicator.rs`, `src/doctor.rs`, `src/config.rs` (use only).

1. `src/doctor.rs`: make `herdr_config_location` and `Location` `pub(crate)`.
2. `Painter` gains `fn herdr_configuration(&self) -> Option<String>;`. It is not
   recorded as a `Paint` call (the harness's "a tick painted something" check counts
   only herdr calls that paint or list).
   `HerdrPainter`: `match crate::doctor::herdr_config_location() { Location::Path(p) => std::fs::read_to_string(p).ok(), Location::Unknown => None }`.
   `RecordingPainter`: a `configuration: Option<String>` field in `Inner`, default
   `None`, with `pub fn set_configuration(&self, text: Option<&str>)`.
3. `pub const RADAR_BLOCK: &str = "# >>> herdr-radar sidebar block";`,
   `fn radar_owns_sidebar(text: &str) -> bool` (some line, trimmed, equals it),
   `fn resolve(style: crate::config::Style, painter: &dyn Painter) -> Look`.
4. Tests first: `resolve` for `Glyphs` and `Text` (configuration ignored); `Auto`
   with text containing the line at column 0 and indented → `Glyphs`; text without
   it, and `None` → `Text`.

## Task 5 — drawing (AC-5, AC-7, AC-8)

File: `src/indicator.rs`, plus `src/daemon.rs:5548`.

1. `Drawn`: replace `blink: bool` with `frame: u64`; add `look: Option<Look>`.
2. In `tick`, **after** the other-tab check that may call `finish`
   (`src/indicator.rs:456-462`) and before the token is painted:
   `let look = *self.look.get_or_insert_with(|| resolve(runtime.ui.style, painter));`
   Resolving after that check is what makes a take that moved tabs resolve again.
3. Token: `token(&state, look, self.frame)`; then
   `self.frame = self.frame.wrapping_add(1);` where `blink` was flipped (whether or
   not the token was written, as now).
4. `paint_tab` gains a `look: Look` parameter, passed the local `look` from step 2,
   and composes `decorate(&held.original, &label(state, look))`. No `Option` reaches
   it, so there is no `None` case.
5. `finish`: `self.frame = 0; self.look = None;` where `blink = false` was.
6. Remove `value`, `BLANK`, and `MARKER`; add `const LEGACY_MARKER: &str = "🎙️";`
   used only by `strip` (task 6 rewrites `strip`; until then it uses
   `LEGACY_MARKER` exactly as it used `MARKER`). `src/daemon.rs:5548` becomes
   `assert_eq!(crate::indicator::label(&crate::indicator::State::Fixing { elapsed_ms: 0 }, crate::indicator::Look::Text), "FIX 0:00");`.
7. Existing tests: the harness uses `Ui::default()`, so `style = Auto` and the
   recording painter's configuration is `None` — every existing drawing test runs in
   the text look. Replace exactly these assertions:
   - the old form tests at `src/indicator.rs:835-918`
     (`the_three_states_read_as_the_owner_settled_them`,
     `the_blink_form_keeps_the_width_...`, `both_forms_begin_with_the_marker_...`)
     are deleted; task 3 step 3 replaces them. The old `columns` helper goes with
     them.
   - `decorating_and_stripping_are_inverses` and
     `stripping_a_label_nobody_decorated_leaves_it_alone`: build the decoration
     with `label(&State::Recording { elapsed_ms: 5_000 }, Look::Text)` and
     `Look::Glyphs`.
   - `the_token_is_renewed_every_tick_and_alternates` (around `:1487`): alternation
     is `values[0] != values[1]` and `values[0] == values[2]`; the marker loop
     becomes `assert!(!value.starts_with(char::is_whitespace))`, and the expected
     values are `REC 0:00`, `REC 0 00`, `REC 0:00`, `REC 0 00`.
   - `the_tab_is_renamed_once_a_second_...` (`:1585`): `renames[1].1 == "1 REC 0:01"`.
   - `a_state_with_no_clock_renames_the_tab_once_...` (`:1617`): renamed
     `the_tab_is_renamed_when_a_working_stages_clock_moves`; `Working` gets
     `since: 0`; five ticks with no take-clock advance give one rename `"1 TRANSCR 0:00"`;
     after `take_clock.advance(1_000)` and one tick, a second rename
     `"1 TRANSCR 0:01"` (AC-5, AC-7).
   - `the_daemon_going_away_puts_the_tab_back` (`:1661`) and the sweep-retry test
     (`:2050`): `label.contains(MARKER)` becomes `strip(label) != *label` ("the label
     carries our suffix").
   - `an_empty_original_label_is_restored_as_empty` (`:1729`): expected `"REC 0:00"`.
   - Tests that put legacy `🎙️` labels on tabs for the sweep (`:1145`, `:1864-1866`,
     `:1905`, `:1924`, `:1970`, `:2009-2011`) and the argument tests with literal
     values (`:1241`, `:1274`, `:1316`, `:1325`) stay as they are: they test the
     legacy sweep and the argument plumbing, not the forms.
   Any other test that fails after this step is a defect in the step, not a test to
   edit; stop and report it.
8. New drawing tests: `Ui { style: Style::Glyphs, ..Ui::default() }` gives tokens
   `● REC 0:00`, `○ REC 0:00` and tab `1 ● REC 0:00`; a `Working` transcribing take
   in glyphs gives tokens `⣷ TRANSCR 0:00`, `⣯ TRANSCR 0:00`, `⣟ TRANSCR 0:00` and
   tab `1 ⋯ TRANSCR 0:00`; with `Auto`, `set_configuration(Some(RADAR_BLOCK))` before
   the take gives glyph tokens, and `set_configuration(None)` in the middle of that
   take leaves them glyphs until `Idle`; the next take is text (AC-8).

## Task 6 — sweep (AC-9)

File: `src/indicator.rs`.

1. `strip(label)`: if it contains `LEGACY_MARKER`, cut as today. Otherwise parse the
   end of the label as design §6: optional `"● "` or `"⋯ "`, one of `REC`,
   `TRANSCR`, `FIX`, `" "`, one or more ASCII digits, `":"`, exactly two ASCII
   digits, end of string; the suffix must start at index 0 or follow a space.
   Return `label[..start].trim_end()` or the label unchanged.
2. `sweep_tabs`: rename when `strip(&label) != label`.
3. Tests first: every `label` form of both looks and all three states, decorated
   onto `"api"` and onto `""`, strips back; legacy `"api 🎙️🔴 REC 0:05"` → `"api"`;
   unchanged: `"REC notes"`, `"build 1:05"`, `"xREC 0:05"`, `"api REC 0:5"`,
   `"api FIX 0:02 later"`. The grammar is exactly step 1 and nothing more: a
   character before the suffix other than a space is not examined, so
   `"api ○ REC 0:05"` strips to `"api ○"` (the suffix `REC 0:05` follows a space);
   no test asserts on it, since no tab is ever written with `○`. One new sweep harness test: tabs `("w1:t1","1 REC 0:12")`,
   `("w1:t2","review ⋯ TRANSCR 0:03")`, `("w1:t3","notes")` → renames to `"1"` and
   `"review"` only.

## Task 7 — documents (AC-11)

1. `docs/design.md` §6: replace the forms table with design §2's table, describe
   the looks, `auto`, the per-stage clock, the frame counter and the marker-free
   sweep with its cost; §7: add `style = "auto"  # auto | glyphs | text`.
   Also in §6, a sidebar row snippet colouring `$voice` with `rules`
   (`starts_with = "●"` / `"○"` red, `contains = "TRANSCR"` amber,
   `contains = "FIX"` violet).
2. `README.md`: the "Says what it is doing" bullet (`README.md:26-27`) says the
   indicator has a glyph look and a text look, chosen by `[ui] style`, with a clock
   for every stage. The README holds no configuration reference, so nothing more.

## Task 8 — checks

`cargo test`, `cargo clippy --all-targets -- -D warnings`, `cargo fmt --check`,
`python3 scripts/check_manifest.py`. All four pass before S4 review.
