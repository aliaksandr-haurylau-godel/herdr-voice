//! What a popup of this plugin needs: the terminal, an answer read from it, the
//! daemon, and the seam (`World`) every flow talks to so that it can be tested with
//! no file, no daemon, no sound device, no download and no server.
//!
//! See `tasks/104/DESIGN_104.md`, section 2.2.

use std::io::{BufRead, IsTerminal, Write};
use std::path::PathBuf;
use std::time::Duration;

use crate::client::{self, ClientError};
use crate::config::{Loaded, Source, FILE_NAME};
use crate::config_edit::{self, Edit, WriteError};
use crate::proto::Reply;
use crate::reload;
use crate::rewrite_models::{self, ListFailure};
use crate::stt::candle::store::{self, Glance};
use crate::stt::catalogue::Entry;
use crate::stt::fetch;
use crate::transport::Address;

/// The bound on asking herdr to open the popup, the same as every other call to
/// herdr (`docs/decisions.md`).
pub const OPEN_BOUND: Duration = Duration::from_secs(10);

/// What every question that can be left says, so the words are the same everywhere.
pub const LEAVE_HINT: &str = "Esc then Enter, or an empty line, leaves it as it is";

/// What came of telling the daemon about a change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reached {
    /// The daemon answered a reload.
    Applied(reload::Applied),
    /// Nothing is listening: the change applies when a daemon starts.
    NoDaemon,
    /// The daemon did not take it, with the reason in words.
    Failed(String),
}

/// Maps what `client::exchange` returned onto what a popup does. The wording of a
/// failure is the client's own, so a person sees the same sentence here as from
/// `herdr-voice cancel`.
pub fn reached(result: Result<Reply, ClientError>) -> Reached {
    match result {
        Err(ClientError::NoDaemon(_)) => Reached::NoDaemon,
        Ok(Reply::Ok(text)) => match reload::parse(&text) {
            Some(applied) => Reached::Applied(applied),
            None => Reached::Failed(format!(
                "the daemon answered {text:?}, which is not a reload"
            )),
        },
        Ok(Reply::Error(text)) => Reached::Failed(text),
        Err(other) => Reached::Failed(client::outcome(Err(other)).message.unwrap_or_default()),
    }
}

/// Asks the daemon at `address` to read the configuration again.
pub fn tell_daemon(address: &Address) -> Reached {
    reached(client::exchange(address, "reload", None, Vec::new()))
}

#[derive(Debug, PartialEq, Eq)]
pub enum Answer {
    /// Esc, or nothing: leave what is being asked.
    Leave,
    /// The zero-based position in the list.
    Pick(usize),
    Invalid(String),
}

/// A line is read, so Esc and the arrow keys arrive as text that begins with the
/// Esc character, and only after Enter. Anything that begins with it leaves.
pub fn parse_answer(line: &str, count: usize) -> Answer {
    let text = line.trim();
    if text.is_empty() || text.starts_with('\u{1b}') {
        return Answer::Leave;
    }
    match text.parse::<usize>() {
        Ok(n) if (1..=count).contains(&n) => Answer::Pick(n - 1),
        _ => Answer::Invalid(text.to_string()),
    }
}

/// The sentence for an answer that is not a number in the list.
pub fn not_in_list(text: &str, count: usize) -> String {
    format!("{text:?} is not one of the numbers above; type a number between 1 and {count}")
}

/// A question, flushed: standard output is line buffered and a question has no
/// newline, so without the flush it stays unseen while the process waits.
pub fn prompt(out: &mut dyn Write, question: &str) {
    let _ = write!(out, "{question}");
    let _ = out.flush();
}

/// The terminal a popup talks to.
pub struct Io<'a> {
    pub input: &'a mut dyn BufRead,
    pub out: &'a mut dyn Write,
    /// Set by any flow that ends in something the person has to act on. The popup's
    /// exit code is 1 when it is set.
    pub failed: bool,
    /// The end of the input was reached, and said.
    ended: bool,
}

