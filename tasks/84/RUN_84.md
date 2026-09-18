# RUN 84 — what recognition produced is recorded, not only what the rewrite made of it

| field | value |
|---|---|
| issue | #84 — nothing records what recognition produced, only what the rewrite made of it |
| input | GitHub issue, read with `gh issue view 84`. The issue body is the ticket; it has no comments. |
| stage | S1 |
| branch | `feat/84-record-stages`, cut from `main` at `d83570c`, in its own worktree |
| opened | 2026-09-18 |

Every stage runs. This adds a configuration key and writes something the plugin
did not write before, so nothing here is an infrastructure skip.

## Stages

### S1 Assess

- artifact: `AC_84.md`, 12 criteria
- produced: 2026-09-18

Four things were established by reading the code or by running it, rather than by
taking the ticket's word for them.

**The code has four paths that deliver an unrewritten transcript, not the two the
ticket names.** `src/daemon.rs:918-937`: the stage switched off, an engine that did
not resolve, the skip heuristic, and a resolved engine whose call returned an
error. The fourth is the one where a rewrite was attempted and failed, which is
the most diagnostic of the four, so the criteria take all four rather than the
ticket's two. The reading is written at the end of `AC_84.md` rather than taken
quietly.

**The herdr plugin log cannot hold a per-take record, and this was measured rather
than reasoned.** `herdr plugin log list --limit 10000` returned exactly 200 records
with `log_id` running `plugin-log-3365` to `plugin-log-3564` — 200 consecutive ids,
no gap, shared across three installed plugins. Of those, 198 were this plugin's
`ptt` action, and 199 `ptt` records seen a moment earlier spanned 43 seconds of
wall clock, because `ptt` fires on every key auto-repeat. Holding a key for a
minute therefore overwrites the whole ring. This settles the destination question
against the plugin log without needing to know where the daemon's standard error
goes.

**What herdr does with a `[[startup]]` process's standard error was not
established, and the reason is recorded rather than guessed.** Observing it
requires restarting herdr, and the owner is dictating through it. A throwaway
plugin with its own id was linked and its `[[startup]]` entry did not run on
linking — no process appeared and no record was written — so a link alone cannot
produce the observation. The probe's *action* was invoked, which confirmed that
an action's stdout and stderr are captured verbatim into the ring; the probe was
unlinked afterwards and the owner's installation was not touched.

**The plugin already keeps every delivered take's audio with no retention rule.**
`std::fs::remove_file` on a take's path appears outside tests only at
`src/capture.rs:401` and `src/daemon.rs:665`, both on paths where the take was
refused or discarded. So S2 has no existing answer to copy for "what removes the
record" and has to choose one.

Gate, round 1:

```yaml
gate:
  stage: S1
  artifact: AC_84.md
  reviewer: designer
  verdict: READY
  date: 2026-09-18
  questions: []
  blocker: null
```

The reviewer opened every path and line range cited in `AC_84.md` and found no
off-by-one. One range was imprecise without being wrong and is corrected in the
artifact: the citation for `tell_once` being once per daemon is
`src/daemon.rs:1167-1185`, not `1160-1181` — the earlier span began inside the
doc comment of `rewrite_unavailable_line`, and still contained the
`runtime.told.swap` that carries the claim.

Three judgements the reviewer made and recorded:

- **AC-4 is designable.** It excludes two destinations and asks for one positive
  property, and the plugin already owns a state directory
  (`src/transport.rs:121`) and already has commands that print paths. Choosing a
  file there and having a command print it is a choice inside the criteria, not
  an invention.
- **The four-way reading is supported, not inflation.** The four arms are
  literally the four at `src/daemon.rs:918-937`, each distinguishable from state
  the daemon already holds at the point the record would be written, and AC-10
  is testable through the fake-engine seam the project already uses.
- **Nothing forces a guess.** Take uniqueness comes free from
  `{millis}-{pid}-{counter}`, and the one as-is item left unestablished — where
  herdr sends a `[[startup]]` process's standard error — is depended on by no
  criterion, because AC-4 excludes standard error as a destination whatever the
  answer turns out to be.

