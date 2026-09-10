# PLAN_15 — Recognition: the built-in candle engine, and choosing a model on first run

> **For agentic workers:** implement task by task, in order. Each task ends with a
> commit and a green suite. Steps use `- [ ]` for tracking.

**Goal:** `[stt] engine = "candle"` — the shipped default — transcribes a take
inside this process with no external program, using a Whisper model the person
chose from a list and this plugin downloaded and verified.

**Architecture:** `candle-transformers` supplies the network. This repository
supplies everything around it, split so the tensor half is thin and every
decision that can make a transcript wrong lives in a pure module that tests
reach with no weights, no network and no GPU.

**Spec:** `tasks/15/DESIGN_15.md`. **Criteria:** `tasks/15/AC_15.md`.

## Global Constraints

- Everything in the repository is English: code, comments, output strings,
  commits (`CLAUDE.md`, "Language").
- Nothing that identifies an employer, client, internal system or private
  machine enters the repository. Cite paths **relative to the repository root**.
  An absolute path is itself a leak and the gate rejects it.
- No panic paths in the daemon. Every user-visible failure names what to do
  next. A silent failure weighs the same as a wrong transcript.
- Every configuration key has a default; an absent configuration file is valid.
- Device selection by name, never by index.
- `cargo test` must pass with **no model file present and no network
  reachable**, and must not assume a GPU.
- Every commit message ends with:
  `Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>`
- All four gates must be green before the pull request:
  `cargo test`, `cargo clippy --all-targets -- -D warnings`, `cargo fmt --check`,
  `python3 scripts/check_manifest.py`.
- One commit per task. Never commit to `main`.

## Naming used across tasks

Fixed here so no two tasks invent different names for one thing.

```rust
// src/stt/catalogue.rs
pub struct File   { pub name: &'static str, pub bytes: u64, pub sha256: &'static str }
pub struct Entry  { pub identifier: &'static str, pub repo: &'static str,
                    pub revision: &'static str, pub mel_bins: usize,
                    pub files: [File; 3] }
pub const BASE: &str = "https://huggingface.co";
pub const MODELS: [Entry; 6];
pub fn get(identifier: &str) -> Option<&'static Entry>;
pub fn weights(e: &Entry) -> &'static File;   // the model.safetensors entry

// src/stt/candle/store.rs
pub struct ModelDir { pub dir: PathBuf }
impl ModelDir { pub fn weights(&self)->PathBuf; pub fn config(&self)->PathBuf;
                pub fn tokenizer(&self)->PathBuf; }
pub enum Found { Verified(ModelDir), Unpinned { dir: ModelDir, identifier: String } }
pub enum StoreError { Missing{dir:PathBuf,identifier:String,absent:&'static str},
                      WrongSize{path:PathBuf,expected:u64,found:u64},
                      NotSafetensors{path:PathBuf,why:String},
                      Unparsable{path:PathBuf,why:String},
                      DigestMismatch{path:PathBuf},
                      Unreadable{path:PathBuf,why:String} }
pub enum Glance { Whole, Absent, WrongSize }   // by name and size only, never hashes
pub fn locate(models:&Path, identifier:&str, expected:Option<&catalogue::Entry>)
    -> Result<Found, StoreError>;
pub fn glance(models:&Path, identifier:&str, expected:Option<&catalogue::Entry>) -> Glance;
pub fn directory(models:&Path, identifier:&str) -> PathBuf;  // models/candle/<id>

// src/stt/candle/device.rs
pub enum Selection { Metal, Cpu { why: &'static str } }
pub fn select() -> Selection;
pub fn describe(s: &Selection) -> String;
pub fn device_for(s: &Selection) -> candle_core::Device;

// src/stt.rs
pub enum ModelState { NotUsed,
                      Ggml(Result<PathBuf, model::ModelError>),
                      Candle(Result<candle::store::Found, candle::store::StoreError>) }
pub enum Ready { Command { program: String, model: Option<PathBuf> },
                 Candle  { device: device::Selection, model: candle::store::Found } }
pub fn locate_configured_model(stt:&Stt, models:&Path) -> ModelState;
pub fn check_with(stt:&Stt, state:&ModelState) -> Result<Ready, EngineError>;
pub fn resolve_with(stt:&Stt, state:ModelState)
    -> Result<Box<dyn Engine + Send + Sync>, EngineError>;

// src/stt/fetch.rs
pub trait Progress { fn file(&mut self, name:&str, total:u64);
                     fn bytes(&mut self, done:u64); fn done(&mut self, name:&str); }
pub enum FetchError { Http{url:String,why:String}, Status{url:String,code:u16},
                      ShortRead{name:String,expected:u64,found:u64},
                      Digest{name:String}, Io{path:PathBuf,why:String},
                      Unknown(String) }
pub fn model(identifier:&str, models:&Path, p:&mut dyn Progress)
    -> Result<PathBuf, FetchError>;

// src/stt/candle/plan.rs  (pure; no tensor in any signature)
pub struct Window { pub start: usize, pub len: usize }
pub fn next_window(frames:usize, seek:usize) -> Option<Window>;
pub fn advance(window:&Window, last_timestamp:Option<u32>, ts_begin:u32) -> usize;
pub fn prompt_tokens(sop:u32, bias_ids:&[u32], limit:usize) -> Vec<u32>;
pub fn text_tokens(body:&[u32], ts_begin:u32) -> Vec<u32>;
pub fn last_timestamp(body:&[u32], ts_begin:u32) -> Option<u32>;
pub const MIN_REAL_FRAMES: usize = 100;

// src/audio/wav.rs
pub fn read(path:&Path) -> Result<(Vec<f32>, u32), ReadError>;
```

---

### Task 1: The candle dependencies, and the decisions they carry

**Files:**
- Modify: `Cargo.toml`
- Modify: `docs/decisions.md`

**Interfaces:**
- Produces: `candle_core`, `candle_nn`, `candle_transformers`, `tokenizers`
  available to every later task.

- [ ] **Step 1: Add the dependencies**

In `Cargo.toml`, after the existing `[dependencies]` block's `cpal` line, keeping
the list alphabetical as it is today:

```toml
[dependencies]
candle-core = "0.11"
candle-nn = "0.11"
candle-transformers = "0.11"
cpal = "0.18"
interprocess = { version = "2.4.3", default-features = false }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
# 0.22, not 0.23: candle-core depends on 0.22 itself, and matching it keeps one
# copy of a large crate in the tree instead of two.
tokenizers = { version = "0.22", default-features = false, features = ["onig"] }
toml = "1"
ureq = { version = "2", features = ["json"] }

# Metal must be enabled on every candle crate, not only candle-core: the fused
# layer-norm kernel lives behind the feature on candle-nn, and without it
# inference fails at the first encoder layer with
# "Metal error no metal implementation for layer-norm".
[target.'cfg(target_os = "macos")'.dependencies]
candle-core = { version = "0.11", features = ["metal"] }
candle-nn = { version = "0.11", features = ["metal"] }
candle-transformers = { version = "0.11", features = ["metal"] }
```

- [ ] **Step 2: Confirm it builds, and record what it cost**

```sh
cargo build --release
cargo tree --prefix none --no-dedupe | awk '{print $1" "$2}' | sort -u | wc -l
```

Expected: it builds. Note the crate count for the S5 evidence entry; the design
measured 149 against the 87 this repository had before.

- [ ] **Step 3: Check the declared MSRV, and correct it if it is wrong**

`Cargo.toml` declares `rust-version = "1.82"` and both workflows build
`dtolnay/rust-toolchain@stable`, so nothing has ever enforced it, and no candle
crate declares a `rust-version` for cargo to check against. If a 1.82 toolchain
is available, build with it. If it is not, change the declaration to the version
actually built with and say so in the commit body — an unenforced number that is
wrong is worse than an honest one. Do not leave it unexamined.

- [ ] **Step 4: Append four rows to `docs/decisions.md`'s table**

Dated `2026-09-09, #15`. The model list and the download source are **not** rows:
the owner decided those, and this file's first line says it records what was
decided without him.

| Decision | Basis |
|---|---|
| `candle-core`, `candle-nn`, `candle-transformers` and `tokenizers` enter the tree, and the `metal` feature goes on all three candle crates on macOS, not only `candle-core` | The built-in engine cannot exist without a tensor library, and this is the one whose Whisper implementation already exists. The feature is on all three because with it on `candle-core` alone, inference dies at the first encoder layer: `Metal error no metal implementation for layer-norm`. Found by running it, not by reading |
| `tokenizers` is pinned to 0.22, not the current 0.23 | `candle-core` depends on 0.22 itself. Asking for 0.23 puts two copies of a large crate in the tree; matching it takes the tree from 151 crates to 149 |
| `onig_sys`, a C library, is accepted as a transitive dependency | It arrives through `candle-core`'s own `tokenizers` dependency and no feature choice here avoids it — `unstable_wasm` was tried and adds a second `fancy-regex` while still pulling `onig`. It means the five release targets in `docs/design.md` section 8 now need a C compiler, which is named here rather than discovered at tagging time |
| `hf-hub` is not taken; downloading uses the `ureq` already in the tree | The need is a `GET` of a pinned URL. `hf-hub` brings a second HTTP stack and its own on-disk cache layout, which would sit beside this plugin's `<state>/models/` rather than in it |

- [ ] **Step 5: Commit**

```sh
git add Cargo.toml Cargo.lock docs/decisions.md
git commit -m "$(cat <<'MSG'
Take the candle dependencies, with metal on every candle crate

The metal feature on candle-core alone is not enough: the fused layer-norm
kernel lives behind the feature on candle-nn, and without it inference fails
at the first encoder layer. tokenizers is pinned to 0.22 to match the copy
candle-core already depends on.

Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>
MSG
)"
```

---

### Task 2: `audio::wav::read`

**Files:**
- Modify: `src/audio/wav.rs`

**Interfaces:**
- Produces: `wav::read(&Path) -> Result<(Vec<f32>, u32), wav::ReadError>` and
  `wav::ReadError`. Task 9 calls it.

The engine is the first thing in this crate that has to decode a take rather
than hand its path to another program. It reads the container this crate itself
writes, and refuses anything else by name rather than converting it —
`audio::resample` exists for capture and the recorder already produces the one
shape this reads.

- [ ] **Step 1: Write the failing tests**

Append to the `mod tests` in `src/audio/wav.rs`:

```rust
    #[test]
    fn what_encode_writes_read_reads_back() {
        let samples: Vec<f32> = (0..8000)
            .map(|i| ((i as f32) / 40.0).sin() * 0.5)
            .collect();
        let dir = std::env::temp_dir().join(format!("wav-rt-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("take.wav");
        write(&path, &samples, 16_000).unwrap();

        let (back, rate) = read(&path).expect("must read what we wrote");
        assert_eq!(rate, 16_000);
        assert_eq!(back.len(), samples.len());
        // 16-bit quantisation is the only loss.
        for (a, b) in samples.iter().zip(&back) {
            assert!((a - b).abs() < 1.0 / 32_767.0, "{a} vs {b}");
        }
    }

    #[test]
    fn a_file_that_is_not_a_wav_is_named_as_one() {
        let dir = std::env::temp_dir().join(format!("wav-bad-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("nope.wav");
        std::fs::write(&path, b"<html>not audio at all, really not").unwrap();
        let error = read(&path).expect_err("must refuse");
        let message = error.to_string();
        assert!(message.contains("not a WAV"), "got {message}");
    }

    #[test]
    fn stereo_and_the_wrong_width_are_refused_by_name_not_converted() {
        let dir = std::env::temp_dir().join(format!("wav-shape-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();

        // Two channels, 16-bit, 16 kHz.
        let path = dir.join("stereo.wav");
        std::fs::write(&path, fake_wav(2, 16, 16_000)).unwrap();
        let message = read(&path).expect_err("stereo must be refused").to_string();
        assert!(message.contains("mono"), "got {message}");
        assert!(message.contains('2'), "it must say what it found: {message}");

        // One channel, 8-bit, 16 kHz.
        let path = dir.join("eight.wav");
        std::fs::write(&path, fake_wav(1, 8, 16_000)).unwrap();
        let message = read(&path).expect_err("8-bit must be refused").to_string();
        assert!(message.contains("16-bit"), "got {message}");
    }

    #[test]
    fn the_rate_is_reported_rather_than_resampled() {
        // read() reports the rate it found; refusing a wrong one is the caller's
        // job, because only the caller knows what it needs.
        let dir = std::env::temp_dir().join(format!("wav-rate-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("r.wav");
        write(&path, &[0.0f32; 100], 44_100).unwrap();
        let (_, rate) = read(&path).expect("a readable file at another rate");
        assert_eq!(rate, 44_100);
    }

    /// A WAV header with the given shape and an empty `data` chunk.
    fn fake_wav(channels: u16, bits: u16, rate: u32) -> Vec<u8> {
        let block_align = channels * bits / 8;
        let byte_rate = rate * block_align as u32;
        let mut v = Vec::new();
        v.extend_from_slice(b"RIFF");
        v.extend_from_slice(&36u32.to_le_bytes());
        v.extend_from_slice(b"WAVEfmt ");
        v.extend_from_slice(&16u32.to_le_bytes());
        v.extend_from_slice(&1u16.to_le_bytes());
        v.extend_from_slice(&channels.to_le_bytes());
        v.extend_from_slice(&rate.to_le_bytes());
        v.extend_from_slice(&byte_rate.to_le_bytes());
        v.extend_from_slice(&block_align.to_le_bytes());
        v.extend_from_slice(&bits.to_le_bytes());
        v.extend_from_slice(b"data");
        v.extend_from_slice(&0u32.to_le_bytes());
        v
    }
```

- [ ] **Step 2: Run to verify they fail**

```sh
cargo test --lib audio::wav 2>&1 | tail -20
```

Expected: FAIL, `cannot find function 'read' in this scope`.

- [ ] **Step 3: Implement `read` and `ReadError`**

Add to `src/audio/wav.rs`:

```rust
/// Why a take could not be read back. Each names what was found, because the
/// only useful thing to say about a file of the wrong shape is what shape it is.
#[derive(Debug)]
pub enum ReadError {
    NotAWav { path: PathBuf },
    NoChunk { path: PathBuf, chunk: &'static str },
    NotPcm { path: PathBuf, format: u16 },
    Channels { path: PathBuf, found: u16 },
    BitDepth { path: PathBuf, found: u16 },
    Io { path: PathBuf, why: String },
}

impl fmt::Display for ReadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ReadError::NotAWav { path } => write!(
                f,
                "{} is not a WAV file; the take is not readable and cannot be \
                 transcribed. Record it again",
                path.display()
            ),
            ReadError::NoChunk { path, chunk } => write!(
                f,
                "{} is a WAV file with no {chunk} chunk, so there is nothing to \
                 transcribe. Record it again",
                path.display()
            ),
            ReadError::NotPcm { path, format } => write!(
                f,
                "{} is WAV format {format}, not uncompressed PCM (1); this engine \
                 reads what this plugin records and converts nothing",
                path.display()
            ),
            ReadError::Channels { path, found } => write!(
                f,
                "{} has {found} channels; the engine reads mono, which is what \
                 this plugin records",
                path.display()
            ),
            ReadError::BitDepth { path, found } => write!(
                f,
                "{} is {found}-bit; the engine reads 16-bit, which is what this \
                 plugin records",
                path.display()
            ),
            ReadError::Io { path, why } => write!(f, "cannot read {}: {why}", path.display()),
        }
    }
}

impl std::error::Error for ReadError {}

/// The samples and the rate a take was written at. The inverse of `encode`,
/// and deliberately no more general than that: `audio::resample` serves capture,
/// and a take that is not the shape this crate writes is a fault to name rather
/// than a conversion to perform.
pub fn read(path: &Path) -> Result<(Vec<f32>, u32), ReadError> {
    let bytes = std::fs::read(path).map_err(|e| ReadError::Io {
        path: path.to_path_buf(),
        why: e.to_string(),
    })?;
    let bad = || ReadError::NotAWav { path: path.to_path_buf() };
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err(bad());
    }

    let u16_at = |at: usize| u16::from_le_bytes([bytes[at], bytes[at + 1]]);
    let u32_at = |at: usize| {
        u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
    };

    let (mut fmt_at, mut data) = (None, None);
    let mut at = 12usize;
    while at + 8 <= bytes.len() {
        let id = &bytes[at..at + 4];
        let size = u32_at(at + 4) as usize;
        let body = at + 8;
        if body + size > bytes.len() {
            // A chunk that claims more than the file holds: truncated, not ours.
            if id == b"data" && body <= bytes.len() {
                data = Some((body, bytes.len() - body));
            }
            break;
        }
        if id == b"fmt " && size >= 16 {
            fmt_at = Some(body);
        } else if id == b"data" {
            data = Some((body, size));
        }
        at = body + size + (size & 1);
    }

    let fmt_at = fmt_at.ok_or(ReadError::NoChunk { path: path.to_path_buf(), chunk: "fmt " })?;
    let format = u16_at(fmt_at);
    if format != 1 {
        return Err(ReadError::NotPcm { path: path.to_path_buf(), format });
    }
    let channels = u16_at(fmt_at + 2);
    if channels != 1 {
        return Err(ReadError::Channels { path: path.to_path_buf(), found: channels });
    }
    let rate = u32_at(fmt_at + 4);
    let bits = u16_at(fmt_at + 14);
    if bits != 16 {
        return Err(ReadError::BitDepth { path: path.to_path_buf(), found: bits });
    }

    let (start, size) = data.ok_or(ReadError::NoChunk { path: path.to_path_buf(), chunk: "data" })?;
    let samples = bytes[start..start + size]
        .chunks_exact(2)
        .map(|c| i16::from_le_bytes([c[0], c[1]]) as f32 / 32_768.0)
        .collect();
    Ok((samples, rate))
}
```

Add `use std::fmt;` and `use std::path::PathBuf;` to the file's imports if they
are not already there — it imports `std::path::Path` today.

- [ ] **Step 4: Run to verify they pass**

```sh
cargo test --lib audio::wav
cargo clippy --all-targets -- -D warnings && cargo fmt --check
```

- [ ] **Step 5: Commit**

```sh
git add src/audio/wav.rs
git commit -m "$(cat <<'MSG'
Read a take back: wav::read, the inverse of the encode already here

The built-in engine is the first thing in this crate that decodes a take
rather than handing its path to another program. It reads exactly what
wav::encode writes and names any other shape rather than converting it.

Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>
MSG
)"
```

---

### Task 3: `stt::candle::mel` — the filterbank from the specification

**Files:**
- Create: `src/stt/candle.rs` (module root; `mel` declared, the rest added later)
- Create: `src/stt/candle/mel.rs`
- Create: `src/stt/candle/testdata/melfilters80.bytes` (64 320 bytes)
- Create: `src/stt/candle/testdata/melfilters128.bytes` (102 912 bytes)
- Modify: `src/stt.rs` (declare `pub mod candle;`)

**Interfaces:**
- Consumes: nothing.
- Produces: `mel::filters(n_mels: usize) -> Vec<f32>` and
  `mel::spectrogram(cfg: &whisper::Config, pcm: &[f32], filters: &[f32]) -> Vec<f32>`.
  Task 9 calls both.

Whisper's encoder takes mel frames, not samples, and
`candle_transformers::models::whisper::audio::pcm_to_mel` takes the filterbank as
a parameter — it does not compute one. Candle's own example ships two opaque
binary blobs instead. This computes them, and keeps the blobs only as the
fixture that proves the computation right. Measured while designing: the computed
80-bin bank differs from candle's reference by at most 1.86 × 10⁻⁹ and the
128-bin bank by at most 3.73 × 10⁻⁹, so a 10⁻⁶ tolerance has four orders of
magnitude of room.

- [ ] **Step 1: Put the reference blobs in place**

They are the fixture, not a shipped asset, and they are `include_bytes!`-ed only
under `#[cfg(test)]`, so the release binary carries none of it.

```sh
mkdir -p src/stt/candle/testdata
curl -sL -o src/stt/candle/testdata/melfilters80.bytes \
  https://raw.githubusercontent.com/huggingface/candle/0.11.0/candle-examples/examples/whisper/melfilters.bytes
curl -sL -o src/stt/candle/testdata/melfilters128.bytes \
  https://raw.githubusercontent.com/huggingface/candle/0.11.0/candle-examples/examples/whisper/melfilters128.bytes
test "$(wc -c < src/stt/candle/testdata/melfilters80.bytes)"  = "64320"
test "$(wc -c < src/stt/candle/testdata/melfilters128.bytes)" = "102912"
```

Create `src/stt/candle/testdata/README.md`:

