# DESIGN_15

How `[stt] engine = "candle"` is built: a Whisper model running inside the daemon,
a pinned catalogue of models to choose from, and a download that is verified
before anything loads it.

Every number below was measured on this machine on 2026-09-09 with a throwaway
probe crate, not recalled. The probe is not kept; section 11 lists what it
established and what it cost.

## 0. What is already fixed, and is not revisited here

`stt::Engine` keeps its one method (`src/stt.rs:17-19`) — nothing this issue
needs travels through the trait; section 2c says what carries it instead. The
take path (`daemon::transcribe`, `src/daemon.rs:315`) gets no new branch. Context, rewrite
and delivery are untouched. `stt::model::locate` and the `ggml-<model>.bin`
contract keep serving `[stt] command` exactly as they do today, unchanged.

> **Superseded in one respect, 2026-09-10.** The owner chose `command` as the
> shipped default for `[stt] engine`, on the measurement in `docs/evidence.md`.
> Everything in this document about how the `candle` engine is built, verified
> and chosen still holds; it is no longer what an unconfigured install runs.
> Sections 3 and 8 carry the specific consequences.

The model list and the download source were settled by the owner on 2026-09-09:
six models, `tiny` through `large-v3`, fetched from the `openai/whisper-*`
repositories on Hugging Face, pinned by commit and verified by SHA-256. They are
therefore not provisional and do not go into `docs/decisions.md`, which records
what was decided *without* him.

## 1. Where the engine lives

### Context

Recognition is one trait with one method, and the engine is built once at daemon
start (`stt::resolve`, called at `src/daemon.rs:517`). Everything the built-in
engine needs beyond the per-take audio path and bias string — the weights, the
tokenizer, the mel filterbank, the device — is construction-time state.

### Problem

A Whisper implementation is several thousand lines of tensor work if written
here, and none of it is what this project is about. But the parts that decide
whether a transcript is right — which frames go into which window, what tokens
precede the audio, where a window's text ends — must be testable on a machine
with no weights, no network and no GPU, which is the whole test suite's
standing requirement.

### Decision

`candle-transformers` supplies the network; this repository supplies everything
around it, split so that the tensor half is thin and the decidable half is pure.

```
src/stt/candle.rs          CandleEngine: implements stt::Engine. Owns the model,
                           the tokenizer, the filterbank and the device.
src/stt/candle/mel.rs      the mel filterbank from the specification, and the
                           call into candle's pcm_to_mel
src/stt/candle/plan.rs     pure: window planning, prompt-token assembly, the
                           timestamp advance, text assembly from tokens.
                           No tensor appears in any signature here.
src/stt/candle/decode.rs   the tensor half: encoder pass, greedy argmax loop.
                           Calls plan.rs for every decision it makes.
src/stt/catalogue.rs       the six models, pinned: repository, commit, and for
                           each of three files an exact byte count and SHA-256.
                           catalogue::get(identifier) -> Option<&'static Entry>
src/stt/fetch.rs           download and verify, over the ureq already in the tree
src/stt/candle/store.rs    what a candle model on disk is, and whether this one is
src/audio/wav.rs           gains read(), the inverse of the encode() it already has
```

### Why

`plan.rs` is where a wrong answer produces a wrong transcript, and it can be
tested exhaustively with no model present. `decode.rs` is a loop that asks
`plan.rs` what to do and multiplies matrices; what remains untestable without
weights is reduced to that. This is the same split the pipeline already uses
between its stages, applied inside one of them.

## 2. The model on disk

### Context

`stt::model::locate` accepts one file, `ggml-<model>.bin`, on four checks in
order: the exact name, at least 1 MiB, the four leading bytes `6C 6D 67 67`, and
a `.sha256` sidecar when one happens to exist (`src/stt/model.rs:97-154`). The
issue requires that contract be "honoured, not loosened".

### Problem

`candle` cannot read a `ggml` file. The weights it reads are `safetensors`, and a
Whisper model is three files, not one: `model.safetensors`, `config.json` and
`tokenizer.json`. The contract's four *checks* apply directly. Three of its
*constants* — one file, that name template, that magic — describe a different
format, and reusing them would mean either renaming a safetensors file to
`ggml-*.bin` or dropping checks.

### Decision

