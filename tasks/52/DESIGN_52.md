# DESIGN_52 — failures say what actually failed

Covers AC-1 to AC-16 of `AC_52.md`. Citations for every statement about the
current code are in `DESIGN_52_evidence.md`.

## What changes

| Place | Change |
|---|---|
| `src/http_failure.rs` (new) | The four causes of an HTTP failure, how each is read out of a `ureq::Error`, the message each produces, and the bounded excerpt of a server's explanation. Used by both HTTP engines. |
| `src/rewrite/http.rs` | `HttpError::Failed` carries a cause instead of a pre-written detail. The engine takes its time bound as a parameter, with `TIMEOUT` as the value `new` passes. |
| `src/stt/http.rs` | Same as the rewrite engine. `HttpError::Refused` is removed; a failure to read the take's audio file gets its own variant. |
| `src/delivery.rs` | `DeliveryError` gains two variants; one pure function turns an `io::Error` from starting the program into the right variant. |
| `src/main.rs` | `mod http_failure;` |

Nothing else changes. `src/daemon.rs` constructs `DeliveryError::NotFound { binary,
path }` in one test; that variant keeps its name and fields, so the file is not
touched.

## The causes and their messages

An HTTP failure is one of four causes. Each message names the URL and ends in
what to do next.

| Cause | Message |
|---|---|
| Timed out | `{url} did not reply within {bound}. If the server is still loading a model, wait and try again` |
| Connection refused | `{url} refused the connection: nothing is listening at that address and port. Start the server, or correct the address in the configuration` |
| Answered with a status | `{url} answered with status {status} and refused the request: {explanation}. {advice}` |
| Any other transport failure | `cannot reach {url}: {detail}; check the server is running and the address is correct` |

The last row is the sentence that used to be printed for everything. It remains
for transport failures that are neither a timeout nor a refusal, such as a name
that does not resolve, where it names the right thing to check.

`{bound}` is the bound the engine was built with, written as whole seconds when
it is a whole number ("30 seconds", "1 second") and as milliseconds otherwise
("200 milliseconds").

`{advice}` depends on the status. For 408 and 429: "The server asked to be tried
again later; wait and try again." At 500 and above: "The server failed on its side; its
own log says why." For any other status below 500: "Correct the address, model or
token in the configuration." An empty explanation is written as "the server
gave no explanation" in place of `{explanation}`.

The time bound covers the whole request, connection included, so "did not reply
within" is true for a connection that never completes as well as for a reply that
never starts.

## Decisions

### Reading the cause from ureq

- **Context.** `ureq::Error` is either `Status(code, response)` or `Transport(t)`.
  A transport error has a kind and a chain of sources. ureq 2.12.1 builds a
  timeout as an `io::Error` of kind `TimedOut`, and normalises `WouldBlock` to
  it on Unix.
- **Problem.** The kind alone does not separate a timeout from a refusal: both
  can have kind `Io` or `ConnectionFailed`. Matching on message text would break
  when ureq rewords.
- **Decision.** Walk the error's `source()` chain to the first `io::Error` and
  decide from its `ErrorKind`: `TimedOut` or `WouldBlock` is a timeout,
  `ConnectionRefused` is a refusal, anything else is "other".
- **Why.** The `io::ErrorKind` is the operating system's own classification and
  does not depend on wording. A test produces each of the three with a real
  socket: a listener that never answers, and a port nothing listens on.

### The server's explanation

- **Context.** An OpenAI-compatible server puts the reason for a refusal in the
  response body, usually as JSON.
- **Problem.** The body can be arbitrarily large, not valid UTF-8, or contain
  newlines. The message reaches the run journal and a toast, both of which are
  single-line.
- **Decision.** Read at most 2048 bytes of the body. Convert lossily to text, replace
  every run of whitespace with one space, and keep at most 300 characters; when
  cut, end with `...`. A failure while reading the body leaves the explanation
  empty and does not replace the status.
- **Why.** Reading a bounded number of bytes bounds the work, not only the
  message. Counting characters, not bytes, keeps a Cyrillic body from being cut
  inside a character. UTF-8 uses at most four bytes per character, so 2048 bytes hold at least
  512 characters; a replacement character produced by a cut at the read limit
  lies beyond the 300 kept. The body is shown as the server sent it, not
  parsed: parsing a shape that differs between servers is a second way to lose
  the explanation.

