# PLAN_13 — recognition

> Execute with `superpowers:executing-plans` and `superpowers:test-driven-development`:
> the failing test first, then the code, then the commit, one task at a time.

**Goal:** a finished take becomes text, through an external program the person
configured.

**Spec:** `tasks/13/DESIGN_13.md`, built from `tasks/13/AC_13.md`.

## Global constraints

- No new dependencies. This issue adds none: running a program is `std::process`.
- English everywhere; paths cited relative to the repository root.
- No panic paths; every failure names what to do next.
- `cargo fmt` before every commit. Four local checks and five CI checks stay green.
- Nothing falls back to a working engine when the configured one is absent.

## Order

Task 1 first — everything reads configuration. Tasks 2 and 3 are independent of
each other. Task 4 needs 1, 2 and 3. Tasks 5 and 8 are independent of everything.
Task 6 needs 4 and 5. Task 7 needs 2 and 4. Task 9 is last.

---

### Task 1: the `[stt]` keys

**Files:** `src/config.rs`
**Output:** `Stt { model, engine, language, command }` — `engine` defaulting to
`"candle"`, `language` to `"auto"`, `command` to an empty `Vec<String>`, `model`
unchanged.
**Done when:** tests cover the three new defaults, a file setting all three, and an
unknown key inside `[stt]` being ignored.

---

### Task 2: `stt::model`

**Files:** create `src/stt.rs` and `src/stt/model.rs`; declare the module in
`src/main.rs`
**Output:** `file_name(model: &str) -> String` returning `ggml-<model>.bin`;
`locate(models_dir: &Path, model: &str) -> Result<PathBuf, ModelError>` running the
four checks in order — exact name, plausible size, the four leading bytes
`6C 6D 67 67`, and a `.sha256` sidecar when one exists; `ModelError` whose `Display`
names the file and what to do.
**Done when:** tests cover a good file; a file whose name merely contains the model;
an empty file; a file below the size floor; a file with the wrong first bytes; a
matching sidecar; a mismatching sidecar; and an absent sidecar being no failure.
Each error message names the path.
**Watch for:** the size floor is a constant with a comment saying what it is for —
smaller than any real model, larger than an error page saved by mistake. One
megabyte.

---

### Task 3: `stt::command`

**Files:** create `src/stt/command.rs`
**Output:** `render(argv: &[String], audio: &Path, model: Option<&Path>, language: &str) -> Vec<String>`
replacing `{audio}`, `{model}` and `{language}` and appending the audio path when no
`{audio}` appears; `CommandEngine` holding a rendered argument list and running it;
`CommandError` for a program that is absent, one that exits non-zero, and one that
prints nothing.
**Done when:** tests cover each placeholder; the appended path; `auto` substituted
like any other value; a program that does not exist, whose message names the `PATH`
searched; a program exiting non-zero, whose message carries its standard error; a
program printing nothing; and a program printing text with surrounding whitespace,
which is trimmed. The tests use `echo`, `false` and a name that does not exist —
no model, no microphone, no network.
**Watch for:** `{model}` with no model resolved must not render the literal
`{model}`. If the list asks for a model and none was resolved, that is an error
before the program runs.

---

### Task 4: the trait, and resolving an engine

**Files:** `src/stt.rs`
**Output:** `trait Engine { fn transcribe(&self, audio: &Path) -> Result<String, EngineError>; }`;
`resolve(stt: &config::Stt, models_dir: &Path) -> Result<Box<dyn Engine>, EngineError>`;
a `Fake` behind `#[cfg(test)]` for other modules to borrow.
**Done when:** tests cover `candle` and `http` each reporting that they are not
built and naming `command`; an unknown name refused with the three listed; `command`
with an empty list naming the key to set and showing an example; `command` with a
list containing `{model}` and a missing model failing with the model's message; and
`command` with a workable list resolving.
**Depends on:** 1, 2, 3.

---

### Task 5: the reply bound

**Files:** `src/client.rs`, and its call site in `src/main.rs` if needed
**Output:** `timeout_for(command: &str) -> Duration` — two minutes for `dictate`,
two seconds otherwise — used by `send_to`, and the timeout message rendered from the
bound actually used rather than from a single constant.
**Done when:** the rewritten test asserts the short bound stays at or under five
seconds, `dictate` gets the long one, `cancel` gets the short one, and the long one
is still bounded. The timeout message names the number of seconds waited.
**Watch for:** `src/client.rs:59` renders the message from `REPLY_TIMEOUT`; that
becomes the bound the call used.

---

### Task 6: the daemon transcribes

**Files:** `src/daemon.rs`
**Output:** the daemon holds a resolved engine beside its recorder, and the
`dictate` that finishes a take transcribes before replying. The reply carries the
transcript and the measured level. A take that transcribes to nothing is reported as
such. An engine that could not be resolved at start-up is reported when a take
finishes, not at start-up — the daemon must still run for `cancel` and `doctor`.
**Done when:** tests with the fake engine cover a transcript coming back, an engine
error coming back, an empty transcript, and the level still appearing in the reply.
**Depends on:** 4, 5.

---

### Task 7: `doctor` agrees

**Files:** `src/doctor.rs`
**Output:** an engine line reporting what `resolve` reports; a model line using
`stt::model::locate`; and a fourth `State` — the model is not used by this
configuration — for when the engine is not built or the list has no `{model}`.
**Done when:** tests cover the four states of the model line and the engine line for
each engine name, and no test asserts the substring rule any more.
**Depends on:** 2, 4.

---

### Task 8: the README line

**Files:** `README.md`
**Output:** in the section about installing, one short paragraph: recognition needs
an external engine until the built-in one is built, with the two keys to set and
issue #15 named.
**Done when:** the paragraph is there and names `[stt] engine`, `[stt] command` and
#15.

---

### Task 9: run it against a real transcriber

**Files:** `docs/evidence.md`
**Output:** a section with the platform, recording what happened.
**Done when:** these are run and written down as they happened: a take transcribed
end to end with `whisper-cli` and the model on this machine; the same with
`[stt] command` empty; the same with `engine = "candle"`; and `doctor` under each.
The transcript of a real spoken take is the one thing that cannot be produced at
this hour, and its absence is recorded rather than glossed.
**Depends on:** everything.