A second store beside the first, with the same four checks made stronger, and no
change of any kind to the first.

A candle model lives at `<state>/models/candle/<identifier>/` and holds exactly
`model.safetensors`, `config.json` and `tokenizer.json`. The lookup is

```rust
pub fn locate(
    models: &Path,
    identifier: &str,
    expected: Option<&catalogue::Entry>,
) -> Result<Found, StoreError>
```

— the catalogue entry is a **parameter**, not something `locate` fetches for
itself, so a test can hand it an entry describing a small fabricated file.
Production callers pass `catalogue::get(identifier)`, which is `None` for an
identifier the catalogue does not know. It checks, in order and per file:

1. **The exact name**, in the exact directory. Nothing is matched by substring,
   and a leftover `model.safetensors.part` matches nothing.
2. **The exact byte count** from the catalogue — not a floor. A size floor exists
   in the `ggml` contract because no true size was known there; here one is.
3. **The format**: `model.safetensors` begins with an 8-byte little-endian header
   length followed by that many bytes of JSON naming tensors, and both are read
   and checked. `config.json` and `tokenizer.json` must parse.
4. **The SHA-256** from the catalogue, computed with the `Sha256` already in
   `src/stt/model.rs:173-278` and compared. Always, not only when a sidecar
   exists.

An identifier the catalogue does not know is still usable: checks 1 and 3 apply,
2 and 4 are skipped, and the report says so in as many words. A person who
placed weights there by hand keeps a working plugin, and is told which checks
were not made. That is the third outcome, and it is in the type:

```rust
pub struct ModelDir { pub dir: PathBuf }          // the three paths derive from it

pub enum Found {
    /// Every check made, the pinned byte count and digest among them.
    Verified(ModelDir),
    /// Structurally sound, but the catalogue does not know this identifier,
    /// so checks 2 and 4 were not made. The identifier is carried rather than
    /// re-derived, so the report names the value that was not found.
    Unpinned { dir: ModelDir, identifier: String },
}

pub enum StoreError {
    Missing { dir: PathBuf, identifier: String, absent: &'static str },
    WrongSize { path: PathBuf, expected: u64, found: u64 },
    NotSafetensors { path: PathBuf, why: String },
    Unparsable { path: PathBuf, why: String },     // config.json, tokenizer.json
    DigestMismatch { path: PathBuf },
    Unreadable { path: PathBuf, why: String },
}
```

`StoreError` is a new type beside `model::ModelError`, not variants added to it.
`ModelError`'s five variants all name a single file and three of them are
`ggml`-specific; a candle model is a directory, and widening the old type would
be the change to the `ggml` store this section promises not to make.

### Why

Every check the `ggml` contract makes is made here, and two of them are exact
rather than heuristic. The `ggml` contract is left byte for byte as it is,
because `[stt] command` still depends on it and this issue has no business
changing what serves another engine. The separate directory is what lets both be
exact at once: two formats cannot share one name template without one of them
being loosened.

## 2b. How a lookup reaches `resolve` and `doctor`

### Context

`stt::locate_configured_model` returns `Option<Result<PathBuf,
model::ModelError>>` and one call feeds both `resolve_with` and
`doctor::model_finding_from` (`src/stt.rs:87-96`, `src/doctor.rs:199-257`), so a
multi-gigabyte model is read and hashed once per `doctor` run rather than once
per line. `None` means "nothing here asks for a model" and prints `not used`.

### Problem

That type can carry one engine's answer. It cannot carry a candle lookup: the
value is a directory rather than a path to a file, the error type is different,
and there is a third outcome — present but unpinned — that `Result` has no room
for. AC-10 requires `doctor` to report the candle engine's real model state, so
something has to widen; section 2 forbids that something being `ModelError`.

### Decision

`locate_configured_model` keeps its name, its single call site per `doctor` run
and its purpose, and returns a three-way enum instead of an `Option<Result<..>>`:

```rust
pub enum ModelState {
    /// Nothing in this configuration would ever look for a model:
    /// engine = "http", or "command" with no {model} placeholder.
    NotUsed,
    /// The ggml file [stt] command asks for. Unchanged in every respect.
    Ggml(Result<PathBuf, model::ModelError>),
    /// The candle directory [stt] engine = "candle" loads.
    Candle(Result<candle::store::Found, candle::store::StoreError>),
}
```