Two readings the reviewer states it will carry into the design rather than ask
about:

- The configuration key's name is the owner's call, so the design proposes one
  and marks it as such.
- AC-5 is scoped to the new record only. The existing `delivering:` line goes on
  carrying the delivered text on standard error with the key off, which is the
  only reading consistent with the out-of-scope note that the line is unchanged.

### S2 Design

- artifact: `DESIGN_84.md`
- produced: 2026-09-18

Two things the criteria left to this stage were settled here, and one was sent
out rather than settled.

**The destination is a file per take beside that take's recording**, at
`<state>/takes/{unix-millis}-{pid}-{counter}.json`, the same stem the `.wav`
already has, with `doctor` gaining a seventh line that says whether recording is
on and names the directory. The stem is already unique per take, so AC-3 costs
nothing, and `doctor` is the command this project already uses to answer "what is
missing and where is it".

**What removes it is a cap of fifty**, applied at the moment a record is written,
ordered by file name — which is ordering by time, because the stem begins with
thirteen digits of unix milliseconds. Fifty is a named constant, not a key, on the
footing `SKIP_WORD_LIMIT` is on. The audio is left exactly as it is; that the
plugin now bounds the text it keeps and not the audio is recorded as an
inconsistency rather than fixed, since audio is out of the ticket's bounds.

**The configuration key's name was sent to the owner**, with `[record] takes`
recommended and `[record] transcripts` as the alternative. The design is written
with `takes` and says plainly that the name is proposed: whichever comes back
changes two words here, two in `docs/design.md` section 7 and one field in
`src/config.rs`, and nothing else.

One structural consequence worth naming: the rewrite match at
`src/daemon.rs:918-937` moves into a function that returns a five-variant outcome
instead of the text. That is what makes AC-10 testable without a recorder, and it
also removes the shadowing that caused the ticket — after it, the transcript is a
binding nothing writes over.

Gate, round 1:

```yaml
gate:
  stage: S2
  artifact: DESIGN_84.md
  reviewer: planner
  verdict: READY
  date: 2026-09-18
  questions: []
  blocker: null
```

Every citation was opened and none was off by one in a way that changes a claim.
Two spans were imprecise and are corrected in the artifact: `StderrJournal` ends
at `src/daemon.rs:1028`, not 1030, and `SKIP_WORD_LIMIT` is `src/rewrite/skip.rs:13`
rather than a range beginning inside its doc comment.

Three things the reviewer flagged as choices rather than gaps are now settled in
the design rather than left to a task:

- **Where `tell_once` goes.** The extracted function takes `&Runtime` and carries
  both calls with it, from `src/daemon.rs:921` and `:931`. A pure function whose
  caller raises the notice would put it back into the caller, which is the shape
  the extraction exists to remove. `publish(Activity::Working { Fixing })` at
  `src/daemon.rs:910-917` stays where it is: only the match moves.
- **What counts toward the fifty.** Every file in the takes directory whose name
  ends in `.json`, ordered by file name. Nothing else in the tree writes a `.json`
  there.
- **The `doctor` line's state word.** `ok` when recording is on and `default` when
  it is off, never `Missing` — `Missing` is what makes `doctor` exit non-zero
  (`src/doctor.rs:92-94`), and a key sitting at its own default is not a fault.

### The configuration key, settled

The owner chose `[record] transcripts = false`. The collision this run raised —
`[context] source = "transcript"` already means the agent's conversation, in the
same configuration file — was put to him with the recommendation and he took
`transcripts` anyway, so the consent is read off the key's own name.

Two requirements follow from that choice and are written into the design rather
than carried as documentation taste:

- `docs/design.md` section 7 separates the two where the reader meets them, in the
  same breath: this key is about what the person said, `[context] source` is about
  what the agent said, and that one has never written anything.
- The key's own documentation says what is kept and where, because a name that
  carries consent only works when switching it on tells the person what they are
  switching on.

