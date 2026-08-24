//! The configuration file, and a default for every key.
//!
//! Only the keys this issue needs are read: `doctor` reports on the model and the
//! rewrite engine. Keys belonging to later stages are ignored rather than refused,
//! so a file written for a later version does not stop the daemon. The location is
//! the directory herdr itself computes — `herdr plugin config-dir haurylau.voice`
//! prints it even for a plugin that is not installed. See `tasks/3/DESIGN_3.md`,
//! section 4, and `docs/design.md`, section 7.

use serde::Deserialize;
use std::path::{Path, PathBuf};

use crate::transport::PLUGIN_ID;

pub const FILE_NAME: &str = "config.toml";

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct Config {
    pub stt: Stt,
    pub rewrite: Rewrite,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct Stt {
    pub model: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct Rewrite {
    pub engine: String,
    pub agent: String,
}

impl Default for Stt {
    fn default() -> Self {
        Stt {
            model: "large-v3-turbo".to_string(),
        }
    }
}

impl Default for Rewrite {
    fn default() -> Self {
        Rewrite {
            engine: "agent".to_string(),
            agent: "auto".to_string(),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Vars {
    pub config_dir: Option<String>,
    pub xdg_config_home: Option<String>,
    pub home: Option<String>,
}

impl Vars {
    pub fn from_env() -> Vars {
        Vars {
            config_dir: std::env::var("HERDR_PLUGIN_CONFIG_DIR").ok(),
            xdg_config_home: std::env::var("XDG_CONFIG_HOME").ok(),
            home: std::env::var("HOME").ok(),
        }
    }
}

pub fn directory(vars: &Vars) -> Option<PathBuf> {
    if let Some(given) = &vars.config_dir {
        return Some(PathBuf::from(given));
    }
    if let Some(xdg) = &vars.xdg_config_home {
        return Some(
            PathBuf::from(xdg)
                .join("herdr/plugins/config")
                .join(PLUGIN_ID),
        );
    }
    vars.home.as_ref().map(|home| {
        PathBuf::from(home)
            .join(".config/herdr/plugins/config")
            .join(PLUGIN_ID)
    })
}

#[derive(Debug)]
pub enum Source {
    File(PathBuf),
    Defaults(Option<PathBuf>),
    Invalid { path: PathBuf, why: String },
}

#[derive(Debug)]
pub struct Loaded {
    pub config: Config,
    pub source: Source,
}

pub fn load(directory: Option<&Path>) -> Loaded {
    let Some(directory) = directory else {
        return Loaded {
            config: Config::default(),
            source: Source::Defaults(None),
        };
    };
    let path = directory.join(FILE_NAME);
    match std::fs::read_to_string(&path) {
        Err(_) => Loaded {
            config: Config::default(),
            source: Source::Defaults(Some(path)),
        },
        Ok(text) => match toml::from_str::<Config>(&text) {
            Ok(config) => Loaded {
                config,
                source: Source::File(path),
            },
            Err(e) => Loaded {
                config: Config::default(),
                source: Source::Invalid {
                    path,
                    why: e.to_string(),
                },
            },
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!("herdr-voice-config-{tag}-{}", std::process::id()));
        std::fs::create_dir_all(&path).expect("create scratch");
        path
    }

    #[test]
    fn every_key_has_a_default() {
        let defaults = Config::default();
        assert_eq!(defaults.stt.model, "large-v3-turbo");
        assert_eq!(defaults.rewrite.engine, "agent");
        assert_eq!(defaults.rewrite.agent, "auto");
    }

    #[test]
    fn no_file_is_a_valid_state() {
        let directory = scratch("absent");
        let loaded = load(Some(&directory));
        assert_eq!(loaded.config, Config::default());
        match loaded.source {
            Source::Defaults(Some(path)) => assert_eq!(path, directory.join("config.toml")),
            other => panic!("expected defaults with a path, got {other:?}"),
        }
    }

    #[test]
    fn a_partial_file_keeps_the_other_defaults() {
        let directory = scratch("partial");
        std::fs::write(directory.join("config.toml"), "[stt]\nmodel = \"small\"\n").unwrap();
        let loaded = load(Some(&directory));
        assert_eq!(loaded.config.stt.model, "small");
        assert_eq!(loaded.config.rewrite.engine, "agent");
        assert!(
            matches!(loaded.source, Source::File(_)),
            "got {:?}",
            loaded.source
        );
    }

    #[test]
    fn a_key_of_a_later_stage_is_ignored_rather_than_fatal() {
        let directory = scratch("future");
        std::fs::write(
            directory.join("config.toml"),
            "[ptt]\nrelease_ms = 250\n\n[audio]\ninput = \"\"\n\n[stt]\nmodel = \"small\"\n",
        )
        .unwrap();
        let loaded = load(Some(&directory));
        assert_eq!(loaded.config.stt.model, "small");
        assert!(
            matches!(loaded.source, Source::File(_)),
            "got {:?}",
            loaded.source
        );
    }

    #[test]
    fn a_broken_file_is_reported_and_the_defaults_are_used() {
        let directory = scratch("broken");
        std::fs::write(directory.join("config.toml"), "[stt\nmodel =").unwrap();
        let loaded = load(Some(&directory));
        assert_eq!(loaded.config, Config::default());
        match loaded.source {
            Source::Invalid { why, .. } => assert!(!why.is_empty()),
            other => panic!("expected invalid, got {other:?}"),
        }
    }

    #[test]
    fn the_directory_herdr_gives_wins_then_xdg_then_home() {
        let given = Vars {
            config_dir: Some("/tmp/from-herdr".into()),
            xdg_config_home: Some("/tmp/xdg".into()),
            home: Some("/tmp/home".into()),
        };
        assert_eq!(directory(&given).unwrap(), PathBuf::from("/tmp/from-herdr"));

        let xdg = Vars {
            config_dir: None,
            ..given.clone()
        };
        assert_eq!(
            directory(&xdg).unwrap(),
            PathBuf::from("/tmp/xdg/herdr/plugins/config/haurylau.voice")
        );

        let home = Vars {
            config_dir: None,
            xdg_config_home: None,
            home: Some("/tmp/home".into()),
        };
        assert_eq!(
            directory(&home).unwrap(),
            PathBuf::from("/tmp/home/.config/herdr/plugins/config/haurylau.voice")
        );

        let nothing = Vars {
            config_dir: None,
            xdg_config_home: None,
            home: None,
        };
        assert_eq!(directory(&nothing), None);
    }
}