`check_with(stt, state)` matches it — `Ggml(Ok(path))` approves the command
engine exactly as today, `Candle(Ok(found))` approves the candle engine for that
directory, either `Err` becomes the engine error naming it, and `NotUsed` reaches
only the arms that need no model — and `resolve_with` builds what it approved
(section 2c). `doctor::model_finding_from` matches the same value: `Ggml(Ok(_))` and `Candle(Ok(Verified(_)))` are `Ok`,
`Candle(Ok(Unpinned(_)))` is `Ok` with a detail naming the identifier and saying
the byte count and the digest were not checked because the catalogue does not
know it, every `Err` is `Missing` with the error's own message, and `NotUsed`
keeps the wording it has today.

### Why

One value, one lookup, two readers — which is the property `doctor` was given in
PR #20 and must not lose. An enum rather than a second parallel function because
two functions would let the engine line and the model line disagree, which is
precisely what `engine_finding_from` and `model_finding_from` were merged onto
one lookup to prevent.

## 2c. Checking without loading, and where the device is reported

### Context

`doctor` calls `stt::resolve_with` and prints whatever it says, so the engine
line and the daemon can never disagree (`src/doctor.rs:199-215`). Section 8 makes
the candle engine load eagerly: 1.6 GB of weights, a tokenizer, and an encoder
pass over 30 seconds of silence. Section 8 also requires the chosen device to
appear on `doctor`'s engine line.

### Problem

Two things break at once if nothing is said. `herdr-voice doctor` would load 1.6
GB and run inference to print six lines — the opposite of the reason `doctor`
was given a single model lookup in the first place. And the device is chosen
inside `CandleEngine::new`, behind a `Box<dyn Engine>` with one method, so its
name has nowhere to travel to the line that must print it.

### Decision

Validation and construction are separated, and validation is what `doctor` runs.

```rust
/// Everything that can be decided without reading weights.
pub enum Ready {
    /// `model` is the ggml path when the argument list has a {model}
    /// placeholder and None when it brings its own — the same Option
    /// `CommandEngine::new` takes today (`src/stt.rs:123-132`). It is carried
    /// here because `check_with` consumes the lookup and `resolve_with` builds
    /// only from what `check_with` returned.
    Command { program: String, model: Option<PathBuf> },
    Candle { device: device::Selection, model: candle::store::Found },
}

/// Every check, no weights, no device work beyond asking for a handle.
pub fn check_with(stt: &Stt, state: ModelState) -> Result<Ready, EngineError>;

/// check_with, and then actually build what it approved.
pub fn resolve_with(stt: &Stt, state: ModelState)
    -> Result<Box<dyn Engine + Send + Sync>, EngineError>;
```

`resolve_with` is `check_with` followed by construction and nothing else, so
every error `doctor` prints is produced by the code the daemon runs — the
property that mattered, kept — while the load happens only where a take will use
it. `doctor` calls `check_with`; the daemon calls `resolve_with`.

The device travels in `Ready::Candle`, not through the trait:

```rust
pub enum Selection {
    Metal,
    /// Why the CPU: the build has no metal feature, or asking for a device failed.
    Cpu { why: &'static str },
}

pub fn select() -> Selection;                  // cfg(metal) + Device::new_metal(0)
pub fn describe(selection: &Selection) -> String;   // the sentence doctor prints
```

`select` asks for a device handle and nothing more — no weights, no kernels — so
`doctor` calling it costs nothing. `CandleEngine::new(dir, language, selection)`
takes the selection rather than choosing one, which is what lets a test build the
engine on a device of its choosing and lets `describe` be tested against both
variants with no hardware assumption at all.

### Why

Splitting on "does this need the weights" is the only line that puts the eager
load where section 8 wants it and keeps `doctor` cheap, and defining
`resolve_with` in terms of `check_with` is what stops the split from becoming two
answers to one question. Reporting the device through a value rather than through
the trait keeps section 0's promise: `Engine` still has one method, because the
device is construction-time state and the trait is for per-take work.

## 3. The catalogue

### Context

Verifying a download against a digest requires knowing the digest before the
download. Knowing it requires pinning what is being downloaded.

### Problem