The design was also amended to carry the plugin-log measurement itself rather than
only its conclusion — the 200-record ring shared by every plugin, 198 of them
`ptt`, 43 seconds of wall clock — because the next person wanting a per-take
record will reach for that log too.

Gate, round 2: not re-run. The four amendments change a name, two citations and
two paragraphs of documentation requirement; no structure, no task boundary and
no dependency in the design moved, and the reviewer had already recorded the
rename as cheap and expected.

### S3 Plan

- artifact: `PLAN_84.md`, seven tasks
- produced: 2026-09-18

The plan is cut so that the two halves of the change can be written and judged
apart. Task 1 builds `src/record.rs` — the five-outcome type, the JSON document as
a pure function, and the writer with its cap — against seven tests that need no
daemon. Task 2 moves the rewrite match out of `transcribe_take` and has it return
the outcome instead of the text, which is testable on its own. Tasks 2 and 3 both
depend on task 1 and on nothing else; task 4 is where they join and is the first
task that writes a record on a real take path.

Two things in the plan are there because of what the reviewers asked at earlier
gates: the `.json` extension is what counts toward the fifty, stated in the
writer's own comment and asserted by a test that puts a `.wav` in the way; and the
`doctor` line is `ok` or `default` and never `Missing`, asserted through
`exit_code` so that a default installation keeps exit code 0.

Gate, round 1:

```yaml
gate:
  stage: S3
  artifact: PLAN_84.md
  reviewer: implementer
  verdict: READY
  date: 2026-09-18
  questions: []
  blocker: null
```

The reviewer checked every path, line number, symbol and helper the plan names
against the files, and type-checked the code blocks by hand. Everything held:
`src/daemon.rs:918-937` is the expression task 2 replaces with `text`, `bias`,
`runtime` and `take` in scope; every test helper exists with the name and
signature the plan gives it; `takes` is computed at `src/daemon.rs:1203-1205` and
only moved into `Recorder::spawn` at `:1229-1233`, so task 3's `takes.clone()`
has something to clone; `transcribe` returns the `(Reply, Reported)` task 4's
tests destructure; and no existing `doctor` test asserts how many findings `run`
produces, so task 5's conditional step needs no action. Nothing sits behind a
platform `cfg`, and no `unwrap` or `expect` appears outside `#[cfg(test)]`.

Two cosmetic errors were found and are corrected in the artifact:

- `mod record;` belongs between `mod ptt;` and `mod rewrite;` in `src/main.rs`.
  The plan gave an insertion point after `mod doctor;` and called it
  alphabetical, which it is not; the module list is strictly alphabetical.
- `docs/decisions.md` has three columns, `Decision | Basis | Where`, not two. The
  third is `2026-09-18, #84` on all three new rows.

### S4 Implement — round 1

- produced: 2026-09-18
- commits: `96ba0d2` the mechanism, `e0d8261` `doctor`'s seventh line, `d331283`
  the documentation

**One deviation from the plan, and its reason.** The plan cut tasks 1 to 4 into
four commits. That is not possible: the crate is a binary, `pub` does not exempt
an item from `dead_code`, and after task 1 alone `cargo clippy -- -D warnings`
reports six errors — `KEEP`, `Rewrite`, `document`, `Report`, `Records` and every
method of `Records` unused. Task 2 makes only `Rewrite` live and task 3 only
`Records::new`; everything is live only at task 4. The smallest green unit is
therefore tasks 1 to 4 together, and they are one commit. "Never commit red" wins
over "one commit per task".

Gate, round 1 — a review of the diff, before any pull request exists:

```yaml
gate:
  stage: S4
  artifact: git diff d83570c..HEAD
  reviewer: code
  verdict: QUESTIONS
  date: 2026-09-18
  questions:
    - a record could be deleted by the call that wrote it
    - doctor and the daemon disagreed about where records go
    - nothing tested the key itself
    - plan task 7 was not executed
    - the cap counted things that are not records
    - neither journal line named what to do next
    - a stale comment in doctor still said six lines
  blocker: null
```

Three were defects and are fixed in `132ef9f`:

- **A record could be deleted by the call that wrote it.** `trim` sorted every
  record and removed everything past the newest fifty without excluding the one
  just written, and `Report.written` still said `Ok`. A clock stepping backwards
  far enough — an NTP correction, a resumed suspend — names a take below every
  record already there, and the person finds nothing for the take they just made
  and no line saying why. `trim` now takes that path and never removes it, and
  counts down as removals succeed rather than slicing the front, so a removal
  that fails leaves the next oldest to be tried instead of leaving the directory
  over its cap.
- **`doctor` and the daemon disagreed about where records go.** With none of
  `HERDR_PLUGIN_STATE_DIR`, `XDG_STATE_HOME` or `HOME` set, the daemon wrote to a
  relative `takes` and `doctor` said there was nowhere to write — false, and
  against AC-4, the one criterion the destination was chosen for. Both now call
  `transport::takes_directory`, so they cannot disagree.
- **Nothing tested the key itself.** The mapping from `[record] transcripts` to a
  writer was three lines inside `start`; inverting it recorded every take with
  the key off and none with it on, and all 556 tests still passed. It is now
  `record::records_for`, tested in both directions — the inversion was applied
  and the test failed, then reverted — and the key-off take test builds its
  runtime through it and asserts on a directory that exists rather than on one
  that was never created.

Three notes were taken: a subdirectory named like a record no longer counts
toward the fifty or fails a removal on every take thereafter; both journal lines
name the next action; the stale `doctor` comment says seven.

One note was declined with a reason. The reviewer observed that two Russian test
fixtures could be digits instead, keeping the repository English. They match the
existing fixtures in `src/rewrite/skip.rs`, which are Russian for the same
reason — `skip::plain` refuses any text with two consecutive ASCII letters, so
the gate cannot be demonstrated in English — and a third convention for one test
is worse than the precedent.

Task 7 of the plan, committing the run's own artifacts, is done at the end of the
run rather than mid-way, so that the record of the run is complete when it lands.

### S1 Assess — round 2, the owner widened the ticket

- artifact: `AC_84.md`, now 17 criteria
- widened: 2026-09-18

The owner's decision: delete the recordings unless debug is on. The ticket's own
text puts audio out of bounds, and the criteria were gated against that text, so
this is a new round on the artifact rather than a quiet edit — the same way #42
handled a widening.

What the ticket now covers: with `[record] transcripts` off, which is the
default, a take whose text was delivered leaves no recording behind; with it on,
the recording and the record are both kept and both bounded.

Four things the wider criteria answer, and the measurement behind one of them.

