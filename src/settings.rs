//! The settings popup: every section and key of the configuration, the microphone,
//! the speech model and the rewrite model, from one place.
//!
//! The list of keys is read from the configuration type (`Serialize` is derived on
//! it), so a field added there is listed with no other change. Every flow talks to a
//! `World` (`crate::popup`), so it is tested with no file, daemon, device, download or
//! server. See `tasks/104/DESIGN_104.md`.

use toml::Value;

use crate::config::Config;
use crate::config_edit;
use crate::popup::{
    config_note, not_in_list, parse_answer, pause, save_and_tell, Answer, Io, Real, World,
    LEAVE_HINT, OPEN_BOUND,
};
use crate::stt::catalogue;

/// The sections in the order a person meets them. A test fails when the serialised
/// defaults hold a section that is not here, and when this names one they lack.
const ORDER: [&str; 8] = [
    "audio", "stt", "rewrite", "ui", "delivery", "context", "ptt", "record",
];

/// The keys that hold a secret. They are shown as set or not set and never changed
/// here. A test checks that this holds every key named `token`.
pub const SECRETS: [(&str, &str); 2] = [("stt", "token"), ("rewrite", "token")];

/// `[rewrite] engine` is matched by name in `src/rewrite.rs`; its names are kept here
/// once.
const REWRITE_ENGINES: &[&str] = &["off", "agent", "http", "command"];

/// How much of a value is shown on one line.
const SHOWN_CHARS: usize = 60;

pub struct KeyInfo {
    pub name: String,
    pub default: Value,
}

pub struct SectionInfo {
    pub name: &'static str,
    pub keys: Vec<KeyInfo>,
}

/// The sections of the configuration, each with its keys and their defaults, read
/// from the type.
pub fn sections() -> Vec<SectionInfo> {
    let Ok(value) = Value::try_from(Config::default()) else {
        return Vec::new();
    };
    let Some(table) = value.as_table() else {
        return Vec::new();
    };
    ORDER
        .iter()
        .filter_map(|name| {
            let keys = table.get(*name)?.as_table()?;
            Some(SectionInfo {
                name,
                keys: keys
                    .iter()
                    .map(|(key, default)| KeyInfo {
                        name: key.clone(),
                        default: default.clone(),
                    })
                    .collect(),
            })
        })
        .collect()
}

pub fn is_secret(section: &str, key: &str) -> bool {
    SECRETS.iter().any(|(s, k)| *s == section && *k == key)
}

/// The values a string key accepts, when the code accepts only some.
fn allowed(section: &str, key: &str) -> Option<Vec<&'static str>> {
    match (section, key) {
        ("stt", "engine") => Some(crate::stt::ENGINES.to_vec()),
        ("rewrite", "engine") => Some(REWRITE_ENGINES.to_vec()),
        ("context", "source") => Some(crate::bias::source::VALUES.to_vec()),
        _ => None,
    }
}

/// A key's value as the list shows it.
pub struct Shown {
    pub text: String,
    pub is_default: bool,
}

fn value_text(value: &Value) -> String {
    let text = value.to_string().replace('\n', " ");
    if text.chars().count() > SHOWN_CHARS {
        let cut: String = text.chars().take(SHOWN_CHARS - 3).collect();
        format!("{cut}...")
    } else {
        text
    }
}

fn set_in<'a>(file: Option<&'a toml::Table>, section: &str, key: &str) -> Option<&'a Value> {
    file?.get(section)?.as_table()?.get(key)
}

/// What the list shows for a key: the file's value when the file has the key,
/// otherwise the default, marked. A secret is only ever `set` or `not set`.
pub fn shown(section: &str, key: &KeyInfo, file: Option<&toml::Table>) -> Shown {
    let in_file = set_in(file, section, &key.name);
    if is_secret(section, &key.name) {
        let set = matches!(in_file, Some(Value::String(text)) if !text.is_empty());
        return Shown {
            text: if set { "set" } else { "not set" }.to_string(),
            is_default: false,
        };
    }
    match in_file {
        Some(value) => Shown {
            text: value_text(value),
            is_default: false,
        },
        None => Shown {
            text: value_text(&key.default),
            is_default: true,
        },
    }
}

/// The sections, numbered, with how many of their keys the file sets.
pub fn render_sections(sections: &[SectionInfo], file: Option<&toml::Table>) -> String {
    let mut out = String::new();
    for (i, section) in sections.iter().enumerate() {
        let set = section
            .keys
            .iter()
            .filter(|key| set_in(file, section.name, &key.name).is_some())
            .count();
        out.push_str(&format!(
            "  {}. {:<9} {set} of {} keys set\n",
            i + 1,
            section.name,
            section.keys.len()
        ));
    }
    out
}

/// The keys of one section, numbered, with their values.
pub fn render_keys(section: &SectionInfo, file: Option<&toml::Table>) -> String {
    let width = section.keys.iter().map(|k| k.name.len()).max().unwrap_or(0);
    let mut out = format!("[{}]\n", section.name);
    for (i, key) in section.keys.iter().enumerate() {
        let shown = shown(section.name, key, file);
        let mark = if shown.is_default { "  (default)" } else { "" };
        out.push_str(&format!(
            "  {}. {:<width$} = {}{mark}\n",
            i + 1,
            key.name,
            shown.text
        ));
    }
    out
}

