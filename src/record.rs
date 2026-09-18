//! What a take's two text stages produced, written beside that take's recording.
//!
//! Off unless `[record] transcripts` says otherwise: what this holds is what
//! somebody said, and keeping it is their decision. See `docs/design.md`
//! section 7 and `tasks/84/DESIGN_84.md`.

use std::path::{Path, PathBuf};

/// How many records are kept. A named constant rather than a configuration key,
/// on the footing `rewrite::skip::SKIP_WORD_LIMIT` is on: no measurement
/// distinguishes one value from another, and a key invites tuning where there is
/// nothing to tune. Fifty is far more takes than the question this record answers
/// reaches back through — it is asked about the take that just landed wrong — and
/// it bounds what accumulates without asking anybody to remember anything.
const KEEP: usize = 50;

/// What the rewrite stage did with the transcript. Five outcomes, because four
/// different things all deliver the transcript unchanged and a record that
/// cannot tell them apart is the defect issue #84 describes.
#[derive(Debug, PartialEq, Eq)]
pub enum Rewrite {
    /// The engine ran and returned this.
    Ran { text: String },
    /// `[rewrite] engine = "off"`. Nothing was attempted.
    Off,
    /// The engine resolved and was deliberately not called: a short phrase with
    /// nothing to fix (`rewrite::skip::plain`).
    Skipped,
    /// No engine resolved at start, so none could be called.
    Unavailable { why: String },
    /// An engine was called and returned an error. The one outcome where two
    /// texts existed and the second was thrown away.
    Failed { why: String },
}

/// The record itself, as JSON. Separate from writing it so that the shape can be
/// asserted without a file system.
///
/// There is no timestamp field: the take's name begins with unix milliseconds and
/// the file's own modification time says it again, so a third copy would be a
/// third thing that can disagree. There is no delivered-text field either: the
/// delivered text is `Ran`'s text, or the transcript in every other case.
pub fn document(take: &Path, transcript: &str, rewrite: &Rewrite) -> serde_json::Value {
    let rewrite = match rewrite {
        Rewrite::Ran { text } => serde_json::json!({ "ran": true, "text": text }),
        Rewrite::Off => serde_json::json!({ "ran": false, "why": "off" }),
        Rewrite::Skipped => serde_json::json!({ "ran": false, "why": "skipped" }),
        Rewrite::Unavailable { why } => {
            serde_json::json!({ "ran": false, "why": "unavailable", "detail": why })
        }
        Rewrite::Failed { why } => {
            serde_json::json!({ "ran": false, "why": "failed", "detail": why })
        }
    };
    serde_json::json!({
        "take": stem(take).unwrap_or_default(),
        "transcript": transcript,
        "rewrite": rewrite,
    })
}

/// What a `write` did, in the two parts a caller reports differently: the record
/// itself, and whatever the cap should have removed and could not. Neither is
/// allowed to end a take, so neither is returned as a failure of the call.
pub struct Report {
    pub written: Result<PathBuf, String>,
    pub not_removed: Vec<String>,
}

/// The key, turned into a writer or into nothing. A function rather than three
/// lines at the call site: it is the whole of what `[record] transcripts` does,
/// and inverted by accident it would record every take with the key off and none
/// with it on — a mistake nothing else in the suite could see.
pub fn records_for(record: &crate::config::Record, takes: &Path) -> Option<Records> {
    record
        .transcripts
        .then(|| Records::new(takes.to_path_buf()))
}

/// Where records go. Built only when the key is on, so there is nothing to
/// consult per take.
pub struct Records {
    directory: PathBuf,
}

impl Records {
    pub fn new(directory: PathBuf) -> Records {
        Records { directory }
    }

    pub fn directory(&self) -> &Path {
        &self.directory
    }

    /// Writes one record and then applies the cap. The cap runs here because this
    /// is the only moment anything touches the directory anyway.
    pub fn write(&self, take: &Path, transcript: &str, rewrite: &Rewrite) -> Report {
        let written = self.write_one(take, transcript, rewrite);
        let not_removed = match &written {
            Ok(path) => self.trim(path),
            Err(_) => Vec::new(),
        };
        Report {
            written,
            not_removed,
        }
    }

    fn write_one(
        &self,
        take: &Path,
        transcript: &str,
        rewrite: &Rewrite,
    ) -> Result<PathBuf, String> {
        let Some(stem) = stem(take) else {
            return Err(format!("{} names no take", take.display()));
        };
        if let Err(why) = std::fs::create_dir_all(&self.directory) {
            return Err(format!("{}: {why}", self.directory.display()));
        }
        let path = self.directory.join(format!("{stem}.json"));
        let text = serde_json::to_string_pretty(&document(take, transcript, rewrite))
            .map_err(|why| format!("{}: {why}", path.display()))?;
        std::fs::write(&path, text).map_err(|why| format!("{}: {why}", path.display()))?;
        Ok(path)
    }

