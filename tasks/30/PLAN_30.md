# PLAN_30

Two tasks, each test-first. Names and wording are those in `DESIGN_30.md`.

Before each commit run these four commands from the worktree root; all must
succeed with no warnings (`CARGO_BUILD_JOBS=6`, one cargo process at a time):

```sh
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt --check
python3 scripts/check_manifest.py
```

And the Windows dead-code check, on a scratch copy, never in the worktree. Clean
means clippy exits 0 with no warning:

```sh
W=$(mktemp -d) && rsync -a --exclude target --exclude .git ./ "$W/" && cd "$W" &&
find src -name '*.rs' -exec sed -i '' \
  -e 's/#\[cfg(all(test, unix))\]/#[cfg(any())]/g' \
  -e 's/#\[cfg(unix)\]/#[cfg(any())]/g' \
  -e 's/#\[cfg(not(unix))\]/#[cfg(not(any()))]/g' \
  -e 's/#\[cfg(windows)\]/#[cfg(not(any()))]/g' {} + &&
CARGO_TARGET_DIR="${TMPDIR:-/tmp}/hv-30-target" cargo clippy --all-targets -- -D warnings
```

Before each commit also grep every file written for `<new_string>`,
`</new_string>`, `<old_string>`, `</old_string>` and line-start conflict markers.

## Task 1 — `src/repeat.rs` — depends on nothing

Add `mod repeat;` to `src/main.rs`, after `mod record;` (the list is
alphabetical). Create `src/repeat.rs` with a module comment, the type and the
function below, and a `#[cfg(test)] mod tests`. Write the tests first; they fail
to compile until the function exists.

```rust
#[derive(Debug, PartialEq, Eq)]
pub struct Repetition {
    pub times: usize,
    pub words: usize,
}

pub fn one_phrase_repeated(text: &str) -> Option<Repetition> {
    let words: Vec<String> = text
        .split_whitespace()
        .map(|piece| {
            piece
                .trim_matches(|c: char| !c.is_alphanumeric())
                .to_lowercase()
        })
        .filter(|word| !word.is_empty())
        .collect();
    let n = words.len();
    (1..=n / 3).find_map(|m| {
        (n % m == 0 && words.iter().enumerate().all(|(i, w)| *w == words[i % m]))
            .then_some(Repetition {
                times: n / m,
                words: m,
            })
    })
}
```

Tests, one `assert_eq!` each, names exact:

1. `the_example_from_the_issue_is_one_phrase_repeated_four_times` — text
   `"Продолжение следует... Продолжение следует... Продолжение следует... Продолжение следует..."`
   gives `Some(Repetition { times: 4, words: 2 })`.
2. `case_and_punctuation_do_not_matter` — `"Thank you. THANK YOU! thank, you"`
   gives `Some(Repetition { times: 3, words: 2 })`.
3. `one_word_three_times_is_flagged` — `"no no no"` gives `times: 3, words: 1`.
4. `the_smallest_block_is_reported` — `"a b a b a b a b"` gives `times: 4, words: 2`
   (not `times: 2, words: 4`).
5. `newlines_between_the_copies_do_not_matter` — `"thank you\nthank you\nthank you"`
   gives `times: 3, words: 2`.
6. `a_block_twice_is_not_flagged` — `"thank you thank you"` gives `None`.
7. `three_copies_with_another_word_are_not_flagged` — four texts, each `None`:
   `"so thank you thank you thank you"`, `"thank you so thank you thank you"`,
   `"thank you thank you thank you so"`, `"thank you thank you thank"`.
8. `ordinary_speech_that_repeats_a_word_is_not_flagged` — `"I said no no and then no"`
   gives `None`; so does `"it is what it is and that is what it is"`.
9. `three_different_words_are_not_flagged` — `"one two three"` gives `None`.
10. `empty_and_tiny_texts_are_not_flagged` — `""`, `"   "`, `"..."`, `"hello"`,
    `"hello hello"` each give `None`.
11. `a_dash_on_its_own_is_not_a_word` — `"yes — yes — yes"` gives
    `times: 3, words: 1`.

Done when the eleven tests pass and the whole suite is green. The function is
unused outside tests until task 2, so task 1 is not committed on its own;
clippy would reject the dead code.

## Task 2 — wiring into `transcribe_take` (`src/daemon.rs`) — depends on task 1

Tests first, in the `tests` module of `src/daemon.rs`, after
`a_delivered_take_keeps_its_recording_when_the_key_is_on`. Add one helper beside
`runtime_recording`:

```rust
/// A runtime that recognises `transcript`, with a deliverer the test can read
/// back, a journal it can read back, `[ui] toasts` as given, no records, and a
/// takes directory of its own that already holds the take's file.
fn runtime_flagging(
    transcript: &str,
    tag: &str,
    toasts: bool,
) -> (
    Runtime,
    crate::capture::Take,
    crate::delivery::tests_support::FakeDeliverer,
    std::sync::Arc<RecordingJournal>,
    std::path::PathBuf,
) {
    let dir = records_dir(tag);
    std::fs::remove_dir_all(&dir).ok();
    std::fs::create_dir_all(&dir).expect("create the directory");
    let fake = crate::delivery::tests_support::FakeDeliverer::ok();
    let (mut runtime, journal) = runtime_reading_back(fake.clone(), toasts);
    runtime.recognition = Ok(Box::new(crate::stt::tests_support::Fake(Ok(
        transcript.to_string()
    ))));
    runtime.takes = dir.clone();
    let take = take_for_recording(tag);
    std::fs::write(&take.path, b"audio").expect("write the recording");
    (runtime, take, fake, journal, dir)
}
```

