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
/// Presence is judged by `store::glance`, not `store::locate`: listing six
/// models must not hash six multi-gigabyte files. A person asking what is
/// installed is asking a question about names and sizes, and waiting a minute
/// for the answer would be absurd. The full check happens where it matters —
/// before a model is loaded, and again right after a download.
fn list(models: &Path, configured: &str) -> String {
    let mut out = String::from("Speech models this plugin can install:\n\n");
    for (i, entry) in catalogue::MODELS.iter().enumerate() {
        let size = human(catalogue::weights(entry).bytes);
        let present = match store::glance(models, entry.identifier, Some(entry)) {
            store::Glance::Whole => "installed",
            store::Glance::Absent => "not installed",
            store::Glance::WrongSize => "installed, but the wrong size",
        };
        let marker = if entry.identifier == configured {
            "  (current)"
        } else {
            ""
        };
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
        // A zero total means the catalogue said the file is empty, which it
        // never does; checked_div keeps that from being a division by zero
        // anyway, because a progress line is not worth a panic.
        let percent = (done * 100).checked_div(self.total).unwrap_or(0);
        print!("\r  {} {percent:>3}%  ", self.name);
        let _ = std::io::stdout().flush();
    }
    fn done(&mut self, name: &str) {
        println!("\r  {name} done            ");
    }
}

/// `[stt] model = "<identifier>"`, put into a configuration file's text without
/// disturbing anything else in it.
fn set_model_key(existing: &str, identifier: &str) -> String {
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
        // Only a `model` key inside [stt]. [rewrite] has one too, and editing
        // that one would silently repoint the rewrite engine.
        if in_stt && !wrote {
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

/// `herdr-voice model`, and `--choose`. Returns the process's exit code.
///
/// The models directory is resolved here rather than passed in, the same way
/// `doctor::run` does it, because this is the outermost layer: `list`, `pick`
/// and `set_model_key` all take what they need and are testable without an
/// environment.
pub fn run(choosing: bool) -> u8 {
    let models = match crate::transport::state_directory(&crate::transport::Vars::from_env()) {
        Some(state) => state.join("models"),
        None => {
            eprintln!(
                "cannot tell where models live: neither HERDR_PLUGIN_STATE_DIR nor a \
                 home directory is set, so there is nowhere to put one. Set \
                 HERDR_PLUGIN_STATE_DIR and try again"
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

    println!(
        "Installing {} ({})",
        entry.identifier,
        human(catalogue::weights(entry).bytes)
    );
    let mut line = Line {
        name: String::new(),
        total: 0,
    };
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
            println!(
                "{} is installed, and {} now names it.",
                entry.identifier,
                path.display()
            );
            println!("The daemon still holds the previous model. Restart it to use this one.");
            0
        }
        Err(why) => {
            // The model is on disk and good; only the configuration edit failed,
            // so the person needs one line, not another three gigabytes.
            eprintln!(
                "{} is installed, but the configuration could not be written: {why}",
                entry.identifier
            );
            eprintln!("Add this to [stt] in your configuration file by hand:");
            eprintln!("  model = \"{}\"", entry.identifier);
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_listing_shows_every_model_with_a_size_before_anything_is_downloaded() {
        let text = list(
            &std::path::PathBuf::from("/nowhere-at-all"),
            "large-v3-turbo",
        );
        for entry in catalogue::MODELS.iter() {
            assert!(
                text.contains(entry.identifier),
                "{} is missing: {text}",
                entry.identifier
            );
        }
        assert!(text.contains("151 MB"), "tiny's size: {text}");
        assert!(text.contains("1.62 GB"), "the default's size: {text}");
        assert!(
            text.contains("not installed"),
            "it must say what is there: {text}"
        );
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
        let out = set_model_key(before, "small");
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
        assert_eq!(
            parsed["rewrite"]["model"].as_str(),
            Some("haiku"),
            "got {out}"
        );
        assert_eq!(parsed["stt"]["model"].as_str(), Some("small"), "got {out}");
    }

    #[test]
    fn an_empty_file_becomes_a_valid_one() {
        let out = set_model_key("", "tiny");
        let parsed: toml::Value = toml::from_str(&out).expect("must remain valid TOML");
        assert_eq!(parsed["stt"]["model"].as_str(), Some("tiny"));
    }

    #[test]
    fn a_stt_table_that_is_the_last_one_still_gains_the_key_inside_itself() {
        // The "leaving [stt]" branch never fires when [stt] is last, so the
        // after-the-loop branch is the one that has to work.
        let out = set_model_key(
            "[audio]\ninput = \"\"\n\n[stt]\nlanguage = \"en\"\n",
            "base",
        );
        let parsed: toml::Value = toml::from_str(&out).expect("must remain valid TOML");
        assert_eq!(parsed["stt"]["model"].as_str(), Some("base"));
        assert_eq!(parsed["stt"]["language"].as_str(), Some("en"));
        assert_eq!(parsed["audio"]["input"].as_str(), Some(""));
    }
}
