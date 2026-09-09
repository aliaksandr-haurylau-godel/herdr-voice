//! Fetching a model, and refusing to keep one that did not arrive whole.
//!
//! Bytes land in `<name>.part` and are verified before the rename to the real
//! name. So a file under its real name is, by construction, one that passed
//! every check, and there is no window in which a half-written model exists.
//! See `tasks/15/DESIGN_15.md`, section 4.

// No caller until the chooser lands (`tasks/15/PLAN_15.md`, Task 12).
#![allow(dead_code)]

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
    Http {
        url: String,
        why: String,
    },
    Status {
        url: String,
        code: u16,
    },
    ShortRead {
        name: String,
        expected: u64,
        found: u64,
    },
    Digest {
        name: String,
    },
    Io {
        path: PathBuf,
        why: String,
    },
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
            FetchError::ShortRead {
                name,
                expected,
                found,
            } => write!(
                f,
                "{name} arrived as {found} bytes and should be {expected} — the \
                 download stopped early. Nothing was kept; run \
                 `herdr-voice model --choose` again"
            ),
            FetchError::Digest { name } => write!(
                f,
                "{name} arrived complete but does not match the digest this plugin \
                 pins for it. Nothing was kept; run `herdr-voice model --choose` \
                 again, and if it happens twice the pinned model has changed upstream"
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
        Err(e) => {
            return Err(FetchError::Http {
                url,
                why: e.to_string(),
            })
        }
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
            return Err(FetchError::Digest {
                name: file.name.to_string(),
            });
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stt::catalogue::{Entry, File as CatFile};
    use std::net::TcpListener;

    /// Serves each requested path from a fixed table until nothing has connected
    /// for a short while, then returns. One thread, any number of connections.
    ///
    /// It stops on a deadline rather than on a connection count, because the
    /// count is not knowable from the test: `fetch_into` fetches three files but
    /// stops at the first failure, so a double waiting for three connections
    /// blocks forever on `join` in exactly the tests that exercise a failure.
    /// That is not hypothetical — it hung this suite before the deadline existed.
    ///
    /// The request is read to the end of its headers before the response is
    /// written, for the reason `src/rewrite/http.rs` records: a stream dropped
    /// while the kernel still holds unread bytes can turn the close into a reset,
    /// which surfaces as an intermittent, unrelated-looking failure.
    fn serve(
        files: Vec<(&'static str, &'static str, Vec<u8>)>,
    ) -> (String, std::thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        listener.set_nonblocking(true).expect("nonblocking");
        let addr = listener.local_addr().expect("addr");
        let base = format!("http://{addr}");
        let handle = std::thread::spawn(move || {
            // Generous: it only ever elapses after the client has stopped asking.
            let quiet_for = std::time::Duration::from_millis(750);
            let mut last = std::time::Instant::now();
            loop {
                let (mut stream, _) = match listener.accept() {
                    Ok(pair) => pair,
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        if last.elapsed() > quiet_for {
                            return;
                        }
                        std::thread::sleep(std::time::Duration::from_millis(5));
                        continue;
                    }
                    Err(_) => return,
                };
                last = std::time::Instant::now();
                stream.set_nonblocking(false).expect("blocking stream");
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
                let first = request.lines().next().unwrap_or_default().to_string();
                let matched = files.iter().find(|(suffix, _, _)| first.contains(suffix));
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

    /// See `store`'s own `sha`: the scratch name carries a counter, not a hash of
    /// the content, because these tests run on many threads with identical bytes.
    fn sha(bytes: &[u8]) -> String {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let tmp = std::env::temp_dir().join(format!(
            "fetch-sha-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
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
                CatFile {
                    name: "model.safetensors",
                    bytes: WEIGHTS.len() as u64,
                    sha256: Box::leak(sha(WEIGHTS).into_boxed_str()),
                },
                CatFile {
                    name: "config.json",
                    bytes: CONFIG.len() as u64,
                    sha256: Box::leak(sha(CONFIG).into_boxed_str()),
                },
                CatFile {
                    name: "tokenizer.json",
                    bytes: TOKENIZER.len() as u64,
                    sha256: Box::leak(sha(TOKENIZER).into_boxed_str()),
                },
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
        let (base, handle) = serve(all_three("200 OK"));
        let entry = entry();
        fetch_into(&base, &entry, &dir, &mut Silent).expect("must succeed");
        handle.join().unwrap();

        for name in ["model.safetensors", "config.json", "tokenizer.json"] {
            assert!(dir.join(name).exists(), "{name} is missing");
            assert!(
                !dir.join(format!("{name}.part")).exists(),
                "{name}.part was left behind"
            );
        }
        assert_eq!(std::fs::read(dir.join("config.json")).unwrap(), CONFIG);
    }

    #[test]
    fn a_short_transfer_names_the_size_and_leaves_nothing_behind() {
        let dir = scratch("short");
        let mut files = all_three("200 OK");
        files[0].2.truncate(8);
        let (base, handle) = serve(files);
        let error = fetch_into(&base, &entry(), &dir, &mut Silent).expect_err("must refuse");
        handle.join().unwrap();

        match &error {
            FetchError::ShortRead {
                name,
                expected,
                found,
            } => {
                assert_eq!(name, "model.safetensors");
                assert_eq!(*expected, WEIGHTS.len() as u64);
                assert_eq!(*found, 8);
            }
            other => panic!("expected ShortRead, got {other:?}"),
        }
        assert!(
            !dir.join("model.safetensors").exists(),
            "an unverified file under the real name"
        );
        assert!(
            !dir.join("model.safetensors.part").exists(),
            "a .part was left behind"
        );
    }

    #[test]
    fn the_right_length_and_the_wrong_bytes_are_caught_by_the_digest() {
        let dir = scratch("digest");
        let mut files = all_three("200 OK");
        let last = files[0].2.len() - 1;
        files[0].2[last] ^= 0xff;
        let (base, handle) = serve(files);
        let error = fetch_into(&base, &entry(), &dir, &mut Silent).expect_err("must refuse");
        handle.join().unwrap();

        assert!(matches!(error, FetchError::Digest { .. }), "got {error:?}");
        assert!(!dir.join("model.safetensors").exists());
        assert!(!dir.join("model.safetensors.part").exists());
    }

    #[test]
    fn a_non_2xx_response_names_the_status_and_the_address() {
        let dir = scratch("status");
        let (base, handle) = serve(all_three("503 Service Unavailable"));
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
        let error = fetch_into(
            &format!("http://127.0.0.1:{port}"),
            &entry(),
            &dir,
            &mut Silent,
        )
        .expect_err("must refuse");
        assert!(matches!(error, FetchError::Http { .. }), "got {error:?}");
        assert!(
            error.to_string().contains("again"),
            "it must say what to do: {error}"
        );
    }

    #[test]
    fn every_failure_names_what_to_do_next() {
        let cases = [
            FetchError::Http {
                url: "u".into(),
                why: "refused".into(),
            },
            FetchError::Status {
                url: "u".into(),
                code: 404,
            },
            FetchError::ShortRead {
                name: "n".into(),
                expected: 2,
                found: 1,
            },
            FetchError::Digest { name: "n".into() },
            FetchError::Io {
                path: PathBuf::from("/p"),
                why: "full".into(),
            },
            FetchError::Unknown("x".into()),
        ];
        for case in cases {
            let message = case.to_string();
            assert!(
                message.contains("again") || message.contains("space") || message.contains("Check"),
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

    #[test]
    fn a_file_already_there_and_whole_is_not_fetched_again() {
        // What makes a retry after one failed file cheap rather than another
        // three gigabytes: the weights are already there, so the double is never
        // asked for them and only two requests arrive.
        let dir = scratch("resume");
        std::fs::write(dir.join("model.safetensors"), WEIGHTS).unwrap();
        let (base, handle) = serve(vec![
            ("config.json", "200 OK", CONFIG.to_vec()),
            ("tokenizer.json", "200 OK", TOKENIZER.to_vec()),
        ]);
        fetch_into(&base, &entry(), &dir, &mut Silent).expect("must succeed");
        handle.join().unwrap();
        assert!(dir.join("tokenizer.json").exists());
    }
}
