# RUN_19

| field | value |
|---|---|
| issue | #19 — A reply with a newline in it arrives truncated, silently |
| input | GitHub issue, read with `gh issue view 19` |
| stage | S1 |
| branch | fix/19-multiline-reply |
| opened | 2026-09-30 |

## Stages

<!-- One block per stage, appended, never rewritten. -->

### S1 Assess
- artifact: `AC_19.md`
- produced: 2026-09-30

Input came from GitHub: `gh issue view 19 --json ...` (issue #19, open, no
comments; the plain `--comments` form printed nothing because there are none).
The per-issue triage report (`## #19`) was checked against the code: the line
numbers hold, but its claim about the success format does not — the success reply
on `main` does not contain the transcript (see `AC_19.md`, as-is). `octoflow-assess`
ran as the skill states; the assessment was made from reading the code, not from
running it.

```yaml
gate:
  stage: S1
  artifact: AC_19.md
  reviewer: designer
  verdict: READY
  date: 2026-09-30
  blocker: null
```

Reviewer's note, not a gate question: AC-7 refers to "the Windows dead-code
check of the brief", and the run holds no brief. The plan (S3) will spell the
command out in full instead of editing `AC_19.md` after the verdict. The reviewer
had no `gh` access and checked the criteria against the triage report, not the
issue body; the issue body was read by the author with `gh issue view 19 --json`.

### S2 Design
- artifact: `DESIGN_19.md` (evidence for its claims in `DESIGN_19.evidence.md`)
- produced: 2026-09-30

The choice between carrying a multi-line reply whole and refusing to send one was
settled in the design: carried whole, in a frame that announces its length. The
orchestrator asked for it to be settled in S2 with the reasoning, so it was not
put to the owner.

```yaml
gate:
  stage: S2
  artifact: DESIGN_19.md
  reviewer: planner
  verdict: READY
  date: 2026-09-30
  blocker: null
```

Reviewer's note, not a gate question: the command of the Windows dead-code check
named in AC-7 is not spelled out in the design; the plan (S3) spells it out.

### S3 Plan
- artifact: `PLAN_19.md`
- produced: 2026-09-30

```yaml
gate:
  stage: S3
  artifact: PLAN_19.md
  reviewer: implementer
  verdict: READY
  date: 2026-09-30
  blocker: null
```

Reviewer's remarks, neither a gate question: the interface list of Task 2 names
`takes_dir(tag)`, which no Task 2 test uses; and a formatting fix found in Task 3
is committed as its own commit, because Tasks 1 and 2 are already committed by
then.

### S4 Implement
- artifact: code in `src/proto.rs`, `src/client.rs` (comment only) and tests in `src/daemon.rs`
- produced: 2026-09-30

Commits: `8664cd8` (framing, unit and socket tests, run artifacts), `da1299a`
(tests through the real client and the connection handler), then a separate
formatting commit.

Deviations from `PLAN_19.md`, none of them changing what it asks for:

- The plan gave `cargo test proto::tests client::tests`; `cargo test` takes one
  filter before `--`, and the package is a binary (no `--lib`). The command run was
  `cargo test -- proto::tests client::tests`. The plan text was corrected.
- The plan's marker grep held the literal editing-tool tags, and the pre-commit
  hook refused the commit for that ("editing debris"). The grep was rewritten with
  a regular expression that does not contain them. The hook was not bypassed.
- The red step of Task 1 was a failed build (`ProtoError::NotText` did not exist),
  not a failed assertion. Task 2 gave the behavioural red: against `src/proto.rs`
  from `main`, `ac3_...` ended at `...For example:` and `ac4_...` at
  `...model not found`; `ac2_...` passed, as the plan predicted, because the success
  reply on `main` does not carry the transcript.
- `cargo fmt --check` found three places in the added tests; `cargo fmt` changed only
  those, and they were committed on their own.

Gates, run fresh after the last commit (macOS, this machine):

```
cargo test                                   584 passed, 0 failed, 1 ignored; 2 passed (integration)
cargo clippy --all-targets -- -D warnings    no warning
cargo fmt --check                            fmt-ok
python3 scripts/check_manifest.py            manifest: 12 entries, all commands known
Windows dead-code check (brief)              clippy no warning; src restored, git status clean
```

No test failed intermittently, so no rerun was needed.

S4 code review (a fresh reviewer that did not write the code, over `git diff
13733c5..HEAD`, read-only). No blocker; no code path in the diff left that loses
text silently. Six findings, each read against the code before acting:

1. `ac2_...` passes against `src/proto.rs` from `main`. Known and stated in the
   plan: the success reply carries no transcript, so AC-2 is a regression guard,
   not evidence that the fix works. The tests that fail against `main` are the AC-3
   and AC-4 tests, the client test and the tests in `src/proto.rs`. No change.
