# PLAN_26 — Pass the bias string to the engine, so the terms actually come back

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `Engine::transcribe` carries the bias string `bias::collect` already
assembles, so the `command` engine can substitute it into `{prompt}`, and a
spoken take proves an English term inside Russian speech comes back as the
term rather than a near-homophone.

**Architecture:** `Engine::transcribe` gains a second parameter, `bias: &str`,
following the audio path's own precedent — both vary per take, so both are
parameters rather than construction-time state. `command::render` substitutes
`{prompt}` by name, with no forced append when the placeholder is absent.
`daemon::dictate` stops discarding `take_bias`'s return and threads `.bias`
into `transcribe`, which threads it into `engine.transcribe`.

**Tech Stack:** Rust, `std::process::Command`, the existing `cargo test` /
`clippy` / `fmt` / manifest gates.

**Spec:** `tasks/26/DESIGN_26.md`, against `tasks/26/AC_26.md` (S1: READY, S2:
READY — `tasks/26/RUN_26.md`).

## Global Constraints

- No forced append of the bias string when `{prompt}` is absent from `[stt]
  command` — `{audio}` is force-appended because a transcriber cannot run at
  all without it; a bias string is an opt-in enhancement (AC-3, DESIGN_26.md
  §3).
- No new cap on the bias string. `[context] prompt_chars` (default 600,
  `tasks/21/AC_21.md`) is the only one (AC-5).
- No new logging of a rendered argument list or the bias string.
  `bias_line`/`bias_refused_line` (`src/daemon.rs`) are the only lines built
  from a `Collected`, and this issue adds no third one (AC-9 of #21, unchanged
  by this issue — DESIGN_26.md §5).
- Everything in the repository is in English: code, comments, output strings,
  commit messages (`CLAUDE.md`).
- Test first: write the failing test, confirm it fails for the stated reason,
  then implement.
- One commit per task.

---

### Task 1: Widen `Engine::transcribe` and `command::render` to carry the bias string

**Files:**
- Modify: `src/stt.rs:1-6` (module doc comment), `:17` (trait method), `:143-152`
  (`tests_support::Fake`)
- Modify: `src/stt/command.rs:15` (`render`), `:107-109` (`CommandEngine::transcribe`),
  `:157-235`+ (existing tests — every call to `render(...)` and
  `engine.transcribe(...)` in this file gains an argument)

**Before starting:** read `src/stt/command.rs:1-40` for `render`'s existing
substitution loop (`{audio}`, `{model}`, `{language}`, in that order) and the
force-append behavior for `{audio}` at the end of the function — the new
`{prompt}` substitution joins the same loop; the force-append stays specific
to `{audio}` alone.

