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