impl<'a> Io<'a> {
    pub fn new(input: &'a mut dyn BufRead, out: &'a mut dyn Write) -> Io<'a> {
        Io {
            input,
            out,
            failed: false,
            ended: false,
        }
    }

    pub fn say(&mut self, text: &str) {
        let _ = writeln!(self.out, "{text}");
    }

    pub fn fail(&mut self) {
        self.failed = true;
    }

    /// Whether the end of the input was reached, so a menu can stop instead of
    /// drawing itself once more.
    pub fn has_ended(&self) -> bool {
        self.ended
    }

    /// Asks `question` and returns the line read. `None` means there is nothing more
    /// to read; that is said once, the popup is marked failed, and every later
    /// question answers `None` at once, so every level of a menu unwinds.
    pub fn ask(&mut self, question: &str) -> Option<String> {
        if self.ended {
            return None;
        }
        prompt(self.out, question);
        let mut line = String::new();
        match self.input.read_line(&mut line) {
            Ok(0) | Err(_) => {
                self.ended = true;
                self.failed = true;
                self.say(
                    "\nnothing was read from the terminal; open the settings again from herdr",
                );
                None
            }
            Ok(_) => Some(line),
        }
    }
}

/// The configuration file as it stands, read again each time it is asked for.
pub struct Snapshot {
    /// The file's text; empty when there is none.
    pub text: String,
    /// What `config::load` makes of it.
    pub loaded: Loaded,
}

impl Snapshot {
    /// The file as a table, when it parses; which keys are set is read from it.
    pub fn table(&self) -> Option<toml::Table> {
        // A file that parses but does not load as the configuration is shown as
        // defaults, as `config_note` says: the daemon cannot use what it holds.
        if matches!(self.loaded.source, Source::Invalid { .. }) {
            return None;
        }
        toml::from_str(&self.text).ok()
    }
}

/// Everything a flow asks of the outside. `settings::Real` is the world of a real
/// run; tests use `tests_support::FakeWorld`.
pub trait World {
    fn snapshot(&self) -> Snapshot;
    fn input_names(&self) -> Result<Vec<String>, String>;
    fn save(&mut self, edits: &[Edit]) -> Result<PathBuf, WriteError>;
    fn tell(&mut self) -> Reached;
    fn models_dir(&self) -> Option<PathBuf>;
    fn model_state(&self, entry: &Entry) -> Glance;
    /// Downloads and verifies a model. Shows its own progress.
    fn install(&mut self, entry: &Entry) -> Result<(), String>;
    fn rewrite_models(&mut self, url: &str, token: &str) -> Result<Vec<String>, ListFailure>;
}

/// What a saved change does, in words, and whether the person has to act on it.
pub fn change_note(section: &str, reached: &Reached) -> (String, bool) {
    match reached {
        Reached::NoDaemon => (
            "No dictation daemon is running, so nothing was told; the change applies when it starts."
                .to_string(),
            false,
        ),
        Reached::Failed(why) => (
            format!(
                "The file was changed, but the daemon did not take it: {why}. Restart herdr, or \
                 check `herdr plugin log list --plugin herdr-voice`."
            ),
            true,
        ),
        Reached::Applied(done) => {
            let applied = done.applied.iter().any(|s| s == section);
            let needs = done.restart.iter().any(|s| s == section);
            let mut text = if section == "audio" {
                if applied {
                    "The daemon applied it: the next take records from it.".to_string()
                } else {
                    "The daemon already uses it; nothing to apply.".to_string()
                }
            } else if needs {
                format!("This needs a restart of herdr to apply: [{section}].")
            } else {
                "The daemon already runs with this value; nothing to apply.".to_string()
            };
            let others: Vec<&str> = done
                .restart
                .iter()
                .filter(|s| s.as_str() != section)
                .map(String::as_str)
                .collect();
            if !others.is_empty() {
                text.push_str(&format!(
                    "\nOther changes in the file also need a restart of herdr: {}.",
                    others.join(", ")
                ));
            }
            (text, false)
        }
    }
}

/// Writes one key, tells the daemon, and says what happened and what to do next.
/// `value` is the TOML text written; `shown` is how the value is named to the person.
/// Returns whether the key was written. A failure sets `io.failed`.
pub fn save_and_tell(
    world: &mut dyn World,
    io: &mut Io,
    section: &str,
    key: &str,
    value: String,
    shown: &str,
) -> bool {
    let edits = [Edit {
        table: section,
        key,
        value: value.clone(),
    }];
    match world.save(&edits) {
        Ok(path) => {
            io.say(&format!(
                "[{section}] {key} is now {shown} in {}.",
                path.display()
            ));
            let (note, failed) = change_note(section, &world.tell());
            io.say(&note);
            if failed {
                io.fail();
            }
            true
        }
        Err(why) => {
            io.say(&why.to_string());
            if why.needs_hand_edit() {
                io.say(&format!(
                    "Add this under [{section}] in your configuration file by hand:"
                ));
                io.say(&format!("  {key} = {value}"));
            }
            io.fail();
            false
        }
    }
}

