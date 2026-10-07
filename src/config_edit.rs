//! Editing the plugin's configuration file: one key at a time, comments and every
//! other line left alone.
//!
//! The edit is line-oriented because the `toml` crate in the tree parses and does
//! not preserve formatting, and `toml_edit` would be a new dependency for a few
//! lines of text. See `tasks/103/DESIGN_103.md`, section 2.1.

/// One key to set: `[table] key = value`, where `value` is TOML text (`quote`
/// builds it for a string).
pub struct Edit<'a> {
    pub table: &'a str,
    pub key: &'a str,
    pub value: String,
}

/// A TOML basic string on one line. The `toml` crate's own rendering writes a
/// string with a line break over several lines, which a line edit cannot replace
/// (`tasks/103/DESIGN_103_evidence.md`, D2).
pub fn quote(text: &str) -> String {
    let mut out = String::from("\"");
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 || c as u32 == 0x7f => {
                out.push_str(&format!("\\u{:04X}", c as u32))
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// The table a line opens, if it opens one: `[stt] # speech` is the `[stt]`
/// table, and TOML says so.
///
/// A naive `starts_with('[') && ends_with(']')` misses exactly that line, and
/// missing it made the editor append a second `[stt]` table and produce a file
/// that does not parse — which `config::load` then discards whole, losing every
/// other setting in it. Found by an S4 review.
fn table_name(line: &str) -> Option<&str> {
    // A byte order mark some Windows editors put first is not part of the header.
    let line = line
        .trim_start()
        .trim_start_matches('\u{feff}')
        .trim_start();
    let mut rest = line.strip_prefix('[')?;
    // Scan to the closing bracket, ignoring one inside a quoted key.
    let mut quoted = false;
    let mut end = None;
    for (at, c) in rest.char_indices() {
        match c {
            '"' => quoted = !quoted,
            ']' if !quoted => {
                end = Some(at);
                break;
            }
            _ => {}
        }
    }
    let end = end?;
    let name = &rest[..end];
    rest = &rest[end + 1..];
    // Only whitespace or a comment may follow the header.
    let tail = rest.trim_start();
    if tail.is_empty() || tail.starts_with('#') {
        Some(name.trim())
    } else {
        None
    }
}

/// `existing` with every edit applied, in order. Comments, other keys and other
/// tables are left alone, and so are the line endings: a file that uses `\r\n`
/// keeps them.
pub fn set_keys(existing: &str, edits: &[Edit]) -> String {
    let eol = if existing.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    let mut text = existing.to_string();
    for edit in edits {
        text = set_one(&text, edit, eol);
    }
    text
}

/// One edit: replace the first `key =` line inside `[table]`, or add the line at
/// the end of that table, or add the table at the end of the file.
fn set_one(existing: &str, edit: &Edit, eol: &str) -> String {
    let line = format!("{} = {}", edit.key, edit.value);
    let mut out: Vec<String> = Vec::new();
    let mut in_table = false;
    let mut wrote = false;
    let mut saw_table = false;

    for text in existing.lines() {
        let trimmed = text.trim();
        if let Some(name) = table_name(text) {
            // Leaving the table without having found the key: add it at its end.
            if in_table && !wrote {
                out.push(line.clone());
                wrote = true;
            }
            in_table = name == edit.table;
            saw_table |= in_table;
            out.push(text.to_string());
            continue;
        }
        // Only a key inside the named table. Another table may have a key of the
        // same name (`[rewrite]` and `[stt]` both have `model`), and editing the
        // wrong one would silently repoint something else.
        if in_table && !wrote {
            if let Some(rest) = trimmed.strip_prefix(edit.key) {
                if rest.trim_start().starts_with('=') {
                    out.push(line.clone());
                    wrote = true;
                    continue;
                }
            }
        }
        out.push(text.to_string());
    }

    if in_table && !wrote {
        out.push(line.clone());
        wrote = true;
    }
    if !saw_table {
        if !out.is_empty() && !out.last().is_some_and(|l| l.trim().is_empty()) {
            out.push(String::new());
        }
        out.push(format!("[{}]", edit.table));
        out.push(line);
    } else if !wrote {
        out.push(line);
    }

    // A file that did not end with a newline keeps ending without one, unless the
    // last line is the one that was written.
    let unchanged_last = out.last().map(String::as_str) == existing.lines().last();
    let had_no_final_newline = !existing.is_empty() && !existing.ends_with('\n');
    let mut text = out.join(eol);
    if !(had_no_final_newline && unchanged_last) {
        text.push_str(eol);
    }
    text
}

use std::path::{Path, PathBuf};

use crate::config::{self, Config};

/// Why a write did not happen, each with what to do next. In every case but `Io`
/// the file on disk is exactly what it was.
#[derive(Debug, PartialEq, Eq)]
pub enum WriteError {
    /// No configuration directory can be worked out from the environment.
    NoDirectory,
    /// The file exists and cannot be read as text.
    Unreadable { path: String, why: String },
    /// The file as it stands does not load as the configuration, so the daemon is
    /// already running on defaults and an edit would be blamed for it.
    AlreadyInvalid { path: String, why: String },
    /// The edit would have made the file unloadable.
    Refused {
        path: String,
        edit: String,
        why: String,
    },
    /// A step of creating or replacing the file failed.
    Io { path: String, why: String },
}

impl std::fmt::Display for WriteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WriteError::NoDirectory => write!(
                f,
                "there is no configuration directory to write to: none of \
                 HERDR_PLUGIN_CONFIG_DIR, XDG_CONFIG_HOME and HOME is set. Set one and try again"
            ),
            WriteError::Unreadable { path, why } => write!(
                f,
                "cannot read {path} as text ({why}), so nothing was written. \
                 Fix the file's permissions or contents and try again"
            ),
            WriteError::AlreadyInvalid { path, why } => write!(
                f,
                "{path} does not load as a configuration ({why}), so nothing was written. \
                 Fix the file and try again"
            ),
            WriteError::Refused { path, edit, why } => write!(
                f,
                "the edit ({edit}) would have made {path} unloadable ({why}), so nothing was \
                 written. If the key is written in a form this editor does not read, such as \
                 an inline table or a dotted key, change it in the file yourself"
            ),
            WriteError::Io { path, why } => write!(f, "cannot write {path}: {why}"),
        }
    }
}

