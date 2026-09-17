//! `setup`: the keybindings this plugin needs, and putting them into the user's
//! herdr configuration. A herdr plugin manifest cannot declare keys — see
//! `docs/design.md` section 7 — so this action bridges the gap.

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
    /// `(key, action)` for every string assignment directly under `[keys]` —
    /// `prefix`, `goto`, `new_tab` and the rest. The action's name travels with
    /// the key because a report that says only "some herdr action" leaves the
    /// person with nothing to go and look at. herdr's own defaults are not
    /// visible here: only what the user wrote down is.
    pub reserved_keys: Vec<(String, String)>,
    /// Every key carried by a `[[keys.command]]` block whose command could not
    /// be read. herdr honours the block regardless, so the key is taken, and
    /// nothing in the file says by what.
    pub unnamed_command_keys: Vec<String>,
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
                let Some(key) = block.get("key").and_then(|v| v.as_str()) else {
                    continue;
                };
                match block.get("command").and_then(|v| v.as_str()) {
                    Some(command) => existing
                        .commands
                        .push((key.to_string(), command.to_string())),
                    None => existing.unnamed_command_keys.push(key.to_string()),
                }
            }
        } else if let Some(bound) = value.as_str() {
            if !bound.is_empty() {
                existing
                    .reserved_keys
                    .push((bound.to_string(), name.to_string()));
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
                    .find(|(k, _)| k == binding.key)
                    .map(|(_, action)| format!("the herdr action {action:?}"))
            })
            .or_else(|| {
                existing
                    .unnamed_command_keys
                    .iter()
                    .find(|k| *k == binding.key)
                    .map(|_| {
                        "a [[keys.command]] block that names no command, so the run \
                         cannot say what it does"
                            .to_string()
                    })
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

    /// Points at an arbitrary program instead of at what `HERDR_BIN_PATH`
    /// names, so a test can pin the argument lists, the environment the child
    /// is given and both exit statuses against a small recorder script —
    /// without mutating an environment variable a parallel suite shares. The
    /// released build has one way to resolve the binary, `new()`, and a second
    /// constructor nothing calls there would trip `dead_code`.
    #[cfg(test)]
    pub fn with_binary(binary: impl Into<String>) -> Self {
        HerdrCli {
            binary: binary.into(),
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

#[derive(Debug)]
pub enum WriteError {
    /// The file, or its directory, would not cooperate. Names the path it
    /// happened on and what the system said.
    Io { path: String, reason: String },
    /// herdr already reports issues with the file as it stands.
    OriginalRejected { path: String, output: String },
    /// herdr rejects the file this action would have produced.
    CandidateRejected { path: String, output: String },
    /// herdr could not be asked.
    Herdr(HerdrError),
}

impl std::fmt::Display for WriteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WriteError::Io { path, reason } => {
                write!(f, "cannot write {path}: {reason}")
            }
            WriteError::OriginalRejected { path, output } => write!(
                f,
                "herdr already reports issues with {path}, so nothing was added — a \
                 configuration it is ignoring would swallow the binding silently. Fix \
                 what it names and run this again.\n{output}"
            ),
            WriteError::CandidateRejected { path, output } => write!(
                f,
                "herdr refused the configuration this would have written, so {path} is \
                 unchanged.\n{output}"
            ),
            WriteError::Herdr(e) => write!(f, "{e}"),
        }
    }
}

/// Appends `addition` to the file at `path`, with herdr's approval and nobody
/// else's bytes lost.
///
/// The original is checked first: a configuration herdr already complains about
/// is one it is ignoring, wholly or in part, and a binding appended to it would
/// do nothing when pressed — a failure that would look like this action's.
///
/// The write is a candidate beside the real file plus a rename, so the only
/// moment the real file changes is the rename, and an interrupted run cannot
/// leave a half-written configuration. The candidate takes the original's
/// permissions first, so replacing a file does not change its mode.
///
/// A path that is a symbolic link is resolved first, and the file it points at
/// is the one that is read, written beside and renamed over.
pub fn append(herdr: &dyn Herdr, path: &Path, addition: &str) -> Result<(), WriteError> {
    let io = |path: &Path, e: std::io::Error| WriteError::Io {
        path: path.display().to_string(),
        reason: e.to_string(),
    };

    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| io(dir, e))?;
    }

    // A configuration file is often a symbolic link into a dotfiles
    // repository. The file that is read, written beside and renamed over is the
    // one the link points at, so the link survives and the real file is the one
    // that gains the bindings. Renaming over the link itself would leave a
    // regular file in its place and the real file unchanged, while the run
    // reported success. A path that does not exist resolves to itself.
    let target = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let target = target.as_path();

    let original = match std::fs::read_to_string(target) {
        Ok(text) => Some(text),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(io(path, e)),
    };

    if original.is_some() {
        let verdict = herdr.check_config(target).map_err(WriteError::Herdr)?;
        if !verdict.ok {
            return Err(WriteError::OriginalRejected {
                path: path.display().to_string(),
                output: verdict.output,
            });
        }
    }

    let mut candidate_text = original.clone().unwrap_or_default();
    if !candidate_text.is_empty() && !candidate_text.ends_with('\n') {
        candidate_text.push('\n');
    }
    candidate_text.push('\n');
    candidate_text.push_str(addition);

    let candidate = target.with_extension("toml.herdr-voice-candidate");
    std::fs::write(&candidate, &candidate_text).map_err(|e| io(&candidate, e))?;
    // The original exists, so carry its mode onto the candidate: the rename
    // must not silently change the file's permissions.
    if original.is_some() {
        if let Ok(meta) = std::fs::metadata(target) {
            let _ = std::fs::set_permissions(&candidate, meta.permissions());
        }
    }

    let verdict = match herdr.check_config(&candidate) {
        Ok(v) => v,
        Err(e) => {
            let _ = std::fs::remove_file(&candidate);
            return Err(WriteError::Herdr(e));
        }
    };
    if !verdict.ok {
        let _ = std::fs::remove_file(&candidate);
        return Err(WriteError::CandidateRejected {
            path: path.display().to_string(),
            output: verdict.output,
        });
    }

    std::fs::rename(&candidate, target).map_err(|e| {
        let _ = std::fs::remove_file(&candidate);
        io(path, e)
    })
}

