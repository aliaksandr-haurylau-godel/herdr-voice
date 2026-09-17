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
