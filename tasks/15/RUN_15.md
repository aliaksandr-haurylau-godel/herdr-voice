# RUN_15

| field | value |
|---|---|
| issue | #15 — Recognition: the built-in candle engine, and choosing a model on first run |
| input | GitHub issue, read with `gh issue view 15` |
| stage | S1 |
| branch | feat/15-candle-engine |
| opened | 2026-09-09 |

## Stages

<!-- One block per stage, appended, never rewritten. -->

### S1 Assess
- artifact: `AC_15.md`
- produced: 2026-09-09

Written from the issue body and from what is actually in the tree at `851821c`:
`src/stt.rs`, `src/stt/model.rs`, `src/config.rs`, `src/doctor.rs`,
`src/main.rs`, `src/daemon.rs`, `src/audio/wav.rs`, `herdr-plugin.toml`,
`Cargo.toml`, `README.md`, `docs/design.md`, `docs/decisions.md` and
`docs/evidence.md`. Every line-number citation in `AC_15.md` was re-checked
against the file after the first draft; several were wrong by a few lines and
were corrected. No ADO ticket and no `Risk` field exist — the trigger is a
GitHub issue (`CLAUDE.md`, "Trigger and run root"), and it has no comments.

Two things the issue treats as settled turned out not to be, on reading the
code:

- **The model contract does not carry over as written.** `src/stt/model.rs`
  checks one file named `ggml-<model>.bin` beginning with the `ggml` magic.
  `candle-transformers` reads `safetensors` (or GGUF), and the Hugging Face
  `openai/whisper-*` repositories ship three files — `model.safetensors`,
  `config.json`, `tokenizer.json` — none of which is a `ggml` file. The
  contract's four *checks* apply; three of its *constants* do not. The issue
  says the exact-name contract "must be honoured, not loosened", so how it
  extends is a design problem, named as the first of six items handed to S2
  rather than assumed away.
- **A log-mel spectrogram has to come from somewhere.** Whisper's encoder takes
  mel frames, not samples; candle's own Whisper example ships a precomputed mel
  filterbank as a binary blob. Nothing in this repository has one, and nothing
  computes one. Named for S2.

Also established while writing the as-is, because the AC could not be checkable
without it: `src/audio/wav.rs` writes WAV and does not read it, so this engine
is the first thing in the crate that has to decode a take rather than hand its
path to another program.

Both owner-reserved items (the model list with sizes, and the download source)
are proposed concretely in `AC_15.md`'s requirements section, marked provisional
and renameable on the same footing `[stt] command` was introduced on
(`docs/decisions.md`, 2026-08-25, #13). The sizes were read off the Hugging Face
API on 2026-09-09, not recalled. The owner is not reachable from this session;
per the run brief the run proceeds on the proposal rather than blocking. AC-13
requires the choice to be written into `docs/decisions.md` in the four-part form.

```yaml
gate:
  stage: S1
  artifact: AC_15.md
  reviewer: designer
  verdict: null
  date: null
```

```yaml
gate:
  stage: S1
  artifact: AC_15.md
  reviewer: designer
  verdict: READY
  date: 2026-09-09
  questions: []
  blocker: null
```

Every citation was re-checked by the reviewer against the source and held,
including the two the as-is section rests its central claim on: `src/stt/model.rs`'s
four checks with their two constants, and `Cargo.toml`'s six dependencies with no
tensor library among them.

One thing noted by the reviewer and not raised as a gate question, because it
follows mechanically from AC-4: `herdr-voice model` leaving the exit-69 arm also
breaks the two tests that pin the unbuilt state —
`the_commands_this_issue_implements_are_not_in_the_unimplemented_arm`
(`src/main.rs:211-215`) and `the_usage_text_names_every_implemented_command`
(`src/main.rs:228-233`), which walks a `USAGE` string that does not name `model`.
AC-4 was extended to name both, so the plan does not have to rediscover them.

S1 is closed. Next is S2 Design.

### S2 Design
- artifact: `DESIGN_15.md`
- produced: 2026-09-09

The owner answered both reserved questions directly rather than being unreachable:
six models (`tiny`, `base`, `small`, `large-v3-turbo`, `medium`, `large-v3`), and
Hugging Face pinned by commit with a SHA-256 per file. Because he decided them,
they do not go into `docs/decisions.md`, which by its own first line records
"everything decided without the repository's owner"; they are stated in
`DESIGN_15.md` sections 3 and 5 instead. AC-13 is satisfied for the choices that
were in fact taken without him, listed below.

`superpowers:brainstorming` classifies this as architectural and requires live
approval of each design section. This run follows the precedent #26 and #36 set:
the project's own planner gate substitutes for that approval, and the two
questions that were genuinely the owner's were put to him directly before the
design was written rather than after.

**The design was not written from reading alone.** A throwaway probe crate in the
session scratchpad built the whole inference path against the real crates and the
real weights, and it changed the design three times:

- **Metal needs the feature on every candle crate, not just `candle-core`.** With
  it on `candle-core` alone, inference dies at the first encoder layer with
  `Metal error no metal implementation for layer-norm`. This would have been
  discovered mid-implementation.
- **A window that is nearly all padding transcribes the padding.** A final window
  holding 2 frames of real audio and 2 998 of zeros returned "[Music]". The
  one-second minimum in section 7 exists because of that observation, not because
  it seemed prudent.
- **F16 is not available and the engine is about three times slower than
  whisper.cpp.** 5.1 s for a 66-second take against the 1.65 s `docs/evidence.md`
  records for `whisper-cli` on 70 seconds with the same model. Loading as F16
  fails: `dtype mismatch in add, lhs: F16, rhs: F32`, because
  `candle-transformers` 0.11 mixes F32 constants into the Whisper graph. Recorded
  in section 10 so issue #2 starts from a number.

Also established by measurement rather than assumed: the mel filterbank computed
from the specification matches candle's reference blobs to 1.86 × 10⁻⁹ (80 bins)
and 3.73 × 10⁻⁹ (128 bins), which is what makes section 6's "compute it, do not
embed it" decision safe; the `<|startofprev|>` bias-prompt path is live (the same
audio produced different punctuation with a prompt); the timestamp-conditioned
window advance yields a continuous boundary; and the four crates cost 58 s of
clean release build and take the dependency tree from 87 to roughly 151.

