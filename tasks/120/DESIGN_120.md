# DESIGN_120

Built from `AC_120.md`. Each decision: context, problem, decision, why.

## 1. Two looks, one key

**Context.** `value` (`src/indicator.rs:41`) is the only place forms are composed;
the drawing thread calls it for the token on every tick and for the tab label.

**Problem.** The forms must differ by look while staying one source of truth, and
the look must be decided before the first paint of a take.

**Decision.** `config::Style { Auto, Glyphs, Text }` deserialised from
`[ui] style` in lowercase, default `Auto`. `indicator::Look { Glyphs, Text }` is
what a take actually uses. Two composing functions replace `value`:

- `token(state, look, frame) -> String` — the form the token carries on tick
  `frame` of the take;
- `label(state, look) -> String` — the steady form the tab suffix carries.

**Why.** A serde enum makes a wrong value fail the file the way `blink_ms = "fast"`
does (AC-1), and the settings popup already refuses such a file before saving
(`src/config_edit.rs:263`). Splitting token and label makes "the tab never
animates" a property of the signature rather than of a flag.

## 2. Forms

**Context.** Requirements 3–5. herdr trims surrounding whitespace off a token value,
so a form that begins or ends with a blank would change width.

**Problem.** Animation must not move text, and the text look must carry no marks.

**Decision.**

| state | glyphs, token on frame n | glyphs, tab | text, token on frame n | text, tab |
|---|---|---|---|---|
| recording | `●` (n even) / `○` (n odd) `+ " REC m:ss"` | `● REC m:ss` | `REC m:ss` (n even) / `REC m ss` (n odd) | `REC m:ss` |
| transcribing | `SPIN[n % 8] + " TRANSCR m:ss"` | `⋯ TRANSCR m:ss` | `TRANSCR m:ss` / `TRANSCR m ss` | `TRANSCR m:ss` |
| fixing | `SPIN[n % 8] + " FIX m:ss"` | `⋯ FIX m:ss` | `FIX m:ss` / `FIX m ss` | `FIX m:ss` |

`SPIN = ['⣷','⣯','⣟','⡿','⢿','⣻','⣽','⣾']`. Every alternate keeps the character
count and starts with a non-blank character; the colon and the space are both
one narrow cell; the braille cells are all Narrow; `●` and `○` are both
East Asian Ambiguous, so whatever width a terminal gives one it gives the other.

**Why.** These are the forms the owner accepted. The colon blink keeps a blink in
the text look with nothing at either end that herdr could trim.

## 3. A clock for every stage

**Context.** `Activity::Working` (`src/daemon.rs:73`) has no time stamp; recording
has `since`, stamped from `Runtime.clock`.

**Problem.** Requirement 6 needs elapsed time for transcribing and fixing.

**Decision.** `Activity::Working` gains `since: crate::ptt::Stamp`, stamped with
`runtime.clock.now()` at each of the three places that publish it
(`src/daemon.rs:430`, `:783`, `:1085`) — so it restarts when the stage changes from
transcribing to fixing. `State::Transcribing` and `State::Fixing` gain
`elapsed_ms`, computed the way recording's is (`runtime.clock.now() - since`).

**Why.** Same clock, same arithmetic as the clock that already works; a stamp per
publish is what makes "from the start of this stage" true.

## 4. Deciding `auto`

**Context.** `doctor` already finds herdr's configuration file
(`herdr_config_location`, `src/doctor.rs:696`). The drawing thread talks to herdr
only through `Painter` (`src/indicator.rs:130`), which tests replace with a
recording fake.

**Problem.** The look must be resolved once per take (AC-8) and be testable without
the real file.

**Decision.**

- `Painter` gains `fn herdr_configuration(&self) -> Option<String>`: the text of
  herdr's configuration file, or `None` when it cannot be located or read.
  `HerdrPainter` reads the file at `doctor`'s location (that function becomes
  `pub(crate)`); `RecordingPainter` answers with text a test set.
- `fn radar_owns_sidebar(text) -> bool`: some line, trimmed, equals
  `# >>> herdr-radar sidebar block`.
- `fn resolve(style, painter) -> Look`: `Glyphs` and `Text` map directly; `Auto`
  reads the configuration and returns `Glyphs` only when `radar_owns_sidebar`.
- `Drawn` gains `look: Option<Look>`. A tick that sees a take and has no look
  resolves it; `finish` clears it. A take that moves to another tab passes through
  `finish`, so it resolves again.