### A time bound a test can set

- **Context.** The bound is the constant `TIMEOUT`, 30 seconds, in each engine.
  A test double that never answers would make each timeout test wait 30 seconds,
  and CI stops the test job at ten minutes.
- **Problem.** The shipped value must stay 30 seconds (changing it is out of
  scope), and the test must still exercise the real `ureq` timeout path.
- **Decision.** `HttpEngine::with_timeout(..., bound: Duration)` holds the bound
  in a field; `HttpEngine::new(...)` calls it with `TIMEOUT`. The message reads
  the field.
- **Why.** The field is the only thing a test changes; the code path from the
  timeout to the message is the one that ships. One test asserts that `new`
  stores `TIMEOUT` and that a message built from it says "30 seconds", so the
  shipped value is covered without waiting for it.

### Two engines, one set of causes

- **Context.** `src/rewrite/http.rs` and `src/stt/http.rs` each define an
  `HttpError` and each printed the same sentence. The transcriber's module
  states that it mirrors the rewrite engine's shape.
- **Problem.** Writing the four messages twice leaves two places to keep in step;
  the next change to one would be missed in the other, which is how the same
  defect came to be in both.
- **Decision.** The cause, the reading of a `ureq::Error` and the messages live in
  `src/http_failure.rs`. Each `HttpError` holds `{ url, cause }` for a failed
  request, and its `Display` calls the shared message.
- **Why.** One definition of each message. The two enums otherwise keep their
  own shape: `Unreadable` stays in both, and the transcriber keeps its
  `EngineError` wrapper.

### The transcriber's own file

- **Context.** The transcriber reads the take's audio file before it contacts the
  server. A read failure was reported through `HttpError::Refused`, which printed
  "cannot reach {url}" and "check the server is running".
- **Problem.** The server was never contacted; the advice sends the person to the
  wrong place.
- **Decision.** A new variant `AudioUnreadable { path, detail }` prints
  `cannot read the recording {path}: {detail}`. The `Refused` variant is removed, since
  no path produces it any more.
- **Why.** The message names the file, which is what the person has to look at. A
  variant nothing constructs is dead code, and CI compiles with `-D warnings`.

### Starting herdr

- **Context.** `HerdrDeliverer::run` turns any `io::Error` from starting the
  program into `DeliveryError::NotFound`.
- **Problem.** Only `ErrorKind::NotFound` means the program is not on the `PATH`.
  `PermissionDenied` means it was found and cannot be run. Anything else (a
  file still open for writing, exhausted processes) carries the operating
  system's own text and usually passes by trying again.
- **Decision.** A function `start_failure(binary, &io::Error) -> DeliveryError`
  maps `NotFound` to `NotFound { binary, path }` (unchanged), `PermissionDenied`
  to a new `NotExecutable { binary }`, and every other error to a new
  `StartFailed { binary, reason }` where `reason` is the `io::Error`'s text.
  Messages:
  - `NotFound`: unchanged.
  - `NotExecutable`: `cannot run {binary}: the file was found but this process is not
    allowed to run it. Make it executable (on Unix, chmod +x), or point
    HERDR_BIN_PATH at the herdr program itself`.
  - `StartFailed`: `cannot run {binary}: the operating system reported "{reason}".
    This is often temporary: try again, and if it keeps happening, report that
    text`.
- **Why.** `run` stays one match; the mapping is a pure function, so the cases a
  machine cannot produce on demand (a file still open for writing) are tested
  by giving the function an `io::Error`, and the two that can be produced are
  tested by starting a real program: a path that does not exist, and a file
  without the execute bit (Unix only; Windows decides execution by extension,
  so the fixture would not fail there).

## Testing

All tests run without a live endpoint or a live herdr.

- `src/http_failure.rs`: reading a cause from a timeout, a refusal and another
  transport error; the excerpt for empty, whitespace-only, multi-line, exactly at
  the limit, over the limit, and multi-byte text; the bound's wording for whole
  seconds, one second and milliseconds; the advice at 499 and 500.
