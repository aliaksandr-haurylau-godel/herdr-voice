# DESIGN_30

A new module `src/repeat.rs` decides whether a transcript is one phrase
repeated. `transcribe_take` in `src/daemon.rs` calls it on the raw transcript,
keeps the take's file when it answers yes, and raises the warning when the text
was delivered. Nothing in the engines, the recorder, the manifest, the client or
the configuration changes.

## Shape

### The check

```rust
// src/repeat.rs
#[derive(Debug, PartialEq, Eq)]
pub struct Repetition {
    /// How many times the block occurs; at least 3.
    pub times: usize,
    /// How many words the block has; at least 1.
    pub words: usize,
}

pub fn one_phrase_repeated(text: &str) -> Option<Repetition>
```

1. Split `text` on whitespace. For each piece, trim characters that are not
   letters or digits from both ends, and lower-case it with `to_lowercase`. A
   piece left empty (a dash on its own) is dropped. The result is `words`.
2. `n = words.len()`. Smallest block first, for `m` in `1..=n / 3`: when `n % m ==
   0` and `words[i] == words[i % m]` for every `i`, return
   `Repetition { times: n / m, words: m }`.
3. Otherwise `None`. This includes `n < 3`, and any text with a word that is not
   part of the repeating block, before, between or after.

The check reads the whole text and nothing else, so a block that is only the start
of the text, or a repetition that follows real speech, is not flagged (out of
scope in the AC).

### The pipeline

In `transcribe_take`, directly after `engine.transcribe` returns `text`:

```rust
let repetition = crate::repeat::one_phrase_repeated(&text);
```

`text` is the raw transcript; the rewrite step runs later and cannot change the
answer. In the success branch of delivery:

- the file removal (`if runtime.records.is_none()`) becomes
  `if runtime.records.is_none() && repetition.is_none()`;
- when `repetition` is `Some`, three things happen, in this order: the journal
  line, the toast, and the reply is the delivery reply with the warning added.

The delivery-failed branch is unchanged: it already keeps the take and names it,
and it prints the text, in which the repetition is plain.

### Wording

`path` is `take.path` with newlines replaced by spaces; `target` is `take.target`;
`n` is `times`.

- Sentence (shared by the reply and the toast): `the text is one phrase repeated
  {n} times, the way a transcriber fills silence. It was delivered; check it
  before sending. The take is kept at {path}`
- Reply: `delivered to {target} [{level} dB]; probably not speech: {sentence}`
- Toast: title `Probably not speech`, body `{target}: {sentence}`, through the
  existing `toast`, so `[ui] toasts` decides whether the person is interrupted.
- Journal: `probably not speech: pane={target} repeats={n} block_words={m}
  take={path}`. The text itself is already in the `delivering:` line before it.

The reply contains no newline and no text from the transcript.

### Why kept means kept only from deletion

The takes directory is still bounded by `record::bound`, which runs after every
take and removes the oldest beyond the limit. A flagged take is subject to it
exactly as a take kept by `[record] transcripts` is today.

## Decisions

### 1. Where the guard looks
- Context: a minute of room tone measured -54.4 dB, passed the -60 dB floor and
  came back as one phrase four times; room tone and quiet speech overlap in level,
  so no threshold separates them.
- Problem: any check on level either refuses real quiet speech or admits a noisy
  empty room.
- Decision: the check reads the raw transcript, not the level and not the
  rewritten text.
- Why: the repetition is a property of the output that needs no configuration
  and no microphone, and the rewrite step can rewrite or drop it.

### 2. What counts as one phrase repeated
- Context: ordinary speech repeats words; a transcriber filling silence repeats
  a whole phrase and nothing else.
- Problem: a rule that is too loose reports real speech and teaches the person to
  ignore the warning; a rule that is too tight misses the observed shape.
- Decision: the whole text, ignoring case and punctuation, is one block of one or
  more words repeated at least three times.
- Why: three is the smallest count that is a pattern rather than emphasis, and
  "whole text" excludes every sentence that merely contains a repeat. One word
  said three times alone ("no no no") is flagged; the cost is a warning on text
  that was delivered and whose take is kept.

### 3. What the warning does to the take
- Context: the issue bounds the change to reporting; refusing, deleting or
  blocking delivery is out.
- Problem: the take is the only evidence of what the microphone heard, and it is
  deleted after delivery unless `[record]` keeps it.
- Decision: the text is delivered as before, the take is kept, and the warning
  names it.
- Why: a flagged take that was real speech loses nothing, and one that was
  silence can be listened to.

### 4. When the warning is raised
- Context: a delivery can fail, and a hold has no reply to carry a message.
- Problem: a warning in only one place is missed by the other path.
- Decision: after a successful delivery, in the reply, the journal and (with
  toasts on) a toast; after a failed delivery nothing is added.
- Why: the failed-delivery reply already keeps the take, names it and prints the
  text, and a hold reads the journal and the toast.

## Tests

With the fake engine (`fake_runtime(text)`), `tone_recorder` and a recording
`FakeDeliverer`, no microphone and no model.

- `repeat`: the issue's text `Продолжение следует... Продолжение следует...
  Продолжение следует... Продолжение следует...` gives `times 4, words 2`;
  differences of case and punctuation do not matter; `no no no` gives `3, 1`;
  `thank you thank you thank you thank you` gives `4, 2`; a block twice is `None`;
  three copies with a word before, between or after is `None`; empty,
  whitespace-only and one-word texts are `None`; a text of three different words
  is `None`; ordinary speech that repeats a word (`I said no no and then
  no`) is `None`; text containing newlines between the copies is flagged.
- `daemon`: a flagged transcript is delivered with the same text as an
  unflagged one; its reply is the delivery reply plus the warning, with the count
  and the path and no newline; the take's file exists afterwards, and for an
  unflagged transcript it does not; the journal holds the warning line; with
  toasts on the deliverer records a toast titled `Probably not speech`, and with
  toasts off it does not and the journal line is still written; a flagged
  transcript whose delivery fails gets no warning added.