impl WriteError {
    /// Whether the file could not be written at all, so the person has to put the
    /// line in by hand. A refusal is different: the same line, added by hand, would
    /// make the same file unloadable.
    pub fn needs_hand_edit(&self) -> bool {
        matches!(self, WriteError::NoDirectory | WriteError::Io { .. })
    }
}

/// Write the file with `edits` applied, and return its path.
///
/// Never leaves a file that does not load: `config::load` treats an unparsable
/// file as absent and falls back to every default, so a bad edit would silently
/// discard every other setting the person has. The edited text is therefore parsed
/// as `Config`, the type `config::load` parses, before anything is written.
///
/// The file is replaced whole, by writing a candidate beside it and renaming it
/// over the target: a write in place that fails halfway leaves an empty file,
/// which loads as "every default". A symbolic link is followed, so the link
/// survives and the file it points at changes; the file's permissions carry over,
/// because it may hold a token.
pub fn write_keys(directory: Option<&Path>, edits: &[Edit]) -> Result<PathBuf, WriteError> {
    let directory = directory.ok_or(WriteError::NoDirectory)?;
    let io = |path: &Path, e: std::io::Error| WriteError::Io {
        path: path.display().to_string(),
        why: e.to_string(),
    };
    std::fs::create_dir_all(directory).map_err(|e| io(directory, e))?;
    let path = directory.join(config::FILE_NAME);
    let shown = path.display().to_string();
    let target = std::fs::canonicalize(&path).unwrap_or_else(|_| path.clone());

    // An absent file is a valid state, so that is a create. Any other failure to
    // read is not "empty": treating it so would replace the person's file with one
    // holding a single key.
    let original = match std::fs::read_to_string(&target) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => {
            return Err(WriteError::Unreadable {
                path: shown,
                why: e.to_string(),
            })
        }
    };
    if let Err(e) = toml::from_str::<Config>(&original) {
        return Err(WriteError::AlreadyInvalid {
            path: shown,
            why: e.message().to_string(),
        });
    }

    let edited = set_keys(&original, edits);
    if let Err(e) = toml::from_str::<Config>(&edited) {
        // The value is shown: the edits this writer is given are never secrets (a
        // key that holds one must not be passed here without redacting it).
        let edit = edits
            .iter()
            .map(|e| format!("[{}] {} = {}", e.table, e.key, e.value))
            .collect::<Vec<_>>()
            .join(", ");
        return Err(WriteError::Refused {
            path: shown,
            edit,
            why: e.message().to_string(),
        });
    }

    let candidate = target.with_extension("toml.herdr-voice-candidate");
    write_candidate(&candidate, &edited).map_err(|e| {
        let _ = std::fs::remove_file(&candidate);
        io(&path, e)
    })?;
    // The target exists when it was read: carry its mode onto the candidate, so
    // the rename does not change who may read it.
    if let Ok(meta) = std::fs::metadata(&target) {
        let _ = std::fs::set_permissions(&candidate, meta.permissions());
    }
    std::fs::rename(&candidate, &target).map_err(|e| {
        let _ = std::fs::remove_file(&candidate);
        io(&path, e)
    })?;
    Ok(path)
}

