//! What makes a model usable, checked before anything tries to load one.
//!
//! Issue 3 accepted a substring test and recorded that this stage must replace it:
//! a substring matches an unrelated file, and a truncated download matches its own
//! name perfectly and fails later, somewhere less helpful. See
//! `tasks/13/DESIGN_13.md`, section 3.

use std::fmt;
use std::path::{Path, PathBuf};

/// Smaller than any real speech model, larger than an error page saved by mistake.
const SMALLEST_PLAUSIBLE_BYTES: u64 = 1024 * 1024;

/// The first four bytes of a ggml model, read off one on disk rather than recalled.
const GGML_MAGIC: [u8; 4] = [0x6C, 0x6D, 0x67, 0x67];

/// The file a model identifier names.
pub fn file_name(model: &str) -> String {
    format!("ggml-{model}.bin")
}

/// How many times `locate` has run, counted only in tests. Thread-local rather than
/// a shared static: `cargo test` runs tests concurrently on separate threads, and a
/// process-wide counter would pick up unrelated tests' calls. It pins one property:
/// `doctor` must locate a configured model at most once per invocation, not once
/// per report line. See the test `the_model_is_located_once_per_doctor_run` in
/// `src/doctor.rs`.
#[cfg(test)]
pub(crate) mod locate_calls {
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

    pub(super) fn increment() {
        COUNT.with(|c| c.set(c.get() + 1));
    }
}

#[derive(Debug, Clone)]
pub enum ModelError {
    Missing { path: PathBuf, model: String },
    TooSmall { path: PathBuf, bytes: u64 },
    NotAModel { path: PathBuf },
    DigestMismatch { path: PathBuf },
    Unreadable { path: PathBuf, why: String },
}

impl fmt::Display for ModelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ModelError::Missing { path, model } => write!(
                f,
                "no speech model at {}; put the {model} model there, or set \
                 [stt] model to one that is",
                path.display()
            ),
            ModelError::TooSmall { path, bytes } => write!(
                f,
                "{} is only {bytes} bytes, which is too small to be a model — \
                 a download that stopped early looks like this. Delete it and fetch it again",
                path.display()
            ),
            ModelError::NotAModel { path } => write!(
                f,
                "{} does not begin like a ggml model; it is something else under the \
                 right name. Delete it and fetch the model again",
                path.display()
            ),
            ModelError::DigestMismatch { path } => write!(
                f,
                "{} does not match the digest beside it; it is the wrong file or a \
                 damaged one. Delete both and fetch the model again",
                path.display()
            ),
            ModelError::Unreadable { path, why } => {
                write!(f, "cannot read {}: {why}", path.display())
            }
        }
    }
}

impl std::error::Error for ModelError {}

/// The model file, if it is one. Four checks in order, each catching what the next
/// cannot: the wrong name, a truncated file, a file that is not a model at all, and
/// the wrong model under the right name.
pub fn locate(models: &Path, model: &str) -> Result<PathBuf, ModelError> {
    #[cfg(test)]
    locate_calls::increment();

    let path = models.join(file_name(model));

    let metadata = match std::fs::metadata(&path) {
        Ok(metadata) => metadata,
        Err(_) => {
            return Err(ModelError::Missing {
                path,
                model: model.to_string(),
            })
        }
    };
    if metadata.len() < SMALLEST_PLAUSIBLE_BYTES {
        return Err(ModelError::TooSmall {
            path,
            bytes: metadata.len(),
        });
    }

    let mut head = [0u8; 4];
    match std::fs::File::open(&path).and_then(|mut f| {
        use std::io::Read;
        f.read_exact(&mut head)
    }) {
        Ok(()) => {}
        Err(e) => {
            return Err(ModelError::Unreadable {
                path,
                why: e.to_string(),
            })
        }
    }
    if head != GGML_MAGIC {
        return Err(ModelError::NotAModel { path });
    }

    // A digest is checked when somebody wrote one down. Nothing downloads models
    // yet, so its absence is the ordinary case rather than a fault.
    let sidecar = path.with_extension("bin.sha256");
    if let Ok(expected) = std::fs::read_to_string(&sidecar) {
        let expected = expected
            .split_whitespace()
            .next()
            .unwrap_or("")
            .to_lowercase();
        let actual = sha256_of(&path).map_err(|why| ModelError::Unreadable {
            path: path.clone(),
            why,
        })?;
        if !expected.is_empty() && expected != actual {
            return Err(ModelError::DigestMismatch { path });
        }
    }
    Ok(path)
}

/// A SHA-256 of a file, computed here rather than pulled in: one digest, used once,
/// against a dependency the project would carry forever.
fn sha256_of(path: &Path) -> Result<String, String> {
    use std::io::Read;
    let mut file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 1 << 16];
    loop {
        let read = file.read(&mut buffer).map_err(|e| e.to_string())?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hasher.finish())
}

/// SHA-256, from the specification. Small, self-contained and tested against the
/// published vectors.
struct Sha256 {
    state: [u32; 8],
    buffer: [u8; 64],
    filled: usize,
    length: u64,
}

const K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

impl Sha256 {
    fn new() -> Sha256 {
        Sha256 {
            state: [
                0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
                0x5be0cd19,
            ],
            buffer: [0u8; 64],
            filled: 0,
            length: 0,
        }
    }