Decisions taken without the owner, for `docs/decisions.md` at implementation
time: the separate `<state>/models/candle/<identifier>/` store rather than
extending the `ggml` name template; the pinned catalogue as a compile-time table
with a regeneration script; `ureq` rather than `hf-hub`; the mel filterbank
computed from the specification with candle's blobs kept only as test fixtures;
greedy decoding with no temperature fallback, and what that leaves open; the
one-second window guard; the eager load plus a silence warm-up pass at daemon
start; and the line-oriented configuration edit rather than a `toml_edit`
dependency.

```yaml
gate:
  stage: S2
  artifact: DESIGN_15.md
  reviewer: planner
  verdict: null
  date: null
```

```yaml
gate:
  stage: S2
  artifact: DESIGN_15.md
  reviewer: planner
  verdict: QUESTIONS
  date: 2026-09-09
  questions:
    - "Device selection is named but never bounded: which device is chosen,
       whether an unavailable Metal device falls back or refuses, and what the
       message says. AC-8 requires an unsupported compute device to be a named
       failure, and section 8 builds eagerly at daemon start, so this is the
       construction task's main branch. It also decides what engine = \"candle\"
       does on the Linux and Windows CI runners."
    - "The stated signatures and the tests section 11 asks of them contradict
       each other. store::locate takes its expected size and digest from the
       compile-time catalogue, and fetch::model builds its URL inside itself —
       so a fabricated scratch file cannot match a pinned 151 MB digest, and a
       loopback double cannot be reached through a huggingface.co URL. Design
       must name the seam, because the chooser, resolve and doctor tasks all
       call these."
    - "Nothing states the type carrying a candle model's outcome from store into
       resolve_with and doctor. locate_configured_model returns
       Option<Result<PathBuf, ModelError>> today and feeds both; section 12
       widens it while section 2 forbids changing ModelError, and section 2 adds
       a third outcome (present but unpinned) that Result has no room for."
  blocker: null
```

### Answered

All three fixed in `DESIGN_15.md`; none reopened a decision, and one was
answered by measurement rather than by choosing.

**The device.** Section 8 is now "The device, loading, and the first take" and
states the rule: `Device::new_metal(0)` on macOS, `Device::Cpu` when that fails
or when the binary was built without the feature — which is every Linux and
Windows build, both CI runners included — and never a refusal to build the
engine. What made the rule decidable was measuring the fallback rather than
assuming it: on a 66-second take the default model runs 5.1 s on Metal and
**52.3 s on the CPU**, and `tiny` runs 0.69 s against 6.4 s. A factor of ten is
a different product, not a slower one, so the fallback is reported at daemon
start and on `doctor`'s engine line, naming what it costs and naming `tiny` as
the model that stays usable there — but a take is never refused for it. No
configuration key for the device: a key is a user-visible name.