```markdown
# Reference mel filterbanks

`melfilters80.bytes` and `melfilters128.bytes` are the precomputed Whisper mel
filterbanks from huggingface/candle 0.11.0, `candle-examples/examples/whisper/`
(MIT/Apache-2.0). Each is a little-endian `f32` array of `n_mels x 201` values.

They are test fixtures only. `src/stt/candle/mel.rs` computes the same matrices
from the specification, and the test compares against these; nothing includes
them outside `#[cfg(test)]`, so the shipped binary contains neither.
```

- [ ] **Step 2: Write the failing tests**

Create `src/stt/candle/mel.rs` containing only this test module for now:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn reference(bytes: &[u8]) -> Vec<f32> {
        bytes
            .chunks_exact(4)
            .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect()
    }

    #[test]
    fn the_computed_filterbank_matches_the_reference_one() {
        // Without this, a hand-written filterbank is an assumption rather than a
        // fact, and a wrong one degrades every transcript quietly. Same standard
        // as the SHA-256 in src/stt/model.rs, which is checked against the
        // published vectors.
        for (n_mels, blob) in [
            (80usize, include_bytes!("testdata/melfilters80.bytes").as_slice()),
            (128usize, include_bytes!("testdata/melfilters128.bytes").as_slice()),
        ] {
            let theirs = reference(blob);
            let mine = filters(n_mels);
            assert_eq!(mine.len(), theirs.len(), "{n_mels} bins: wrong length");
            assert_eq!(mine.len(), n_mels * (N_FFT / 2 + 1));
            let worst = mine
                .iter()
                .zip(&theirs)
                .map(|(a, b)| (a - b).abs())
                .fold(0f32, f32::max);
            assert!(worst < 1e-6, "{n_mels} bins: worst difference {worst:e}");
        }
    }

    #[test]
    fn every_filter_has_weight_somewhere() {
        // A filterbank of zeros would pass a tolerance test against nothing and
        // silently produce a blank spectrogram.
        for n_mels in [80usize, 128] {
            let f = filters(n_mels);
            let bins = N_FFT / 2 + 1;
            for i in 0..n_mels {
                let row: f32 = f[i * bins..(i + 1) * bins].iter().sum();
                assert!(row > 0.0, "{n_mels} bins: filter {i} is empty");
            }
        }
    }

    #[test]
    fn the_scale_is_not_linear() {
        // Guards against hz_to_mel/mel_to_hz being replaced by an identity pair,
        // which would still produce a plausible-looking triangular bank.
        assert!((hz_to_mel(1000.0) - 15.0).abs() < 1e-9, "the break point is 1000 Hz");
        assert!(hz_to_mel(8000.0) < 4.0 * hz_to_mel(2000.0));
        assert!((mel_to_hz(hz_to_mel(3000.0)) - 3000.0).abs() < 1e-6, "round trip");
    }
}
```

- [ ] **Step 3: Run to verify they fail**

```sh
cargo test --lib candle::mel 2>&1 | tail -20
```

Expected: FAIL — the module is not declared and `filters` does not exist.

- [ ] **Step 4: Implement**

Create `src/stt.rs`'s declaration, next to the existing `pub mod command;` and
`pub mod model;`:

```rust
pub mod candle;
```

Create `src/stt/candle.rs`:

```rust
//! The built-in engine: a Whisper model running in this process.
//!
//! `candle-transformers` supplies the network. Everything around it lives here,
//! split so that the tensor half is thin and every decision that can make a
//! transcript wrong lives in `plan`, which no test needs weights to reach.
//! See `tasks/15/DESIGN_15.md`, section 1.

pub mod mel;
```

Write `src/stt/candle/mel.rs` above its test module:

```rust
//! The log-mel spectrogram Whisper's encoder takes, and the filterbank it needs.
//!
//! `candle_transformers`'s `pcm_to_mel` takes the filterbank as a parameter and
//! does not compute one; candle's own example ships it as an opaque binary blob.
//! This computes it from the specification — the Slaney mel scale, triangular
//! filters, area-normalised — and the test compares against that blob, kept as a
//! fixture. See `tasks/15/DESIGN_15.md`, section 6.

use candle_transformers::models::whisper::{self as whisper, N_FFT, SAMPLE_RATE};

/// The Slaney mel scale: linear below 1 kHz, logarithmic above it. Not the HTK
/// formula, which is the other convention and produces a different bank.
fn hz_to_mel(hz: f64) -> f64 {
    let f_sp = 200.0 / 3.0;
    let min_log_hz = 1000.0;
    let min_log_mel = min_log_hz / f_sp;
    let logstep = (6.4f64).ln() / 27.0;
    if hz >= min_log_hz {
        min_log_mel + (hz / min_log_hz).ln() / logstep
    } else {
        hz / f_sp
    }
}

fn mel_to_hz(mel: f64) -> f64 {
    let f_sp = 200.0 / 3.0;
    let min_log_hz = 1000.0;
    let min_log_mel = min_log_hz / f_sp;
    let logstep = (6.4f64).ln() / 27.0;
    if mel >= min_log_mel {
        min_log_hz * (logstep * (mel - min_log_mel)).exp()
    } else {
        f_sp * mel
    }
}

/// `n_mels` triangular filters over the `N_FFT / 2 + 1` rFFT bins, row-major,
/// each normalised by the width of the band it covers.
pub fn filters(n_mels: usize) -> Vec<f32> {
    let rate = SAMPLE_RATE as f64;
    let bins = N_FFT / 2 + 1;
    let bin_hz: Vec<f64> = (0..bins).map(|i| i as f64 * rate / N_FFT as f64).collect();

    let low = hz_to_mel(0.0);
    let high = hz_to_mel(rate / 2.0);
    let edges: Vec<f64> = (0..n_mels + 2)
        .map(|i| mel_to_hz(low + (high - low) * i as f64 / (n_mels + 1) as f64))
        .collect();

    let mut bank = vec![0f32; n_mels * bins];
    for i in 0..n_mels {
        let (left, centre, right) = (edges[i], edges[i + 1], edges[i + 2]);
        let area = 2.0 / (right - left);
        for (j, &hz) in bin_hz.iter().enumerate() {
            let rising = (hz - left) / (centre - left);
            let falling = (right - hz) / (right - centre);
            bank[i * bins + j] = (rising.min(falling).max(0.0) * area) as f32;
        }
    }
    bank
}

/// The log-mel spectrogram of a take, row-major, `n_mels` rows.
pub fn spectrogram(config: &whisper::Config, pcm: &[f32], filters: &[f32]) -> Vec<f32> {
    whisper::audio::pcm_to_mel(config, pcm, filters)
}
```

- [ ] **Step 5: Run to verify they pass**

```sh
cargo test --lib candle::mel
cargo clippy --all-targets -- -D warnings && cargo fmt --check
```

- [ ] **Step 6: Commit**

```sh
git add src/stt.rs src/stt/candle.rs src/stt/candle/mel.rs src/stt/candle/testdata
git commit -m "$(cat <<'MSG'
Compute the mel filterbank from the specification

candle's Whisper takes the filterbank as a parameter and its example ships
one as an opaque blob. Computing it keeps 164 KB of unexplainable floats out
of the binary; keeping the blob as a fixture keeps the proof that the
computation is right. Worst difference against the reference is 1.9e-9.

Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>
MSG
)"
```

---

### Task 4: `stt::catalogue` — the six models, pinned

**Files:**
- Create: `src/stt/catalogue.rs`
- Create: `scripts/model_catalogue.py`
- Modify: `src/stt.rs` (declare `pub mod catalogue;`)

**Interfaces:**
- Produces: `catalogue::{File, Entry, BASE, MODELS, get, weights}` exactly as the
  "Naming used across tasks" block states. Tasks 5, 6, 10 and 12 all use them.

Values read from the Hugging Face API on 2026-09-09. The two non-LFS files were
downloaded and hashed, because the API reports a git blob SHA-1 for those and not
a SHA-256.

- [ ] **Step 1: Write the failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_entry_is_completely_pinned() {
        assert_eq!(MODELS.len(), 6);
        for entry in MODELS.iter() {
            assert!(!entry.identifier.is_empty());
            assert!(entry.repo.starts_with("openai/whisper-"), "{}", entry.repo);
            assert_eq!(entry.revision.len(), 40, "{} revision", entry.identifier);
            assert!(
                entry.revision.chars().all(|c| c.is_ascii_hexdigit()),
                "{} revision is not a commit",
                entry.identifier
            );
            assert!(
                entry.mel_bins == 80 || entry.mel_bins == 128,
                "{} mel bins {}",
                entry.identifier,
                entry.mel_bins
            );
            let names: Vec<&str> = entry.files.iter().map(|f| f.name).collect();
            assert!(names.contains(&"model.safetensors"), "{names:?}");
            assert!(names.contains(&"config.json"), "{names:?}");
            assert!(names.contains(&"tokenizer.json"), "{names:?}");
            for file in entry.files.iter() {
                assert!(file.bytes > 0, "{} {}", entry.identifier, file.name);
                assert_eq!(file.sha256.len(), 64, "{} {}", entry.identifier, file.name);
                assert!(
                    file.sha256.chars().all(|c| c.is_ascii_hexdigit() && !c.is_uppercase()),
                    "{} {} digest must be lowercase hex",
                    entry.identifier,
                    file.name
                );
            }
        }
    }

    #[test]
    fn the_identifiers_are_unique() {
        let mut seen: Vec<&str> = MODELS.iter().map(|e| e.identifier).collect();
        let before = seen.len();
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(seen.len(), before, "a duplicate identifier: {seen:?}");
    }

    #[test]
    fn the_shipped_default_model_is_one_of_them() {
        // A default nothing can download is a fresh install that cannot be fixed
        // from inside the plugin.
        let default = crate::config::Stt::default().model;
        assert!(
            get(&default).is_some(),
            "[stt] model defaults to {default:?}, which the catalogue does not offer"
        );
    }

    #[test]
    fn an_identifier_nobody_pinned_is_simply_unknown() {
        assert!(get("whisper-of-my-own").is_none());
    }

    #[test]
    fn the_weights_are_the_biggest_file_and_are_found_by_name() {
        for entry in MODELS.iter() {
            let w = weights(entry);
            assert_eq!(w.name, "model.safetensors");
            assert!(
                entry.files.iter().all(|f| f.bytes <= w.bytes),
                "{}: the weights are not the largest file",
                entry.identifier
            );
        }
    }

    #[test]
    fn the_base_address_has_no_trailing_slash() {
        // fetch joins with a leading slash; two would make a URL nothing serves.
        assert!(!BASE.ends_with('/'), "{BASE}");
        assert!(BASE.starts_with("https://"), "{BASE}");
    }
}
```

- [ ] **Step 2: Run to verify they fail**

```sh
cargo test --lib stt::catalogue 2>&1 | tail -20
```

- [ ] **Step 3: Implement `src/stt/catalogue.rs`**

```rust
//! The models offered on first run, pinned so a download can be verified.
//!
//! Each entry names a commit, not a branch: a digest pinned against a moving
//! branch turns a legitimate upstream update into a corruption report the person
//! cannot act on. Read from the Hugging Face API on 2026-09-09 by
//! `scripts/model_catalogue.py`, which regenerates this table; the two non-LFS
//! files were downloaded and hashed, because the API reports a git blob SHA-1 for
//! those. See `tasks/15/DESIGN_15.md`, section 3.

/// Where the models are fetched from. No trailing slash: `fetch` joins paths
/// with a leading one.
pub const BASE: &str = "https://huggingface.co";

pub struct File {
    pub name: &'static str,
    pub bytes: u64,
    pub sha256: &'static str,
}

pub struct Entry {
    pub identifier: &'static str,
    pub repo: &'static str,
    pub revision: &'static str,
    /// 80 for every model but the two large-v3 ones, which use 128. It decides
    /// which filterbank is computed, and `config.json` states it too — this copy
    /// exists so the chooser can show it before anything is downloaded.
    pub mel_bins: usize,
    pub files: [File; 3],
}

pub const MODELS: [Entry; 6] = [
    Entry {
        identifier: "tiny",
        repo: "openai/whisper-tiny",
        revision: "169d4a4341b33bc18d8881c4b69c2e104e1cc0af",
        mel_bins: 80,
        files: [
            File { name: "model.safetensors", bytes: 151_061_672,
                   sha256: "7ebd0e69e78190ffe1438491fa05cc1f5c1aa3a4c4db3bc1723adbb551ea2395" },
            File { name: "config.json", bytes: 1_983,
                   sha256: "ffdccec4f3211f4c63310f2b7098f309fe70f3952cedc5e4d11e43f5b2379b98" },
            File { name: "tokenizer.json", bytes: 2_480_466,
                   sha256: "27fc476bfe7f17299480be2273fc0608e4d5a99aba2ab5dec5374b4482d1a566" },
        ],
    },
    Entry {
        identifier: "base",
        repo: "openai/whisper-base",
        revision: "e37978b90ca9030d5170a5c07aadb050351a65bb",
        mel_bins: 80,
        files: [
            File { name: "model.safetensors", bytes: 290_403_936,
                   sha256: "07cadb9f25677c8d50df603e66a98fbd842cce45047139baeb16e6219a1e807b" },
            File { name: "config.json", bytes: 1_983,
                   sha256: "a153c53883a6799b6f056b4a8d1a515c9926d03994682ba88a7616618d7da0c1" },
            File { name: "tokenizer.json", bytes: 2_480_466,
                   sha256: "27fc476bfe7f17299480be2273fc0608e4d5a99aba2ab5dec5374b4482d1a566" },
        ],
    },
    Entry {
        identifier: "small",
        repo: "openai/whisper-small",
        revision: "973afd24965f72e36ca33b3055d56a652f456b4d",
        mel_bins: 80,
        files: [
            File { name: "model.safetensors", bytes: 966_995_080,
                   sha256: "1d7734884874f1a1513ed9aa760a4f8e97aaa02fd6d93a3a85d27b2ae9ca596b" },
            File { name: "config.json", bytes: 1_967,
                   sha256: "e6a2b489da1b5aed65a8eb8d1e7466fa867ad5643a8bc138ba708bd56b2875c4" },
            File { name: "tokenizer.json", bytes: 2_480_466,
                   sha256: "27fc476bfe7f17299480be2273fc0608e4d5a99aba2ab5dec5374b4482d1a566" },
        ],
    },
    Entry {
        identifier: "large-v3-turbo",
        repo: "openai/whisper-large-v3-turbo",
        revision: "41f01f3fe87f28c78e2fbf8b568835947dd65ed9",
        mel_bins: 128,
        files: [
            File { name: "model.safetensors", bytes: 1_617_824_864,
                   sha256: "542566a422ae4f3fd23f1ba11add198fca01bbf82e66e6a2857b3f608b1eb9d1" },
            File { name: "config.json", bytes: 1_256,
                   sha256: "c5b526b3e3cd64cd8940dabb45e8ba726629e22d8ed389c29b552f9140daf04a" },
            File { name: "tokenizer.json", bytes: 2_710_337,
                   sha256: "297b13372ac43916285644fb9687add3cc62ee2a1adb60da3dc25cc94c1871fd" },
        ],
    },
    Entry {
        identifier: "medium",
        repo: "openai/whisper-medium",
        revision: "abdf7c39ab9d0397620ccaea8974cc764cd0953e",
        mel_bins: 80,
        files: [
            File { name: "model.safetensors", bytes: 3_055_544_304,
                   sha256: "62f73550fa6db24b0c6f6c5962bd0dae80fa644e93cde9cd9c3792971b47fd28" },
            File { name: "config.json", bytes: 1_991,
                   sha256: "18706810eb740d1dc54d1db181358d5f8578600d0f449e51dfd4798c0223a1f5" },
            File { name: "tokenizer.json", bytes: 2_480_466,
                   sha256: "27fc476bfe7f17299480be2273fc0608e4d5a99aba2ab5dec5374b4482d1a566" },
        ],
    },
    Entry {
        identifier: "large-v3",
        repo: "openai/whisper-large-v3",
        revision: "06f233fe06e710322aca913c1bc4249a0d71fce1",
        mel_bins: 128,
        files: [
            File { name: "model.safetensors", bytes: 3_087_130_976,
                   sha256: "a8e94b85976e5864ba3e9525c7e6c83b2a1eca42d4b797a0c7c24d778e40fd95" },
            File { name: "config.json", bytes: 1_272,
                   sha256: "ad0e8d1e46f4d01f7861a21509e5d0f977d6cc1f367a370603c92541d819807b" },
            File { name: "tokenizer.json", bytes: 2_480_617,
                   sha256: "6d8cbd7cd0d8d5815e478dac67b85a26bbe77c1f5e0c6d76d1ce2abc0e5f21ca" },
        ],
    },
];

/// The entry for an identifier, or `None` when nobody pinned it. `None` is not a
/// failure: a model placed by hand is still usable, with the checks that need a
/// pinned value skipped and named (`store::Found::Unpinned`).
pub fn get(identifier: &str) -> Option<&'static Entry> {
    MODELS.iter().find(|e| e.identifier == identifier)
}