2. The AC-4 test built its error by hand rather than through a real process.
   Fixed: a second test (`#[cfg(unix)]`, since it runs `sh`) goes through
   `CommandEngine` with a real process writing two lines to standard error. It
   fails against `main` (the message ends at `model not found`) and passes now.
   The hand-built test stays, because it also runs on Windows.
3. The refusal for a reply that is cut off or not text (`ShortBody`, `NotText`)
   reached the person as `the daemon spoke something unexpected: ...` with no next
   step, against the rule that a failure names what to do. Fixed in `c12339a`: the
   message now points at `herdr plugin log list --plugin herdr-voice` and says to
   restart the daemon if the log shows it stopped. A client built before this
   change, meeting a daemon built after it, prints `malformed header: "ok+57"` and
   cannot be changed; that pairing is described in `DESIGN_19.md`.
4. A single-line text that ends in `\r` loses it on reading (`trim_end_matches`
   strips `\r` and `\n`). It predates this change, the wire bytes of such a reply
   are unchanged as AC-6 requires, and no text is lost in practice. Not changed.
5. The test with `4_000_000_000` does not compile on a 32-bit target. The plugin
   is built for 64-bit macOS, Linux and Windows only. Not changed.
6. Outside the diff: `src/stt/command.rs:152` calls `String::truncate(400)` on the
   transcriber's standard error with no character-boundary check, which panics when
   byte 400 falls inside a multi-byte character (for example a long non-ASCII error
   message). It predates this change and is a panic path in the daemon. Not fixed
   here, because it is another defect; filed as issue #94, which also names the same
   line in `src/rewrite/command.rs`.

S4 mutation test (a fresh tester that did not write the code, in its own
worktree, at `e7554f5`; one mutation at a time, the full `cargo test` after each).
33 mutations of the lines `src/proto.rs` adds or changes: 27 killed, 6 survived.
No test failed intermittently, so nothing was rerun. The tester's table is the
evidence; the survivors and what was done:

- M08 (`digits.is_empty() ||` removed): equivalent. An empty string does not parse
  as `usize`, so `announced` returns `None` either way.
- M12 (`body.len() != length` to `<`): equivalent. `Read::take(length)` never
  returns more than `length` bytes.
- M25 (`digits.parse()` made to accept a leading `+`): equivalent while the all-digits
  check stands; it is killed together with M09 below.
- M19 (`w.flush()` removed from `Reply::write_to`): equivalent today. Every caller
  writes to an unbuffered stream or a `Vec`. The line predates this change.
- M09 (all-digits check removed, so `ok++3` read as length 3): a missing test.
  Killed by adding `ok++3` to `a_header_whose_length_is_not_digits_is_refused_as_a_bad_header`.
- M28 (text of the `NotText` refusal emptied): a missing test. Killed by
  `the_refusal_of_a_body_that_is_not_text_says_so`.
- M24 (`\r` no longer trimmed from the header line): a missing test on a line that
  predates this change. Killed by
  `a_header_line_that_ends_in_carriage_return_and_newline_is_still_read`.

The three new tests were run against their mutation by the author of the run, not
by a fresh tester: M09, M24 and M28 each fail the suite. There is no mutation
target in `src/client.rs` of the diff at `e7554f5`; the message added afterwards in
`c12339a` is covered by `a_reply_the_client_cannot_read_says_where_to_look` and was
not mutated by a fresh tester.

### S5 Verify
- artifact: a section in `docs/evidence.md`, "A reply with a newline in it, by hand on macOS, for issue #19"
- produced: 2026-09-30

Run on macOS with a daemon started from this worktree's build, with its own
configuration, state directory and socket, and a stand-in for `herdr`; the daemon that
was already running on the machine was not touched. The two replies that carry a newline
were run against a build of `main` and against the branch: on `main` the client printed
the first line only and exited 1 (the example, the second line of standard error and the
path of the kept recording were lost); on the branch the whole text arrived. A take with a
multi-line transcript delivered its eight lines to the stand-in and left the target and
the level in the client's output.

Negative results recorded there, not hidden: the first build of `main` was not `main`
(found by searching both binaries for strings that exist only in the new code, and
discarded); with the documented command `whisper-cli ... -np -nt` the output of a
38.7 second clip is one line, so the trigger the issue names was not reproduced with that
command; the success reply never carried the transcript, so AC-2 is a guard and not a
proof; herdr's own display of a multi-line message and Windows were not run.

```yaml
verdict:
  stage: S5
  artifact: docs/evidence.md
  verdict: READY
  date: 2026-09-30
  limits: the trigger named in the issue does not occur with the documented whisper-cli command; herdr's display and Windows were not run
```

## Notes

<!-- Anything a later stage needs and the artifacts do not carry. -->
