# DESIGN_84

What recognition produced and what the rewrite made of it are both recorded, for
the take they belong to. Built against `AC_84.md`; issue #84 is the ticket.

## 1. Where the record goes

### Context

A take already leaves one file behind: `take_path` (`src/capture.rs:405-411`)
writes `<state>/takes/{unix-millis}-{pid}-{counter}.wav`, and the state directory
is `transport::state_directory` (`src/transport.rs:121-133`). The journal the
daemon writes goes to standard error and nowhere else
(`StderrJournal`, `src/daemon.rs:1019-1028`).

### Problem

Two destinations suggest themselves and both fail. Standard error is where the
journal already goes, and AC-4 excludes it: reaching it means knowing what herdr
did with the daemon's file descriptors, which nobody dictating knows. The herdr
plugin log looks like the answer and is not, and the measurement belongs here
rather than only in the criteria, because the next person wanting a per-take
record will reach for it too. `herdr plugin log list --limit 10000` returns
exactly 200 records — `plugin-log-3365` through `plugin-log-3564`, 200
consecutive ids with no gap and nothing older — and that ring is shared by every
installed plugin rather than held per plugin. Of those 200, 198 were this
plugin's own `ptt` action, and 199 `ptt` records seen a minute earlier spanned 43
seconds of wall clock. `ptt` fires on every key auto-repeat, about twelve times a
second (`docs/design.md` section 2), and each repeat is one record, so a minute
of holding the dictation key overwrites the whole log. Anything this plugin
writes there is gone by the next take.

That also settles a question this design does not have to answer. Where herdr
sends a `[[startup]]` process's standard error was never established, because
observing it means restarting herdr. It does not matter: the log is out either
way, and standard error is excluded by AC-4 whatever it is connected to.

### Decision

One file per take, beside that take's recording, with the same stem:
`<state>/takes/{unix-millis}-{pid}-{counter}.json`. `doctor` gains a seventh line
named `record`, which says whether recording is on and names the directory.

### Why

The stem is already unique per take and already carries the time, so AC-3 costs
nothing: two takes cannot be confused because two takes never share a stem, and
the record can be matched to its audio by eye. `doctor` is where this project
already answers "what is missing and where is it" in six fixed lines
(`src/doctor.rs:1-6`), so a person who has just dictated runs one command they
already have rather than learning a new one.

The seventh line is `ok` when recording is on and `default` when it is off, and
never `Missing`. `Missing` is what makes `doctor` exit non-zero
(`src/doctor.rs:92-94`), and a key that is off because off is its default is not
something to go and fix.

## 2. What one record holds

### Context

AC-1 asks for both texts, distinguishable. AC-2 asks for the rewrite's status,
and when it did not run, which of four ways. `serde_json` is already a
dependency (`Cargo.toml:27`).

### Problem

A record that holds only the two texts cannot answer "did the rewrite run", since
a rewrite that changed nothing and a rewrite that never happened both leave two
identical strings.

### Decision

Pretty-printed JSON, one object:

```json
{
  "take": "1789729477005-4242-1",
  "transcript": "fix the worklog entry",
  "rewrite": { "ran": true, "text": "Fix the worklog entry." }
}
```

and when it did not run, the same object with one of:

```json
  "rewrite": { "ran": false, "why": "off" }
  "rewrite": { "ran": false, "why": "skipped" }
  "rewrite": { "ran": false, "why": "unavailable", "detail": "<what to fix>" }
  "rewrite": { "ran": false, "why": "failed", "detail": "<what went wrong>" }
```

There is no timestamp field and no delivered-text field.

### Why

Pretty-printed because the reader is a person with `cat`, not a program. `take`
repeats the stem inside the file so a record that has been copied somewhere still
says which take it is. No timestamp field, because the stem's first component is
unix milliseconds and the file's own modification time says the same thing again
— a third copy would be a third thing that can disagree, and adding a readable
date would mean adding a date library this repository does not have. No
delivered-text field, because the delivered text is the rewritten text when
`ran` is true and the transcript in every other case, and a field that only ever
repeats one of two others is a field that can contradict them.