/// The weights file. Every entry has one; the constant table is what guarantees
/// it, and `the_weights_are_the_biggest_file_and_are_found_by_name` checks it.
pub fn weights(entry: &'static Entry) -> &'static File {
    entry
        .files
        .iter()
        .find(|f| f.name == "model.safetensors")
        .expect("every catalogue entry has model.safetensors; the test pins this")
}
```

`weights` is the one `expect` in this task, and it is on a compile-time constant
that a test checks — not on anything a running daemon can influence. It is not a
panic path in the sense `CLAUDE.md` forbids.

Declare the module in `src/stt.rs`, beside the others:

```rust
pub mod catalogue;
```

- [ ] **Step 4: Write the regeneration script**

Create `scripts/model_catalogue.py`, executable. It is what makes the table
auditable; the table is what makes a download verifiable offline.

```python
#!/usr/bin/env python3
"""Regenerate the pinned model catalogue in src/stt/catalogue.rs.

Reads the Hugging Face API for each model, takes the current commit, and for
each of the three files the exact byte count and SHA-256. The API reports a
SHA-256 for LFS files; the other two are downloaded and hashed, because for
those it reports a git blob SHA-1 instead.

Prints the Rust table on standard output. It does not edit the file: a change
to a pinned model is deliberate, and a person should read the diff.
"""
import hashlib
import json
import sys
import urllib.request

REPOS = [
    ("tiny", "openai/whisper-tiny"),
    ("base", "openai/whisper-base"),
    ("small", "openai/whisper-small"),
    ("large-v3-turbo", "openai/whisper-large-v3-turbo"),
    ("medium", "openai/whisper-medium"),
    ("large-v3", "openai/whisper-large-v3"),
]
WANTED = ("model.safetensors", "config.json", "tokenizer.json")


def fetch(url):
    request = urllib.request.Request(url, headers={"User-Agent": "herdr-voice"})
    return urllib.request.urlopen(request, timeout=600)


def entry(identifier, repo):
    info = json.load(fetch(f"https://huggingface.co/api/models/{repo}"))
    revision = info["sha"]
    tree = json.load(fetch(f"https://huggingface.co/api/models/{repo}/tree/main"))
    files = {}
    for item in tree:
        if item["path"] in WANTED:
            files[item["path"]] = {
                "bytes": item["size"],
                "sha256": (item.get("lfs") or {}).get("oid"),
            }
    missing = set(WANTED) - set(files)
    if missing:
        sys.exit(f"{repo}: {sorted(missing)} not in the repository")
    for name, meta in files.items():
        if not meta["sha256"]:
            body = fetch(f"https://huggingface.co/{repo}/resolve/{revision}/{name}").read()
            if len(body) != meta["bytes"]:
                sys.exit(f"{repo}/{name}: {len(body)} bytes, the API said {meta['bytes']}")
            meta["sha256"] = hashlib.sha256(body).hexdigest()
    config = json.load(fetch(f"https://huggingface.co/{repo}/resolve/{revision}/config.json"))
    return revision, config["num_mel_bins"], files


def main():
    print(f"pub const MODELS: [Entry; {len(REPOS)}] = [")
    for identifier, repo in REPOS:
        revision, mel_bins, files = entry(identifier, repo)
        print("    Entry {")
        print(f'        identifier: "{identifier}",')
        print(f'        repo: "{repo}",')
        print(f'        revision: "{revision}",')
        print(f"        mel_bins: {mel_bins},")
        print("        files: [")
        for name in WANTED:
            meta = files[name]
            print(f'            File {{ name: "{name}", bytes: {meta["bytes"]:_},')
            print(f'                   sha256: "{meta["sha256"]}" }},')
        print("        ],")
        print("    },")
    print("];")


if __name__ == "__main__":
    main()
```

- [ ] **Step 5: Run to verify the tests pass, and that the script agrees**

```sh
cargo test --lib stt::catalogue
python3 scripts/model_catalogue.py > /tmp/catalogue.rs && head -20 /tmp/catalogue.rs
```

The script's output must match the table committed in Step 3, entry for entry.
If it does not, upstream moved between the design and now: take the script's
answer, say so in the commit body, and do not keep both.

- [ ] **Step 6: Commit**

```sh
git add src/stt.rs src/stt/catalogue.rs scripts/model_catalogue.py
git commit -m "$(cat <<'MSG'
Pin the six offered models by commit, byte count and SHA-256

A digest pinned against a moving branch turns an upstream update into a
corruption report nobody can act on, so the commit is pinned with it.
scripts/model_catalogue.py regenerates the table and is what makes it
auditable.

Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>
MSG
)"
```

---

### Task 5: `stt::candle::store` — what a candle model on disk is

**Files:**
- Create: `src/stt/candle/store.rs`
- Modify: `src/stt/candle.rs` (declare `pub mod store;`)
- Modify: `src/stt/model.rs` (make `sha256_of` reachable: `pub(crate) fn sha256_of`)

**Interfaces:**
- Consumes: `catalogue::{Entry, File}` (Task 4);
  `crate::stt::model::sha256_of` (existing, made `pub(crate)`).
- Produces: `store::{ModelDir, Found, StoreError, Glance, locate, glance,
  directory}`. Tasks 6, 9, 10, 11 and 12 use them.

The `ggml` contract in `src/stt/model.rs` is not touched: it still serves
`[stt] command`, and `StoreError` is a new type beside `ModelError` rather than
variants added to it. This store makes the same four checks, two of them exact
rather than heuristic, because a pinned size and digest exist here and did not
there.

- [ ] **Step 1: Write the failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::stt::catalogue::{Entry, File as CatFile};

    fn scratch(tag: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("candle-store-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("scratch");
        path
    }

    /// A safetensors file: an 8-byte little-endian header length, then that many
    /// bytes of JSON. Small on purpose — the entry is a parameter, so a test
    /// never needs a real 151 MB file to exercise the verified path.
    fn safetensors(body: &str) -> Vec<u8> {
        let mut v = (body.len() as u64).to_le_bytes().to_vec();
        v.extend_from_slice(body.as_bytes());
        v
    }

    struct Written {
        models: PathBuf,
        weights: Vec<u8>,
        config: Vec<u8>,
        tokenizer: Vec<u8>,
    }

    /// Writes a complete, well-formed model and returns what was written, so a
    /// test can build the matching catalogue entry from the real bytes.
    fn write_model(tag: &str, identifier: &str) -> Written {
        let models = scratch(tag);
        let dir = directory(&models, identifier);
        std::fs::create_dir_all(&dir).unwrap();
        let weights = safetensors(r#"{"encoder.weight":{"dtype":"F32","shape":[2],"data_offsets":[0,8]}}"#);
        let config = br#"{"num_mel_bins":80}"#.to_vec();
        let tokenizer = br#"{"version":"1.0"}"#.to_vec();
        std::fs::write(dir.join("model.safetensors"), &weights).unwrap();
        std::fs::write(dir.join("config.json"), &config).unwrap();
        std::fs::write(dir.join("tokenizer.json"), &tokenizer).unwrap();
        Written { models, weights, config, tokenizer }
    }

    fn sha(bytes: &[u8]) -> String {
        let tmp = std::env::temp_dir().join(format!("sha-{}-{}", std::process::id(), bytes.len()));
        std::fs::write(&tmp, bytes).unwrap();
        let digest = crate::stt::model::sha256_of(&tmp).unwrap();
        let _ = std::fs::remove_file(&tmp);
        digest
    }

    /// A catalogue entry that describes exactly what `write_model` wrote.
    fn entry_for(w: &Written) -> Entry {
        Entry {
            identifier: "fixture",
            repo: "openai/whisper-fixture",
            revision: "0000000000000000000000000000000000000000",
            mel_bins: 80,
            files: [
                CatFile { name: "model.safetensors", bytes: w.weights.len() as u64,
                          sha256: Box::leak(sha(&w.weights).into_boxed_str()) },
                CatFile { name: "config.json", bytes: w.config.len() as u64,
                          sha256: Box::leak(sha(&w.config).into_boxed_str()) },
                CatFile { name: "tokenizer.json", bytes: w.tokenizer.len() as u64,
                          sha256: Box::leak(sha(&w.tokenizer).into_boxed_str()) },
            ],
        }
    }

    #[test]
    fn a_model_that_matches_its_entry_is_verified() {
        let w = write_model("good", "fixture");
        let entry = entry_for(&w);
        match locate(&w.models, "fixture", Some(&entry)) {
            Ok(Found::Verified(dir)) => {
                assert!(dir.weights().ends_with("model.safetensors"));
                assert!(dir.config().ends_with("config.json"));
                assert!(dir.tokenizer().ends_with("tokenizer.json"));
            }
            other => panic!("expected Verified, got {other:?}"),
        }
    }

    #[test]
    fn a_model_nobody_pinned_is_unpinned_and_says_which_checks_were_skipped() {
        let w = write_model("unpinned", "homegrown");
        match locate(&w.models, "homegrown", None) {
            Ok(Found::Unpinned { identifier, .. }) => assert_eq!(identifier, "homegrown"),
            other => panic!("expected Unpinned, got {other:?}"),
        }
    }

    #[test]
    fn a_truncated_download_is_named_by_its_size_not_by_its_digest() {
        // The size check must come first: "1024 bytes, expected 151061672" says
        // what happened; "the digest does not match" does not.
        let w = write_model("short", "fixture");
        let entry = entry_for(&w);
        let dir = directory(&w.models, "fixture");
        std::fs::write(dir.join("model.safetensors"), &w.weights[..8]).unwrap();
        let error = locate(&w.models, "fixture", Some(&entry)).expect_err("must refuse");
        match &error {
            StoreError::WrongSize { expected, found, .. } => {
                assert_eq!(*expected, w.weights.len() as u64);
                assert_eq!(*found, 8);
            }
            other => panic!("expected WrongSize, got {other:?}"),
        }
        let message = error.to_string();
        assert!(message.contains("again"), "it must say what to do: {message}");
    }

    #[test]
    fn the_right_size_and_the_wrong_bytes_are_caught_by_the_digest() {
        let w = write_model("swapped", "fixture");
        let entry = entry_for(&w);
        let dir = directory(&w.models, "fixture");
        let mut other = w.weights.clone();
        let last = other.len() - 1;
        other[last] ^= 0xff;
        std::fs::write(dir.join("model.safetensors"), &other).unwrap();
        let error = locate(&w.models, "fixture", Some(&entry)).expect_err("must refuse");
        assert!(matches!(error, StoreError::DigestMismatch { .. }), "got {error:?}");
    }

    #[test]
    fn a_file_that_is_not_safetensors_is_named_as_one() {
        let w = write_model("notst", "homegrown");
        let dir = directory(&w.models, "homegrown");
        std::fs::write(dir.join("model.safetensors"), b"<html>an error page saved by mistake").unwrap();
        // Unpinned, so no size or digest check can catch it: the format must.
        let error = locate(&w.models, "homegrown", None).expect_err("must refuse");
        assert!(matches!(error, StoreError::NotSafetensors { .. }), "got {error:?}");
    }

    #[test]
    fn config_and_tokenizer_that_do_not_parse_are_named_as_such() {
        for broken in ["config.json", "tokenizer.json"] {
            let w = write_model("parse", "homegrown");
            let dir = directory(&w.models, "homegrown");
            std::fs::write(dir.join(broken), b"{not json at all").unwrap();
            let error = locate(&w.models, "homegrown", None).expect_err("must refuse");
            match &error {
                StoreError::Unparsable { path, .. } => {
                    assert!(path.ends_with(broken), "got {path:?}")
                }
                other => panic!("expected Unparsable for {broken}, got {other:?}"),
            }
        }
    }

    #[test]
    fn a_missing_file_names_which_one_and_where_it_should_be() {
        for absent in ["model.safetensors", "config.json", "tokenizer.json"] {
            let w = write_model("missing", "homegrown");
            std::fs::remove_file(directory(&w.models, "homegrown").join(absent)).unwrap();
            let error = locate(&w.models, "homegrown", None).expect_err("must refuse");
            match &error {
                StoreError::Missing { absent: named, .. } => assert_eq!(*named, absent),
                other => panic!("expected Missing for {absent}, got {other:?}"),
            }
            assert!(
                error.to_string().contains("model --choose"),
                "it must say what to do: {error}"
            );
        }
    }

    #[test]
    fn a_leftover_part_file_is_not_a_model() {
        // The exact-name rule, which is the one #13 wrote and this must not
        // loosen: a download in progress must never be mistaken for a model.
        let models = scratch("part");
        let dir = directory(&models, "homegrown");
        std::fs::create_dir_all(&dir).unwrap();
        let good = safetensors(r#"{"a":{"dtype":"F32","shape":[1],"data_offsets":[0,4]}}"#);
        std::fs::write(dir.join("model.safetensors.part"), &good).unwrap();
        std::fs::write(dir.join("config.json"), br#"{"num_mel_bins":80}"#).unwrap();
        std::fs::write(dir.join("tokenizer.json"), br#"{"version":"1.0"}"#).unwrap();
        let error = locate(&models, "homegrown", None).expect_err("a .part is not a model");
        assert!(matches!(error, StoreError::Missing { .. }), "got {error:?}");
    }

    #[test]
    fn a_glance_answers_by_name_and_size_and_never_hashes() {
        // The chooser lists six models; hashing six multi-gigabyte files to say
        // what is installed would take a minute.
        let w = write_model("glance", "fixture");
        let entry = entry_for(&w);
        assert_eq!(glance(&w.models, "fixture", Some(&entry)), Glance::Whole);
        assert_eq!(glance(&w.models, "absent", Some(&entry)), Glance::Absent);

        let dir = directory(&w.models, "fixture");
        std::fs::write(dir.join("config.json"), b"{}").unwrap();
        assert_eq!(glance(&w.models, "fixture", Some(&entry)), Glance::WrongSize);

        // Corrupt bytes at the right length are invisible to a glance, by
        // design: locate is what catches those.
        let w = write_model("glance2", "fixture");
        let entry = entry_for(&w);
        let dir = directory(&w.models, "fixture");
        let mut bad = w.weights.clone();
        let last = bad.len() - 1;
        bad[last] ^= 0xff;
        std::fs::write(dir.join("model.safetensors"), &bad).unwrap();
        assert_eq!(glance(&w.models, "fixture", Some(&entry)), Glance::Whole);
        assert!(
            matches!(
                locate(&w.models, "fixture", Some(&entry)),
                Err(StoreError::DigestMismatch { .. })
            ),
            "locate is the check that catches it"
        );
    }

    #[test]
    fn a_directory_that_does_not_exist_names_the_directory() {
        let models = scratch("absent");
        let error = locate(&models, "large-v3-turbo", None).expect_err("must refuse");
        let message = error.to_string();
        assert!(message.contains("large-v3-turbo"), "got {message}");
        assert!(message.contains("model --choose"), "got {message}");
    }
}
```

- [ ] **Step 2: Run to verify they fail**

```sh
cargo test --lib candle::store 2>&1 | tail -20
```

- [ ] **Step 3: Make `sha256_of` reachable**

In `src/stt/model.rs`, change the one line

```rust
fn sha256_of(path: &Path) -> Result<String, String> {
```

to

```rust
pub(crate) fn sha256_of(path: &Path) -> Result<String, String> {
```

Nothing else in that file changes. This is the only edit this issue makes to the
`ggml` store, and it changes no behaviour.

- [ ] **Step 4: Implement `src/stt/candle/store.rs`**

```rust
//! What a candle model on disk is, and whether this one is.
//!
//! The `ggml` store in `src/stt/model.rs` is untouched and still serves
//! `[stt] command`. This makes the same four checks — the exact name, the size,
//! the format, the digest — with two of them exact rather than heuristic,
//! because a pinned size and digest exist here and did not there. See
//! `tasks/15/DESIGN_15.md`, section 2.

use std::fmt;
use std::path::{Path, PathBuf};

use crate::stt::catalogue;
use crate::stt::model::sha256_of;

/// The three files a model is, addressed from the directory that holds them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelDir {
    pub dir: PathBuf,
}

impl ModelDir {
    pub fn weights(&self) -> PathBuf {
        self.dir.join("model.safetensors")
    }
    pub fn config(&self) -> PathBuf {
        self.dir.join("config.json")
    }
    pub fn tokenizer(&self) -> PathBuf {
        self.dir.join("tokenizer.json")
    }
}

/// A model that is there. Two outcomes, not one, because a model nobody pinned
/// is usable and must say which checks were not made.
#[derive(Debug)]
pub enum Found {
    /// Every check made, the pinned byte count and digest among them.
    Verified(ModelDir),
    /// Structurally sound; the catalogue does not know this identifier, so the
    /// byte count and the digest were not checked. The identifier is carried so
    /// the report names the value that was not found.
    Unpinned { dir: ModelDir, identifier: String },
}

impl Found {
    pub fn dir(&self) -> &ModelDir {
        match self {
            Found::Verified(dir) => dir,
            Found::Unpinned { dir, .. } => dir,
        }
    }
}

#[derive(Debug)]
pub enum StoreError {
    Missing { dir: PathBuf, identifier: String, absent: &'static str },
    WrongSize { path: PathBuf, expected: u64, found: u64 },
    NotSafetensors { path: PathBuf, why: String },
    Unparsable { path: PathBuf, why: String },
    DigestMismatch { path: PathBuf },
    Unreadable { path: PathBuf, why: String },
}

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StoreError::Missing { dir, identifier, absent } => write!(
                f,
                "no {absent} for the {identifier} model in {}; run \
                 `herdr-voice model --choose` to download one, or set [stt] model \
                 to a model that is there",
                dir.display()
            ),
            StoreError::WrongSize { path, expected, found } => write!(
                f,
                "{} is {found} bytes and should be {expected} — a download that \
                 stopped early looks like this. Delete the file and run \
                 `herdr-voice model --choose` again",
                path.display()
            ),
            StoreError::NotSafetensors { path, why } => write!(
                f,
                "{} is not a safetensors file ({why}); it is something else under \
                 the right name. Delete it and run `herdr-voice model --choose` again",
                path.display()
            ),
            StoreError::Unparsable { path, why } => write!(
                f,
                "{} is not readable JSON ({why}); delete it and run \
                 `herdr-voice model --choose` again",
                path.display()
            ),
            StoreError::DigestMismatch { path } => write!(
                f,
                "{} does not match the digest this plugin pins for it; it is the \
                 wrong file or a damaged one. Delete it and run \
                 `herdr-voice model --choose` again",
                path.display()
            ),
            StoreError::Unreadable { path, why } => {
                write!(f, "cannot read {}: {why}", path.display())
            }
        }
    }
}

impl std::error::Error for StoreError {}

/// Where a candle model lives. A directory per identifier, under `candle/`, so
/// the flat `ggml-<model>.bin` namespace beside it is left exactly as it is.
pub fn directory(models: &Path, identifier: &str) -> PathBuf {
    models.join("candle").join(identifier)
}

/// Whether a model looks installed, by name and size alone. Deliberately does
/// not hash: the chooser lists six models, and hashing six multi-gigabyte files
/// to answer "what is installed" would take a minute. `locate` is the real check
/// and runs before anything is loaded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Glance {
    Whole,
    Absent,
    WrongSize,
}

pub fn glance(models: &Path, identifier: &str, expected: Option<&catalogue::Entry>) -> Glance {
    let dir = ModelDir { dir: directory(models, identifier) };
    let mut any_wrong = false;
    for (name, path) in [
        ("model.safetensors", dir.weights()),
        ("config.json", dir.config()),
        ("tokenizer.json", dir.tokenizer()),
    ] {
        let metadata = match std::fs::metadata(&path) {
            Ok(m) if m.is_file() => m,
            _ => return Glance::Absent,
        };
        if let Some(entry) = expected {
            if let Some(file) = entry.files.iter().find(|f| f.name == name) {
                if metadata.len() != file.bytes {
                    any_wrong = true;
                }
            }
        }
    }
    if any_wrong {
        Glance::WrongSize
    } else {
        Glance::Whole
    }
}

/// The model, if it is one. `expected` is a parameter rather than something this
/// looks up, so a test can describe a small fabricated file; production passes
/// `catalogue::get(identifier)`.
pub fn locate(
    models: &Path,
    identifier: &str,
    expected: Option<&catalogue::Entry>,
) -> Result<Found, StoreError> {
    let dir = directory(models, identifier);
    let model_dir = ModelDir { dir: dir.clone() };

    for (name, path) in [
        ("model.safetensors", model_dir.weights()),
        ("config.json", model_dir.config()),
        ("tokenizer.json", model_dir.tokenizer()),
    ] {
        // 1. The exact name in the exact directory. A leftover `.part` is not a
        //    file with this name, so it matches nothing.
        let metadata = match std::fs::metadata(&path) {
            Ok(metadata) if metadata.is_file() => metadata,
            _ => {
                return Err(StoreError::Missing {
                    dir,
                    identifier: identifier.to_string(),
                    absent: name,
                })
            }
        };

        // 2. The exact byte count, when one is pinned. Before the digest, because
        //    a size says what happened and a digest only says something did.
        if let Some(entry) = expected {
            if let Some(file) = entry.files.iter().find(|f| f.name == name) {
                if metadata.len() != file.bytes {
                    return Err(StoreError::WrongSize {
                        path,
                        expected: file.bytes,
                        found: metadata.len(),
                    });
                }
            }
        }

        // 3. The format. This is the only check an unpinned model gets beyond
        //    the name, so it has to be a real one.
        if name == "model.safetensors" {
            check_safetensors(&path, metadata.len())?;
        } else {
            let text = std::fs::read_to_string(&path).map_err(|e| StoreError::Unreadable {
                path: path.clone(),
                why: e.to_string(),
            })?;
            serde_json::from_str::<serde_json::Value>(&text).map_err(|e| {
                StoreError::Unparsable { path: path.clone(), why: e.to_string() }
            })?;
        }

        // 4. The digest, when one is pinned.
        if let Some(entry) = expected {
            if let Some(file) = entry.files.iter().find(|f| f.name == name) {
                let actual = sha256_of(&path).map_err(|why| StoreError::Unreadable {
                    path: path.clone(),
                    why,
                })?;
                if actual != file.sha256 {
                    return Err(StoreError::DigestMismatch { path });
                }
            }
        }
    }

    Ok(match expected {
        Some(_) => Found::Verified(model_dir),
        None => Found::Unpinned {
            dir: model_dir,
            identifier: identifier.to_string(),
        },
    })
}

/// A safetensors file begins with an 8-byte little-endian header length followed
/// by that many bytes of JSON naming the tensors. Both are read, because the
/// length alone is satisfied by any file whose first eight bytes happen to be
/// small.
fn check_safetensors(path: &Path, size: u64) -> Result<(), StoreError> {
    use std::io::Read;
    let mut file = std::fs::File::open(path).map_err(|e| StoreError::Unreadable {
        path: path.to_path_buf(),
        why: e.to_string(),
    })?;
    let mut length = [0u8; 8];
    file.read_exact(&mut length).map_err(|_| StoreError::NotSafetensors {
        path: path.to_path_buf(),
        why: "shorter than a header".to_string(),
    })?;
    let header = u64::from_le_bytes(length);
    if header == 0 || header.saturating_add(8) > size {
        return Err(StoreError::NotSafetensors {
            path: path.to_path_buf(),
            why: format!("its header claims {header} bytes and the file holds {size}"),
        });
    }
    let mut json = vec![0u8; header as usize];
    file.read_exact(&mut json).map_err(|e| StoreError::Unreadable {
        path: path.to_path_buf(),
        why: e.to_string(),
    })?;
    match serde_json::from_slice::<serde_json::Value>(&json) {
        Ok(serde_json::Value::Object(_)) => Ok(()),
        Ok(_) => Err(StoreError::NotSafetensors {
            path: path.to_path_buf(),
            why: "its header is not an object".to_string(),
        }),
        Err(e) => Err(StoreError::NotSafetensors {
            path: path.to_path_buf(),
            why: e.to_string(),
        }),
    }
}
```

Declare it in `src/stt/candle.rs`:

```rust
pub mod store;
```

- [ ] **Step 5: Run to verify they pass**

```sh
cargo test --lib candle::store
cargo test --lib stt::model   # the ggml store must be entirely unaffected
cargo clippy --all-targets -- -D warnings && cargo fmt --check
```

- [ ] **Step 6: Commit**

```sh
git add src/stt/candle.rs src/stt/candle/store.rs src/stt/model.rs
git commit -m "$(cat <<'MSG'
A candle model store beside the ggml one, with the same checks made exact

candle reads safetensors and a Whisper model is three files, so the ggml name
template cannot serve both without loosening one. The four checks carry over
and two of them stop being heuristics: a pinned byte count and a pinned digest
exist here and did not there. The ggml store changes only in that sha256_of
became pub(crate).

Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>
MSG
)"
```

---

### Task 6: `stt::fetch` — download and verify

**Files:**
- Create: `src/stt/fetch.rs`
- Modify: `src/stt.rs` (declare `pub mod fetch;`)

**Interfaces:**
- Consumes: `catalogue::{BASE, Entry, File, get}` (Task 4);
  `store::directory` (Task 5); `crate::stt::model::sha256_of`.
- Produces: `fetch::{Progress, FetchError, model, model_from}`. Task 12 calls
  `model`; nothing else does.

A file under its real name is, by construction, a file that passed every check.
That is the whole design of this module: bytes land in `<name>.part`, are hashed
as they arrive, and the rename happens only after the byte count and the digest
both match.

- [ ] **Step 1: Write the failing tests**

