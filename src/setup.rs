//! `setup`: the keybindings this plugin needs, and putting them into the user's
//! herdr configuration. A herdr plugin manifest cannot declare keys — see
//! `docs/design.md` section 7 — so this action bridges the gap.

/// The plugin id, as `herdr-plugin.toml` declares it. A binding addresses an
/// action as `<plugin id>.<action id>`. Re-exported rather than written out a
/// second time: two copies of the id could drift apart, and the id is already
/// the one `src/transport.rs:18` names.
pub use crate::transport::{LEGACY_PLUGIN_ID, PLUGIN_ID};

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
    /// A block naming the id this plugin had before issue #73, with the key it
    /// sits on. Its key does nothing until the block is rewritten.
    pub superseded: Vec<(&'static Binding, String)>,
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
        // Before anything else: a block naming this plugin's own previous id is
        // its own past, not a stranger holding the key. Reported as blocked it
        // would tell the person to pick another key for a binding they added on
        // this plugin's instruction.
        let superseded = format!("{LEGACY_PLUGIN_ID}.{}", binding.action);
        if let Some((key, _)) = existing.commands.iter().find(|(_, c)| *c == superseded) {
            decision.superseded.push((binding, key.clone()));
            continue;
        }
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

/// The keys of every `[[keys.command]]` block naming this plugin's previous id,
/// in the order `BINDINGS` declares the actions — so the sentence the daemon
/// builds from them does not depend on the order somebody's file happens to be
/// in.
///
/// A file that does not parse has none: the daemon reads this at start and has
/// no terminal to report a broken configuration on. `setup` has one, and does.
pub fn superseded_keys(text: &str) -> Vec<String> {
    let Ok(existing) = inspect(text) else {
        return Vec::new();
    };
    BINDINGS
        .iter()
        .filter_map(|binding| {
            let command = format!("{LEGACY_PLUGIN_ID}.{}", binding.action);
            existing
                .commands
                .iter()
                .find(|(_, c)| *c == command)
                .map(|(key, _)| key.clone())
        })
        .collect()
}

/// Which kind of multi-line string a line ended inside, if any. A basic one is
/// closed only by `\"\"\"` and a literal one only by `'''`, so a `'''` inside a
/// `\"\"\"` block is text and closes nothing.
#[derive(PartialEq, Clone, Copy)]
enum Multiline {
    Basic,
    Literal,
}

/// Walks one line and answers what the line ends inside, given what it started
/// inside. A single-quoted or double-quoted string cannot span a line in TOML,
/// so only the two multi-line forms carry over; `#` outside a string starts a
/// comment, and nothing after it counts.
///
/// This is a scanner over string state, not a parser: it builds no value and
/// reads no key. It exists because the line edit below must not treat a
/// `[[keys.command]]` line inside somebody's prose as a block, nor a `\"\"\"` in a
/// comment as the start of one.
fn string_state(line: &str, inside: Option<Multiline>) -> Option<Multiline> {
    let mut at = 0;
    let mut state = inside;
    while at < line.len() {
        let rest = &line[at..];
        match state {
            Some(Multiline::Basic) => {
                // A basic string honours escapes, and a literal one does not, so
                // this is the one branch that has them. Without it `\\"\"\"` —
                // an escaped quote and two ordinary ones, which TOML accepts as
                // content — reads as a closing delimiter, and everything after
                // somebody's text is scanned as structure.
                if rest.starts_with('\\') {
                    at += 1;
                    at += line[at..].chars().next().map_or(0, char::len_utf8);
                    continue;
                }
                if rest.starts_with("\"\"\"") {
                    state = None;
                    at += 3;
                    continue;
                }
            }
            Some(Multiline::Literal) => {
                if rest.starts_with("'''") {
                    state = None;
                    at += 3;
                    continue;
                }
            }
            None => {
                if rest.starts_with('#') {
                    return None;
                }
                if rest.starts_with("\"\"\"") {
                    state = Some(Multiline::Basic);
                    at += 3;
                    continue;
                }
                if rest.starts_with("'''") {
                    state = Some(Multiline::Literal);
                    at += 3;
                    continue;
                }
                // A single-line string. It cannot reach the end of the line, so
                // it is skipped whole rather than tracked across lines. One with
                // no closing quote is scanned on from the next character, which
                // can read a delimiter inside it as one: such a file does not
                // parse, `inspect` refuses it, and the rewrite is never reached.
                if let Some(quote) = rest.chars().next().filter(|c| *c == '"' || *c == '\'') {
                    let mut escaped = false;
                    for (offset, c) in rest.char_indices().skip(1) {
                        if quote == '"' && c == '\\' && !escaped {
                            escaped = true;
                            continue;
                        }
                        if c == quote && !escaped {
                            at += offset;
                            break;
                        }
                        escaped = false;
                    }
                }
            }
        }
        at += line[at..].chars().next().map_or(1, char::len_utf8);
    }
    state
}

/// Every `command` value naming this plugin's previous id, inside a
/// `[[keys.command]]` block, rewritten to name the current one. Every other byte
/// of the file is copied through.
///
/// A line edit rather than a pass through a TOML document: the file is
/// hand-written and commented, the reason a key was chosen sits above the block
/// that uses it, and a value tree keeps neither comments nor layout. The key does
/// not change, so the comment goes on describing the binding under it.
pub fn rewrite_commands(original: &str) -> String {
    let mut out = String::with_capacity(original.len());
    let mut in_block = false;
    let mut inside: Option<Multiline> = None;
    for line in original.split_inclusive('\n') {
        // A multi-line string can hold anything, a `[[keys.command]]` line and a
        // `command =` line included. Nothing inside one is read as structure and
        // nothing inside one is rewritten: it is somebody's text, not a binding.
        let was_inside = inside.is_some();
        inside = string_state(line, inside);
        if was_inside {
            out.push_str(line);
            continue;
        }

        let trimmed = line.trim();
        if trimmed == "[[keys.command]]" {
            in_block = true;
        } else if trimmed.starts_with('[') {
            in_block = false;
        }
        let rewritten = if in_block {
            BINDINGS.iter().find_map(|binding| replaced(line, binding))
        } else {
            None
        };
        match rewritten {
            Some(line) => out.push_str(&line),
            None => out.push_str(line),
        }
    }
    out
}

/// `line` with the quoted value of a `command` assignment naming `binding`'s
/// predecessor replaced, or `None` when this line is not that assignment. A
/// comment is not an assignment, so one that merely mentions the old command is
/// left as it is.
fn replaced(line: &str, binding: &Binding) -> Option<String> {
    let (key, rest) = line.split_once('=')?;
    if key.trim() != "command" {
        return None;
    }
    let old = format!("\"{LEGACY_PLUGIN_ID}.{}\"", binding.action);
    let at = rest.find(&old)?;
    // Only blank space may precede the value: `command = x "old"` is not an
    // assignment of that value.
    if !rest[..at].trim().is_empty() {
        return None;
    }
    let new = format!("\"{PLUGIN_ID}.{}\"", binding.action);
    Some(format!(
        "{key}={}{new}{}",
        &rest[..at],
        &rest[at + old.len()..]
    ))
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
    ///
    /// `unix` as well as `test`, and not `test` alone: the only caller is the
    /// `herdr_cli` module below, which drives a recorder written as a shell
    /// script and is therefore unix-only. Gated on `test` alone this is dead
    /// code in the Windows test build, and CI compiles with `-D warnings`.
    /// It passed on macOS and failed on `windows-latest`.
    #[cfg(all(test, unix))]
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

/// Replaces the file at `path` with whatever `make` returns for its current
/// text, with herdr's approval and nobody else's bytes lost.
///
/// The original is checked first: a configuration herdr already complains about
/// is one it is ignoring, wholly or in part, and a binding written into it would
/// do nothing when pressed — a failure that would look like this action's.
///
/// The write is a candidate beside the real file plus a rename, so the only
/// moment the real file changes is the rename, and an interrupted run cannot
/// leave a half-written configuration. The candidate takes the original's
/// permissions first, so replacing a file does not change its mode.
///
/// A path that is a symbolic link is resolved first, and the file it points at
/// is the one that is read, written beside and renamed over.
///
/// `make` is given the empty string when the file does not exist.
fn commit(
    herdr: &dyn Herdr,
    path: &Path,
    make: impl FnOnce(&str) -> String,
) -> Result<(), WriteError> {
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

    let candidate_text = make(original.as_deref().unwrap_or_default());

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

/// `original` with `addition` after it.
///
/// The blank line between them is what keeps an appended block off the end of
/// whatever was already there, and the newline before that is what keeps a file
/// with no trailing newline from having the block glued onto its last line —
/// which herdr answers with a parse error and a fall back to its defaults.
fn appended(original: &str, addition: &str) -> String {
    let mut text = original.to_string();
    if !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    }
    text.push('\n');
    text.push_str(addition);
    text
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

/// What an install under the id this plugin had before issue #73 left behind.
///
/// Gathered by `main` and handed in, so `run` has no branch that depends on the
/// environment and the tests need not mutate one they share — the reason
/// `config_path_from` above takes its three values rather than reading them.
#[derive(Debug, Default, Clone)]
pub struct Legacy {
    /// The configuration file under the old id, when one is there.
    pub config_file: Option<PathBuf>,
    /// The directory the plugin reads now, which the same sentence names.
    pub current_config_dir: Option<PathBuf>,
    /// The old socket, when a daemon still answers on it. Unix only: on Windows
    /// the address is a name in the pipe namespace, with no path.
    pub daemon_socket: Option<String>,
}

/// "rewrite 2 bindings to name herdr-voice", "append 1 binding", or both — the
/// question sits under a wall of TOML and has to say what answering it does.
fn ask(decision: &Decision) -> String {
    let mut parts = Vec::new();
    if !decision.superseded.is_empty() {
        parts.push(format!(
            "rewrite {} to name {PLUGIN_ID}",
            plural_bindings(decision.superseded.len())
        ));
    }
    if !decision.to_add.is_empty() {
        parts.push(format!("append {}", plural_bindings(decision.to_add.len())));
    }
    parts.join(" and ")
}

/// What the rename left behind besides the keybindings. Printed whether or not
/// anything was rewritten: a person who repairs the keys and stops there runs on
/// defaults, with a daemon from the old build still holding the old socket.
fn report_legacy(legacy: &Legacy, out: &mut dyn std::io::Write) {
    if let (Some(file), Some(now)) = (&legacy.config_file, &legacy.current_config_dir) {
        let _ = writeln!(
            out,
            "\nyour configuration from before the rename is still at {}, and this \
             plugin now reads {}. Move it with:\n  mkdir -p {} && mv {} {}/",
            file.display(),
            now.display(),
            now.display(),
            file.display(),
            now.display()
        );
    }
    if let Some(socket) = &legacy.daemon_socket {
        let _ = writeln!(
            out,
            "\na daemon from before the rename is still running and holding {socket}. \
             No herdr restart ends it — it is not herdr's child. End it with:\n  \
             kill $(lsof -t {socket})"
        );
    }
}

/// The whole action. `interactive` decides which half runs, and it is a fact
/// about the process rather than a flag: a pane has a terminal, the action herdr
/// starts does not.
pub fn run(
    herdr: &dyn Herdr,
    path: Option<PathBuf>,
    legacy: &Legacy,
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
        report_legacy(legacy, out);
        return 1;
    };

    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => {
            let _ = writeln!(out, "cannot read {}: {e}", path.display());
            report_legacy(legacy, out);
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
            report_legacy(legacy, out);
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
    for (binding, key) in &decision.superseded {
        let _ = writeln!(
            out,
            "superseded: {key} in {} carries this plugin's previous id, {}.{}. \
             The id is now {PLUGIN_ID}, so that key does nothing when it is pressed.",
            path.display(),
            LEGACY_PLUGIN_ID,
            binding.action
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

    if decision.to_add.is_empty() && decision.superseded.is_empty() {
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
        report_legacy(legacy, out);
        return 0;
    }

    let snippet = render(&decision.to_add);
    if !decision.to_add.is_empty() {
        let _ = writeln!(out, "\nthese go into {}:\n\n{snippet}", path.display());
    }
    let _ = write!(
        out,
        "\n{} in the file named above? [y/N] then Enter: ",
        ask(&decision)
    );
    // "then Enter" is not decoration. The answer is read with `read_line`, which
    // returns nothing until a newline arrives, while the terminal echoes the
    // keystroke — so a person who presses `y` alone sees their answer on the
    // screen and the run standing still, and reads that as done. It happened
    // twice to the first person who used this, and both times nothing was
    // written and nothing said why.
    //
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
        report_legacy(legacy, out);
        return 0;
    }

    // One write for both halves, so herdr judges the file once and one rename
    // puts it in place. The rewrite goes first: a block that is about to name
    // the current id must not also be appended.
    let rewriting = !decision.superseded.is_empty();
    // What still names the previous id in the text that was actually written.
    // `decide` finds a superseded block by parsing the file; the rewrite matches
    // one spelling of the value, `command = "<id>.<action>"`. A block whose value
    // is written another way — a literal string, a multi-line one — is found by
    // the first and left by the second, and reporting it as repaired would be a
    // failure that announced success.
    let mut left_as_it_was = Vec::new();
    let outcome = commit(herdr, &path, |original| {
        let mut text = if rewriting {
            rewrite_commands(original)
        } else {
            original.to_string()
        };
        if !snippet.is_empty() {
            text = appended(&text, &snippet);
        }
        left_as_it_was = superseded_keys(&text);
        text
    });

    let code = match outcome {
        Ok(()) => {
            let _ = writeln!(out, "\nin {}:", path.display());
            for (binding, key) in &decision.superseded {
                if left_as_it_was.contains(key) {
                    let _ = writeln!(
                        out,
                        "  {key} was left as it was: its command is not written as \
                         `command = \"{}.{}\"`, which is the one form this edits. \
                         Change the value on that line to \"{}\" by hand.",
                        LEGACY_PLUGIN_ID,
                        binding.action,
                        binding.command()
                    );
                } else {
                    let _ = writeln!(out, "  {} rewritten on {key}", binding.command());
                }
            }
            // A key the loop above did not cover: `decide` reports one key per
            // action and the read-back finds the first block still naming the
            // old id, so a file with two blocks for one action can leave a key
            // neither of them names. Failing and saying nothing about why is the
            // same defect as saying a dead key was repaired.
            for key in &left_as_it_was {
                if !decision.superseded.iter().any(|(_, named)| named == key) {
                    let _ = writeln!(
                        out,
                        "  {key} was left as it was: it names {LEGACY_PLUGIN_ID} in a \
                         form this does not edit, and another key carries the same \
                         action. Change the value on that line by hand."
                    );
                }
            }
            for binding in &decision.to_add {
                let _ = writeln!(out, "  {} added on {}", binding.command(), binding.key);
            }
            let _ = writeln!(
                out,
                "\nthe running herdr does not see this until you run \
                 `herdr server reload-config`, or press prefix+shift+r."
            );
            // A key that is still dead is a failure, whatever else landed.
            u8::from(!left_as_it_was.is_empty())
        }
        Err(e) => {
            let _ = writeln!(out, "\n{e}");
            1
        }
    };
    report_legacy(legacy, out);
    code
}

/// A daemon from before the rename still answers on the old socket, or nothing
/// does. Nothing is sent on the connection; a connection that opens is the whole
/// answer.
///
/// Unix only: on Windows the address is a name in the pipe namespace, with no
/// path for this to look beside and no `lsof` to name a process with.
#[cfg(unix)]
pub fn legacy_daemon_socket(state_dir: Option<PathBuf>) -> Option<String> {
    let socket = crate::transport::legacy_sibling(&state_dir?)?.join(crate::transport::SOCKET_FILE);
    let address = crate::transport::Address::path(socket.to_string_lossy().into_owned());
    crate::transport::connect(&address).ok()?;
    Some(address.display().to_string())
}

/// What the rename of issue #73 left on this machine, read once, outside `run`.
fn legacy() -> Legacy {
    let current_config_dir = crate::config::directory(&crate::config::Vars::from_env());
    let config_file = current_config_dir
        .as_deref()
        .and_then(crate::transport::legacy_sibling)
        .map(|dir| dir.join(crate::config::FILE_NAME))
        .filter(|file| file.is_file());
    #[cfg(unix)]
    let daemon_socket = legacy_daemon_socket(crate::transport::state_directory(
        &crate::transport::Vars::from_env(),
    ));
    #[cfg(not(unix))]
    let daemon_socket = None;
    Legacy {
        config_file,
        current_config_dir,
        daemon_socket,
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
    run(
        &herdr,
        config_path(),
        &legacy(),
        interactive,
        &mut answer,
        &mut out,
    )
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
            rendered.contains("command = \"herdr-voice.ptt\""),
            "{rendered}"
        );
        assert!(rendered.contains("description = "), "{rendered}");
        assert!(
            !rendered.contains("haurylau"),
            "the snippet is what a person pastes into their configuration: {rendered}"
        );
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
            &Legacy::default(),
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
        assert!(
            on_screen.contains("Enter"),
            "the answer is read a line at a time, so the question must say that a \
             keystroke alone is not an answer: {on_screen:?}"
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
            &Legacy::default(),
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
        assert!(!after.contains("herdr-voice.ptt"));
    }

    #[test]
    fn with_no_path_to_resolve_it_says_so_rather_than_guessing() {
        let (code, said) = capture(&FakeHerdr::clean(), None, true, vec!["y"]);
        assert_eq!(code, 1);
        assert!(said.contains("HERDR_CONFIG_PATH"), "{said}");
    }

    fn scratch(name: &str) -> std::path::PathBuf {
        // The process id belongs in the name. Without it two test binaries
        // running at once — `cargo test` builds and runs more than one — share
        // the directory, and the `remove_dir_all` below deletes a fixture the
        // other one is still using. This repository fixed the same defect once
        // already, recorded in 9ca95d5: "wav_path() named its file by pid
        // alone, shared across every test".
        let dir =
            std::env::temp_dir().join(format!("herdr-voice-setup-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("config.toml")
    }

    #[test]
    fn a_clean_append_lands_and_keeps_every_byte_that_was_there() {
        let path = scratch("clean");
        std::fs::write(&path, "[theme]\nname = \"something\"\n").unwrap();
        commit(&FakeHerdr::clean(), &path, |original| {
            appended(original, "[[keys.command]]\nkey = \"ctrl+g\"\n")
        })
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
        commit(&FakeHerdr::clean(), &path, |original| {
            appended(original, "[[keys.command]]\nkey = \"ctrl+g\"\n")
        })
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

    /// The probe is a connection attempt and nothing else: a socket file with
    /// nothing behind it is a daemon that is gone, and saying it is still
    /// running would send a person to kill a process that does not exist.
    #[cfg(unix)]
    #[test]
    fn the_probe_answers_only_while_something_is_listening() {
        let current = scratch("probe").parent().unwrap().join("herdr-voice");
        std::fs::create_dir_all(&current).unwrap();
        let legacy = crate::transport::legacy_sibling(&current).unwrap();
        std::fs::create_dir_all(&legacy).unwrap();
        assert_eq!(legacy_daemon_socket(Some(current.clone())), None);

        let address = crate::transport::Address::path(
            legacy.join("voice.sock").to_string_lossy().into_owned(),
        );
        let listener = crate::transport::listen(&address).unwrap();
        let answered = legacy_daemon_socket(Some(current));
        drop(listener);
        assert_eq!(answered.as_deref(), Some(address.display()));
    }

    #[test]
    fn y_rewrites_the_predecessor_s_blocks_and_says_which_keys_they_were_on() {
        let path = scratch("rewrite-yes");
        std::fs::write(
            &path,
            "# why ctrl+g: alt+g typed ©\n\
             [[keys.command]]\n\
             key = \"ctrl+g\"\n\
             type = \"plugin_action\"\n\
             command = \"haurylau.voice.ptt\"\n",
        )
        .unwrap();
        let mut said = Vec::new();
        let mut answer = || Some("y\n".to_string());
        let code = run(
            &FakeHerdr::clean(),
            Some(path.clone()),
            &Legacy::default(),
            true,
            &mut answer,
            &mut said,
        );
        let said = String::from_utf8(said).unwrap();
        assert_eq!(code, 0, "{said}");
        assert!(said.contains("ctrl+g"), "the key it sits on: {said}");
        assert!(
            said.contains(&path.display().to_string()),
            "the file it sits in: {said}"
        );
        let after = std::fs::read_to_string(&path).unwrap();
        assert!(after.contains("command = \"herdr-voice.ptt\""), "{after}");
        assert!(
            after.contains("# why ctrl+g: alt+g typed ©\n[[keys.command]]"),
            "{after}"
        );
        assert!(!after.contains("haurylau.voice"), "{after}");
    }

    /// `decide` finds a superseded block by parsing the file; the rewrite edits
    /// one spelling of the value. A block that only the first recognises must be
    /// reported as left alone, with the line to change — not as repaired. Saying
    /// a dead key was fixed is worse than saying nothing.
    #[test]
    fn a_value_the_rewrite_cannot_edit_is_reported_as_left_alone_and_the_run_fails() {
        let path = scratch("rewrite-literal");
        let original = "[[keys.command]]\nkey = \"ctrl+g\"\ncommand = '''haurylau.voice.ptt'''\n";
        std::fs::write(&path, original).unwrap();
        let mut said = Vec::new();
        let mut answer = || Some("y\n".to_string());
        let code = run(
            &FakeHerdr::clean(),
            Some(path.clone()),
            &Legacy::default(),
            true,
            &mut answer,
            &mut said,
        );
        let said = String::from_utf8(said).unwrap();
        assert_eq!(code, 1, "a key that is still dead is a failure: {said}");
        assert!(
            said.contains("ctrl+g was left as it was"),
            "it must not claim the key was repaired: {said}"
        );
        assert!(
            said.contains("by hand"),
            "and it must say what to do about it: {said}"
        );
        assert!(!said.contains("rewritten on ctrl+g"), "{said}");
    }

    /// A multi-line string holds somebody's text. `[[keys.command]]` inside one
    /// opens no block, and a `command =` line inside one is not a binding.
    #[test]
    fn nothing_inside_a_multi_line_string_is_read_as_a_binding() {
        let original = "[notes]\ntext = \"\"\"\n\
                        [[keys.command]]\n\
                        command = \"haurylau.voice.ptt\"\n\
                        \"\"\"\n";
        assert_eq!(rewrite_commands(original), original);
    }

    /// A `\"\"\"` in a comment is prose. Counting delimiters without knowing
    /// whether they are delimiters left every block after such a line
    /// unrewritten, and the report then blamed a line that was written exactly
    /// as the rewrite expects.
    #[test]
    fn a_multi_line_delimiter_inside_a_comment_opens_nothing() {
        let original = "# a value can be written \"\"\"like this\"\"\", or \"\"\"\n\
                        [[keys.command]]\n\
                        key = \"ctrl+g\"\n\
                        command = \"haurylau.voice.ptt\"\n";
        let after = rewrite_commands(original);
        assert!(after.contains("command = \"herdr-voice.ptt\""), "{after}");
        assert!(after.starts_with("# a value can be written"), "{after}");
    }

    /// A basic multi-line string is closed by \"\"\" and by nothing else. Treating
    /// a ''' inside one as a delimiter ended the string early and rewrote a line
    /// of somebody's prose — the one outcome worse than leaving a line alone.
    #[test]
    fn the_other_delimiter_inside_a_multi_line_string_closes_nothing() {
        let original = "[notes]\n\
                        text = \"\"\"\n\
                        he said '''\n\
                        [[keys.command]]\n\
                        key = \"ctrl+g\"\n\
                        command = \"haurylau.voice.ptt\"\n\
                        \"\"\"\n";
        assert_eq!(rewrite_commands(original), original);
    }

    /// A basic multi-line string honours escapes, so `\\\"\"\"` is content and
    /// not a closing delimiter. Reading it as one left the string early, edited a
    /// line of somebody's prose, and then blamed the real binding — which the
    /// fixture below is a valid TOML document carrying.
    #[test]
    fn an_escaped_quote_does_not_close_a_multi_line_string() {
        let original = "[[keys.command]]\n\
                        key = \"ctrl+g\"\n\
                        why = \"\"\"\n\
                        he said \\\"\"\" and then\n\
                        command = \"haurylau.voice.dictate\"\n\
                        \"\"\"\n\
                        command = \"haurylau.voice.ptt\"\n";
        assert!(
            toml::from_str::<toml::Value>(original).is_ok(),
            "the fixture has to be a document TOML accepts"
        );
        let after = rewrite_commands(original);
        assert!(
            !after.contains("herdr-voice.dictate"),
            "the line inside the string is somebody's text: {after}"
        );
        assert!(
            after.contains("command = \"herdr-voice.ptt\""),
            "and the binding below it is the one to rewrite: {after}"
        );
    }

    #[test]
    fn a_single_line_string_holding_a_delimiter_opens_nothing() {
        let original = "message = '''he said \"\"\"'''\n\
                        [[keys.command]]\n\
                        key = \"ctrl+g\"\n\
                        command = \"haurylau.voice.ptt\"\n";
        let after = rewrite_commands(original);
        assert!(after.contains("command = \"herdr-voice.ptt\""), "{after}");
    }

    /// `decide` reports one key per action and the read-back finds the first
    /// block still naming the old id. When those are different keys, the one
    /// left behind was named by neither: the run failed and said nothing about
    /// why, which is the silent failure this repository weighs the same as a
    /// wrong transcript.
    #[test]
    fn a_key_left_behind_is_named_even_when_another_key_for_it_was_repaired() {
        let path = scratch("rewrite-two-blocks");
        std::fs::write(
            &path,
            "[[keys.command]]\nkey = \"ctrl+g\"\ncommand = \"haurylau.voice.ptt\"\n\n\
             [[keys.command]]\nkey = \"ctrl+h\"\ncommand = '''haurylau.voice.ptt'''\n",
        )
        .unwrap();
        let mut said = Vec::new();
        let mut answer = || Some("y\n".to_string());
        let code = run(
            &FakeHerdr::clean(),
            Some(path),
            &Legacy::default(),
            true,
            &mut answer,
            &mut said,
        );
        let said = String::from_utf8(said).unwrap();
        assert_eq!(code, 1, "{said}");
        assert!(
            said.contains("ctrl+h was left as it was"),
            "the key nothing repaired must be named: {said}"
        );
    }

    #[test]
    fn the_question_says_which_halves_it_will_do() {
        let mut decision = Decision::default();
        decision
            .superseded
            .push((&BINDINGS[0], "ctrl+g".to_string()));
        assert_eq!(ask(&decision), "rewrite 1 binding to name herdr-voice");
        decision.to_add.push(&BINDINGS[1]);
        assert_eq!(
            ask(&decision),
            "rewrite 1 binding to name herdr-voice and append 1 binding"
        );
    }

    #[test]
    fn anything_but_y_leaves_the_predecessor_s_blocks_where_they_are() {
        let path = scratch("rewrite-no");
        let original = "[[keys.command]]\nkey = \"ctrl+g\"\ncommand = \"haurylau.voice.ptt\"\n";
        std::fs::write(&path, original).unwrap();
        let mut said = Vec::new();
        let mut answer = || Some("n\n".to_string());
        run(
            &FakeHerdr::clean(),
            Some(path.clone()),
            &Legacy::default(),
            true,
            &mut answer,
            &mut said,
        );
        assert_eq!(std::fs::read_to_string(&path).unwrap(), original);
    }

    #[test]
    fn the_report_names_the_old_configuration_file_and_the_one_read_now() {
        let path = scratch("legacy-config");
        std::fs::write(&path, "[theme]\n").unwrap();
        let legacy = Legacy {
            config_file: Some(PathBuf::from("/tmp/c/haurylau.voice/config.toml")),
            current_config_dir: Some(PathBuf::from("/tmp/c/herdr-voice")),
            daemon_socket: None,
        };
        let mut said = Vec::new();
        let mut answer = || Some("n\n".to_string());
        run(
            &FakeHerdr::clean(),
            Some(path),
            &legacy,
            true,
            &mut answer,
            &mut said,
        );
        let said = String::from_utf8(said).unwrap();
        assert!(said.contains("/tmp/c/haurylau.voice/config.toml"), "{said}");
        assert!(said.contains("/tmp/c/herdr-voice"), "{said}");
        assert!(
            said.contains("mv "),
            "it gives the one command that moves it: {said}"
        );
    }

    #[test]
    fn the_report_names_a_daemon_that_still_answers_on_the_old_socket() {
        let path = scratch("legacy-daemon");
        std::fs::write(&path, "[theme]\n").unwrap();
        let legacy = Legacy {
            daemon_socket: Some("/tmp/s/haurylau.voice/voice.sock".to_string()),
            ..Legacy::default()
        };
        let mut said = Vec::new();
        let mut answer = || Some("n\n".to_string());
        run(
            &FakeHerdr::clean(),
            Some(path),
            &legacy,
            true,
            &mut answer,
            &mut said,
        );
        let said = String::from_utf8(said).unwrap();
        assert!(said.contains("/tmp/s/haurylau.voice/voice.sock"), "{said}");
        assert!(said.contains("kill"), "and the one way to end it: {said}");
    }

    #[test]
    fn nothing_is_said_about_leftovers_that_are_not_there() {
        let path = scratch("legacy-none");
        std::fs::write(&path, "[theme]\n").unwrap();
        let mut said = Vec::new();
        let mut answer = || Some("n\n".to_string());
        run(
            &FakeHerdr::clean(),
            Some(path),
            &Legacy::default(),
            true,
            &mut answer,
            &mut said,
        );
        let said = String::from_utf8(said).unwrap();
        assert!(!said.contains("mv "), "{said}");
        assert!(!said.contains("kill"), "{said}");
    }

    #[test]
    fn the_superseded_keys_are_the_ones_found_in_the_order_the_bindings_declare() {
        let text =
            "[[keys.command]]\nkey = \"ctrl+shift+g\"\ncommand = \"haurylau.voice.cancel\"\n\n\
                    [[keys.command]]\nkey = \"ctrl+g\"\ncommand = \"haurylau.voice.ptt\"\n\n\
                    [[keys.command]]\nkey = \"ctrl+t\"\ncommand = \"somebody.else.thing\"\n";
        assert_eq!(superseded_keys(text), vec!["ctrl+g", "ctrl+shift+g"]);
    }

    #[test]
    fn a_configuration_with_no_predecessor_block_has_no_superseded_keys() {
        let text = "[[keys.command]]\nkey = \"ctrl+g\"\ncommand = \"herdr-voice.ptt\"\n";
        assert!(superseded_keys(text).is_empty());
    }

    /// A file the daemon cannot parse is not evidence that a key is dead, and
    /// the daemon has no terminal to report a broken configuration on. `setup`
    /// has one, and does.
    #[test]
    fn a_configuration_that_does_not_parse_has_no_superseded_keys() {
        assert!(superseded_keys("[keys\nbroken =").is_empty());
    }

    #[test]
    fn a_block_naming_the_previous_id_is_this_plugin_s_own_predecessor() {
        let existing =
            inspect("[[keys.command]]\nkey = \"ctrl+g\"\ncommand = \"haurylau.voice.ptt\"\n")
                .unwrap();
        let decision = decide(&existing);
        assert_eq!(
            decision
                .superseded
                .iter()
                .map(|(b, key)| (b.action, key.as_str()))
                .collect::<Vec<_>>(),
            vec![("ptt", "ctrl+g")]
        );
        assert!(
            decision.blocked.is_empty(),
            "its own past is not a stranger holding the key: {:?}",
            decision.blocked
        );
        assert!(
            !decision.to_add.iter().any(|b| b.action == "ptt"),
            "ptt is repaired by the rewrite, not by an append"
        );
    }

    /// A key held by something that is not this plugin, under either id, is
    /// still somebody else's.
    #[test]
    fn a_block_naming_another_plugin_is_still_a_stranger() {
        let existing =
            inspect("[[keys.command]]\nkey = \"ctrl+g\"\ncommand = \"somebody.else.ptt\"\n")
                .unwrap();
        let decision = decide(&existing);
        assert!(decision.superseded.is_empty(), "{:?}", decision.superseded);
        assert_eq!(decision.blocked.len(), 1);
    }

    /// The file this edits is hand-written, and the reason a key was chosen sits
    /// above the block that uses it. The key does not change, so the comment
    /// stays true — as long as the block stays where it is and only the
    /// command's value moves.
    #[test]
    fn the_rewrite_changes_the_command_value_and_nothing_else() {
        let original = "# my herdr configuration\n\
                        \n\
                        [keys]\n\
                        prefix = \"ctrl+b\"\n\
                        \n\
                        # ctrl+g, because alt+v did nothing and alt+g typed ©\n\
                        [[keys.command]]\n\
                        \tkey = \"ctrl+g\"\n\
                        \ttype = \"plugin_action\"\n\
                        \tcommand   =   \"haurylau.voice.ptt\"  # hold to talk\n\
                        \n\
                        [[keys.command]]\n\
                        key = \"prefix+i\"\n\
                        command = \"haurylau.voice.dictate\"\n";
        let after = rewrite_commands(original);
        assert!(
            after.contains("\tcommand   =   \"herdr-voice.ptt\"  # hold to talk"),
            "{after}"
        );
        assert!(
            after.contains("command = \"herdr-voice.dictate\""),
            "{after}"
        );
        assert!(
            after.contains(
                "# ctrl+g, because alt+v did nothing and alt+g typed ©\n[[keys.command]]"
            ),
            "the comment stays directly above the block it explains: {after}"
        );
        assert_eq!(
            after.replace("herdr-voice.", "haurylau.voice."),
            original,
            "nothing but the two command values moved"
        );
    }

    /// A comment is not an assignment. One that mentions the old command says
    /// what it said; rewriting it would edit a person's prose.
    #[test]
    fn a_comment_that_mentions_the_old_command_is_left_alone() {
        let original = "[[keys.command]]\n\
                        # this used to be haurylau.voice.ptt before the rename\n\
                        key = \"ctrl+g\"\n\
                        command = \"haurylau.voice.ptt\"\n";
        let after = rewrite_commands(original);
        assert!(
            after.contains("# this used to be haurylau.voice.ptt before the rename"),
            "{after}"
        );
        assert!(after.contains("command = \"herdr-voice.ptt\""), "{after}");
    }

    /// Outside a [[keys.command]] block nothing is a binding, whatever it looks
    /// like.
    #[test]
    fn a_command_assignment_outside_a_binding_block_is_left_alone() {
        let original = "[some.other.section]\ncommand = \"haurylau.voice.ptt\"\n";
        assert_eq!(rewrite_commands(original), original);
    }

    #[test]
    fn an_action_this_plugin_does_not_have_is_left_alone() {
        let original =
            "[[keys.command]]\nkey = \"ctrl+j\"\ncommand = \"haurylau.voice.whatever\"\n";
        assert_eq!(rewrite_commands(original), original);
    }

    #[test]
    fn commit_writes_what_the_maker_returns_and_not_the_original() {
        let path = scratch("commit-replaces");
        std::fs::write(&path, "[theme]\nname = \"old\"\n").unwrap();
        commit(&FakeHerdr::clean(), &path, |original| {
            assert_eq!(original, "[theme]\nname = \"old\"\n");
            "[theme]\nname = \"new\"\n".to_string()
        })
        .unwrap();
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "[theme]\nname = \"new\"\n"
        );
    }

    /// The whole reason the write path is shared: herdr judges the candidate,
    /// and a candidate it refuses never reaches the real file.
    #[test]
    fn commit_leaves_the_file_alone_when_herdr_refuses_the_candidate() {
        let path = scratch("commit-refused");
        std::fs::write(&path, "[theme]\n").unwrap();
        let herdr = FakeHerdr::answering(vec![
            Check::from_status(0, "config: ok".into()),
            Check::from_status(1, "config: issues found".into()),
        ]);
        let err = commit(&herdr, &path, |_| "[nonsense]\n".to_string()).unwrap_err();
        assert!(
            matches!(err, WriteError::CandidateRejected { .. }),
            "{err:?}"
        );
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "[theme]\n");
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
        commit(&FakeHerdr::clean(), &path, |original| {
            appended(original, "[[keys.command]]\n")
        })
        .unwrap();
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
        commit(&FakeHerdr::clean(), &path, |original| {
            appended(original, "[[keys.command]]\nkey = \"ctrl+g\"\n")
        })
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
        commit(&FakeHerdr::clean(), &path, |original| {
            appended(original, "[[keys.command]]\nkey = \"ctrl+g\"\n")
        })
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
        commit(&FakeHerdr::clean(), &path, |original| {
            appended(original, "[[keys.command]]\n")
        })
        .unwrap();
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "got {mode:o}");
    }

    #[test]
    fn a_file_that_does_not_exist_is_created_with_its_directory() {
        let path = scratch("absent")
            .parent()
            .unwrap()
            .join("deeper/config.toml");
        commit(&FakeHerdr::clean(), &path, |original| {
            appended(original, "[[keys.command]]\n")
        })
        .unwrap();
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
        let err = commit(&herdr, &path, |original| {
            appended(original, "[[keys.command]]\n")
        })
        .unwrap_err();
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
        let err = commit(&herdr, &path, |original| {
            appended(original, "[[keys.command]]\n")
        })
        .unwrap_err();
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
        let err = commit(&FakeHerdr::clean(), &path, |original| {
            appended(original, "[[keys.command]]\n")
        })
        .unwrap_err();
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
                "herdr-voice",
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
                    "herdr-voice",
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
            commands: vec![("ctrl+z".into(), "herdr-voice.ptt".into())],
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
            commands: vec![("ctrl+g".into(), "herdr-voice.ptt".into())],
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
            !after.contains("herdr-voice.ptt"),
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
command = "herdr-voice.ptt"
description = "ours, already here"
"#;

    #[test]
    fn it_reads_every_command_binding_as_a_key_and_a_command() {
        let existing = inspect(SAMPLE).unwrap();
        assert!(existing
            .commands
            .contains(&("ctrl+g".to_string(), "herdr-voice.ptt".to_string())));
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