## 3. The four ways a rewrite does not run

### Context

`src/daemon.rs:918-937` has four arms that deliver an unrewritten transcript: the
stage switched off, an engine that did not resolve, the skip heuristic, and a
resolved engine whose call returned an error. Today all four produce the same
`delivering:` line.

### Problem

The match expression yields only the text. The reason it took a given arm is
discarded at the closing brace, so nothing downstream can record it.

### Decision

The match moves into a function of its own that returns the outcome rather than
the text:

```rust
pub enum Rewrite {
    Ran { text: String },
    Off,
    Skipped,
    Unavailable { why: String },
    Failed { why: String },
}
```

`transcribe_take` calls it, then takes the text to deliver from the outcome:
`Ran` gives its own text, and the other four give the transcript back unchanged.

The function takes `&Runtime` and carries the two `tell_once` calls with it, from
`src/daemon.rs:921` and `:931`. They keep firing on the same two arms, for the
same reasons, in the same order relative to everything else. The alternative — a
pure function whose caller matches `Unavailable` and `Failed` and calls
`tell_once` itself — puts the notice back into the caller, which is the shape the
extraction exists to get out of. `publish(runtime, Activity::Working { stage:
Stage::Fixing })` at `src/daemon.rs:910-917` stays in `transcribe_take`, before
the call: only the match moves.

### Why

The five outcomes become directly assertable without a recorder, a microphone or
a delivery, which is what AC-10 asks for. Returning the outcome rather than the
text also removes the shadowing that caused the ticket: today the transcript is
overwritten by the match's own result, and after this the transcript is a binding
nothing writes over.

## 4. What removes it

### Context

Nothing removes a delivered take's audio. `std::fs::remove_file` on a take's path
appears outside tests only at `src/capture.rs:401` and `src/daemon.rs:669`, both
on paths where the take was refused or discarded.

### Problem

A record of what somebody said, accumulating with no limit, is the thing the key
is off by default to prevent. Copying the audio's answer — nothing removes it —
would mean the key's promise is only as good as the owner remembering to empty a
directory.

### Decision

The most recent `KEEP` records are kept, and writing a record removes any beyond
that. `KEEP` is 50, a named constant in the module, not a configuration key.
What counts toward the fifty is every file in that directory whose name ends
in `.json`, ordered by file name; the audio is not touched.

### Why

A cap needs no clock, no configuration and no scheduled work: the only moment
anything is removed is the moment something is written, which is also the only
moment the daemon is already touching that directory. Ordering by file name is
ordering by time, because the stem begins with unix milliseconds and every such
stamp is thirteen digits until the year 2286, so lexicographic order and
chronological order are the same order. Fifty is far more takes than a diagnosis
reaches back through — the question this record answers is asked about the take
that just landed wrong — and it bounds what accumulates without asking anybody to
remember anything. It is a constant rather than a key for the reason
`SKIP_WORD_LIMIT` is (`src/rewrite/skip.rs:13`): no measurement distinguishes
one value from another, and a key invites tuning where there is nothing to tune.

The audio is out of the ticket's bounds and is left exactly as it is. That the
plugin now bounds the text it keeps and not the audio it keeps is an
inconsistency, and it is recorded in `AC_84.md` under "Out of scope / noticed"
rather than resolved here.

## 5. The configuration key

### Context

Every key in `docs/design.md` section 7 has a default, and an absent
configuration file is a valid state (`src/config.rs:281-300`).

### Problem

What the key keeps is what somebody said in their own room, so switching it on is
a decision about their own speech and not a preference about output.

### Decision

A new section with one key:

```toml
[record]
transcripts = false       # keep each take's transcript and rewrite on disk
```

`Runtime` carries `records: Option<Records>` — `Some` when the key is on, `None`
when it is off. Nothing else in the take path consults the key: `start` builds
the `Option` once, and `doctor` reads the loaded configuration directly for its
own line.

The name carries the consent, so two things follow, and they are requirements of
this design rather than documentation taste:

- **Section 7 of `docs/design.md` separates this key from `[context] source =
  "transcript"` where the reader meets them**, in the same breath, not by
  inference. They are one word apart in one file and mean opposite things: this
  key is about what *you* said, and `[context] source` is about what the *agent*
  said — the session transcript of the pane's conversation, read to bias
  recognition towards terms already on screen. Switching this key on keeps your
  speech; that one has never written anything.
- **The key's own documentation says what is kept and where.** A name that
  carries the consent only works when the person switching it on is told what
  they are switching on: the transcript, the rewritten text, the rewrite's
  outcome, one file per take in the state directory, the last fifty kept, nothing
  sent anywhere.

### Why

A new section rather than a key under `[ui]` or `[delivery]`, because this is
neither an indicator nor a delivery setting. `Option` rather than a boolean
beside a path, because with the key off there is then no writer to call and no
directory to name: AC-5 becomes a property of the type rather than a branch
somebody can forget to write.

`transcripts` rather than a word naming the unit, although `[context] source =
"transcript"` already uses that word for something else in the same file. The
collision is real and is the price of a key whose name states what is kept; the
disambiguation above is what pays it.

## 6. Where it is written, and what happens when it cannot be

### Context

`src/daemon.rs:941` writes the `delivering:` line immediately before the delivery
attempt, so that the text is not held only in memory while an outward call runs.
`CLAUDE.md` forbids panic paths in the daemon and rates a silent failure as a
defect of the same weight as a wrong transcript.

### Problem

A write to the state directory can fail — a full disk, a directory somebody
removed, permissions. AC-12 says that must not end the take.

### Decision

The record is written after the rewrite outcome is known and before the
`delivering:` line. A failure to write it is journalled through a new line,
`record_failed_line(path, why)`, and the take carries on to delivery unchanged.
The same is true of removing an old record: a removal that fails is journalled
and nothing else happens.

### Why

Before the `delivering:` line, because the record is the more durable of the two
and the ordering then matches what the existing comment says about not holding
the text only in memory. Journalled rather than toasted, because a toast
interrupts somebody mid-dictation for a diagnostic they did not ask for, and the
journal line is what `CLAUDE.md` requires: the failure is recorded, and it names
the path that could not be written.

## 7. What is not changed

The `delivering:` line keeps its text, its shape and its position, and goes on
being written whether the key is on or off. `tell_once` keeps raising one notice
per daemon lifetime; AC-2 makes the record carry the status itself, so nothing
depends on that notice any more, but its frequency is not this ticket's. Audio,
delivery, the indicator, the reply protocol and the bias are untouched.

## 8. Documentation

`docs/design.md` section 7 gains the `[record]` block in the configuration
listing and, beneath the listing, a paragraph in two parts: what is written, where
it is written, what removes it and that nothing is sent anywhere; then the
separation of `[record] transcripts` from `[context] source = "transcript"`,
stated where the reader meets both rather than left to inference. Section 4 gains
one sentence, under Delivery, pointing at it.

## 9. Tests

Against the criteria, all without a microphone, a model or a live herdr:

- **`src/record.rs`** — the JSON for each of the five outcomes; `take` carries the
  stem; the cap keeps the newest fifty and removes the rest; a write into a
  directory that cannot be created returns an error rather than panicking.
- **`src/daemon.rs`** — the rewrite-outcome function returns each of the five
  outcomes, driven by a `Resolution` and a fake engine (AC-10); a take that was
  rewritten, with `records: Some`, writes a file holding both texts (AC-8); the
  same take with `records: None` writes no file and leaves the directory empty
  (AC-9); a take whose record cannot be written is still delivered and leaves a
  `record_failed_line` in the journal (AC-12).

AC-11 is satisfied by construction and is checked by reading the diff: nothing
added here opens a socket or runs a process.

---

# The widening, 2026-09-18

The owner widened the ticket: delete the recordings unless debug is on. Sections
10 to 13 answer AC-13 to AC-17 and amend sections 4, 5 and 8 above; sections 1, 2,
3, 6, 7 and 9 stand unchanged.