The double is the one `src/rewrite/http.rs:131-200` already uses, narrowed —
this client sends no body, so the request needs draining only to its headers.

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::stt::catalogue::{Entry, File as CatFile};
    use std::io::{Read, Write};
    use std::net::TcpListener;

    /// Serves each requested path from a fixed table, then returns. One thread,
    /// as many connections as the caller makes: `model_from` fetches three
    /// files, so a one-shot double would hang on the second.
    ///
    /// The request is read to the end of its headers before the response is
    /// written, for the reason `src/rewrite/http.rs:138-142` records: a stream
    /// dropped while the kernel still holds unread bytes can turn the close into
    /// a reset, which surfaces as an intermittent, unrelated-looking failure.
    fn serve(
        files: Vec<(&'static str, &'static str, Vec<u8>)>, // (path suffix, status, body)
        connections: usize,
    ) -> (String, std::thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let addr = listener.local_addr().expect("addr");
        let base = format!("http://{addr}");
        let handle = std::thread::spawn(move || {
            for _ in 0..connections {
                let (mut stream, _) = match listener.accept() {
                    Ok(pair) => pair,
                    Err(_) => return,
                };
                let mut buf = Vec::new();
                let mut chunk = [0u8; 4096];
                loop {
                    if buf.windows(4).any(|w| w == b"\r\n\r\n") {
                        break;
                    }
                    match stream.read(&mut chunk) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => buf.extend_from_slice(&chunk[..n]),
                    }
                }
                let request = String::from_utf8_lossy(&buf).to_string();
                let matched = files
                    .iter()
                    .find(|(suffix, _, _)| request.lines().next().is_some_and(|l| l.contains(suffix)));
                match matched {
                    Some((_, status, body)) => {
                        let head = format!(
                            "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            body.len()
                        );
                        let _ = stream.write_all(head.as_bytes());
                        let _ = stream.write_all(body);
                    }
                    None => {
                        let _ = stream.write_all(
                            b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                        );
                    }
                }
                let _ = stream.flush();
            }
        });
        (base, handle)
    }

    struct Silent;
    impl Progress for Silent {
        fn file(&mut self, _: &str, _: u64) {}
        fn bytes(&mut self, _: u64) {}
        fn done(&mut self, _: &str) {}
    }

    fn scratch(tag: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("fetch-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("scratch");
        path
    }

    fn sha(bytes: &[u8]) -> String {
        let tmp = std::env::temp_dir()
            .join(format!("fetch-sha-{}-{}", std::process::id(), bytes.len()));
        std::fs::write(&tmp, bytes).unwrap();
        let digest = crate::stt::model::sha256_of(&tmp).unwrap();
        let _ = std::fs::remove_file(&tmp);
        digest
    }

    const WEIGHTS: &[u8] = b"\x20\x00\x00\x00\x00\x00\x00\x00{\"a\":{\"dtype\":\"F32\"}}      ";
    const CONFIG: &[u8] = br#"{"num_mel_bins":80}"#;
    const TOKENIZER: &[u8] = br#"{"version":"1.0"}"#;

    fn entry() -> Entry {
        Entry {
            identifier: "fixture",
            repo: "openai/whisper-fixture",
            revision: "0000000000000000000000000000000000000000",
            mel_bins: 80,
            files: [
                CatFile { name: "model.safetensors", bytes: WEIGHTS.len() as u64,
                          sha256: Box::leak(sha(WEIGHTS).into_boxed_str()) },
                CatFile { name: "config.json", bytes: CONFIG.len() as u64,
                          sha256: Box::leak(sha(CONFIG).into_boxed_str()) },
                CatFile { name: "tokenizer.json", bytes: TOKENIZER.len() as u64,
                          sha256: Box::leak(sha(TOKENIZER).into_boxed_str()) },
            ],
        }
    }

    fn all_three(status: &'static str) -> Vec<(&'static str, &'static str, Vec<u8>)> {
        vec![
            ("model.safetensors", status, WEIGHTS.to_vec()),
            ("config.json", status, CONFIG.to_vec()),
            ("tokenizer.json", status, TOKENIZER.to_vec()),
        ]
    }

    #[test]
    fn a_good_transfer_leaves_three_files_and_no_part() {
        let dir = scratch("good");
        let (base, handle) = serve(all_three("200 OK"), 3);
        let entry = entry();
        fetch_into(&base, &entry, &dir, &mut Silent).expect("must succeed");
        handle.join().unwrap();

        for name in ["model.safetensors", "config.json", "tokenizer.json"] {
            assert!(dir.join(name).exists(), "{name} is missing");
            assert!(!dir.join(format!("{name}.part")).exists(), "{name}.part was left behind");
        }
        assert_eq!(std::fs::read(dir.join("config.json")).unwrap(), CONFIG);
    }

    #[test]
    fn a_short_transfer_names_the_size_and_leaves_nothing_behind() {
        let dir = scratch("short");
        let mut files = all_three("200 OK");
        files[0].2.truncate(8);
        let (base, handle) = serve(files, 3);
        let error = fetch_into(&base, &entry(), &dir, &mut Silent).expect_err("must refuse");
        handle.join().unwrap();

        match &error {
            FetchError::ShortRead { name, expected, found } => {
                assert_eq!(name, "model.safetensors");
                assert_eq!(*expected, WEIGHTS.len() as u64);
                assert_eq!(*found, 8);
            }
            other => panic!("expected ShortRead, got {other:?}"),
        }
        assert!(!dir.join("model.safetensors").exists(), "an unverified file under the real name");
        assert!(!dir.join("model.safetensors.part").exists(), "a .part was left behind");
    }

    #[test]
    fn the_right_length_and_the_wrong_bytes_are_caught_by_the_digest() {
        let dir = scratch("digest");
        let mut files = all_three("200 OK");
        let last = files[0].2.len() - 1;
        files[0].2[last] ^= 0xff;
        let (base, handle) = serve(files, 3);
        let error = fetch_into(&base, &entry(), &dir, &mut Silent).expect_err("must refuse");
        handle.join().unwrap();

        assert!(matches!(error, FetchError::Digest { .. }), "got {error:?}");
        assert!(!dir.join("model.safetensors").exists());
        assert!(!dir.join("model.safetensors.part").exists());
    }

    #[test]
    fn a_non_2xx_response_names_the_status_and_the_address() {
        let dir = scratch("status");
        let (base, handle) = serve(all_three("503 Service Unavailable"), 3);
        let error = fetch_into(&base, &entry(), &dir, &mut Silent).expect_err("must refuse");
        handle.join().unwrap();

        match &error {
            FetchError::Status { code, url } => {
                assert_eq!(*code, 503);
                assert!(url.contains("model.safetensors"), "got {url}");
            }
            other => panic!("expected Status, got {other:?}"),
        }
        assert!(!dir.join("model.safetensors.part").exists());
    }

    #[test]
    fn an_unreachable_address_says_so_rather_than_hanging() {
        let dir = scratch("closed");
        // Bound and immediately dropped: the port is not listening.
        let port = {
            let l = TcpListener::bind("127.0.0.1:0").unwrap();
            l.local_addr().unwrap().port()
        };
        let error = fetch_into(&format!("http://127.0.0.1:{port}"), &entry(), &dir, &mut Silent)
            .expect_err("must refuse");
        assert!(matches!(error, FetchError::Http { .. }), "got {error:?}");
        assert!(error.to_string().contains("again"), "it must say what to do: {error}");
    }

    #[test]
    fn every_failure_names_what_to_do_next() {
        let cases = [
            FetchError::Http { url: "u".into(), why: "refused".into() },
            FetchError::Status { url: "u".into(), code: 404 },
            FetchError::ShortRead { name: "n".into(), expected: 2, found: 1 },
            FetchError::Digest { name: "n".into() },
            FetchError::Io { path: PathBuf::from("/p"), why: "full".into() },
            FetchError::Unknown("x".into()),
        ];
        for case in cases {
            let message = case.to_string();
            assert!(
                message.contains("again") || message.contains("space") || message.contains("check"),
                "{message}"
            );
        }
    }

    #[test]
    fn an_unknown_identifier_is_refused_before_anything_is_fetched() {
        let dir = scratch("unknown");
        let error = model("no-such-model", &dir, &mut Silent).expect_err("must refuse");
        assert!(error.to_string().contains("no-such-model"), "got {error}");
    }
}
```

Note the test module calls `fetch_into`, the name used below for the inner
function; `model_from` in the design's prose and `fetch_into` here are the same
function, and `fetch_into` is the name that ships.

- [ ] **Step 2: Run to verify they fail**

```sh
cargo test --lib stt::fetch 2>&1 | tail -20
```

- [ ] **Step 3: Implement `src/stt/fetch.rs`**

```rust
//! Fetching a model, and refusing to keep one that did not arrive whole.
//!
//! Bytes land in `<name>.part` and are hashed as they arrive; the rename to the
//! real name happens only after the byte count and the digest both match. So a
//! file under its real name is, by construction, one that passed every check,
//! and there is no window in which a half-written model exists. See
//! `tasks/15/DESIGN_15.md`, section 4.

use std::fmt;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use crate::stt::candle::store;
use crate::stt::catalogue;

/// What a caller is told while a multi-gigabyte file arrives. A trait rather
/// than a closure so the chooser can hold a line of its own state.
pub trait Progress {
    fn file(&mut self, name: &str, total: u64);
    fn bytes(&mut self, done: u64);
    fn done(&mut self, name: &str);
}

#[derive(Debug)]
pub enum FetchError {
    Http { url: String, why: String },
    Status { url: String, code: u16 },
    ShortRead { name: String, expected: u64, found: u64 },
    Digest { name: String },
    Io { path: PathBuf, why: String },
    Unknown(String),
}

impl fmt::Display for FetchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FetchError::Http { url, why } => write!(
                f,
                "cannot reach {url}: {why}. Check the network and run \
                 `herdr-voice model --choose` again"
            ),
            FetchError::Status { url, code } => write!(
                f,
                "{url} answered {code}. If it is 404 the pinned model moved and \
                 this plugin needs updating; otherwise run \
                 `herdr-voice model --choose` again"
            ),
            FetchError::ShortRead { name, expected, found } => write!(
                f,
                "{name} arrived as {found} bytes and should be {expected} — the \
                 download stopped early. Nothing was kept; run \
                 `herdr-voice model --choose` again"
            ),
            FetchError::Digest { name } => write!(
                f,
                "{name} arrived complete but does not match the digest this plugin \
                 pins for it. Nothing was kept; run `herdr-voice model --choose` \
                 again, and if it happens twice the pinned model has changed \
                 upstream"
            ),
            FetchError::Io { path, why } => write!(
                f,
                "cannot write {}: {why}. Check the space and permissions on that \
                 directory, then run `herdr-voice model --choose` again",
                path.display()
            ),
            FetchError::Unknown(why) => write!(
                f,
                "the download failed: {why}. Run `herdr-voice model --choose` again"
            ),
        }
    }
}

impl std::error::Error for FetchError {}

/// Fetch a catalogued model into the models directory, and return the directory
/// it landed in. The one entry point production uses.
pub fn model(
    identifier: &str,
    models: &Path,
    progress: &mut dyn Progress,
) -> Result<PathBuf, FetchError> {
    let entry = catalogue::get(identifier).ok_or_else(|| {
        FetchError::Unknown(format!(
            "{identifier} is not a model this plugin offers; run `herdr-voice model` \
             for the list"
        ))
    })?;
    let dir = store::directory(models, identifier);
    fetch_into(catalogue::BASE, entry, &dir, progress)?;
    Ok(dir)
}

/// The same work with the base address as a parameter, so a test drives it
/// against a listener on loopback.
pub(crate) fn fetch_into(
    base: &str,
    entry: &catalogue::Entry,
    dir: &Path,
    progress: &mut dyn Progress,
) -> Result<(), FetchError> {
    std::fs::create_dir_all(dir).map_err(|e| FetchError::Io {
        path: dir.to_path_buf(),
        why: e.to_string(),
    })?;
    for file in entry.files.iter() {
        // Already there and whole: nothing to fetch. This is what makes a retry
        // after one failed file cheap instead of another three gigabytes.
        if let Ok(metadata) = std::fs::metadata(dir.join(file.name)) {
            if metadata.len() == file.bytes {
                progress.done(file.name);
                continue;
            }
        }
        one(base, entry, file, dir, progress)?;
    }
    Ok(())
}

fn one(
    base: &str,
    entry: &catalogue::Entry,
    file: &catalogue::File,
    dir: &Path,
    progress: &mut dyn Progress,
) -> Result<(), FetchError> {
    let url = format!(
        "{base}/{}/resolve/{}/{}",
        entry.repo, entry.revision, file.name
    );
    let part = dir.join(format!("{}.part", file.name));
    let final_path = dir.join(file.name);

    let response = match ureq::get(&url).call() {
        Ok(response) => response,
        Err(ureq::Error::Status(code, _)) => return Err(FetchError::Status { url, code }),
        Err(e) => return Err(FetchError::Http { url, why: e.to_string() }),
    };

    progress.file(file.name, file.bytes);
    let outcome = stream_to(&mut response.into_reader(), &part, progress);
    // Whatever happened, a .part never survives this function.
    let result = outcome.and_then(|written| {
        if written != file.bytes {
            return Err(FetchError::ShortRead {
                name: file.name.to_string(),
                expected: file.bytes,
                found: written,
            });
        }
        let digest = crate::stt::model::sha256_of(&part).map_err(|why| FetchError::Io {
            path: part.clone(),
            why,
        })?;
        if digest != file.sha256 {
            return Err(FetchError::Digest { name: file.name.to_string() });
        }
        std::fs::rename(&part, &final_path).map_err(|e| FetchError::Io {
            path: final_path.clone(),
            why: e.to_string(),
        })
    });
    if result.is_err() {
        let _ = std::fs::remove_file(&part);
        return result;
    }
    progress.done(file.name);
    Ok(())
}

fn stream_to(
    reader: &mut dyn Read,
    part: &Path,
    progress: &mut dyn Progress,
) -> Result<u64, FetchError> {
    let mut out = std::fs::File::create(part).map_err(|e| FetchError::Io {
        path: part.to_path_buf(),
        why: e.to_string(),
    })?;
    let mut buffer = vec![0u8; 1 << 16];
    let mut written = 0u64;
    loop {
        let read = reader.read(&mut buffer).map_err(|e| FetchError::Http {
            url: part.display().to_string(),
            why: e.to_string(),
        })?;
        if read == 0 {
            break;
        }
        out.write_all(&buffer[..read]).map_err(|e| FetchError::Io {
            path: part.to_path_buf(),
            why: e.to_string(),
        })?;
        written += read as u64;
        progress.bytes(written);
    }
    out.flush().map_err(|e| FetchError::Io {
        path: part.to_path_buf(),
        why: e.to_string(),
    })?;
    Ok(written)
}
```

Declare it in `src/stt.rs`:

```rust
pub mod fetch;
```

The digest is computed by re-reading the `.part` rather than hashed during the
stream. It is one extra pass over a file already in the page cache, and it means
`sha256_of` — the function the `ggml` store has used since #13 — is the single
implementation both stores verify with, rather than a second one inlined here.

- [ ] **Step 4: Run to verify they pass**

```sh
cargo test --lib stt::fetch
# and repeatedly, because it binds sockets and joins threads
for i in 1 2 3 4 5; do cargo test --lib stt::fetch -- --test-threads=16 || break; done
cargo clippy --all-targets -- -D warnings && cargo fmt --check
```

- [ ] **Step 5: Commit**

```sh
git add src/stt.rs src/stt/fetch.rs
git commit -m "$(cat <<'MSG'
Fetch a model into .part, verify it, and only then give it its name

A file under its real name is by construction one that passed every check, so
there is no window in which a half-written model exists for a later run to
mistake for a whole one. The base address is a parameter of the inner
function, so the tests drive it against a listener on loopback.

Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>
MSG
)"
```

---

### Task 7: `stt::candle::device` — which device, and saying so

**Files:**
- Create: `src/stt/candle/device.rs`
- Modify: `src/stt/candle.rs` (declare `pub mod device;`)

**Interfaces:**
- Produces: `device::{Selection, select, describe, device_for}`. Tasks 9, 10 and
  11 use them.

The selection is made outside `CandleEngine::new` and handed to it, so `doctor`
can report the device without building an engine and a test can pin it without
the hardware. Measured while designing, on a 66-second take: the default model
runs 5.1 s on Metal and 52.3 s on the CPU; `tiny` runs 0.69 s against 6.4 s.

- [ ] **Step 1: Write the failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asking_for_a_device_always_answers() {
        // Never a failure: a slow transcript beats no transcript. This is the
        // one test here that touches the machine, and it asserts only that some
        // answer comes back, so it passes on a CI runner with no GPU.
        let _ = select();
    }

    #[test]
    fn metal_is_described_without_alarming_anybody() {
        let text = describe(&Selection::Metal);
        assert!(text.to_lowercase().contains("metal"), "got {text}");
        assert!(!text.contains("slow"), "nothing is wrong here: {text}");
    }

    #[test]
    fn the_cpu_says_why_and_what_it_costs_and_what_to_do() {
        let text = describe(&Selection::Cpu { why: "Metal exists only on macOS" });
        assert!(text.to_lowercase().contains("cpu"), "got {text}");
        assert!(text.contains("Metal exists only on macOS"), "the why must survive: {text}");
        // Ten times slower is a different product, not a slower one. The person
        // must learn that before they wait a minute for a minute of speech.
        assert!(text.contains("ten times"), "it must name the cost: {text}");
        assert!(text.contains("tiny"), "it must name a model that stays usable: {text}");
    }

    #[test]
    fn both_variants_are_describable_with_no_hardware_at_all() {
        // Selection is a plain enum precisely so this holds on every runner.
        for selection in [Selection::Metal, Selection::Cpu { why: "asked and refused" }] {
            assert!(!describe(&selection).is_empty());
        }
    }
}
```

- [ ] **Step 2: Run to verify they fail**

```sh
cargo test --lib candle::device 2>&1 | tail -20
```

- [ ] **Step 3: Implement `src/stt/candle/device.rs`**

```rust
//! Which device the model runs on, chosen once and reported everywhere.
//!
//! The choice is made here rather than inside `CandleEngine::new`, so `doctor`
//! can report it without building an engine and a test can pin it without the
//! hardware. Falling back is never a refusal, and never silent: it costs a
//! factor of ten. See `tasks/15/DESIGN_15.md`, section 8.

/// What the engine will run on, and — when it is not the fast answer — why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Selection {
    Metal,
    Cpu { why: &'static str },
}

/// Ask for a device. Never fails: a slow transcript beats no transcript, and the
/// person is told which they are getting before they wait for it.
pub fn select() -> Selection {
    #[cfg(target_os = "macos")]
    {
        // Task 1's target-specific dependency block gives every macOS build the
        // metal feature, so asking is the whole test: if a device comes back,
        // the kernels are there.
        match candle_core::Device::new_metal(0) {
            Ok(_) => Selection::Metal,
            Err(_) => Selection::Cpu {
                why: "this machine refused a Metal device",
            },
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        Selection::Cpu {
            why: "Metal exists only on macOS",
        }
    }
}

/// The device a selection names. Asking Metal for a handle twice is cheap; it is
/// a handle, not a context full of compiled kernels.
pub fn device_for(selection: &Selection) -> candle_core::Device {
    match selection {
        Selection::Metal => {
            candle_core::Device::new_metal(0).unwrap_or(candle_core::Device::Cpu)
        }
        Selection::Cpu { .. } => candle_core::Device::Cpu,
    }
}

/// The sentence `doctor` prints and the daemon writes at start.
pub fn describe(selection: &Selection) -> String {
    match selection {
        Selection::Metal => "on the GPU, through Metal".to_string(),
        Selection::Cpu { why } => format!(
            "on the CPU ({why}), which is about ten times slower than the GPU — a \
             minute of speech takes about a minute with the default model. Set \
             [stt] model to \"tiny\" if that is too slow"
        ),
    }
}
```

`device_for`'s `unwrap_or` is the one fallible call here and it cannot panic: a
refused Metal handle becomes the CPU, which is the same answer `select` would
have given. It is a fallback, not a swallow — `select` has already reported which
device the person is on.

**No feature flag on this crate, and no manifest change.** Task 1's
`[target.'cfg(target_os = "macos")'.dependencies]` block already gives every
macOS build the `metal` feature on all three candle crates, and cargo unifies
features per crate, so there is nothing for a `cfg(feature = ...)` here to test.
That is why `select` branches on `target_os` alone and then simply asks for a
device: on macOS the kernels are compiled in, and a refusal means the machine,
not the build. `herdr-plugin.toml`'s `[[build]]` entries stay exactly as they
are, which is also what keeps `scripts/check_manifest.py` unaffected.

This arrangement is the one that was verified during design — the probe crate
used precisely this block and Metal worked. The alternative, a `metal` feature on
this crate passed by CI and the release build, was considered and dropped:
cargo cannot enable a feature per target, so it would mean every macOS build
needing a flag nobody can forget, for no gain over a block cargo already
resolves.

- [ ] **Step 4: Run to verify they pass**

```sh
cargo test --lib candle::device
python3 scripts/check_manifest.py    # unchanged, and must stay so
cargo clippy --all-targets -- -D warnings && cargo fmt --check
```

On macOS `select()` must answer `Metal`. Confirm it once by hand rather than
assuming, since every later task's timing depends on it:

```sh
cargo test --lib candle::device -- --nocapture
```

- [ ] **Step 5: Commit**

```sh
git add src/stt/candle.rs src/stt/candle/device.rs
git commit -m "$(cat <<'MSG'
Choose the device outside the engine, and say which one it is

Reporting the device through a value rather than the trait keeps stt::Engine
at one method and lets doctor name the device without building an engine.
Falling back to the CPU is never a refusal and never silent: measured at ten
times slower, which is a different product rather than a slower one.

Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>
MSG
)"
```

---

### Task 8: `stt::candle::plan` — every decision, with no tensor in sight

**Files:**
- Create: `src/stt/candle/plan.rs`
- Modify: `src/stt/candle.rs` (declare `pub mod plan;`)

**Interfaces:**
- Produces: `plan::{Window, MIN_REAL_FRAMES, next_window, advance, prompt_tokens,
  text_tokens, last_timestamp}`. Task 9 calls every one of them.

This is where a wrong answer produces a wrong transcript, and none of it needs
weights. Task 9's loop asks this module what to do and multiplies matrices.