with a constant `const REPEATED: &str = "Продолжение следует... Продолжение следует... Продолжение следует... Продолжение следует...";`.
`take_for_recording` gives target `wJ:pE`, level `-20.0`. Names exact:

1. `a_flagged_transcript_is_delivered_unchanged_and_its_take_is_kept` — transcribe
   `REPEATED`; the deliverer's calls contain a `Call::Insert("wJ:pE", REPEATED)`
   (the first element is the pane, the second the text, as the existing
   delivery tests assert); `take.path.exists()` is true.
2. `an_unflagged_transcript_still_removes_its_take` — transcribe
   `"fix the worklog entry"`; `take.path.exists()` is false; the reply is exactly
   `Reply::Ok("delivered to wJ:pE [-20.0 dB]".to_string())`.
3. `a_flagged_reply_is_the_delivery_reply_plus_the_warning` — the reply is
   `Reply::Ok(format!("delivered to wJ:pE [-20.0 dB]; probably not speech: the text is one phrase repeated 4 times, the way a transcriber fills silence. It was delivered; check it before sending. The take is kept at {}", take.path.display()))`.
4. `the_warning_in_the_reply_is_one_line_and_carries_no_transcript_text` — the
   reply of test 3 contains no `'\n'` and does not contain `Продолжение`; and
   `repetition_sentence(4, "a\nb.wav")` contains no `'\n'` and contains `a b.wav`.
5. `a_flagged_take_is_journalled` — the journal contains exactly one line equal
   to `format!("probably not speech: pane=wJ:pE repeats=4 block_words=2 take={}", take.path.display())`,
   and it comes after the `delivering: …` line.
6. `a_flagged_take_raises_a_toast_when_toasts_are_on` — with `toasts = true` the
   deliverer's calls end with `Call::Notify("Probably not speech", format!("wJ:pE: the text is one phrase repeated 4 times, the way a transcriber fills silence. It was delivered; check it before sending. The take is kept at {}", take.path.display()))`.
7. `with_toasts_off_the_warning_is_journalled_and_not_toasted` — with
   `toasts = false` the calls contain no `Call::Notify`; the journal line is there.
8. `an_unflagged_take_raises_nothing` — transcribe `"fix the worklog entry"` with
   toasts on; no `Call::Notify`; no journal line starting with `probably not speech`.
9. `a_flagged_take_whose_delivery_fails_gets_no_warning` — build the runtime with
   `FakeDeliverer::failing(…)`: copy how the existing test builds a failing
   delivery (search `FakeDeliverer::failing(` in `src/daemon.rs`), with the
   recognition and takes directory set as in `runtime_flagging`; the reply is
   `Reply::Error(…)` and does not contain `probably not speech`, the journal has
   no `probably not speech` line, and the file exists.
10. `a_flagged_take_with_records_on_is_kept_and_warned` — `runtime.records = Some(crate::record::Records::new(dir.clone()))`;
    the file exists and the reply contains `probably not speech`.

Then the code, in `src/daemon.rs`:

- Two pure functions near `delivering_line` (they are `pub` like that one only if a
  test outside the module needs them; keep them private):
  `fn repetition_sentence(times: usize, path: &str) -> String` returning
  `format!("the text is one phrase repeated {times} times, the way a transcriber fills silence. It was delivered; check it before sending. The take is kept at {path}")`;
  and `fn probably_not_speech_line(target: &str, repetition: &crate::repeat::Repetition, path: &str) -> String`
  returning `format!("probably not speech: pane={target} repeats={} block_words={} take={path}", repetition.times, repetition.words)`.
  Test 4 calls `repetition_sentence(4, "a\nb.wav")` and requires that it contains
  no newline, so `repetition_sentence` must collapse newlines in `path` itself:
  `path.replace('\n', " ")`; the journal line does the same.
- In `transcribe_take`, directly after the `let text = match engine.transcribe(…)`
  block: `let repetition = crate::repeat::one_phrase_repeated(&text);`.
- In the `Ok(())` branch of the delivery `match`: change
  `if runtime.records.is_none() {` to
  `if runtime.records.is_none() && repetition.is_none() {`.
- In the same branch, before the existing `(Reply::Ok(format!(…)), Reported::No)`,
  compute
  `let warning = repetition.as_ref().map(|repetition| { … })` that, when
  `repetition` is `Some`: builds `path = take.path.display().to_string()`; writes
  the journal line; calls `toast(runtime, "Probably not speech", &format!("{}: {}", take.target, sentence))`;
  and returns `sentence`. The reply is
  `format!("delivered to {} [{:.1} dB]", take.target, take.level_dbfs)` followed by
  `format!("; probably not speech: {sentence}")` when `warning` is `Some`.
  The `Err(why)` branch is not touched.

Done when the ten tests pass, the eleven of task 1 still pass, the whole suite is
green, and the four commands and the Windows check are clean.