**"Finished with" is delivery succeeding, and nothing else.** Four other endings
already promise somebody the file by path: recognition unavailable
(`src/daemon.rs:888-890`), transcription failed (`:899-902`), delivery refused
(`:990`) and the daemon stopping with the key down (`kept_on_shutdown_line`,
`:1195-1201`). Deleting on any of them turns a promise into a pointer at nothing.
A delivery herdr refused therefore keeps its recording, against the argument that
deleting it is defensible: that reply was built to carry the whole text precisely
because it reached neither the pane nor the screen (`docs/decisions.md`,
2026-08-26, #22), and the path it names is part of the same promise.

**The shutdown path is safe by construction, not by a check.** `finish_on_shutdown`
(`src/daemon.rs:551-565`) stops the recorder and journals the path; that take
never reaches `transcribe_take`. A deletion placed on the delivery-success arm
cannot touch it.

**The refusal paths keep sole responsibility for what they discard.**
`src/capture.rs:401` and `src/daemon.rs:665` already remove a file and neither
names a path to anybody. The new deletion is on a path where the file still
exists and no other code has an opinion about it, which is AC-17.

**The cap now carries audio.** Measured on this machine on 2026-09-18: eight
takes, 3.4 MB, between 205 KB and 717 KB each. 16 kHz mono 16-bit is 32 KB a
second, so the largest is about 22 seconds — and a 21.5-second take that
afternoon was 700 KB, which agrees. Whether the bound counts takes or bytes is
left to S2 rather than decided here, along with where the deletion sits.

The principle this rests on is already in the repository, at `src/capture.rs:398`
above `discard`: "A take nobody will read is a take nobody should find later."
What was missing is its other half — a take that was read, and is therefore
finished with.

Gate, round 2:

```yaml
gate:
  stage: S1
  artifact: AC_84.md
  reviewer: designer
  verdict: QUESTIONS
  date: 2026-09-18
  questions:
    - AC-16 does not say which files the bound covers, and the two readings collide with AC-14
  blocker: null
```

**The question, and the answer that resolved it.** The record is written before
the delivery attempt, so a take herdr refused has both a record and a recording,
while a take that failed recognition has only its `.wav`. Read one way, a bound
that removes "the recording and the record together" would eventually remove a
recording AC-14 says is kept; read the other way, the recordings the four endings
leave behind are never counted at all — and with the key off those are the only
files there is, so the bound would be false for the whole set.

The answer is that **the bound is a property of the takes directory, not of the
recording feature.** It covers both kinds of file, keyed by the take they belong
to, and it runs whether the key is on or off. AC-16 now says so, and AC-14 says
what it always meant: a take's own ending does not delete its recording. It never
promised the file is there for ever, and no message the plugin prints says it is.

One consequence goes to S2 rather than here: the bound can no longer be a method
of the record writer, because that type exists only when the key is on.

Two citations in the new material were off and are corrected in the artifact:
the second refusal path is `src/daemon.rs:669`, in `discard_take`, not 665; and
`kept_on_shutdown_line` is `src/daemon.rs:1200-1206`, not 1195-1201.

**The round-1 as-is was stale against this run's own S4**, which moved
`delivering_line`, the `Journal` trait and the take path. Rather than rewrite it
to the current tree, the as-is now says what it always meant: every path and line
in it is cited against `d83570c`, the commit this run was cut from. Checked: at
that commit `delivering_line` is at 1032-1034, the `Journal` trait at 1019, and
the `delivering:` write at 941, exactly as written. Rewriting it to today's tree
would erase the problem the ticket is about rather than record it.

Three things the reviewer established by walking the code, which S2 can rely on
rather than re-derive:

- **There are five endings in total and no sixth.** A `.wav` exists only from
  `stop_one`'s `wav::write` (`src/capture.rs:386`); the recorder's own refusals —
  device lost, level under the floor, the in-loop failure — all remove the file
  through `discard` before returning an error, and the too-short tap removes it
  at `src/daemon.rs:669`.
- **Cancel is not an ending.** `answer` handles it at `src/daemon.rs:165-168` by
  replying "nothing to cancel" and continuing; it never touches the recorder, so
  no file is written. That its doc comment at `src/daemon.rs:38-39` claims
  otherwise is a pre-existing contradiction, outside this ticket, and it does not
  reach the deletion because a cancelled take has nothing on disk.
- **One pre-existing hole, created by neither criterion.** If `wav::write` fails
  at `src/capture.rs:386`, `CaptureError::Unusable` is returned without
  `discard`, so a partly written file can be left with nothing removing it and no
  message naming it. Recorded, not fixed here.

Gate, round 3:

```yaml
gate:
  stage: S1
  artifact: AC_84.md
  reviewer: designer
  verdict: READY
  date: 2026-09-18
  questions: []
  blocker: null
```

Both corrected citations hold, and AC-14 and AC-16 were judged to say one thing
each: the unit of AC-14's promise is the ending itself, and AC-16 names its
subject — the take, the whole takes directory, both kinds of file, irrespective of
the key. The reviewer checked that AC-16 no longer competes with AC-15, that it
leaves AC-17 holding by construction because a bound that enumerates a directory
can only see files that exist, and that it is designable without inventing a
trigger or a schedule.

One bookkeeping inconsistency the reviewer raised, half of it right, and it was
measured rather than inferred — the reviewer said plainly it could not measure it
without a shell. `git show d83570c:src/daemon.rs` puts `remove_file` at 665, so
that citation did shift with S4 and both numbers are now given. But line 1195 at
that commit is `let listener = transport::listen(&address)?;` — `kept_on_shutdown_line`
was never there. That material was written in round 2, after S4, against the
working tree. The as-is header was therefore too broad and now says which part is
against which tree, and why: the round-1 material against the commit, because it
records the problem; the widening's material against the working tree, because it
is what the implementer edits.

One design hazard the reviewer carries into S2 rather than asking about: the bound
now covers recordings and runs whether the key is on or off, so it can run while
another connection's take is in the pipeline. A recording appears only at
`stop_one` and is the newest by name when it does, so only a clock stepping
backwards could put a live take's file inside the bound — the same hazard `trim`
already documents and handles by excluding the file it has just written.

### S2 Design — round 2, the widening

- artifact: `DESIGN_84.md`, sections 10 to 14 appended
- produced: 2026-09-18

Sections 1, 2, 3, 6, 7 and 9 stand unchanged; 4, 5 and 8 are amended by section 13.

**The deletion sits on the delivery-success arm and nowhere else.** That is the
only ending where the words arrived somewhere a person can see them and nothing
named a file. The shutdown path is out of reach by construction rather than by a
check: `finish_on_shutdown` (`src/daemon.rs:550-566`) journals the path and that
take never reaches `transcribe_take`.

**The bound leaves `Records` and becomes a free function over the directory.**
`Records::write` no longer trims; `Runtime` gains `takes: PathBuf` beside the
`records: Option<Records>` it already has, and `bound(directory, in_hand)` runs
once per take whether or not the key is on. A free function over a path has
nothing to be switched off, and `records: Option<Records>` stays exactly as it
is — so AC-5 remains a property of the type rather than a branch somebody can
forget.

**The bound counts takes by stem, not files and not bytes.** Grouping by stem is
what makes AC-16's "kept or removed together" true by construction instead of by
two rules that have to agree. Bytes were the alternative and are worse: a byte
budget deletes a different number of takes each time, so a person cannot say how
far back the record goes, and it still needs a stem rule to avoid splitting a
pair. `KEEP` stays at fifty, now meaning about 20 MB rather than tens of
kilobytes — eight takes in one afternoon makes fifty roughly a week.

The design carries the hazard the S1 reviewer raised: the stem in hand is never
removed, whatever it sorts as, which is the same clock-stepping-backwards case
`trim` already documents.

Gate, round 2:

```yaml
gate:
  stage: S2
  artifact: DESIGN_84.md
  reviewer: planner
  verdict: QUESTIONS
  date: 2026-09-18
  questions:
    - which exits of the take path call bound; the design named no call site
  blocker: null
```

**The question, and the answer.** `bound` is called in `transcribe`
(`src/daemon.rs:867-874`), beside the `publish(runtime, Activity::Idle)` already
there, and not inside `transcribe_take`. That function has two exits which return
before any record is written and before delivery is attempted — recognition
unavailable (`:886-894`) and transcription failed (`:898-906`) — and both leave a
`.wav` behind. A daemon whose `runtime.recognition` is `Err` is in a standing
configuration state rather than a transient one, so every take it serves takes the
first of those exits; a `bound` inside the delivery match would then never run
while exactly those recordings accumulated. The reason is already written in
`transcribe`'s own comment about `publish`: "Published here rather than at each
return so that a path added later cannot forget it."

**The sharpest thing in the report was not the question.** The test
`a_directory_named_like_a_record_is_not_counted_and_not_removed` asserts through
`write`'s `not_removed`, and goes vacuous the moment `write` stops trimming — so
the defect the S4 gate found an hour earlier would have lost its cover silently.
The design now says the test moves with the behaviour, along with the non-file
exclusion it covers.

Five things moved from "a task would decide this" into the design:

- `Report.not_removed` is dropped. The only thing that put a value in it was the
  trim, and a field that is always empty is a field somebody will one day believe.
- `recording_kept_line(path, why)` is named, with what it says.
- The condition for removing a recording is `runtime.records.is_none()`, stated in
  section 10 rather than implied by section 13.
- `record_not_removed_line` becomes `take_not_removed_line`: it now carries
  recordings as well as records, and a line named for one while reporting the
  other misleads whoever reads it.
- `file_stem` semantics are stated for a name with two dots, a name with none, and
  the closed set of extensions anything writes there.

Section 4's citation of the second `remove_file` was stale after this run's own
S4 and is corrected to `src/daemon.rs:669`.

Gate, round 3:

```yaml
gate:
  stage: S2
  artifact: DESIGN_84.md
  reviewer: planner
  verdict: READY
  date: 2026-09-18
  questions: []
  blocker: null
```

The placement is checkable by someone other than its author: section 14's test —
a take that never reached delivery because recognition was unavailable keeps its
`.wav` and still has `bound` run for it — is what fails if the call is later moved
into the delivery match. The reviewer walked all four call sites of
`Report.not_removed` and found each a stated destination, and confirmed nothing in
the widening is reachable only from a `#[cfg(unix)]` path.

### S3 Plan — round 2, the widening

- artifact: `PLAN_84.md`, tasks 8 to 11 appended
- produced: 2026-09-18

Four tasks, cut the way the S2 reviewer said they could be. Task 8 writes `bound`
in `src/record.rs`, takes the trim out of `Records::write` and drops
`Report.not_removed`; task 9 puts the directory on `Runtime` and calls `bound` in
`transcribe`; task 10 removes a delivered take's recording on the
delivery-success arm; task 11 is documentation and two decisions.

Tasks 8 and 9 land in one commit, for the same reason tasks 1 to 4 did: task 8
alone leaves `src/daemon.rs` referring to a field that no longer exists, so the
tree is red at its own commit.

Three tests in `src/record.rs` are deleted rather than kept. All three drive the
trim through `Records::write` and go vacuous the moment `write` stops trimming —
including the one that covers the subdirectory defect the S4 gate found. Their
five replacements assert against `bound` directly.

Gate, round 2:

```yaml
gate:
  stage: S3
  artifact: PLAN_84.md
  reviewer: implementer
  verdict: QUESTIONS
  date: 2026-09-18
  questions:
    - task 9 step 3 does not compile: takes is moved into Recorder::spawn before the Runtime literal
  blocker: null
```

The question was right and the plan was wrong. `takes` is bound at
`src/daemon.rs:1263` and moved by value into `Recorder::spawn` at `:1288-1292` —
its signature takes a `PathBuf`, not a reference (`src/capture.rs:196`) — and the
`Runtime` literal at `:1304` is after that move, so `takes.clone()` there is a use
of a moved value. The clone now goes at the call and the literal takes the
original. The task also miscounted the literals: five in all — `start` at `:1304`,
one in `tests_support` at `:1487`, three in `mod tests` at `:1546`, `:1687` and
`:2160`.

Gate, round 3:

```yaml
gate:
  stage: S3
  artifact: PLAN_84.md
  reviewer: implementer
  verdict: READY
  date: 2026-09-18
  questions: []
  blocker: null
```

Everything else was traced and holds: `the_take_in_hand_is_never_removed_even_when_it_sorts_first`
genuinely exercises the skip rather than passing on the fixture's shape;
`a_take_that_never_reached_delivery_still_has_the_bound_run_for_it` does fail if
`bound` is moved into the delivery match, because recognition being `Err` returns
before that match is reached; `BTreeMap<String, ..>` iterates in byte order, which
is chronological for a fixed-width thirteen-digit millisecond prefix; `stem` needs
no visibility change because the tests are a descendant module; and the three
deleted tests exist under the names given.

One inaccuracy was found in the coverage table and is corrected rather than
defended. It claimed AC-14's other three endings were covered by existing tests
asserting the reply names the path. They are not: no test asserts the file still
exists on those paths. A test for the recognition-unavailable ending is added to
task 10, and the table now says plainly that the remaining two — transcription
failing and the daemon stopping with the key down — hold by construction and have
no test.

## Notes

Two questions the design has to answer and the criteria deliberately do not:
where the record goes (AC-4 states the property, not the destination), and what
removes it (AC-7). Naming the configuration key is the owner's, not this run's:
it is a user-visible name.