- [ ] **Step 1: Write the failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use candle_transformers::models::whisper::N_FRAMES;

    #[test]
    fn a_take_shorter_than_a_window_is_one_window() {
        let w = next_window(1_500, 0).expect("some window");
        assert_eq!(w.start, 0);
        assert_eq!(w.len, 1_500, "the real audio, not the padded length");
    }

    #[test]
    fn a_long_take_is_cut_at_the_window_length() {
        let w = next_window(9_000, 0).expect("some window");
        assert_eq!(w.len, N_FRAMES);
        let w = next_window(9_000, 6_000).expect("some window");
        assert_eq!(w.start, 6_000);
        assert_eq!(w.len, N_FRAMES);
    }

    #[test]
    fn a_tail_that_is_almost_all_padding_is_not_a_window() {
        // Found by running it: a final window holding 2 frames of real audio and
        // 2998 of zeros returned "[Music]" — confident text transcribed from
        // silence. The take ends at the guard instead.
        assert!(next_window(9_000, 8_998).is_none(), "2 frames is not a window");
        assert!(next_window(9_000, 9_000 - (MIN_REAL_FRAMES - 1)).is_none());
        assert!(next_window(9_000, 9_000 - MIN_REAL_FRAMES).is_some(), "exactly the guard is enough");
    }

    #[test]
    fn a_seek_at_or_past_the_end_is_the_end() {
        assert!(next_window(3_000, 3_000).is_none());
        assert!(next_window(3_000, 4_000).is_none());
    }

    #[test]
    fn a_full_window_advances_to_its_last_timestamp() {
        // Measured on a 66-second sample: window 1 advanced 2998 frames, not
        // 3000, and the boundary read continuously.
        let w = Window { start: 0, len: N_FRAMES };
        let ts_begin = 50_364u32;
        // Each timestamp token is 20 ms, and a frame is 10 ms.
        let at_29_98s = ts_begin + 1_499;
        assert_eq!(advance(&w, Some(at_29_98s), ts_begin), 2_998);
    }

    #[test]
    fn a_window_with_no_timestamp_advances_by_its_whole_length() {
        let w = Window { start: 0, len: N_FRAMES };
        assert_eq!(advance(&w, None, 50_364), N_FRAMES);
    }

    #[test]
    fn a_final_short_window_advances_past_the_end_whatever_it_said() {
        // Otherwise a timestamp inside the padding sends the seek backwards and
        // the same audio is transcribed forever.
        let w = Window { start: 6_000, len: 900 };
        assert_eq!(advance(&w, Some(50_364 + 10), 50_364), 900);
    }

    #[test]
    fn a_zero_or_backward_timestamp_cannot_stall_the_loop() {
        let w = Window { start: 0, len: N_FRAMES };
        let ts_begin = 50_364u32;
        assert_eq!(advance(&w, Some(ts_begin), ts_begin), N_FRAMES, "0 s would never advance");
        assert_eq!(advance(&w, Some(ts_begin + 10_000), ts_begin), N_FRAMES, "past the window");
    }

    #[test]
    fn the_prompt_is_the_marker_then_the_end_of_the_bias() {
        let ids: Vec<u32> = (1..=10).collect();
        assert_eq!(prompt_tokens(999, &ids, 4), vec![999, 7, 8, 9, 10]);
    }

    #[test]
    fn the_end_of_the_bias_is_what_survives_the_cut() {
        // bias::collect puts the conversation at the end, and the conversation is
        // what carries the terms. Cutting the head would drop exactly that.
        let ids: Vec<u32> = (1..=300).collect();
        let out = prompt_tokens(999, &ids, 224);
        assert_eq!(out.len(), 225);
        assert_eq!(out[0], 999);
        assert_eq!(*out.last().unwrap(), 300);
    }

    #[test]
    fn an_empty_bias_is_no_prompt_at_all() {
        // Not a bare marker: a <|startofprev|> with nothing after it is a prompt
        // the model has to make sense of.
        assert!(prompt_tokens(999, &[], 224).is_empty());
    }

    #[test]
    fn timestamps_are_stripped_from_the_text_and_read_for_the_advance() {
        let ts_begin = 50_364u32;
        let body = vec![ts_begin, 11, 12, ts_begin + 40, 13, ts_begin + 90];
        assert_eq!(text_tokens(&body, ts_begin), vec![11, 12, 13]);
        assert_eq!(last_timestamp(&body, ts_begin), Some(ts_begin + 90));
    }

    #[test]
    fn a_window_that_produced_no_timestamp_says_so() {
        assert_eq!(last_timestamp(&[11, 12, 13], 50_364), None);
        assert_eq!(last_timestamp(&[], 50_364), None);
    }
}
```

- [ ] **Step 2: Run to verify they fail**

```sh
cargo test --lib candle::plan 2>&1 | tail -20
```

- [ ] **Step 3: Implement `src/stt/candle/plan.rs`**

```rust
//! Every decision the decoder makes, with no tensor in any signature.
//!
//! Which frames go into which window, what tokens precede the audio, where a
//! window's text ends and where the next one starts. This is where a wrong
//! answer produces a wrong transcript, and none of it needs weights to test.
//! See `tasks/15/DESIGN_15.md`, sections 1 and 7.

use candle_transformers::models::whisper::N_FRAMES;

/// A window of mel frames: where it starts, and how much of it is real audio
/// rather than the zero padding that fills a window out to 30 seconds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Window {
    pub start: usize,
    pub len: usize,
}

/// One second. A window with less real audio than this is not decoded: found by
/// running it, a window of 2 real frames and 2 998 of padding returned "[Music]".
pub const MIN_REAL_FRAMES: usize = 100;

/// Each timestamp token is 20 milliseconds and each mel frame is 10, so a
/// timestamp's index doubles into frames.
const FRAMES_PER_TIMESTAMP: usize = 2;

/// The next window, or `None` when what is left is padding.
pub fn next_window(frames: usize, seek: usize) -> Option<Window> {
    if seek >= frames {
        return None;
    }
    let len = (frames - seek).min(N_FRAMES);
    if len < MIN_REAL_FRAMES {
        return None;
    }
    Some(Window { start: seek, len })
}

/// How far to move after a window. A full window moves to the last timestamp it
/// produced, which is what keeps a word from being cut at the boundary; anything
/// else moves past its own end, because a timestamp inside padding is about
/// silence and a backward or zero advance would decode the same audio forever.
pub fn advance(window: &Window, last_timestamp: Option<u32>, ts_begin: u32) -> usize {
    if window.len < N_FRAMES {
        return window.len;
    }
    match last_timestamp {
        Some(token) if token > ts_begin => {
            let frames = (token - ts_begin) as usize * FRAMES_PER_TIMESTAMP;
            if frames == 0 || frames > N_FRAMES {
                N_FRAMES
            } else {
                frames
            }
        }
        _ => N_FRAMES,
    }
}

/// The initial prompt: the marker, then the last `limit` tokens of the bias.
/// The end survives the cut because `bias::collect` puts the conversation there
/// and the conversation is what carries the technical terms.
pub fn prompt_tokens(start_of_prev: u32, bias: &[u32], limit: usize) -> Vec<u32> {
    if bias.is_empty() || limit == 0 {
        return Vec::new();
    }
    let keep = bias.len().min(limit);
    let mut out = Vec::with_capacity(keep + 1);
    out.push(start_of_prev);
    out.extend_from_slice(&bias[bias.len() - keep..]);
    out
}

/// The tokens that are words, with the timestamps taken out.
pub fn text_tokens(body: &[u32], ts_begin: u32) -> Vec<u32> {
    body.iter().copied().filter(|&t| t < ts_begin).collect()
}

/// The last timestamp a window produced, which is where the next one starts.
pub fn last_timestamp(body: &[u32], ts_begin: u32) -> Option<u32> {
    body.iter().rev().copied().find(|&t| t >= ts_begin)
}
```

Declare it in `src/stt/candle.rs`:

```rust
pub mod plan;
```

- [ ] **Step 4: Run to verify they pass**

```sh
cargo test --lib candle::plan
cargo clippy --all-targets -- -D warnings && cargo fmt --check
```

- [ ] **Step 5: Commit**

```sh
git add src/stt/candle.rs src/stt/candle/plan.rs
git commit -m "$(cat <<'MSG'
Put every decoding decision in a pure module the tests can reach

Window planning, the one-second guard, the timestamp advance and prompt
assembly are where a wrong answer becomes a wrong transcript, and none of it
needs weights. The guard exists because a tail window of 2 real frames
returned "[Music]" when the design was being probed.

Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>
MSG
)"
```

---

### Task 9: `CandleEngine` — the tensor half

**Files:**
- Create: `src/stt/candle/decode.rs`
- Modify: `src/stt/candle.rs` (declare `pub mod decode;`, and hold `CandleEngine`)

**Interfaces:**
- Consumes: `mel::{filters, spectrogram}` (Task 3); `store::Found` (Task 5);
  `device::{Selection, device_for}` (Task 7); every `plan::` item (Task 8);
  `audio::wav::read` (Task 2).
- Produces: `candle::CandleEngine::new(found: &store::Found, language: &str,
  selection: &device::Selection) -> Result<CandleEngine, EngineError>`, and its
  `impl stt::Engine`. Task 10 constructs it.

This is the residue: a loop that asks `plan` what to do and multiplies matrices.
Nothing here is tested without weights, which is why Task 8 exists.

- [ ] **Step 1: Write what can be tested without weights**

These tests go in **`src/stt/candle.rs`**, not in `decode.rs`: the two functions
they exercise, `check_mel_bins` and `read_take`, are defined there in Step 4.
`decode.rs` gets no test module of its own — nothing in it can be reached without
weights, which is the whole reason `plan.rs` exists.

Only two things in this task can be tested without weights: that construction refuses a model whose
`config.json` names a mel-bin count nothing supports, and that `transcribe`
refuses a take of the wrong audio shape before it touches the model. Both are
before any tensor work.

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unsupported_mel_bin_count_is_refused_by_name() {
        // Whisper is 80 or 128. Anything else means a model this engine cannot
        // run, and the message has to say so rather than producing a filterbank
        // of the wrong shape and a blank transcript.
        let error = check_mel_bins(64).expect_err("64 is not a Whisper model");
        let message = error.to_string();
        assert!(message.contains("64"), "got {message}");
        assert!(message.contains("80"), "got {message}");
        assert!(message.contains("128"), "got {message}");
    }

    #[test]
    fn eighty_and_a_hundred_and_twenty_eight_are_accepted() {
        assert!(check_mel_bins(80).is_ok());
        assert!(check_mel_bins(128).is_ok());
    }

    #[test]
    fn a_take_at_the_wrong_rate_is_refused_before_the_model_is_touched() {
        let dir = std::env::temp_dir().join(format!("candle-rate-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("take.wav");
        crate::audio::wav::write(&path, &[0.0f32; 1000], 44_100).unwrap();
        let error = read_take(&path).expect_err("must refuse");
        let message = error.to_string();
        assert!(message.contains("44100"), "it must say what it found: {message}");
        assert!(message.contains("16000") || message.contains("16 kHz"), "got {message}");
    }

    #[test]
    fn a_take_at_sixteen_kilohertz_is_read() {
        let dir = std::env::temp_dir().join(format!("candle-ok-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("take.wav");
        crate::audio::wav::write(&path, &[0.1f32; 1600], 16_000).unwrap();
        assert_eq!(read_take(&path).expect("must read").len(), 1600);
    }
}
```

- [ ] **Step 2: Run to verify they fail**

```sh
cargo test --lib candle:: 2>&1 | tail -20
```

- [ ] **Step 3: Implement `src/stt/candle/decode.rs`**

The decoding loop, taking the model and the mel tensor and returning text. It
calls `plan` for every decision.

```rust
//! The greedy decoding loop. Every decision it makes comes from `plan`; what is
//! left here is the encoder pass, an argmax and the tokenizer.
//!
//! Greedy at temperature zero, no beam search and no temperature fallback. What
//! the fallback buys — a retry when the output degenerates into repetition — is
//! not free to omit, and `tasks/15/DESIGN_15.md` section 10 records what that
//! leaves open. See section 7 for the rest.

use candle_core::{IndexOp, Tensor, D};
use candle_transformers::models::whisper::{self as whisper, Config, N_FRAMES};
use tokenizers::Tokenizer;

use super::plan;

/// The 99 languages Whisper's multilingual models know, as the tokenizer spells
/// them. Used for `[stt] language = "auto"` and to refuse an unknown value by
/// name. Taken from the model card's own list.
pub const LANGUAGES: [&str; 99] = [
    "en", "zh", "de", "es", "ru", "ko", "fr", "ja", "pt", "tr", "pl", "ca", "nl", "ar", "sv",
    "it", "id", "hi", "fi", "vi", "he", "uk", "el", "ms", "cs", "ro", "da", "hu", "ta", "no",
    "th", "ur", "hr", "bg", "lt", "la", "mi", "ml", "cy", "sk", "te", "fa", "lv", "bn", "sr",
    "az", "sl", "kn", "et", "mk", "br", "eu", "is", "hy", "ne", "mn", "bs", "kk", "sq", "sw",
    "gl", "mr", "pa", "si", "km", "sn", "yo", "so", "af", "oc", "ka", "be", "tg", "sd", "gu",
    "am", "yi", "lo", "uz", "fo", "ht", "ps", "tk", "nn", "mt", "sa", "lb", "my", "bo", "tl",
    "mg", "as", "tt", "haw", "ln", "ha", "ba", "jw", "su",
];

/// The special tokens a decode needs, looked up once.
pub struct Tokens {
    pub sot: u32,
    pub eot: u32,
    pub transcribe: u32,
    pub no_timestamps: u32,
    pub start_of_prev: u32,
    /// The first timestamp token: everything at or above it is a time, not a word.
    pub ts_begin: u32,
}

impl Tokens {
    pub fn look_up(tokenizer: &Tokenizer) -> Result<Tokens, String> {
        let id = |name: &str| {
            tokenizer
                .token_to_id(name)
                .ok_or_else(|| format!("the tokenizer has no {name} token, so this is not a Whisper tokenizer"))
        };
        let no_timestamps = id(whisper::NO_TIMESTAMPS_TOKEN)?;
        Ok(Tokens {
            sot: id(whisper::SOT_TOKEN)?,
            eot: id(whisper::EOT_TOKEN)?,
            transcribe: id(whisper::TRANSCRIBE_TOKEN)?,
            no_timestamps,
            start_of_prev: id("<|startofprev|>")?,
            ts_begin: no_timestamps + 1,
        })
    }
}

/// The language token for a configured value, or `None` for `auto`, or an error
/// naming a value the tokenizer does not know.
pub fn language_token(tokenizer: &Tokenizer, language: &str) -> Result<Option<u32>, String> {
    if language.eq_ignore_ascii_case("auto") {
        return Ok(None);
    }
    let name = format!("<|{}|>", language.to_lowercase());
    tokenizer.token_to_id(&name).map(Some).ok_or_else(|| {
        format!(
            "[stt] language is {language:?}, which this model does not know. Use a \
             two-letter code such as \"en\" or \"ru\", or \"auto\" to detect it"
        )
    })
}
```

The rest of `decode.rs` is the loop. The API surface below was read off
`candle-transformers` 0.11.0's own source, not recalled: `Whisper` exposes
`pub encoder: AudioEncoder`, `pub decoder: TextDecoder` and `pub config: Config`;
`AudioEncoder::forward(&mut self, x: &Tensor, flush_kv_cache: bool)`;
`TextDecoder::forward(&mut self, x: &Tensor, xa: &Tensor, flush_kv_cache: bool)`;
`TextDecoder::final_linear(&self, x: &Tensor)`; and `Whisper::reset_kv_cache(&mut self)`.

```rust
/// Which language is being spoken, as a token. One decoder step from the start
/// token, an argmax over the language tokens only. Measured at 6 ms warm, so it
/// runs on the first window and the answer is reused for the rest of the take.
pub fn detect_language(
    model: &mut Whisper,
    tokenizer: &Tokenizer,
    tokens: &Tokens,
    features: &Tensor,
    device: &candle_core::Device,
) -> Result<u32, String> {
    // The 99 names become the ids index_select needs. A model that knows none of
    // them is not multilingual, and saying so beats detecting nothing silently.
    let mut ids = Vec::with_capacity(LANGUAGES.len());
    for code in LANGUAGES.iter() {
        if let Some(id) = tokenizer.token_to_id(&format!("<|{code}|>")) {
            ids.push(id);
        }
    }
    if ids.is_empty() {
        return Err(
            "this model has no language tokens, so [stt] language cannot be \"auto\"; \
             set it to the language you speak, such as \"en\""
                .to_string(),
        );
    }

    model.reset_kv_cache();
    let start = Tensor::new(&[[tokens.sot]], device).map_err(|e| e.to_string())?;
    let hidden = model
        .decoder
        .forward(&start, features, true)
        .map_err(|e| e.to_string())?;
    // The one row of logits for the one position fed in.
    let logits = model
        .decoder
        .final_linear(&hidden.i(..1).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?
        .i(0)
        .map_err(|e| e.to_string())?
        .i(0)
        .map_err(|e| e.to_string())?;
    let wanted = Tensor::new(ids.as_slice(), device).map_err(|e| e.to_string())?;
    let restricted = logits.index_select(&wanted, 0).map_err(|e| e.to_string())?;
    let probabilities = candle_nn::ops::softmax(&restricted, D::Minus1)
        .map_err(|e| e.to_string())?
        .to_vec1::<f32>()
        .map_err(|e| e.to_string())?;
    let best = probabilities
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.total_cmp(b.1))
        .map(|(at, _)| at)
        .unwrap_or(0);
    Ok(ids[best])
}

/// Greedy decoding: the argmax token, appended, until the end token or the cap.
/// Returns only what was generated after `prefix`.
///
/// Temperature zero and no fallback ladder — `tasks/15/DESIGN_15.md` section 10
/// records what that leaves open. The cap is what bounds a degenerate repetition.
fn greedy(
    model: &mut Whisper,
    features: &Tensor,
    prefix: &[u32],
    limit: usize,
    eot: u32,
    device: &candle_core::Device,
) -> Result<Vec<u32>, String> {
    let mut tokens = prefix.to_vec();
    model.reset_kv_cache();
    for step in 0..limit {
        let input = Tensor::new(tokens.as_slice(), device)
            .and_then(|t| t.unsqueeze(0))
            .map_err(|e| e.to_string())?;
        // Flush on the first step only: the cache is being filled from empty.
        let hidden = model
            .decoder
            .forward(&input, features, step == 0)
            .map_err(|e| e.to_string())?;
        let last = hidden.dim(1).map_err(|e| e.to_string())? - 1;
        let logits = model
            .decoder
            .final_linear(&hidden.i((..1, last..)).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?
            .i(0)
            .map_err(|e| e.to_string())?
            .i(0)
            .map_err(|e| e.to_string())?
            .to_vec1::<f32>()
            .map_err(|e| e.to_string())?;
        let next = logits
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.total_cmp(b.1))
            .map(|(at, _)| at as u32)
            .ok_or_else(|| "the model produced no logits".to_string())?;
        if next == eot {
            break;
        }
        tokens.push(next);
    }
    Ok(tokens[prefix.len().min(tokens.len())..].to_vec())
}

/// A whole take: window by window, each conditioned on the bias string and on
/// what the previous window said. Every decision here comes from `plan`.
#[allow(clippy::too_many_arguments)]
pub fn run(
    model: &mut Whisper,
    tokenizer: &Tokenizer,
    tokens: &Tokens,
    mel: &Tensor,
    frames: usize,
    bias: &[u32],
    configured_language: Option<u32>,
    config: &Config,
    device: &candle_core::Device,
) -> Result<String, String> {
    let mut language = configured_language;
    let mut seek = 0usize;
    let mut text = String::new();
    let mut previous: Vec<u32> = Vec::new();

    while let Some(window) = plan::next_window(frames, seek) {
        let slice = mel
            .narrow(2, window.start, window.len)
            .map_err(|e| e.to_string())?;
        // Whisper's encoder takes exactly 30 seconds; a short window is padded.
        let padded = if window.len < N_FRAMES {
            let pad = Tensor::zeros(
                (1, config.num_mel_bins, N_FRAMES - window.len),
                whisper::DTYPE,
                device,
            )
            .map_err(|e| e.to_string())?;
            Tensor::cat(&[&slice, &pad], 2).map_err(|e| e.to_string())?
        } else {
            slice
        };
        let features = model
            .encoder
            .forward(&padded, true)
            .map_err(|e| e.to_string())?;

        if language.is_none() {
            language = Some(detect_language(model, tokenizer, tokens, &features, device)?);
        }
        let language_token = language.expect("just set above if it was None");

        // The bias string first, then what the last window said: the prompt is
        // cut from the front, and the newest context is what must survive.
        let mut carry = bias.to_vec();
        carry.extend_from_slice(&previous);
        let mut prefix = plan::prompt_tokens(
            tokens.start_of_prev,
            &carry,
            config.max_target_positions / 2 - 1,
        );
        // Timestamps on: the advance depends on them. No no_timestamps token.
        prefix.extend_from_slice(&[tokens.sot, language_token, tokens.transcribe]);

        let limit = config.max_target_positions.saturating_sub(prefix.len() + 8);
        let body = greedy(model, &features, &prefix, limit, tokens.eot, device)?;

        let words = plan::text_tokens(&body, tokens.ts_begin);
        let piece = tokenizer.decode(&words, true).map_err(|e| e.to_string())?;
        let piece = piece.trim();
        if !piece.is_empty() {
            if !text.is_empty() {
                text.push(' ');
            }
            text.push_str(piece);
        }
        previous = words;

        seek += plan::advance(
            &window,
            plan::last_timestamp(&body, tokens.ts_begin),
            tokens.ts_begin,
        );
    }
    Ok(text)
}
```