**Interfaces:**
- Produces: `pub trait Engine { fn transcribe(&self, audio: &Path, bias: &str) -> Result<String, EngineError>; }`
- Produces: `pub fn render(argv: &[String], audio: &Path, model: Option<&Path>, language: &str, bias: &str) -> Vec<String>`
- Consumed by: Task 2 (`daemon.rs`'s `transcribe` and `dictate`)

- [ ] **Step 1: Write the failing tests in `src/stt/command.rs`**

Add to the `mod tests` block, alongside the existing `render` tests:

```rust
#[test]
fn the_prompt_placeholder_is_substituted() {
    let rendered = render(
        &argv(&["whisper-cli", "--prompt", "{prompt}", "{audio}"]),
        Path::new("/takes/one.wav"),
        None,
        "auto",
        "recent terms: pull request, worklog",
    );
    assert_eq!(
        rendered,
        argv(&[
            "whisper-cli",
            "--prompt",
            "recent terms: pull request, worklog",
            "/takes/one.wav"
        ])
    );
}

#[test]
fn a_list_with_no_prompt_placeholder_does_not_gain_one() {
    // Unlike {audio}, an absent {prompt} is not force-appended: the bias
    // string is an opt-in enhancement, not something the program cannot
    // run without (AC-3).
    let rendered = render(
        &argv(&["whisper-cli", "{audio}"]),
        Path::new("/takes/one.wav"),
        None,
        "auto",
        "recent terms: pull request",
    );
    assert_eq!(rendered, argv(&["whisper-cli", "/takes/one.wav"]));
}

#[test]
fn an_empty_bias_substitutes_to_an_empty_string() {
    let rendered = render(
        &argv(&["whisper-cli", "--prompt", "{prompt}", "{audio}"]),
        Path::new("/takes/one.wav"),
        None,
        "auto",
        "",
    );
    assert_eq!(
        rendered,
        argv(&["whisper-cli", "--prompt", "", "/takes/one.wav"])
    );
}
```

Update every other existing call in this file so the crate still compiles.
None of these six tests is about the bias string, so each gains a trailing
`""` argument:

Three direct calls to `render(...)`, each gains a fifth argument, `""`:
- `each_placeholder_is_replaced` (`render(&argv(&[...]), ..., "en")`)
- `auto_is_substituted_like_any_other_language` (`render(&argv(&[...]), ..., "auto")`)
- `a_list_that_never_asks_for_the_audio_gets_it_appended`
  (`render(&argv(&["prog"]), Path::new("/takes/one.wav"), None, "auto")`)

Six calls to `engine.transcribe(...)`, each gains a second argument, `""`:
- `the_take_reaches_the_program`
- `a_list_with_no_placeholder_still_receives_the_take`
- `a_program_that_prints_a_transcript_gives_one_back`
- `a_program_that_is_not_there_names_the_path_it_searched`
- `a_program_that_fails_carries_what_it_complained_about`
- `a_program_that_prints_nothing_is_not_an_empty_success`

- [ ] **Step 2: Run to verify the new tests fail**

Run: `cargo test --lib stt::command`
Expected: FAIL to compile — `render` and `transcribe` do not yet take a fifth/
second argument. (A signature change fails the whole crate to build, not just
the new tests; that is expected and is why this step and the next are one
task.)

- [ ] **Step 3: Widen the trait, `render`, and `CommandEngine::transcribe`**

In `src/stt.rs:17`, change the trait method to
`fn transcribe(&self, audio: &Path, bias: &str) -> Result<String, EngineError>;`.

In `src/stt.rs:1-6`, correct the module doc comment: it currently says "the
path is the only thing that changes between takes" — bias now does too, so
state both as per-take parameters and keep model/language as construction-time
state.

In `src/stt.rs:143-152` (`tests_support::Fake`), add the `bias: &str`
parameter to its `Engine` impl and ignore it, the same way it already ignores
`_audio`.

In `src/stt/command.rs:15`, add `bias: &str` as `render`'s fifth parameter.
Inside the existing substitution closure, add a fourth `.replace("{prompt}", bias)`
after the `{language}` replacement, unconditionally — the loop already runs
over every argument regardless of which placeholders it contains. Do **not**
touch the force-append block below the closure; it names `{audio}` only.

In `src/stt/command.rs:107-109` (`CommandEngine::transcribe`), add the
`bias: &str` parameter and pass it through to `render(...)`.

- [ ] **Step 4: Run to verify all tests pass**

Run: `cargo test --lib stt`
Expected: PASS, including the three new tests and every existing test in
`src/stt.rs` and `src/stt/command.rs`.

- [ ] **Step 5: Run the full suite once, to catch every other call site**

Run: `cargo build --tests 2>&1 | grep -E 'error\[|error:'`
Expected: no output. If `src/daemon.rs` fails to compile here, that is
expected and is Task 2's job — do not fix `daemon.rs` in this task; confirm
the *only* remaining errors are in `daemon.rs`, naming `transcribe`'s call to
`engine.transcribe(&take.path)` missing an argument.

- [ ] **Step 6: Commit**

```bash
git add src/stt.rs src/stt/command.rs
git commit -m "Widen Engine::transcribe to carry the bias string"
```

---

### Task 2: Thread the collected bias string through the daemon

**Files:**
- Modify: `src/daemon.rs:159-165` (`dictate`), `:303-320` (`transcribe`), `:842`
  (the test that calls `transcribe` directly)
- Modify: `src/stt.rs` (`tests_support` module — add a capturing double)

**Before starting:** confirm Task 1 landed and `cargo build --tests` fails
only in `src/daemon.rs`, naming `transcribe`'s call to `engine.transcribe(&take.path)`.
Read `src/daemon.rs:1040-1076` for `bias_scratch`, `git_repo` and
`REPOSITORY_FILE` — the existing scratch-git-repository fixture this task's
test reuses — and `src/daemon.rs:1107` (`pane_request`) and `:648`
(`runtime_with`) for the request/runtime helpers this task's test drives.

**Interfaces:**
- Consumes: `Engine::transcribe(&self, audio: &Path, bias: &str)` (Task 1)
- Produces: `fn transcribe(runtime: &Runtime, take: &crate::capture::Take, bias: &str) -> Reply`
- Produces (test-only): `crate::stt::tests_support::CapturingFake`, an `Engine`
  whose `transcribe` records the `bias` it was called with into a shared
  `Arc<Mutex<Option<String>>>` a test keeps a handle to, alongside a canned
  `Result<String, String>` it returns the way `Fake` does.

- [ ] **Step 1: Write the failing test in `src/daemon.rs`**

Add to the `#[cfg(test)] mod tests` block, near
`the_bias_is_built_from_the_pane_the_take_was_pinned_to`:

```rust
#[test]
fn the_collected_bias_string_reaches_the_engine() {
    let cwd = bias_scratch("bias-to-engine-cwd");
    git_repo(&cwd);

    let (fake, received) =
        crate::stt::tests_support::CapturingFake::new(Ok("a transcript".to_string()));
    let mut runtime = runtime_with(crate::delivery::tests_support::FakeDeliverer::ok(), false);
    runtime.recognition = Ok(Box::new(fake));

    let recorder = tone_recorder("bias-to-engine");
    let request = pane_request("w1:p2", &cwd);
    answer(&request, &recorder, &runtime);
    answer(&request, &recorder, &runtime);

    let bias = received.lock().unwrap().clone().expect("the engine was called");
    assert!(bias.contains(REPOSITORY_FILE), "got {bias:?}");
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test --lib daemon::tests::the_collected_bias_string_reaches_the_engine`
Expected: FAIL to compile — `CapturingFake` does not exist yet, and
`transcribe`/`dictate` do not yet thread a bias string.

- [ ] **Step 3: Add `CapturingFake` to `src/stt.rs`'s `tests_support` module**

```rust
pub struct CapturingFake {
    result: Result<String, String>,
    received_bias: std::sync::Arc<std::sync::Mutex<Option<String>>>,
}

impl CapturingFake {
    pub fn new(
        result: Result<String, String>,
    ) -> (Self, std::sync::Arc<std::sync::Mutex<Option<String>>>) {
        let received = std::sync::Arc::new(std::sync::Mutex::new(None));
        (
            CapturingFake {
                result,
                received_bias: received.clone(),
            },
            received,
        )
    }
}

impl Engine for CapturingFake {
    fn transcribe(&self, _audio: &Path, bias: &str) -> Result<String, EngineError> {
        *self.received_bias.lock().unwrap() = Some(bias.to_string());
        match &self.result {
            Ok(text) => Ok(text.clone()),
            Err(why) => Err(EngineError::Unknown(why.clone())),
        }
    }
}
```

- [ ] **Step 4: Thread the bias string through `dictate` and `transcribe`**

In `src/daemon.rs:159-165`, bind `take_bias`'s return instead of discarding it,
and pass its `.bias` field into `transcribe`:

```rust
let collected = take_bias(
    runtime,
    &take.target,
    take.cwd.as_deref(),
    take.agent.as_deref(),
);
transcribe(runtime, &take, &collected.bias)
```

In `src/daemon.rs:303`, widen `transcribe`'s signature to
`fn transcribe(runtime: &Runtime, take: &crate::capture::Take, bias: &str) -> Reply`,
and change its call at (what was) line 315 to
`engine.transcribe(&take.path, bias)`.

In `src/daemon.rs:842`, update the direct test call to
`transcribe(&runtime, &take, "")` — that test is about a delivery rejection
surviving newline-collapsing, not about bias, so an empty string is correct
and keeps the test's own intent unchanged.

- [ ] **Step 5: Run to verify the new test passes, and the full daemon suite too**

Run: `cargo test --lib daemon`
Expected: PASS, including `the_collected_bias_string_reaches_the_engine` and
every pre-existing test in `src/daemon.rs` (in particular
`the_bias_is_built_from_the_pane_the_take_was_pinned_to`, which does not use
`CapturingFake` and must be unaffected by this task).

- [ ] **Step 6: Run the whole suite**

Run: `cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt --check && python3 scripts/check_manifest.py`
Expected: all four green.

- [ ] **Step 7: Commit**

```bash
git add src/daemon.rs src/stt.rs
git commit -m "Thread the collected bias string from take_bias into the engine"
```

---

### Task 3: Record the decision in `docs/decisions.md`

**Files:**
- Modify: `docs/decisions.md` (append one row to the table)

**Before starting:** read the existing row for `[stt] command`'s introduction
(search for `{audio}`, `{model}` and `{language} placeholders` — it is the row
dated 2026-08-25, #13) so the new row's wording sits next to it and uses the
same voice.

- [ ] **Step 1: Append one row to the decisions table**

```markdown
| `Engine::transcribe` widens to take the bias string as a second parameter, `bias: &str`, alongside `audio: &Path`; `{prompt}` joins `{audio}`, `{model}` and `{language}` as a fourth substitution in `[stt] command`, with no forced append when it is absent | Bias is per-take, exactly the way the audio path is — both depend on the pinned pane, unlike the model and the language, which are construction-time state. `{audio}` is force-appended because a transcriber cannot run at all without it; a bias string is an opt-in enhancement a person adds to their own argument list by writing the placeholder | 2026-09-02, #26 |
```

- [ ] **Step 2: Commit**

```bash
git add docs/decisions.md
git commit -m "Record the Engine::transcribe widening decision"
```

---

### Task 4: S4 — review the diff before a pull request exists

Invoke `superpowers:requesting-code-review` against the full diff on
`feat/26-bias-to-engine` since it diverged from `main`, naming this plan and
`DESIGN_26.md` as what it should satisfy. Explicitly ask the reviewer to check
that no path force-appends the bias string the way `{audio}` is (AC-3), and
that nothing newly logs a rendered argument list (the constraint DESIGN_26.md
§5 states and the S1 gate reviewer noted).

- [ ] Run the review.
- [ ] Append a `## Gate S4` block to `tasks/26/RUN_26.md` with the verdict; if
  not `READY`, address the findings and re-run before proceeding.
- [ ] Commit: `git add tasks/26/RUN_26.md && git commit -m "Record the S4 diff review for #26"`

---

### Task 5: S5 — verify by test suite, then by a spoken take

- [ ] Run `cargo test`, reading the whole output; then `cargo clippy
  --all-targets -- -D warnings`, `cargo fmt --check`,
  `python3 scripts/check_manifest.py`.
- [ ] Append a section to `docs/evidence.md`: what the test suite establishes
  (the bias string reaches `command::render`, substitutes `{prompt}` by name,
  is not force-appended, and an empty string is handled without a special
  case) and what it does not — AC-7 needs a person at a microphone, and no
  test discharges it.
- [ ] **The manual step (AC-7), for the owner:** `herdr plugin link .` this
  worktree, speak the same Russian phrase containing an English technical term
  that produced "пули квест" in `docs/evidence.md`'s "Recognition, by hand on
  macOS" section, with `[stt] command` configured to include `--prompt
  "{prompt}"` (or whatever flag the configured transcriber takes). Record the
  platform, the phrase, and the returned transcript in `docs/evidence.md`, next
  to that section, the same way it was recorded.
- [ ] Append a `## Gate S5` block to `tasks/26/RUN_26.md` with the verdict —
  note explicitly if the manual step is still pending the owner, rather than
  claiming it done.
- [ ] Commit: `git add docs/evidence.md tasks/26/RUN_26.md && git commit -m "Verify the bias string reaches the engine, and record the spoken-take result"`

---

## Coverage

AC-1 → Task 2 (`CapturingFake` test). AC-2 → Task 1
(`the_prompt_placeholder_is_substituted`). AC-3 → Task 1
(`a_list_with_no_prompt_placeholder_does_not_gain_one`). AC-4 → Task 1
(`an_empty_bias_substitutes_to_an_empty_string`). AC-5 → no task adds a cap;
checked at Task 4's review. AC-6 → Task 1 (every existing `render`/`transcribe`
test with no `{prompt}` in its argument list is unaffected). AC-7 → Task 5's
manual step. AC-8 → Task 4's review (no `EngineError`/`CommandError` variant's
reporting changes in this plan).

## What could not be cut into a checkable task

AC-7's manual take cannot be scheduled as a task with a done-criterion this
plan can check itself — it needs a human at a microphone, the same limit
`tasks/21/PLAN_21.md`'s own S5 task recorded. Task 5 states plainly what a
person must do and what claim would be false without it.
