//! `setup`: the keybindings this plugin needs, and putting them into the user's
//! herdr configuration. A herdr plugin manifest cannot declare keys — see
//! `docs/design.md` section 7 — so this action bridges the gap.

// Nothing calls this module until the `setup` subcommand is routed to it; the
// allowance goes away with that wiring.
#![allow(dead_code)]

/// The plugin id, as `herdr-plugin.toml` declares it. A binding addresses an
/// action as `<plugin id>.<action id>`. Re-exported rather than written out a
/// second time: two copies of the id could drift apart, and the id is already
/// the one `src/transport.rs:18` names.
pub use crate::transport::PLUGIN_ID;

#[derive(Debug)]
pub struct Binding {
    pub action: &'static str,
    pub key: &'static str,
    pub description: &'static str,
}

impl Binding {
    pub fn command(&self) -> String {
        format!("{PLUGIN_ID}.{}", self.action)
    }
}

/// `static`, not `const`: a `const` is inlined at every use site, so
/// `BINDINGS.iter()` would hand out references into a temporary and nothing
/// could hold a `&'static Binding`. `decide` returns exactly that.
///
/// The keys are the owner's decision, recorded in `tasks/41/RUN_41.md`. They are
/// direct `ctrl+...` chords and one prefix chord: herdr's own default
/// configuration says `alt+...` depends on the terminal, and on the machine this
/// was chosen on `alt+v` did nothing and `alt+g` typed a copyright sign.
pub static BINDINGS: [Binding; 3] = [
    Binding {
        action: "ptt",
        key: "ctrl+g",
        description: "dictation: hold to talk",
    },
    Binding {
        action: "dictate",
        key: "prefix+i",
        description: "dictation: start or finish",
    },
    Binding {
        action: "cancel",
        key: "ctrl+shift+g",
        description: "dictation: cancel the recording",
    },
];

/// The blocks, in the order given, as herdr's configuration wants them.
pub fn render(bindings: &[&Binding]) -> String {
    let mut out = String::new();
    for binding in bindings {
        out.push_str("[[keys.command]]\n");
        out.push_str(&format!("key = {:?}\n", binding.key));
        out.push_str("type = \"plugin_action\"\n");
        out.push_str(&format!("command = {:?}\n", binding.command()));
        out.push_str(&format!("description = {:?}\n", binding.description));
        out.push('\n');
    }
    out
}

use std::path::PathBuf;

/// herdr's own order, stated by its binary: `HERDR_CONFIG_PATH overrides config
/// file path`, otherwise `herdr/config.toml` under `XDG_CONFIG_HOME`, otherwise
/// the same under `~/.config`.
///
/// Takes the three values rather than reading them, so the tests do not mutate
/// an environment the parallel suite shares — the reason `src/delivery.rs:171`
/// gives for the same split.
pub fn config_path_from(
    config_path_var: Option<String>,
    xdg: Option<String>,
    home: Option<String>,
) -> Option<PathBuf> {
    let non_empty = |v: Option<String>| v.filter(|s| !s.is_empty());
    if let Some(explicit) = non_empty(config_path_var) {
        return Some(PathBuf::from(explicit));
    }
    if let Some(dir) = non_empty(xdg) {
        return Some(PathBuf::from(dir).join("herdr").join("config.toml"));
    }
    let home = non_empty(home)?;
    Some(
        PathBuf::from(home)
            .join(".config")
            .join("herdr")
            .join("config.toml"),
    )
}

pub fn config_path() -> Option<PathBuf> {
    config_path_from(
        std::env::var("HERDR_CONFIG_PATH").ok(),
        std::env::var("XDG_CONFIG_HOME").ok(),
        std::env::var("HOME").ok(),
    )
}

/// What the user's configuration already binds. Two lists, because the two
/// questions this action asks have different answers: is one of our own
/// bindings already present, and is one of our keys already taken by something
/// else.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Existing {
    /// `(key, command)` for every `[[keys.command]]` block.
    pub commands: Vec<(String, String)>,
    /// Every key named by a string assignment directly under `[keys]` —
    /// `prefix`, `goto`, `new_tab` and the rest. herdr's own defaults are not
    /// visible here: only what the user wrote down is.
    pub reserved_keys: Vec<String>,
}

