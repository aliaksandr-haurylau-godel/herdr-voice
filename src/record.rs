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
pub(crate) const KEEP: usize = 50;

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

/// Keeps the newest `KEEP` takes in `directory` and removes every file belonging
/// to an older one. Returns one line about what it could not remove, or `None`
/// when there was nothing to say. Never a reason to end a take.
///
/// A free function over a path rather than a method of `Records`, because the
/// bound applies whether or not `[record] transcripts` is on: with it off, the
/// four endings that keep a recording are the only files there are, and a bound
/// living inside the recording feature would not run for them at all.
///
/// The unit is the take, by file stem, so a take's record and its recording are
/// kept or removed together rather than by two rules that have to agree.
/// Ordering by stem is ordering by time: a take's name begins with unix
/// milliseconds, thirteen digits until the year 2286, and `BTreeMap` walks its
/// keys in byte order.
///
/// `in_hand` — the take in the pipeline — is never removed, whatever it sorts as.
/// A clock stepping backwards names it below everything already there, and
/// removing it would take the recording out from under the take being served.
///
/// Entries that are not plain files are ignored. A subdirectory named `x.json`
/// would otherwise take one of the fifty places and fail a removal on every take
/// thereafter. A name whose bytes are not UTF-8 is ignored for the same reason it
/// cannot be a take's: nothing here creates one, and a name this cannot read is a
/// name it must not delete.
///
/// A file that cannot be removed keeps its take counted, so the next oldest is
/// removed instead and the directory does not drift over its cap. It holds one of
/// the fifty places and is retried on every take, so somebody is told about it
/// every time — which is what gets it removed by hand.
///
/// **One line, however many failed.** A directory nothing can be removed from —
/// one made read-only — otherwise puts a line per file into the journal on every
/// take, which is hundreds of lines for a state nobody can act on more than once.
/// The count and the oldest name are what a person needs; the rest is noise.
pub fn bound(directory: &Path, in_hand: &Path) -> Option<String> {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return None;
    };
    let mut takes: std::collections::BTreeMap<String, Vec<PathBuf>> =
        std::collections::BTreeMap::new();
    for entry in entries.flatten() {
        if !entry
            .file_type()
            .map(|kind| kind.is_file())
            .unwrap_or(false)
        {
            continue;
        }
        let path = entry.path();
        let extension = path.extension().and_then(|e| e.to_str());
        if !matches!(extension, Some("wav") | Some("json")) {
            continue;
        }
        let Some(name) = stem(&path) else {
            continue;
        };
        takes.entry(name).or_default().push(path);
    }
    let in_hand_stem = stem(in_hand);
    let mut failures = Vec::new();
    let mut remaining = takes.len();
    for (name, files) in takes {
        if remaining <= KEEP {
            break;
        }
        if Some(&name) == in_hand_stem.as_ref() {
            continue;
        }
        // One entry per take, not per file. The count is the one thing in the
        // line a person acts on, and with the key on a take has two files — a
        // per-file count would say twice the number of takes that are stuck.
        let mut refused: Option<String> = None;
        for path in files {
            if let Err(why) = std::fs::remove_file(&path) {
                refused.get_or_insert_with(|| format!("{}: {why}", path.display()));
            }
        }
        match refused {
            None => remaining -= 1,
            Some(why) => failures.push(why),
        }
    }
    match failures.len() {
        0 => None,
        1 => failures.pop(),
        count => Some(format!(
            "{count} takes could not be removed, the oldest of them {}",
            failures.remove(0)
        )),
    }
}

/// What a `write` did. One field, because the bound is no longer this type's
/// business: it belongs to the directory and runs whether or not the key is on.
pub struct Report {
    pub written: Result<PathBuf, String>,
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