/// A line to print before anything else when the configuration cannot be taken at
/// its word: `config::load` returns every default for a file that does not parse or
/// cannot be read, and a list would then show defaults for settings the file may set.
pub fn config_note(source: &Source) -> Option<String> {
    match source {
        Source::Invalid { path, why } => Some(format!(
            "{} does not load ({why}): the settings shown are the defaults, and saving a \
             change is refused until the file is fixed.\n",
            path.display()
        )),
        Source::Defaults(Some(path))
            if std::fs::read_to_string(path)
                .is_err_and(|e| e.kind() != std::io::ErrorKind::NotFound) =>
        {
            Some(format!(
                "{} cannot be read: the settings shown are the defaults, and saving a \
                 change is refused until it can be.\n",
                path.display()
            ))
        }
        _ => None,
    }
}

/// Waits for Enter when there is a person at a terminal, so a result printed just
/// before the process exits is not gone with the pane. Whether herdr closes a popup
/// the moment its command exits was not established (`tasks/104/DESIGN_104.md`,
/// section 5).
pub fn pause() {
    if std::io::stdin().is_terminal() {
        print!("\nPress Enter to close.");
        let _ = std::io::stdout().flush();
        let mut ignored = String::new();
        let _ = std::io::stdin().read_line(&mut ignored);
    }
}