**The seams.** `store::locate(models, identifier, expected: Option<&catalogue::Entry>)`
takes the catalogue entry as a parameter, so a test hands it an entry describing
a few-kilobyte fabricated file; production passes `catalogue::get(identifier)`,
which is `None` for an unknown identifier and yields `Unpinned`. `fetch` splits
into `model(identifier, models, progress)` and an inner
`model_from(base, entry, dir, progress)` whose base address is a parameter, so
the test drives it against a loopback `TcpListener` the way `src/rewrite/http.rs`
already does. Section 11's two unwritable rows were rewritten against the new
signatures.

**The outcome type.** New section 2b. `store::locate` returns
`Result<Found, StoreError>`, where `Found` is `Verified` or `Unpinned` — the
third outcome, in the type — and `StoreError` is a new type beside
`model::ModelError` rather than variants added to it, which is what keeps
section 2's promise that the `ggml` store changes in no respect.
`locate_configured_model` keeps its name and its one-call-per-`doctor`-run
property and returns `ModelState`: `NotUsed`, `Ggml(Result<PathBuf, ModelError>)`
or `Candle(Result<Found, StoreError>)`. `resolve_with` and
`doctor::model_finding_from` both match that one value, so the engine line and
the model line cannot disagree — the property PR #20 gave `doctor` and this must
not lose.

Four dependency refinements were folded into section 9 in the same pass, all
measured after the design first went to gate: `tokenizers` pins to **0.22**, not
0.23, because `candle-core` depends on 0.22 itself and matching it drops the tree
from 151 crates to 149; `onig_sys`, a C library, enters through `candle-core`
regardless of the feature set chosen here, which the five-platform release matrix
in `docs/design.md` section 8 will meet; `unstable_wasm` was tried as a pure-Rust
escape and is not one; and `rust-version = "1.82"` has never been enforced — both
workflows build `stable` (`.github/workflows/check.yml:33`, `release.yml:26`) and
no candle crate declares a `rust-version` — so the plan either verifies it or
corrects it.

S2 continues; re-running the gate against the revised artifact.

```yaml
gate:
  stage: S2
  artifact: DESIGN_15.md
  reviewer: planner
  verdict: null
  date: null
```

```yaml
gate:
  stage: S2
  artifact: DESIGN_15.md
  reviewer: planner
  verdict: QUESTIONS
  date: 2026-09-09
  questions:
    - "Section 8 requires the CPU fallback on doctor's engine line, but nothing
       states the channel that carries the device out of CandleEngine: section 0
       fixes stt::Engine at one method and doctor builds its line from
       resolve_with's return value plus stt.engine, so the device name has
       nowhere to travel. The same gap makes section 11's 'CPU fallback's
       report' row unwritable — no signature lets a test force a device."
    - "Whether doctor builds the candle engine is unstated, and two sections
       point opposite ways: 2b keeps doctor on one resolve_with call, section 8
       makes resolve build eagerly with weights and a warm-up pass. Together
       every doctor run would load 1.6 GB and run inference, contradicting the
       reason PR #20 gave for reading the model once — and making section 11's
       ModelState row unwritable, since Candle(Ok(Verified)) through
       resolve_with cannot be asserted from a directory with no weights."
  blocker: null
```

### Answered

The second question found a real defect, not an omission: as written, every
`herdr-voice doctor` run would have loaded 1.6 GB of weights and run an encoder
pass to print six lines. New section 2c splits the two things `resolve_with` was
doing at once. `check_with(stt, state) -> Result<Ready, EngineError>` makes every
check that needs no weights; `resolve_with` is defined as `check_with` followed by
construction and nothing else. `doctor` calls `check_with`, the daemon calls
`resolve_with`. The split cannot produce two answers to one question, because one
is written in terms of the other — which is the property PR #20 gave `doctor` and
the reason it was worth keeping.

That also answers the first question without touching the trait. The device is
chosen by `device::select()` and handed to `CandleEngine::new` rather than chosen
inside it, and it reaches `doctor` inside `Ready::Candle { device, model }`.
`Selection` is a plain two-variant enum, so `device::describe` is tested against
both `Metal` and `Cpu { why }` with no hardware assumption — which is what makes
section 11's fallback row writable, and it is the row that covers every Linux and
Windows CI runner. Section 0's "one method" promise stands: the device is
construction-time state, and the trait is for per-take work.