pub fn inspect(text: &str) -> Result<Existing, String> {
    let parsed: toml::Value = toml::from_str(text).map_err(|e| e.to_string())?;
    let mut existing = Existing::default();
    let Some(keys) = parsed.get("keys").and_then(|k| k.as_table()) else {
        return Ok(existing);
    };
    for (name, value) in keys {
        if name == "command" {
            let Some(blocks) = value.as_array() else {
                continue;
            };
            for block in blocks {
                let key = block.get("key").and_then(|v| v.as_str());
                let command = block.get("command").and_then(|v| v.as_str());
                if let (Some(key), Some(command)) = (key, command) {
                    existing
                        .commands
                        .push((key.to_string(), command.to_string()));
                }
            }
        } else if let Some(bound) = value.as_str() {
            if !bound.is_empty() {
                existing.reserved_keys.push(bound.to_string());
            }
        }
    }
    Ok(existing)
}

#[derive(Debug, Default)]
pub struct Decision {
    pub to_add: Vec<&'static Binding>,
    /// Ours, already bound — with the key it is bound to, which may not be ours.
    pub already: Vec<(&'static Binding, String)>,
    /// Our key, held by something else — with what holds it.
    pub blocked: Vec<(&'static Binding, String)>,
}

/// Present beats blocked: a binding of ours that already exists is reported as
/// present whatever key it sits on, and its key is never treated as somebody
/// else's.
pub fn decide(existing: &Existing) -> Decision {
    let mut decision = Decision::default();
    for binding in BINDINGS.iter() {
        let command = binding.command();
        if let Some((key, _)) = existing.commands.iter().find(|(_, c)| *c == command) {
            decision.already.push((binding, key.clone()));
            continue;
        }
        let holder = existing
            .commands
            .iter()
            .find(|(k, _)| *k == binding.key)
            .map(|(_, c)| c.clone())
            .or_else(|| {
                existing
                    .reserved_keys
                    .iter()
                    .find(|k| *k == binding.key)
                    .map(|_| "a herdr action in your [keys] block".to_string())
            });
        match holder {
            Some(what) => decision.blocked.push((binding, what)),
            None => decision.to_add.push(binding),
        }
    }
    decision
}

use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HerdrError {
    /// herdr could not be started at all.
    NotFound { binary: String, path: String },
    /// herdr ran and refused. The string is what it said.
    Rejected(String),
}

impl std::fmt::Display for HerdrError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HerdrError::Rejected(why) => write!(f, "{why}"),
            HerdrError::NotFound { binary, path } => write!(
                f,
                "cannot run {binary:?}: it is not on the PATH this process has, which is \
                 {path:?}. Set HERDR_BIN_PATH to herdr's location, or start herdr from a \
                 shell where it is on the PATH"
            ),
        }
    }
}

/// herdr's verdict on a configuration file. `ok` is the exit status — measured,
/// not guessed: 0 with `config: ok`, 1 with `config: issues found`. `output` is
/// what it printed, shown to the person verbatim and never parsed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Check {
    pub ok: bool,
    pub output: String,
}

impl Check {
    pub fn from_status(code: i32, output: String) -> Self {
        Check {
            ok: code == 0,
            output,
        }
    }
}

pub fn open_pane_args() -> Vec<&'static str> {
    vec![
        "plugin",
        "pane",
        "open",
        "--plugin",
        PLUGIN_ID,
        "--entrypoint",
        "setup",
    ]
}

/// `herdr config check` judges whatever `HERDR_CONFIG_PATH` names, so the file
/// travels in the environment of the child rather than in its arguments.
pub fn check_config_args(_path: &Path) -> Vec<&'static str> {
    vec!["config", "check"]
}

pub trait Herdr {
    fn open_pane(&self) -> Result<(), HerdrError>;
    fn check_config(&self, path: &Path) -> Result<Check, HerdrError>;
    fn notify(&self, title: &str, body: &str) -> Result<(), HerdrError>;
}

pub struct HerdrCli {
    binary: String,
}

impl HerdrCli {
    pub fn new() -> Self {
        HerdrCli {
            binary: crate::delivery::herdr_binary(),
        }
    }

    fn not_found(&self) -> HerdrError {
        HerdrError::NotFound {
            binary: self.binary.clone(),
            path: std::env::var("PATH").unwrap_or_default(),
        }
    }
}