/// Writes `text` to a new file that only its owner can read until the target's
/// mode replaces that: the text may contain a token.
fn write_candidate(path: &Path, text: &str) -> std::io::Result<()> {
    use std::io::Write;
    // A candidate left behind by an earlier run that died is removed, and the new one
    // is created exclusively: it cannot be a stale longer file written over, or a
    // link someone put there.
    let _ = std::fs::remove_file(path);
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)?.write_all(text.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `[stt] model = "<id>"`, the edit the model chooser makes.
    fn model(existing: &str, id: &str) -> String {
        set_keys(
            existing,
            &[Edit {
                table: "stt",
                key: "model",
                value: quote(id),
            }],
        )
    }

    #[test]
    fn a_file_with_no_stt_table_gains_one() {
        let out = model("[audio]\ninput = \"\"\n", "tiny");
        assert!(out.contains("[stt]"), "got {out}");
        assert!(out.contains("model = \"tiny\""), "got {out}");
        assert!(
            out.contains("[audio]"),
            "it must not lose what was there: {out}"
        );
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
        let out = model(before, "small");
        assert!(out.contains("model = \"small\""), "got {out}");
        assert!(
            !out.contains("large-v3-turbo"),
            "the old value must go: {out}"
        );
        assert!(
            out.contains("# which model to use"),
            "comments must survive: {out}"
        );
        assert!(
            out.contains("language = \"ru\""),
            "other keys must survive: {out}"
        );
        assert!(
            out.contains("toasts = false"),
            "other tables must survive: {out}"
        );
        let parsed: toml::Value = toml::from_str(&out).expect("must remain valid TOML");
        assert_eq!(parsed["stt"]["model"].as_str(), Some("small"));
        assert_eq!(parsed["ui"]["toasts"].as_bool(), Some(false));
    }

    #[test]
    fn a_stt_table_with_no_model_key_gains_one_inside_itself() {
        let out = model("[stt]\nlanguage = \"ru\"\n\n[ui]\ntoasts = true\n", "base");
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
        let out = model(before, "small");
        let parsed: toml::Value = toml::from_str(&out).expect("must remain valid TOML");
        assert_eq!(
            parsed["rewrite"]["model"].as_str(),
            Some("haiku"),
            "got {out}"
        );
        assert_eq!(parsed["stt"]["model"].as_str(), Some("small"), "got {out}");
    }

    #[test]
    fn a_table_header_with_a_comment_after_it_is_still_that_table() {
        // Found by an S4 review: `[stt] # speech` matched neither the header test
        // nor the name test, so a second [stt] table was appended and the file
        // stopped parsing — which config::load turns into "every setting lost".
        let before =
            "[rewrite]\nmodel = \"haiku\"\n\n[stt] # speech\nmodel = \"tiny\"\nlanguage = \"ru\"\n";
        let out = model(before, "small");
        let parsed: toml::Value = toml::from_str(&out).expect("must remain valid TOML");
        assert_eq!(parsed["stt"]["model"].as_str(), Some("small"), "got {out}");
        assert_eq!(parsed["stt"]["language"].as_str(), Some("ru"));
        assert_eq!(parsed["rewrite"]["model"].as_str(), Some("haiku"));
        assert_eq!(out.matches("[stt]").count(), 1, "one table, not two: {out}");
        assert!(out.contains("# speech"), "the comment must survive: {out}");
    }

    #[test]
    fn headers_are_recognised_whatever_surrounds_them() {
        assert_eq!(table_name("[stt]"), Some("stt"));
        assert_eq!(table_name("  [stt]  "), Some("stt"));
        assert_eq!(table_name("[stt] # speech"), Some("stt"));
        assert_eq!(table_name("[ stt ]"), Some("stt"));
        assert_eq!(table_name("[a.b]"), Some("a.b"));
        // Not headers.
        assert_eq!(table_name("model = \"tiny\""), None);
        assert_eq!(table_name("# [stt]"), None);
        assert_eq!(table_name("[stt] model = 1"), None);
        assert_eq!(table_name(""), None);
    }

    #[test]
    fn spaces_inside_the_header_still_name_the_table() {
        let out = model("[ stt ]\nlanguage = \"ru\"\n", "base");
        let parsed: toml::Value = toml::from_str(&out).expect("must remain valid TOML");
        assert_eq!(parsed["stt"]["model"].as_str(), Some("base"), "got {out}");
        assert_eq!(out.matches("stt").count(), 1, "one table, not two: {out}");
    }

    #[test]
    fn an_empty_file_becomes_a_valid_one() {
        let out = model("", "tiny");
        let parsed: toml::Value = toml::from_str(&out).expect("must remain valid TOML");
        assert_eq!(parsed["stt"]["model"].as_str(), Some("tiny"));
    }

    #[test]
    fn a_stt_table_that_is_the_last_one_still_gains_the_key_inside_itself() {
        // The "leaving [stt]" branch never fires when [stt] is last, so the
        // after-the-loop branch is the one that has to work.
        let out = model(
            "[audio]\ninput = \"\"\n\n[stt]\nlanguage = \"en\"\n",
            "base",
        );
        let parsed: toml::Value = toml::from_str(&out).expect("must remain valid TOML");
        assert_eq!(parsed["stt"]["model"].as_str(), Some("base"));
        assert_eq!(parsed["stt"]["language"].as_str(), Some("en"));
        assert_eq!(parsed["audio"]["input"].as_str(), Some(""));
    }

    #[test]
    fn quote_writes_one_line_that_parses_back_equal() {
        for text in [
            "MacBook Pro Microphone",
            "Mic \"A\" \\ B",
            "Микрофон é 🎙",
            "line\nbreak\ttab\r",
            "bell\u{7} and delete\u{7f}",
            "",
        ] {
            let quoted = quote(text);
            assert!(!quoted.contains('\n'), "one line, got {quoted:?}");
            let parsed: toml::Value = toml::from_str(&format!("k = {quoted}\n"))
                .unwrap_or_else(|e| panic!("{quoted:?} must parse: {e}"));
            assert_eq!(parsed["k"].as_str(), Some(text), "round trip of {text:?}");
        }
    }

    #[test]
    fn a_name_with_quotes_and_a_backslash_is_written_and_read_back() {
        let out = set_keys(
            "[audio]\ninput = \"Old\"\n",
            &[Edit {
                table: "audio",
                key: "input",
                value: quote("Mic \"A\" \\ B"),
            }],
        );
        let parsed: toml::Value = toml::from_str(&out).expect("must stay valid TOML");
        assert_eq!(parsed["audio"]["input"].as_str(), Some("Mic \"A\" \\ B"));
    }

    #[test]
    fn two_edits_in_one_call_both_land() {
        let out = set_keys(
            "[stt]\nlanguage = \"ru\"\n",
            &[
                Edit {
                    table: "stt",
                    key: "model",
                    value: quote("tiny"),
                },
                Edit {
                    table: "stt",
                    key: "engine",
                    value: quote("candle"),
                },
            ],
        );
        let parsed: toml::Value = toml::from_str(&out).expect("must stay valid TOML");
        assert_eq!(parsed["stt"]["model"].as_str(), Some("tiny"), "got {out}");
        assert_eq!(
            parsed["stt"]["engine"].as_str(),
            Some("candle"),
            "got {out}"
        );
        assert_eq!(parsed["stt"]["language"].as_str(), Some("ru"), "got {out}");
    }

    #[test]
    fn a_key_of_the_same_name_in_another_table_is_not_touched_in_either_direction() {
        let before = "[stt]\nmodel = \"a\"\n\n[rewrite]\nmodel = \"b\"\n";
        let out = set_keys(
            before,
            &[Edit {
                table: "rewrite",
                key: "model",
                value: quote("c"),
            }],
        );
        let parsed: toml::Value = toml::from_str(&out).unwrap();
        assert_eq!(parsed["stt"]["model"].as_str(), Some("a"), "got {out}");
        assert_eq!(parsed["rewrite"]["model"].as_str(), Some("c"), "got {out}");
        let out = model(before, "d");
        let parsed: toml::Value = toml::from_str(&out).unwrap();
        assert_eq!(parsed["stt"]["model"].as_str(), Some("d"), "got {out}");
        assert_eq!(parsed["rewrite"]["model"].as_str(), Some("b"), "got {out}");
    }

    #[test]
    fn windows_line_endings_survive_and_every_other_line_is_byte_identical() {
        let before = "# mine\r\n[audio]\r\ninput = \"Old\"\r\nsilence_db = -50.0\r\n\r\n[ui]\r\ntoasts = false\r\n";
        let out = set_keys(
            before,
            &[Edit {
                table: "audio",
                key: "input",
                value: quote("New"),
            }],
        );
        assert_eq!(
            out,
            before.replace("input = \"Old\"", "input = \"New\""),
            "only the one line may differ"
        );
    }

    #[test]
    fn an_edit_changes_one_line_and_nothing_else() {
        let before = "# my configuration\n[audio]\n# the input\ninput = \"Old\"\nsilence_db = -50.0\n\n[ui]\ntoasts = false\n";
        let out = set_keys(
            before,
            &[Edit {
                table: "audio",
                key: "input",
                value: quote("New"),
            }],
        );
        assert_eq!(out, before.replace("input = \"Old\"", "input = \"New\""));
    }

    fn scratch(tag: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("herdr-voice-edit-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn input(name: &str) -> Edit<'static> {
        Edit {
            table: "audio",
            key: "input",
            value: quote(name),
        }
    }

    const ORIGINAL: &str =
        "# mine\n[audio]\ninput = \"Old\"\nsilence_db = -50.0\n\n[ui]\ntoasts = false\n";

    #[test]
    fn a_good_edit_is_written_and_every_other_byte_is_unchanged() {
        let dir = scratch("good");
        std::fs::write(dir.join("config.toml"), ORIGINAL).unwrap();
        let path = write_keys(Some(&dir), &[input("New")]).expect("written");
        assert_eq!(path, dir.join("config.toml"));
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            ORIGINAL.replace("input = \"Old\"", "input = \"New\"")
        );
        let leftovers: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
            .collect();
        assert_eq!(
            leftovers,
            vec!["config.toml".to_string()],
            "no candidate left"
        );
    }

    #[test]
    fn a_value_of_the_wrong_type_is_refused_and_the_file_is_untouched() {
        // `toml::Value` accepts this and `config::load` does not, which makes
        // `config::load` fall back to every default.
        let dir = scratch("wrong-type");
        let before = "[ui]\nblink_ms = 250\n[audio]\ninput = \"Old\"\n";
        std::fs::write(dir.join("config.toml"), before).unwrap();
        let wrong = Edit {
            table: "ui",
            key: "blink_ms",
            value: quote("fast"),
        };
        let error = write_keys(Some(&dir), &[wrong]).expect_err("must be refused");
        assert!(matches!(error, WriteError::Refused { .. }), "got {error:?}");
        let said = error.to_string();
        assert!(said.contains("config.toml"), "names the file: {said}");
        assert!(said.contains("nothing was written"), "{said}");
        assert_eq!(
            std::fs::read_to_string(dir.join("config.toml")).unwrap(),
            before
        );
    }

    #[test]
    fn a_file_that_already_does_not_load_is_not_edited() {
        let dir = scratch("already-invalid");
        let before = "[audio\ninput = ";
        std::fs::write(dir.join("config.toml"), before).unwrap();
        let error = write_keys(Some(&dir), &[input("New")]).expect_err("refused");
        assert!(
            matches!(error, WriteError::AlreadyInvalid { .. }),
            "got {error:?}"
        );
        assert!(error.to_string().contains("nothing was written"));
        assert_eq!(
            std::fs::read_to_string(dir.join("config.toml")).unwrap(),
            before
        );
    }

    #[test]
    fn a_file_that_is_not_text_is_not_replaced_by_one_holding_the_new_key() {
        let dir = scratch("not-text");
        let bytes = [0xffu8, 0xfe, 0x00, 0x41];
        std::fs::write(dir.join("config.toml"), bytes).unwrap();
        let error = write_keys(Some(&dir), &[input("New")]).expect_err("refused");
        assert!(
            matches!(error, WriteError::Unreadable { .. }),
            "got {error:?}"
        );
        assert_eq!(std::fs::read(dir.join("config.toml")).unwrap(), bytes);
    }

    #[test]
    fn no_file_and_no_directory_gives_both_and_only_the_edited_key() {
        let dir = scratch("absent").join("nested").join("deeper");
        let path = write_keys(Some(&dir), &[input("New")]).expect("created");
        assert_eq!(
            std::fs::read_to_string(path).unwrap(),
            "[audio]\ninput = \"New\"\n"
        );
    }

    #[test]
    fn no_known_directory_says_so() {
        let error = write_keys(None, &[input("New")]).expect_err("nowhere to write");
        assert_eq!(error, WriteError::NoDirectory);
        assert!(error.to_string().contains("HERDR_PLUGIN_CONFIG_DIR"));
    }

    #[test]
    fn a_directory_that_cannot_be_created_is_an_io_error_naming_it() {
        let dir = scratch("blocked");
        let in_the_way = dir.join("occupied");
        std::fs::write(&in_the_way, "a file, not a directory").unwrap();
        let error = write_keys(Some(&in_the_way.join("config")), &[input("New")])
            .expect_err("cannot create");
        assert!(matches!(error, WriteError::Io { .. }), "got {error:?}");
        assert!(error.to_string().contains("occupied"), "{error}");
    }

    #[test]
    fn one_refused_edit_among_two_changes_neither() {
        let dir = scratch("all-or-nothing");
        std::fs::write(dir.join("config.toml"), ORIGINAL).unwrap();
        let edits = [
            input("New"),
            Edit {
                table: "ui",
                key: "blink_ms",
                value: quote("fast"),
            },
        ];
        write_keys(Some(&dir), &edits).expect_err("refused as a whole");
        assert_eq!(
            std::fs::read_to_string(dir.join("config.toml")).unwrap(),
            ORIGINAL
        );
        let both = [
            input("New"),
            Edit {
                table: "ui",
                key: "toasts",
                value: "true".to_string(),
            },
        ];
        write_keys(Some(&dir), &both).expect("both written");
        let after = std::fs::read_to_string(dir.join("config.toml")).unwrap();
        assert!(
            after.contains("input = \"New\"") && after.contains("toasts = true"),
            "{after}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_read_only_directory_is_an_io_error_and_leaves_the_file_and_no_candidate() {
        use std::os::unix::fs::PermissionsExt;
        let dir = scratch("read-only");
        std::fs::write(dir.join("config.toml"), ORIGINAL).unwrap();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o500)).unwrap();
        let result = write_keys(Some(&dir), &[input("New")]);
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
        let error = result.expect_err("cannot write");
        assert!(matches!(error, WriteError::Io { .. }), "got {error:?}");
        assert_eq!(
            std::fs::read_to_string(dir.join("config.toml")).unwrap(),
            ORIGINAL
        );
        assert_eq!(
            std::fs::read_dir(&dir).unwrap().count(),
            1,
            "no candidate left"
        );
    }

    #[cfg(unix)]
    #[test]
    fn the_mode_of_the_file_survives_because_it_may_hold_a_token() {
        use std::os::unix::fs::PermissionsExt;
        let dir = scratch("mode");
        let file = dir.join("config.toml");
        std::fs::write(&file, ORIGINAL).unwrap();
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o640)).unwrap();
        write_keys(Some(&dir), &[input("New")]).expect("written");
        let mode = std::fs::metadata(&file).unwrap().permissions().mode() & 0o777;
        // The candidate is created with 0600, so only copying the original's mode gives 0640.
        assert_eq!(mode, 0o640);
    }

    #[cfg(unix)]
    #[test]
    fn a_symbolic_link_stays_a_link_and_the_file_it_points_at_changes() {
        let dir = scratch("link");
        let real_dir = scratch("link-real");
        let real = real_dir.join("dotfiles-config.toml");
        std::fs::write(&real, ORIGINAL).unwrap();
        std::os::unix::fs::symlink(&real, dir.join("config.toml")).unwrap();
        write_keys(Some(&dir), &[input("New")]).expect("written");
        assert!(
            std::fs::symlink_metadata(dir.join("config.toml"))
                .unwrap()
                .file_type()
                .is_symlink(),
            "the link must still be a link"
        );
        assert!(std::fs::read_to_string(&real)
            .unwrap()
            .contains("input = \"New\""));
    }

    #[test]
    fn a_refusal_names_the_key_and_the_value_it_would_have_written() {
        let dir = scratch("names-the-edit");
        std::fs::write(dir.join("config.toml"), "[ui]\nblink_ms = 250\n").unwrap();
        let wrong = Edit {
            table: "ui",
            key: "blink_ms",
            value: quote("fast"),
        };
        let said = write_keys(Some(&dir), &[wrong]).unwrap_err().to_string();
        assert!(said.contains("[ui] blink_ms = \"fast\""), "{said}");
    }

    #[test]
    fn only_a_failure_to_write_asks_for_a_hand_edit() {
        let path = || "config.toml".to_string();
        let why = || "x".to_string();
        assert!(WriteError::NoDirectory.needs_hand_edit());
        assert!(WriteError::Io {
            path: path(),
            why: why()
        }
        .needs_hand_edit());
        assert!(!WriteError::Refused {
            path: path(),
            edit: why(),
            why: why()
        }
        .needs_hand_edit());
        assert!(!WriteError::AlreadyInvalid {
            path: path(),
            why: why()
        }
        .needs_hand_edit());
        assert!(!WriteError::Unreadable {
            path: path(),
            why: why()
        }
        .needs_hand_edit());
    }

    #[test]
    fn a_byte_order_mark_before_the_first_table_does_not_hide_it() {
        let dir = scratch("bom");
        let before = "\u{feff}[audio]\r\ninput = \"Old\"\r\n";
        std::fs::write(dir.join("config.toml"), before).unwrap();
        write_keys(Some(&dir), &[input("New")]).expect("written");
        assert_eq!(
            std::fs::read_to_string(dir.join("config.toml")).unwrap(),
            before.replace("input = \"Old\"", "input = \"New\"")
        );
    }

    #[test]
    fn a_file_with_no_final_newline_does_not_gain_one_on_a_line_that_was_not_edited() {
        let before = "[audio]\ninput = \"Old\"\n[ui]\ntoasts = false";
        let out = set_keys(
            before,
            &[Edit {
                table: "audio",
                key: "input",
                value: quote("New"),
            }],
        );
        assert_eq!(out, "[audio]\ninput = \"New\"\n[ui]\ntoasts = false");
    }

    #[cfg(unix)]
    #[test]
    fn a_file_that_did_not_exist_is_created_readable_by_its_owner_only() {
        use std::os::unix::fs::PermissionsExt;
        let dir = scratch("new-file-mode");
        write_keys(Some(&dir), &[input("New")]).expect("created");
        let mode = std::fs::metadata(dir.join("config.toml"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600, "the file may come to hold a token");
    }

    #[test]
    fn a_candidate_left_by_a_run_that_died_does_not_stop_the_next_one() {
        let dir = scratch("stale-candidate");
        std::fs::write(dir.join("config.toml"), ORIGINAL).unwrap();
        std::fs::write(
            dir.join("config.toml.herdr-voice-candidate"),
            "left behind, and longer than the text that replaces it ".repeat(20),
        )
        .unwrap();
        write_keys(Some(&dir), &[input("New")]).expect("written");
        assert_eq!(
            std::fs::read_to_string(dir.join("config.toml")).unwrap(),
            ORIGINAL.replace("input = \"Old\"", "input = \"New\"")
        );
        assert_eq!(
            std::fs::read_dir(&dir).unwrap().count(),
            1,
            "no candidate left"
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_failure_while_writing_names_the_configuration_not_the_candidate() {
        use std::os::unix::fs::PermissionsExt;
        let dir = scratch("names-the-config");
        std::fs::write(dir.join("config.toml"), ORIGINAL).unwrap();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o500)).unwrap();
        let result = write_keys(Some(&dir), &[input("New")]);
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
        let said = result.unwrap_err().to_string();
        assert!(said.contains("config.toml"), "{said}");
        assert!(!said.contains("candidate"), "{said}");
    }
}
