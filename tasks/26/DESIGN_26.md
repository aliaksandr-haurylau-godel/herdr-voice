# DESIGN_26

Design for issue #26, against `tasks/26/AC_26.md` (gate S1: READY,
`tasks/26/RUN_26.md`).

## 1. The one decision this issue has to make

`Engine::transcribe` widens to take the bias string as a second parameter:

```rust
pub trait Engine: Send + Sync {
    fn transcribe(&self, audio: &Path, bias: &str) -> Result<String, EngineError>;
}
```

Not "the string arrives some other way." The module's own doc comment
(`src/stt.rs:1-6`) already states the reasoning this decision follows:
"Everything an engine needs beyond the path — the model, the language, the
argument list — is given to it when it is built, because the path is the only
thing that changes between takes." Bias is per-take too — it depends on the
pinned pane (`tasks/21/DESIGN_21.md`, §9) exactly the way the audio path
depends on the take — so it joins `audio` as a parameter for the same reason
audio is one, rather than becoming baked-in construction state the way the
model and the language are. That doc comment is corrected as part of this
issue's diff, since "the path is the only thing that changes between takes" is
no longer true once this lands.

### Alternatives considered

**Interior mutability — a `set_bias`/`Cell` on the engine, `transcribe`'s
signature unchanged.** Rejected. It reintroduces the ordering bug the plan for
#21 already found a version of once (`docs/design.md`'s account of the same
class of mistake in `CommandEngine::new`, "argv is kept unrendered ... that
mistake was made once here and caught by the test below"): a caller must
remember to call the setter before every `transcribe`, and nothing in the type
system enforces it. It also has no answer for `Send + Sync` without a lock
around a plain `String` field, for no benefit over a parameter.

**Daemon builds the final command line itself, bypassing the trait for the
`command` engine specifically.** Rejected. `Runtime.recognition` is a
`Recognition`, `type Recognition = Result<Box<dyn Engine + Send + Sync>, String>`
(`src/daemon.rs:47`, field at `52`, built at `451`), precisely so the daemon
does not know which engine it holds. A path that
only works for `command` and leaves `candle`/`http` (#15, #16) to invent their
own would be exactly the retrofit AC_26's requirement 4 exists to avoid.

**Only `CommandEngine` widens; the trait keeps one parameter and a second trait
method carries the bias.** Rejected as needless: nothing in the two engines
that do not exist yet, or in the fake test double, suggests a bias-optional
engine is a real category — every measured recognition path takes a prompt of
some kind (`docs/evidence.md`, "Recognition, by hand on macOS"). Two methods
would make every future `Engine` implementor decide what to do when only one
is called, for no case the codebase has today.

## 2. Affected components

| File | Change |
|---|---|
| `src/stt.rs` | `Engine::transcribe` gains `bias: &str`. Doc comment at the top corrected (bias is now a second thing that varies per take). `tests_support::Fake` gains the parameter; see §4. |
| `src/stt/command.rs` | `CommandEngine::transcribe` passes `bias` through. `render` gains `bias: &str` and substitutes `{prompt}` by name, the same way `{model}` and `{language}` are substituted — no forced append (AC-3). |
| `src/daemon.rs` | `dictate` keeps `take_bias`'s return (currently discarded, `src/daemon.rs:159-164`) and passes `&collected.bias` to `transcribe`. `transcribe` gains a `bias: &str` parameter and passes it to `engine.transcribe(&take.path, bias)`. |
| `docs/design.md` | The `{audio}`/`{model}`/`{language}` placeholder list gains `{prompt}`, stated the same way the other three are. |
| `docs/decisions.md` | This section's decision (trait widens, no forced append) recorded in the four-part form, since it is mine to make and record per the project's standing delegation. |

Nothing else implements `Engine` today — `resolve_with` (`src/stt.rs:105-135`)
returns `EngineError::NotBuilt` for `candle` and `http` directly, without
constructing a value of either. The blast radius is exactly the two production
files above plus the daemon's threading, matching AC_26's as-is section.

## 3. Contract: `command::render`

```rust
pub fn render(
    argv: &[String],
    audio: &Path,
    model: Option<&Path>,
    language: &str,
    bias: &str,
) -> Vec<String>
```

`{prompt}` is substituted by name, in every argument that contains it, the same
loop that already handles `{model}` and `{language}` (`src/stt/command.rs:15-34`
today). Unlike `{audio}`, a missing `{prompt}` placeholder does **not** cause
`bias` to be appended to the rendered list — AC-3's chosen reading, carried
from `AC_26.md` unchanged: `{audio}` is force-appended because there is no
other way to invoke a transcriber at all without the audio path reaching it
somehow, while a bias string is an enhancement a person opts into by writing
the placeholder, not an argument the program cannot run without. An empty
`bias` (AC-4) substitutes to an empty string, which is ordinary text
substitution — no special case needed.

## 4. Test doubles

`tests_support::Fake` (`src/stt.rs:143-152`) gains the `bias: &str` parameter
and ignores it, matching its existing treatment of `_audio` — it already
returns a canned `Result` regardless of what it is asked to transcribe. It is
constructed at three call sites in `src/daemon.rs`'s test module (`565`,
`653`, `775`), each of which the widened signature reaches mechanically. A
fourth site, `src/daemon.rs:842`, calls the private `transcribe` function
directly and needs its own call site updated the same way — noted by the S2
gate so the plan does not miss it.