**This code was compiled before the plan shipped**, against
`candle-transformers` 0.11.0 with `candle-core`/`candle-nn` at the same version
and the `metal` feature on — together with `plan.rs` from Task 8 and
`CandleEngine::new` from Step 4, in a throwaway crate. It builds with no warnings
and passes `cargo clippy --all-targets -- -D warnings`. That is not a promise it
is correct, only that its API surface is real and its types line up; the
behaviour is what Task 15 measures.

The `expect` on `language` is on a value assigned two lines above in the same
scope, on the only path that can reach it. If that reads as a panic path to the
implementer, replace it with `let Some(t) = language else { continue };` — but do
not leave it as `unwrap()` without the comment.

Every branch in `run` is `plan`'s, and `plan`'s tests cover each one. Add the
imports `decode.rs` needs: `candle_core::{IndexOp, Tensor, D}`,
`candle_transformers::models::whisper::{self as whisper, model::Whisper, Config,
N_FRAMES}`, `tokenizers::Tokenizer`, and `super::plan`.

- [ ] **Step 4: Implement `CandleEngine` in `src/stt/candle.rs`**

```rust
pub mod decode;
pub mod device;
pub mod mel;
pub mod plan;
pub mod store;

use std::path::Path;
use std::sync::Mutex;

use candle_core::{Device, Tensor};
use candle_nn::VarBuilder;
use candle_transformers::models::whisper::{self as whisper, model::Whisper, Config, N_FRAMES};
use tokenizers::Tokenizer;

use crate::stt::{Engine, EngineError};

/// A Whisper model, resident. The `Mutex` is not for contention — a take is one
/// at a time — but because decoding mutates the model's key-value cache and
/// `Engine::transcribe` takes `&self`.
pub struct CandleEngine {
    model: Mutex<Whisper>,
    tokenizer: Tokenizer,
    tokens: decode::Tokens,
    config: Config,
    filters: Vec<f32>,
    device: Device,
    language: Option<u32>,
}

/// Whisper is 80 mel bins, or 128 for the large-v3 models. Anything else is a
/// model this engine cannot run, and saying so beats a filterbank of the wrong
/// shape and a blank transcript.
fn check_mel_bins(bins: usize) -> Result<(), EngineError> {
    if bins == 80 || bins == 128 {
        Ok(())
    } else {
        Err(EngineError::Candle(format!(
            "this model wants {bins} mel bins and this engine builds 80 or 128; it \
             is not a Whisper model this plugin can run. Run `herdr-voice model \
             --choose` to install one that is"
        )))
    }
}

/// A take, as samples, refusing anything that is not what the recorder writes.
fn read_take(path: &Path) -> Result<Vec<f32>, EngineError> {
    let (pcm, rate) = crate::audio::wav::read(path)
        .map_err(|e| EngineError::Candle(e.to_string()))?;
    if rate != whisper::SAMPLE_RATE as u32 {
        return Err(EngineError::Candle(format!(
            "the take at {} is {rate} Hz and this engine reads 16000 Hz, which is \
             what this plugin records. The file was not produced by this plugin",
            path.display()
        )));
    }
    Ok(pcm)
}
```

`CandleEngine::new` is written out here for the same reason the decoding
functions are: every call it makes is into a crate that is not in the tree yet
and has no precedent anywhere in this repository. The signatures used below were
read off `candle-transformers` 0.11.0 and `tokenizers` 0.22, and **this code was
compiled** together with the decoding functions before the plan shipped:
`Config` derives `Deserialize` and `Clone`, so `serde_json::from_str` loads it;
`Tokenizer::from_file` takes a path and its error has `to_string`;
`VarBuilder::from_mmaped_safetensors(&[PathBuf], DType, &Device)` is `unsafe`;
and `Whisper::load(&VarBuilder, Config)` takes the builder by reference and the
config by value.

```rust
impl CandleEngine {
    pub fn new(
        found: &store::Found,
        language: &str,
        selection: &device::Selection,
    ) -> Result<CandleEngine, EngineError> {
        // Counted so that `doctor_reads_no_weights` (Task 11) means what it says:
        // this is the one place weights are read.
        #[cfg(test)]
        store::weight_reads::increment();

        let dir = found.dir();
        // Every failure below names the file and what to do, which is the whole
        // of `CLAUDE.md`'s rule applied to a directory of three files.
        let named = |path: &Path, why: String| {
            EngineError::Candle(format!(
                "cannot use the speech model at {}: {why}. Run `herdr-voice model \
                 --choose` to install one again",
                path.display()
            ))
        };

        let config_path = dir.config();
        let config_text = std::fs::read_to_string(&config_path)
            .map_err(|e| named(&config_path, e.to_string()))?;
        let config: Config = serde_json::from_str(&config_text)
            .map_err(|e| named(&config_path, e.to_string()))?;
        check_mel_bins(config.num_mel_bins)?;

        let tokenizer_path = dir.tokenizer();
        let tokenizer = Tokenizer::from_file(&tokenizer_path)
            .map_err(|e| named(&tokenizer_path, e.to_string()))?;
        let tokens = decode::Tokens::look_up(&tokenizer).map_err(EngineError::Candle)?;
        let language = decode::language_token(&tokenizer, language)
            .map_err(EngineError::Candle)?;

        let filters = mel::filters(config.num_mel_bins);
        let device = device::device_for(selection);

        let weights_path = dir.weights();
        // Safe here in the sense that matters: the file was verified by
        // `store::locate` before this was called, so it is not a mapping of
        // something arbitrary. A mapped file changed underneath is the residual
        // risk, and it is the same one every mmap-based loader carries.
        let builder = unsafe {
            VarBuilder::from_mmaped_safetensors(
                &[weights_path.clone()],
                whisper::DTYPE,
                &device,
            )
        }
        .map_err(|e| named(&weights_path, e.to_string()))?;
        let model = Whisper::load(&builder, config.clone())
            .map_err(|e| named(&weights_path, e.to_string()))?;

        let engine = CandleEngine {
            model: Mutex::new(model),
            tokenizer,
            tokens,
            config,
            filters,
            device,
            language,
        };
        engine.warm();
        Ok(engine)
    }

    /// One encoder pass over 30 seconds of silence, so the first real take does
    /// not pay for it — measured at 0.77-0.84 s for the default model
    /// (`tasks/15/DESIGN_15.md`, section 8).
    ///
    /// Not fatal if it fails: a warm-up is an optimisation, and the take path
    /// reports its own failures with a take in hand to name. Nothing here
    /// unwraps, so a poisoned lock or a device hiccup costs the warm-up and
    /// nothing else.
    fn warm(&self) {
        if let Ok(silence) = Tensor::zeros(
            (1, self.config.num_mel_bins, N_FRAMES),
            whisper::DTYPE,
            &self.device,
        ) {
            if let Ok(mut model) = self.model.lock() {
                let _ = model.encoder.forward(&silence, true);
                model.reset_kv_cache();
            }
        }
    }
}
```

`src/stt/candle.rs` needs these imports for the above: `std::path::Path`,
`std::sync::Mutex`, `candle_core::Tensor`, `candle_nn::VarBuilder`,
`candle_transformers::models::whisper::{self as whisper, model::Whisper, Config,
N_FRAMES}`, and `tokenizers::Tokenizer`.

And the trait:

```rust
impl Engine for CandleEngine {
    fn transcribe(&self, audio: &Path, bias: &str) -> Result<String, EngineError> {
        let pcm = read_take(audio)?;
        let frames_data = mel::spectrogram(&self.config, &pcm, &self.filters);
        let frames = frames_data.len() / self.config.num_mel_bins;
        let mel = Tensor::from_vec(
            frames_data,
            (1, self.config.num_mel_bins, frames),
            &self.device,
        )
        .map_err(|e| EngineError::Candle(format!("cannot build the spectrogram: {e}")))?;

        let bias_ids: Vec<u32> = if bias.is_empty() {
            Vec::new()
        } else {
            // A bias string that will not tokenise costs the take its terms, not
            // the take: the transcript is still worth having.
            self.tokenizer
                .encode(bias, false)
                .map(|e| e.get_ids().to_vec())
                .unwrap_or_default()
        };

        let mut model = self
            .model
            .lock()
            .map_err(|_| EngineError::Candle(
                "the speech model is in an unusable state after an earlier failure; \
                 restart the daemon".to_string(),
            ))?;
        decode::run(
            &mut model,
            &self.tokenizer,
            &self.tokens,
            &mel,
            frames,
            &bias_ids,
            self.language,
            &self.config,
            &self.device,
        )
        .map_err(EngineError::Candle)
    }
}
```

A poisoned `Mutex` is handled rather than unwrapped: `CLAUDE.md` forbids panic
paths in the daemon, and `lock().unwrap()` is one.

- [ ] **Step 5: Run**

This task adds `EngineError::Candle(String)` itself, because nothing here
compiles without it. In `src/stt.rs`, add the variant and its `Display` arm:

```rust
    /// Anything the built-in engine could not do: a model that is not there or
    /// not right, a device, a take of the wrong shape, a decode that failed. The
    /// message is carried whole because each of those already names what to do.
    Candle(String),
```

```rust
            EngineError::Candle(why) => write!(f, "{why}"),
```

Leave `resolve_with`'s `"candle"` arm reporting `NotBuilt` for now — Task 10
replaces it, and this commit keeps a green tree.

```sh
cargo test --lib candle::
cargo test --lib                     # the tree must be green at this commit
cargo clippy --all-targets -- -D warnings && cargo fmt --check
```

- [ ] **Step 6: Commit**

```sh
git add src/stt.rs src/stt/candle.rs src/stt/candle/decode.rs
git commit -m "$(cat <<'MSG'
The candle engine: mel in, greedy decode out, model resident

Greedy at temperature zero with a timestamp-conditioned window advance. Every
decision the loop makes comes from plan, which is tested without weights; what
is left here is an encoder pass, an argmax and the tokenizer. The model is
warmed with one pass over silence at construction so the first take does not
pay for it.

Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>
MSG
)"
```

---

### Task 10: `ModelState`, `Ready`, `check_with` — and `resolve_with` built on it

**Files:**
- Modify: `src/stt.rs` (`locate_configured_model`, `resolve_with`, new
  `check_with` and `Ready`; the existing test at `src/stt.rs:204-219` is
  replaced). The two tests at `src/stt.rs:250-262` need no change — they drive
  `resolve`, whose signature is unchanged — but re-read them after the edit to
  confirm that.

**Interfaces:**
- Consumes: everything from Tasks 4, 5, 7, 9.
- Produces: `stt::{ModelState, Ready, check_with}`, a changed
  `locate_configured_model` and a changed `resolve_with`. Tasks 11, 12 and 13
  consume them, and #16's `http` arm will live in `check_with`.

The split exists because `doctor` must not load 1.6 GB of weights to print six
lines, and `resolve_with` is defined as `check_with` plus construction so the two
cannot give different answers.

- [ ] **Step 1: Write the failing tests**

Replace `the_unbuilt_engines_say_so_and_name_the_one_that_works`
(`src/stt.rs:204-219`) — `candle` is built now, so the test that asserts it is
not must go, not be weakened — and add the rest:

```rust
    #[test]
    fn http_is_the_only_engine_still_unbuilt_here() {
        // candle was in this list until this issue. http leaves it in #16.
        let error = match resolve(&stt("http", &[]), &nowhere()) {
            Err(error) => error,
            Ok(_) => panic!("http must not resolve: it is not built"),
        };
        let message = error.to_string();
        assert!(message.contains("http"), "got {message}");
        assert!(message.contains("#16"), "got {message}");
    }

    #[test]
    fn candle_no_longer_reports_itself_unbuilt() {
        let error = match resolve(&stt("candle", &[]), &nowhere()) {
            Err(error) => error,
            Ok(_) => panic!("with no model at all it cannot resolve"),
        };
        assert!(
            !matches!(error, EngineError::NotBuilt { .. }),
            "candle is built; it fails for want of a model, not for want of code: {error:?}"
        );
        let message = error.to_string();
        assert!(message.contains("large-v3-turbo"), "it must name the model: {message}");
        assert!(message.contains("model --choose"), "it must say what to do: {message}");
    }

    #[test]
    fn the_lookup_answers_for_the_engine_that_is_configured() {
        let models = nowhere();

        // http asks for nothing of ours.
        assert!(matches!(
            locate_configured_model(&stt("http", &[]), &models),
            ModelState::NotUsed
        ));
        // command with no placeholder brings its own.
        assert!(matches!(
            locate_configured_model(&stt("command", &["prog", "{audio}"]), &models),
            ModelState::NotUsed
        ));
        // command with a placeholder asks for the ggml file.
        assert!(matches!(
            locate_configured_model(&stt("command", &["prog", "-m", "{model}"]), &models),
            ModelState::Ggml(Err(_))
        ));
        // candle asks for its own directory — this is what changed.
        assert!(matches!(
            locate_configured_model(&stt("candle", &[]), &models),
            ModelState::Candle(Err(_))
        ));
    }

    #[test]
    fn check_with_approves_a_command_engine_and_keeps_its_model_path() {
        // Ready::Command carries the path because check_with consumed the lookup
        // and resolve_with builds only from what check_with returned.
        let path = std::path::PathBuf::from("/models/ggml-tiny.bin");
        let ready = check_with(
            &stt("command", &["prog", "-m", "{model}"]),
            &ModelState::Ggml(Ok(path.clone())),
        )
        .expect("must approve");
        match ready {
            Ready::Command { model, .. } => assert_eq!(model, Some(path)),
            other => panic!("expected Command, got {other:?}"),
        }
    }

    #[test]
    fn check_with_refuses_an_empty_command_and_an_unknown_engine_as_before() {
        assert!(matches!(
            check_with(&stt("command", &[]), &ModelState::NotUsed),
            Err(EngineError::NotConfigured)
        ));
        assert!(matches!(
            check_with(&stt("vosk", &[]), &ModelState::NotUsed),
            Err(EngineError::Unknown(_))
        ));
    }

    #[test]
    fn check_with_passes_a_store_failure_through_with_its_own_words() {
        let state = ModelState::Candle(Err(candle::store::StoreError::DigestMismatch {
            path: std::path::PathBuf::from("/models/candle/tiny/model.safetensors"),
        }));
        let error = check_with(&stt("candle", &[]), &state).expect_err("must refuse");
        let message = error.to_string();
        assert!(message.contains("digest"), "got {message}");
        assert!(message.contains("model --choose"), "got {message}");
    }

    #[test]
    fn resolve_with_never_disagrees_with_check_with() {
        // The property that makes the split safe. Everything check_with refuses,
        // resolve_with must refuse with the same words — it is defined as
        // check_with plus construction.
        let cases = [
            (stt("command", &[]), ModelState::NotUsed),
            (stt("vosk", &[]), ModelState::NotUsed),
            (stt("http", &[]), ModelState::NotUsed),
            (stt("candle", &[]), ModelState::Candle(Err(
                candle::store::StoreError::Missing {
                    dir: std::path::PathBuf::from("/models/candle/tiny"),
                    identifier: "tiny".to_string(),
                    absent: "model.safetensors",
                },
            ))),
        ];
        for (config, state) in cases {
            let checked = check_with(&config, &state).err().map(|e| e.to_string());
            let resolved = resolve_with(&config, state).err().map(|e| e.to_string());
            assert_eq!(checked, resolved, "for engine {:?}", config.engine);
            assert!(checked.is_some(), "for engine {:?}", config.engine);
        }
    }
```

- [ ] **Step 2: Run to verify they fail**

```sh
cargo test --lib stt:: 2>&1 | tail -30
```

- [ ] **Step 3: Implement in `src/stt.rs`**

`EngineError::Candle` already exists — Task 9 added it. Remove the `"candle"`
arm of `resolve_with`'s match — the one at
`src/stt.rs:111-114` — and replace `locate_configured_model` and `resolve_with`
with:

```rust
/// What the configured engine wants from the models directory, and whether it is
/// there. One value, looked up once, read by both `check_with` and `doctor` — the
/// property PR #20 gave `doctor` when it stopped reading a model twice per run.
#[derive(Debug)]
pub enum ModelState {
    /// Nothing in this configuration would ever look for a model.
    NotUsed,
    /// The ggml file `[stt] command` asks for, unchanged in every respect.
    Ggml(Result<PathBuf, model::ModelError>),
    /// The directory `[stt] engine = "candle"` loads.
    Candle(Result<candle::store::Found, candle::store::StoreError>),
}

/// Everything that can be decided without reading weights.
#[derive(Debug)]
pub enum Ready {
    Command {
        program: String,
        model: Option<PathBuf>,
    },
    Candle {
        device: candle::device::Selection,
        model: candle::store::Found,
    },
}

pub fn locate_configured_model(stt: &Stt, models: &Path) -> ModelState {
    match stt.engine.as_str() {
        "candle" => ModelState::Candle(candle::store::locate(
            models,
            &stt.model,
            catalogue::get(&stt.model),
        )),
        "command" if wants_our_model(&stt.command) => {
            ModelState::Ggml(model::locate(models, &stt.model))
        }
        _ => ModelState::NotUsed,
    }
}

/// Every check the daemon makes before it loads anything, and every check
/// `doctor` makes at all. Takes the lookup by reference so the caller keeps it:
/// `doctor` reports on the model separately from the engine, out of this one value.
pub fn check_with(stt: &Stt, state: &ModelState) -> Result<Ready, EngineError> {
    match stt.engine.as_str() {
        "candle" => match state {
            ModelState::Candle(Ok(found)) => Ok(Ready::Candle {
                device: candle::device::select(),
                model: found.clone(),
            }),
            ModelState::Candle(Err(why)) => Err(EngineError::Candle(why.to_string())),
            // Only reachable if a caller pairs a configuration with a lookup made
            // for a different one, which is a programming error rather than a
            // state a person can reach. Reported, never panicked on.
            other => Err(EngineError::Candle(format!(
                "the model was looked up for a different engine ({other:?}); this is \
                 a defect in the plugin, not in your configuration"
            ))),
        },
        "http" => Err(EngineError::NotBuilt {
            engine: "http".to_string(),
            issue: "issue #16",
        }),
        "command" => {
            if stt.command.is_empty() {
                return Err(EngineError::NotConfigured);
            }
            let model = match state {
                ModelState::Ggml(Ok(path)) => Some(path.clone()),
                ModelState::Ggml(Err(e)) => return Err(EngineError::Model(e.clone())),
                _ => None,
            };
            Ok(Ready::Command {
                program: stt.command[0].clone(),
                model,
            })
        }
        other => Err(EngineError::Unknown(other.to_string())),
    }
}

/// `check_with`, and then build what it approved. Defined this way on purpose:
/// every error `doctor` prints is produced by the code the daemon runs, and the
/// only thing this adds is the load itself.
pub fn resolve_with(
    stt: &Stt,
    state: ModelState,
) -> Result<Box<dyn Engine + Send + Sync>, EngineError> {
    match check_with(stt, &state)? {
        Ready::Command { model, .. } => Ok(Box::new(command::CommandEngine::new(
            stt.command.clone(),
            model,
            stt.language.clone(),
        ))),
        Ready::Candle { device, model } => Ok(Box::new(candle::CandleEngine::new(
            &model,
            &stt.language,
            &device,
        )?)),
    }
}
```

`store::Found` needs `#[derive(Clone)]` for the `found.clone()` above; add it in
`src/stt/candle/store.rs`. `ModelDir` already derives `Clone` (Task 5), and
`ModelError` already derives it too.

`resolve` keeps its signature and becomes one line:

```rust
pub fn resolve(stt: &Stt, models: &Path) -> Result<Box<dyn Engine + Send + Sync>, EngineError> {
    resolve_with(stt, locate_configured_model(stt, models))
}
```

Update `ENGINES`' comment if it says candle is unbuilt; the list itself is
unchanged, since the set was always exactly these three.

- [ ] **Step 4: Run to verify they pass**

```sh
cargo test --lib
cargo clippy --all-targets -- -D warnings && cargo fmt --check
```

`src/doctor.rs` will not compile until Task 11 — it calls the old
`locate_configured_model`. Do Task 11 immediately after this one; if you need a
green tree at this commit, make the two one commit and say so in the message.

- [ ] **Step 5: Commit**

```sh
git add src/stt.rs src/stt/candle/store.rs
git commit -m "$(cat <<'MSG'
Split checking from loading, so doctor stops paying for inference

resolve_with was going to make every `herdr-voice doctor` run load 1.6 GB of
weights and run an encoder pass to print six lines. check_with makes every
weights-free check, and resolve_with is defined as check_with plus
construction, so the two can never give different answers.

Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>
MSG
)"
```

---

### Task 11: `doctor` reports the built-in engine's real state

**Files:**
- Modify: `src/doctor.rs` (`engine_finding_from`, `model_finding_from`,
  `engine_and_model_findings`, and the tests at `:509-521` and `:587-601`)