## 10. When a take is finished with

### Context

Five endings reach a take that has a recording on disk, and four of them name that
recording to somebody by path: recognition unavailable (`src/daemon.rs:888-890`),
transcription failed (`:899-902`), delivery refused (`:990`), and the daemon
stopping with the key still down (`kept_on_shutdown_line`, `:1200-1206`). The
fifth, delivery succeeding, replies `"delivered to {target} [{level} dB]"`
(`src/daemon.rs:960-962`) and names no path at all.

### Problem

"Delete the recordings" does not say when. Deleting on an ending that promised the
file turns the promise into a pointer at nothing, which is worse than keeping the
file and worse than never having promised it.

### Decision

The recording is removed on the delivery-success arm of
`crate::delivery::deliver` in `transcribe_take`, and on no other path. Everything
else keeps it.

The condition is `runtime.records.is_none()`: `Runtime` carries the key in exactly
one form, and the absence of a writer is what "not keeping" means.

A removal that fails does not end the take. The reply stays
`"delivered to {target} [{level} dB]"`, and a new journal line,
`recording_kept_line(path, why)`, says the recording is still there, why, and that
it can be removed by hand. This is a second non-fatal failure on the same take as
AC-12's and composes with it: both are reported, neither changes what the person
gets.

### Why

That arm is the only ending where the words arrived somewhere a person can see
them and nothing named a file. The repository already states half of this rule at
`src/capture.rs:398`, above `discard`: "A take nobody will read is a take nobody
should find later." This is its other half — a take that was read is a take
nobody needs to find.

