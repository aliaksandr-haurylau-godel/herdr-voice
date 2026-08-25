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

// `Eq` is absent on purpose: `silence_db` is an `f32`, which has no total
// ordering. Nothing here needs more than `PartialEq`.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct Config {
    pub audio: Audio,
    pub stt: Stt,
    pub rewrite: Rewrite,
    pub ui: Ui,
    pub delivery: Delivery,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default)]
pub struct Audio {
    /// The input device's name. Empty means the system default. Never an index:
    /// indices shift when a headset is connected and the recording goes elsewhere
    /// in silence.
    pub input: String,
    /// A take quieter than this, in decibels relative to full scale, is refused as
    /// the wrong input rather than passed on as speech.
    pub silence_db: f32,
}

impl Default for Audio {
    fn default() -> Self {
        Audio {
            input: String::new(),
            silence_db: -60.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default)]
pub struct Stt {
    /// A model identifier, not a file name: the file is `ggml-<model>.bin`.
    pub model: String,
    /// `candle`, `http` or `command`. The first two are not built yet.
    pub engine: String,
    /// The spoken language, or `auto` to let the engine decide.
    pub language: String,
    /// The program and its arguments for `engine = "command"`, with `{audio}`,
    /// `{model}` and `{language}` replaced before it runs. Provisional name.
    pub command: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default)]
pub struct Rewrite {
    pub engine: String,
    pub agent: String,
}

impl Default for Stt {
    fn default() -> Self {
        Stt {
            model: "large-v3-turbo".to_string(),
            engine: "candle".to_string(),
            language: "auto".to_string(),
            command: Vec::new(),
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

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default)]
pub struct Ui {
    pub toasts: bool,
}
impl Default for Ui {
    fn default() -> Self {
        Ui { toasts: true }
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default)]
pub struct Delivery {
    pub submit: bool,
}
impl Default for Delivery {
    fn default() -> Self {
        Delivery { submit: false }
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
        assert_eq!(defaults.audio.input, "");
        assert_eq!(defaults.audio.silence_db, -60.0);
        assert_eq!(defaults.stt.model, "large-v3-turbo");
        assert_eq!(defaults.stt.engine, "candle");
        assert_eq!(defaults.stt.language, "auto");
        assert!(defaults.stt.command.is_empty());
        assert_eq!(defaults.rewrite.engine, "agent");
        assert_eq!(defaults.rewrite.agent, "auto");
    }

    #[test]
    fn the_ui_and_delivery_defaults_are_set() {
        let defaults = Config::default();
        assert!(defaults.ui.toasts);
        assert!(!defaults.delivery.submit);
    }

    #[test]
    fn a_file_that_sets_neither_table_keeps_both_defaults() {
        let directory = scratch("ui-delivery-absent");
        std::fs::write(directory.join("config.toml"), "[stt]\nmodel = \"small\"\n").unwrap();
        let loaded = load(Some(&directory));
        assert!(loaded.config.ui.toasts);
        assert!(!loaded.config.delivery.submit);
    }

    #[test]
    fn the_ui_and_delivery_tables_are_read_when_present() {
        let directory = scratch("ui-delivery-set");
        std::fs::write(
            directory.join("config.toml"),
            "[ui]\ntoasts = false\n\n[delivery]\nsubmit = true\n",
        )
        .unwrap();
        let loaded = load(Some(&directory));
        assert!(!loaded.config.ui.toasts);
        assert!(loaded.config.delivery.submit);
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
            "[ptt]\nrelease_ms = 250\n\n[stt]\nmodel = \"small\"\n",
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
    fn the_audio_table_is_read_now_that_capture_uses_it() {
        let directory = scratch("audio");
        std::fs::write(
            directory.join("config.toml"),
            "[audio]\ninput = \"Headset\"\nsilence_db = -55.5\n",
        )
        .unwrap();
        let loaded = load(Some(&directory));
        assert_eq!(loaded.config.audio.input, "Headset");
        assert_eq!(loaded.config.audio.silence_db, -55.5);
        // The rest still comes from the defaults.
        assert_eq!(loaded.config.stt.model, "large-v3-turbo");
    }

    #[test]
    fn an_unknown_key_inside_audio_is_ignored() {
        let directory = scratch("audio-unknown");
        std::fs::write(
            directory.join("config.toml"),
            "[audio]\ninput = \"Headset\"\nchannels = 2\n",
        )
        .unwrap();
        let loaded = load(Some(&directory));
        assert_eq!(loaded.config.audio.input, "Headset");
        assert!(
            matches!(loaded.source, Source::File(_)),
            "an unknown key must not make the file invalid, got {:?}",
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
    fn the_recognition_keys_are_read() {
        let directory = scratch("stt");
        std::fs::write(
            directory.join("config.toml"),
            "[stt]\nengine = \"command\"\nlanguage = \"en\"\n\
             command = [\"whisper-cli\", \"-m\", \"{model}\", \"-f\", \"{audio}\"]\n",
        )
        .unwrap();
        let loaded = load(Some(&directory));
        assert_eq!(loaded.config.stt.engine, "command");
        assert_eq!(loaded.config.stt.language, "en");
        assert_eq!(loaded.config.stt.command.len(), 5);
        assert_eq!(loaded.config.stt.command[0], "whisper-cli");
        // The rest still comes from the defaults.
        assert_eq!(loaded.config.stt.model, "large-v3-turbo");
    }

    #[test]
    fn an_unknown_key_inside_stt_is_ignored() {
        let directory = scratch("stt-unknown");
        std::fs::write(
            directory.join("config.toml"),
            "[stt]\nengine = \"command\"\nbeam_size = 5\n",
        )
        .unwrap();
        let loaded = load(Some(&directory));
        assert_eq!(loaded.config.stt.engine, "command");
        assert!(
            matches!(loaded.source, Source::File(_)),
            "an unknown key must not make the file invalid, got {:?}",
            loaded.source
        );
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