impl Default for HerdrCli {
    fn default() -> Self {
        Self::new()
    }
}

impl Herdr for HerdrCli {
    fn open_pane(&self) -> Result<(), HerdrError> {
        match std::process::Command::new(&self.binary)
            .args(open_pane_args())
            .output()
        {
            Err(_) => Err(self.not_found()),
            Ok(out) if out.status.success() => Ok(()),
            Ok(out) => Err(HerdrError::Rejected(
                String::from_utf8_lossy(if out.stdout.is_empty() {
                    &out.stderr
                } else {
                    &out.stdout
                })
                .trim()
                .to_string(),
            )),
        }
    }

    fn check_config(&self, path: &Path) -> Result<Check, HerdrError> {
        match std::process::Command::new(&self.binary)
            .args(check_config_args(path))
            .env("HERDR_CONFIG_PATH", path)
            .output()
        {
            Err(_) => Err(self.not_found()),
            Ok(out) => {
                let mut text = String::from_utf8_lossy(&out.stdout).to_string();
                text.push_str(&String::from_utf8_lossy(&out.stderr));
                Ok(Check::from_status(
                    out.status.code().unwrap_or(1),
                    text.trim_end().to_string(),
                ))
            }
        }
    }

    fn notify(&self, title: &str, body: &str) -> Result<(), HerdrError> {
        match std::process::Command::new(&self.binary)
            .args(crate::delivery::notify_args(title, body))
            .output()
        {
            Err(_) => Err(self.not_found()),
            Ok(out) if out.status.success() => Ok(()),
            Ok(out) => Err(HerdrError::Rejected(
                String::from_utf8_lossy(&out.stderr).trim().to_string(),
            )),
        }
    }
}

#[cfg(test)]
pub mod tests_support {
    use super::{Check, Herdr, HerdrError};
    use std::path::Path;
    use std::sync::{Arc, Mutex};

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum Call {
        OpenPane,
        CheckConfig(String),
        Notify(String, String),
    }

    /// Records every call in order and answers with a fixed verdict, so the
    /// argument lists and both exit statuses are asserted without a live herdr.
    #[derive(Clone)]
    pub struct FakeHerdr {
        calls: Arc<Mutex<Vec<Call>>>,
        verdicts: Arc<Mutex<Vec<Check>>>,
    }

    impl FakeHerdr {
        /// Answers `config: ok` to every check.
        pub fn clean() -> Self {
            FakeHerdr {
                calls: Arc::new(Mutex::new(Vec::new())),
                verdicts: Arc::new(Mutex::new(Vec::new())),
            }
        }

        /// Answers the given verdicts in order, then `config: ok` after them.
        pub fn answering(verdicts: Vec<Check>) -> Self {
            FakeHerdr {
                calls: Arc::new(Mutex::new(Vec::new())),
                verdicts: Arc::new(Mutex::new(verdicts)),
            }
        }

        pub fn calls(&self) -> Vec<Call> {
            self.calls.lock().unwrap().clone()
        }
    }

    impl Herdr for FakeHerdr {
        fn open_pane(&self) -> Result<(), HerdrError> {
            self.calls.lock().unwrap().push(Call::OpenPane);
            Ok(())
        }

        fn check_config(&self, path: &Path) -> Result<Check, HerdrError> {
            self.calls
                .lock()
                .unwrap()
                .push(Call::CheckConfig(path.display().to_string()));
            let mut verdicts = self.verdicts.lock().unwrap();
            if verdicts.is_empty() {
                Ok(Check {
                    ok: true,
                    output: "config: ok".to_string(),
                })
            } else {
                Ok(verdicts.remove(0))
            }
        }

