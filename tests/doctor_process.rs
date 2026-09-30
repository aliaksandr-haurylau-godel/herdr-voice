//! What `doctor` prints as a process. `run()` in `src/doctor.rs` joins the
//! findings, feeds `notifications` the plugin's own `[ui] toasts` and the file
//! herdr's configuration path names, and prints them in a fixed order. None of
//! that is reachable from a unit test of the pieces, so it is asserted by
//! running the built binary, pointed at a scratch herdr configuration and a
//! scratch plugin configuration directory.
//!
//! Unix only, like `tests/setup_process.rs`.
#![cfg(unix)]

use std::path::PathBuf;
use std::process::Command;

/// A scratch directory unique per test and per process, removed on drop.
struct Scratch {
    dir: PathBuf,
}

impl Scratch {
    fn new(name: &str) -> Scratch {
        let dir = std::env::temp_dir().join(format!(
            "herdr-voice-doctor-process-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("plugin")).expect("scratch directory");
        Scratch { dir }
    }

    /// Writes herdr's configuration, or leaves it absent when `None`.
    fn herdr_config(&self, text: Option<&str>) -> PathBuf {
        let path = self.dir.join("herdr-config.toml");
        match text {
            Some(text) => std::fs::write(&path, text).expect("write herdr configuration"),
            None => {
                let _ = std::fs::remove_file(&path);
            }
        }
        path
    }

    fn plugin_config(&self, text: &str) {
        std::fs::write(self.dir.join("plugin").join("config.toml"), text)
            .expect("write plugin configuration");
    }

    fn doctor(&self, herdr_config: &PathBuf) -> Vec<String> {
        let out = Command::new(env!("CARGO_BIN_EXE_herdr-voice"))
            .arg("doctor")
            .env("HERDR_CONFIG_PATH", herdr_config)
            .env("HERDR_PLUGIN_CONFIG_DIR", self.dir.join("plugin"))
            .output()
            .expect("the built binary must be runnable");
        // A finding's detail can continue on indented lines; those are not
        // findings of their own.
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .filter(|l| !l.starts_with(char::is_whitespace))
            .map(|l| l.to_string())
            .collect()
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn line<'a>(lines: &'a [String], name: &str) -> &'a str {
    lines
        .iter()
        .find(|l| l.split_whitespace().next() == Some(name))
        .unwrap_or_else(|| panic!("no {name} line in {lines:?}"))
}

fn states(lines: &[String], name: &str) -> String {
    line(lines, name)
        .split_whitespace()
        .nth(1)
        .unwrap_or_default()
        .to_string()
}

#[test]
fn the_lines_come_in_the_documented_order_with_notifications_third() {
    let scratch = Scratch::new("order");
    let herdr = scratch.herdr_config(Some("[ui.toast]\ndelivery = \"herdr\"\n"));
    let names: Vec<String> = scratch
        .doctor(&herdr)
        .iter()
        .map(|l| l.split_whitespace().next().unwrap_or("").to_string())
        .collect();
    assert_eq!(
        names,
        [
            "herdr",
            "daemon",
            "notifications",
            "config",
            "engine",
            "model",
            "rewrite",
            "record"
        ]
    );
}

#[test]
fn the_line_follows_what_the_herdr_file_says() {
    let scratch = Scratch::new("values");
    for (text, state, word) in [
        (Some("[ui.toast]\ndelivery = \"herdr\"\n"), "ok", "herdr"),
        (
            Some("[ui.toast]\ndelivery = \"terminal\"\n"),
            "warning",
            "terminal",
        ),
        (Some("[ui.toast]\ndelivery = \"off\"\n"), "missing", "off"),
        (Some(""), "missing", "default is \"off\""),
        (None, "missing", "no file at"),
    ] {
        let herdr = scratch.herdr_config(text);
        let lines = scratch.doctor(&herdr);
        assert_eq!(states(&lines, "notifications"), state, "{text:?}");
        assert!(
            line(&lines, "notifications").contains(word),
            "{word:?} in {}",
            line(&lines, "notifications")
        );
        assert!(
            line(&lines, "notifications").contains(herdr.to_str().unwrap()),
            "the line names the file it read"
        );
    }
}

#[test]
fn toasts_switched_off_in_the_plugin_configuration_make_the_line_unused() {
    let scratch = Scratch::new("unused");
    scratch.plugin_config("[ui]\ntoasts = false\n");
    let herdr = scratch.herdr_config(Some("[ui.toast]\ndelivery = \"terminal\"\n"));
    assert_eq!(states(&scratch.doctor(&herdr), "notifications"), "unused");
}

#[test]
fn toasts_left_on_in_the_plugin_configuration_do_not_make_the_line_unused() {
    let scratch = Scratch::new("used");
    scratch.plugin_config("[ui]\ntoasts = true\n");
    let herdr = scratch.herdr_config(Some("[ui.toast]\ndelivery = \"terminal\"\n"));
    assert_eq!(states(&scratch.doctor(&herdr), "notifications"), "warning");
}