Section 11 gained a third row while fixing the other two — a thread-local read
counter asserting that a `doctor` run over a present, verified candle model reads
zero bytes of weights, using the same idiom `src/stt/model.rs:23-48` already uses
to pin `locate` to one call per run. Without it the split is a claim rather than
a checked property, and the failure it guards against is invisible until somebody
times `doctor` on a real model.

Section 12 gained a consequence for #16: `doctor` moving to `check_with` means
the `http` arm has to exist there. The arm is #16's to write; which function it
lives in is settled here.

S2 continues; re-running the gate against the revised artifact.

```yaml
gate:
  stage: S2
  artifact: DESIGN_15.md
  reviewer: planner
  verdict: null
  date: null
```

```yaml
gate:
  stage: S2
  artifact: DESIGN_15.md
  reviewer: planner
  verdict: READY
  date: 2026-09-09
  questions: []
  blocker: null
```

Every citation re-verified against source on the third read and all held.

Five non-blocking notes came with the verdict, each local to one task. Four were
fixed before S3 rather than left for the plan to trip over:

- `Ready::Command` as written dropped the ggml model path that
  `CommandEngine::new` takes today (`src/stt.rs:123-132`), and since
  `check_with` consumes the lookup and `resolve_with` builds only from what
  `check_with` returned, the path had no route to the constructor. The variant now
  carries `model: Option<PathBuf>` — the same `Option` the constructor already
  takes.
- `Found::Unpinned` claimed in its doc comment to carry the identifier and held
  only a `ModelDir`. It now carries both.
- Section 8's opening sentence still said `CandleEngine::new` asks for the Metal
  device, which section 2c had superseded. Reworded to `device::select()`.
- A duplicated `### Problem` heading in section 8, removed.

The fifth — that AC-12's README edit has no section of its own — is left as it
is: the criterion states the change exactly, and a section restating it would add
nothing.

S2 is closed after three rounds. Next is S3 Plan.

### S3 Plan
- artifact: `PLAN_15.md`
- produced: 2026-09-09

Fifteen tasks: the dependencies and their decision rows (1), `wav::read` (2), the
mel filterbank (3), the pinned catalogue and its regeneration script (4), the
candle store (5), fetch-and-verify (6), device selection (7), the pure decoding
decisions (8), the engine itself (9), `ModelState`/`check_with`/`resolve_with`
(10), `doctor` (11), the chooser (12), the daemon's device line and the documents
(13), the S4 mutation review (14), and S5 (15). A coverage table maps all
fourteen acceptance criteria onto tasks.

Self-review before the gate found three real problems and fixed them:

- **`EngineError::Candle` was introduced in Task 10 but first used in Task 9**,
  which left the tree red at a commit boundary. Moved into Task 9, where the code
  that needs it lives; Task 10 now only removes the `NotBuilt` arm.
- **The chooser's listing would have hashed every installed model.** `list` called
  `store::locate` for all six entries, so `herdr-voice model` with a real
  multi-gigabyte model installed would take about a minute to answer a question
  about names and sizes. Added `store::glance`, which judges by name and byte
  count and never hashes, with a test pinning that corrupt bytes at the right
  length are invisible to it and caught by `locate` — the distinction is the
  point, not an accident.
- **One `doctor` test asserted something false and the plan said so in prose
  instead of fixing it.** A small fixture written under the pinned
  `large-v3-turbo` identifier is exactly the shape of a truncated download and
  must report `Missing`; only an unpinned identifier gives `Ok`. Split into two
  tests that assert the right thing, with a note on which way round it goes and
  why the store must never be weakened to make a small fixture pass a pinned
  digest.

Also settled while writing, rather than left to the implementer: Task 7's device
selection needs no feature flag on this crate and no manifest change, because
Task 1's target-specific dependency block already gives macOS builds the `metal`
feature and cargo unifies features per crate — which is the arrangement the
design's probe actually used and verified. An earlier draft invented a `metal`
feature, a `--features` flag in the manifest's `[[build]]` entries and a
`.cargo/config.toml` that did nothing; it was removed.

```yaml
gate:
  stage: S3
  artifact: PLAN_15.md
  reviewer: implementer
  verdict: null
  date: null
```