/// The inputs, numbered, with the one the configuration names marked, and the lines
/// about the setting.
pub fn render_devices(names: &[String], configured: &str) -> String {
    let mut out = String::from("Microphones this machine offers:\n\n");
    for (i, name) in names.iter().enumerate() {
        // The first input of a name is the one a take selects.
        let first = names.iter().position(|n| n == name).unwrap_or(i);
        let current = if !configured.is_empty() && name == configured && first == i {
            "  (current)"
        } else {
            ""
        };
        let same = if first != i {
            format!(
                "  (same name as {}; the plugin selects by name and uses the first)",
                first + 1
            )
        } else {
            String::new()
        };
        out.push_str(&format!("  {}. {name}{current}{same}\n", i + 1));
    }
    out.push('\n');
    if names.is_empty() {
        out.push_str("No input devices were found. Connect a microphone and open this again.\n");
    } else if configured.is_empty() {
        out.push_str("[audio] input is not set, so the system default input is used.\n");
    } else if !names.iter().any(|n| n == configured) {
        out.push_str(&format!(
            "[audio] input is {configured:?}, which matches none of these inputs; a take is \
             refused until you choose one of them.\n"
        ));
    }
    out
}

/// The settings, from the sections level.
pub fn run_menu(world: &mut dyn World, io: &mut Io) -> u8 {
    let sections = sections();
    io.say("herdr-voice: settings");
    loop {
        let snapshot = world.snapshot();
        let file = snapshot.table();
        io.say("");
        if let Some(note) = config_note(&snapshot.loaded.source) {
            io.say(&note);
        }
        io.say(render_sections(&sections, file.as_ref()).trim_end());
        let Some(line) = io.ask(&format!(
            "Type a section number, then Enter. {LEAVE_HINT}: "
        )) else {
            break;
        };
        match parse_answer(&line, sections.len()) {
            Answer::Leave => break,
            Answer::Invalid(text) => io.say(&not_in_list(&text, sections.len())),
            Answer::Pick(at) => keys_menu(world, io, &sections[at]),
        }
    }
    u8::from(io.failed)
}

fn keys_menu(world: &mut dyn World, io: &mut Io, section: &SectionInfo) {
    loop {
        let snapshot = world.snapshot();
        let file = snapshot.table();
        io.say("");
        io.say(render_keys(section, file.as_ref()).trim_end());
        let Some(line) = io.ask(&format!("Type a key number, then Enter. {LEAVE_HINT}: ")) else {
            return;
        };
        match parse_answer(&line, section.keys.len()) {
            Answer::Leave => return,
            Answer::Invalid(text) => io.say(&not_in_list(&text, section.keys.len())),
            Answer::Pick(at) => edit_key(world, io, section.name, &section.keys[at]),
        }
    }
}

fn edit_key(world: &mut dyn World, io: &mut Io, section: &str, key: &KeyInfo) {
    io.say("");
    match (section, key.name.as_str()) {
        ("audio", "input") => microphone(world, io),
        ("stt", "model") => crate::chooser::choose_speech_model(world, io, &catalogue::MODELS),
        ("rewrite", "model") => rewrite_model(world, io),
        _ if is_secret(section, &key.name) => io.say(&format!(
            "[{section}] {} is a secret: it is not shown or changed here. Change it in the \
             configuration file.",
            key.name
        )),
        _ => match &key.default {
            Value::Boolean(_) | Value::Integer(_) | Value::Float(_) | Value::String(_) => {
                scalar(world, io, section, key)
            }
            _ => io.say(&format!(
                "[{section}] {} is a list of values. It is changed in the configuration file, \
                 not here.",
                key.name
            )),
        },
    }
}

/// A boolean, a number or a text, typed.
fn scalar(world: &mut dyn World, io: &mut Io, section: &str, key: &KeyInfo) {
    let snapshot = world.snapshot();
    let file = snapshot.table();
    let current = shown(section, key, file.as_ref());
    let choices = allowed(section, &key.name);
    let hint = match (&key.default, &choices) {
        (Value::Boolean(_), _) => "true or false".to_string(),
        (Value::Integer(_), _) => "a whole number".to_string(),
        (Value::Float(_), _) => "a number".to_string(),
        (_, Some(values)) => values.join(", "),
        _ => "text".to_string(),
    };
    io.say(&format!(
        "[{section}] {} is {}{}.",
        key.name,
        current.text,
        if current.is_default { " (default)" } else { "" }
    ));
    let Some(line) = io.ask(&format!("New value ({hint}), then Enter. {LEAVE_HINT}: ")) else {
        return;
    };
    let typed = line.trim();
    if typed.is_empty() || typed.starts_with('\u{1b}') {
        io.say("Nothing was changed.");
        return;
    }
    let value = match &key.default {
        Value::Boolean(_) => match typed {
            "true" | "false" => Some(typed.to_string()),
            _ => None,
        },
        Value::Integer(_) => typed.parse::<i64>().ok().map(|n| n.to_string()),
        Value::Float(_) => typed.parse::<f64>().ok().map(|n| format!("{n:?}")),
        _ => Some(config_edit::quote(typed)),
    };
    let Some(value) = value else {
        io.say(&format!(
            "{typed:?} is not {hint}; nothing was changed. Open the key again and type it as \
             {hint}."
        ));
        return;
    };
    if let Some(values) = &choices {
        if !values.contains(&typed) {
            io.say(&format!(
                "{typed:?} is not one of {}; nothing was changed.",
                values.join(", ")
            ));
            return;
        }
    }
    let shown = match &key.default {
        Value::String(_) => format!("{typed:?}"),
        _ => value.clone(),
    };
    save_and_tell(world, io, section, &key.name, value, &shown);
}

