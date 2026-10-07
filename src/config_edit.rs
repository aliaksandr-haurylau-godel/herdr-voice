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
    let line = line.trim_start();
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

    let mut text = out.join(eol);
    text.push_str(eol);
    text
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
        assert_eq!(parsed["stt"]["engine"].as_str(), Some("candle"), "got {out}");
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
}