```yaml
gate:
  stage: S3
  artifact: PLAN_15.md
  reviewer: implementer
  verdict: QUESTIONS
  date: 2026-09-09
  questions:
    - "Task 12 names chooser::{list, choose, set_model_key} and describes
       choose's behaviour in prose, but gives code for list, pick, human, Line
       and set_model_key only. Step 4 then wires main.rs to chooser::run, a name
       that appears nowhere else and has no signature. Executing the task would
       mean inventing run's signature, how it composes list and choose, where it
       resolves the models directory, and what each outcome exits with."
    - "Task 11's weight_reads::increment is declared pub(super) inside a module
       in src/stt/candle/store.rs and called from CandleEngine::new in
       src/stt/candle.rs — the parent module, not a descendant, so it does not
       compile. The cited precedent (model.rs's locate_calls) works only because
       locate calls it from inside its own module."
  blocker: null
```

### Answered

Both fixed, and both were real: the second is a compile error, not an ambiguity.

**The visibility.** `weight_reads::increment` is now `pub(crate)`, with a comment
saying why the precedent's `pub(super)` does not carry over — `src/stt/model.rs`
calls `locate_calls::increment` from `locate`, inside the same module
(`src/stt/model.rs:99`), while the caller here is the parent module. The call site
is now written out rather than described.

**The chooser.** `chooser::run(choosing: bool) -> u8` is written in full: it
resolves the models directory the way `doctor::run` does
(`src/doctor.rs:345-347`), prints the listing, and for `--choose` reads the
answer, refuses anything that is not one of the offered numbers, downloads,
**re-verifies with `store::locate` rather than the glance the listing uses**, and
writes `[stt] model`. Two outcomes the prose had not settled are now stated: a
choice that is already the configured model says so and writes nothing, and a
successful download whose configuration write fails prints the one line to add by
hand and exits 1 — the model is on disk and good, so the person needs a line of
text, not another three gigabytes. `run` is the only function in the module that
touches the environment, which is why the other four are unit-tested and it is
exercised by hand in Task 15.

The reviewer's third note, not raised as a gate question, was fixed in the same
pass: Task 9's test module now says it belongs in `src/stt/candle.rs`, where
`check_mel_bins` and `read_take` are defined, and states that `decode.rs` gets no
test module of its own because nothing in it is reachable without weights — which
is the reason `plan.rs` exists.

S3 continues; re-running the gate against the revised artifact.

```yaml
gate:
  stage: S3
  artifact: PLAN_15.md
  reviewer: implementer
  verdict: null
  date: null
```

```yaml
gate:
  stage: S3
  artifact: PLAN_15.md
  reviewer: implementer
  verdict: QUESTIONS
  date: 2026-09-09
  questions:
    - "Task 9's three decoding functions — detect_language, greedy and run — are
       given as English prose and a text pseudocode block, while every other
       module in the plan is complete compilable Rust. None of them states the
       candle_transformers Whisper API surface needed to write them: the
       decoder's forward signature and where flush_kv_cache goes, how logits are
       obtained, how the 99 language strings become the token ids index_select
       needs, which axis softmax and argmax run over. The crate is not in the
       tree at that point and no other file uses it, so there is no precedent to
       read it off, and DESIGN_15.md section 7 also stays at prose level. This is
       the riskiest new tensor code in the issue specified at a categorically
       lower level than everything around it."
  blocker: null
```

Round two confirmed the previous fixes against the real source — `config::Vars`,
`directory`, `load`, `FILE_NAME`, `Loaded`, `transport::state_directory`,
`stt::model::sha256_of`, the `doctor` finding functions and `main.rs`'s
`Command::Model` arm all match what the plan writes — and found one remaining
blank.

### Answered

The question is correct and the gap was real: prose where every neighbouring task
gives code, on the one piece with no precedent in the tree to fall back on.

All three functions are now written out as Rust. The API surface is stated
explicitly at the top of the block and was read off `candle-transformers`
0.11.0's own source rather than recalled: `Whisper` exposes `pub encoder`,
`pub decoder` and `pub config`; `AudioEncoder::forward(&mut self, &Tensor, bool)`;
`TextDecoder::forward(&mut self, &Tensor, &Tensor, bool)`;
`TextDecoder::final_linear(&self, &Tensor)`; `Whisper::reset_kv_cache(&mut self)`.
Writing it out settled three things the prose had left implicit: the language
strings become ids by `tokenizer.token_to_id("<|xx|>")` before `index_select`,
with a named error when a model has none of them rather than a silent empty
selection; `flush_kv_cache` is true on the first decoder step of a window and
false after; and the bias string precedes the previous window's text in the
prompt, because `prompt_tokens` cuts from the front and the newest context is
what must survive the cut.