A Hugging Face repository's `main` moves. A digest pinned against a moving branch
turns a legitimate upstream update into a verification failure the person cannot
act on — the file is genuine, and the plugin calls it corrupt.

### Decision

A compile-time constant table in `src/stt/catalogue.rs`: for each of six
identifiers, the repository, the **commit** it is pinned to, and for each of the
three files an exact byte count and SHA-256. Read from the Hugging Face API on
2026-09-09; the two non-LFS files were downloaded and hashed rather than trusted,
because the API reports a git blob SHA-1 for those, not a SHA-256.

| identifier | weights | commit | mel bins |
|---|---|---|---|
| `tiny` | 151 061 672 B | `169d4a4341b33bc18d8881c4b69c2e104e1cc0af` | 80 |
| `base` | 290 403 936 B | `e37978b90ca9030d5170a5c07aadb050351a65bb` | 80 |
| `small` | 966 995 080 B | `973afd24965f72e36ca33b3055d56a652f456b4d` | 80 |
| `large-v3-turbo` | 1 617 824 864 B | `41f01f3fe87f28c78e2fbf8b568835947dd65ed9` | 128 |
| `medium` | 3 055 544 304 B | `abdf7c39ab9d0397620ccaea8974cc764cd0953e` | 80 |
| `large-v3` | 3 087 130 976 B | `06f233fe06e710322aca913c1bc4249a0d71fce1` | 128 |

`large-v3-turbo` stays the default of `[stt] model`: it is already the default
(`src/config.rs:88-97`) and is the model every recognition number in
`docs/evidence.md` was produced with.

> **Still true, 2026-09-10, and worth stating precisely.** The default that
> changed is `[stt] engine`, not `[stt] model`. `large-v3-turbo` remains the
> model this catalogue defaults to; it is simply not loaded unless the engine is
> set to `candle`. One consequence: `--choose` writes `[stt] model` and never
> `[stt] engine`, so on a default install installing a model does not by itself
> make anything use it.

Refreshing the table is a deliberate act with a script beside it,
`scripts/model_catalogue.py`, which regenerates it from the API and re-hashes the
small files. The script is what makes the table auditable; the table is what
makes a download verifiable offline.

### Why

Pinning the commit and the digest together is the only combination where a
failed check means what it says. Pinned digest with a moving commit accuses
upstream of corruption; a pinned commit with no digest catches a truncated
transfer only by luck of the byte count.

## 4. Downloading

### Context

