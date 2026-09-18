# PLAN_84 — what recognition produced is recorded beside what the rewrite made of it

> **For agentic workers:** REQUIRED SUB-SKILL: use `superpowers:executing-plans`
> with `superpowers:test-driven-development` to implement this plan task by task.
> Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** with `[record] transcripts = true`, each finished take leaves one JSON
file beside its own recording holding the transcript recognition returned, the
text the rewrite returned, and which of five things the rewrite did.

**Architecture:** a new module `src/record.rs` owns the outcome type, the JSON
document and the writer with its fifty-record cap. The rewrite match in
`src/daemon.rs` moves into a function that returns that outcome instead of the
text, which is what makes the five cases assertable and what stops the transcript
being shadowed. `Runtime` carries `records: Option<Records>`, so with the key off
there is no writer to call. `doctor` gains a seventh line naming the directory.

**Tech stack:** Rust 2021, `serde_json` (already a dependency, `Cargo.toml:27`),
`std::fs`. No new crate. Nothing here is platform-specific: `transport::state_directory`
is cross-platform, and no item added by this plan may sit behind `#[cfg(unix)]`.

**Spec:** `tasks/84/DESIGN_84.md`. The criteria it answers: `tasks/84/AC_84.md`.

## Global constraints

- Everything in the repository is English: code, comments, output strings,
  commits, the pull request.
- Nothing that identifies an employer, a client, an internal system or a private
  machine enters any file. Paths in comments are relative to the repository root.
- Four gates before every commit, run fresh, all green: `cargo test`,
  `cargo clippy --all-targets -- -D warnings`, `cargo fmt --check`,
  `python3 scripts/check_manifest.py`.
- No panic path in daemon code. `unwrap` and `expect` are allowed inside
  `#[cfg(test)]` and nowhere else.
- **Nothing may be reachable only from a `#[cfg(unix)]` path.** CI compiles with
  `-D warnings`, and an item dead on Windows fails the build.
- Grep every file written for `<new_string>`, `</new_string>`, `<old_string>`,
  `</old_string>` and for conflict markers at the start of a line.
- The `delivering:` line keeps its text, its shape and its position relative to
  the delivery attempt. Audio, delivery, the indicator, the reply protocol and
  the bias are not touched.
- The owner's installation is not touched: no `herdr plugin link` from this
  worktree, no daemon restart, no plugin action, no edit under his herdr
  configuration.

---

### Task 1: the record module

**Files:**
- Create: `src/record.rs`
- Modify: `src/main.rs:27` — add `mod record;` between `mod ptt;` and
  `mod rewrite;`. The list is alphabetical and that is where `record` belongs.

**Interfaces:**
- Consumes: nothing from earlier tasks.
- Produces, all public to the crate:
  - `pub enum Rewrite { Ran { text: String }, Off, Skipped, Unavailable { why: String }, Failed { why: String } }`
  - `pub fn document(take: &Path, transcript: &str, rewrite: &Rewrite) -> serde_json::Value`
  - `pub struct Records` with `pub fn new(directory: PathBuf) -> Records`,
    `pub fn directory(&self) -> &Path` and
    `pub fn write(&self, take: &Path, transcript: &str, rewrite: &Rewrite) -> Report`
  - `pub struct Report { pub written: Result<PathBuf, String>, pub not_removed: Vec<String> }`
  - `const KEEP: usize = 50;` — private to the module.

- [ ] **Step 1: write the failing tests**

