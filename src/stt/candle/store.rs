//! What a candle model on disk is, and whether this one is.
//!
//! The `ggml` store in `src/stt/model.rs` is untouched and still serves
//! `[stt] command`. This makes the same four checks — the exact name, the size,
//! the format, the digest — with two of them exact rather than heuristic,
//! because a pinned size and digest exist here and did not there. See
//! `tasks/15/DESIGN_15.md`, section 2.

// No caller until the engine, `check_with`, `doctor` and the chooser land
// (`tasks/15/PLAN_15.md`, Tasks 9 to 12). Task 13 sweeps these out.
#![allow(dead_code)]

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
#[derive(Debug, Clone)]
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
    Missing {
        dir: PathBuf,
        identifier: String,
        absent: &'static str,
    },
    WrongSize {
        path: PathBuf,
        expected: u64,
        found: u64,
    },
    NotSafetensors {
        path: PathBuf,
        why: String,
    },
    Unparsable {
        path: PathBuf,
        why: String,
    },
    DigestMismatch {
        path: PathBuf,
    },
    Unreadable {
        path: PathBuf,
        why: String,
    },
}

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StoreError::Missing {
                dir,
                identifier,
                absent,
            } => write!(
                f,
                "no {absent} for the {identifier} model in {}; run \
                 `herdr-voice model --choose` to download one, or set [stt] model \
                 to a model that is there",
                dir.display()
            ),
            StoreError::WrongSize {
                path,
                expected,
                found,
            } => write!(
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

/// How many times an engine has been built over the weights, counted only in
/// tests. Thread-local for the reason `model::locate_calls` is: `cargo test` runs
/// on many threads and a process-wide counter would pick up unrelated tests.
/// It pins one property: `doctor` reports on a model without loading it.
#[cfg(test)]
pub(crate) mod weight_reads {
    use std::cell::Cell;

    thread_local! {
        static COUNT: Cell<usize> = const { Cell::new(0) };
    }

    pub(crate) fn reset() {
        COUNT.with(|c| c.set(0));
    }

    pub(crate) fn get() -> usize {
        COUNT.with(|c| c.get())
    }

    /// `pub(crate)`, not `pub(super)`. The precedent in `src/stt/model.rs` uses
    /// `pub(super)` because `locate` calls it from inside its own module; the
    /// caller here is `CandleEngine::new` in `src/stt/candle.rs`, this module's
    /// parent, which `pub(super)` does not reach.
    pub(crate) fn increment() {
        COUNT.with(|c| c.set(c.get() + 1));
    }
}

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
    let dir = ModelDir {
        dir: directory(models, identifier),
    };
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

        // 3. The format. This is the only check an unpinned model gets beyond the
        //    name, so it has to be a real one.
        if name == "model.safetensors" {
            check_safetensors(&path, metadata.len())?;
        } else {
            let text = std::fs::read_to_string(&path).map_err(|e| StoreError::Unreadable {
                path: path.clone(),
                why: e.to_string(),
            })?;
            serde_json::from_str::<serde_json::Value>(&text).map_err(|e| {
                StoreError::Unparsable {
                    path: path.clone(),
                    why: e.to_string(),
                }
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
    file.read_exact(&mut length)
        .map_err(|_| StoreError::NotSafetensors {
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
    file.read_exact(&mut json)
        .map_err(|e| StoreError::Unreadable {
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

    /// A safetensors file: an 8-byte little-endian header length, that many bytes
    /// of JSON, then the tensor data the header describes. Small on purpose — the
    /// entry is a parameter, so a test never needs a real 151 MB file to exercise
    /// the verified path.
    ///
    /// The trailing data region matters for more than realism: a test that wants
    /// to corrupt the bytes *without* breaking the format needs somewhere to do
    /// it. Flipping a byte of the header JSON is caught by check 3 and never
    /// reaches the digest.
    fn safetensors(body: &str) -> Vec<u8> {
        let mut v = (body.len() as u64).to_le_bytes().to_vec();
        v.extend_from_slice(body.as_bytes());
        v.extend_from_slice(&[0u8; 8]);
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
        let weights =
            safetensors(r#"{"encoder.weight":{"dtype":"F32","shape":[2],"data_offsets":[0,8]}}"#);
        let config = br#"{"num_mel_bins":80}"#.to_vec();
        let tokenizer = br#"{"version":"1.0"}"#.to_vec();
        std::fs::write(dir.join("model.safetensors"), &weights).unwrap();
        std::fs::write(dir.join("config.json"), &config).unwrap();
        std::fs::write(dir.join("tokenizer.json"), &tokenizer).unwrap();
        Written {
            models,
            weights,
            config,
            tokenizer,
        }
    }

    /// The digest of some bytes, via the same `sha256_of` production uses.
    ///
    /// The scratch file's name carries a process-wide counter, not a hash of the
    /// content: `cargo test` runs these on many threads, every fixture writes the
    /// same bytes, and a name derived from the content had two threads writing
    /// and deleting one path underneath each other.
    fn sha(bytes: &[u8]) -> String {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let tmp = std::env::temp_dir().join(format!(
            "store-sha-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::write(&tmp, bytes).unwrap();
        let digest = sha256_of(&tmp).unwrap();
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
                CatFile {
                    name: "model.safetensors",
                    bytes: w.weights.len() as u64,
                    sha256: Box::leak(sha(&w.weights).into_boxed_str()),
                },
                CatFile {
                    name: "config.json",
                    bytes: w.config.len() as u64,
                    sha256: Box::leak(sha(&w.config).into_boxed_str()),
                },
                CatFile {
                    name: "tokenizer.json",
                    bytes: w.tokenizer.len() as u64,
                    sha256: Box::leak(sha(&w.tokenizer).into_boxed_str()),
                },
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
            StoreError::WrongSize {
                expected, found, ..
            } => {
                assert_eq!(*expected, w.weights.len() as u64);
                assert_eq!(*found, 8);
            }
            other => panic!("expected WrongSize, got {other:?}"),
        }
        let message = error.to_string();
        assert!(
            message.contains("again"),
            "it must say what to do: {message}"
        );
    }

    #[test]
    fn the_right_size_and_the_wrong_bytes_are_caught_by_the_digest() {
        let w = write_model("swapped", "fixture");
        let entry = entry_for(&w);
        let dir = directory(&w.models, "fixture");
        // The last byte is tensor data, not header JSON, so the format check
        // passes and the digest is what has to catch this.
        let mut other = w.weights.clone();
        let last = other.len() - 1;
        other[last] ^= 0xff;
        std::fs::write(dir.join("model.safetensors"), &other).unwrap();
        let error = locate(&w.models, "fixture", Some(&entry)).expect_err("must refuse");
        assert!(
            matches!(error, StoreError::DigestMismatch { .. }),
            "got {error:?}"
        );
    }

    #[test]
    fn a_file_that_is_not_safetensors_is_named_as_one() {
        let w = write_model("notst", "homegrown");
        let dir = directory(&w.models, "homegrown");
        std::fs::write(
            dir.join("model.safetensors"),
            b"<html>an error page saved by mistake",
        )
        .unwrap();
        // Unpinned, so no size or digest check can catch it: the format must.
        let error = locate(&w.models, "homegrown", None).expect_err("must refuse");
        assert!(
            matches!(error, StoreError::NotSafetensors { .. }),
            "got {error:?}"
        );
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
    fn a_directory_that_does_not_exist_names_the_directory() {
        let models = scratch("absent");
        let error = locate(&models, "large-v3-turbo", None).expect_err("must refuse");
        let message = error.to_string();
        assert!(message.contains("large-v3-turbo"), "got {message}");
        assert!(message.contains("model --choose"), "got {message}");
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
        assert_eq!(
            glance(&w.models, "fixture", Some(&entry)),
            Glance::WrongSize
        );

        // Corrupt bytes at the right length are invisible to a glance, by
        // design: locate is what catches those. Again the flipped byte is tensor
        // data, so it is the digest doing the catching and not the format check.
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
}