`ureq` 2 is already a dependency, blocking, with `rustls`
(`docs/decisions.md`, 2026-09-04, #36). Nothing in the crate downloads anything
today.

### Problem

An interrupted download must not leave a file that a later run mistakes for a
model, and the failure the person sees must name what to do next rather than
surfacing later as a load error inside a tensor library.

### Decision

```rust
pub fn model(identifier: &str, models: &Path, progress: &mut dyn Progress)
    -> Result<PathBuf, FetchError>;                 // production: catalogue::BASE

fn model_from(base: &str, entry: &catalogue::Entry, dir: &Path,
              progress: &mut dyn Progress) -> Result<(), FetchError>;
```

The base address is a **parameter of the inner function**, so a test drives it
against a scratch `TcpListener` on loopback; the outer function is the one-line
wrapper that looks the identifier up and passes `catalogue::BASE`,
`"https://huggingface.co"`. `model_from` fetches each of the three files from
`<base>/<repo>/resolve/<commit>/<file>` into `<file>.part` in the destination
directory, hashing as the bytes arrive. When the
stream ends it compares the byte count and the digest against the catalogue.
Only when both match is the `.part` renamed to its real name. On any failure the
`.part` is removed and the error names the file, what was expected, what arrived,
and that the download can simply be run again.

No new crate. `hf-hub` was considered and rejected: it brings its own HTTP stack
and its own on-disk cache layout, which would sit beside this plugin's own
`<state>/models/` rather than in it, and it does the one thing this needs — a
`GET` of a pinned URL — behind an abstraction built for a different problem.

### Why

A file under its real name is, by construction, a file that passed every check.
That is what makes AC-6 true rather than hoped for: there is no window in which a
half-written `model.safetensors` exists. Hashing during the transfer rather than
after it means a multi-gigabyte file is read once, the same concern that made
`doctor` locate a model once per run rather than once per line.

## 5. Choosing a model

### Context

The manifest already declares a `model` pane — "Choose a speech model", a 70% ×
16 popup running `herdr-voice model --choose`. `src/main.rs` parses `model` and
routes it to the arm that prints "not implemented yet" and exits 69
(`src/main.rs:161-168`); `--choose` is read by nothing.

### Problem

The chooser has to work before there is anything to choose with: no model, and
possibly no configuration file. It also runs in its own process, not in the
daemon, so a model it downloads is not the model the running daemon holds.

### Decision

`herdr-voice model` prints the catalogue — identifier, size, mel bins, and
whether it is already present and verified — and exits. `herdr-voice model
--choose` prints the same list, reads one number from standard input, downloads
that model with a progress line, verifies it, and then, when the chosen
identifier differs from `[stt] model`, edits the configuration file to name it:
the `model = ` line inside the `[stt]` table is replaced, or inserted after the
header, or the file is created with just that table. The edit is line-oriented so
that comments and every other key survive; the `toml` crate in the tree parses
and does not preserve formatting, and `toml_edit` is a new dependency for one
line of text.

Having changed the configuration, the chooser says in one line that a running
daemon still holds the previous model and names what to do: restart it. It does
not gain a wire command to reload — a new command is a new user-visible name,
which `docs/decisions.md` (2026-08-24, #3) reserves.

`--choose` with nothing on standard input, or an answer that is not one of the
offered numbers, prints the list again and exits non-zero rather than guessing.

### Why

Two entry points because two questions are being asked: `doctor` and a person
reading the pane want to know what exists, and only sometimes does anyone want to
spend three gigabytes finding out. Editing the configuration rather than printing
a snippet is what makes "the model is one the person chose" true across a
restart; the `setup` action's precedent of printing a snippet exists because
keybindings live in *herdr's* configuration, which is not this plugin's to write.
This file is.

## 6. Audio in

### Context

A take is 16 kHz mono 16-bit PCM in a WAV container, written by
`audio::wav::write` (`src/audio/wav.rs:45`). Whisper's encoder does not take
samples; it takes a log-mel spectrogram, 80 or 128 bins depending on the model.

### Problem

Two gaps. Nothing in this crate reads a WAV back — every engine so far handed the
path to another program. And `candle_transformers`'s `pcm_to_mel` takes the mel
filterbank as a parameter; it does not compute one. Candle's own Whisper example
ships two precomputed filterbanks as opaque binary blobs beside the example.

### Decision

`audio::wav::read(path) -> Result<(Vec<f32>, u32)>` parses the container this
crate itself writes: the `fmt ` chunk is checked for one channel and 16 bits, the
`data` chunk is converted to `f32` in `[-1, 1)`, and any other shape is refused
by name — "expected 16 kHz mono 16-bit, found 44 100 Hz stereo" — rather than
being resampled. `audio::resample` exists for capture and has no business here:
the recorder already produces the one shape this reads.

The filterbank is **computed from the specification** at engine construction —
the Slaney mel scale, triangular filters over the 201 rFFT bins of a 400-sample
window, area-normalised — and tested against candle's reference blobs, which are
committed as test fixtures and not compiled into the shipped binary.

Measured: the computed 80-bin bank differs from candle's reference by at most
1.86 × 10⁻⁹ absolute, and the 128-bin bank by at most 3.73 × 10⁻⁹. Both are
below single-precision rounding at these magnitudes, so the test asserts a
tolerance of 10⁻⁶ and has four orders of magnitude of room.

### Why

The same reason SHA-256 was written from the specification and tested against the
published vectors rather than taken as a dependency
(`src/stt/model.rs:156-160` and `:173-175`): sixty lines of arithmetic with a reference to check
them against is smaller than the alternative and cannot silently drift. Here the
alternative is worse than a dependency — it is 164 KB of opaque floats in the
binary that nothing in the repository can explain or verify. Committing the same
floats as a *fixture* keeps the proof and drops the opacity: the test fails if
the computation is wrong, and the shipped binary carries none of it.

## 7. Decoding

### Context

A take is longer than the 30 seconds Whisper's encoder accepts — the one real
take in `docs/evidence.md` is 70 seconds — so a take is several windows. The
bias string already reaches the engine (`Engine::transcribe`'s second parameter,
#26), and `docs/evidence.md` establishes that it is the bias prompt, and nothing
else, that restores English technical terms inside Russian speech.

### Problem

Four decisions with no default: how a window's end is found, what happens to the
tail of a take that does not fill a window, how the bias string enters the model,
and how much decoding machinery is enough.

### Decision

**Greedy, temperature 0, no beam search and no temperature fallback.** The next
token is the argmax of the logits. Measured decode for the default model on a
66-second take is 5.1 s; the reference implementation's fallback ladder would
multiply that by up to six in exactly the cases it fires. What it buys — a retry
when the output degenerates into repetition — is not free to omit, and section 10
says what is left open by omitting it.

**Windows advance by the last timestamp the window produced.** Each window is
decoded with timestamp tokens enabled; the text is the non-timestamp tokens, and
the next window starts at the frame the last timestamp names, not 30 seconds
later. Measured on a 66-second sample: window 1 advanced 2 998 frames rather than
3 000, and the boundary read continuously — "…can do for you." / "Ask what you
can do for your country." — with no duplicated or dropped words.

**A window with less than one second of real audio is not decoded.** Found by
running it: a final window holding 2 frames of audio and 2 998 of zero padding
returned "[Music]" — confident text transcribed from silence. A 100-frame
minimum removes it, and the take ends there.

**The bias string enters as an initial prompt.** The token sequence for a window
is `<|startofprev|>` followed by the bias string's tokens, then
`<|startoftranscript|>`, the language token and `<|transcribe|>`. The prompt is
cut to the last half of the text context (`max_target_positions / 2 - 1` tokens,
224 for every model in the catalogue), keeping the end because that is where
`bias::collect` puts the conversation. From the second window on, the previous
window's text is appended to the bias string in the same prompt, which is how
Whisper carries context across a boundary. Verified in the probe: the same audio
with and without a prompt produced different punctuation, so the path is live
rather than merely present.

**Language.** `[stt] language = "auto"` detects once, on the first window, by
running the decoder one step from `<|startoftranscript|>` and taking the argmax
over the language tokens; the result is reused for every later window. Any other
value is `<|xx|>` looked up directly, and a value the tokenizer does not know is
an error naming the value and that `auto` detects instead. Measured: detection
costs 6 ms warm.

### Why

Greedy is what the ticket allows and what the measurements support: recognition
is not the slow stage in this pipeline, and the stage that repairs terms is the
rewrite one, which already exists. The timestamp advance is the reference
implementation's own answer to the boundary problem and it cost one line of
arithmetic to adopt. The one-second guard was not a design idea; it was a
hallucination observed in the probe and closed. And an engine that accepted the
bias string and dropped it would undo issue #26 while appearing to succeed —
the worst shape of failure this repository has a rule against.

## 8. The device, loading, and the first take

### Context

`stt::resolve` runs once at daemon start and returns either an engine or the
reason there is none (`src/daemon.rs:517`). `docs/design.md` section 2 states the
purpose plainly: "The daemon keeps the speech model resident, which removes the
model load from every dictation." Section 9's `metal` feature decides what the
binary *can* use; nothing so far says what it *does* use.

### The device

`device::select()` asks for `Device::new_metal(0)` on macOS and answers
`Cpu { why }` when that fails or when the binary was built without the feature —
which is every Linux and Windows build, including both CI runners. The engine is
never refused over the device. The selection is handed to `CandleEngine::new`
rather than made inside it, so that `doctor` can report it without building an
engine and a test can pin it; section 2c gives the types.

**It is not silent about it.** Falling back costs a factor of ten, measured on
this machine with a 66-second take: the default model takes 5.1 s on Metal and
**52.3 s on the CPU**, and `tiny` takes 0.69 s against 6.4 s. Fifty-two seconds
for a minute of speech is a different product, not a slower one, so the fallback
is reported in both places a person looks — one line on the daemon's standard
error at start, and the `engine` line in `doctor`, built by
`device::describe` from the `Selection` inside `Ready::Candle`, which says the
engine is ready on the CPU, names the measured order of magnitude, and names
`tiny` as the model that stays usable there. A take is never refused for it: a slow transcript beats
no transcript, and the person is told which they are getting before they wait.

There is no configuration key for the device. A key is a user-visible name
(`docs/decisions.md`, 2026-08-24, #3), the automatic choice is right on every
machine looked at, and somebody who wants the CPU on a Metal machine has no
reason yet that this issue knows of.

### Problem

Two costs are hidden in "load": reading 1.6 GB of weights, and Metal compiling
its shaders the first time a kernel runs. Neither belongs on the path of somebody
who has just finished speaking.

### Decision

The engine is built eagerly in `resolve` — which the daemon calls and `doctor`
does not (section 2c) — weights and tokenizer and all, and then runs **one
encoder pass over 30 seconds of silence** before returning. Measured
for the default model: 0.25–0.67 s to load with a warm page cache, 3.0 s cold,
and 0.77–0.84 s for the warm-up pass. A separate cost was seen once and not
again: the very first Metal run on this machine took 3.4 s in kernel compilation,
which the operating system then cached across processes.

### Why

`resolve` already returns a `Result` the daemon keeps and reports when a take
finishes, so an eager load has somewhere to fail to. Doing it lazily would move
roughly a second onto the first dictation and put the failure at the moment the
person is waiting for text. The warm-up pass costs the daemon under a second at
start and is the difference between the first take of a session behaving like
the rest and behaving three seconds worse.

## 9. Dependencies, and the Metal feature

### Context

The crate has six dependencies. `docs/decisions.md` (2026-08-24, #3) establishes
that dependencies here are decided deliberately and the reason written down.

### Decision

Four crates: `candle-core`, `candle-nn`, `candle-transformers` and
`tokenizers = "0.22"`. Not `hf-hub` (section 4), not `byteorder`, not `rand` —
greedy decoding needs no sampling.

`tokenizers` is pinned to **0.22, not the current 0.23**, because `candle-core`
depends on 0.22 itself: asking for 0.23 puts two copies of a large crate in the
tree, and matching it takes the tree from 151 crates to 149. It is taken with
`default-features = false, features = ["onig"]`. The pure-Rust-looking
alternative, `unstable_wasm`, was tried and is not one — it adds a second
`fancy-regex` and pulls `onig` anyway.

**`onig_sys`, a C library built by `cc`, enters the tree no matter what this
manifest says**, because `candle-core`'s own `tokenizers` dependency brings it.
That is a fact for `docs/design.md` section 8, which promises release archives
for macOS on two architectures, Linux on two and Windows on one: those builds now
need a C compiler. Nothing in this issue tags a release, so nothing here proves
the matrix still builds — named so that whoever tags one starts from it rather
than discovering it.

**Every candle crate carries the `metal` feature on macOS, not only
`candle-core`.** Found by running it: with the feature on `candle-core` alone,
inference fails at the first encoder layer with `Metal error no metal
implementation for layer-norm`, because the fused kernel lives behind the feature
on `candle-nn`. The manifest expresses this per target, as `CLAUDE.md` requires
of platform differences.

Measured cost: a clean release build of a probe carrying the four crates took
58 s, and its dependency tree is 149 crates against this repository's present 87.

One thing this issue does not settle and should not pretend to: `Cargo.toml`
declares `rust-version = "1.82"`, and both CI workflows build
`dtolnay/rust-toolchain@stable` (`.github/workflows/check.yml:33`,
`release.yml:26`), so nothing has ever enforced that number. None of the candle
crates declares a `rust-version` at all, so cargo cannot check it either, and the
probe was built on 1.98. Whether 1.82 still holds is unknown, and the plan should
either verify it or change the declaration to what was actually verified — an
unenforced number that is wrong is worse than an honest one.

### Why

Written down because it is a large change to a small crate, and because the Metal
feature is the kind of fact that costs an afternoon when it is discovered during
implementation instead of now.

## 10. What is deliberately not built, and what that leaves open

- **No temperature fallback.** When greedy decoding degenerates into repeating a
  phrase, the reference implementation retries the window at a higher
  temperature. Here it does not; the repetition is delivered. It is bounded — a
  window stops at the text context limit — but it is a real behaviour somebody
  will eventually meet. It is not in the ticket, and the rewrite stage sees the
  result before the person does.
- **No beam search.** Same reason, stated by the ticket.
- **Nothing about hallucinated speech from silence.** `docs/evidence.md` already
  records a minute of room tone producing confident text through `whisper-cli`
  and says "Nothing is changed here yet". This engine inherits that unchanged;
  only the padding case in section 7, which this engine creates itself, is closed
  here.
- **F16 is not available, and the engine is roughly three times slower than
  whisper.cpp.** Measured: the default model transcribes a 66-second take in
  5.1 s, against the 1.65 s `docs/evidence.md` records for `whisper-cli` on a
  70-second take with the same model on Metal. Loading the weights as F16, which
  would be the obvious answer, fails — `candle-transformers` 0.11's Whisper mixes
  F32 constants into the graph and the first addition reports `dtype mismatch in
  add, lhs: F16, rhs: F32`. That is upstream, not here. Issue #2 is where this
  comparison is made properly; this section exists so #2 starts from a number
  rather than from nothing.

## 11. Testing with no model, no network and no GPU

Every test in the suite must pass with the models directory absent and the
network unreachable, and nothing may assume a GPU.

| what | how it is tested with nothing present |
|---|---|
| the mel filterbank | computed and compared against candle's reference blobs, committed as fixtures, tolerance 10⁻⁶ |
| `wav::read` | round-trip against `wav::encode`, plus refusals for stereo, 8-bit and a wrong rate |
| the catalogue | every entry has three files, a 64-character digest and a non-zero size; `Stt::default().model` is one of them |
| `store::locate` | scratch directories with fabricated files and a fabricated `catalogue::Entry` describing them — the entry is a parameter, so the "right one" case is a few kilobytes, not 151 MB: a right one, a wrong byte count, a wrong digest, a missing file, a `.part` left behind, a file that is not safetensors, and `expected: None` giving `Unpinned` |
| `fetch` | `model_from` against a scratch `TcpListener` on loopback — the base address is its parameter — using the idiom `src/rewrite/http.rs` already uses: a good transfer, a short one, a wrong digest, a non-200. In every failing case, that no file exists under the real name afterwards and no `.part` is left behind. |
| window planning, the one-second guard, the timestamp advance, prompt assembly, text assembly | `plan.rs` is pure, and these are ordinary unit tests |
| the chooser's configuration edit | scratch files: no file, a file with no `[stt]`, a file with `[stt]` and no `model`, one with a `model` line and comments around it |
| `resolve` for `"candle"` with no model | an error naming the model, the directory and `herdr-voice model --choose` |
| `ModelState`'s three arms through `check_with` and `doctor` | constructed directly, no weights needed — `check_with` reads none: `NotUsed`, `Ggml(Ok/Err)`, `Candle(Ok(Verified))`, `Candle(Ok(Unpinned))`, `Candle(Err(..))` — one `doctor` line asserted per arm, including that `Unpinned` is `Ok` and names the checks it skipped |
| the CPU fallback's report | `device::describe(&Selection::Cpu { why })` and `describe(&Selection::Metal)` called directly — `Selection` is a plain enum a test constructs, so neither variant needs the hardware it names. The `doctor` engine line is then asserted from a `Ready::Candle` carrying each. |
| that `doctor` reads no weights | `check_with` is the only entry point `doctor::run` uses, asserted by the same thread-local counter idiom `src/stt/model.rs:23-48` already uses for `locate` — a candle model that is present and verified must still produce zero weight reads |
| the encoder and the decoder loop | not tested without weights. What is left untested is a loop that asks `plan.rs` what to do and multiplies matrices; section 1 exists to make that residue small. AC-14 covers it by hand on macOS. |

## 12. Sequencing with #16

This branch changes `stt::resolve_with`'s `"candle"` arm and
`doctor::model_finding_from`; #16 changes the `"http"` arm and the same
`doctor` area. Whichever merges first, the other rebases. `tasks/36/RUN_36.md`
records the same shape of conflict twice; it is match-arm and comment text.

Two things worth naming so they are not a surprise. `stt::locate_configured_model`
(`src/stt.rs:87-96`) currently answers only for `engine = "command"`; this branch
widens it to `ModelState` so `engine = "candle"` also reports a real model state,
which is what AC-10 requires. And `doctor` moves from `resolve_with` to
`check_with` (section 2c), so #16's `http` arm must exist in `check_with` — the
arm itself is #16's, but which function it lives in is settled here. If #16 lands first and has touched the same function, the
resolution keeps both engines' arms.