**Interfaces:**
- Consumes: `stt::{ModelState, Ready, check_with}` (Task 10);
  `candle::device::describe` (Task 7).
- Produces: nothing later tasks consume.

Today `doctor` reports `model … is not used by this configuration` for
`engine = "candle"`, and a test pins it (`src/doctor.rs:509-521`). It is true now
and false the moment this issue lands. AC-10.

- [ ] **Step 1: Write the failing tests**

Replace `the_model_line_is_not_used_when_the_engine_is_not_built`
(`src/doctor.rs:509-521`) — it asserts the opposite of what must now be true —
and add:

```rust
    #[test]
    fn the_model_line_is_not_used_only_for_engines_that_ask_for_nothing() {
        let models = scratch_models("not-used");
        // http asks for nothing of ours; candle no longer belongs in this list.
        let stt = config::Stt { engine: "http".to_string(), ..config::Stt::default() };
        assert_eq!(engine_and_model_findings(&stt, &models).1.state, State::NotUsed);
    }

    #[test]
    fn the_candle_model_line_reports_the_directory_it_wants() {
        let models = scratch_models("candle-missing");
        let stt = config::Stt { engine: "candle".to_string(), ..config::Stt::default() };
        let finding = engine_and_model_findings(&stt, &models).1;
        assert_eq!(finding.state, State::Missing, "got {finding:?}");
        assert!(finding.detail.contains("large-v3-turbo"), "got {}", finding.detail);
        assert!(finding.detail.contains("model --choose"), "got {}", finding.detail);
    }

    #[test]
    fn a_pinned_model_whose_bytes_are_wrong_is_missing_not_ok() {
        // A small fixture under a pinned identifier is exactly the shape of a
        // truncated download, and doctor must say so rather than accept it.
        let models = scratch_models("candle-pinned-wrong");
        write_candle_model(&models, "large-v3-turbo");
        let stt = config::Stt { engine: "candle".to_string(), ..config::Stt::default() };
        let finding = engine_and_model_findings(&stt, &models).1;
        assert_eq!(finding.state, State::Missing, "got {finding:?}");
        assert!(
            finding.detail.contains("1617824864"),
            "it must name the size it wanted: {}",
            finding.detail
        );
    }

    #[test]
    fn an_unpinned_model_is_ok_and_says_which_checks_were_skipped() {
        // The path a model placed by hand takes: usable, and honest about what
        // was not verified.
        let models = scratch_models("candle-unpinned");
        write_candle_model(&models, "homegrown");
        let stt = config::Stt {
            engine: "candle".to_string(),
            model: "homegrown".to_string(),
            ..config::Stt::default()
        };
        let finding = engine_and_model_findings(&stt, &models).1;
        assert_eq!(finding.state, State::Ok, "an unpinned model still works");
        assert!(
            finding.detail.contains("did not check"),
            "it must name the checks it skipped: {}",
            finding.detail
        );
        assert!(finding.detail.contains("homegrown"), "got {}", finding.detail);
    }

    #[test]
    fn the_candle_engine_line_names_the_device() {
        let models = scratch_models("candle-device");
        write_candle_model(&models, "homegrown");
        let stt = config::Stt {
            engine: "candle".to_string(),
            model: "homegrown".to_string(),
            ..config::Stt::default()
        };
        let finding = engine_and_model_findings(&stt, &models).0;
        assert_eq!(finding.state, State::Ok, "got {finding:?}");
        let lowered = finding.detail.to_lowercase();
        assert!(
            lowered.contains("metal") || lowered.contains("cpu"),
            "the engine line must say what it runs on: {}",
            finding.detail
        );
    }

    #[test]
    fn doctor_reads_no_weights() {
        // The property that makes the check/load split worth having. A present,
        // verified model must still cost doctor nothing to report on: if this
        // regresses, `herdr-voice doctor` silently starts taking seconds and
        // loading gigabytes, and nothing else would catch it.
        let models = scratch_models("no-weights");
        write_candle_model(&models, "homegrown");
        let stt = config::Stt {
            engine: "candle".to_string(),
            model: "homegrown".to_string(),
            ..config::Stt::default()
        };
        crate::stt::candle::store::weight_reads::reset();
        let _ = engine_and_model_findings(&stt, &models);
        assert_eq!(
            crate::stt::candle::store::weight_reads::get(),
            0,
            "doctor loaded the weights"
        );
    }

    /// A small, well-formed candle model: three real files, none of them the
    /// gigabyte the catalogue pins. Enough for every check but the digest.
    fn write_candle_model(models: &Path, identifier: &str) {
        let dir = crate::stt::candle::store::directory(models, identifier);
        std::fs::create_dir_all(&dir).unwrap();
        let body = r#"{"a":{"dtype":"F32","shape":[1],"data_offsets":[0,4]}}"#;
        let mut weights = (body.len() as u64).to_le_bytes().to_vec();
        weights.extend_from_slice(body.as_bytes());
        std::fs::write(dir.join("model.safetensors"), weights).unwrap();
        std::fs::write(dir.join("config.json"), br#"{"num_mel_bins":80}"#).unwrap();
        std::fs::write(dir.join("tokenizer.json"), br#"{"version":"1.0"}"#).unwrap();
    }
```

Note what those two tests split on, because it is easy to get backwards: a small
fixture written under a **pinned** identifier is indistinguishable from a
truncated download and must be `Missing`; the same fixture under an **unpinned**
one is `Ok` with the skipped checks named. Never weaken the store so a small
fixture passes a pinned digest — the pinned path is the one AC-5 and AC-6 rest on.

There is no test here for a genuinely `Verified` candle model, because writing
one needs the real 1.6 GB file. `store`'s own tests cover `Verified` with a
fabricated entry (Task 5), which is the seam that exists for exactly this reason;
`doctor`'s rendering of it is one match arm, checked by reading in Task 14 and by
hand in Task 15.

- [ ] **Step 2: Add the read counter to the store**

In `src/stt/candle/store.rs`, beside `locate`, the same idiom
`src/stt/model.rs:23-48` uses:

```rust
/// How many times an engine has been built over the weights, counted only in
/// tests. Thread-local for the reason `model::locate_calls` is: `cargo test` runs
/// on many threads and a process-wide counter would pick up unrelated tests.
/// It pins one property: `doctor` reports on a model without loading it.
#[cfg(test)]
pub(crate) mod weight_reads {
    use std::cell::Cell;
    thread_local! { static COUNT: Cell<usize> = const { Cell::new(0) }; }
    pub(crate) fn reset() { COUNT.with(|c| c.set(0)); }
    pub(crate) fn get() -> usize { COUNT.with(|c| c.get()) }
    /// `pub(crate)`, not `pub(super)`. The precedent in `src/stt/model.rs:23-48`
    /// uses `pub(super)` because `locate` calls it from inside its own module
    /// (`src/stt/model.rs:99`). The caller here is `CandleEngine::new` in
    /// `src/stt/candle.rs`, which is this module's *parent* and so not covered by
    /// `pub(super)` — that would not compile.
    pub(crate) fn increment() { COUNT.with(|c| c.set(c.get() + 1)); }
}
```

Call it at the top of `CandleEngine::new` in `src/stt/candle.rs` — the one place
weights are read, which is what makes the assertion mean "no engine was built":

```rust
    #[cfg(test)]
    store::weight_reads::increment();
```

- [ ] **Step 3: Run to verify they fail**

```sh
cargo test --lib doctor 2>&1 | tail -30
```

- [ ] **Step 4: Implement in `src/doctor.rs`**

`engine_and_model_findings` looks the model up once and derives both lines from
it, as it does today — that does not change. What changes is that it calls
`check_with` instead of `resolve_with`, and that `model_finding_from` matches
`ModelState` instead of `Option<Result<..>>`:

```rust
fn engine_finding_from(stt: &config::Stt, state: &stt::ModelState) -> Finding {
    match stt::check_with(stt, state) {
        Ok(stt::Ready::Candle { device, .. }) => Finding {
            name: "engine",
            state: State::Ok,
            detail: format!(
                "\"candle\" is ready, running {}",
                crate::stt::candle::device::describe(&device)
            ),
        },
        Ok(_) => Finding {
            name: "engine",
            state: State::Ok,
            detail: format!("{:?} is ready", stt.engine),
        },
        Err(e) => Finding {
            name: "engine",
            state: State::Missing,
            detail: e.to_string(),
        },
    }
}

fn model_finding_from(stt: &config::Stt, models: &Path, state: &stt::ModelState) -> Finding {
    match state {
        stt::ModelState::NotUsed => Finding {
            name: "model",
            state: State::NotUsed,
            detail: format!(
                "[stt] model ({}) is not used by this configuration; nothing in it \
                 asks for one. It would be looked for in {}",
                stt.model,
                models.display()
            ),
        },
        stt::ModelState::Ggml(Ok(path)) => Finding {
            name: "model",
            state: State::Ok,
            detail: format!("{}", path.display()),
        },
        stt::ModelState::Candle(Ok(found)) => match found {
            crate::stt::candle::store::Found::Verified(dir) => Finding {
                name: "model",
                state: State::Ok,
                detail: format!("{}", dir.dir.display()),
            },
            crate::stt::candle::store::Found::Unpinned { dir, identifier } => Finding {
                name: "model",
                state: State::Ok,
                detail: format!(
                    "{} — this plugin does not offer {identifier}, so it did not check \
                     its size or its digest. Run `herdr-voice model` for the models it \
                     does offer",
                    dir.dir.display()
                ),
            },
        },
        stt::ModelState::Ggml(Err(e)) => Finding {
            name: "model",
            state: State::Missing,
            detail: e.to_string(),
        },
        stt::ModelState::Candle(Err(e)) => Finding {
            name: "model",
            state: State::Missing,
            detail: e.to_string(),
        },
    }
}

fn engine_and_model_findings(stt: &config::Stt, models: &Path) -> (Finding, Finding) {
    let state = stt::locate_configured_model(stt, models);
    let engine = engine_finding_from(stt, &state);
    let model_line = model_finding_from(stt, models, &state);
    (engine, model_line)
}
```

Taking `&ModelState` rather than cloning is what removes the `model.clone()` at
`src/doctor.rs:254` and keeps the one-lookup property literal rather than nearly
true.

Check `the_model_is_located_once_per_doctor_run` (`src/doctor.rs:587-601`) still
holds — it should, unchanged, because the lookup is still made exactly once.

- [ ] **Step 5: Run**

```sh
cargo test --lib
cargo clippy --all-targets -- -D warnings && cargo fmt --check
```

- [ ] **Step 6: Commit**

```sh
git add src/doctor.rs src/stt/candle/store.rs src/stt/candle.rs
git commit -m "$(cat <<'MSG'
doctor reports the built-in engine's real model state and its device

"model not used by this configuration" was true for candle until now and is
false the moment the engine exists. The engine line names the device it will
run on, and a new test asserts that reporting on a present model still reads
zero bytes of weights.

Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>
MSG
)"
```

---

### Task 12: `herdr-voice model` and `--choose`

**Files:**
- Create: `src/chooser.rs`
- Modify: `src/main.rs` (the `Model` arm, `IMPLEMENTED`, `USAGE`, and the tests
  at `:211-215` and `:228-233`)

**Interfaces:**
- Consumes: `catalogue::{MODELS, Entry, weights}` (Task 4);
  `store::{glance, Glance}` (Task 5); `fetch::{model, Progress}` (Task 6);
  `config` (existing).
- Produces: `chooser::run(choosing: bool) -> u8`, the only item `main.rs` calls,
  plus the module-private `list`, `pick`, `human` and `set_model_key` its tests
  reach. Nothing later consumes any of them.

The manifest already declares the pane. `--choose` is read by nothing today, and
`model` exits 69.

- [ ] **Step 1: Write the failing tests**

The configuration edit is the part with edge cases; the listing is the part a
person reads. Both are pure functions over strings, so both are ordinary tests.

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_listing_shows_every_model_with_a_size_before_anything_is_downloaded() {
        let text = list(&std::path::PathBuf::from("/nowhere-at-all"), "large-v3-turbo");
        for entry in crate::stt::catalogue::MODELS.iter() {
            assert!(text.contains(entry.identifier), "{} is missing: {text}", entry.identifier);
        }
        assert!(text.contains("151 MB"), "tiny's size: {text}");
        assert!(text.contains("1.62 GB"), "the default's size: {text}");
        assert!(text.contains("not installed"), "it must say what is there: {text}");
    }

    #[test]
    fn the_configured_model_is_marked_in_the_listing() {
        let text = list(&std::path::PathBuf::from("/nowhere-at-all"), "small");
        let marked: Vec<&str> = text.lines().filter(|l| l.contains("small")).collect();
        assert_eq!(marked.len(), 1, "got {marked:?}");
        assert!(marked[0].contains("current"), "got {}", marked[0]);
    }

    #[test]
    fn sizes_are_rendered_the_way_a_person_reads_them() {
        assert_eq!(human(151_061_672), "151 MB");
        assert_eq!(human(1_617_824_864), "1.62 GB");
        assert_eq!(human(3_087_130_976), "3.09 GB");
    }

    #[test]
    fn a_choice_outside_the_list_is_refused_rather_than_guessed() {
        for answer in ["", "0", "7", "large", "-1", "2x"] {
            assert!(
                pick(answer).is_none(),
                "{answer:?} must not resolve to a model"
            );
        }
        assert_eq!(pick("1").map(|e| e.identifier), Some("tiny"));
        assert_eq!(pick(" 4 ").map(|e| e.identifier), Some("large-v3-turbo"));
    }

    #[test]
    fn a_file_with_no_stt_table_gains_one() {
        let out = set_model_key("[audio]\ninput = \"\"\n", "tiny");
        assert!(out.contains("[stt]"), "got {out}");
        assert!(out.contains("model = \"tiny\""), "got {out}");
        assert!(out.contains("[audio]"), "it must not lose what was there: {out}");
        // And it must still parse.
        let parsed: toml::Value = toml::from_str(&out).expect("must remain valid TOML");
        assert_eq!(parsed["stt"]["model"].as_str(), Some("tiny"));
    }

    #[test]
    fn an_existing_model_line_is_replaced_and_the_comments_around_it_survive() {
        let before = "\
# my configuration
[stt]
# which model to use
model = \"large-v3-turbo\"
language = \"ru\"

[ui]
toasts = false
";
        let out = set_model_key(before, "small");
        assert!(out.contains("model = \"small\""), "got {out}");
        assert!(!out.contains("large-v3-turbo"), "the old value must go: {out}");
        assert!(out.contains("# which model to use"), "comments must survive: {out}");
        assert!(out.contains("language = \"ru\""), "other keys must survive: {out}");
        assert!(out.contains("toasts = false"), "other tables must survive: {out}");
        let parsed: toml::Value = toml::from_str(&out).expect("must remain valid TOML");
        assert_eq!(parsed["stt"]["model"].as_str(), Some("small"));
        assert_eq!(parsed["ui"]["toasts"].as_bool(), Some(false));
    }

    #[test]
    fn a_stt_table_with_no_model_key_gains_one_inside_itself() {
        let out = set_model_key("[stt]\nlanguage = \"ru\"\n\n[ui]\ntoasts = true\n", "base");
        let parsed: toml::Value = toml::from_str(&out).expect("must remain valid TOML");
        assert_eq!(parsed["stt"]["model"].as_str(), Some("base"));
        assert_eq!(parsed["stt"]["language"].as_str(), Some("ru"));
        assert_eq!(parsed["ui"]["toasts"].as_bool(), Some(true));
    }

    #[test]
    fn a_model_key_in_another_table_is_not_the_one_that_changes() {
        // [rewrite] has a model key too. Editing the wrong one would silently
        // repoint the rewrite engine.
        let before = "[rewrite]\nmodel = \"haiku\"\n\n[stt]\nmodel = \"tiny\"\n";
        let out = set_model_key(before, "small");
        let parsed: toml::Value = toml::from_str(&out).expect("must remain valid TOML");
        assert_eq!(parsed["rewrite"]["model"].as_str(), Some("haiku"), "got {out}");
        assert_eq!(parsed["stt"]["model"].as_str(), Some("small"), "got {out}");
    }

    #[test]
    fn an_empty_file_becomes_a_valid_one() {
        let out = set_model_key("", "tiny");
        let parsed: toml::Value = toml::from_str(&out).expect("must remain valid TOML");
        assert_eq!(parsed["stt"]["model"].as_str(), Some("tiny"));
    }
}
```

- [ ] **Step 2: Run to verify they fail**

```sh
cargo test --lib chooser 2>&1 | tail -20
```

- [ ] **Step 3: Implement `src/chooser.rs`**

```rust
//! Choosing a speech model, and downloading the one chosen.
//!
//! Two entry points because two questions are asked: `herdr-voice model` says
//! what exists, and `--choose` spends the gigabytes. The configuration edit is
//! line-oriented so comments and every other key survive — the `toml` crate in
//! the tree parses and does not preserve formatting, and `toml_edit` is a new
//! dependency for one line of text. See `tasks/15/DESIGN_15.md`, section 5.

use std::io::Write;
use std::path::Path;

use crate::stt::candle::store;
use crate::stt::catalogue::{self, Entry};
use crate::stt::fetch;

/// A size the way a person reads it, not the way a computer stores it.
fn human(bytes: u64) -> String {
    const MB: f64 = 1_000_000.0;
    const GB: f64 = 1_000_000_000.0;
    let b = bytes as f64;
    if b >= GB {
        format!("{:.2} GB", b / GB)
    } else {
        format!("{:.0} MB", b / MB)
    }
}

/// The catalogue, with each model's size and whether it is already there.
///
/// Presence is judged by `store::glance`, not `store::locate`: listing six models
/// must not hash six multi-gigabyte files. A person asking what is installed is
/// asking a question about names and sizes, and waiting a minute for the answer
/// would be absurd. The full check happens where it matters — before a model is
/// loaded.
pub fn list(models: &Path, configured: &str) -> String {
    let mut out = String::from("Speech models this plugin can install:\n\n");
    for (i, entry) in catalogue::MODELS.iter().enumerate() {
        let size = human(catalogue::weights(entry).bytes);
        let present = match store::glance(models, entry.identifier, Some(entry)) {
            store::Glance::Whole => "installed",
            store::Glance::Absent => "not installed",
            store::Glance::WrongSize => "installed, but the wrong size",
        };
        let marker = if entry.identifier == configured { "  (current)" } else { "" };
        out.push_str(&format!(
            "  {}. {:<16} {:>8}  {} mel bins  — {present}{marker}\n",
            i + 1,
            entry.identifier,
            size,
            entry.mel_bins
        ));
    }
    out.push_str("\nThey live in ");
    out.push_str(&models.join("candle").display().to_string());
    out.push('\n');
    out
}

/// The entry a typed answer names, or `None` — never a guess.
fn pick(answer: &str) -> Option<&'static Entry> {
    let n: usize = answer.trim().parse().ok()?;
    if n == 0 {
        return None;
    }
    catalogue::MODELS.get(n - 1)
}

/// A progress line that overwrites itself, so a 3 GB download is one line.
struct Line {
    name: String,
    total: u64,
}

impl fetch::Progress for Line {
    fn file(&mut self, name: &str, total: u64) {
        self.name = name.to_string();
        self.total = total;
    }
    fn bytes(&mut self, done: u64) {
        let percent = if self.total == 0 { 0 } else { done * 100 / self.total };
        print!("\r  {} {percent:>3}%  ", self.name);
        let _ = std::io::stdout().flush();
    }
    fn done(&mut self, name: &str) {
        println!("\r  {name} done            ");
    }
}