/// "1 binding" or "2 bindings" — the count belongs in the question, because the
/// question sits under a wall of TOML and has to say what answering it does.
fn plural_bindings(count: usize) -> String {
    if count == 1 {
        "1 binding".to_string()
    } else {
        format!("{count} bindings")
    }
}

/// The whole action. `interactive` decides which half runs, and it is a fact
/// about the process rather than a flag: a pane has a terminal, the action herdr
/// starts does not.
pub fn run(
    herdr: &dyn Herdr,
    path: Option<PathBuf>,
    interactive: bool,
    answer: &mut dyn FnMut() -> Option<String>,
    out: &mut dyn std::io::Write,
) -> u8 {
    if !interactive {
        return match herdr.open_pane() {
            Ok(()) => 0,
            Err(e) => {
                let body = format!(
                    "could not open the setup pane: {e}. Close any popup that is open, \
                     or run `herdr-voice setup` in a terminal."
                );
                let _ = herdr.notify("Dictation: setup", &body);
                let _ = writeln!(out, "{body}");
                1
            }
        };
    }

    let Some(path) = path else {
        let _ = writeln!(
            out,
            "cannot tell where your herdr configuration is: neither \
             HERDR_CONFIG_PATH, XDG_CONFIG_HOME nor HOME is set. Set \
             HERDR_CONFIG_PATH to the file and run this again."
        );
        return 1;
    };

    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => {
            let _ = writeln!(out, "cannot read {}: {e}", path.display());
            return 1;
        }
    };

    let existing = match inspect(&text) {
        Ok(existing) => existing,
        Err(why) => {
            let _ = writeln!(
                out,
                "cannot read {} as TOML, so nothing was changed: {why}",
                path.display()
            );
            return 1;
        }
    };

    let decision = decide(&existing);

    for (binding, key) in &decision.already {
        let _ = writeln!(
            out,
            "already there: {} is bound to {key}",
            binding.command()
        );
    }
    for (binding, holder) in &decision.blocked {
        let _ = writeln!(
            out,
            "not added: {} is already held by {holder} in {}. Bind {} to a key of \
             your choosing by hand.",
            binding.key,
            path.display(),
            binding.command()
        );
    }

    if decision.to_add.is_empty() {
        if decision.blocked.is_empty() {
            let _ = writeln!(out, "nothing to add to {}.", path.display());
        } else {
            let _ = writeln!(
                out,
                "\nnothing was added to {}: {} of the three bindings are on keys \
                 something else already holds, named above. Free those keys and run \
                 this again, or bind the actions to keys of your choosing by hand.",
                path.display(),
                decision.blocked.len()
            );
        }
        return 0;
    }

    let snippet = render(&decision.to_add);
    let _ = writeln!(out, "\nthese go into {}:\n\n{snippet}", path.display());
    let _ = write!(
        out,
        "append these {} to the file named above? [y/N] ",
        plural_bindings(decision.to_add.len())
    );
    // The question has no newline of its own, and standard output is line
    // buffered: without this flush it sits in the buffer while the process
    // blocks on the answer, and the person is looking at a cursor on an empty
    // line with nothing to tell them what it wants. Found by looking at the
    // pane, after 492 tests, four gate rounds and a mutation review had all
    // passed — every one of them writes into a buffer that needs no flushing.
    let _ = out.flush();

    let said = answer().unwrap_or_default();
    if !matches!(said.trim().to_ascii_lowercase().as_str(), "y" | "yes") {
        let _ = writeln!(out, "\nnothing was changed.");
        return 0;
    }

    match append(herdr, &path, &snippet) {
        Ok(()) => {
            let _ = writeln!(out, "\nadded to {}:", path.display());
            for binding in &decision.to_add {
                let _ = writeln!(out, "  {} on {}", binding.command(), binding.key);
            }
            let _ = writeln!(
                out,
                "\nthe running herdr does not see this until you run \
                 `herdr server reload-config`, or press prefix+shift+r."
            );
            0
        }
        Err(e) => {
            let _ = writeln!(out, "\n{e}");
            1
        }
    }
}