    fn update(&mut self, mut data: &[u8]) {
        self.length += data.len() as u64;
        while !data.is_empty() {
            let room = 64 - self.filled;
            let take = room.min(data.len());
            self.buffer[self.filled..self.filled + take].copy_from_slice(&data[..take]);
            self.filled += take;
            data = &data[take..];
            if self.filled == 64 {
                let block = self.buffer;
                self.compress(&block);
                self.filled = 0;
            }
        }
    }

    fn compress(&mut self, block: &[u8; 64]) {
        let mut w = [0u32; 64];
        for (i, word) in w.iter_mut().enumerate().take(16) {
            let at = i * 4;
            *word = u32::from_be_bytes([block[at], block[at + 1], block[at + 2], block[at + 3]]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = self.state;
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let t1 = h
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);
            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }
        for (slot, value) in self.state.iter_mut().zip([a, b, c, d, e, f, g, h]) {
            *slot = slot.wrapping_add(value);
        }
    }

    fn finish(mut self) -> String {
        let bits = self.length * 8;
        self.update(&[0x80]);
        while self.filled != 56 {
            self.update(&[0]);
        }
        // `update` counted the padding; the length written is the message's.
        let block = self.buffer;
        let mut last = block;
        last[56..].copy_from_slice(&bits.to_be_bytes());
        self.compress(&last);
        self.state
            .iter()
            .map(|word| format!("{word:08x}"))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("stt-model-{tag}-{}", std::process::id()));
        std::fs::create_dir_all(&path).expect("scratch");
        path
    }

    fn write_model(dir: &Path, name: &str, magic: &[u8; 4], bytes: usize) -> PathBuf {
        let path = dir.join(name);
        let mut content = magic.to_vec();
        content.resize(bytes, 0u8);
        std::fs::write(&path, content).expect("write");
        path
    }

    #[test]
    fn the_file_name_is_the_identifier_in_a_template() {
        assert_eq!(file_name("large-v3-turbo"), "ggml-large-v3-turbo.bin");
    }

    #[test]
    fn a_real_model_is_found() {
        let dir = scratch("good");
        let path = write_model(&dir, "ggml-tiny.bin", &GGML_MAGIC, 2 * 1024 * 1024);
        assert_eq!(locate(&dir, "tiny").expect("found"), path);
    }

    #[test]
    fn a_name_that_merely_contains_the_model_does_not_count() {
        // The rule issue 3 left behind, and the reason it had to go.
        let dir = scratch("substring");
        write_model(&dir, "ggml-tiny.bin.part", &GGML_MAGIC, 2 * 1024 * 1024);
        write_model(&dir, "old-ggml-tiny.bin", &GGML_MAGIC, 2 * 1024 * 1024);
        let error = locate(&dir, "tiny").expect_err("must refuse");
        assert!(matches!(error, ModelError::Missing { .. }), "got {error:?}");
    }

    #[test]
    fn a_truncated_download_is_named_as_one() {
        let dir = scratch("small");
        write_model(&dir, "ggml-tiny.bin", &GGML_MAGIC, 32);
        let error = locate(&dir, "tiny").expect_err("must refuse");
        assert!(
            matches!(error, ModelError::TooSmall { .. }),
            "got {error:?}"
        );
        assert!(error.to_string().contains("stopped early"), "got {error}");
    }

    #[test]
    fn a_file_that_is_not_a_model_is_named_as_one() {
        let dir = scratch("wrong-magic");
        write_model(&dir, "ggml-tiny.bin", b"<htm", 2 * 1024 * 1024);
        let error = locate(&dir, "tiny").expect_err("must refuse");
        assert!(
            matches!(error, ModelError::NotAModel { .. }),
            "got {error:?}"
        );
    }

    #[test]
    fn a_digest_that_matches_is_accepted_and_one_that_does_not_is_refused() {
        let dir = scratch("digest");
        let path = write_model(&dir, "ggml-tiny.bin", &GGML_MAGIC, 2 * 1024 * 1024);
        let actual = sha256_of(&path).expect("digest");

        std::fs::write(dir.join("ggml-tiny.bin.sha256"), &actual).unwrap();
        assert!(locate(&dir, "tiny").is_ok(), "a matching digest must pass");

        std::fs::write(dir.join("ggml-tiny.bin.sha256"), "0".repeat(64)).unwrap();
        let error = locate(&dir, "tiny").expect_err("must refuse");
        assert!(
            matches!(error, ModelError::DigestMismatch { .. }),
            "got {error:?}"
        );
    }

    #[test]
    fn no_digest_beside_the_model_is_not_a_failure() {
        let dir = scratch("no-digest");
        write_model(&dir, "ggml-tiny.bin", &GGML_MAGIC, 2 * 1024 * 1024);
        assert!(locate(&dir, "tiny").is_ok());
    }

    #[test]
    fn the_digest_matches_the_published_vectors() {
        // Without this, a hand-written hash is an assumption rather than a fact.
        let mut hasher = Sha256::new();
        hasher.update(b"abc");
        assert_eq!(
            hasher.finish(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );

        let mut empty = Sha256::new();
        empty.update(b"");
        assert_eq!(
            empty.finish(),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }
}