**The code was then compiled rather than assumed.** It was built together with
Task 8's `plan.rs` in a throwaway crate against the real crates with the `metal`
feature on: it compiles with no warnings and passes
`cargo clippy --all-targets -- -D warnings`. The plan says so at that point, and
says plainly what that does and does not establish — the API surface is real and
the types line up; whether it transcribes correctly is what Task 15 measures.

S3 continues; re-running the gate against the revised artifact.

```yaml
gate:
  stage: S3
  artifact: PLAN_15.md
  reviewer: implementer
  verdict: null
  date: null
```

```yaml
gate:
  stage: S3
  artifact: PLAN_15.md
  reviewer: implementer
  verdict: QUESTIONS
  date: 2026-09-09
  questions:
    - "CandleEngine::new — the constructor Task 10 calls and Task 11 adds a line
       to — is still prose, immediately beside the decoding functions that were
       just rewritten as code for exactly this reason. 'Load the tokenizer' names
       no function and no error conversion; 'read config.json into a Config' does
       not say whether Config is Deserialize or how it is parsed; Whisper::load
       is named with no signature anywhere in the plan. Task 11 then presumes the
       function exists in a known shape."
  blocker: null
```

Round three confirmed everything else: `run` and `detect_language`'s call sites
match their declarations argument for argument, `Tokens`/`LANGUAGES`/
`language_token` are used consistently by the new code, every `plan::` call
matches Task 8, and `Ready::Candle`'s destructure matches
`CandleEngine::new`'s interface line.

### Answered

Correct, and the same class of gap as the previous round — left standing two
paragraphs below the fix for it, which is how this one survived.

`CandleEngine::new` and its `warm` are now written out, with the API surface
stated: `Config` derives `Deserialize` and `Clone` so `serde_json::from_str`
loads it; `Tokenizer::from_file` takes a path and its error has `to_string`;
`VarBuilder::from_mmaped_safetensors(&[PathBuf], DType, &Device)` is `unsafe`;
`Whisper::load(&VarBuilder, Config)` takes the builder by reference and the
config by value. The imports the file needs are listed. Writing it out also put
three things on the page that the prose had not: one `named` closure so all six
failure paths name the file and what to do rather than each inventing its own
wording, a comment on why the `unsafe` mmap is acceptable here (the file was
verified by `store::locate` before this is called) and what residual risk remains,
and `warm` as a method that unwraps nothing, so a poisoned lock costs the warm-up
and nothing else.

**Compiled, not asserted.** The constructor was added to the same throwaway crate
as the decoding functions and built against the real crates with `metal` on: no
warnings from the code itself, and clippy clean at `-D warnings`.

S3 continues; re-running the gate against the revised artifact.

```yaml
gate:
  stage: S3
  artifact: PLAN_15.md
  reviewer: implementer
  verdict: null
  date: null
```

```yaml
gate:
  stage: S3
  artifact: PLAN_15.md
  reviewer: implementer
  verdict: READY
  date: 2026-09-09
  questions: []
  blocker: null
```

Fourth read, hunting specifically for what three successive rewrites of Task 9
might have left inconsistent. `CandleEngine`'s fields, `new`'s construction,
`warm` and `transcribe` agree on every name and type, with no field set and never
read or read and never set; the `store::weight_reads` forward reference from Task
9 to Task 11 resolves; and `config::directory`, `config::load`,
`transport::state_directory`, `transport::Vars::from_env`, `daemon::Recognition`
and `stt::model::sha256_of` were each checked against the current source where
Tasks 12 and 13 depend on them.

Two loose ends came with the verdict, neither gating, both fixed before starting:
Task 10's file header cited `src/stt.rs:250-262` as tests that change, when they
need no change — the citation now says so and asks for them to be re-read after
the edit rather than edited. And Task 10 told the implementer to derive `Clone`
on `ModelDir`, which Task 5 already derives it on; now only `Found` is named.

S3 is closed after four rounds. Next is S4 Implement.

## S4 Implement

Executing `PLAN_15.md` task by task, one commit each, tests first.

### Tasks 1 to 5 done

| task | commit | tests |
|---|---|---|
| 1 the candle dependencies | `e462404` | 249 |
| 2 `wav::read` | `4289c2b` | 253 |
| 3 the mel filterbank | `ef95cb2` | 256 |
| 4 the pinned catalogue | `f57f8f2` | 262 |
| 5 the candle store | `7b761ad` | 272 |