    /// Writes one record. What accumulates in the directory is `bound`'s
    /// business, not this type's: the bound applies whether or not the key is on.
    pub fn write(&self, take: &Path, transcript: &str, rewrite: &Rewrite) -> Report {
        Report {
            written: self.write_one(take, transcript, rewrite),
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
    fn the_key_decides_whether_there_is_a_writer_at_all() {
        let takes = Path::new("/state/takes");
        assert!(records_for(&crate::config::Record::default(), takes).is_none());
        let on = records_for(&crate::config::Record { transcripts: true }, takes)
            .expect("the key being on builds a writer");
        assert_eq!(on.directory(), takes);
    }

    #[test]
    fn the_bound_keeps_the_newest_fifty_takes_and_removes_both_files_of_an_older_one() {
        let dir = scratch("bound-pairs");
        for n in 0..(KEEP + 3) {
            let name = format!("{}-1-{n}", 1_000_000_000_000u64 + n as u64);
            std::fs::write(dir.join(format!("{name}.wav")), b"audio").expect("wav");
            std::fs::write(dir.join(format!("{name}.json")), b"{}").expect("json");
        }
        let in_hand = dir.join(format!("{}-1-52.wav", 1_000_000_000_052u64));
        let failure = bound(&dir, &in_hand);
        assert!(failure.is_none(), "{failure:?}");
        let mut stems: Vec<String> = std::fs::read_dir(&dir)
            .expect("read dir")
            .flatten()
            .filter_map(|entry| stem(&entry.path()))
            .collect();
        stems.sort();
        stems.dedup();
        assert_eq!(stems.len(), KEEP, "fifty takes, not fifty files");
        for name in &stems {
            assert!(
                dir.join(format!("{name}.wav")).exists(),
                "{name} lost its wav"
            );
            assert!(
                dir.join(format!("{name}.json")).exists(),
                "{name} lost its json"
            );
        }
        assert!(
            !dir.join("1000000000000-1-0.wav").exists(),
            "the oldest take's recording goes with its record"
        );
    }

    #[test]
    fn a_take_with_only_a_recording_counts_as_a_take() {
        let dir = scratch("bound-wav-only");
        for n in 0..(KEEP + 2) {
            let name = format!("{}-1-{n}", 1_000_000_000_000u64 + n as u64);
            std::fs::write(dir.join(format!("{name}.wav")), b"audio").expect("wav");
        }
        let failure = bound(&dir, Path::new("/nowhere/none.wav"));
        assert!(failure.is_none(), "{failure:?}");
        let kept = std::fs::read_dir(&dir).expect("read dir").flatten().count();
        assert_eq!(kept, KEEP);
        assert!(!dir.join("1000000000000-1-0.wav").exists());
        assert!(dir.join("1000000000051-1-51.wav").exists());
    }

    #[test]
    fn the_take_in_hand_is_never_removed_even_when_it_sorts_first() {
        let dir = scratch("bound-in-hand");
        for n in 0..KEEP {
            let name = format!("{}-1-{n}", 9_000_000_000_000u64 + n as u64);
            std::fs::write(dir.join(format!("{name}.wav")), b"audio").expect("wav");
        }
        let in_hand = dir.join("1000000000000-1-0.wav");
        std::fs::write(&in_hand, b"audio").expect("wav");
        let failure = bound(&dir, &in_hand);
        assert!(failure.is_none(), "{failure:?}");
        assert!(
            in_hand.exists(),
            "a clock that stepped backwards must not cost the take in hand its recording"
        );
    }

    #[test]
    fn a_directory_named_like_a_take_is_not_counted_and_not_removed() {
        let dir = scratch("bound-subdir");
        // Named to sort before every take, so the guard is what keeps it out of
        // the fifty. A name sorting after them would be the newest entry, the
        // loop would break before reaching it, and the test would pass with the
        // guard deleted.
        let intruder = dir.join("0000000000000-1-0.json");
        std::fs::create_dir_all(&intruder).expect("create intruder");
        for n in 0..(KEEP + 1) {
            let name = format!("{}-1-{n}", 1_000_000_000_000u64 + n as u64);
            std::fs::write(dir.join(format!("{name}.wav")), b"audio").expect("wav");
        }
        let failure = bound(&dir, Path::new("/nowhere/none.wav"));
        assert!(
            failure.is_none(),
            "a directory must never be counted toward the bound: {failure:?}"
        );
        assert!(intruder.is_dir(), "the intruder is left where it is");
    }

    /// The `gone = false` branch, and the one line it produces however many
    /// files failed. A read-only directory is what refuses a `remove_file` on
    /// Unix; the file's own mode does not, which is why this is the mechanism and
    /// why the test is Unix-only.
    #[test]
    #[cfg(unix)]
    fn a_directory_nothing_can_be_removed_from_reports_once_and_not_per_file() {
        use std::os::unix::fs::PermissionsExt;

        let dir = scratch("bound-readonly");
        // Both files per take, so that a count of files and a count of takes are
        // different numbers and the assertion below can tell them apart.
        for n in 0..(KEEP + 2) {
            let name = format!("{}-1-{n}", 1_000_000_000_000u64 + n as u64);
            std::fs::write(dir.join(format!("{name}.wav")), b"audio").expect("wav");
            std::fs::write(dir.join(format!("{name}.json")), b"{}").expect("json");
        }
        let mut locked = std::fs::metadata(&dir).expect("metadata").permissions();
        locked.set_mode(0o500);
        std::fs::set_permissions(&dir, locked).expect("lock the directory");

        let failure = bound(&dir, Path::new("/nowhere/none.wav"));

        let mut open = std::fs::metadata(&dir).expect("metadata").permissions();
        open.set_mode(0o700);
        std::fs::set_permissions(&dir, open).expect("unlock the directory");

        let why = failure.expect("a directory nothing can be removed from is reported");
        assert!(
            why.starts_with(&format!("{} takes could not be removed", KEEP + 2)),
            "one line naming how many, not one line per file: {why}"
        );
        assert!(
            why.contains("1000000000000-1-0."),
            "and naming the oldest of them: {why}"
        );
        let left = std::fs::read_dir(&dir).expect("read dir").flatten().count();
        assert_eq!(left, (KEEP + 2) * 2, "nothing was removed");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_directory_that_cannot_be_read_is_not_a_failure() {
        assert!(bound(Path::new("/nowhere/at/all"), Path::new("/nowhere/x.wav")).is_none());
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