/// The command that asks herdr to open the pane `entrypoint` of `plugin`.
pub fn open_command(herdr: &str, plugin: &str, entrypoint: &str) -> Vec<String> {
    [
        herdr,
        "plugin",
        "pane",
        "open",
        "--plugin",
        plugin,
        "--entrypoint",
        entrypoint,
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

/// Asks `herdr` to open the pane, waiting at most `bound`. The `Err` is the message
/// for the person.
pub fn open_with(
    herdr: &str,
    plugin: &str,
    entrypoint: &str,
    bound: Duration,
) -> Result<(), String> {
    let argv = open_command(herdr, plugin, entrypoint);
    let mut command = std::process::Command::new(&argv[0]);
    command.args(&argv[1..]);
    match crate::outward::run(&mut command, bound) {
        Ok(output) if output.status.success() => Ok(()),
        Ok(output) => Err(format!(
            "herdr could not open the settings popup: {}. Check \
             `herdr plugin log list --plugin herdr-voice`",
            crate::outward::shorten(String::from_utf8_lossy(&output.stderr).trim(), 300)
        )),
        Err(crate::outward::RunError::TimedOut) => Err(format!(
            "herdr did not answer within {} seconds when asked to open the popup. Check \
             that herdr is running, then try again",
            bound.as_secs()
        )),
        Err(crate::outward::RunError::Start(e)) => Err(format!(
            "cannot run {herdr:?}: {e}. Check that herdr is installed and on the PATH"
        )),
    }
}

/// A progress line that overwrites itself, so a 3 GB download is one line.
#[derive(Default)]
pub struct Line {
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

/// The world of a real run: the real file, daemon, devices, models directory, download
/// and server.
pub struct Real {
    directory: Option<PathBuf>,
    models: Option<PathBuf>,
}

impl Real {
    pub fn from_env() -> Real {
        let vars = crate::config::Vars::from_env();
        Real {
            directory: crate::config::directory(&vars),
            models: crate::transport::state_directory(&crate::transport::Vars::from_env())
                .map(|state| state.join("models")),
        }
    }
}

impl World for Real {
    fn snapshot(&self) -> Snapshot {
        let text = self
            .directory
            .as_ref()
            .and_then(|dir| std::fs::read_to_string(dir.join(FILE_NAME)).ok())
            .unwrap_or_default();
        Snapshot {
            text,
            loaded: crate::config::load(self.directory.as_deref()),
        }
    }

    fn input_names(&self) -> Result<Vec<String>, String> {
        crate::capture::cpal_source::input_names()
    }

    fn save(&mut self, edits: &[config_edit::Edit]) -> Result<PathBuf, WriteError> {
        config_edit::write_keys(self.directory.as_deref(), edits)
    }

    fn tell(&mut self) -> Reached {
        match crate::transport::address(&crate::transport::Vars::from_env()) {
            Ok(address) => tell_daemon(&address),
            Err(e) => Reached::Failed(e.to_string()),
        }
    }

    fn models_dir(&self) -> Option<PathBuf> {
        self.models.clone()
    }

    fn model_state(&self, entry: &Entry) -> Glance {
        match &self.models {
            Some(models) => store::glance(models, entry.identifier, Some(entry)),
            None => Glance::Absent,
        }
    }

    fn install(&mut self, entry: &Entry) -> Result<(), String> {
        let Some(models) = &self.models else {
            return Err(
                "cannot tell where models live: neither HERDR_PLUGIN_STATE_DIR nor a home \
                 directory is set, so there is nowhere to put one. Set HERDR_PLUGIN_STATE_DIR \
                 and try again"
                    .to_string(),
            );
        };
        let mut line = Line::default();
        fetch::model(entry.identifier, models, &mut line).map_err(|e| e.to_string())?;
        // Verify what was just downloaded with the real check, not the glance the
        // listing uses: this is the moment a bad download must be caught.
        store::locate(models, entry.identifier, Some(entry)).map_err(|e| e.to_string())?;
        Ok(())
    }

    fn rewrite_models(&mut self, url: &str, token: &str) -> Result<Vec<String>, ListFailure> {
        rewrite_models::fetch(url, token, rewrite_models::LIST_BOUND)
    }
}

/// A `World` for tests: a real configuration directory under the temporary directory,
/// the real writer, and everything else recorded and answered from fields.
#[cfg(test)]
pub mod tests_support {
    use super::*;

    pub struct FakeWorld {
        pub dir: PathBuf,
        pub names: Result<Vec<String>, String>,
        pub reached: Reached,
        pub told: usize,
        /// `[table] key = value` for every edit saved, in order.
        pub saved: Vec<String>,
        pub save_error: Option<WriteError>,
        pub models: Option<PathBuf>,
        pub states: Vec<(String, Glance)>,
        pub installs: Vec<String>,
        pub install_error: Option<String>,
        pub list: Result<Vec<String>, ListFailure>,
        pub list_calls: Vec<(String, String)>,
    }

    impl FakeWorld {
        /// A world whose configuration file holds `text` (no file when it is empty).
        pub fn new(tag: &str, text: &str) -> FakeWorld {
            let dir = std::env::temp_dir()
                .join(format!("herdr-voice-world-{tag}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            if !text.is_empty() {
                std::fs::write(dir.join("config.toml"), text).unwrap();
            }
            FakeWorld {
                dir,
                names: Ok(Vec::new()),
                reached: Reached::NoDaemon,
                told: 0,
                saved: Vec::new(),
                save_error: None,
                models: Some(PathBuf::from("/models")),
                states: Vec::new(),
                installs: Vec::new(),
                install_error: None,
                list: Err(ListFailure::Empty),
                list_calls: Vec::new(),
            }
        }

        pub fn file(&self) -> String {
            std::fs::read_to_string(self.dir.join("config.toml")).unwrap_or_default()
        }
    }

    impl World for FakeWorld {
        fn snapshot(&self) -> Snapshot {
            Snapshot {
                text: self.file(),
                loaded: crate::config::load(Some(&self.dir)),
            }
        }

        fn input_names(&self) -> Result<Vec<String>, String> {
            self.names.clone()
        }

        fn save(&mut self, edits: &[Edit]) -> Result<PathBuf, WriteError> {
            for edit in edits {
                self.saved
                    .push(format!("[{}] {} = {}", edit.table, edit.key, edit.value));
            }
            if let Some(error) = self.save_error.take() {
                return Err(error);
            }
            crate::config_edit::write_keys(Some(&self.dir), edits)
        }

        fn tell(&mut self) -> Reached {
            self.told += 1;
            self.reached.clone()
        }

        fn models_dir(&self) -> Option<PathBuf> {
            self.models.clone()
        }

        fn model_state(&self, entry: &Entry) -> Glance {
            self.states
                .iter()
                .find(|(id, _)| id == entry.identifier)
                .map(|(_, state)| *state)
                .unwrap_or(Glance::Absent)
        }

        fn install(&mut self, entry: &Entry) -> Result<(), String> {
            self.installs.push(entry.identifier.to_string());
            if let Some(why) = self.install_error.take() {
                return Err(why);
            }
            self.states.retain(|(id, _)| id != entry.identifier);
            self.states
                .push((entry.identifier.to_string(), Glance::Whole));
            Ok(())
        }

        fn rewrite_models(&mut self, url: &str, token: &str) -> Result<Vec<String>, ListFailure> {
            self.list_calls.push((url.to_string(), token.to_string()));
            self.list.clone()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn answers_are_read_the_way_the_prompt_says() {
        assert_eq!(parse_answer("2\n", 3), Answer::Pick(1));
        assert_eq!(parse_answer(" 1 \r\n", 3), Answer::Pick(0));
        assert_eq!(parse_answer("\n", 3), Answer::Leave);
        assert_eq!(parse_answer("\u{1b}\n", 3), Answer::Leave);
        // An arrow key reaches a line read as an Esc-led sequence.
        assert_eq!(parse_answer("\u{1b}[A\n", 3), Answer::Leave);
        for bad in ["0\n", "4\n", "-1\n", "x\n", "2x\n"] {
            assert!(
                matches!(parse_answer(bad, 3), Answer::Invalid(_)),
                "{bad:?}"
            );
        }
    }

    #[test]
    fn an_answer_outside_the_list_names_the_range() {
        let said = not_in_list("7", 2);
        assert!(
            said.contains("\"7\"") && said.contains("between 1 and 2"),
            "{said}"
        );
    }

    #[test]
    fn a_question_is_flushed_before_anyone_waits_for_the_answer() {
        #[derive(Default)]
        struct Watch {
            written: Vec<u8>,
            flushed_at: usize,
        }
        impl Write for Watch {
            fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
                self.written.extend_from_slice(bytes);
                Ok(bytes.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                self.flushed_at = self.written.len();
                Ok(())
            }
        }
        let mut watch = Watch::default();
        prompt(&mut watch, "Type a number, then Enter: ");
        assert!(!watch.written.is_empty());
        assert_eq!(
            watch.flushed_at,
            watch.written.len(),
            "the whole question must be on screen"
        );
    }

    #[test]
    fn asking_prints_the_question_and_returns_the_line() {
        let mut input = Cursor::new(b"2\n".to_vec());
        let mut out: Vec<u8> = Vec::new();
        let mut io = Io::new(&mut input, &mut out);
        assert_eq!(io.ask("Which? ").as_deref(), Some("2\n"));
        assert!(!io.failed);
        assert_eq!(String::from_utf8(out).unwrap(), "Which? ");
    }

    #[test]
    fn the_end_of_the_input_is_said_once_and_every_later_question_answers_at_once() {
        let mut input = Cursor::new(Vec::new());
        let mut out: Vec<u8> = Vec::new();
        let mut io = Io::new(&mut input, &mut out);
        assert_eq!(io.ask("First? "), None);
        assert!(io.failed);
        assert_eq!(io.ask("Second? "), None);
        let said = String::from_utf8(out).unwrap();
        assert_eq!(
            said.matches("nothing was read from the terminal").count(),
            1,
            "{said}"
        );
        assert!(
            !said.contains("Second?"),
            "a question after the end is not printed: {said}"
        );
    }

    #[test]
    fn every_result_of_the_exchange_maps_to_what_the_popup_does() {
        let applied = |applied: &[&str], restart: &[&str]| {
            Reached::Applied(reload::Applied {
                applied: applied.iter().map(|s| s.to_string()).collect(),
                restart: restart.iter().map(|s| s.to_string()).collect(),
            })
        };
        assert_eq!(
            reached(Ok(Reply::Ok("applied: audio".to_string()))),
            applied(&["audio"], &[])
        );
        assert_eq!(
            reached(Ok(Reply::Ok(
                "applied: nothing; needs a restart: stt".to_string()
            ))),
            applied(&[], &["stt"])
        );
        assert!(matches!(
            reached(Ok(Reply::Ok("pong".to_string()))),
            Reached::Failed(_)
        ));
        assert_eq!(
            reached(Ok(Reply::Error("nothing was changed".to_string()))),
            Reached::Failed("nothing was changed".to_string())
        );
        assert_eq!(
            reached(Err(ClientError::NoDaemon("x".to_string()))),
            Reached::NoDaemon
        );
        let Reached::Failed(said) = reached(Err(ClientError::Timeout(Duration::from_secs(10))))
        else {
            panic!("a timeout is a failure");
        };
        assert!(said.contains("did not answer within 10 seconds"), "{said}");
        assert_eq!(
            reached(Err(ClientError::Transport("x".to_string()))),
            Reached::Failed("x".to_string())
        );
        assert!(matches!(
            reached(Err(ClientError::Protocol("odd".to_string()))),
            Reached::Failed(_)
        ));
    }

    #[test]
    fn telling_the_daemon_sends_a_reload_and_reads_the_answer() {
        let address = crate::transport::tests_support::probe_address("popup-tell");
        let listener = crate::transport::listen(&address).expect("listen");
        let server = std::thread::spawn(move || {
            let mut reader = std::io::BufReader::new(listener.accept().expect("accept"));
            let request = crate::proto::Request::read_from(&mut reader).expect("read");
            Reply::Ok("applied: audio".to_string())
                .write_to(reader.get_mut())
                .expect("write");
            request
        });
        let Reached::Applied(done) = tell_daemon(&address) else {
            panic!("the daemon applied it");
        };
        assert_eq!(done.applied, vec!["audio".to_string()]);
        assert_eq!(server.join().unwrap().command, "reload");
    }

    fn applied(applied: &[&str], restart: &[&str]) -> Reached {
        Reached::Applied(reload::Applied {
            applied: applied.iter().map(|s| s.to_string()).collect(),
            restart: restart.iter().map(|s| s.to_string()).collect(),
        })
    }

    #[test]
    fn a_change_to_audio_says_the_next_take_uses_it_when_the_daemon_applied_it() {
        let (said, failed) = change_note("audio", &applied(&["audio"], &[]));
        assert!(said.contains("next take"), "{said}");
        assert!(!failed);
        let (said, _) = change_note("audio", &applied(&[], &[]));
        assert!(said.contains("already uses it"), "{said}");
    }

    #[test]
    fn a_change_to_any_other_section_says_it_needs_a_restart_of_herdr() {
        let (said, failed) = change_note("stt", &applied(&[], &["stt"]));
        assert!(said.contains("needs a restart of herdr"), "{said}");
        assert!(said.contains("[stt]"), "{said}");
        assert!(!failed);
        let (said, _) = change_note("ui", &applied(&[], &[]));
        assert!(said.contains("already runs with this value"), "{said}");
    }

    #[test]
    fn other_sections_that_also_need_a_restart_are_named() {
        let (said, _) = change_note("audio", &applied(&["audio"], &["stt", "ui"]));
        assert!(said.contains("restart of herdr: stt, ui."), "{said}");
        let (said, _) = change_note("stt", &applied(&[], &["stt", "ui"]));
        assert!(said.contains("restart of herdr: ui."), "{said}");
        assert!(!said.contains("restart of herdr: stt"), "{said}");
    }

    #[test]
    fn no_daemon_means_the_change_applies_when_it_starts() {
        let (said, failed) = change_note("audio", &Reached::NoDaemon);
        assert!(said.contains("No dictation daemon is running"), "{said}");
        assert!(said.contains("applies when it starts"), "{said}");
        assert!(!failed);
    }

    #[test]
    fn a_daemon_that_did_not_take_it_is_a_failure_that_says_what_to_check() {
        let (said, failed) = change_note(
            "audio",
            &Reached::Failed("the daemon did not answer within 10 seconds".to_string()),
        );
        assert!(failed);
        assert!(said.contains("did not answer within 10 seconds"), "{said}");
        assert!(
            said.contains("herdr plugin log list --plugin herdr-voice"),
            "{said}"
        );
    }

    #[test]
    fn the_action_opens_the_named_pane_of_this_plugin_through_herdr() {
        assert_eq!(
            open_command("herdr", "herdr-voice", "settings"),
            vec![
                "herdr",
                "plugin",
                "pane",
                "open",
                "--plugin",
                "herdr-voice",
                "--entrypoint",
                "settings"
            ]
        );
    }

    #[test]
    fn a_file_that_does_not_parse_is_noted_and_the_snapshot_has_no_table() {
        let world = tests_support::FakeWorld::new("popup-invalid", "[audio\ninput = ");
        let snapshot = world.snapshot();
        assert!(snapshot.table().is_none());
        let note = config_note(&snapshot.loaded.source).expect("a note");
        assert!(
            note.contains("does not load") && note.contains("defaults"),
            "{note}"
        );
        let fine = tests_support::FakeWorld::new("popup-fine", "[audio]\ninput = \"X\"\n");
        assert!(fine.snapshot().table().is_some());
        assert_eq!(config_note(&fine.snapshot().loaded.source), None);
        let absent = tests_support::FakeWorld::new("popup-absent", "");
        assert_eq!(config_note(&absent.snapshot().loaded.source), None);
    }

    #[cfg(unix)]
    mod opening {
        use super::super::*;

        fn script(tag: &str, body: &str) -> String {
            let dir = std::env::temp_dir().join(format!(
                "herdr-voice-popup-open-{tag}-{}",
                std::process::id()
            ));
            std::fs::create_dir_all(&dir).unwrap();
            let path = dir.join("herdr");
            crate::script_fixture::write_executable(&path, body);
            path.display().to_string()
        }

        #[test]
        fn herdr_is_asked_to_open_the_pane_of_this_plugin() {
            let record = std::env::temp_dir().join(format!(
                "herdr-voice-popup-argv-record-{}",
                std::process::id()
            ));
            let herdr = script(
                "args",
                &format!("#!/bin/sh\necho \"$@\" > {}\n", record.display()),
            );
            assert_eq!(
                open_with(&herdr, "herdr-voice", "settings", Duration::from_secs(10)),
                Ok(())
            );
            assert_eq!(
                std::fs::read_to_string(&record).unwrap().trim(),
                "plugin pane open --plugin herdr-voice --entrypoint settings"
            );
        }

        #[test]
        fn a_refusal_from_herdr_is_reported_with_what_it_said_and_where_to_look() {
            let herdr = script("refuse", "#!/bin/sh\necho boom >&2\nexit 1\n");
            let said =
                open_with(&herdr, "herdr-voice", "settings", Duration::from_secs(10)).unwrap_err();
            assert!(said.contains("boom"), "{said}");
            assert!(
                said.contains("herdr plugin log list --plugin herdr-voice"),
                "{said}"
            );
        }

        #[test]
        fn what_herdr_said_is_cut_to_a_length_a_person_can_read() {
            let herdr = script("long", "#!/bin/sh\nyes x | head -c 5000 >&2\nexit 1\n");
            let said =
                open_with(&herdr, "herdr-voice", "settings", Duration::from_secs(10)).unwrap_err();
            assert!(said.len() < 600, "{} bytes", said.len());
        }

        #[test]
        fn a_herdr_that_does_not_answer_is_given_up_on_and_the_bound_is_named() {
            let herdr = script("slow", "#!/bin/sh\nsleep 5\n");
            let said =
                open_with(&herdr, "herdr-voice", "settings", Duration::from_secs(1)).unwrap_err();
            assert!(said.contains("did not answer within 1 seconds"), "{said}");
        }

        #[test]
        fn a_herdr_that_is_not_there_says_how_to_find_it() {
            let said = open_with(
                "/definitely/not/a/real/herdr",
                "herdr-voice",
                "settings",
                Duration::from_secs(1),
            )
            .unwrap_err();
            assert!(said.contains("cannot run"), "{said}");
            assert!(said.contains("PATH"), "{said}");
        }

        #[test]
        fn an_unreadable_configuration_is_noted_and_an_absent_one_is_not() {
            use std::os::unix::fs::PermissionsExt;
            let dir =
                std::env::temp_dir().join(format!("herdr-voice-popup-note-{}", std::process::id()));
            std::fs::create_dir_all(&dir).unwrap();
            let file = dir.join("config.toml");
            std::fs::write(&file, "[audio]\n").unwrap();
            std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o000)).unwrap();
            let note = config_note(&Source::Defaults(Some(file.clone())));
            std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600)).unwrap();
            assert!(note.expect("a note").contains("cannot be read"));
            assert_eq!(
                config_note(&Source::Defaults(Some(dir.join("absent.toml")))),
                None
            );
        }
    }

    #[test]
    fn saving_writes_the_key_tells_the_daemon_and_says_what_it_does() {
        let mut world = tests_support::FakeWorld::new("save-ok", "[ui]\ntoasts = true\n");
        world.reached = applied(&[], &["ui"]);
        let mut input = Cursor::new(Vec::new());
        let mut out: Vec<u8> = Vec::new();
        let mut io = Io::new(&mut input, &mut out);
        let saved = save_and_tell(
            &mut world,
            &mut io,
            "ui",
            "toasts",
            "false".to_string(),
            "false",
        );
        assert!(saved && !io.failed);
        assert_eq!(world.told, 1);
        assert!(world.file().contains("toasts = false"), "{}", world.file());
        let said = String::from_utf8(out).unwrap();
        assert!(said.contains("[ui] toasts is now false"), "{said}");
        assert!(said.contains("needs a restart of herdr"), "{said}");
    }

    #[test]
    fn a_refused_write_does_not_tell_the_daemon_and_does_not_ask_for_a_hand_edit_that_would_fail_too(
    ) {
        let mut world = tests_support::FakeWorld::new("save-refused", "[ui]\nblink_ms = 250\n");
        let mut input = Cursor::new(Vec::new());
        let mut out: Vec<u8> = Vec::new();
        let mut io = Io::new(&mut input, &mut out);
        let saved = save_and_tell(
            &mut world,
            &mut io,
            "ui",
            "blink_ms",
            crate::config_edit::quote("fast"),
            "\"fast\"",
        );
        assert!(!saved && io.failed);
        assert_eq!(world.told, 0);
        assert_eq!(world.file(), "[ui]\nblink_ms = 250\n");
        let said = String::from_utf8(out).unwrap();
        assert!(said.contains("nothing was written"), "{said}");
        assert!(!said.contains("by hand"), "{said}");
    }

    #[test]
    fn a_write_that_could_not_happen_prints_the_line_to_add_by_hand() {
        let mut world = tests_support::FakeWorld::new("save-io", "");
        world.save_error = Some(WriteError::Io {
            path: "config.toml".to_string(),
            why: "permission denied".to_string(),
        });
        let mut input = Cursor::new(Vec::new());
        let mut out: Vec<u8> = Vec::new();
        let mut io = Io::new(&mut input, &mut out);
        let saved = save_and_tell(
            &mut world,
            &mut io,
            "audio",
            "input",
            crate::config_edit::quote("Mic \"B\""),
            "\"Mic \\\"B\\\"\"",
        );
        assert!(!saved && io.failed);
        let said = String::from_utf8(out).unwrap();
        assert!(said.contains("permission denied"), "{said}");
        assert!(said.contains("Add this under [audio]"), "{said}");
        assert!(
            said.contains("input = \"Mic \\\"B\\\"\""),
            "valid TOML: {said}"
        );
        assert_eq!(world.told, 0);
    }

    #[test]
    fn a_file_that_parses_but_does_not_load_is_shown_as_defaults_like_the_note_says() {
        let world = tests_support::FakeWorld::new("popup-wrong-type", "[ui]\nblink_ms = \"x\"\n");
        let snapshot = world.snapshot();
        assert!(
            snapshot.table().is_none(),
            "the list must not show values the daemon cannot use"
        );
        let note = config_note(&snapshot.loaded.source).expect("a note");
        assert!(note.contains("defaults"), "{note}");
    }

    #[test]
    fn a_section_is_only_reported_applied_or_to_restart_when_it_is_the_one_changed() {
        let (said, _) = change_note("audio", &applied(&["ui"], &[]));
        assert!(said.contains("already uses it"), "{said}");
        assert!(!said.contains("next take"), "{said}");
        let (said, _) = change_note("ui", &applied(&[], &["stt"]));
        assert!(
            !said.contains("needs a restart of herdr to apply: [ui]"),
            "{said}"
        );
        assert!(said.contains("already runs with this value"), "{said}");
        assert!(
            said.contains("Other changes in the file also need a restart of herdr: stt"),
            "{said}"
        );
    }

    #[test]
    fn a_saved_change_the_daemon_did_not_take_fails_the_popup_though_the_key_was_written() {
        let mut world = tests_support::FakeWorld::new("save-daemon-fails", "[ui]\ntoasts = true\n");
        world.reached = Reached::Failed("the daemon did not answer".to_string());
        let mut input = Cursor::new(Vec::new());
        let mut out: Vec<u8> = Vec::new();
        let mut io = Io::new(&mut input, &mut out);
        let saved = save_and_tell(
            &mut world,
            &mut io,
            "ui",
            "toasts",
            "false".to_string(),
            "false",
        );
        assert!(saved, "the key was written");
        assert!(io.failed, "and the person has to act on the daemon");
    }

    #[test]
    fn the_confirmation_uses_the_shown_form_and_the_hand_edit_line_uses_the_value() {
        let mut world = tests_support::FakeWorld::new("save-shown", "");
        let mut input = Cursor::new(Vec::new());
        let mut out: Vec<u8> = Vec::new();
        let mut io = Io::new(&mut input, &mut out);
        save_and_tell(
            &mut world,
            &mut io,
            "ui",
            "toasts",
            "false".to_string(),
            "SHOWN-FORM",
        );
        assert!(String::from_utf8(out)
            .unwrap()
            .contains("is now SHOWN-FORM in"));

        let mut world = tests_support::FakeWorld::new("save-value", "");
        world.save_error = Some(WriteError::NoDirectory);
        let mut input = Cursor::new(Vec::new());
        let mut out: Vec<u8> = Vec::new();
        let mut io = Io::new(&mut input, &mut out);
        save_and_tell(
            &mut world,
            &mut io,
            "ui",
            "toasts",
            "VALUE-TEXT".to_string(),
            "SHOWN-FORM",
        );
        let said = String::from_utf8(out).unwrap();
        assert!(said.contains("  toasts = VALUE-TEXT"), "{said}");
        assert!(!said.contains("SHOWN-FORM"), "{said}");
    }

    #[test]
    fn the_bound_on_opening_the_popup_and_the_words_that_leave_a_question_are_what_they_say() {
        assert_eq!(OPEN_BOUND, Duration::from_secs(10));
        assert_eq!(
            LEAVE_HINT,
            "Esc then Enter, or an empty line, leaves it as it is"
        );
    }
}