/// `[audio] input`, by name.
fn microphone(world: &mut dyn World, io: &mut Io) {
    let snapshot = world.snapshot();
    let configured = snapshot.loaded.config.audio.input.clone();
    let names = match world.input_names() {
        Ok(names) => names,
        Err(why) => {
            io.say(&format!(
                "{why}. Check that an input is connected and that this program may use the \
                 microphone"
            ));
            io.fail();
            return;
        }
    };
    io.say(&render_devices(&names, &configured));
    if names.is_empty() {
        io.fail();
        return;
    }
    let Some(line) = io.ask(&format!(
        "Type the number of the input, then Enter. {LEAVE_HINT}: "
    )) else {
        return;
    };
    let at = match parse_answer(&line, names.len()) {
        Answer::Leave => {
            io.say("Nothing was changed.");
            return;
        }
        Answer::Invalid(text) => {
            io.say(&not_in_list(&text, names.len()));
            return;
        }
        Answer::Pick(at) => at,
    };
    let name = &names[at];
    if save_and_tell(
        world,
        io,
        "audio",
        "input",
        config_edit::quote(name),
        &format!("{name:?}"),
    ) {
        // A take selects by name and uses the first input that has it.
        if let Some(first) = names.iter().position(|n| n == name) {
            if first != at {
                io.say(&format!(
                    "Two inputs are called {name:?}: the plugin uses the first of them, input {}.",
                    first + 1
                ));
            }
        }
    }
}

/// `[rewrite] model`: from the list the server serves, or typed when there is none.
fn rewrite_model(world: &mut dyn World, io: &mut Io) {
    let snapshot = world.snapshot();
    let rewrite = snapshot.loaded.config.rewrite.clone();
    let list = if rewrite.url.is_empty() {
        None
    } else {
        Some(world.rewrite_models(&rewrite.url, &rewrite.token))
    };
    match list {
        Some(Ok(names)) => {
            io.say("Models the server serves:\n");
            for (i, name) in names.iter().enumerate() {
                let mark = if *name == rewrite.model {
                    "  (current)"
                } else {
                    ""
                };
                io.say(&format!("  {}. {name}{mark}", i + 1));
            }
            io.say("");
            let Some(line) = io.ask(&format!(
                "Type the number of the model, then Enter. {LEAVE_HINT}: "
            )) else {
                return;
            };
            match parse_answer(&line, names.len()) {
                Answer::Leave => io.say("Nothing was changed."),
                Answer::Invalid(text) => io.say(&not_in_list(&text, names.len())),
                Answer::Pick(at) => {
                    let name = &names[at];
                    save_and_tell(
                        world,
                        io,
                        "rewrite",
                        "model",
                        config_edit::quote(name),
                        &format!("{name:?}"),
                    );
                }
            }
        }
        other => {
            match other {
                Some(Err(why)) => io.say(&why.to_string()),
                _ => io.say(
                    "[rewrite] url is not set, so there is no server to ask for a list of models.",
                ),
            }
            let Some(line) = io.ask(&format!(
                "Type the model's name instead, then Enter. {LEAVE_HINT}: "
            )) else {
                return;
            };
            let typed = line.trim();
            if typed.is_empty() || typed.starts_with('\u{1b}') {
                io.say("Nothing was changed.");
                return;
            }
            save_and_tell(
                world,
                io,
                "rewrite",
                "model",
                config_edit::quote(typed),
                &format!("{typed:?}"),
            );
        }
    }
}

/// `herdr-voice settings`: the popup. Returns the process's exit code.
pub fn run() -> u8 {
    let mut world = Real::from_env();
    let code = {
        let stdin = std::io::stdin();
        let mut input = stdin.lock();
        let mut out = std::io::stdout();
        let mut io = Io::new(&mut input, &mut out);
        run_menu(&mut world, &mut io)
    };
    // The lock on standard input is released before `pause` reads from it again: the
    // lock is not re-entrant, and a second one taken on the same thread waits for the
    // first for ever, which in a popup is a pane that never closes.
    pause();
    code
}

/// Whether `herdr-voice settings` was asked to open the popup (the action) and not to
/// be it.
pub fn wants_open(args: &[String]) -> bool {
    args.iter().any(|a| a == "--open")
}

