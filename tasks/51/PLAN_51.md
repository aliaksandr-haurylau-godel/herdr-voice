# PLAN 51 — a reply is delivered without backticks the take did not contain

All code is in `src/rewrite/http.rs`; all tests are in its `tests` module, which
already holds `respond_once`, `serve_once`, `body_of` and `message`. One cargo
process at a time, `CARGO_BUILD_JOBS=6`.

## T1 — the rule as a pure function

**Input:** `src/rewrite/http.rs` as on `main`.

**Do:** write the tests first, watch them fail to compile or fail, then add the
function above `impl HttpEngine` (after `user_message`, before `TIMEOUT`):

```rust
/// A backtick in the reply is the model's own unless the take carried one
/// first: with none in the take, every backtick is removed and the edges are
/// trimmed again; with one in the take the engine cannot tell the speaker's
/// from the model's, so the reply is only trimmed (`tasks/51/DESIGN_51.md`,
/// section 2).
fn without_added_backticks(transcript: &str, reply: &str) -> String {
    if transcript.contains('`') {
        return reply.trim().to_string();
    }
    reply.replace('`', "").trim().to_string()
}
```

Tests, each a `#[test]` calling the function directly, named by what holds:

| name | transcript | reply | expected |
|---|---|---|---|
| `a_term_wrapped_in_backticks_is_unwrapped` | `run cargo test` | ``run `cargo test` `` | `run cargo test` |
| `a_run_of_backticks_around_several_words_is_removed` | `open the file readme md now` | ``open `the file readme md` now`` | `open the file readme md now` |
| `a_fence_is_removed_and_the_edges_are_trimmed` | `foo` | a triple-backtick fence, newline, `foo`, newline, a triple-backtick fence | `foo` |
| `a_take_with_a_backtick_leaves_the_reply_as_it_is` | `say ` followed by a backtick followed by ` mid-sentence` | ``say `x` now`` with a leading newline and a trailing space | ``say `x` now`` (trimmed only) |
| `a_take_that_is_only_a_backtick_leaves_the_reply_as_it_is` | a single backtick | ``a `b` c`` | ``a `b` c`` |
| `only_the_backtick_is_removed` | `x` | ``"a" 'b' *c* ~d~ e_f - g `h` `` | ``"a" 'b' *c* ~d~ e_f - g h`` |
| `cyrillic_stays_whole_around_a_backticked_latin_term` | `запусти cargo test` | ``запусти `cargo test` сейчас`` | `запусти cargo test сейчас` |
| `a_reply_of_only_backticks_is_empty` | `x` | a run of three backtick characters and nothing else | the empty string |
| `a_reply_of_backticks_and_whitespace_is_empty` | `x` | a backtick, a space, a newline, a backtick | the empty string |

**Done when:** all nine pass with the function in place, and each failed before it.

## T2 — wire it in, and the socket-served tests

**Depends on:** T1.

**Do:** in the `Some(text)` arm of `HttpEngine::rewrite`, replace
`Ok(text.trim().to_string())` with `Ok(without_added_backticks(transcript, text))`.
Add a helper in the tests module and three tests:

```rust
fn rewrite_through_socket(take: &str, reply_json: &'static str) -> String {
    let (url, handle) = respond_once(reply_json);
    let engine = HttpEngine::new(url, String::new(), String::new());
    let out = engine.rewrite(take, "").expect("text");
    handle.join().expect("server thread");
    out
}
```

- `a_backticked_reply_is_delivered_without_them`: take `run cargo test`, reply JSON
  ``{"choices":[{"message":{"content":"run `cargo test`"}}]}`` as a raw string;
  expect `run cargo test`.
- `a_fenced_reply_is_delivered_as_its_text`: reply content is the three-backtick
  fence, `\n`, `foo`, `\n`, the fence, written as JSON escapes in a raw string;
  take `foo`; expect `foo`.
- `a_reply_for_a_take_with_a_backtick_is_delivered_with_them`: take
  ``say `x` ``, reply content ``say `x` `` ; expect it unchanged.

**Done when:** the existing test `the_rewritten_text_is_read_back` and the whole
module still pass, and the three new tests pass.

## T3 — through `rewrite::resolve` and the `Engine` trait

**Depends on:** T2.

**Do:** one test in the same module, `the_engine_the_daemon_holds_strips_the_backticks`:
start `respond_once` with the backticked reply of T2's first test, build
`crate::config::Rewrite { engine: "http".to_string(), url, ..Default::default() }`,
call `crate::rewrite::resolve(&config)`, match `Resolution::Engine(engine)` (any
other arm is a failed assertion with the arm printed), call
`engine.rewrite("run cargo test", "")`, take the `Ok` with `.expect("text")` (`EngineError` has no
`PartialEq`) and assert the string equals `run cargo test`.

**Done when:** it passes.

## T4 — evidence, and the four gates

**Depends on:** T2 and T3.

**Do:** run `CARGO_BUILD_JOBS=6 perl -e 'alarm shift; exec @ARGV' 600 cargo test
rewrite::http -- --nocapture`, read the whole output, and write a section "Repaired
terms and backticks, for issue #51" at the end of `docs/evidence.md`: the command,
the platform (macOS, this machine), the date, the names of the tests that ran and
their result, the case where the take contains a backtick, a sentence that no
herdr-pane run was made and why, and a sentence that the series on
`google/gemma-4-e4b` is pending. Then run these four gates, each fresh, with
`CARGO_BUILD_JOBS=6`, one cargo process at a time:

```sh
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt --check
python3 scripts/check_manifest.py
```

Then the Windows dead-code check, on a scratch copy and never in the worktree.
It rewrites the platform attributes so that macOS compiles the code CI compiles on
Windows, with warnings as errors. Its pass condition is that the last command
exits 0 with no warning:

```sh
W=$(mktemp -d) && rsync -a --exclude target --exclude .git ./ "$W/" && cd "$W" &&
find src -name '*.rs' -exec sed -i '' \
  -e 's/#\[cfg(all(test, unix))\]/#[cfg(any())]/g' \
  -e 's/#\[cfg(unix)\]/#[cfg(any())]/g' \
  -e 's/#\[cfg(not(unix))\]/#[cfg(not(any()))]/g' \
  -e 's/#\[cfg(windows)\]/#[cfg(not(any()))]/g' {} + &&
CARGO_TARGET_DIR="$W/target" cargo clippy --all-targets -- -D warnings
```

**Done when:** `cargo test`, `cargo clippy --all-targets -- -D warnings`,
`cargo fmt --check` and `python3 scripts/check_manifest.py` pass, the Windows check
passes, and the section exists with output beside every claim.