Add to `src/record.rs`, below the implementation you will write in step 3:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "herdr-voice-records-{tag}-{}",
            std::process::id()
        ));
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
            let take = take_in(&dir, &format!("{:013}-1-{n}", 1_000_000_000_000u64 + n as u64));
            let report = records.write(&take, "words", &Rewrite::Off);
            assert!(report.written.is_ok(), "{:?}", report.written);
            assert!(report.not_removed.is_empty());
        }
        let mut kept: Vec<String> = std::fs::read_dir(&dir)
            .expect("read dir")
            .map(|entry| entry.expect("entry").file_name().to_string_lossy().into_owned())
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
    fn a_take_whose_path_has_no_name_is_refused_rather_than_panicking() {
        let dir = scratch("nameless");
        let records = Records::new(dir.clone());
        let report = records.write(Path::new("/"), "words", &Rewrite::Off);
        let why = report.written.expect_err("a path with no file name is refused");
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
```

- [ ] **Step 2: run the tests to verify they fail**

Run: `cargo test record::`
Expected: FAIL — the module does not exist.

- [ ] **Step 3: write the implementation**

`src/record.rs`, above the test module:

```rust
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
        let not_removed = if written.is_ok() { self.trim() } else { Vec::new() };
        Report { written, not_removed }
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
    fn trim(&self) -> Vec<String> {
        let Ok(entries) = std::fs::read_dir(&self.directory) else {
            return Vec::new();
        };
        let mut records: Vec<PathBuf> = entries
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| path.extension().and_then(|e| e.to_str()) == Some("json"))
            .collect();
        if records.len() <= KEEP {
            return Vec::new();
        }
        records.sort();
        let over = records.len() - KEEP;
        let mut failures = Vec::new();
        for path in records.into_iter().take(over) {
            if let Err(why) = std::fs::remove_file(&path) {
                failures.push(format!("{}: {why}", path.display()));
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
```

And in `src/main.rs`, between `mod ptt;` at line 27 and `mod rewrite;` at line 28:

```rust
mod record;
```

- [ ] **Step 4: run the tests to verify they pass**

Run: `cargo test record::`
Expected: PASS, seven tests.

- [ ] **Step 5: run the four gates and commit**

```sh
cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt --check && python3 scripts/check_manifest.py
git add src/record.rs src/main.rs
git commit -m "Add the take record: five rewrite outcomes, one file per take, fifty kept

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>"
```

---

### Task 2: the rewrite outcome, out of the match

**Files:**
- Modify: `src/daemon.rs:918-937` — the rewrite match becomes a call; add
  `fn resolve_rewrite` beside `transcribe_take`; add tests to `mod tests`.

**Interfaces:**
- Consumes: `crate::record::Rewrite` from task 1.
- Produces: `fn resolve_rewrite(runtime: &Runtime, transcript: &str, bias: &str) -> crate::record::Rewrite`,
  private to the module.

**What must not change:** `tell_once` fires on the same two arms it fires on
today and nowhere else. `publish(runtime, Activity::Working { stage: Stage::Fixing })`
at `src/daemon.rs:910-917` stays in `transcribe_take`, before the call. The
`delivering:` line keeps its text and its position.

- [ ] **Step 1: write the failing tests**

Add to `src/daemon.rs`'s `mod tests`:

```rust
    #[test]
    fn the_five_rewrite_outcomes_are_each_named() {
        use crate::record::Rewrite;

        let mut runtime = fake_runtime("some spoken words");
        runtime.rewrite = crate::rewrite::Resolution::Off;
        assert_eq!(
            resolve_rewrite(&runtime, "some spoken words", ""),
            Rewrite::Off
        );

        runtime.rewrite = crate::rewrite::Resolution::Unavailable("no engine".to_string());
        assert_eq!(
            resolve_rewrite(&runtime, "some spoken words", ""),
            Rewrite::Unavailable {
                why: "no engine".to_string()
            }
        );

        // Eight words or fewer, no run of two ASCII letters, nothing shared with
        // an empty bias: exactly what `rewrite::skip::plain` skips.
        runtime.rewrite = crate::rewrite::Resolution::Engine(Box::new(
            crate::rewrite::tests_support::Fake(Ok("never called".to_string())),
        ));
        runtime.skip_if_plain = true;
        assert_eq!(resolve_rewrite(&runtime, "просто пара слов", ""), Rewrite::Skipped);

        // The same runtime with the gate off reaches the engine.
        runtime.skip_if_plain = false;
        assert_eq!(
            resolve_rewrite(&runtime, "просто пара слов", ""),
            Rewrite::Ran {
                text: "never called".to_string()
            }
        );

        runtime.rewrite = crate::rewrite::Resolution::Engine(Box::new(
            crate::rewrite::tests_support::Fake(Err("the endpoint refused".to_string())),
        ));
        match resolve_rewrite(&runtime, "просто пара слов", "") {
            Rewrite::Failed { why } => assert!(
                why.contains("the endpoint refused"),
                "the reason travels with the outcome: {why}"
            ),
            other => panic!("expected Failed, got {other:?}"),
        }
    }

    #[test]
    fn an_unavailable_engine_still_tells_the_person_once() {
        let (mut runtime, journal) =
            runtime_reading_back(crate::delivery::tests_support::FakeDeliverer::ok(), false);
        runtime.rewrite = crate::rewrite::Resolution::Unavailable("no engine".to_string());
        let _ = resolve_rewrite(&runtime, "some spoken words", "");
        let _ = resolve_rewrite(&runtime, "some spoken words", "");
        let notices = journalled(&journal)
            .into_iter()
            .filter(|line| line.contains("no engine"))
            .count();
        assert_eq!(notices, 1, "one notice per daemon, not per take");
    }
```

- [ ] **Step 2: run the tests to verify they fail**

Run: `cargo test daemon::tests::the_five_rewrite_outcomes daemon::tests::an_unavailable_engine_still`
Expected: FAIL — `resolve_rewrite` is not defined.

- [ ] **Step 3: write the implementation**

Replace `src/daemon.rs:918-937` — the whole `let text = match &runtime.rewrite { ... };`
expression — with:

```rust
    let rewrite = resolve_rewrite(runtime, &text, bias);
    let text = match &rewrite {
        crate::record::Rewrite::Ran { text } => text.clone(),
        _ => text,
    };
```

and add, immediately after `transcribe_take`:

```rust
/// Which of five things the rewrite stage did, rather than what it produced.
///
/// The match used to yield the text, which shadowed the transcript recognition
/// returned and discarded the reason a given arm was taken — issue #84. Returning
/// the outcome keeps both: the transcript stays a binding nothing writes over, and
/// the four ways a take is delivered unrewritten stop being indistinguishable.
///
/// It takes `&Runtime` and carries the two `tell_once` calls with it. A pure
/// function whose caller raised the notice would put the notice back in the
/// caller, which is the shape this extraction exists to remove.
fn resolve_rewrite(runtime: &Runtime, transcript: &str, bias: &str) -> crate::record::Rewrite {
    match &runtime.rewrite {
        crate::rewrite::Resolution::Off => crate::record::Rewrite::Off,
        crate::rewrite::Resolution::Unavailable(why) => {
            tell_once(runtime, why);
            crate::record::Rewrite::Unavailable { why: why.clone() }
        }
        crate::rewrite::Resolution::Engine(engine) => {
            if crate::rewrite::skip::plain(transcript, bias, runtime.skip_if_plain) {
                crate::record::Rewrite::Skipped
            } else {
                match engine.rewrite(transcript, bias) {
                    Ok(text) => crate::record::Rewrite::Ran { text },
                    Err(why) => {
                        let why = why.to_string();
                        tell_once(runtime, &why);
                        crate::record::Rewrite::Failed { why }
                    }
                }
            }
        }
    }
}
```

- [ ] **Step 4: run the tests to verify they pass**

Run: `cargo test daemon::`
Expected: PASS, including every test that already exercised the take path — the
delivered text is unchanged on all five paths.

- [ ] **Step 5: run the four gates and commit**

```sh
cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt --check && python3 scripts/check_manifest.py
git add src/daemon.rs
git commit -m "Return the rewrite's outcome instead of its text, so the transcript survives it

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>"
```

---

### Task 3: the configuration key

**Files:**
- Modify: `src/config.rs` — add `Record` beside `Delivery`, add the field to
  `Config`, add tests to `mod tests`.
- Modify: `src/daemon.rs` — add `records: Option<crate::record::Records>` to
  `Runtime`, build it in `start`, and set it to `None` in every `Runtime { ... }`
  literal in `mod tests`.

**Interfaces:**
- Consumes: `crate::record::Records` from task 1.
- Produces: `config::Record { pub transcripts: bool }`, `Config::record`, and
  `Runtime::records`.

- [ ] **Step 1: write the failing tests**

Add to `src/config.rs`'s `mod tests`:

```rust
    #[test]
    fn recording_transcripts_is_off_when_nothing_says_otherwise() {
        assert!(!Config::default().record.transcripts);
    }

    #[test]
    fn recording_transcripts_is_read_from_the_file() {
        let directory = scratch("record-on");
        std::fs::write(
            directory.join("config.toml"),
            "[record]\ntranscripts = true\n",
        )
        .unwrap();
        let loaded = load(Some(&directory));
        assert!(loaded.config.record.transcripts);
    }
```

`scratch` is the module's existing temporary-directory helper, at
`src/config.rs:321`. Do not add a second one.

- [ ] **Step 2: run the tests to verify they fail**

Run: `cargo test config::tests::recording_transcripts`
Expected: FAIL — `Config` has no field `record`.

- [ ] **Step 3: write the implementation**

In `src/config.rs`, add the field to `Config` after `ptt`:

```rust
    pub record: Record,
```

and the section beside `Delivery`:

```rust
/// What the plugin keeps of a take's words. Off, because a transcript is what
/// somebody said in their own room and keeping it is their decision, not the
/// plugin's.
///
/// `transcripts` here and `[context] source = "transcript"` are different things:
/// this is what the person said, and that is what the agent said. See
/// `docs/design.md` section 7.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct Record {
    pub transcripts: bool,
}
```

In `src/daemon.rs`, add to `Runtime` after `ui`:

```rust
    /// Where a take's record goes, or `None` when `[record] transcripts` is off.
    /// An `Option` rather than a flag beside a path: with the key off there is
    /// then no writer to call, so nothing in the take path can forget to check.
    pub records: Option<crate::record::Records>,
```

In `start`, beside where `takes` is computed (`src/daemon.rs:1203-1205`), build it
from the same directory:

```rust
    let records = loaded
        .config
        .record
        .transcripts
        .then(|| crate::record::Records::new(takes.clone()));
```

`takes` is moved into `Recorder::spawn` further down, so this must be computed
before that call and uses `takes.clone()`. Add `records,` to the `Runtime { ... }`
literal at `src/daemon.rs:1245`.

Then add `records: None,` to every other `Runtime { ... }` literal in the file.
Find them with `grep -n "Runtime {" src/daemon.rs`; there are five in total, one
in `start` and four in `mod tests`.

- [ ] **Step 4: run the tests to verify they pass**

Run: `cargo test config:: daemon::`
Expected: PASS.

- [ ] **Step 5: run the four gates and commit**

```sh
cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt --check && python3 scripts/check_manifest.py
git add src/config.rs src/daemon.rs
git commit -m "Add [record] transcripts, off by default, and carry it as Option<Records>

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>"
```

---

### Task 4: write the record on the take path

**Files:**
- Modify: `src/daemon.rs` — write the record in `transcribe_take` before the
  `delivering:` line, add two journal lines beside `delivering_line`, add tests.

**Interfaces:**
- Consumes: `Records::write`, `Report`, `Rewrite` (task 1); `resolve_rewrite`
  (task 2); `Runtime::records` (task 3).
- Produces: `pub fn record_failed_line(path: &str, why: &str) -> String` and
  `pub fn record_not_removed_line(why: &str) -> String`.

- [ ] **Step 1: write the failing tests**

Add to `src/daemon.rs`'s `mod tests`:

```rust
    /// A runtime that recognises `transcript`, rewrites to `rewritten`, records
    /// into a fresh directory, and hands back the journal and that directory.
    fn runtime_recording(
        transcript: &str,
        rewritten: Option<&str>,
        tag: &str,
    ) -> (
        Runtime,
        std::sync::Arc<RecordingJournal>,
        std::path::PathBuf,
    ) {
        let dir = std::env::temp_dir().join(format!(
            "herdr-voice-take-records-{tag}-{}",
            std::process::id()
        ));
        std::fs::remove_dir_all(&dir).ok();
        let (mut runtime, journal) =
            runtime_reading_back(crate::delivery::tests_support::FakeDeliverer::ok(), false);
        runtime.recognition = Ok(Box::new(crate::stt::tests_support::Fake(Ok(transcript
            .to_string()))));
        runtime.skip_if_plain = false;
        runtime.rewrite = match rewritten {
            Some(text) => crate::rewrite::Resolution::Engine(Box::new(
                crate::rewrite::tests_support::Fake(Ok(text.to_string())),
            )),
            None => crate::rewrite::Resolution::Off,
        };
        runtime.records = Some(crate::record::Records::new(dir.clone()));
        (runtime, journal, dir)
    }

    fn only_record_in(dir: &std::path::Path) -> serde_json::Value {
        let mut found: Vec<std::path::PathBuf> = std::fs::read_dir(dir)
            .expect("the records directory exists")
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| path.extension().and_then(|e| e.to_str()) == Some("json"))
            .collect();
        found.sort();
        assert_eq!(found.len(), 1, "exactly one record: {found:?}");
        serde_json::from_str(&std::fs::read_to_string(&found[0]).expect("read record"))
            .expect("valid json")
    }

    #[test]
    fn a_rewritten_take_records_both_stages() {
        let (runtime, _journal, dir) = runtime_recording(
            "fix the worklog entry",
            Some("Fix the worklog entry."),
            "both",
        );
        let take = take_for_recording("both");
        let (reply, _) = transcribe(&runtime, &take, "");
        assert!(matches!(reply, Reply::Ok(_)), "{reply:?}");
        let doc = only_record_in(&dir);
        assert_eq!(doc["transcript"], "fix the worklog entry");
        assert_eq!(doc["rewrite"]["ran"], true);
        assert_eq!(doc["rewrite"]["text"], "Fix the worklog entry.");
        assert_eq!(
            doc["take"],
            take.path
                .file_stem()
                .and_then(|s| s.to_str())
                .expect("a stem")
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn the_key_being_off_writes_neither_stage() {
        let (mut runtime, _journal, dir) = runtime_recording(
            "fix the worklog entry",
            Some("Fix the worklog entry."),
            "off",
        );
        runtime.records = None;
        let take = take_for_recording("off");
        let (reply, _) = transcribe(&runtime, &take, "");
        assert!(matches!(reply, Reply::Ok(_)), "{reply:?}");
        assert!(
            !dir.exists() || std::fs::read_dir(&dir).into_iter().flatten().count() == 0,
            "nothing is written with the key off"
        );
    }

    #[test]
    fn a_record_that_cannot_be_written_does_not_end_the_take() {
        let (mut runtime, journal, dir) =
            runtime_recording("fix the worklog entry", Some("Fix the worklog entry."), "blocked");
        // A file where the directory should be: the record cannot be written and
        // the take must not care.
        std::fs::create_dir_all(&dir).expect("create parent");
        let blocked = dir.join("no-directory-here");
        std::fs::write(&blocked, b"in the way").expect("write blocker");
        runtime.records = Some(crate::record::Records::new(blocked));
        let take = take_for_recording("blocked");
        let (reply, _) = transcribe(&runtime, &take, "");
        assert!(
            matches!(reply, Reply::Ok(_)),
            "the take is still delivered: {reply:?}"
        );
        let lines = journalled(&journal);
        assert!(
            lines.iter().any(|line| line.starts_with("record failed:")),
            "the failure is recorded, not swallowed: {lines:?}"
        );
        assert!(
            lines.iter().any(|line| line.contains("delivering:")),
            "the delivering line still goes out: {lines:?}"
        );
        std::fs::remove_dir_all(&dir).ok();
    }
```

`take_for_recording` builds a `crate::capture::Take` pointing at a `.wav` name in
a scratch directory. There is no such helper today — the one `Take` literal in
`mod tests` is inline at `src/daemon.rs:1958` and belongs to a test about
newlines in a reply — so add this one:

```rust
    fn take_for_recording(tag: &str) -> crate::capture::Take {
        crate::capture::Take {
            path: std::env::temp_dir()
                .join(format!("herdr-voice-take-records-{tag}-{}", std::process::id()))
                .join("1789729477005-4242-1.wav"),
            level_dbfs: -20.0,
            target: "wJ:pE".to_string(),
            agent: None,
            cwd: None,
            tab: None,
        }
    }
```

- [ ] **Step 2: run the tests to verify they fail**

Run: `cargo test daemon::tests::a_rewritten_take_records daemon::tests::the_key_being_off daemon::tests::a_record_that_cannot`
Expected: FAIL — nothing writes a record.

- [ ] **Step 3: write the implementation**

In `transcribe_take`, between the `let text = match &rewrite { ... };` from task 2
and the `delivering:` line at `src/daemon.rs:941`, insert:

```rust
    // Before the delivering line, for the reason that line gives about not
    // holding the text only in memory — and the record is the more durable of
    // the two. A record that cannot be written is reported and never ends the
    // take: `CLAUDE.md` forbids both a panic path and a silent failure.
    if let Some(records) = &runtime.records {
        let report = records.write(&take.path, &transcript, &rewrite);
        if let Err(why) = &report.written {
            runtime.journal.write(&record_failed_line(
                &records.directory().display().to_string().replace('\n', " "),
                &why.replace('\n', " "),
            ));
        }
        for why in &report.not_removed {
            runtime.journal.write(&record_not_removed_line(&why.replace('\n', " ")));
        }
    }
```

and change the two lines task 2 wrote so the transcript is kept:

```rust
    let rewrite = resolve_rewrite(runtime, &text, bias);
    let transcript = text;
    let text = match &rewrite {
        crate::record::Rewrite::Ran { text } => text.clone(),
        _ => transcript.clone(),
    };
```

Add beside `delivery_failed_line`:

```rust
/// Written when a take's record could not be written. The take is delivered
/// regardless; this is what keeps the failure from being silent.
pub fn record_failed_line(directory: &str, why: &str) -> String {
    format!("record failed: directory={directory} reason={why}")
}

/// Written when the fifty-record cap could not remove something. Nothing else
/// happens: a record that outstays its turn is not a reason to end a take.
pub fn record_not_removed_line(why: &str) -> String {
    format!("record not removed: {why}")
}
```

- [ ] **Step 4: run the tests to verify they pass**

Run: `cargo test`
Expected: PASS, whole suite.

- [ ] **Step 5: run the four gates and commit**

```sh
cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt --check && python3 scripts/check_manifest.py
git add src/daemon.rs
git commit -m "Write a take's record before delivery, and report it when it cannot be written

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>"
```

---

### Task 5: doctor names the directory

**Files:**
- Modify: `src/doctor.rs` — add `record_finding`, push it in `run`, add tests.

**Interfaces:**
- Consumes: `config::Record` from task 3.
- Produces: `pub fn record_finding(record: &config::Record, takes: Option<&Path>) -> Finding`.

- [ ] **Step 1: write the failing tests**

Add to `src/doctor.rs`'s `mod tests`:

```rust
    #[test]
    fn recording_off_is_a_default_and_never_a_failure() {
        let finding = record_finding(&config::Record::default(), Some(Path::new("/state/takes")));
        assert_eq!(finding.name, "record");
        assert_eq!(finding.state, State::Default);
        assert!(
            finding.detail.contains("[record] transcripts"),
            "it names the key that turns it on: {}",
            finding.detail
        );
        assert_eq!(exit_code(&[finding]), 0);
    }

    #[test]
    fn recording_on_names_the_directory() {
        let finding = record_finding(
            &config::Record { transcripts: true },
            Some(Path::new("/state/takes")),
        );
        assert_eq!(finding.state, State::Ok);
        assert!(
            finding.detail.contains("/state/takes"),
            "it names where to look: {}",
            finding.detail
        );
        assert_eq!(exit_code(&[finding]), 0);
    }

    #[test]
    fn recording_on_with_nowhere_to_write_says_so_without_failing_the_run() {
        let finding = record_finding(&config::Record { transcripts: true }, None);
        assert_ne!(finding.state, State::Missing);
        assert!(!finding.detail.is_empty());
    }
```

- [ ] **Step 2: run the tests to verify they fail**

Run: `cargo test doctor::tests::recording_`
Expected: FAIL — `record_finding` is not defined.

- [ ] **Step 3: write the implementation**

Add to `src/doctor.rs`, beside `rewrite_finding`:

```rust
/// Where a take's record goes, and whether anything is being written there.
///
/// Never `Missing`: `Missing` is what makes this command exit non-zero
/// (`exit_code`), and a key sitting at its own default is not a fault to go and
/// fix.
pub fn record_finding(record: &config::Record, takes: Option<&Path>) -> Finding {
    if !record.transcripts {
        return Finding {
            name: "record",
            state: State::Default,
            detail: "off; set [record] transcripts = true to keep each take's \
                     transcript and rewrite beside its recording"
                .to_string(),
        };
    }
    match takes {
        Some(directory) => Finding {
            name: "record",
            state: State::Ok,
            detail: format!(
                "on; each take's transcript and rewrite are written to {}, and the \
                 last 50 are kept",
                directory.display()
            ),
        },
        None => Finding {
            name: "record",
            state: State::Default,
            detail: "on, but there is nowhere to write: neither HERDR_PLUGIN_STATE_DIR, \
                     XDG_STATE_HOME nor HOME is set"
                .to_string(),
        },
    }
}
```

and in `run`, after `findings.push(rewrite_finding(&loaded.config.rewrite));`:

```rust
    let takes = transport::state_directory(&transport::Vars::from_env())
        .map(|state| state.join("takes"));
    findings.push(record_finding(&loaded.config.record, takes.as_deref()));
```

Update the module's opening doc comment at `src/doctor.rs:3` from "Six lines in a
fixed order: herdr, daemon, config, engine, model, rewrite." to "Seven lines in a
fixed order: herdr, daemon, config, engine, model, rewrite, record."

- [ ] **Step 4: run the tests to verify they pass**

Run: `cargo test doctor::`
Expected: PASS. If a test asserts the number of findings `run` produces, update it
to seven.

- [ ] **Step 5: run the four gates and commit**

```sh
cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt --check && python3 scripts/check_manifest.py
git add src/doctor.rs
git commit -m "Give doctor a seventh line naming where take records go

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>"
```

---

### Task 6: the documentation

**Files:**
- Modify: `docs/design.md` — the configuration listing in section 7, whose
  `[delivery]` block ends at `docs/design.md:329`; the prose beneath it; and
  section 4's Delivery subsection, which ends at `docs/design.md:173`.
- Modify: `docs/decisions.md` — three rows.

- [ ] **Step 1: add the key to the configuration listing**

In `docs/design.md`, after the `[delivery]` block:

```toml
[record]
transcripts = false       # keep each take's transcript and rewrite on disk
```

- [ ] **Step 2: add the paragraph beneath the listing**

Immediately after the `blink_ms` paragraph and before the keybindings paragraph:

```markdown
`[record] transcripts` is off, and with it off nothing of a take's words is
written to disk. Switched on, each finished take writes one file beside its own
recording — `<state>/takes/<the take's name>.json`, the same name the recording
already has — holding the transcript recognition returned, the text the rewrite
returned, and which of five things the rewrite did: ran, was switched off, was
skipped as a plain phrase, was unavailable, or was called and failed. The last
fifty of those files are kept, and writing a new one removes the oldest beyond
that. `doctor` names the directory. Nothing is sent anywhere: the file is written
and read on the machine that made it, and deleting it, or the directory, is what
removes it.

**`transcripts` here and `transcript` under `[context]` are two different
things.** This key is about what *you* said: the words recognition made of your
speech. `[context] source = "transcript"` is about what the *agent* said — the
session transcript of the conversation in the pane, read to bias recognition
towards terms already on screen. Switching this key on keeps your speech; that
one has never written anything.
```

- [ ] **Step 3: add the sentence to section 4**

At the end of the **Delivery** subsection of section 4, after "Submitting is
opt-in.":

```markdown
What the two text stages produced is not kept unless `[record] transcripts` says
so — section 7.
```

- [ ] **Step 4: add the decisions**

Three rows appended to the table in `docs/decisions.md`, which has three columns —
`Decision | Basis | Where` (`docs/decisions.md:10`). The third column is
`2026-09-18, #84` on all three rows.

| Decision | Basis | Where |
|---|---|---|
| A take's record is a file beside that take's recording, under the same name, and not a line in the herdr plugin log | The plugin log is one ring of 200 records shared by every installed plugin, and this plugin's own `ptt` action writes one per key auto-repeat — 198 of the 200 present, 43 seconds of wall clock. A record put there is gone by the next take, whatever else is true of it. The take's own name is already unique and already carries the time, so nothing has to be invented to tell two takes apart | 2026-09-18, #84 |
| The last fifty records are kept, and the constant is not a configuration key | A key that is off by default exists so that nothing accumulates unasked, and an uncapped file defeats that the moment somebody switches it on and forgets. Fifty is a constant on the footing `SKIP_WORD_LIMIT` is on: no measurement distinguishes one value from another, and a key invites tuning where there is nothing to tune | 2026-09-18, #84 |
| The record names all four ways a rewrite did not run, not the two the issue names | The code has four arms that deliver an unrewritten transcript, and the one the issue omits — an engine called that returned an error — is the only case where two texts existed and the second was thrown away. That is where the record earns its place | 2026-09-18, #84 |

- [ ] **Step 5: run the four gates and commit**

```sh
cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt --check && python3 scripts/check_manifest.py
git add docs/design.md docs/decisions.md
git commit -m "Document [record] transcripts, and separate it from [context] source

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>"
```

---

### Task 7: the run's own artifacts

**Files:**
- Modify: `tasks/84/RUN_84.md` — the S3 and S4 stage blocks with their verdicts.
- Add: `tasks/84/AC_84.md`, `tasks/84/DESIGN_84.md`, `tasks/84/PLAN_84.md` if they
  are not yet committed.

- [ ] **Step 1: commit the run artifacts**

```sh
git add tasks/84
git commit -m "Record the run for #84: criteria, design and plan

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>"
```

---

## Dependencies

```
Task 1 (record module)
  ├── Task 2 (resolve_rewrite)  — needs Rewrite
  └── Task 3 (config key)       — needs Records
        └── Task 4 (write it)   — needs Tasks 1, 2 and 3
Task 3 ──> Task 5 (doctor)      — needs config::Record
Task 6 (docs) and Task 7 (artifacts) — no code dependency; run last
```

Tasks 2 and 3 are independent of each other and both depend only on task 1. Task 4
is the join. Task 5 needs only task 3. Nothing in tasks 6 or 7 touches code.

## Coverage against the criteria

| AC | Where |
|---|---|
| AC-1 both texts, distinguishable | Task 1 step 1, `a_rewritten_take_carries_both_texts_and_says_the_rewrite_ran`; Task 4, `a_rewritten_take_records_both_stages` |
| AC-2 the rewrite's status, four ways | Task 1, `each_way_the_rewrite_did_not_run_is_named_and_the_two_that_have_one_carry_a_reason` |
| AC-3 named per take, never confused | Task 1, `the_record_lands_beside_the_take_under_the_same_stem_and_can_be_read_back` |
| AC-4 reachable | Task 5, `recording_on_names_the_directory` |
| AC-5 off writes nothing, off by default | Task 3, `recording_transcripts_is_off_when_nothing_says_otherwise`; Task 4, `the_key_being_off_writes_neither_stage` |
| AC-6 a default like every other key | Task 3, both tests |
| AC-7 documented in section 7 | Task 6, steps 1 to 3 |
| AC-8 both stages appear | Task 4, `a_rewritten_take_records_both_stages` |
| AC-9 the key off writes neither | Task 4, `the_key_being_off_writes_neither_stage` |
| AC-10 each of the four ways | Task 2, `the_five_rewrite_outcomes_are_each_named` |
| AC-11 nothing is sent anywhere | By construction; checked when the diff is reviewed — nothing added opens a socket or runs a process |
| AC-12 a failed record does not end the take | Task 4, `a_record_that_cannot_be_written_does_not_end_the_take` |

---

# The widening, 2026-09-18

Tasks 8 to 11 implement `DESIGN_84.md` sections 10 to 14, against AC-13 to AC-17.
Tasks 1 to 7 are landed. The global constraints above apply unchanged.

### Task 8: the bound over the directory

**Files:**
- Modify: `src/record.rs` — add `bound`, take `trim` out of `Records::write`,
  drop `Report.not_removed`, move the tests that drove `trim` onto `bound`.

**Interfaces:**
- Consumes: `Records`, `Report`, `KEEP`, `stem` — all present.
- Produces: `pub fn bound(directory: &Path, in_hand: &Path) -> Vec<String>`.
  `Report` keeps `written` and loses `not_removed`. `Records::write` returns the
  same `Report` type with the trim gone.

- [ ] **Step 1: write the failing tests**

In `src/record.rs`'s `mod tests`, add:

```rust
    #[test]
    fn the_bound_keeps_the_newest_fifty_takes_and_removes_both_files_of_an_older_one() {
        let dir = scratch("bound-pairs");
        for n in 0..(KEEP + 3) {
            let name = format!("{}-1-{n}", 1_000_000_000_000u64 + n as u64);
            std::fs::write(dir.join(format!("{name}.wav")), b"audio").expect("wav");
            std::fs::write(dir.join(format!("{name}.json")), b"{}").expect("json");
        }
        let in_hand = dir.join(format!("{}-1-52.wav", 1_000_000_000_052u64));
        let failures = bound(&dir, &in_hand);
        assert!(failures.is_empty(), "{failures:?}");
        let mut stems: Vec<String> = std::fs::read_dir(&dir)
            .expect("read dir")
            .flatten()
            .filter_map(|entry| stem(&entry.path()))
            .collect();
        stems.sort();
        stems.dedup();
        assert_eq!(stems.len(), KEEP, "fifty takes, not fifty files");
        for name in &stems {
            assert!(dir.join(format!("{name}.wav")).exists(), "{name} lost its wav");
            assert!(dir.join(format!("{name}.json")).exists(), "{name} lost its json");
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
        let failures = bound(&dir, Path::new("/nowhere/none.wav"));
        assert!(failures.is_empty(), "{failures:?}");
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
        let failures = bound(&dir, &in_hand);
        assert!(failures.is_empty(), "{failures:?}");
        assert!(
            in_hand.exists(),
            "a clock that stepped backwards must not cost the take in hand its recording"
        );
    }

    #[test]
    fn a_directory_named_like_a_take_is_not_counted_and_not_removed() {
        let dir = scratch("bound-subdir");
        let intruder = dir.join("not-a-take.json");
        std::fs::create_dir_all(&intruder).expect("create intruder");
        for n in 0..(KEEP + 1) {
            let name = format!("{}-1-{n}", 1_000_000_000_000u64 + n as u64);
            std::fs::write(dir.join(format!("{name}.wav")), b"audio").expect("wav");
        }
        let failures = bound(&dir, Path::new("/nowhere/none.wav"));
        assert!(
            failures.is_empty(),
            "a directory must never be counted toward the bound: {failures:?}"
        );
        assert!(intruder.is_dir(), "the intruder is left where it is");
    }

    #[test]
    fn a_directory_that_cannot_be_read_is_not_a_failure() {
        assert!(bound(Path::new("/nowhere/at/all"), Path::new("/nowhere/x.wav")).is_empty());
    }
```

Delete `only_the_newest_fifty_records_are_kept_and_the_audio_is_left_alone`,
`a_record_is_never_removed_by_the_call_that_wrote_it` and
`a_directory_named_like_a_record_is_not_counted_and_not_removed`: all three drive
`trim` through `write` and go vacuous when `write` stops trimming. The five above
replace them. Remove the `assert!(report.not_removed.is_empty())` line from
`the_record_lands_beside_the_take_under_the_same_stem_and_can_be_read_back`.

- [ ] **Step 2: run the tests to verify they fail**

Run: `cargo test record::`
Expected: FAIL — `bound` is not defined.

- [ ] **Step 3: write the implementation**

In `src/record.rs`, replace `Report` and `Records::write`, and delete `trim`:

```rust
/// What a `write` did. One field, because the bound is no longer this type's
/// business: it belongs to the directory and runs whether or not the key is on.
pub struct Report {
    pub written: Result<PathBuf, String>,
}
```

```rust
    pub fn write(&self, take: &Path, transcript: &str, rewrite: &Rewrite) -> Report {
        Report {
            written: self.write_one(take, transcript, rewrite),
        }
    }
```

and add, beside `document`:

```rust
/// Keeps the newest `KEEP` takes in `directory` and removes every file belonging
/// to an older one. Returns what it could not remove, which is reported and is
/// never a reason to end a take.
///
/// A free function over a path rather than a method of `Records`, because the
/// bound applies whether or not `[record] transcripts` is on: with it off, the
/// four endings that keep a recording are the only files there are, and a bound
/// living inside the recording feature would not run for them at all.
///
/// The unit is the take, by file stem, so a take's record and its recording are
/// kept or removed together rather than by two rules that have to agree.
/// Ordering by stem is ordering by time: a take's name begins with unix
/// milliseconds, thirteen digits until the year 2286.
///
/// `in_hand` — the take in the pipeline — is never removed, whatever it sorts as.
/// A clock stepping backwards names it below everything already there, and
/// removing it would take the recording out from under the take being served.
///
/// Entries that are not plain files are ignored. A subdirectory named `x.json`
/// would otherwise take one of the fifty places and fail a removal on every take
/// thereafter.
pub fn bound(directory: &Path, in_hand: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return Vec::new();
    };
    let mut takes: std::collections::BTreeMap<String, Vec<PathBuf>> =
        std::collections::BTreeMap::new();
    for entry in entries.flatten() {
        if !entry.file_type().map(|kind| kind.is_file()).unwrap_or(false) {
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
        let mut gone = true;
        for path in files {
            if let Err(why) = std::fs::remove_file(&path) {
                failures.push(format!("{}: {why}", path.display()));
                gone = false;
            }
        }
        if gone {
            remaining -= 1;
        }
    }
    failures
}
```

`BTreeMap` rather than a sort: it groups and orders in one pass, and its iteration
order is the stem order the doc comment relies on.

- [ ] **Step 4: run the tests to verify they pass**

Run: `cargo test record::`
Expected: PASS. `cargo test` as a whole will still fail to compile until task 9
removes the `not_removed` loop in `src/daemon.rs`; that is the dependency, and the
two tasks land in one commit for the reason recorded in this run for tasks 1 to 4.

---

### Task 9: the directory on `Runtime`, and the bound on every ending

**Files:**
- Modify: `src/daemon.rs` — add `takes: std::path::PathBuf` to `Runtime`, call
  `bound` in `transcribe`, delete the `not_removed` loop, rename
  `record_not_removed_line` to `take_not_removed_line`, add a test.

**Interfaces:**
- Consumes: `crate::record::bound` from task 8.
- Produces: `Runtime::takes`; `pub fn take_not_removed_line(why: &str) -> String`.

- [ ] **Step 1: write the failing test**

```rust
    #[test]
    fn a_take_that_never_reached_delivery_still_has_the_bound_run_for_it() {
        let (mut runtime, _journal, dir) =
            runtime_recording("unused", Some("unused"), "no-recognition");
        runtime.records = None;
        runtime.recognition = Err("no engine is configured".to_string());
        std::fs::create_dir_all(&dir).expect("create the directory");
        // Fifty-one takes' recordings, none of them the one in hand.
        for n in 0..=crate::record::KEEP {
            let name = format!("{}-1-{n}", 1_000_000_000_000u64 + n as u64);
            std::fs::write(dir.join(format!("{name}.wav")), b"audio").expect("wav");
        }
        runtime.takes = dir.clone();
        let take = take_for_recording("no-recognition");
        let (reply, _) = transcribe(&runtime, &take, "");
        assert!(
            matches!(reply, Reply::Error(_)),
            "recognition is unavailable: {reply:?}"
        );
        let left = std::fs::read_dir(&dir).expect("read dir").flatten().count();
        assert_eq!(
            left,
            crate::record::KEEP,
            "the bound runs on the exit that gives up before delivery"
        );
        std::fs::remove_dir_all(&dir).ok();
    }
```

`KEEP` has to become `pub(crate)` in `src/record.rs` for this test to name it.

- [ ] **Step 2: run it to verify it fails**

Run: `cargo test daemon::tests::a_take_that_never_reached_delivery`
Expected: FAIL to compile — `Runtime` has no field `takes`.

- [ ] **Step 3: write the implementation**

Add to `Runtime`, after `records`:

```rust
    /// The directory a take's files live in. Held separately from `records`
    /// because the bound on what accumulates there runs whether or not
    /// `[record] transcripts` is on.
    pub takes: std::path::PathBuf,
```

In `start`, `takes` is bound at `src/daemon.rs:1263` and **moved by value** into
`Recorder::spawn` at `:1288-1292` — its signature is
`spawn<F>(make_source: F, audio: Audio, takes: PathBuf)` (`src/capture.rs:196`),
not a reference — and the `Runtime { ... }` literal at `:1304` comes after that
move. So the clone goes at the call, not at the literal. Change the call to:

```rust
    let recorder = Recorder::spawn(
        || Box::new(crate::capture::cpal_source::CpalSource::new()),
        loaded.config.audio,
        takes.clone(),
    );
```

and add plain `takes,` to the `Runtime { ... }` literal, which then moves the
original. One clone, at the point the value is first needed twice.

Add `takes: std::path::PathBuf::new(),` to every other `Runtime { ... }` literal;
find them with `grep -n "Runtime {" src/daemon.rs`. There are five literals in
all: one in `start`, one in `tests_support` and three in `mod tests`.

In `transcribe`, after the `publish`:

```rust
fn transcribe(runtime: &Runtime, take: &crate::capture::Take, bias: &str) -> (Reply, Reported) {
    let outcome = transcribe_take(runtime, take, bias);
    // Every way out of the pipeline is a take that ended, the two that give up
    // before delivery included. Published here rather than at each return so
    // that a path added later cannot forget it: a take left saying TRANSCR is a
    // token renewed forever and a tab decorated forever.
    publish(runtime, Activity::Idle);
    // The bound on what the takes directory holds, here for the same reason and
    // not inside the pipeline: a daemon with no recognition engine takes the
    // first exit on every take, and a bound placed after delivery would never
    // run while those recordings accumulated.
    for why in crate::record::bound(&runtime.takes, &take.path) {
        runtime
            .journal
            .write(&take_not_removed_line(&why.replace('\n', " ")));
    }
    outcome
}
```

Delete the `for why in &report.not_removed { ... }` loop in `transcribe_take` and
rename the line:

```rust
/// Written when the bound could not remove a take's file — a record or a
/// recording. Nothing else happens: a file that outstays its turn is not a reason
/// to end a take.
pub fn take_not_removed_line(why: &str) -> String {
    format!("take not removed: {why}; remove it by hand if the directory is growing")
}
```

- [ ] **Step 4: run the tests to verify they pass**

Run: `cargo test`
Expected: PASS, whole suite.

- [ ] **Step 5: run the four gates and commit tasks 8 and 9 together**

```sh
cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt --check && python3 scripts/check_manifest.py
git add src/record.rs src/daemon.rs
git commit -m "Bound the takes directory by take, on every ending

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>"
```

---

### Task 10: a delivered take leaves nothing behind

**Files:**
- Modify: `src/daemon.rs` — remove the recording on the delivery-success arm, add
  `recording_kept_line`, add three tests.

**Interfaces:**
- Consumes: `Runtime::records` (task 3), `Runtime::takes` (task 9).
- Produces: `pub fn recording_kept_line(path: &str, why: &str) -> String`.

- [ ] **Step 1: write the failing tests**

```rust
    #[test]
    fn a_delivered_take_leaves_no_recording_when_the_key_is_off() {
        let (mut runtime, _journal, dir) =
            runtime_recording("fix the worklog entry", Some("Fix it."), "gone");
        runtime.records = None;
        runtime.takes = dir.clone();
        std::fs::create_dir_all(&dir).expect("create the directory");
        let take = take_for_recording("gone");
        std::fs::write(&take.path, b"audio").expect("write the recording");
        let (reply, _) = transcribe(&runtime, &take, "");
        assert!(matches!(reply, Reply::Ok(_)), "{reply:?}");
        assert!(!take.path.exists(), "the recording goes with the take");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_delivered_take_keeps_its_recording_when_the_key_is_on() {
        let (mut runtime, _journal, dir) =
            runtime_recording("fix the worklog entry", Some("Fix it."), "kept");
        runtime.takes = dir.clone();
        std::fs::create_dir_all(&dir).expect("create the directory");
        let take = take_for_recording("kept");
        std::fs::write(&take.path, b"audio").expect("write the recording");
        let (reply, _) = transcribe(&runtime, &take, "");
        assert!(matches!(reply, Reply::Ok(_)), "{reply:?}");
        assert!(take.path.exists(), "the recording is kept beside its record");
        assert!(
            take.path.with_extension("json").exists(),
            "and the record is there"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_take_whose_delivery_was_refused_keeps_its_recording_with_the_key_off() {
        let dir = records_dir("refused");
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).expect("create the directory");
        let fake = crate::delivery::tests_support::FakeDeliverer::failing(
            crate::delivery::DeliveryError::Rejected("pane_not_found".into()),
        );
        let (mut runtime, _journal) = runtime_reading_back(fake, false);
        runtime.recognition = Ok(Box::new(crate::stt::tests_support::Fake(Ok(
            "fix the worklog entry".to_string(),
        ))));
        runtime.records = None;
        runtime.takes = dir.clone();
        let take = take_for_recording("refused");
        std::fs::write(&take.path, b"audio").expect("write the recording");
        let (reply, _) = transcribe(&runtime, &take, "");
        assert!(matches!(reply, Reply::Error(_)), "{reply:?}");
        assert!(
            take.path.exists(),
            "the reply names this path; deleting it would point at nothing"
        );
        std::fs::remove_dir_all(&dir).ok();
    }
```

```rust
    #[test]
    fn a_take_that_failed_recognition_keeps_its_recording_with_the_key_off() {
        let (mut runtime, _journal, dir) =
            runtime_recording("unused", Some("unused"), "no-engine");
        runtime.records = None;
        runtime.recognition = Err("no engine is configured".to_string());
        runtime.takes = dir.clone();
        std::fs::create_dir_all(&dir).expect("create the directory");
        let take = take_for_recording("no-engine");
        std::fs::write(&take.path, b"audio").expect("write the recording");
        let (reply, _) = transcribe(&runtime, &take, "");
        assert!(matches!(reply, Reply::Error(_)), "{reply:?}");
        assert!(
            take.path.exists(),
            "the reply names this path; deleting it would point at nothing"
        );
        std::fs::remove_dir_all(&dir).ok();
    }
```

`records_dir` and `take_for_recording` take a tag, so `"refused"` gives this test
a directory of its own. `runtime_recording` is not used by the third test because
it always builds a deliverer that succeeds.

- [ ] **Step 2: run them to verify they fail**

Run: `cargo test daemon::tests::a_delivered_take daemon::tests::a_take_whose_delivery_was_refused`
Expected: the first fails — the recording is still there.

- [ ] **Step 3: write the implementation**

In `transcribe_take`, in the `Ok(())` arm of `crate::delivery::deliver`:

```rust
        Ok(()) => {
            // A take that was read is a take nobody needs to find — the other
            // half of the rule `capture::discard` states. Only here: every other
            // ending names this path to somebody, in a reply or in the journal,
            // and deleting there would turn a promise into a pointer at nothing.
            if runtime.records.is_none() {
                if let Err(why) = std::fs::remove_file(&take.path) {
                    runtime.journal.write(&recording_kept_line(
                        &take.path.display().to_string().replace('\n', " "),
                        &why.to_string().replace('\n', " "),
                    ));
                }
            }
            (
                Reply::Ok(format!(
                    "delivered to {} [{:.1} dB]",
                    take.target, take.level_dbfs
                )),
                Reported::No,
            )
        }
```

and beside `record_failed_line`:

```rust
/// Written when a delivered take's recording could not be removed. The take
/// succeeded and the reply says so; this says the recording is still on disk,
/// why, and what to do about it.
pub fn recording_kept_line(path: &str, why: &str) -> String {
    format!(
        "recording kept: {path} reason={why}; the text was delivered — remove the \\
         file by hand, or set [record] transcripts = true to keep recordings on \\
         purpose"
    )
}
```

- [ ] **Step 4: run the tests to verify they pass**

Run: `cargo test`
Expected: PASS, whole suite.

- [ ] **Step 5: run the four gates and commit**

```sh
cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt --check && python3 scripts/check_manifest.py
git add src/daemon.rs
git commit -m "A delivered take leaves nothing behind unless the key says to keep it

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>"
```

---

### Task 11: the documentation of the wider key

**Files:**
- Modify: `docs/design.md` section 7 — the paragraph beneath the configuration
  listing, and the sentence in section 4.
- Modify: `docs/decisions.md` — two rows.

- [ ] **Step 1: widen the section 7 paragraph**

Replace the paragraph beginning "`[record] transcripts` is off, and with it off
nothing of a take's words is written to disk." with:

```markdown
`[record] transcripts` is off, and with it off a take leaves nothing behind. Its
recording is removed once the text has been delivered, and no transcript is
written anywhere. Switched on, each finished take keeps its recording and writes
one file beside it — `<state>/takes/<the take's name>.json`, the same name the
recording already has — holding the transcript recognition returned, the text the
rewrite returned, and which of five things the rewrite did: ran, was switched off,
was skipped as a plain phrase, was unavailable, or was called and failed.

A take that ended some other way keeps its recording whatever the key says: when
recognition is unavailable, when it fails, when herdr refuses the delivery, and
when the daemon stops with the key still down. Each of those says where the file
is, in the reply or in the plugin's own output, so that it can be recovered by
hand.

The last fifty takes are kept, counted by take rather than by file, so a
recording and its record go together. `doctor` names the directory. Nothing is
sent anywhere: what is written is written and read on the machine that made it,
and deleting the file, or the directory, is what removes it.
```

- [ ] **Step 2: widen the section 4 sentence**

Replace "What the two text stages produced is not kept unless `[record] transcripts`
says so — section 7." with:

```markdown
Neither the recording nor what the two text stages produced is kept unless
`[record] transcripts` says so — section 7.
```

- [ ] **Step 3: add the decisions**

Two rows appended to `docs/decisions.md`, third column `2026-09-18, #84`:

| Decision | Basis | Where |
|---|---|---|
| A delivered take's recording is removed on the delivery-success arm and on no other path | Four other endings name that recording to somebody by path — recognition unavailable, transcription failed, a delivery herdr refused, and the daemon stopping with the key down — and deleting on any of them turns a promise into a pointer at nothing. Delivery succeeding is the only ending where the words reached somewhere a person can see them and nothing named a file. It is the other half of the rule already stated above `capture::discard`: a take nobody will read is a take nobody should find later | 2026-09-18, #84 |
| The bound on the takes directory counts takes by file stem, runs on every ending, and lives outside the record writer | Counting stems is what makes a recording and its record kept or removed together by construction rather than by two rules that have to agree; bytes would delete a different number of takes each time, so nobody could say how far back the record goes. It runs in `transcribe` rather than after delivery because a daemon with no recognition engine takes the first exit on every take, and it lives outside `Records` because that type exists only when the key is on — with it off, the recordings the four keeping endings leave are the only files there are | 2026-09-18, #84 |

- [ ] **Step 4: run the four gates and commit**

```sh
cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt --check && python3 scripts/check_manifest.py
git add docs/design.md docs/decisions.md
git commit -m "Say that the key keeps the recording too, and what keeps its own

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>"
```

---

## Dependencies, tasks 8 to 11

```
Task 8 (bound, and write stops trimming)
  └── Task 9 (Runtime.takes, the call in transcribe, the renamed line)
        — lands in the same commit as task 8: task 8 alone leaves the tree red
Task 9 ──> Task 10 (the deletion on the delivery-success arm)
Task 11 (docs) — no code dependency; run last
```

## Coverage, AC-13 to AC-17

| AC | Where |
|---|---|
| AC-13 delivered take leaves no recording, key off | Task 10, `a_delivered_take_leaves_no_recording_when_the_key_is_off` |
| AC-14 the four other endings keep it | Task 10, `a_take_whose_delivery_was_refused_keeps_its_recording_with_the_key_off` and `a_take_that_failed_recognition_keeps_its_recording_with_the_key_off`. The remaining two — transcription failing, and the daemon stopping with the key down — hold by construction: the deletion lives only inside the `Ok(())` arm of the delivery match, which neither path reaches, and `finish_on_shutdown` never enters the pipeline at all. No existing test asserts the file survives those two, and none is added |
| AC-15 key on keeps the recording beside the record | Task 10, `a_delivered_take_keeps_its_recording_when_the_key_is_on` |
| AC-16 bounded, both kinds, either key state | Task 8, all five tests; Task 9, `a_take_that_never_reached_delivery_still_has_the_bound_run_for_it` |
| AC-17 nothing removed twice | By construction: the deletion sees only a `Take` that `stop_one` returned, and `stop_one` never returns one it discarded. Read off the diff |