/// `herdr-voice settings --open`: what the manifest's `settings` action runs.
pub fn open() -> u8 {
    let herdr = crate::delivery::herdr_binary();
    let plugin = std::env::var("HERDR_PLUGIN_ID")
        .unwrap_or_else(|_| crate::transport::PLUGIN_ID.to_string());
    match crate::popup::open_with(&herdr, &plugin, "settings", OPEN_BOUND) {
        Ok(()) => 0,
        Err(why) => {
            eprintln!("{why}");
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config_edit::WriteError;
    use crate::popup::tests_support::FakeWorld;
    use crate::popup::Reached;
    use crate::rewrite_models::ListFailure;
    use std::io::Cursor;

    /// Runs the menu with `typed` as the person's input.
    fn drive(world: &mut FakeWorld, typed: &str) -> (u8, String) {
        let mut input = Cursor::new(typed.as_bytes().to_vec());
        let mut out: Vec<u8> = Vec::new();
        let code = {
            let mut io = Io::new(&mut input, &mut out);
            run_menu(world, &mut io)
        };
        (code, String::from_utf8(out).unwrap())
    }

    /// The number the menu gives a section.
    fn s(name: &str) -> usize {
        sections()
            .iter()
            .position(|x| x.name == name)
            .expect("a section")
            + 1
    }

    /// The number the keys menu gives a key.
    fn k(section: &str, key: &str) -> usize {
        sections()
            .iter()
            .find(|x| x.name == section)
            .expect("a section")
            .keys
            .iter()
            .position(|x| x.name == key)
            .expect("a key")
            + 1
    }

    fn applied(applied: &[&str], restart: &[&str]) -> Reached {
        Reached::Applied(crate::reload::Applied {
            applied: applied.iter().map(|s| s.to_string()).collect(),
            restart: restart.iter().map(|s| s.to_string()).collect(),
        })
    }

    // --- the list of keys

    #[test]
    fn the_sections_are_the_serialised_ones_in_the_fixed_order() {
        let names: Vec<&str> = sections().iter().map(|x| x.name).collect();
        assert_eq!(
            names,
            ["audio", "stt", "rewrite", "ui", "delivery", "context", "ptt", "record"]
        );
        // The order names exactly the sections the type has: a section added to the
        // configuration fails here until it is placed.
        let value = Value::try_from(Config::default()).unwrap();
        let mut serialised: Vec<&str> = value
            .as_table()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        serialised.sort_unstable();
        let mut ordered = ORDER.to_vec();
        ordered.sort_unstable();
        assert_eq!(serialised, ordered);
    }

    #[test]
    fn every_serialised_key_is_listed_with_its_type_and_default() {
        let value = Value::try_from(Config::default()).unwrap();
        for section in sections() {
            let table = value[section.name].as_table().unwrap();
            let listed: Vec<&str> = section.keys.iter().map(|k| k.name.as_str()).collect();
            let expected: Vec<&str> = table.keys().map(String::as_str).collect();
            assert_eq!(listed, expected, "[{}]", section.name);
        }
        let total: usize = sections().iter().map(|x| x.keys.len()).sum();
        assert!(total >= 25, "{total} keys");
    }

    #[test]
    fn the_secrets_are_every_key_called_token() {
        let value = Value::try_from(Config::default()).unwrap();
        let mut named: Vec<(&str, &str)> = Vec::new();
        for section in sections() {
            for key in &section.keys {
                if key.name == "token" {
                    named.push((section.name, "token"));
                }
            }
        }
        let mut declared = SECRETS.to_vec();
        declared.sort_unstable();
        named.sort_unstable();
        assert_eq!(declared, named, "{value:?}");
    }

    #[test]
    fn a_key_the_file_sets_shows_the_files_value_and_one_it_does_not_shows_its_default_marked() {
        let table: toml::Table = toml::from_str("[ui]\ntoasts = false\nblink_ms = 600\n").unwrap();
        let ui = sections().into_iter().find(|x| x.name == "ui").unwrap();
        let rendered = render_keys(&ui, Some(&table));
        let line = |key: &str| {
            rendered
                .lines()
                .find(|l| l.contains(key))
                .unwrap()
                .to_string()
        };
        assert!(line("toasts").contains("= false") && !line("toasts").contains("(default)"));
        // Set to the same value as the default is still set (reading C3).
        assert!(line("blink_ms").contains("= 600") && !line("blink_ms").contains("(default)"));
        assert!(
            line("sidebar_token").contains("= true") && line("sidebar_token").contains("(default)")
        );
    }

    #[test]
    fn secrets_are_only_ever_set_or_not_set() {
        let table: toml::Table =
            toml::from_str("[stt]\ntoken = \"s3cret\"\n[rewrite]\ntoken = \"\"\n").unwrap();
        let find = |section: &str| sections().into_iter().find(|x| x.name == section).unwrap();
        let stt = render_keys(&find("stt"), Some(&table));
        let rewrite = render_keys(&find("rewrite"), Some(&table));
        assert!(
            stt.lines()
                .any(|l| l.contains("token") && l.ends_with("= set")),
            "{stt}"
        );
        assert!(!stt.contains("s3cret"));
        assert!(
            rewrite
                .lines()
                .any(|l| l.contains("token") && l.ends_with("= not set")),
            "{rewrite}"
        );
    }

    #[test]
    fn a_long_value_is_cut_to_one_line() {
        let long = "x".repeat(200);
        let table: toml::Table =
            toml::from_str(&format!("[stt]\ncommand = [\"{long}\"]\n")).unwrap();
        let stt = sections().into_iter().find(|x| x.name == "stt").unwrap();
        let rendered = render_keys(&stt, Some(&table));
        let line = rendered.lines().find(|l| l.contains(". command ")).unwrap();
        assert!(line.ends_with("..."), "{line}");
        assert!(line.chars().count() < 100, "{line}");
    }

    #[test]
    fn the_sections_menu_counts_the_keys_the_file_sets() {
        let table: toml::Table = toml::from_str("[ui]\ntoasts = false\n").unwrap();
        let rendered = render_sections(&sections(), Some(&table));
        let ui = rendered.lines().find(|l| l.contains("ui")).unwrap();
        assert!(ui.contains("1 of 4 keys set"), "{ui}");
        let audio = rendered.lines().find(|l| l.contains("audio")).unwrap();
        assert!(audio.contains("0 of 2 keys set"), "{audio}");
    }

    // --- the menus

    #[test]
    fn the_popup_lists_every_section_and_leaves_on_an_empty_line() {
        let mut world = FakeWorld::new("menu-leave", "");
        let (code, said) = drive(&mut world, "\n");
        assert_eq!(code, 0, "{said}");
        assert!(said.contains("herdr-voice: settings"), "{said}");
        for section in ORDER {
            assert!(said.contains(&format!(". {section}")), "{section}: {said}");
        }
        assert!(world.saved.is_empty());
    }

    #[test]
    fn every_key_of_every_section_can_be_reached_and_is_listed() {
        for (n, section) in sections().iter().enumerate() {
            let mut world = FakeWorld::new(&format!("menu-keys-{n}"), "");
            let (code, said) = drive(&mut world, &format!("{}\n\n\n", n + 1));
            assert_eq!(code, 0, "{said}");
            for key in &section.keys {
                assert!(
                    said.lines()
                        .any(|l| l.contains(&format!(". {} ", key.name))),
                    "[{}] {} is missing: {said}",
                    section.name,
                    key.name
                );
            }
        }
    }

    #[test]
    fn esc_leaves_one_level_at_a_time() {
        let mut world = FakeWorld::new("menu-esc", "");
        let (code, said) = drive(&mut world, "1\n\u{1b}\n\u{1b}\n");
        assert_eq!(code, 0, "{said}");
        assert!(said.matches("[audio]").count() >= 1, "{said}");
    }

    #[test]
    fn a_number_outside_the_list_is_said_and_asked_again_at_each_level() {
        let mut world = FakeWorld::new("menu-invalid", "");
        let (code, said) = drive(&mut world, "99\n1\n99\n\n\n");
        assert_eq!(code, 0, "{said}");
        assert!(said.contains("between 1 and 8"), "{said}");
        assert!(said.contains("between 1 and 2"), "{said}");
    }

    #[test]
    fn the_end_of_the_input_ends_the_popup_with_a_failure_and_one_message() {
        let mut world = FakeWorld::new("menu-eof", "");
        let (code, said) = drive(&mut world, "1\n");
        assert_eq!(code, 1, "{said}");
        assert_eq!(
            said.matches("nothing was read from the terminal").count(),
            1,
            "{said}"
        );
    }

    #[test]
    fn a_configuration_that_does_not_parse_is_said_and_the_defaults_are_shown() {
        let mut world = FakeWorld::new("menu-invalid-file", "[ui\ntoasts = ");
        let (_, said) = drive(&mut world, "\n");
        assert!(said.contains("does not parse"), "{said}");
        assert!(said.contains("0 of 4 keys set"), "{said}");
    }

    // --- the scalar editor

    #[test]
    fn a_boolean_is_changed_in_place_and_a_restart_is_named() {
        let mut world = FakeWorld::new("scalar-bool", "# mine\n[ui]\ntoasts = true\n");
        world.reached = applied(&[], &["ui"]);
        let typed = format!("{}\n{}\nfalse\n\n\n", s("ui"), k("ui", "toasts"));
        let (code, said) = drive(&mut world, &typed);
        assert_eq!(code, 0, "{said}");
        assert_eq!(world.file(), "# mine\n[ui]\ntoasts = false\n");
        assert_eq!(world.saved, vec!["[ui] toasts = false".to_string()]);
        assert_eq!(world.told, 1);
        assert!(said.contains("needs a restart of herdr"), "{said}");
        // The list afterwards shows what was written.
        assert!(said.matches("toasts").count() >= 2, "{said}");
    }

    #[test]
    fn a_number_is_changed_in_place_as_a_number() {
        let mut world = FakeWorld::new("scalar-int", "");
        let typed = format!("{}\n{}\n250\n\n\n", s("ui"), k("ui", "blink_ms"));
        let (code, said) = drive(&mut world, &typed);
        assert_eq!(code, 0, "{said}");
        assert_eq!(world.file(), "[ui]\nblink_ms = 250\n");
        let typed = format!("{}\n{}\n-60\n\n\n", s("audio"), k("audio", "silence_db"));
        let (_, said) = drive(&mut world, &typed);
        assert!(
            world.file().contains("silence_db = -60.0"),
            "{said}\n{}",
            world.file()
        );
    }

    #[test]
    fn a_text_is_changed_in_place_as_a_quoted_string() {
        let mut world = FakeWorld::new("scalar-text", "[stt]\nlanguage = \"auto\"\n");
        let typed = format!("{}\n{}\nru\n\n\n", s("stt"), k("stt", "language"));
        let (code, said) = drive(&mut world, &typed);
        assert_eq!(code, 0, "{said}");
        assert!(
            world.file().contains("language = \"ru\""),
            "{}",
            world.file()
        );
    }

    #[test]
    fn an_answer_that_is_not_the_type_is_said_and_nothing_is_written() {
        let mut world = FakeWorld::new("scalar-bad", "[ui]\ntoasts = true\n");
        let before = world.file();
        for (key, value) in [("toasts", "yes"), ("blink_ms", "fast")] {
            let typed = format!("{}\n{}\n{value}\n\n\n", s("ui"), k("ui", key));
            let (code, said) = drive(&mut world, &typed);
            assert_eq!(code, 0, "{said}");
            assert!(said.contains("nothing was changed"), "{key}: {said}");
        }
        assert!(world.saved.is_empty());
        assert_eq!(world.file(), before);
    }

    #[test]
    fn a_number_the_key_cannot_hold_is_refused_by_the_writer_and_the_file_is_untouched() {
        let mut world = FakeWorld::new("scalar-negative", "[ui]\nblink_ms = 250\n");
        let before = world.file();
        let typed = format!("{}\n{}\n-5\n\n\n", s("ui"), k("ui", "blink_ms"));
        let (code, said) = drive(&mut world, &typed);
        assert_eq!(code, 1, "{said}");
        assert!(said.contains("nothing was written"), "{said}");
        assert!(said.contains("[ui] blink_ms = -5"), "{said}");
        assert_eq!(world.file(), before);
        assert_eq!(world.told, 0);
    }

    #[test]
    fn a_key_with_a_fixed_set_of_values_refuses_others_and_names_the_set() {
        let mut world = FakeWorld::new("scalar-engine", "");
        let typed = format!("{}\n{}\nwhisperx\n\n\n", s("stt"), k("stt", "engine"));
        let (_, said) = drive(&mut world, &typed);
        assert!(said.contains("not one of candle, http, command"), "{said}");
        assert!(world.saved.is_empty());
        let typed = format!("{}\n{}\ncandle\n\n\n", s("stt"), k("stt", "engine"));
        drive(&mut world, &typed);
        assert!(
            world.file().contains("engine = \"candle\""),
            "{}",
            world.file()
        );
        let typed = format!("{}\n{}\npane\n\n\n", s("context"), k("context", "source"));
        drive(&mut world, &typed);
        assert!(
            world.file().contains("source = \"pane\""),
            "{}",
            world.file()
        );
    }

    #[test]
    fn an_empty_line_or_esc_at_the_value_leaves_the_key_as_it_is() {
        for typed_value in ["\n", "\u{1b}\n"] {
            let mut world = FakeWorld::new("scalar-leave", "[ui]\ntoasts = true\n");
            let typed = format!("{}\n{}\n{typed_value}\n\n", s("ui"), k("ui", "toasts"));
            let (code, said) = drive(&mut world, &typed);
            assert_eq!(code, 0, "{said}");
            assert!(said.contains("Nothing was changed"), "{said}");
            assert!(world.saved.is_empty());
        }
    }

    #[test]
    fn a_change_to_audio_says_the_next_take_uses_it() {
        let mut world = FakeWorld::new("scalar-audio", "");
        world.reached = applied(&["audio"], &[]);
        let typed = format!("{}\n{}\n-40\n\n\n", s("audio"), k("audio", "silence_db"));
        let (_, said) = drive(&mut world, &typed);
        assert!(said.contains("next take"), "{said}");
    }

    #[test]
    fn a_write_that_could_not_happen_prints_the_line_to_add_by_hand_and_fails() {
        let mut world = FakeWorld::new("scalar-io", "");
        world.save_error = Some(WriteError::Io {
            path: "config.toml".to_string(),
            why: "permission denied".to_string(),
        });
        let typed = format!("{}\n{}\nfalse\n\n\n", s("ui"), k("ui", "toasts"));
        let (code, said) = drive(&mut world, &typed);
        assert_eq!(code, 1, "{said}");
        assert!(said.contains("Add this under [ui]"), "{said}");
        assert!(said.contains("toasts = false"), "{said}");
    }

    // --- secrets and lists

    #[test]
    fn a_secret_is_never_printed_and_cannot_be_changed_here() {
        let mut world = FakeWorld::new(
            "secrets",
            "[stt]\ntoken = \"stt-s3cret\"\n[rewrite]\ntoken = \"rw-s3cret\"\nurl = \"http://h/v1/chat/completions\"\n",
        );
        world.list = Ok(vec!["m".to_string()]);
        let before = world.file();
        for (section, key) in SECRETS {
            let typed = format!("{}\n{}\nanything\n\n\n", s(section), k(section, key));
            let (code, said) = drive(&mut world, &typed);
            assert_eq!(code, 0, "{said}");
            assert!(said.contains("is a secret"), "{said}");
            assert!(said.contains("set"), "{said}");
            for secret in ["stt-s3cret", "rw-s3cret"] {
                assert!(!said.contains(secret), "{section}: {said}");
            }
        }
        // The rewrite model flow sends the token to the server and prints none of it.
        let typed = format!("{}\n{}\n1\n\n\n", s("rewrite"), k("rewrite", "model"));
        let (_, said) = drive(&mut world, &typed);
        assert!(!said.contains("rw-s3cret"), "{said}");
        assert_eq!(world.list_calls[0].1, "rw-s3cret");
        assert!(
            world.file().contains("stt-s3cret"),
            "the file is untouched: {before}"
        );
    }

    #[test]
    fn a_list_valued_key_is_said_to_be_changed_in_the_file() {
        let mut world = FakeWorld::new("lists", "");
        for (section, key) in [("stt", "command"), ("rewrite", "command")] {
            let typed = format!("{}\n{}\n\n\n", s(section), k(section, key));
            let (code, said) = drive(&mut world, &typed);
            assert_eq!(code, 0, "{said}");
            assert!(said.contains("is a list of values"), "{said}");
            assert!(said.contains(&format!("[{section}] {key}")), "{said}");
        }
        assert!(world.saved.is_empty());
    }

    // --- the microphone

    fn mic_world(tag: &str, text: &str, names: &[&str]) -> FakeWorld {
        let mut world = FakeWorld::new(tag, text);
        world.names = Ok(names.iter().map(|n| n.to_string()).collect());
        world
    }

    #[test]
    fn the_list_numbers_every_input_and_marks_the_configured_one() {
        let text = render_devices(&["Built-in".to_string(), "Headset".to_string()], "Headset");
        assert!(text.contains("1. Built-in"), "{text}");
        let headset: Vec<&str> = text.lines().filter(|l| l.contains("Headset")).collect();
        assert_eq!(headset.len(), 1, "{text}");
        assert!(
            headset[0].contains("2. Headset") && headset[0].contains("(current)"),
            "{text}"
        );
        assert!(
            !text
                .lines()
                .any(|l| l.contains("Built-in") && l.contains("(current)")),
            "{text}"
        );
    }

    #[test]
    fn a_configured_name_that_matches_nothing_is_said_and_names_the_setting() {
        let text = render_devices(&["Built-in".to_string()], "Old headset");
        assert!(text.contains("[audio] input"), "{text}");
        assert!(text.contains("\"Old headset\""), "{text}");
        assert!(text.contains("matches none"), "{text}");
        assert!(!text.contains("(current)"), "{text}");
    }

    #[test]
    fn an_unset_input_says_the_default_is_used_and_an_empty_name_is_never_current() {
        let text = render_devices(&["Built-in".to_string()], "");
        assert!(
            text.contains("not set") && text.contains("default input"),
            "{text}"
        );
        assert!(!render_devices(&[String::new()], "").contains("(current)"));
    }

    #[test]
    fn two_inputs_with_one_name_are_both_listed_and_the_second_says_the_first_is_used() {
        let names: Vec<String> = ["USB Mic", "Built-in", "USB Mic"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let text = render_devices(&names, "USB Mic");
        assert!(text.contains("3. USB Mic"), "{text}");
        let marked: Vec<&str> = text.lines().filter(|l| l.contains("(current)")).collect();
        assert_eq!(
            marked.len(),
            1,
            "only the first is the one selected: {text}"
        );
        let third = text.lines().find(|l| l.contains("3. USB Mic")).unwrap();
        assert!(
            third.contains("same name as 1") && third.contains("uses the first"),
            "{text}"
        );
    }

    #[test]
    fn choosing_an_input_by_number_saves_its_name_and_says_the_next_take_uses_it() {
        let mut world = mic_world("mic-pick", "", &["Built-in", "Headset"]);
        world.reached = applied(&["audio"], &[]);
        let typed = format!("{}\n{}\n2\n\n\n", s("audio"), k("audio", "input"));
        let (code, said) = drive(&mut world, &typed);
        assert_eq!(code, 0, "{said}");
        assert_eq!(world.saved, vec!["[audio] input = \"Headset\"".to_string()]);
        assert_eq!(world.file(), "[audio]\ninput = \"Headset\"\n");
        assert!(
            said.contains("\"Headset\"") && said.contains("next take"),
            "{said}"
        );
        assert!(said.contains("2. Headset"), "{said}");
    }

    #[test]
    fn esc_at_the_microphone_leaves_the_file_alone_and_says_so() {
        for typed_value in ["\u{1b}\n", "\n", "\u{1b}[B\n"] {
            let mut world = mic_world("mic-esc", "", &["A", "B"]);
            let typed = format!("{}\n{}\n{typed_value}\n\n", s("audio"), k("audio", "input"));
            let (code, said) = drive(&mut world, &typed);
            assert_eq!(code, 0, "{typed_value:?}: {said}");
            assert!(said.contains("Nothing was changed"), "{said}");
            assert!(world.saved.is_empty());
        }
    }

    #[test]
    fn no_input_devices_says_so_and_what_to_do_and_changes_nothing() {
        let mut world = mic_world("mic-none", "", &[]);
        let typed = format!("{}\n{}\n1\n\n\n", s("audio"), k("audio", "input"));
        let (code, said) = drive(&mut world, &typed);
        assert_eq!(code, 1, "{said}");
        assert!(said.contains("No input devices"), "{said}");
        assert!(said.contains("Connect a microphone"), "{said}");
        assert!(world.saved.is_empty());
    }

    #[test]
    fn a_failure_to_list_the_inputs_names_what_to_check() {
        let mut world = FakeWorld::new("mic-list-fails", "");
        world.names = Err("cannot list input devices: denied".to_string());
        let typed = format!("{}\n{}\n\n\n", s("audio"), k("audio", "input"));
        let (code, said) = drive(&mut world, &typed);
        assert_eq!(code, 1, "{said}");
        assert!(said.contains("cannot list input devices: denied"), "{said}");
        assert!(said.contains("may use the microphone"), "{said}");
    }

    #[test]
    fn choosing_the_second_of_two_inputs_with_one_name_says_the_first_is_used() {
        let mut world = mic_world("mic-dup", "", &["USB Mic", "Built-in", "USB Mic"]);
        let typed = format!("{}\n{}\n3\n\n\n", s("audio"), k("audio", "input"));
        let (_, said) = drive(&mut world, &typed);
        assert_eq!(world.saved, vec!["[audio] input = \"USB Mic\"".to_string()]);
        let after = said
            .split("[audio] input is now")
            .nth(1)
            .expect("the confirmation");
        assert!(
            after.contains("input 1") && after.contains("uses the first"),
            "{after}"
        );
    }

    #[test]
    fn a_name_with_a_quote_and_a_backslash_is_written_so_that_it_reads_back_equal() {
        let name = "Mic \"A\" \\ B";
        let mut world = mic_world("mic-quote", "", &[name]);
        let typed = format!("{}\n{}\n1\n\n\n", s("audio"), k("audio", "input"));
        drive(&mut world, &typed);
        let loaded = crate::config::load(Some(&world.dir));
        assert_eq!(loaded.config.audio.input, name);
    }

    // --- the rewrite model

    fn rewrite_world(tag: &str, url: &str) -> FakeWorld {
        let text = if url.is_empty() {
            String::new()
        } else {
            format!("[rewrite]\nurl = \"{url}\"\nmodel = \"m2\"\n")
        };
        FakeWorld::new(tag, &text)
    }

    #[test]
    fn a_list_the_server_serves_is_shown_numbered_and_a_number_writes_that_name_to_rewrite_model_only(
    ) {
        let mut world = rewrite_world("rw-list", "http://127.0.0.1:4000/v1/chat/completions");
        world.list = Ok(vec!["m1".to_string(), "m2".to_string(), "m3".to_string()]);
        let typed = format!("{}\n{}\n3\n\n\n", s("rewrite"), k("rewrite", "model"));
        let (code, said) = drive(&mut world, &typed);
        assert_eq!(code, 0, "{said}");
        assert!(
            said.contains("1. m1") && said.contains("2. m2  (current)"),
            "{said}"
        );
        assert_eq!(world.saved, vec!["[rewrite] model = \"m3\"".to_string()]);
        let loaded = crate::config::load(Some(&world.dir));
        assert_eq!(loaded.config.rewrite.model, "m3");
        assert_eq!(
            loaded.config.stt.model, "large-v3-turbo",
            "no other model key moved"
        );
        assert_eq!(
            world.list_calls,
            vec![(
                "http://127.0.0.1:4000/v1/chat/completions".to_string(),
                String::new()
            )]
        );
    }

    #[test]
    fn a_server_that_cannot_give_a_list_is_said_and_the_name_can_be_typed() {
        for failure in [
            ListFailure::Server(
                "\"http://h/v1/models\" refused the connection. Start the server".to_string(),
            ),
            ListFailure::NoAddress("http://h/v1".to_string()),
            ListFailure::BadBody("not json".to_string()),
            ListFailure::Empty,
        ] {
            let mut world = rewrite_world("rw-fail", "http://h/v1/chat/completions");
            world.list = Err(failure.clone());
            let typed = format!(
                "{}\n{}\nmy-model\n\n\n",
                s("rewrite"),
                k("rewrite", "model")
            );
            let (code, said) = drive(&mut world, &typed);
            assert_eq!(code, 0, "{failure:?}: {said}");
            assert!(said.contains(&failure.to_string()), "{failure:?}: {said}");
            assert!(said.contains("Type the model's name instead"), "{said}");
            assert_eq!(
                world.saved,
                vec!["[rewrite] model = \"my-model\"".to_string()]
            );
        }
    }

    #[test]
    fn with_no_url_nobody_is_asked_and_the_name_can_be_typed() {
        let mut world = rewrite_world("rw-no-url", "");
        let typed = format!("{}\n{}\nlocal\n\n\n", s("rewrite"), k("rewrite", "model"));
        let (_, said) = drive(&mut world, &typed);
        assert!(world.list_calls.is_empty(), "no server, no request");
        assert!(said.contains("[rewrite] url is not set"), "{said}");
        assert_eq!(world.saved, vec!["[rewrite] model = \"local\"".to_string()]);
    }

    #[test]
    fn esc_at_the_rewrite_model_changes_nothing() {
        for typed_value in ["\u{1b}\n", "\n"] {
            let mut world = rewrite_world("rw-esc", "http://h/v1/chat/completions");
            world.list = Ok(vec!["m1".to_string()]);
            let typed = format!(
                "{}\n{}\n{typed_value}\n\n",
                s("rewrite"),
                k("rewrite", "model")
            );
            let (_, said) = drive(&mut world, &typed);
            assert!(said.contains("Nothing was changed"), "{said}");
            assert!(world.saved.is_empty());
            let mut world = rewrite_world("rw-esc-typed", "");
            let typed = format!(
                "{}\n{}\n{typed_value}\n\n",
                s("rewrite"),
                k("rewrite", "model")
            );
            drive(&mut world, &typed);
            assert!(world.saved.is_empty());
        }
    }

    // --- the speech model, reached from the menu

    #[test]
    fn the_speech_model_entry_opens_the_catalogue_flow() {
        let mut world = FakeWorld::new("menu-speech", "[stt]\nengine = \"candle\"\n");
        let typed = format!("{}\n{}\n1\n\n\n", s("stt"), k("stt", "model"));
        let (code, said) = drive(&mut world, &typed);
        assert_eq!(code, 0, "{said}");
        assert_eq!(world.installs, vec!["tiny".to_string()]);
        assert_eq!(world.saved, vec!["[stt] model = \"tiny\"".to_string()]);
    }

    // --- opening

    #[test]
    fn the_flag_chooses_between_being_the_popup_and_opening_it() {
        let args = |list: &[&str]| -> Vec<String> { list.iter().map(|s| s.to_string()).collect() };
        assert!(wants_open(&args(&["settings", "--open"])));
        assert!(!wants_open(&args(&["settings"])));
    }

    #[test]
    fn the_manifest_opens_the_settings_and_has_no_separate_microphone_entry() {
        let manifest: toml::Value =
            toml::from_str(&std::fs::read_to_string("herdr-plugin.toml").unwrap()).unwrap();
        let entries =
            |kind: &str| -> Vec<toml::Value> { manifest[kind].as_array().unwrap().to_vec() };
        for kind in ["actions", "panes"] {
            let ids: Vec<String> = entries(kind)
                .iter()
                .map(|e| e["id"].as_str().unwrap().to_string())
                .collect();
            assert!(
                !ids.iter().any(|id| id == "mic"),
                "no `mic` {kind} entry may remain"
            );
        }
        let action = entries("actions")
            .into_iter()
            .find(|e| e["id"].as_str() == Some("settings"))
            .expect("an action `settings`");
        assert_eq!(action["title"].as_str(), Some("herdr-voice: settings"));
        let argv: Vec<&str> = action["command"]
            .as_array()
            .unwrap()
            .iter()
            .map(|a| a.as_str().unwrap())
            .collect();
        assert_eq!(&argv[1..], ["settings", "--open"]);
        let pane = entries("panes")
            .into_iter()
            .find(|e| e["id"].as_str() == Some("settings"))
            .expect("a pane `settings`");
        assert_eq!(pane["title"].as_str(), Some("herdr-voice: settings"));
        assert_eq!(pane["placement"].as_str(), Some("popup"));
        let argv: Vec<&str> = pane["command"]
            .as_array()
            .unwrap()
            .iter()
            .map(|a| a.as_str().unwrap())
            .collect();
        assert_eq!(&argv[1..], ["settings"]);
        // The pane that opens when the key is pressed is the one the action names.
        assert_eq!(
            crate::popup::open_command("herdr", "herdr-voice", "settings")[7],
            "settings"
        );
    }
}