A delivery herdr refused therefore keeps its recording, although deleting it
could be argued for. That reply was built to carry the whole text precisely
because it reached neither the pane nor the screen (`docs/decisions.md`,
2026-08-26, #22), and the path it names is part of the same promise.

The shutdown path needs no check of its own: `finish_on_shutdown`
(`src/daemon.rs:550-566`) stops the recorder and journals the path, and that take
never reaches `transcribe_take`. Placing the deletion inside the delivery arm puts
it out of reach by construction rather than by something somebody has to remember.

## 11. Where the bound lives

### Context

The bound is today a method of `Records` (`src/record.rs:142`), which exists only
when `[record] transcripts` is on.

### Problem

With the key off, the four endings of section 10 are the only files in the
directory — and a bound that lives inside the recording feature does not run at
all. That is exactly the accumulation the widening is about.

### Decision

The bound becomes a free function over the directory:

```rust
pub fn bound(directory: &Path, in_hand: &Path) -> Vec<String>
```

`Records::write` no longer trims, and `Report` loses `not_removed`: the only
thing that put a value in it was the trim, and a field that is always empty is a
field somebody will one day believe.

`Runtime` gains `takes: PathBuf`, the directory itself, beside the
`records: Option<Records>` it already has.

**`bound` is called in `transcribe` (`src/daemon.rs:867-874`), beside the
`publish(runtime, Activity::Idle)` already there, and not inside
`transcribe_take`.** `transcribe_take` has two exits that return before any
record is written and before delivery is attempted — recognition unavailable
(`src/daemon.rs:886-894`) and transcription failed (`:898-906`) — and both leave a
`.wav` behind. A daemon whose `runtime.recognition` is `Err` is in a standing
configuration state rather than a transient one, so every take it serves takes the
first of those exits; a `bound` inside the delivery match would then never run at
all, and with the key off exactly those recordings would accumulate without limit.
That is the case the widening exists for.

### Why

A free function over a path has nothing to be switched off. `records:
Option<Records>` stays exactly as it is, so AC-5 remains a property of the type
rather than a branch: with the key off there is still no writer to call.

`transcribe` is where it goes for the reason its own comment already gives about
`publish`: "Published here rather than at each return so that a path added later
cannot forget it." Every way out of the pipeline is a take that ended, and a bound
on what the directory holds has to run on all of them or it is not a bound.

## 12. What the bound counts

### Context

`KEEP` is fifty and counted `.json` files. A record is tens of kilobytes; a
recording is not. Measured on this machine on 2026-09-18: eight takes, 3.4 MB,
between 205 KB and 717 KB each — 16 kHz mono 16-bit is 32 KB a second, so the
largest is about 22 seconds of speech.

### Problem

Two files now belong to one take, and a bound that counted files rather than
takes could remove a recording and leave its record describing audio that is
gone.

### Decision

The bound counts **takes, by stem**. It reads the directory, groups every `.json`
and `.wav` by file stem, keeps the newest fifty stems, and removes every file
belonging to an older one. `KEEP` stays at fifty. The stem in hand — the take
currently in the pipeline — is never removed, whatever it sorts as.

Two things `trim` established carry over to `bound` rather than being rediscovered:
entries that are not plain files are excluded, so a subdirectory named
`x.json` cannot take one of the fifty places and fail a removal on every take
thereafter; and the test that covers that,
`a_directory_named_like_a_record_is_not_counted_and_not_removed`, moves to `bound`
with it. Left where it is, it asserts through `write`'s `not_removed` and goes
vacuous the moment `write` stops trimming.

`file_stem` is the repository's existing definition (`src/record.rs:179-183`): a
name with two dots yields the longer stem and is simply its own take, and a name
with no extension is neither `.json` nor `.wav`, so it is neither counted nor
removed. Those two extensions are the only ones anything writes there —
`take_path` (`src/capture.rs:405-411`) and `Records::write_one`
(`src/record.rs:121`).

### Why

Counting stems is what makes "kept or removed together" true by construction
rather than by two rules that have to agree. Bytes were the alternative and are
worse: a byte budget deletes a different number of takes each time, so a person
cannot say how far back the record goes, and it still needs a stem rule to avoid
splitting a pair.

Fifty stays, and now means about 20 MB rather than tens of kilobytes. Eight takes
in one afternoon makes fifty roughly a week of dictation, and with the key off the
only files counted are those four endings left behind, which are rarer still. The
exclusion of the stem in hand is the same hazard `trim` already documents: a clock
stepping backwards names a take below everything in the directory, and without the
exclusion the bound would remove the take being worked on.

## 13. What this changes above

- **Section 4** said what removes the record. It now also removes the recording,
  and the bound is by stem rather than by `.json`.
- **Section 5** said the key decides whether a record is written. It decides two
  things now: whether the record is written, and whether a delivered take's
  recording survives. The consent the name carries is therefore wider than the
  documentation currently states.
- **Section 8** grows. `docs/design.md` section 7 has to say that with the key off
  a delivered take leaves nothing behind — recording included — and that the four
  endings that name a file keep it. The sentence "with it off nothing of a take's
  words is written to disk" is now too narrow: with it off, a take's words are not
  kept at all, in either form.

## 14. Tests for the widening

- **`src/record.rs`** — `bound` keeps the newest fifty stems and removes both
  files of an older one together; a stem with only a `.wav` and a stem with both
  are counted alike; the stem in hand is never removed even when it sorts first; a
  removal that fails is reported and the next oldest is tried.
- **`src/daemon.rs`** — with the key off, a delivered take leaves no `.wav`
  (AC-13); with the key on, it leaves both (AC-15); a take whose delivery is
  refused keeps its `.wav` with the key off (AC-14); a take that never reached
  delivery because recognition was unavailable keeps its `.wav` and still has
  `bound` run for it (AC-14 and AC-16 on the exit the placement of `bound` was
  decided for); a recording that cannot be removed does not end the take and
  leaves a `recording_kept_line` in the journal.

`record_not_removed_line` is renamed `take_not_removed_line` and reworded: it now
carries recordings as well as records, and a line named for one while reporting
the other is a line that misleads whoever reads it.

AC-14's other three endings return before delivery is attempted and are covered by
the existing tests of those returns, which assert the reply naming the path; the
deletion is not on those paths, so there is nothing new to assert about them.
AC-17 is read off the diff: the deletion sees only a `Take` that `stop_one`
returned, and `stop_one` never returns one it discarded.