**Why.** The configuration file is the one place where "the radar draws the agents
panel" is written: the radar's switch back to herdr's own panel removes that block.
Putting the read behind `Painter` keeps every herdr-facing fact in one seam.

## 5. Frame counter instead of the blink flag

**Context.** `Drawn.blink: bool` flips on every tick, sidebar on or off, and is
reset in `finish`.

**Problem.** The spinner needs eight positions, not two.

**Decision.** `Drawn.frame: u64`, incremented on every tick where `blink` used to
flip, reset to 0 in `finish`. Two-form alternation reads `frame % 2`.

**Why.** It keeps the existing rule (the counter advances whether or not the token
is written) and covers both animations.

## 6. The sweep without a marker

**Context.** The sweep renames a tab whose label contains `🎙️` and cuts from it
(`src/indicator.rs:590`, `:76`). New forms carry no marker.

**Problem.** A killed daemon must still be cleaned up after (AC-9), and a label the
plugin did not write must be left alone.

**Decision.** `strip(label)` first cuts from a legacy `🎙️` as now. Otherwise it
recognises a suffix written by this plugin: the end of the label is, after an
optional `● ` or `⋯ `, one of `REC`, `TRANSCR`, `FIX`, a space, one or more ASCII
digits, `:`, two ASCII digits, and that suffix is the whole label or is preceded by
a space. It returns the label before the suffix with trailing whitespace trimmed,
and the label unchanged when nothing matches. The sweep renames a tab when
`strip(label) != label`. Parsed by hand; no new dependency.

**Why.** The suffix grammar is closed and short. The cost — a tab a person named
`release FIX 1:00` would be stripped once at daemon start — is written down in the
design document; an invisible marker was rejected because herdr's handling of
format characters in tab names has not been measured.

## 7. Configuration surfaces and documents

**Decision.** `settings::allowed("ui","style")` returns `["auto","glyphs","text"]`
so the popup offers the set. `docs/design.md` section 6 replaces the table of forms
and describes the looks, `auto` and the sweep, with a copyable sidebar row that
colours the token through herdr's `rules`; section 7 adds the key. The README's
one-line description of the indicator (`README.md:26-27`) names the two looks.
`docs/evidence.md` gets the by-hand check (S5).

## Tests (by AC)

- AC-1 config: parse three values, absent → `Auto`, `style = "neon"` → `Invalid`.
- AC-2 `resolve` with each style and configuration texts with and without the line,
  and `None`.
- AC-3/4 `token` for each state, look and frames 0..8.
- AC-5 drawing harness: tab renames carry `label` forms, once per change.
- AC-6 every form, every frame: char count equal across frames, first char not
  whitespace, none of the old emoji, `🎙`, U+FE0F, U+3000.
- AC-7 drawing harness: `Working` with `since` shows the clock from that stamp.
- AC-8 change the recording painter's configuration text mid-take; forms stay.
- AC-9 `strip` on every tab form of both looks, with and without a label before it,
  legacy forms, and labels that merely contain `REC` or end in a time.

## Revision 2 — the glyph look without the word

**Context.** `token` and `label` (section 2) compose a mark, the word and the clock in
the glyph look; transcribing and fixing share the braille spinner on the token and `⋯`
on the tab.

**Problem.** Mark and word say the same thing twice, and without the word the two
working stages would look alike.

**Decision.** In the glyph look each state has a pair of marks, filled and hollow:
recording `●`/`○`, transcribing `✎`/`✐` (U+270E, U+2710), fixing `✦`/`✧` (U+2726,
U+2727). The token is `{mark} {clock}` with the filled mark on even frames and the
hollow one on odd frames; the tab is `{filled} {clock}`. The spinner and `⋯` go. The
sweep's grammar (section 6) accepts, before the clock's space, either a word (`REC`,
`TRANSCR`, `FIX`) or one of the filled marks; the suffix is still the whole label or
follows a space. The colour snippet in `docs/design.md` matches the marks.

**Why.** One sign per state, as the owner asked; the pairs keep the alternation that
shows the take is alive, and each pair is one width class (`●`/`○` Ambiguous, the four
dingbats Neutral) with no emoji presentation, so nothing moves and nothing turns into
a colour picture. The cost added to the sweep: a tab a person named ending in, say,
`● 1:00` is cleaned up once at daemon start.