Four things worth recording, three of them mine.

**Two commits went in with clippy red and were amended.** Both times the gate
command was piped — `cargo clippy ... | tail -2` — and under `set -e` a
pipeline's exit status is the last stage's, so `tail` succeeding hid clippy
failing. The same mistake was then repeated inside the gate script written to
prevent it. The script now redirects to a file and tests the exit status
directly, and no gate command in this run is piped. Both affected commits
(Tasks 2 and 4) were amended after the real failure was seen, so no commit on
this branch is red; the branch was checked at its tip after each amend.

**The dead-code lint fires on every module before its consumer lands.** This is
a binary crate, so anything not yet called from `main` is dead code and clippy
runs at `-D warnings`. Each new module carries a narrow `#![allow(dead_code)]`
with a comment naming the task that removes it. Task 13 sweeps them and Task 14
greps for leftovers — a plan step added for this, because an allowance left
behind silently stops catching real dead code later.

**`cargo test --lib` does not work here.** `PLAN_15.md` uses it in several task
steps; this crate has only a `[[bin]]` target, so the command is
`cargo test <filter>`. Not corrected in the plan retroactively — the plan is the
artifact that was gated, and this belongs in the run.

**A test helper raced itself, and a test asserted the wrong thing.** The store's
`sha` helper derived its scratch file's name from the content's length and first
byte; every fixture writes identical bytes, `cargo test` runs on many threads,
and two threads wrote and deleted one path underneath each other. Replaced with a
process-wide counter. Separately,
`the_right_size_and_the_wrong_bytes_are_caught_by_the_digest` flipped the file's
last byte, which is header JSON, so check 3 caught it and the digest was never
reached — the test passed for the wrong reason in the sense that it proved the
format check, not the digest. The safetensors fixture now carries a real tensor
data region and the flip lands there. Found by running the tests, not by reading
them.

Mutation-tested before committing, each confirmed to turn the suite red:
zeroing the mel filterbank; replacing the Slaney scale with a linear one;
skipping the store's digest check; checking the digest before the size; and
returning `Verified` for a model nobody pinned.

## The live take, and what it found

Driven on 2026-09-10 against the real installation: the daemon started on the
owner's own state and configuration directories, `large-v3-turbo` installed by
`herdr-voice model --choose` over the network and verified against the pinned
byte count and digest, `doctor` reporting `engine ok — "candle" is ready,
running on the GPU, through Metal`. The take was two `dictate` invocations about
21 seconds apart, delivered to the pane pinned at the start, unsent, at −34.1 dB.

**The phrase is not recorded anywhere in this repository and must not be.** It
named a client and an internal system. `docs/evidence.md` describes it in
English, the way that file's "Context and its effect on the transcript" section
already describes its own phrase. It was not read out of the pane.

### The defect: the multiplexer's own name came back as an English word, twice

Established before attributing it, because two of the three things that could
have restored it were absent before recognition began.

**1. The pane's working directory was this repository — and the bias string
still could not carry the term.** The journal recorded `file_count=40
file_chars=393 conversation_chars=1124 prompt_chars=600 truncated=true`.
Checked against git afterwards: of the 55 paths in `git status` and the last
twenty commits, **none** contains the multiplexer's name. `bias::collect`
gathers file and directory *names*; the name is in this repository's file
*contents* and in its upstream repository name, and the checkout is a worktree
directory named for the issue. So the answer is neither "the term was absent
from the directory" nor "the term was available and recognition lost it": the
term was everywhere in the directory and nowhere in the kind of thing the bias
mechanism collects.

**2. The rewrite stage did not run at all.** `[rewrite] engine` defaults to
`agent`, which no build invokes; the daemon said so once and delivered the
transcript unrewritten. `docs/evidence.md` already records that it is the
rewrite stage and not recognition that restores terms. So the stage that would
have repaired this never saw the take.

**3. It is issue #31's mechanic, reproduced, but not a complete explanation.**
#31 says file names are assembled first and the cut falls on the conversation,
which loses silently. That happened here exactly: 393 of the 600 characters went
to file names and the conversation was cut from 1 124 characters into what was
left. But #31 also argues that the file-name component is "the one that works",
and here it contributed nothing usable — it could not, for the reason above.
Whether the term sat in the discarded 900 characters of conversation cannot be
established without reading that pane, which this run was told not to do.