/// What `src/main.rs` calls: resolves the path, asks the process whether it has
/// a terminal, and reads the answer from standard input.
pub fn main() -> u8 {
    use std::io::{BufRead, IsTerminal};
    let herdr = HerdrCli::new();
    let interactive = std::io::stdin().is_terminal();
    let mut answer = || {
        let mut line = String::new();
        std::io::stdin().lock().read_line(&mut line).ok()?;
        Some(line)
    };
    let mut out = std::io::stdout();
    run(&herdr, config_path(), interactive, &mut answer, &mut out)
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

    use tests_support::FakeHerdr;

    /// A writer that records whether it has been flushed, shared with the
    /// closure that answers the question, so a test can ask what the person
    /// could actually see at the moment the process began waiting for them.
    #[derive(Clone, Default)]
    struct FlushWatcher {
        written: std::sync::Arc<std::sync::Mutex<Vec<u8>>>,
        flushed_bytes: std::sync::Arc<std::sync::Mutex<usize>>,
    }

    impl std::io::Write for FlushWatcher {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.written.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            *self.flushed_bytes.lock().unwrap() = self.written.lock().unwrap().len();
            Ok(())
        }
    }

    /// Standard output is line buffered and the question ends without a
    /// newline, so a question that is written and not flushed never reaches the
    /// screen while the process waits for the answer. In the pane this showed as
    /// a cursor sitting on an empty line with nothing above it to answer.
    #[test]
    fn the_question_has_reached_the_screen_before_the_answer_is_waited_for() {
        let path = scratch("flush");
        std::fs::write(&path, "[theme]\n").unwrap();
        let watcher = FlushWatcher::default();
        let seen = watcher.clone();
        let mut writer = watcher.clone();
        let visible_when_asked = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
        let record = visible_when_asked.clone();

        let code = run(
            &FakeHerdr::clean(),
            Some(path),
            true,
            &mut || {
                let flushed = *seen.flushed_bytes.lock().unwrap();
                let written = seen.written.lock().unwrap().clone();
                *record.lock().unwrap() = String::from_utf8_lossy(&written[..flushed]).to_string();
                Some("n".to_string())
            },
            &mut writer,
        );

        assert_eq!(code, 0);
        let on_screen = visible_when_asked.lock().unwrap().clone();
        assert!(
            on_screen.contains("[y/N]"),
            "the question had not reached the screen when the process started \
             waiting for the answer; all that was flushed was: {on_screen:?}"
        );
        assert!(
            on_screen.contains("3 bindings"),
            "the question must say what answering it does: {on_screen:?}"
        );
    }

    fn capture(
        herdr: &dyn Herdr,
        path: Option<std::path::PathBuf>,
        interactive: bool,
        answers: Vec<&str>,
    ) -> (u8, String) {
        let mut answers: Vec<String> = answers.into_iter().map(String::from).collect();
        let mut out: Vec<u8> = Vec::new();
        let code = run(
            herdr,
            path,
            interactive,
            &mut || {
                if answers.is_empty() {
                    None
                } else {
                    Some(answers.remove(0))
                }
            },
            &mut out,
        );
        (code, String::from_utf8(out).unwrap())
    }

    #[test]
    fn without_a_terminal_it_opens_the_pane_and_says_nothing_else() {
        let herdr = FakeHerdr::clean();
        let (code, _) = capture(&herdr, None, false, vec![]);
        assert_eq!(code, 0);
        assert_eq!(herdr.calls(), vec![tests_support::Call::OpenPane]);
    }

    #[test]
    fn a_yes_appends_and_names_the_file_and_what_it_added() {
        let path = scratch("run-yes");
        std::fs::write(&path, "[theme]\n").unwrap();
        let (code, said) = capture(&FakeHerdr::clean(), Some(path.clone()), true, vec!["y"]);
        assert_eq!(code, 0, "{said}");
        assert!(said.contains(&path.display().to_string()), "{said}");
        assert!(said.contains("ctrl+g"), "{said}");
        assert!(
            said.contains("herdr server reload-config"),
            "a binding in a file the running server has not reread does nothing: {said}"
        );
        assert!(std::fs::read_to_string(&path).unwrap().contains("prefix+i"));
    }

    #[test]
    fn a_declined_offer_changes_nothing_and_is_not_a_failure() {
        let path = scratch("run-no");
        std::fs::write(&path, "[theme]\n").unwrap();
        let (code, _) = capture(&FakeHerdr::clean(), Some(path.clone()), true, vec!["n"]);
        assert_eq!(code, 0);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "[theme]\n");
    }

    #[test]
    fn a_second_run_adds_nothing_and_says_they_are_already_there() {
        let path = scratch("run-twice");
        std::fs::write(&path, "[theme]\n").unwrap();
        capture(&FakeHerdr::clean(), Some(path.clone()), true, vec!["y"]);
        let before = std::fs::read_to_string(&path).unwrap();
        let (code, said) = capture(&FakeHerdr::clean(), Some(path.clone()), true, vec!["y"]);
        assert_eq!(code, 0, "{said}");
        assert!(said.contains("already"), "{said}");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), before);
    }

    #[test]
    fn a_key_held_by_something_else_is_named_and_the_rest_still_land() {
        let path = scratch("run-collision");
        std::fs::write(
            &path,
            "[[keys.command]]\nkey = \"ctrl+g\"\ntype = \"shell\"\ncommand = \"echo hi\"\n",
        )
        .unwrap();
        let (code, said) = capture(&FakeHerdr::clean(), Some(path.clone()), true, vec!["y"]);
        assert_eq!(code, 0, "{said}");
        assert!(said.contains("ctrl+g"), "{said}");
        assert!(
            said.contains("echo hi"),
            "it must name what holds the key: {said}"
        );
        let after = std::fs::read_to_string(&path).unwrap();
        assert!(after.contains("prefix+i"), "the other two still land");
        assert!(!after.contains("haurylau.voice.ptt"));
    }

    #[test]
    fn with_no_path_to_resolve_it_says_so_rather_than_guessing() {
        let (code, said) = capture(&FakeHerdr::clean(), None, true, vec!["y"]);
        assert_eq!(code, 1);
        assert!(said.contains("HERDR_CONFIG_PATH"), "{said}");
    }

    fn scratch(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("herdr-voice-setup-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("config.toml")
    }

    #[test]
    fn a_clean_append_lands_and_keeps_every_byte_that_was_there() {
        let path = scratch("clean");
        std::fs::write(&path, "[theme]\nname = \"something\"\n").unwrap();
        append(
            &FakeHerdr::clean(),
            &path,
            "[[keys.command]]\nkey = \"ctrl+g\"\n",
        )
        .unwrap();
        let after = std::fs::read_to_string(&path).unwrap();
        assert!(after.starts_with("[theme]\nname = \"something\"\n"));
        assert!(after.contains("key = \"ctrl+g\""));
    }

    /// A configuration file kept in a dotfiles repository and linked into place
    /// is the arrangement this must survive: renaming over the link would leave
    /// a regular file in its place and the real file without the bindings,
    /// while the run reported success.
    #[test]
    #[cfg(unix)]
    fn a_configuration_that_is_a_link_keeps_the_link_and_changes_what_it_points_at() {
        let path = scratch("symlink");
        let target = path.parent().unwrap().join("real-config.toml");
        std::fs::write(&target, "[theme]\n").unwrap();
        std::os::unix::fs::symlink(&target, &path).unwrap();
        append(
            &FakeHerdr::clean(),
            &path,
            "[[keys.command]]\nkey = \"ctrl+g\"\n",
        )
        .unwrap();
        assert!(
            std::fs::symlink_metadata(&path)
                .unwrap()
                .file_type()
                .is_symlink(),
            "the link itself must still be a link"
        );
        let after = std::fs::read_to_string(&target).unwrap();
        assert!(after.starts_with("[theme]\n"), "{after}");
        assert!(
            after.contains("key = \"ctrl+g\""),
            "the file the link points at is the one that gains the bindings: {after}"
        );
    }

    /// The one path by which the offer reaches a person who has no keybinding
    /// yet: the action opens this pane, and the pane runs the interactive half.
    /// A renamed or removed entry leaves the feature installed and unreachable,
    /// and `scripts/check_manifest.py` cannot see it — it checks that every
    /// command named is one the binary accepts, not that this entry exists.
    #[test]
    fn the_manifest_declares_the_pane_the_action_asks_herdr_to_open() {
        let manifest: toml::Value =
            toml::from_str(&std::fs::read_to_string("herdr-plugin.toml").unwrap()).unwrap();
        let args = open_pane_args();
        let entrypoint = args[args.iter().position(|a| *a == "--entrypoint").unwrap() + 1];
        let panes = manifest["panes"].as_array().unwrap();
        let pane = panes
            .iter()
            .find(|p| p["id"].as_str() == Some(entrypoint))
            .unwrap_or_else(|| {
                panic!(
                    "the manifest declares no pane with the id {entrypoint:?}, which is \
                        the entrypoint this action asks herdr to open"
                )
            });
        let command: Vec<&str> = pane["command"]
            .as_array()
            .unwrap()
            .iter()
            .map(|a| a.as_str().unwrap())
            .collect();
        assert_eq!(
            command.get(1),
            Some(&"setup"),
            "the pane must run the setup subcommand, got {command:?}"
        );
    }

    /// The design rests on the write being one rename: an interrupted copy
    /// would leave a half-written configuration, which herdr answers with a
    /// parse error and a fall back to its defaults. A copy in place keeps the
    /// inode; a rename does not.
    #[test]
    #[cfg(unix)]
    fn the_file_is_replaced_by_a_rename_rather_than_written_in_place() {
        use std::os::unix::fs::MetadataExt;
        let path = scratch("rename");
        std::fs::write(&path, "[theme]\n").unwrap();
        let before = std::fs::metadata(&path).unwrap().ino();
        append(&FakeHerdr::clean(), &path, "[[keys.command]]\n").unwrap();
        let after = std::fs::metadata(&path).unwrap().ino();
        assert_ne!(
            before, after,
            "the configuration must arrive by a rename, not by being written over"
        );
    }

    /// The file this edits is hand-written and commented, and the bindings in
    /// it carry the reasons their keys were chosen.
    #[test]
    fn a_commented_hand_written_file_keeps_every_byte_it_had() {
        let path = scratch("hand-written");
        let original = "# my herdr configuration\n\
                        # the prefix stays on ctrl+b — ctrl+a belongs to the shell\n\
                        \n\
                        [keys]\n\
                        prefix = \"ctrl+b\"\n\
                        \n\
                        [[keys.command]]\n\
                        \tkey = \"prefix+t\"\n\
                        \ttype = \"shell\"\n\
                        \tcommand = \"echo hello\"  # a naïve chord: alt+g typed © here\n";
        std::fs::write(&path, original).unwrap();
        append(
            &FakeHerdr::clean(),
            &path,
            "[[keys.command]]\nkey = \"ctrl+g\"\n",
        )
        .unwrap();
        let after = std::fs::read_to_string(&path).unwrap();
        assert!(
            after.starts_with(original),
            "nobody's bytes may be lost, got:\n{after}"
        );
        assert!(after.contains("key = \"ctrl+g\""), "{after}");
    }

    /// The last line of a file with no trailing newline would otherwise have
    /// the first line of the block glued onto it, and the parse error that
    /// makes drops the whole configuration.
    #[test]
    fn a_file_that_does_not_end_in_a_newline_is_not_glued_to_the_block() {
        let path = scratch("no-trailing-newline");
        std::fs::write(&path, "[theme]\nname = \"something\"").unwrap();
        append(
            &FakeHerdr::clean(),
            &path,
            "[[keys.command]]\nkey = \"ctrl+g\"\n",
        )
        .unwrap();
        let after = std::fs::read_to_string(&path).unwrap();
        assert!(
            after.contains("name = \"something\"\n\n[[keys.command]]"),
            "the last line must keep a line of its own, with a blank line before \
             the block: {after:?}"
        );
        let parsed: toml::Value = toml::from_str(&after)
            .unwrap_or_else(|e| panic!("the result must still parse as TOML: {e}\n{after}"));
        assert_eq!(parsed["theme"]["name"].as_str(), Some("something"));
        assert_eq!(parsed["keys"]["command"].as_array().unwrap().len(), 1);
    }

    /// A configuration is a file people keep private, and the rename must not
    /// hand it the candidate's fresh permissions.
    #[test]
    #[cfg(unix)]
    fn the_mode_the_original_had_is_the_mode_the_result_has() {
        use std::os::unix::fs::PermissionsExt;
        let path = scratch("mode");
        std::fs::write(&path, "[theme]\n").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        append(&FakeHerdr::clean(), &path, "[[keys.command]]\n").unwrap();
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "got {mode:o}");
    }

    #[test]
    fn a_file_that_does_not_exist_is_created_with_its_directory() {
        let path = scratch("absent")
            .parent()
            .unwrap()
            .join("deeper/config.toml");
        append(&FakeHerdr::clean(), &path, "[[keys.command]]\n").unwrap();
        assert!(path.exists());
    }

    #[test]
    fn an_original_herdr_already_complains_about_is_left_alone() {
        let path = scratch("dirty-original");
        std::fs::write(&path, "[nonsense]\nfoo = 1\n").unwrap();
        let herdr = FakeHerdr::answering(vec![Check {
            ok: false,
            output: "config: issues found\nunknown config section [nonsense]".into(),
        }]);
        let err = append(&herdr, &path, "[[keys.command]]\n").unwrap_err();
        assert!(
            matches!(err, WriteError::OriginalRejected { .. }),
            "{err:?}"
        );
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "[nonsense]\nfoo = 1\n",
            "nothing may be written into a configuration herdr is already unhappy with"
        );
        // Not only unchanged: untouched. The original is judged before the
        // candidate is written, so no candidate is left beside it.
        let leftovers: Vec<_> = std::fs::read_dir(path.parent().unwrap())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(
            leftovers.len(),
            1,
            "a candidate must never be written next to a configuration that was \
             rejected before it: {leftovers:?}"
        );
    }

    #[test]
    fn a_candidate_herdr_rejects_leaves_the_original_untouched() {
        let path = scratch("dirty-candidate");
        std::fs::write(&path, "[theme]\n").unwrap();
        let herdr = FakeHerdr::answering(vec![
            Check {
                ok: true,
                output: "config: ok".into(),
            },
            Check {
                ok: false,
                output: "config: issues found\nctrl+g: disabled".into(),
            },
        ]);
        let err = append(&herdr, &path, "[[keys.command]]\n").unwrap_err();
        assert!(
            matches!(err, WriteError::CandidateRejected { .. }),
            "{err:?}"
        );
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "[theme]\n");
        let leftovers: Vec<_> = std::fs::read_dir(path.parent().unwrap())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(
            leftovers.len(),
            1,
            "the candidate must be removed: {leftovers:?}"
        );
    }

    /// The fixture is a directory that cannot be written, not a file: renaming
    /// over a read-only file succeeds while its parent is writable, so that
    /// fixture would prove nothing.
    #[test]
    #[cfg(unix)]
    fn a_directory_that_cannot_be_written_is_reported_with_its_path() {
        use std::os::unix::fs::PermissionsExt;
        let path = scratch("readonly-dir");
        std::fs::write(&path, "[theme]\n").unwrap();
        let dir = path.parent().unwrap();
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o500)).unwrap();
        let err = append(&FakeHerdr::clean(), &path, "[[keys.command]]\n").unwrap_err();
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700)).unwrap();
        match err {
            WriteError::Io { path: ref p, .. } => {
                assert!(p.contains("herdr-voice-setup-readonly-dir"), "{p}")
            }
            other => panic!("expected an io failure naming the path, got {other:?}"),
        }
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

    /// The whole chain from the trait method to the process: the argument
    /// list, the environment the child is given, and the exit status coming
    /// back as a verdict. A shell script, so these run on unix only; the
    /// released binary's own behaviour here is platform-independent, and what
    /// is platform-specific is the recorder, not the call.
    #[cfg(unix)]
    mod herdr_cli {
        use super::super::*;

        /// Writes the argv it was given, one line each, and the value of
        /// `HERDR_CONFIG_PATH` it inherited, to a file next to it; then prints
        /// `text` and exits with `code`. The pattern and the reasoning are
        /// `src/delivery.rs`'s, which records argv the same way.
        struct Recorder {
            dir: std::path::PathBuf,
            script: std::path::PathBuf,
            out: std::path::PathBuf,
        }

        /// The scratch directory is unique per process and per tag, so runs
        /// never collide; nothing else would remove it.
        impl Drop for Recorder {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.dir);
            }
        }

        impl Recorder {
            fn new(tag: &str, text: &str, code: i32) -> Self {
                use std::os::unix::fs::PermissionsExt;
                let dir = std::env::temp_dir().join(format!(
                    "herdr-voice-setup-recorder-{tag}-{}",
                    std::process::id()
                ));
                let _ = std::fs::remove_dir_all(&dir);
                std::fs::create_dir_all(&dir).expect("scratch dir");
                let recorder = Recorder {
                    script: dir.join("record.sh"),
                    out: dir.join("record.out"),
                    dir,
                };
                std::fs::write(
                    &recorder.script,
                    format!(
                        "#!/bin/sh\n\
                         {{ printf '%s\\n' \"$@\"; \
                         printf 'HERDR_CONFIG_PATH=%s\\n' \"${{HERDR_CONFIG_PATH-unset}}\"; \
                         }} > {out:?}\n\
                         echo {text:?}\n\
                         exit {code}\n",
                        out = recorder.out,
                        text = text,
                        code = code,
                    ),
                )
                .expect("write the recorder script");
                let mut perms = std::fs::metadata(&recorder.script)
                    .expect("stat")
                    .permissions();
                perms.set_mode(0o755);
                std::fs::set_permissions(&recorder.script, perms).expect("chmod");
                recorder
            }

            fn cli(&self) -> HerdrCli {
                HerdrCli::with_binary(self.script.to_string_lossy().into_owned())
            }

            fn recorded(&self) -> Vec<String> {
                std::fs::read_to_string(&self.out)
                    .expect("the recorder must have run and written what it was given")
                    .lines()
                    .map(|l| l.to_string())
                    .collect()
            }
        }

        /// The file to judge travels in the child's environment, because that
        /// is where `herdr config check` looks for it. Without it the check
        /// judges the user's real configuration and answers about the wrong
        /// file.
        #[test]
        fn check_config_points_herdr_at_the_file_it_is_judging() {
            let recorder = Recorder::new("check-ok", "config: ok", 0);
            let judged = recorder.dir.join("candidate.toml");
            let verdict = recorder
                .cli()
                .check_config(&judged)
                .expect("the recorder always runs");
            let recorded = recorder.recorded();
            assert_eq!(recorded[..2], ["config".to_string(), "check".to_string()]);
            assert_eq!(
                recorded.last().unwrap(),
                &format!("HERDR_CONFIG_PATH={}", judged.display()),
                "got {recorded:?}"
            );
            assert!(verdict.ok, "{verdict:?}");
            assert_eq!(verdict.output, "config: ok");
        }

        #[test]
        fn a_non_zero_exit_from_the_check_is_a_refusal_carrying_what_herdr_said() {
            let recorder = Recorder::new("check-bad", "config: issues found", 1);
            let verdict = recorder
                .cli()
                .check_config(&recorder.dir.join("candidate.toml"))
                .expect("the recorder always runs");
            assert!(!verdict.ok, "{verdict:?}");
            assert_eq!(verdict.output, "config: issues found");
        }

        #[test]
        fn open_pane_asks_herdr_for_this_plugin_s_setup_entrypoint() {
            let recorder = Recorder::new("open-pane", "ok", 0);
            recorder.cli().open_pane().expect("the recorder succeeds");
            assert_eq!(
                recorder.recorded()[..7],
                [
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

        #[test]
        fn a_refused_pane_is_reported_with_what_herdr_said() {
            let recorder = Recorder::new("open-pane-busy", "a popup pane is already open", 1);
            let err = recorder.cli().open_pane().unwrap_err();
            assert_eq!(
                err,
                HerdrError::Rejected("a popup pane is already open".to_string())
            );
        }

        #[test]
        fn notify_runs_notification_show_with_a_body_flag() {
            let recorder = Recorder::new("notify", "ok", 0);
            recorder
                .cli()
                .notify("Dictation: setup", "could not open the setup pane")
                .expect("the recorder succeeds");
            assert_eq!(
                recorder.recorded()[..5],
                [
                    "notification",
                    "show",
                    "Dictation: setup",
                    "--body",
                    "could not open the setup pane",
                ]
            );
        }

        /// herdr starts plugin commands with a minimal PATH, so "not on the
        /// PATH" is a case that happens, and the message has to name the
        /// binary and the PATH the process actually had.
        #[test]
        fn a_binary_that_cannot_be_started_is_reported_as_not_found() {
            let cli = HerdrCli::with_binary("herdr-voice-no-such-program");
            let err = cli
                .check_config(std::path::Path::new("config.toml"))
                .unwrap_err();
            match err {
                HerdrError::NotFound { binary, .. } => {
                    assert_eq!(binary, "herdr-voice-no-such-program")
                }
                other => panic!("expected a not-found failure, got {other:?}"),
            }
            assert!(format!("{}", cli.open_pane().unwrap_err()).contains("HERDR_BIN_PATH"));
            assert!(cli.notify("t", "b").is_err());
        }
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
            ..Existing::default()
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
            ..Existing::default()
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
            reserved_keys: vec![("prefix+i".into(), "goto".into())],
            ..Existing::default()
        };
        let d = decide(&existing);
        assert_eq!(d.blocked.len(), 1);
        assert_eq!(d.blocked[0].0.action, "dictate");
    }

    #[test]
    fn our_own_binding_on_our_own_key_counts_as_present_not_as_a_collision() {
        let existing = Existing {
            commands: vec![("ctrl+g".into(), "haurylau.voice.ptt".into())],
            ..Existing::default()
        };
        let d = decide(&existing);
        assert_eq!(d.already.len(), 1);
        assert!(d.blocked.is_empty(), "it is ours; it is not in the way");
    }

    /// AC-7 asks what the key is bound to, and the value of the answer is that
    /// the person can go and look at the thing that is in the way.
    #[test]
    fn a_key_held_by_a_herdr_action_is_reported_with_that_action_s_name() {
        let existing = inspect("[keys]\ngoto = \"prefix+i\"\n").unwrap();
        let d = decide(&existing);
        assert_eq!(d.blocked.len(), 1);
        assert_eq!(d.blocked[0].0.action, "dictate");
        assert!(
            d.blocked[0].1.contains("goto"),
            "it must name the action holding the key, got {:?}",
            d.blocked[0].1
        );
    }

    #[test]
    fn the_run_names_the_herdr_action_that_holds_the_key() {
        let path = scratch("run-herdr-action");
        std::fs::write(&path, "[keys]\ngoto = \"prefix+i\"\n").unwrap();
        let (code, said) = capture(&FakeHerdr::clean(), Some(path.clone()), true, vec!["y"]);
        assert_eq!(code, 0, "{said}");
        assert!(
            said.contains("goto"),
            "the report must name the herdr action holding prefix+i: {said}"
        );
    }

    /// A block herdr will honour whatever else it is missing. Offering a second
    /// binding on its key would produce an opaque refusal from
    /// `herdr config check` instead of the diagnosis this action exists to give.
    #[test]
    fn a_command_block_with_a_key_and_no_command_still_reserves_that_key() {
        let existing = inspect("[[keys.command]]\nkey = \"ctrl+g\"\ntype = \"shell\"\n").unwrap();
        let d = decide(&existing);
        assert_eq!(d.to_add.len(), 2, "the other two are still added");
        assert_eq!(d.blocked.len(), 1);
        assert_eq!(d.blocked[0].0.action, "ptt");
        assert!(
            d.blocked[0].1.contains("names no command"),
            "it must say plainly that the run cannot identify what holds the key, got {:?}",
            d.blocked[0].1
        );
    }

    #[test]
    fn a_command_block_that_cannot_be_identified_is_reported_and_not_shadowed() {
        let path = scratch("run-unnamed-block");
        std::fs::write(
            &path,
            "[[keys.command]]\nkey = \"ctrl+g\"\ntype = \"shell\"\n",
        )
        .unwrap();
        let (code, said) = capture(&FakeHerdr::clean(), Some(path.clone()), true, vec!["y"]);
        assert_eq!(code, 0, "{said}");
        assert!(said.contains("ctrl+g"), "{said}");
        assert!(said.contains("names no command"), "{said}");
        let after = std::fs::read_to_string(&path).unwrap();
        assert!(
            !after.contains("haurylau.voice.ptt"),
            "the key is taken, so nothing of ours goes on it: {after}"
        );
        assert!(after.contains("prefix+i"), "the other two still land");
    }

    /// Nothing was added at all, and the closing line has to say that rather
    /// than read like a run that had nothing left to do.
    #[test]
    fn with_every_key_taken_it_says_nothing_was_added_rather_than_nothing_to_add() {
        let path = scratch("run-all-taken");
        std::fs::write(
            &path,
            "[[keys.command]]\nkey = \"ctrl+g\"\ntype = \"shell\"\ncommand = \"one\"\n\n\
             [[keys.command]]\nkey = \"prefix+i\"\ntype = \"shell\"\ncommand = \"two\"\n\n\
             [[keys.command]]\nkey = \"ctrl+shift+g\"\ntype = \"shell\"\ncommand = \"three\"\n",
        )
        .unwrap();
        let before = std::fs::read_to_string(&path).unwrap();
        let (code, said) = capture(&FakeHerdr::clean(), Some(path.clone()), true, vec!["y"]);
        assert_eq!(code, 0, "{said}");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), before);
        for holder in ["one", "two", "three"] {
            assert!(said.contains(holder), "every holder is named: {said}");
        }
        assert!(
            said.contains("nothing was added"),
            "three refusals must not close on a line that reads like success: {said}"
        );
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
        assert!(existing
            .reserved_keys
            .contains(&("prefix+g".to_string(), "goto".to_string())));
        assert!(existing
            .reserved_keys
            .contains(&("ctrl+b".to_string(), "prefix".to_string())));
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