AC-1 asks for something end-to-end observable: that the *real* collected bias
string reaches the engine for a real take, not only that `command::render`
substitutes a placeholder in isolation. `command::render` is a pure function
(`src/stt/command.rs:15-34`), so a unit test on it already proves AC-2/AC-3/
AC-4/AC-5 directly, by inspecting its return value. Proving AC-1 at the
`daemon::dictate` level needs a test double that *captures* what it was called
with rather than only replaying a canned answer — the plan (S3) names the
exact mechanism (a `Fake` variant or a second recording double); this design
only states the requirement: something in `src/daemon.rs`'s test module must
be able to assert which `bias` string reached `engine.transcribe`, the same
way `bias::tests`'s scripts today assert which arguments reached a fake
`herdr` (`tasks/21/PLAN_21.md`, Task 10's tests).

## 5. Risk: this issue does not add a fourth way to leak the bias string

Issue #21 established that the collected string reaches no log or file beyond
the take (AC-9). This issue does not touch that: nothing here journals a
rendered argument list, and neither `CommandError`'s variants
(`src/stt/command.rs`, `NotFound`/`Failed`/`Silent`) nor `EngineError`'s carry
`argv` — `Failed` carries the called program's own `stderr`, which is a
pre-existing surface (a badly-behaved transcriber could in principle echo back
what it was invoked with) and is unchanged and out of scope here, exactly as
noted by the S1 gate reviewer (`tasks/26/RUN_26.md`). No new logging is added
by this design, and none should be added later without re-checking this rule.

## 6. Coverage against AC_26.md

| AC | Covered by |
|---|---|
| AC-1 | §1 (the trait carries the real string), §4 (a daemon-level test observes it) |
| AC-2 | §3 (`{prompt}` substituted the same way as `{model}`/`{language}`) |
| AC-3 | §3 (no forced append; reading carried unchanged from `AC_26.md`) |
| AC-4 | §3 (empty bias is ordinary substitution) |
| AC-5 | Nothing in §2/§3 adds a cap; the plan is not asked to add one |
| AC-6 | §3 (a list with no `{prompt}` behaves exactly as before) |
| AC-7 | Not a design concern — needs the plan to schedule a manual take and an evidence entry, the same shape `tasks/21/PLAN_21.md`'s S5 task used |
| AC-8 | §5 (no failure path's reporting changes) |

## What this design does not decide

The exact shape of the daemon-level capturing test double (§4) and the wording
of the `docs/decisions.md` entry are left to the plan and the implementation —
both are mechanical once the contract above is fixed, and deciding them here
would be the kind of premature specificity `tasks/21/DESIGN_21.md`'s own S2
gate pushed back on when a design tried to fix an interface a later stage
should choose.
