//! The models offered on first run, pinned so a download can be verified.
//!
//! Each entry names a commit, not a branch: a digest pinned against a moving
//! branch turns a legitimate upstream update into a corruption report the person
//! cannot act on. Read from the Hugging Face API on 2026-09-09 by
//! `scripts/model_catalogue.py`, which regenerates this table; the two non-LFS
//! files were downloaded and hashed, because the API reports a git blob SHA-1 for
//! those. See `tasks/15/DESIGN_15.md`, section 3.

// Most of this module has no caller until `store`, `fetch` and the chooser land
// (`tasks/15/PLAN_15.md`, Tasks 5, 6 and 12), and this is a binary crate, so
// clippy's dead-code lint fires on the fields until then. Task 13 sweeps every
// such allowance out of this issue's modules and Task 14 greps for leftovers.
#![allow(dead_code)]

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
            File {
                name: "model.safetensors",
                bytes: 151_061_672,
                sha256: "7ebd0e69e78190ffe1438491fa05cc1f5c1aa3a4c4db3bc1723adbb551ea2395",
            },
            File {
                name: "config.json",
                bytes: 1_983,
                sha256: "ffdccec4f3211f4c63310f2b7098f309fe70f3952cedc5e4d11e43f5b2379b98",
            },
            File {
                name: "tokenizer.json",
                bytes: 2_480_466,
                sha256: "27fc476bfe7f17299480be2273fc0608e4d5a99aba2ab5dec5374b4482d1a566",
            },
        ],
    },
    Entry {
        identifier: "base",
        repo: "openai/whisper-base",
        revision: "e37978b90ca9030d5170a5c07aadb050351a65bb",
        mel_bins: 80,
        files: [
            File {
                name: "model.safetensors",
                bytes: 290_403_936,
                sha256: "07cadb9f25677c8d50df603e66a98fbd842cce45047139baeb16e6219a1e807b",
            },
            File {
                name: "config.json",
                bytes: 1_983,
                sha256: "a153c53883a6799b6f056b4a8d1a515c9926d03994682ba88a7616618d7da0c1",
            },
            File {
                name: "tokenizer.json",
                bytes: 2_480_466,
                sha256: "27fc476bfe7f17299480be2273fc0608e4d5a99aba2ab5dec5374b4482d1a566",
            },
        ],
    },
    Entry {
        identifier: "small",
        repo: "openai/whisper-small",
        revision: "973afd24965f72e36ca33b3055d56a652f456b4d",
        mel_bins: 80,
        files: [
            File {
                name: "model.safetensors",
                bytes: 966_995_080,
                sha256: "1d7734884874f1a1513ed9aa760a4f8e97aaa02fd6d93a3a85d27b2ae9ca596b",
            },
            File {
                name: "config.json",
                bytes: 1_967,
                sha256: "e6a2b489da1b5aed65a8eb8d1e7466fa867ad5643a8bc138ba708bd56b2875c4",
            },
            File {
                name: "tokenizer.json",
                bytes: 2_480_466,
                sha256: "27fc476bfe7f17299480be2273fc0608e4d5a99aba2ab5dec5374b4482d1a566",
            },
        ],
    },
    Entry {
        identifier: "large-v3-turbo",
        repo: "openai/whisper-large-v3-turbo",
        revision: "41f01f3fe87f28c78e2fbf8b568835947dd65ed9",
        mel_bins: 128,
        files: [
            File {
                name: "model.safetensors",
                bytes: 1_617_824_864,
                sha256: "542566a422ae4f3fd23f1ba11add198fca01bbf82e66e6a2857b3f608b1eb9d1",
            },
            File {
                name: "config.json",
                bytes: 1_256,
                sha256: "c5b526b3e3cd64cd8940dabb45e8ba726629e22d8ed389c29b552f9140daf04a",
            },
            File {
                name: "tokenizer.json",
                bytes: 2_710_337,
                sha256: "297b13372ac43916285644fb9687add3cc62ee2a1adb60da3dc25cc94c1871fd",
            },
        ],
    },
    Entry {
        identifier: "medium",
        repo: "openai/whisper-medium",
        revision: "abdf7c39ab9d0397620ccaea8974cc764cd0953e",
        mel_bins: 80,
        files: [
            File {
                name: "model.safetensors",
                bytes: 3_055_544_304,
                sha256: "62f73550fa6db24b0c6f6c5962bd0dae80fa644e93cde9cd9c3792971b47fd28",
            },
            File {
                name: "config.json",
                bytes: 1_991,
                sha256: "18706810eb740d1dc54d1db181358d5f8578600d0f449e51dfd4798c0223a1f5",
            },
            File {
                name: "tokenizer.json",
                bytes: 2_480_466,
                sha256: "27fc476bfe7f17299480be2273fc0608e4d5a99aba2ab5dec5374b4482d1a566",
            },
        ],
    },
    Entry {
        identifier: "large-v3",
        repo: "openai/whisper-large-v3",
        revision: "06f233fe06e710322aca913c1bc4249a0d71fce1",
        mel_bins: 128,
        files: [
            File {
                name: "model.safetensors",
                bytes: 3_087_130_976,
                sha256: "a8e94b85976e5864ba3e9525c7e6c83b2a1eca42d4b797a0c7c24d778e40fd95",
            },
            File {
                name: "config.json",
                bytes: 1_272,
                sha256: "ad0e8d1e46f4d01f7861a21509e5d0f977d6cc1f367a370603c92541d819807b",
            },
            File {
                name: "tokenizer.json",
                bytes: 2_480_617,
                sha256: "6d8cbd7cd0d8d5815e478dac67b85a26bbe77c1f5e0c6d76d1ce2abc0e5f21ca",
            },
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
                    file.sha256
                        .chars()
                        .all(|c| c.is_ascii_hexdigit() && !c.is_uppercase()),
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
