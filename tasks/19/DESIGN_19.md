# DESIGN_19

## Decision

**A reply whose text contains a newline is carried whole, in a frame that
announces its length, exactly as the request already is. A reply without a
newline is written and read byte for byte as before.**

### Context

The client makes one connection, sends one frame and reads one reply. The daemon
composes the reply as a `Reply` (`Ok` or `Error`, each with a text), and
`Reply::write_to` writes it as a single line. The client's `Reply::read_from`
reads a single line. The texts come from many places in the daemon and from the
engines beneath it, and some of them contain a newline: the "for example" text of
a misconfigured engine, and the standard error of a transcriber that failed.
Replies are printed by the client as they are, so the person reading sees
whatever the reader returned.

### Problem

The writer emits every line of a multi-line text, the reader takes the first and
stops. The remainder is never read and the client exits with the code of the
first line, so the reply looks complete. Among the things lost is what follows the
text in the same message: the path of the kept recording. The defect is at the
protocol boundary, but the only place that has so far handled it is one call site
that flattens its own text before writing, which is how the sites that were not
flattened went unnoticed.

### Decision

The framing decides, not the call sites.

- Text without `\n`: `ok <text>\n` or `error <text>\n`, unchanged.
- Text with `\n`: a header line `ok+<n>\n` or `error+<n>\n`, followed by exactly
  `<n>` bytes of text, where `<n>` is the byte length of the text. No trailing
  newline is added to the body.
- The reader reads one line. If its first token is `ok` or `error`, it is the
  single-line form as today. If it is `ok+<digits>` or `error+<digits>`, the
  reader then reads exactly `<n>` bytes. Fewer bytes than announced is an error
  that names both numbers (the existing `ShortBody`); a body that is not valid
  UTF-8 is an error that says so. Both reach the person through the client's
  existing "the daemon spoke something unexpected" message and exit code 1.
- The body is read with `take(n)` and `read_to_end`, not into a buffer sized from
  the announced number, so a wrong number cannot allocate more than arrives.
- The daemon does not change how it composes any reply. The one place that
  already flattens newlines (a failed delivery, where the transcript is put in
  the reply) keeps doing so; that is a visible change of whitespace and the issue
  does not ask for it to change.

### Why

- Nothing is dropped and nothing depends on a call site remembering. Every
  current and future text is carried, whatever it contains. Flattening at each
  call site is the approach that has already failed once.
- Nothing relies on when a connection closes. Reading until the peer closes would
  also carry the text, but the daemon's handle is dropped after the reply and how
  a Windows pipe reports that to a reader cannot be verified on the machine this
  run has. A length the reader can check works the same on both platforms and
  also detects a reply cut off by a dying daemon, which reading to the end cannot.
- It matches the request frame, which is a header line plus a body of a promised
  length, so the protocol keeps one idea of how a body travels.
- Every reply written today is still valid: a reply without a newline is
  byte-identical, so the existing tests and the bytes for `holding for w1:p1`
  stay as they are (AC-6).

## Alternatives not chosen

**Refuse to send a reply that contains a newline.** The writer would return an
error, or send a fixed "this reply was refused" line, and the daemon would have
to flatten its own texts so that the useful ones still get through. This keeps the
wire as it is, but the useful texts still need flattening at every place they are
composed, which is the repair that was already insufficient; a text that is not
flattened is replaced by a message that hides it. The "for example" text would be
delivered, but as one long line that loses the layout it was written with.

**Read the reply until the connection closes.** Smaller change, rejected above for
depending on close behaviour and for not detecting a cut-off reply.

## Affected components

- `src/proto.rs`: `Reply::write_to` and `Reply::read_from`; one new
  `ProtoError` variant for a body that is not text. The module comment and the
  first line of `src/client.rs` say "one reply line" and are corrected to describe
  the two forms.
- `src/client.rs`: no code change. `outcome` and `send_to` already pass the reply
  text on, and `main` already prints a text that has several lines.
- `src/daemon.rs`: no code change in the reply path. Tests are added at the end
  of its test module only, to keep a later merge with other edits to that file
  small.

## Contracts

Wire, reply side. The request side (`voice/1`) does not change, and the protocol
token is not bumped: a token that is refused "by name" would turn a working
install into one that refuses everything, to guard against a pairing that only
arises when a daemon outlives an upgrade. In that pairing an older client reading
a multi-line reply sees `ok+<n>` with no space, refuses it as unexpected, and
prints so with exit code 1. That is a visible failure, not a silent one. An older
daemon with a newer client behaves as it does today.

## Coverage of the acceptance criteria

| AC | Covered by |
|---|---|
| AC-1 | The framing: a text with `\n` is read back equal to what was written; a cut-off body is an error with exit code 1 |
| AC-2 | A test through the real client and a real daemon over a socket, with a transcriber that writes two lines; the success reply names the target and the level |
| AC-3 | A test through the same pair with `engine = "command"` and an empty `command`: the client output contains `For example:`, the example line and the kept path |
| AC-4 | A test through the same pair with a transcriber that fails writing two lines to standard error: both lines and the kept path |
| AC-5 | A test that writes with `Reply::write_to` and reads with `Reply::read_from` over a real socket pair |
| AC-6 | A test that pins the bytes of `Reply::Ok("holding for w1:p1")` |
| AC-7 | The gates, written out in the plan |

## Risks

- A text that ends with a newline, a text that is only a newline, and an empty
  text: each must read back equal. They are cases for the tests.
- The announced length is in bytes and the text is UTF-8. A text with multi-byte
  characters and a newline is a case for the tests, since counting characters
  instead of bytes would pass with ASCII only.
- A reader meeting `ok+` followed by something that is not digits must refuse it
  by name rather than treat the line as a single-line reply.