    /// Keeps the newest `KEEP` records. Ordering by file name is ordering by
    /// time: a take's name begins with unix milliseconds, and every such stamp is
    /// thirteen digits until the year 2286. Only `.json` is counted; the
    /// recordings beside them are not this key's business.
    ///
    /// `just_written` is excluded whatever it sorts as. A clock that steps
    /// backwards far enough — an NTP correction, a resumed suspend — names a take
    /// below every record already there, and without this the call that wrote a
    /// record would delete it and still report success: the person would find
    /// nothing for the take they just made and no line saying why.
    ///
    /// Directories and anything that is not a plain file are left out. A
    /// subdirectory named `x.json` would otherwise take one of the fifty places
    /// and fail `remove_file` on every take thereafter.
    fn trim(&self, just_written: &Path) -> Vec<String> {
        let Ok(entries) = std::fs::read_dir(&self.directory) else {
            return Vec::new();
        };
        let mut records: Vec<PathBuf> = entries
            .flatten()
            .filter(|entry| {
                entry
                    .file_type()
                    .map(|kind| kind.is_file())
                    .unwrap_or(false)
            })
            .map(|entry| entry.path())
            .filter(|path| path.extension().and_then(|e| e.to_str()) == Some("json"))
            .collect();
        records.sort();
        // Counted down as removals succeed rather than sliced off the front, so
        // that a removal which fails leaves the next oldest to be tried instead
        // of leaving the directory one over its cap.
        let mut remaining = records.len();
        let mut failures = Vec::new();
        for path in records {
            if remaining <= KEEP {
                break;
            }
            if path == just_written {
                continue;
            }
            match std::fs::remove_file(&path) {
                Ok(()) => remaining -= 1,
                Err(why) => failures.push(format!("{}: {why}", path.display())),
            }
        }
        failures
    }
}