        fn notify(&self, title: &str, body: &str) -> Result<(), HerdrError> {
            self.calls
                .lock()
                .unwrap()
                .push(Call::Notify(title.to_string(), body.to_string()));
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_rendered_block_carries_all_four_fields() {
        let rendered = render(&[&BINDINGS[0]]);
        assert!(rendered.contains("[[keys.command]]"), "{rendered}");
        assert!(rendered.contains("key = \"ctrl+g\""), "{rendered}");
        assert!(rendered.contains("type = \"plugin_action\""), "{rendered}");
        assert!(
            rendered.contains("command = \"haurylau.voice.ptt\""),
            "{rendered}"
        );
        assert!(rendered.contains("description = "), "{rendered}");
    }

    #[test]
    fn the_three_bindings_are_the_ones_the_owner_chose() {
        let pairs: Vec<(&str, &str)> = BINDINGS.iter().map(|b| (b.action, b.key)).collect();
        assert_eq!(
            pairs,
            vec![
                ("ptt", "ctrl+g"),
                ("dictate", "prefix+i"),
                ("cancel", "ctrl+shift+g"),
            ]
        );
    }

    #[test]
    fn what_is_rendered_parses_back_as_toml() {
        let all: Vec<&Binding> = BINDINGS.iter().collect();
        let parsed: toml::Value = toml::from_str(&render(&all))
            .expect("the snippet this action prints must itself be valid TOML");
        let commands = parsed["keys"]["command"].as_array().unwrap();
        assert_eq!(commands.len(), 3);
    }

    #[test]
    fn opening_the_pane_asks_herdr_for_this_plugin_s_setup_entrypoint() {
        assert_eq!(
            open_pane_args(),
            vec![
                "plugin",
                "pane",
                "open",
                "--plugin",
                "haurylau.voice",
                "--entrypoint",
                "setup",
            ]
        );
    }

    /// The file travels in the child's environment rather than in its argument
    /// list, so the arguments are the bare subcommand and name no path.
    #[test]
    fn checking_a_configuration_is_config_check_with_no_path_among_the_arguments() {
        let args = check_config_args(std::path::Path::new("/tmp/candidate.toml"));
        assert_eq!(args, vec!["config", "check"]);
        assert!(!args.iter().any(|a| a.contains("candidate")), "{args:?}");
    }

    #[test]
    fn a_zero_exit_is_a_clean_verdict_and_a_one_exit_is_not() {
        // Measured against herdr 0.9.0: `config: ok` exits 0,
        // `config: issues found` exits 1. See DESIGN_41.md, the appendix.
        assert!(Check::from_status(0, "config: ok\n".into()).ok);
        assert!(!Check::from_status(1, "config: issues found\n".into()).ok);
    }

    #[test]
    fn the_fake_records_what_it_was_asked_to_do() {
        use tests_support::{Call, FakeHerdr};
        let herdr = FakeHerdr::clean();
        herdr.open_pane().unwrap();
        herdr.notify("title", "body").unwrap();
        assert_eq!(
            herdr.calls(),
            vec![Call::OpenPane, Call::Notify("title".into(), "body".into())]
        );
    }

    #[test]
    fn a_clean_configuration_takes_all_three() {
        let d = decide(&Existing::default());
        assert_eq!(d.to_add.len(), 3);
        assert!(d.already.is_empty());
        assert!(d.blocked.is_empty());
    }

    #[test]
    fn a_binding_of_ours_that_is_already_there_is_not_added_again() {
        let existing = Existing {
            commands: vec![("ctrl+z".into(), "haurylau.voice.ptt".into())],
            reserved_keys: vec![],
        };
        let d = decide(&existing);
        assert_eq!(d.to_add.len(), 2);
        assert_eq!(d.already.len(), 1);
        // It says which key it is on, which is not the key we would have used.
        assert_eq!(d.already[0].0.action, "ptt");
        assert_eq!(d.already[0].1, "ctrl+z");
    }

    #[test]
    fn a_key_held_by_something_else_blocks_that_block_and_no_other() {
        let existing = Existing {
            commands: vec![("ctrl+g".into(), "someone.else.thing".into())],
            reserved_keys: vec![],
        };
        let d = decide(&existing);
        assert_eq!(d.to_add.len(), 2, "the other two are still added");
        assert_eq!(d.blocked.len(), 1);
        assert_eq!(d.blocked[0].0.action, "ptt");
        assert_eq!(d.blocked[0].1, "someone.else.thing");
    }

    #[test]
    fn a_key_the_user_gave_to_a_herdr_action_also_blocks() {
        let existing = Existing {
            commands: vec![],
            reserved_keys: vec!["prefix+i".into()],
        };
        let d = decide(&existing);
        assert_eq!(d.blocked.len(), 1);
        assert_eq!(d.blocked[0].0.action, "dictate");
    }

    #[test]
    fn our_own_binding_on_our_own_key_counts_as_present_not_as_a_collision() {
        let existing = Existing {
            commands: vec![("ctrl+g".into(), "haurylau.voice.ptt".into())],
            reserved_keys: vec![],
        };
        let d = decide(&existing);
        assert_eq!(d.already.len(), 1);
        assert!(d.blocked.is_empty(), "it is ours; it is not in the way");
    }

    const SAMPLE: &str = r#"
[keys]
prefix = "ctrl+b"
goto = "prefix+g"

[[keys.command]]
key = "prefix+d"
type = "plugin_action"
command = "someone.else.toggle"
description = "not ours"

[[keys.command]]
key = "ctrl+g"
type = "plugin_action"
command = "haurylau.voice.ptt"
description = "ours, already here"
"#;

    #[test]
    fn it_reads_every_command_binding_as_a_key_and_a_command() {
        let existing = inspect(SAMPLE).unwrap();
        assert!(existing
            .commands
            .contains(&("ctrl+g".to_string(), "haurylau.voice.ptt".to_string())));
        assert!(existing
            .commands
            .contains(&("prefix+d".to_string(), "someone.else.toggle".to_string())));
    }

    #[test]
    fn it_reads_the_keys_herdr_s_own_actions_are_bound_to() {
        let existing = inspect(SAMPLE).unwrap();
        assert!(existing.reserved_keys.contains(&"prefix+g".to_string()));
        assert!(existing.reserved_keys.contains(&"ctrl+b".to_string()));
    }

    #[test]
    fn an_empty_configuration_is_a_valid_one() {
        let existing = inspect("").unwrap();
        assert!(existing.commands.is_empty());
        assert!(existing.reserved_keys.is_empty());
    }

    #[test]
    fn an_unparseable_configuration_is_reported_and_not_guessed_at() {
        let why = inspect("this is not toml [[[").unwrap_err();
        assert!(!why.is_empty(), "the reason must carry herdr's own words");
    }

    #[test]
    fn the_override_wins_over_everything() {
        let p = config_path_from(
            Some("/tmp/somewhere/other.toml".into()),
            Some("/tmp/xdg".into()),
            Some("/tmp/home".into()),
        );
        assert_eq!(
            p.unwrap(),
            std::path::Path::new("/tmp/somewhere/other.toml")
        );
    }

    #[test]
    fn without_the_override_it_is_herdr_config_toml_under_xdg() {
        let p = config_path_from(None, Some("/tmp/xdg".into()), Some("/tmp/home".into()));
        assert_eq!(
            p.unwrap(),
            std::path::Path::new("/tmp/xdg/herdr/config.toml")
        );
    }

    #[test]
    fn without_xdg_it_is_dot_config_under_the_home_directory() {
        let p = config_path_from(None, None, Some("/tmp/home".into()));
        assert_eq!(
            p.unwrap(),
            std::path::Path::new("/tmp/home/.config/herdr/config.toml")
        );
    }

    #[test]
    fn with_nothing_to_resolve_against_there_is_no_path() {
        assert!(config_path_from(None, None, None).is_none());
    }

    #[test]
    fn an_empty_variable_counts_as_unset() {
        let p = config_path_from(Some(String::new()), None, Some("/tmp/home".into()));
        assert_eq!(
            p.unwrap(),
            std::path::Path::new("/tmp/home/.config/herdr/config.toml")
        );
    }

    /// The manifest is the only contract with herdr: a snippet naming an action
    /// the manifest does not declare installs a key that does nothing when it is
    /// pressed. `scripts/check_manifest.py` guards the other direction.
    #[test]
    fn every_action_the_snippet_names_is_declared_in_the_manifest() {
        let manifest: toml::Value =
            toml::from_str(&std::fs::read_to_string("herdr-plugin.toml").unwrap()).unwrap();
        assert_eq!(manifest["id"].as_str().unwrap(), PLUGIN_ID);
        let declared: Vec<&str> = manifest["actions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|a| a["id"].as_str().unwrap())
            .collect();
        for binding in BINDINGS.iter() {
            assert!(
                declared.contains(&binding.action),
                "the snippet names the action {:?}, which the manifest does not declare",
                binding.action
            );
        }
    }
}