/// `[stt] model = "<identifier>"`, put into a configuration file's text without
/// disturbing anything else in it.
pub fn set_model_key(existing: &str, identifier: &str) -> String {
    let line = format!("model = \"{identifier}\"");
    let mut out: Vec<String> = Vec::new();
    let mut in_stt = false;
    let mut wrote = false;
    let mut saw_stt = false;

    for text in existing.lines() {
        let trimmed = text.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            // Leaving [stt] without having found a model key: add one at its end.
            if in_stt && !wrote {
                out.push(line.clone());
                wrote = true;
            }
            in_stt = trimmed == "[stt]";
            saw_stt |= in_stt;
            out.push(text.to_string());
            continue;
        }
        // Only a `model` key inside [stt]. [rewrite] has one too.
        if in_stt && !wrote && trimmed.starts_with("model") {
            if let Some(rest) = trimmed.strip_prefix("model") {
                if rest.trim_start().starts_with('=') {
                    out.push(line.clone());
                    wrote = true;
                    continue;
                }
            }
        }
        out.push(text.to_string());
    }

    if in_stt && !wrote {
        out.push(line.clone());
        wrote = true;
    }
    if !saw_stt {
        if !out.is_empty() && !out.last().is_some_and(|l| l.trim().is_empty()) {
            out.push(String::new());
        }
        out.push("[stt]".to_string());
        out.push(line);
    } else if !wrote {
        out.push(line);
    }

    let mut text = out.join("\n");
    text.push('\n');
    text
}
```

And the one entry point `main.rs` calls, which resolves the directories and turns
every outcome into an exit code:

```rust
/// `herdr-voice model`, and `--choose`. Returns the process's exit code.
///
/// The models directory is resolved here rather than passed in, the same way
/// `doctor::run` does it (`src/doctor.rs:345-347`), because this is the
/// outermost layer: `list`, `pick` and `set_model_key` all take what they need
/// and are testable without an environment.
pub fn run(choosing: bool) -> u8 {
    let models = match crate::transport::state_directory(&crate::transport::Vars::from_env()) {
        Some(state) => state.join("models"),
        None => {
            eprintln!(
                "cannot tell where models live: neither HERDR_PLUGIN_STATE_DIR nor a                  home directory is set, so there is nowhere to put one. Set                  HERDR_PLUGIN_STATE_DIR and try again"
            );
            return 1;
        }
    };
    let vars = crate::config::Vars::from_env();
    let loaded = crate::config::load(crate::config::directory(&vars).as_deref());

    print!("{}", list(&models, &loaded.config.stt.model));
    if !choosing {
        return 0;
    }

    println!("\nType the number of the model to install, then Enter.");
    let mut answer = String::new();
    if std::io::stdin().read_line(&mut answer).is_err() {
        eprintln!("nothing was read from the terminal; run `herdr-voice model --choose` again");
        return 1;
    }
    let entry = match pick(&answer) {
        Some(entry) => entry,
        None => {
            eprintln!(
                "{:?} is not one of the numbers above. Run `herdr-voice model --choose` \
                 again and type a number between 1 and {}",
                answer.trim(),
                catalogue::MODELS.len()
            );
            return 1;
        }
    };

    println!("Installing {} ({})", entry.identifier, human(catalogue::weights(entry).bytes));
    let mut line = Line { name: String::new(), total: 0 };
    if let Err(why) = fetch::model(entry.identifier, &models, &mut line) {
        eprintln!("{why}");
        return 1;
    }

    // Verify what was just downloaded with the real check, not the glance the
    // listing uses: this is the moment a bad download must be caught.
    if let Err(why) = store::locate(&models, entry.identifier, Some(entry)) {
        eprintln!("{why}");
        return 1;
    }

    if loaded.config.stt.model == entry.identifier {
        println!("{} is installed and already configured.", entry.identifier);
        return 0;
    }
    match write_model_key(&vars, entry.identifier) {
        Ok(path) => {
            println!("{} is installed, and {} now names it.", entry.identifier, path.display());
            println!("The daemon still holds the previous model. Restart it to use this one.");
            0
        }
        Err(why) => {
            // The model is on disk and good; only the configuration edit failed,
            // so the person needs one line, not another three gigabytes.
            eprintln!("{} is installed, but the configuration could not be written: {why}", entry.identifier);
            eprintln!("Add this to [stt] in your configuration file by hand:");
            eprintln!("  model = \"{}\"", entry.identifier);
            1
        }
    }
}

/// Read the configuration file, put `[stt] model` in it, write it back. Returns
/// the file written, for the message.
fn write_model_key(
    vars: &crate::config::Vars,
    identifier: &str,
) -> Result<std::path::PathBuf, String> {
    let dir = crate::config::directory(vars)
        .ok_or_else(|| "there is no configuration directory to write to".to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join(crate::config::FILE_NAME);
    // An absent configuration file is a valid state, so this is a create, not a
    // failure (`CLAUDE.md`, "Rules for the code").
    let existing = std::fs::read_to_string(&path).unwrap_or_default();
    std::fs::write(&path, set_model_key(&existing, identifier)).map_err(|e| e.to_string())?;
    Ok(path)
}
```

`run` is the only function in this module that touches the environment; `list`,
`pick`, `human` and `set_model_key` take everything they need as arguments, which
is why all four are tested above and `run` is not. `run` is exercised by hand in
Task 15.

Check `config::directory`'s real signature before writing this
(`src/config.rs`) — it is what `doctor` and the daemon already call, and this
must call it the same way rather than computing a path of its own.

- [ ] **Step 4: Wire it into `src/main.rs`**

Move `Command::Model` out of the exit-69 arm into its own:

```rust
        Command::Model => {
            let choosing = args.iter().any(|a| a == "--choose");
            ExitCode::from(chooser::run(choosing))
        }
```

and remove `Command::Model` from the `other @ (Command::Ptt | …)` pattern.
Add `mod chooser;` beside the other modules. Add `model` to `IMPLEMENTED`
(`src/main.rs:104`) and a line to `USAGE`:

```
  herdr-voice model      list the speech models, or --choose to install one
```

Both pinning tests then need their lists corrected — `model` moves from the
not-implemented list to the implemented one in
`the_commands_this_issue_implements_are_not_in_the_unimplemented_arm`
(`src/main.rs:211-215`), and `the_usage_text_names_every_implemented_command`
(`src/main.rs:228-233`) passes once `USAGE` names it. Do not delete either test.

- [ ] **Step 5: Run**

```sh
cargo test --lib
python3 scripts/check_manifest.py
cargo clippy --all-targets -- -D warnings && cargo fmt --check
cargo run -- model            # the listing, by hand
```

- [ ] **Step 6: Commit**

```sh
git add src/chooser.rs src/main.rs
git commit -m "$(cat <<'MSG'
Choose a speech model from a list with sizes, and install it

herdr-voice model lists what exists; --choose spends the gigabytes. The
configuration edit is line-oriented so comments and every other key survive,
and it changes [stt] model rather than the model key [rewrite] also has.

Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>
MSG
)"
```

---

### Task 13: The daemon says which device, and the documents stop saying the engine is absent

**Files:**
- Modify: `src/daemon.rs` (near `stt::resolve` at `:517`)
- Modify: `README.md:47-49`
- Modify: `docs/design.md` (section 4's "Transcription", and section 7's `[stt]`)
- Modify: `docs/decisions.md`

**Interfaces:** consumes Tasks 7 and 10; produces nothing.

- [ ] **Step 1: Report the device at daemon start**

`stt::resolve` at `src/daemon.rs:517` already reports its failure to standard
error. Add the success case for candle, so a person who is on the CPU learns it
at start rather than after waiting a minute for a minute of speech:

```rust
    let state = stt::locate_configured_model(&loaded.config.stt, &models);
    if let Ok(stt::Ready::Candle { device, .. }) = stt::check_with(&loaded.config.stt, &state) {
        eprintln!(
            "recognition: the built-in engine, {}",
            crate::stt::candle::device::describe(&device)
        );
    }
    let recognition: Recognition = stt::resolve_with(&loaded.config.stt, state).map_err(|e| {
        eprintln!("recognition unavailable: {e}");
        e.to_string()
    });
```

This calls `check_with` once more than strictly needed. That is deliberate and
cheap — `check_with` reads no weights — and it keeps the reporting line out of
`resolve_with`, which the daemon and `doctor` share.

- [ ] **Step 2: Correct `README.md:47-49`**

It currently says recognition needs an external engine until #15. Replace with a
statement of what is true: the built-in engine runs a Whisper model locally with
no external program, the model is chosen and installed with
`herdr-voice model --choose`, and `[stt] engine` can still be `command` for an
external transcriber. A public repository must not promise what it does not
contain, and it must not deny what it does.

- [ ] **Step 3: Correct `docs/design.md`**

Section 4's "Transcription" already describes `candle` as the built-in engine
chosen on first run from a list with sizes — that is now true and needs no
change. Section 7's `[stt]` block gains nothing: no configuration key was added
by this issue. Add one sentence to section 4 naming where models live
(`<state>/models/candle/<identifier>/`) and that a download is verified against
a pinned size and digest.

Leave section 9's open question 2 — candle versus whisper.cpp — standing. It
closes in #2, not here. Add to it the number this issue measured, so #2 starts
from something: 5.1 s for a 66-second take against `whisper-cli`'s 1.65 s for 70
seconds, both with `large-v3-turbo` on Metal.

- [ ] **Step 4: Append the remaining decision rows to `docs/decisions.md`**

Dated `2026-09-09, #15`. Task 1 already added four; these are the rest.

| Decision | Basis |
|---|---|
| A candle model is a directory of three files under `<state>/models/candle/<identifier>/`, checked by exact name, exact byte count, safetensors header and pinned SHA-256; the `ggml` store is untouched | candle reads safetensors and a Whisper model is three files, so one name template cannot serve both formats without loosening one of them. Every check the `ggml` contract makes is made here and two of them stop being heuristics, because a pinned size and digest exist here and did not there |
| The mel filterbank is computed from the specification; candle's reference blobs are kept as test fixtures and compiled into nothing | The same reason SHA-256 was written from the specification and checked against the published vectors. The alternative is 164 KB of opaque floats in the binary that nothing here can explain. Measured worst difference against the reference: 1.86 × 10⁻⁹ for 80 bins, 3.73 × 10⁻⁹ for 128 |
| Greedy decoding at temperature zero, with no beam search and no temperature fallback | The ticket allows greedy, and recognition is not the slow stage. What the fallback buys is a retry when the output degenerates into repetition; without it that repetition is delivered, bounded by the text context limit. Recorded rather than hidden |
| A window with less than one second of real audio is not decoded | Found by running it: a final window of 2 real frames and 2 998 of zero padding returned "[Music]" — confident text transcribed from silence |
| The model loads eagerly at daemon start, followed by one encoder pass over 30 seconds of silence | `docs/design.md` section 2 already says the daemon keeps the model resident to take the load off every dictation. The warm-up costs the daemon under a second at start and is the difference between the first take of a session behaving like the rest and behaving seconds worse |
| Checking and loading are separated: `check_with` makes every weights-free check, and `resolve_with` is `check_with` plus construction | Without the split every `herdr-voice doctor` run would load 1.6 GB and run an encoder pass to print six lines. Defining one in terms of the other is what stops the split from becoming two answers to one question — the property PR #20 gave `doctor` |
| An unavailable Metal device falls back to the CPU and is reported, never refused | Measured at ten times slower — 52.3 s against 5.1 s for a 66-second take with the default model — which is a different product rather than a slower one, so it is said out loud at daemon start and on `doctor`'s engine line. But a slow transcript beats no transcript, and every Linux and Windows build takes this path |
| The chooser edits `[stt] model` in the configuration file, line by line, rather than printing a snippet | "The model is one the person chose" has to survive a restart. `setup` prints a snippet because keybindings live in herdr's configuration, which is not this plugin's to write; this file is. Line-oriented because `toml` in the tree does not preserve comments and `toml_edit` is a new dependency for one line |

- [ ] **Step 5: Run everything**

```sh
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt --check
python3 scripts/check_manifest.py
```

- [ ] **Step 6: Commit**

```sh
git add src/daemon.rs README.md docs/design.md docs/decisions.md
git commit -m "$(cat <<'MSG'
Say which device the engine runs on, and stop documenting an absent engine

README said recognition needs an external engine until #15. It does not any
more, and a public repository must not deny what it contains any more than it
may promise what it does not.

Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>
MSG
)"
```

---

### Task 14: Gate S4 — review the diff by mutation, before any pull request

**Files:** none changed unless the review finds something.

S4 closes on a review of the diff, not on a pull request (`CLAUDE.md`). The gate
exists because two defects went through S4 unreviewed on 2026-08-25. The
technique and the depth expected are in `tasks/36/RUN_36.md`, "Gate S4".

- [ ] **Step 1: Confirm the four gates, repeatedly**

```sh
cargo test 2>&1 | tail -5
for i in 1 2 3 4 5; do cargo test 2>&1 | tail -2; done   # the fetch tests bind sockets
cargo clippy --all-targets -- -D warnings
cargo fmt --check
python3 scripts/check_manifest.py
```

Flakes are findings, not noise. The `fetch` tests accept connections on
loopback and join threads; if any run differs, chase it before going further.

- [ ] **Step 2: Mutation-test the load-bearing behaviours**

For each, break it deliberately, confirm a test goes red, and revert. A behaviour
whose mutation leaves the suite green is untested, whatever its coverage says.

| Break this | A test must go red |
|---|---|
| `store::locate` skips the digest check | `the_right_size_and_the_wrong_bytes_are_caught_by_the_digest` |
| `store::locate` checks the digest before the size | `a_truncated_download_is_named_by_its_size_not_by_its_digest` |
| `store::locate` returns `Verified` when `expected` is `None` | `a_model_nobody_pinned_is_unpinned_and_says_which_checks_were_skipped` |
| `chooser::list` calls `locate` instead of `glance` | nothing — this is a performance defect, not a correctness one. Check it by reading, and by timing `herdr-voice model` with a real multi-gigabyte model installed (Task 15) |
| `fetch::one` renames the `.part` before verifying | `a_short_transfer_names_the_size_and_leaves_nothing_behind`, `the_right_length_and_the_wrong_bytes_are_caught_by_the_digest` |
| `fetch::one` leaves the `.part` on failure | both of the above, on their `.part` assertions |
| `plan::next_window` drops the `MIN_REAL_FRAMES` guard | `a_tail_that_is_almost_all_padding_is_not_a_window` |
| `plan::advance` always returns `N_FRAMES` | `a_full_window_advances_to_its_last_timestamp` |
| `plan::advance` trusts a zero or backward timestamp | `a_zero_or_backward_timestamp_cannot_stall_the_loop` |
| `plan::prompt_tokens` keeps the head of the bias instead of the tail | `the_end_of_the_bias_is_what_survives_the_cut` |
| `plan::prompt_tokens` emits a bare marker for empty bias | `an_empty_bias_is_no_prompt_at_all` |
| `mel::filters` returns zeros | `the_computed_filterbank_matches_the_reference_one`, `every_filter_has_weight_somewhere` |
| `mel::hz_to_mel` becomes the identity | `the_scale_is_not_linear` |
| `wav::read` ignores the channel count | `stereo_and_the_wrong_width_are_refused_by_name_not_converted` |
| `check_with` reports the candle model as ready when the store failed | `check_with_passes_a_store_failure_through_with_its_own_words` |
| `resolve_with` stops calling `check_with` and re-derives its own errors | `resolve_with_never_disagrees_with_check_with` |
| `doctor` calls `resolve_with` instead of `check_with` | `doctor_reads_no_weights` |
| `set_model_key` edits the first `model` key in the file | `a_model_key_in_another_table_is_not_the_one_that_changes` |
| `chooser::pick` falls back to the default on an unparsable answer | `a_choice_outside_the_list_is_refused_rather_than_guessed` |
| `device::describe` drops the cost from the CPU line | `the_cpu_says_why_and_what_it_costs_and_what_to_do` |

- [ ] **Step 3: Read the diff for the things no test can assert**

```sh
git diff 851821c --stat
git diff 851821c
```

Check by reading, and write down the answer to each rather than only the ones
that fail:

- **No panic path in anything new.** Search the diff for `unwrap()`, `expect(`,
  `panic!`, `unreachable!`, indexing with `[` on anything not provably in range,
  and integer arithmetic that can overflow. The two deliberate ones are
  `catalogue::weights`' `expect` on a compile-time constant a test pins, and
  `device::device_for`'s `unwrap_or`. Anything else is a finding.
- **No leak.** No absolute home path, no employer or client name, in code,
  comments, test fixtures or commit messages. The pre-commit hook and
  `.github/workflows/leak-gate.yml` both run, but read the diff too — a fixture
  path is easy to miss.
- **Nothing sensitive is logged.** The transcript, the bias string and the take's
  path must not reach any new log line or error message beyond what already
  carried them.
- **Every pre-existing test that changed was updated meaningfully, not weakened
  to compile.** Specifically `the_unbuilt_engines_say_so_and_name_the_one_that_works`
  (replaced, because candle is built), `the_model_line_is_not_used_when_the_engine_is_not_built`
  (replaced, because the model is used now), and the two in `src/main.rs`. For
  each, confirm the new test asserts the new truth rather than asserting less.
- **Every failure names what to do next.** Walk each `Display` arm added by this
  issue — `ReadError`, `StoreError`, `FetchError`, `EngineError::Candle`'s
  messages — and confirm each ends somewhere actionable.
- **`docs/design.md`, `README.md` and `docs/decisions.md` say what is true now**,
  and no artifact carries a stray tool marker: `git diff 851821c | grep -nE
  '</?(old|new)_string>'` must find nothing. #36 committed one of those into
  `docs/evidence.md` unnoticed through two gates.

- [ ] **Step 4: Record the verdict in `tasks/15/RUN_15.md`**

A `gate:` block with `stage: S4`, the artifact being the diff since `851821c` at
its commit, and the verdict. `QUESTIONS` sends findings back and is followed by
a second block when they are fixed — do not overwrite the first.

---

### Task 15: Gate S5 — verify by running it, and write down what happened

**Files:**
- Modify: `docs/evidence.md`
- Modify: `tasks/15/RUN_15.md`

S5 is a step that can fail, not a request. A negative result, recorded, passes
it; a claim with no command and no output beside it does not.

- [ ] **Step 1: Run the suite fresh, with no model and no network**

```sh
cargo test 2>&1 | tail -5
```

Then prove the two conditions rather than assuming them. The models directory
must be absent for the run, and the network unreachable:

```sh
HERDR_PLUGIN_STATE_DIR=$(mktemp -d) cargo test 2>&1 | tail -3
```

For the network, run the suite with outbound access blocked by whatever this
machine offers, and say in the evidence entry which method was used. If no
method is available, say that instead — an unverified claim marked as
unverified is the honest outcome, and `CLAUDE.md` asks for exactly that rather
than a claim with nothing behind it.

- [ ] **Step 2: Install a model and transcribe a real take, by hand (AC-14)**

```sh
cargo build --release
./target/release/herdr-voice model
./target/release/herdr-voice model --choose      # answer 1 (tiny) first: it is 151 MB
./target/release/herdr-voice doctor
```

Then a real spoken take through the daemon and a herdr pane, the way
`docs/evidence.md`'s "Recognition, by hand on macOS" records the `whisper-cli`
run. Record the platform, the model, the take's length, and how long
recognition took.

Then repeat with `large-v3-turbo`, because it is the default and the one every
other number in `docs/evidence.md` was made with.

- [ ] **Step 3: Exercise the failure paths, by hand**

Each of these is an AC-8 claim, and each is one command:

```sh
# a truncated model
truncate -s 1000000 "$STATE/models/candle/tiny/model.safetensors"
./target/release/herdr-voice doctor

# a file that is not a model
printf '<html>' > "$STATE/models/candle/tiny/model.safetensors"
./target/release/herdr-voice doctor

# no model at all
rm -rf "$STATE/models/candle"
./target/release/herdr-voice doctor

# a model identifier nobody pinned
# ([stt] model = "homegrown", with real files placed by hand)
```

Each must name what to do next, and none may panic. Record what each printed.

- [ ] **Step 4: Write the evidence section**

A new `## The built-in engine, by hand on macOS` in `docs/evidence.md`, with the
platform, the versions, the measurements, and — separately and plainly — what is
still not measured. At least these belong in it, because the design rests on
them and #2 needs them:

- Recognition time for the same take on the built-in engine and, if it is still
  installed, `whisper-cli`, so the comparison #2 will make has a first data
  point. The design measured 5.1 s against 1.65 s in a probe; a measurement
  through the real plugin is what this section records.
- The first-take cost with and without the warm-up pass, if it can be separated.
- Whether the CPU fallback was exercised on this machine at all, and if not,
  that it was not.

- [ ] **Step 5: Record the S5 verdict in `tasks/15/RUN_15.md`** and commit

```sh
git add docs/evidence.md tasks/15/RUN_15.md
git commit -m "$(cat <<'MSG'
S5: what the built-in engine actually did, on macOS, by hand

Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>
MSG
)"
```

- [ ] **Step 6: Only now, the pull request**

All four gates green, S4's review clean, S5 recorded. Title it clearly and put
`Closes #15` in the body, with the checklist from the pull request template.

```sh
git push -u origin feat/15-candle-engine
gh pr create --title "Recognition: the built-in candle engine, and choosing a model on first run" --body "..."
```

Expect a rebase onto #16 or a rebase of #16 onto this, whichever merges second:
`src/stt.rs`'s engine match — now `check_with` — and `src/doctor.rs`. See
`tasks/36/RUN_36.md`, "Rebased onto #26's merge".

## Coverage against `AC_15.md`

| AC | Where |
|---|---|
| AC-1 no child process, no network on the take path | Tasks 9, 10; read in Task 14 Step 3 |
| AC-2 the existing trait, no new branch in `daemon::transcribe` | Task 9's `impl Engine`; Task 13 touches only the start-up report |
| AC-3 reads the 16 kHz mono WAV, refuses other shapes by name | Task 2, Task 9's `read_take` |
| AC-4 a chooser listing models with sizes, `model` off exit 69 | Task 12 |
| AC-5 verified before load, none of the four `ggml` checks weakened | Task 5; `cargo test --lib stt::model` unchanged in Task 5 Step 5 |
| AC-6 an interrupted download is never mistaken for a model | Task 6 (`.part`), Task 5 (`a_leftover_part_file_is_not_a_model`) |
| AC-7 `[stt] model` and `[stt] language`, `auto` included | Task 10 (`locate_configured_model`), Task 9 (`language_token`) |
| AC-8 every failure names what to do next, nothing panics | Tasks 2, 5, 6, 9; Task 14 Step 3; Task 15 Step 3 |
| AC-9 the suite passes with no model, no network, no GPU | Every task's tests; proved in Task 15 Step 1 |
| AC-10 `doctor` reports the real model state | Task 11 |
| AC-11 the `candle` arm no longer reports `NotBuilt` | Task 10 |
| AC-12 `README.md` corrected, design section 9 question 2 left standing | Task 13 |
| AC-13 the decisions taken without the owner are recorded four-part | Tasks 1 and 13 |
| AC-14 one real take on macOS, in `docs/evidence.md` | Task 15 |