fn stem(take: &Path) -> Option<String> {
    take.file_stem()
        .and_then(|stem| stem.to_str())
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("herdr-voice-records-{tag}-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).expect("create scratch");
        dir
    }

    fn take_in(dir: &Path, name: &str) -> PathBuf {
        dir.join(format!("{name}.wav"))
    }

    #[test]
    fn a_rewritten_take_carries_both_texts_and_says_the_rewrite_ran() {
        let doc = document(
            Path::new("/takes/1789729477005-4242-1.wav"),
            "fix the worklog entry",
            &Rewrite::Ran {
                text: "Fix the worklog entry.".to_string(),
            },
        );
        assert_eq!(doc["take"], "1789729477005-4242-1");
        assert_eq!(doc["transcript"], "fix the worklog entry");
        assert_eq!(doc["rewrite"]["ran"], true);
        assert_eq!(doc["rewrite"]["text"], "Fix the worklog entry.");
        assert!(doc["rewrite"]["why"].is_null());
    }

    #[test]
    fn each_way_the_rewrite_did_not_run_is_named_and_the_two_that_have_one_carry_a_reason() {
        let take = Path::new("/takes/1-2-3.wav");
        for (rewrite, why, detail) in [
            (Rewrite::Off, "off", None),
            (Rewrite::Skipped, "skipped", None),
            (
                Rewrite::Unavailable {
                    why: "no engine is configured".to_string(),
                },
                "unavailable",
                Some("no engine is configured"),
            ),
            (
                Rewrite::Failed {
                    why: "the endpoint refused".to_string(),
                },
                "failed",
                Some("the endpoint refused"),
            ),
        ] {
            let doc = document(take, "some words", &rewrite);
            assert_eq!(doc["rewrite"]["ran"], false, "{why}");
            assert_eq!(doc["rewrite"]["why"], why);
            assert!(doc["rewrite"]["text"].is_null(), "{why}");
            match detail {
                Some(expected) => assert_eq!(doc["rewrite"]["detail"], expected),
                None => assert!(doc["rewrite"]["detail"].is_null(), "{why}"),
            }
            assert_eq!(doc["transcript"], "some words");
        }
    }

    #[test]
    fn the_record_lands_beside_the_take_under_the_same_stem_and_can_be_read_back() {
        let dir = scratch("beside");
        let records = Records::new(dir.clone());
        let report = records.write(
            &take_in(&dir, "1789729477005-4242-7"),
            "the transcript",
            &Rewrite::Ran {
                text: "The transcript.".to_string(),
            },
        );
        let path = report.written.expect("written");
        assert_eq!(path, dir.join("1789729477005-4242-7.json"));
        assert!(report.not_removed.is_empty());
        let back: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).expect("read back"))
                .expect("valid json");
        assert_eq!(back["transcript"], "the transcript");
        assert_eq!(back["rewrite"]["text"], "The transcript.");
    }

    #[test]
    fn the_directory_is_created_when_it_does_not_exist_yet() {
        let dir = scratch("created").join("takes");
        let records = Records::new(dir.clone());
        let report = records.write(&take_in(&dir, "1-1-1"), "words", &Rewrite::Off);
        assert!(report.written.is_ok(), "{:?}", report.written);
        assert!(dir.join("1-1-1.json").exists());
    }

    #[test]
    fn only_the_newest_fifty_records_are_kept_and_the_audio_is_left_alone() {
        let dir = scratch("cap");
        let records = Records::new(dir.clone());
        let wav = dir.join("1000000000000-1-0.wav");
        std::fs::write(&wav, b"not audio, but a file in the way").expect("write wav");
        for n in 0..(KEEP + 5) {
            let take = take_in(
                &dir,
                &format!("{:013}-1-{n}", 1_000_000_000_000u64 + n as u64),
            );
            let report = records.write(&take, "words", &Rewrite::Off);
            assert!(report.written.is_ok(), "{:?}", report.written);
            assert!(report.not_removed.is_empty());
        }
        let mut kept: Vec<String> = std::fs::read_dir(&dir)
            .expect("read dir")
            .map(|entry| {
                entry
                    .expect("entry")
                    .file_name()
                    .to_string_lossy()
                    .into_owned()
            })
            .filter(|name| name.ends_with(".json"))
            .collect();
        kept.sort();
        assert_eq!(kept.len(), KEEP);
        assert_eq!(
            kept.first().map(String::as_str),
            Some("1000000000005-1-5.json"),
            "the five oldest records are the ones removed"
        );
        assert!(wav.exists(), "the recording must not be touched");
    }

    #[test]
    fn the_key_decides_whether_there_is_a_writer_at_all() {
        let takes = Path::new("/state/takes");
        assert!(records_for(&crate::config::Record::default(), takes).is_none());
        let on = records_for(&crate::config::Record { transcripts: true }, takes)
            .expect("the key being on builds a writer");
        assert_eq!(on.directory(), takes);
    }

    #[test]
    fn a_record_is_never_removed_by_the_call_that_wrote_it() {
        let dir = scratch("backwards");
        let records = Records::new(dir.clone());
        // Fifty records already there, all newer than the one about to be
        // written: a clock that stepped backwards names the next take below
        // every one of them.
        for n in 0..KEEP {
            let take = take_in(&dir, &format!("{}-1-{n}", 9_000_000_000_000u64 + n as u64));
            assert!(records.write(&take, "words", &Rewrite::Off).written.is_ok());
        }
        let late = take_in(&dir, "1000000000000-1-0");
        let report = records.write(&late, "the take just made", &Rewrite::Off);
        let path = report.written.expect("written");
        assert!(
            path.exists(),
            "the record of the take just made must still be there"
        );
        assert!(report.not_removed.is_empty());
        let back: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).expect("read back"))
                .expect("valid json");
        assert_eq!(back["transcript"], "the take just made");
        let kept = std::fs::read_dir(&dir)
            .expect("read dir")
            .flatten()
            .filter(|entry| entry.path().extension().and_then(|e| e.to_str()) == Some("json"))
            .count();
        assert_eq!(kept, KEEP, "the cap still holds");
    }

    #[test]
    fn a_directory_named_like_a_record_is_not_counted_and_not_removed() {
        let dir = scratch("subdir");
        let records = Records::new(dir.clone());
        let intruder = dir.join("not-a-record.json");
        std::fs::create_dir_all(&intruder).expect("create intruder");
        for n in 0..KEEP {
            let take = take_in(&dir, &format!("{}-1-{n}", 1_000_000_000_000u64 + n as u64));
            let report = records.write(&take, "words", &Rewrite::Off);
            assert!(report.written.is_ok());
            assert!(
                report.not_removed.is_empty(),
                "a directory must never be counted toward the cap: {:?}",
                report.not_removed
            );
        }
        assert!(intruder.is_dir(), "the intruder is left where it is");
    }

    #[test]
    fn a_take_whose_path_has_no_name_is_refused_rather_than_panicking() {
        let dir = scratch("nameless");
        let records = Records::new(dir.clone());
        let report = records.write(Path::new("/"), "words", &Rewrite::Off);
        let why = report
            .written
            .expect_err("a path with no file name is refused");
        assert!(why.contains('/'), "the reason names the path: {why}");
    }

    #[test]
    fn a_directory_that_cannot_be_created_is_reported_rather_than_panicking() {
        let dir = scratch("blocked");
        let blocker = dir.join("takes");
        std::fs::write(&blocker, b"a file where a directory should be").expect("write blocker");
        let records = Records::new(blocker.clone());
        let report = records.write(&take_in(&blocker, "1-1-1"), "words", &Rewrite::Off);
        let why = report.written.expect_err("a blocked directory is reported");
        assert!(!why.is_empty());
    }
}