So: **not a defect of #15.** The built-in engine transcribed what it was given,
with the right punctuation and one English word intact. What failed is upstream
of it — a bias mechanism that collects names rather than terms, and a rewrite
stage that is unconfigured by default. Recorded here and in `docs/evidence.md`;
no GitHub issue opened, that being the owner's call.

### A separate finding, more serious than the transcription miss

**The daemon writes every transcript to its journal, unconditionally.**
`src/daemon.rs:365` calls `runtime.journal.write(&delivering_line(&text))`, and
`delivering_line` (`src/daemon.rs:448-450`) is `format!("delivering: {text}")`.
The default journal is `StderrJournal` (`src/daemon.rs:440-442`), and `[ui]
journal` does not gate it. Under herdr the daemon's stderr is what `herdr plugin
log list` shows.

So every dictated phrase is written out in full — and this take proved it with
speech that named a client and an internal system. For a dictation plugin whose
own repository rule is that such names must never be recorded, that is a
confidentiality defect, not a logging preference. It is also inconsistent with
what this repository already decided next door: `bias_line` was deliberately
built never to contain the bias string, on a hit or a miss.

It is **pre-existing**, from #22, and not introduced by this issue, so it is not
fixed here. It is the most urgent thing this run found. The transcripts this run
produced were kept only in a scratch file outside the repository, which has been
scrubbed of every `delivering:` line and the daemon stopped.

### Whether this makes #27 more urgent: yes

#27 is "doctor says command is ready without having checked anything about the
program". The bare-machine case is still handled well — with nothing configured,
`doctor` says `engine missing — [stt] engine is "command" but [stt] command is
empty, so there is nothing to run`, with a working example, and exits 1;
verified by running it. #27 bites one step later, as soon as the person fills
that key in.

What the default decision changes is which path is the common one. Before,
`candle` was the default and `doctor` verified its model by exact name, byte
count, safetensors header and pinned digest. Now the default is `command`, whose
program `doctor` does not check at all. The default path went from fully
verified to unverified, so #27 stops being a wart on a minority configuration
and becomes the first thing a new user meets. Not fixed here.

## The default engine is `command`

- `src/config.rs`: `Stt::default().engine` is `"command"`, with the reason and
  the measurement beside it; the doc comment no longer says two engines are
  unbuilt. `every_key_has_a_default` updated.
- `docs/design.md` §4: `command` is listed first and named the default with the
  measurement; `candle` is described as fully supported and chosen by
  configuration. §7's block shows `engine = "command"`.
- `README.md`: the default is described as `command`, with `candle` as the
  no-external-program option and a pointer to the measurement.

### What the model picker now means

`herdr-voice model` and `--choose` still work and nothing in them is
unreachable, but the flow has a gap at its end: **installing a model no longer
implies using it.** `--choose` downloads, verifies, and then writes `[stt]
model` — never `[stt] engine`. On a default install that leaves the person with
a verified multi-gigabyte model on disk and an engine that ignores it.

This was not reasoned out; it happened. Installing `large-v3-turbo` for the live
take printed "large-v3-turbo is installed and already configured" and wrote no
configuration file at all, because that model is already the default `[stt]
model`. The engine had to be set by hand for the take to exercise candle.
Worth an issue; not opened here.

### Lines in the artifacts that assumed candle was the default

`AC_15.md` and `DESIGN_15.md` were gated as they stand and are left as written;
these are the places a reader must not take at face value.

| where | what it says | now |
|---|---|---|
| `AC_15.md`, as-is | "`\"candle\"` is the shipped default of `[stt] engine`… so a fresh install refuses every take until this issue lands" | the default is `command`; a fresh install refuses every take because `[stt] command` is empty |
| `AC_15.md`, AC-10 | frames `doctor`'s model line around `engine = "candle"` being the common case | still correct for that engine, no longer the default one |
| `AC_15.md`, out of scope | "a plugin installed by somebody else would need an external binary to hear anything" as the reason no release is tagged | that is now the shipped default, deliberately |
| `DESIGN_15.md` §0 | "The model list and the download source were settled by the owner" | still true, and the engine they belong to is no longer the default |
| `DESIGN_15.md` §3 | "`large-v3-turbo` stays the default of `[stt] model`" | still true: `[stt] model` is unchanged, it is `[stt] engine` that moved |
| `DESIGN_15.md` §8 | the device report at daemon start | only printed when the configured engine is candle, so a default install never sees it |