- `src/rewrite/http.rs` and `src/stt/http.rs`: a listener that accepts and never
  answers, with a 200-millisecond bound (timeout); a port nothing listens on
  (refusal); 400 with a body, 404 with a body, 500 with a body, 400 with no
  body, and a body longer than the limit; `new` holds `TIMEOUT`. The
  transcriber adds the missing audio file.
- `src/delivery.rs`: a path that does not exist (`NotFound`, message keeps the
  `PATH` sentence); a file without the execute bit, Unix only (`NotExecutable`,
  message has no `PATH` sentence); `start_failure` given other errors
  (`StartFailed`, message carries the reason and no `PATH` sentence); a program
  that starts and exits with a failure (`Rejected`, content as before).
- Existing tests that match the removed or changed variants are updated to the new
  ones in the same change.

## What this does not cover

The 30-second bound, the flaky test behind #78, and the same defect in
`src/stt/command.rs`, `src/rewrite/command.rs`, `src/doctor.rs`,
`src/indicator.rs` and `src/setup.rs`. The pull request names them as remaining.

## Risks

- ureq's source chain for a timeout or a refusal may differ on Windows. Both are
  read from `io::ErrorKind`, which `std` normalises across platforms; a cause
  that is not recognised falls to the last row of the table, so the worst case
  is the old sentence for that one failure, not a wrong one.
- `Display` of the two `HttpError` enums changes text. Callers only print them
  (`src/rewrite.rs:29`, `src/stt.rs:78`); no caller matches on a variant.

## Amendment 2026-09-30 — the explanation on a 2xx response with no readable text

Covers AC-17 to AC-20 of `AC_52.md`. Evidence for the current behaviour is in the
amendment of that file.

### What changes

| Place | Change |
|---|---|
| `src/http_failure.rs` | One new public function, `body_note(text: &str) -> String`. |
| `src/rewrite/http.rs` | The 2xx body is read as text, then parsed; both unreadable outcomes carry `body_note`. |
| `src/stt/http.rs` | The same for the transcriber. |

### Decision: what the person is told

- **Context.** A server can refuse with a 2xx status and an error in the body. The
  engines read a 2xx body with `into_json`, which consumes the body: when it is not
  JSON, the text is gone, and when it is JSON without the expected field, the
  engine has no text to show.
- **Problem.** The unreadable-answer message names neither what the server said nor
  that it said anything.
- **Decision.** `body_note(text)` returns `the server said: {excerpt}` when the
  excerpt, built as for a non-2xx body (one line, at most 300 characters, cut with
  `...`), is not empty, and `the body was empty` otherwise. Both unreadable outcomes
  of each engine append it to their detail after a semicolon: `no
  choices[0].message.content string in the response body; the server said: {…}`, `no
  text string in the response body; …`, and, for a body that is not JSON, the parse
  error followed by the same note. The message keeps its start, `{url} answered with
  something this could not read`.
- **Why.** It is the same excerpt, bound and wording as the non-2xx path, through the
  same function, so the two paths cannot drift apart.

### Decision: reading the body

- **Context.** A completion can be longer than the excerpt bound, so the body cannot
  be read with the 2048-byte bound used for a refusal.
- **Problem.** The engine must still read a successful body in full, and must also
  keep its text for the note.
- **Decision.** Read the response's reader to a string with `read_to_string`, without a
  limit, then `serde_json::from_str` on that text. A read error becomes `Unreadable`
  with the read error as detail, as before.
- **Why.** `into_json` reads without a limit, and `into_string` stops at 10 MB with an
  error; using `into_string` would change how a large body is read. The text is held
  once, parsed once and, only when something is wrong, excerpted; a successful
  response is parsed as it was.

### Testing

- `src/http_failure.rs`: `body_note` for text, empty, blank, over the bound.
- `src/rewrite/http.rs` and `src/stt/http.rs`: a 2xx body that is an error object
  (`{"error":"Unexpected endpoint or method."}`), a body that is not JSON, valid JSON
  without the field (`{"choices":[]}`), an empty body, and a long body; each asserts
  the URL, the start of the message and the excerpt. The existing
  `an_unreadable_body_is_named_as_such` and `a_response_with_no_readable_content_is_a_failure`
  tests keep their variant assertions.
