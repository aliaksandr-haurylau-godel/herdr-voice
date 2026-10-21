# DESIGN 51 — a reply is delivered without backticks the take did not contain

## 1. Where the change goes

**Context.** `HttpEngine::rewrite` (`src/rewrite/http.rs:162`) ends with
`Some(text) => Ok(text.trim().to_string())` (`:209-210`). The daemon delivers
whatever it returns (`src/daemon.rs:1227-1229`).

**Problem.** The reply is the only place the markup can be removed that every
delivery path shares, and the engine is the only place that holds both the take
and the reply.

**Decision.** A private function `without_added_backticks(transcript: &str,
reply: &str) -> String` in `src/rewrite/http.rs`, called in the `Some(text)` arm
in place of the bare trim.

**Why.** The `command` engine runs the person's own program and is not touched
(the reading under "Ambiguities" in `AC_51.md`). Nothing in `src/daemon.rs` or the `Engine` trait
changes, so no other caller can start receiving different text.

## 2. The boundary between the speaker's characters and the model's

**Context.** The reply can carry a backtick the speaker dictated only if the take
carried one first. The engine receives the take.

**Problem.** If the take contains a backtick, a backtick in the reply may be the
speaker's, and the engine cannot tell which.

**Decision.** If the take contains the character U+0060 anywhere, the reply is
returned trimmed and otherwise unchanged. If it contains none, every U+0060 is
removed from the reply and the result is trimmed again.

**Why.** With no backtick in the take, every backtick in the reply was added by
the model, whatever shape it has: a pair around one term, a run around several
words, a fence, a single stray mark from a truncated reply. A rule that removes
only matched pairs would leave the stray one. With a backtick in the take, doing
nothing is the only rule that cannot remove a dictated character.

## 3. What is removed

**Context.** Models also add other marks: asterisks for emphasis, quotation
marks, a language tag after a fence.

**Problem.** Each of those is a character the speaker might have dictated, or one
this issue does not name.

**Decision.** Only U+0060 is removed, and edge whitespace through the second
trim. Look-alike characters (U+00B4, U+2018, U+2019) are left alone.

**Why.** The issue names backticks. A wider set widens the chance of removing a
dictated character, which is the one thing the issue forbids.

## 4. The prompt

**Context.** The issue leaves open whether the prompt or a strip is the fix, and
says a prompt instruction is not enforcement.

**Problem.** A line in the prompt changes what is sent to the owner's model by
default, and can change how it answers the #76 takes.

**Decision.** The prompt is unchanged.

**Why.** The strip is enforcement and the prompt line is not; the strip needs no
model measurement to be correct, and a prompt change would need the series on the
model the owner runs, which is not available.

## 5. Tests

Each test serves a recorded reply from a listener on a real socket
(`serve_once` in the module's tests) and calls `HttpEngine::rewrite`. A pure
function holds the logic, so most cases are also asserted on it directly.

| criterion | take | recorded reply | expected |
|---|---|---|---|
| AC-1 | `run cargo test` | ``run `cargo test` `` | `run cargo test` |
| AC-2 | no backtick | ``open `the file readme md` now`` | `open the file readme md now` |
| AC-2 | no backtick | a triple-backtick fenced reply with a newline at each edge | `foo` |
| AC-3 | contains a backtick | a reply with backticks | the reply, trimmed, unchanged |
| AC-4 | no backtick | `"a" 'b' *c* ~d~ e_f - g` around a backticked term | everything but the backticks unchanged |
| AC-5 | Cyrillic, no backtick | Cyrillic with a backticked Latin term | Cyrillic whole, backticks gone |
| AC-6 | no backtick | only backticks, and backticks with whitespace | the empty string |

AC-3 is tested with a take that is only a backtick, and with one that carries a
backtick in the middle of a sentence.

## 6. Evidence

One more test goes through the construction path production uses: a
configuration with `engine = "http"` and the listener's URL, passed to
`rewrite::resolve` (`src/rewrite.rs`), and the engine it yields called through the
`Engine` trait the daemon calls (`src/daemon.rs:1227`, inside `resolve_rewrite`). It shows the strip is
applied to the engine the daemon actually holds, not only to `HttpEngine` built
by hand.

`docs/evidence.md` gets a section for #51 with the command that runs the tests of
this change, its whole output, the platform (macOS, this machine) and the date,
and a case where the take contains a backtick. A run in a herdr pane needs a
microphone and the owner's session, so it is not made here; the section says so,
and says the live series on `google/gemma-4-e4b` is pending.

## Risks

A model that wraps a term in a backtick where the speaker also dictated one
elsewhere in the take is delivered unstripped (section 2). That is the stated
price of the boundary.
