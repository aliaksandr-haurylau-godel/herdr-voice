//! Choosing the microphone: the popup `mic --choose` and the action `mic --open`
//! that opens it. See `tasks/103/DESIGN_103.md`, sections 2.2 and 2.5.

use std::io::{BufRead, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::client::{self, ClientError};
use crate::config_edit::{self, Edit, WriteError};
use crate::proto::Reply;
use crate::reload;
use crate::transport::Address;

/// The bound on asking herdr to open the popup, the same as every other call to
/// herdr (`docs/decisions.md`).
const OPEN_BOUND: Duration = Duration::from_secs(10);

/// What came of telling the daemon about the change.
#[derive(Debug, PartialEq, Eq)]
pub enum Reached {
    /// The daemon answered a reload.
    Applied(reload::Applied),
    /// Nothing is listening: the change applies when a daemon starts.
    NoDaemon,
    /// The daemon did not take it, with the reason in words.
    Failed(String),
}

/// Maps what `client::exchange` returned onto what the popup does. The wording of
/// a failure is the client's own, so a person sees the same sentence here as from
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

/// The inputs, numbered, with the one the configuration names marked and a line
/// about the setting when it does not match anything.
pub fn render(names: &[String], configured: &str) -> String {
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

#[derive(Debug, PartialEq, Eq)]
pub enum Answer {
    /// Esc, or nothing: leave the file as it is.
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

/// The question, flushed: standard output is line buffered and this has no newline,
/// so without the flush it stays unseen while the process waits for the answer.
pub fn prompt(out: &mut dyn Write) {
    let _ = write!(
        out,
        "Type the number of the input, then Enter. Esc then Enter, or an empty line, leaves it as it is: "
    );
    let _ = out.flush();
}

/// The popup, over its inputs so a test needs no device and no terminal.
///
/// `save` writes `[audio] input`; `tell` asks the running daemon to take it. The
/// daemon is told only after the write succeeded.
pub fn choose(
    names: &[String],
    configured: &str,
    input: &mut dyn BufRead,
    out: &mut dyn Write,
    save: &mut dyn FnMut(&str) -> Result<PathBuf, WriteError>,
    tell: &mut dyn FnMut() -> Reached,
) -> u8 {
    let _ = write!(out, "{}", render(names, configured));
    if names.is_empty() {
        return 1;
    }
    prompt(out);
    let mut line = String::new();
    match input.read_line(&mut line) {
        Ok(0) | Err(_) => {
            let _ = writeln!(
                out,
                "\nnothing was read from the terminal; run `herdr-voice mic --choose` again"
            );
            return 1;
        }
        Ok(_) => {}
    }
    let picked;
    let name = match parse_answer(&line, names.len()) {
        Answer::Leave => {
            let _ = writeln!(out, "Nothing was changed.");
            return 0;
        }
        Answer::Invalid(text) => {
            let _ = writeln!(
                out,
                "{text:?} is not one of the numbers above. Open this again and type a number \
                 between 1 and {}",
                names.len()
            );
            return 1;
        }
        Answer::Pick(at) => {
            picked = at;
            names[at].as_str()
        }
    };

    let path = match save(name) {
        Ok(path) => path,
        Err(why) => {
            let _ = writeln!(out, "{why}");
            if why.needs_hand_edit() {
                let _ = writeln!(
                    out,
                    "Add this under [audio] in your configuration file by hand:"
                );
                let _ = writeln!(out, "  input = {}", config_edit::quote(name));
            }
            return 1;
        }
    };
    let _ = writeln!(out, "[audio] input is now {name:?} in {}.", path.display());
    // A take selects by name and uses the first input that has it.
    if let Some(first) = names.iter().position(|n| n == name) {
        if first + 1 != picked + 1 {
            let _ = writeln!(
                out,
                "Two inputs are called {name:?}: the plugin uses the first of them, input {}.",
                first + 1
            );
        }
    }
    match tell() {
        Reached::Applied(done) => {
            if done.applied.iter().any(|s| s == "audio") {
                let _ = writeln!(out, "The daemon applied it: the next take records from it.");
            } else {
                let _ = writeln!(out, "The daemon already uses it; nothing to apply.");
            }
            if !done.restart.is_empty() {
                let _ = writeln!(
                    out,
                    "Other changes in the file need a restart of herdr to apply: {}.",
                    done.restart.join(", ")
                );
            }
            0
        }
        Reached::NoDaemon => {
            let _ = writeln!(
                out,
                "No dictation daemon is running, so nothing was told; the change applies when it starts."
            );
            0
        }
        Reached::Failed(why) => {
            let _ = writeln!(
                out,
                "The file was changed, but the daemon did not take it: {why}. Restart herdr, \
                 or check `herdr plugin log list --plugin herdr-voice`."
            );
            1
        }
    }
}

/// Writes `[audio] input = "<name>"` into the configuration in `directory`.
pub fn save_input(directory: Option<&Path>, name: &str) -> Result<PathBuf, WriteError> {
    config_edit::write_keys(
        directory,
        &[Edit {
            table: "audio",
            key: "input",
            value: config_edit::quote(name),
        }],
    )
}

/// Asks the daemon at `address` to read the configuration again.
pub fn tell_daemon(address: &Address) -> Reached {
    reached(client::exchange(address, "reload", None, Vec::new()))
}

/// What `herdr-voice mic` was asked to do.
#[derive(Debug, PartialEq, Eq)]
pub enum Mode {
    /// `--open`: ask herdr to open the popup.
    Open,
    /// `--choose`: be the popup.
    Choose,
    /// Neither: print the list.
    List,
}

pub fn mode(args: &[String]) -> Mode {
    if args.iter().any(|a| a == "--open") {
        Mode::Open
    } else if args.iter().any(|a| a == "--choose") {
        Mode::Choose
    } else {
        Mode::List
    }
}

/// A line to print before the list when the configuration cannot be taken at its
/// word: `config::load` returns every default for a file that does not parse or
/// cannot be read, and the list would then call the input unset while the file may
/// set one.
pub fn config_note(source: &crate::config::Source) -> Option<String> {
    match source {
        crate::config::Source::Invalid { path, why } => Some(format!(
            "{} does not parse ({why}): the settings shown are the defaults, and saving a \
             choice is refused until the file is fixed.\n",
            path.display()
        )),
        crate::config::Source::Defaults(Some(path))
            if std::fs::read_to_string(path)
                .is_err_and(|e| e.kind() != std::io::ErrorKind::NotFound) =>
        {
            Some(format!(
                "{} cannot be read: the settings shown are the defaults, and saving a \
                 choice is refused until it can be.\n",
                path.display()
            ))
        }
        _ => None,
    }
}

/// `herdr-voice mic`, and `--choose`. Returns the process's exit code.
pub fn run(choosing: bool) -> u8 {
    run_with(choosing, crate::capture::cpal_source::input_names, pause)
}

/// `run` with the two things a test replaces: how the inputs are listed, and the
/// wait for Enter. The popup waits on every path, a failure to list the inputs
/// included, so that its last message is on screen when it closes.
pub fn run_with(
    choosing: bool,
    names: impl FnOnce() -> Result<Vec<String>, String>,
    pause: impl FnOnce(),
) -> u8 {
    let code = run_inner(choosing, names);
    if choosing {
        pause();
    }
    code
}

fn run_inner(choosing: bool, names: impl FnOnce() -> Result<Vec<String>, String>) -> u8 {
    let names = match names() {
        Ok(names) => names,
        Err(why) => {
            eprintln!(
                "{why}. Check that an input is connected and that this program may use the microphone"
            );
            return 1;
        }
    };
    let vars = crate::config::Vars::from_env();
    let directory = crate::config::directory(&vars);
    let loaded = crate::config::load(directory.as_deref());
    let configured = loaded.config.audio.input.clone();
    if let Some(note) = config_note(&loaded.source) {
        println!("{note}");
    }
    if !choosing {
        print!("{}", render(&names, &configured));
        return 0;
    }

    let mut out = std::io::stdout();
    let mut save = |name: &str| save_input(directory.as_deref(), name);
    let mut tell = || match crate::transport::address(&crate::transport::Vars::from_env()) {
        Ok(address) => tell_daemon(&address),
        Err(e) => Reached::Failed(e.to_string()),
    };
    // The lock on standard input is released before `pause` reads from it again: the
    // lock is not re-entrant, and a second one taken on the same thread waits for the
    // first for ever, which in a popup is a pane that never closes.
    let stdin = std::io::stdin();
    let mut input = stdin.lock();
    choose(
        &names,
        &configured,
        &mut input,
        &mut out,
        &mut save,
        &mut tell,
    )
}

/// Waits for Enter when there is a person at a terminal, so a result printed just
/// before the process exits is not gone with the pane. Whether herdr closes a
/// popup the moment its command exits was not established
/// (`tasks/103/DESIGN_103.md`, decision 5).
fn pause() {
    if std::io::stdin().is_terminal() {
        print!("\nPress Enter to close.");
        let _ = std::io::stdout().flush();
        let mut ignored = String::new();
        let _ = std::io::stdin().read_line(&mut ignored);
    }
}

/// The command that asks herdr to open this plugin's `mic` pane.
pub fn open_command(herdr: &str, plugin: &str) -> Vec<String> {
    [
        herdr,
        "plugin",
        "pane",
        "open",
        "--plugin",
        plugin,
        "--entrypoint",
        "mic",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

/// `herdr-voice mic --open`: what the manifest's `mic` action runs.
pub fn open() -> u8 {
    let herdr = crate::delivery::herdr_binary();
    let plugin = std::env::var("HERDR_PLUGIN_ID")
        .unwrap_or_else(|_| crate::transport::PLUGIN_ID.to_string());
    match open_with(&herdr, &plugin, OPEN_BOUND) {
        Ok(()) => 0,
        Err(why) => {
            eprintln!("{why}");
            1
        }
    }
}

/// Asks `herdr` to open the pane, waiting at most `bound`. The `Err` is the
/// message for the person.
pub fn open_with(herdr: &str, plugin: &str, bound: Duration) -> Result<(), String> {
    let argv = open_command(herdr, plugin);
    let mut command = std::process::Command::new(&argv[0]);
    command.args(&argv[1..]);
    match crate::outward::run(&mut command, bound) {
        Ok(output) if output.status.success() => Ok(()),
        Ok(output) => Err(format!(
            "herdr could not open the microphone popup: {}. Check \
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::ClientError;
    use crate::proto::Reply;
    use std::io::Cursor;

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    /// Runs `choose` with the typed `answer`, a writer that stores `Ok(path)` or
    /// fails with `save_error`, and a daemon that answers `reached`. Returns the
    /// exit code, what was printed, and the names that were saved.
    fn run_choose(
        list: &[&str],
        configured: &str,
        answer: &str,
        save_error: Option<WriteError>,
        reached: Reached,
    ) -> (u8, String, Vec<String>) {
        let list = names(list);
        let mut input = Cursor::new(answer.as_bytes().to_vec());
        let mut out: Vec<u8> = Vec::new();
        let saved = std::cell::RefCell::new(Vec::new());
        let mut save_error = save_error;
        let mut save = |name: &str| {
            saved.borrow_mut().push(name.to_string());
            match save_error.take() {
                Some(error) => Err(error),
                None => Ok(PathBuf::from("config.toml")),
            }
        };
        let mut told = Some(reached);
        let mut tell = || told.take().unwrap_or(Reached::NoDaemon);
        let code = choose(
            &list, configured, &mut input, &mut out, &mut save, &mut tell,
        );
        (code, String::from_utf8(out).unwrap(), saved.into_inner())
    }

    fn applied(applied: &[&str], restart: &[&str]) -> Reached {
        Reached::Applied(crate::reload::Applied {
            applied: applied.iter().map(|s| s.to_string()).collect(),
            restart: restart.iter().map(|s| s.to_string()).collect(),
        })
    }

    #[test]
    fn the_list_numbers_every_input_and_marks_the_configured_one() {
        let text = render(&names(&["Built-in", "Headset"]), "Headset");
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
        let text = render(&names(&["Built-in"]), "Old headset");
        assert!(text.contains("[audio] input"), "{text}");
        assert!(text.contains("\"Old headset\""), "{text}");
        assert!(text.contains("matches none"), "{text}");
        assert!(!text.contains("(current)"), "{text}");
    }

    #[test]
    fn an_unset_input_says_the_default_is_used() {
        let text = render(&names(&["Built-in"]), "");
        assert!(text.contains("not set"), "{text}");
        assert!(text.contains("default input"), "{text}");
    }

    #[test]
    fn two_inputs_with_one_name_are_both_listed_and_the_second_says_the_first_is_used() {
        let text = render(&names(&["USB Mic", "Built-in", "USB Mic"]), "USB Mic");
        assert!(text.contains("3. USB Mic"), "{text}");
        let marked: Vec<&str> = text.lines().filter(|l| l.contains("(current)")).collect();
        assert_eq!(
            marked.len(),
            1,
            "only the first is the one selected: {text}"
        );
        assert!(marked[0].contains("1. USB Mic"), "{text}");
        let third = text.lines().find(|l| l.contains("3. USB Mic")).unwrap();
        assert!(third.contains("same name as 1"), "{text}");
        assert!(third.contains("uses the first"), "{text}");
    }

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
    fn the_prompt_is_flushed_before_anyone_waits_for_the_answer() {
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
        prompt(&mut watch);
        assert!(!watch.written.is_empty());
        assert_eq!(
            watch.flushed_at,
            watch.written.len(),
            "the whole prompt must be on screen"
        );
        let text = String::from_utf8(watch.written).unwrap();
        assert!(
            text.contains("Enter"),
            "a keystroke alone is not an answer: {text}"
        );
    }

    #[test]
    fn choosing_a_number_saves_that_inputs_name_and_says_the_next_take_uses_it() {
        let (code, said, saved) = run_choose(
            &["Built-in", "Headset"],
            "Built-in",
            "2\n",
            None,
            applied(&["audio"], &[]),
        );
        assert_eq!(code, 0, "{said}");
        assert_eq!(
            saved,
            vec!["Headset".to_string()],
            "the name, never the number"
        );
        assert!(said.contains("\"Headset\""), "{said}");
        assert!(said.contains("next take"), "{said}");
    }

    #[test]
    fn esc_leaves_the_file_alone_and_says_so() {
        for typed in ["\u{1b}\n", "\n", "\u{1b}[B\n"] {
            let (code, said, saved) = run_choose(&["A", "B"], "A", typed, None, Reached::NoDaemon);
            assert_eq!(code, 0, "{typed:?}: {said}");
            assert!(saved.is_empty(), "{typed:?}");
            assert!(said.contains("Nothing was changed"), "{said}");
        }
    }

    #[test]
    fn an_answer_outside_the_list_is_refused_and_names_the_range() {
        let (code, said, saved) = run_choose(&["A", "B"], "A", "7\n", None, Reached::NoDaemon);
        assert_eq!(code, 1);
        assert!(saved.is_empty());
        assert!(
            said.contains("\"7\"") && said.contains("between 1 and 2"),
            "{said}"
        );
    }

    #[test]
    fn no_input_devices_says_so_and_what_to_do_and_changes_nothing() {
        let (code, said, saved) = run_choose(&[], "", "1\n", None, Reached::NoDaemon);
        assert_eq!(code, 1);
        assert!(saved.is_empty());
        assert!(said.contains("No input devices"), "{said}");
        assert!(said.contains("Connect a microphone"), "{said}");
    }

    #[test]
    fn an_empty_terminal_is_not_an_answer() {
        let (code, said, saved) = run_choose(&["A"], "", "", None, Reached::NoDaemon);
        assert_eq!(code, 1, "{said}");
        assert!(saved.is_empty());
        assert!(said.contains("nothing was read"), "{said}");
    }

    #[test]
    fn a_daemon_that_already_uses_the_input_has_nothing_to_apply() {
        let (code, said, _) = run_choose(&["A", "B"], "A", "2\n", None, applied(&[], &[]));
        assert_eq!(code, 0, "{said}");
        assert!(said.contains("already uses it"), "{said}");
    }

    #[test]
    fn other_changed_sections_are_named_as_needing_a_restart() {
        let (code, said, _) =
            run_choose(&["A", "B"], "A", "2\n", None, applied(&["audio"], &["stt"]));
        assert_eq!(code, 0, "{said}");
        assert!(said.contains("stt"), "{said}");
        assert!(said.contains("restart of herdr"), "{said}");
    }

    #[test]
    fn no_daemon_means_the_change_applies_when_it_starts() {
        let (code, said, saved) = run_choose(&["A", "B"], "A", "2\n", None, Reached::NoDaemon);
        assert_eq!(code, 0, "{said}");
        assert_eq!(saved, vec!["B".to_string()], "the file is still written");
        assert!(said.contains("No dictation daemon is running"), "{said}");
        assert!(said.contains("applies when it starts"), "{said}");
    }

    #[test]
    fn a_daemon_that_did_not_take_it_fails_and_says_what_to_check() {
        let (code, said, _) = run_choose(
            &["A", "B"],
            "A",
            "2\n",
            None,
            Reached::Failed("the daemon did not answer within 2 seconds".to_string()),
        );
        assert_eq!(code, 1, "{said}");
        assert!(said.contains("did not answer within 2 seconds"), "{said}");
        assert!(
            said.contains("herdr plugin log list --plugin herdr-voice"),
            "{said}"
        );
    }

    #[test]
    fn a_write_that_fails_prints_the_error_and_the_line_to_add_by_hand() {
        let (code, said, _) = run_choose(
            &["A", "Mic \"B\""],
            "A",
            "2\n",
            Some(WriteError::Io {
                path: "config.toml".to_string(),
                why: "permission denied".to_string(),
            }),
            Reached::NoDaemon,
        );
        assert_eq!(code, 1, "{said}");
        assert!(said.contains("permission denied"), "{said}");
        assert!(said.contains("Add this under [audio]"), "{said}");
        assert!(
            said.contains("input = \"Mic \\\"B\\\"\""),
            "the line must be valid TOML: {said}"
        );
        assert!(
            !said.contains("daemon"),
            "the daemon is not told of a change that was not written: {said}"
        );
    }

    #[test]
    fn every_result_of_the_exchange_maps_to_what_the_popup_does() {
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
        for error in [
            ClientError::Timeout(std::time::Duration::from_secs(2)),
            ClientError::Transport("broken".to_string()),
            ClientError::Protocol("odd".to_string()),
        ] {
            assert!(matches!(reached(Err(error)), Reached::Failed(_)));
        }
    }

    #[test]
    fn the_action_opens_this_plugins_mic_pane_through_herdr() {
        assert_eq!(
            open_command("herdr", "herdr-voice"),
            vec![
                "herdr",
                "plugin",
                "pane",
                "open",
                "--plugin",
                "herdr-voice",
                "--entrypoint",
                "mic"
            ]
        );
    }

    #[test]
    fn a_refusal_does_not_tell_the_person_to_add_the_line_the_editor_just_refused() {
        let (code, said, _) = run_choose(
            &["A", "B"],
            "A",
            "2\n",
            Some(WriteError::Refused {
                path: "config.toml".to_string(),
                edit: "[audio] input = \"B\"".to_string(),
                why: "duplicate key".to_string(),
            }),
            Reached::NoDaemon,
        );
        assert_eq!(code, 1, "{said}");
        assert!(said.contains("nothing was written"), "{said}");
        assert!(!said.contains("by hand"), "{said}");
    }

    #[test]
    fn choosing_the_second_of_two_inputs_with_one_name_says_the_first_is_used() {
        let (code, said, saved) = run_choose(
            &["USB Mic", "Built-in", "USB Mic"],
            "Built-in",
            "3\n",
            None,
            applied(&["audio"], &[]),
        );
        assert_eq!(code, 0, "{said}");
        assert_eq!(saved, vec!["USB Mic".to_string()]);
        let after = said
            .split("[audio] input is now")
            .nth(1)
            .expect("the confirmation");
        assert!(after.contains("input 1"), "{after}");
        assert!(after.contains("uses the first"), "{after}");
    }

    #[test]
    fn a_broken_configuration_is_said_before_the_list_calls_the_input_unset() {
        let invalid = crate::config::Source::Invalid {
            path: PathBuf::from("config.toml"),
            why: "expected a table".to_string(),
        };
        let note = config_note(&invalid).expect("a note");
        assert!(
            note.contains("config.toml") && note.contains("does not parse"),
            "{note}"
        );
        assert!(note.contains("defaults"), "{note}");
        assert_eq!(
            config_note(&crate::config::Source::Defaults(None)),
            None,
            "an absent file is a valid state and needs no note"
        );
    }

    #[test]
    fn a_failure_to_list_the_inputs_still_waits_for_enter_so_the_message_can_be_read() {
        let paused = std::cell::Cell::new(0);
        let code = run_with(
            true,
            || Err("cannot list input devices: denied".to_string()),
            || paused.set(paused.get() + 1),
        );
        assert_eq!(code, 1);
        assert_eq!(paused.get(), 1, "the popup must wait once before it closes");
        let paused_plain = std::cell::Cell::new(0);
        let code = run_with(
            false,
            || Err("cannot list input devices: denied".to_string()),
            || paused_plain.set(paused_plain.get() + 1),
        );
        assert_eq!(code, 1);
        assert_eq!(paused_plain.get(), 0, "plain `mic` is not a popup");
    }

    #[test]
    fn a_timeout_and_a_transport_failure_are_told_in_the_clients_words() {
        let Reached::Failed(said) = reached(Err(ClientError::Timeout(
            std::time::Duration::from_secs(10),
        ))) else {
            panic!("a timeout is a failure");
        };
        assert!(said.contains("did not answer within 10 seconds"), "{said}");
        assert_eq!(
            reached(Err(ClientError::Transport("x".to_string()))),
            Reached::Failed("x".to_string())
        );
    }

    #[test]
    fn an_empty_name_is_never_marked_as_the_configured_one() {
        let text = render(&names(&[""]), "");
        assert!(!text.contains("(current)"), "{text}");
    }

    #[test]
    fn the_question_is_asked_after_the_list() {
        let (_, said, _) = run_choose(&["A", "B"], "A", "\n", None, Reached::NoDaemon);
        assert!(said.contains("Type the number of the input"), "{said}");
        assert!(
            said.find("2. B").unwrap() < said.find("Type the number").unwrap(),
            "{said}"
        );
    }

    #[test]
    fn several_sections_that_need_a_restart_are_listed_with_commas() {
        let (_, said, _) = run_choose(&["A", "B"], "A", "2\n", None, applied(&[], &["stt", "ui"]));
        assert!(
            said.contains("restart of herdr to apply: stt, ui."),
            "{said}"
        );
    }

    #[test]
    fn the_confirmation_names_the_file_it_wrote() {
        let list = names(&["A", "B"]);
        let mut input = Cursor::new(b"2\n".to_vec());
        let mut out: Vec<u8> = Vec::new();
        let mut save = |_: &str| Ok(PathBuf::from("/some/dir/config.toml"));
        let mut tell = || Reached::NoDaemon;
        choose(&list, "A", &mut input, &mut out, &mut save, &mut tell);
        let said = String::from_utf8(out).unwrap();
        assert!(said.contains("in /some/dir/config.toml."), "{said}");
    }

    #[test]
    fn the_mode_follows_the_flags() {
        let args = |list: &[&str]| -> Vec<String> { list.iter().map(|s| s.to_string()).collect() };
        assert_eq!(mode(&args(&["mic", "--open"])), Mode::Open);
        assert_eq!(mode(&args(&["mic", "--choose"])), Mode::Choose);
        assert_eq!(mode(&args(&["mic"])), Mode::List);
        assert_eq!(mode(&args(&["mic", "--open", "--choose"])), Mode::Open);
    }

    #[test]
    fn plain_mic_prints_the_list_and_does_not_wait_for_enter() {
        let code = run_with(
            false,
            || Ok(Vec::new()),
            || panic!("plain mic is not a popup"),
        );
        assert_eq!(code, 0);
    }

    #[test]
    fn saving_writes_the_audio_input_and_nothing_else_into_the_directory_it_is_given() {
        let dir = std::env::temp_dir().join(format!("herdr-voice-mic-save-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = save_input(Some(&dir), "Headset").expect("written");
        assert_eq!(
            std::fs::read_to_string(path).unwrap(),
            "[audio]\ninput = \"Headset\"\n"
        );
    }

    #[test]
    fn telling_the_daemon_sends_a_reload_and_reads_the_answer() {
        let address = crate::transport::tests_support::probe_address("mic-tell");
        let listener = crate::transport::listen(&address).expect("listen");
        let server = std::thread::spawn(move || {
            let mut reader = std::io::BufReader::new(listener.accept().expect("accept"));
            let request = crate::proto::Request::read_from(&mut reader).expect("read");
            Reply::Ok("applied: audio".to_string())
                .write_to(reader.get_mut())
                .expect("write");
            request
        });
        assert_eq!(tell_daemon(&address), applied(&["audio"], &[]));
        assert_eq!(server.join().unwrap().command, "reload");
    }

    #[cfg(unix)]
    mod opening {
        use super::super::*;

        fn script(tag: &str, body: &str) -> String {
            let dir =
                std::env::temp_dir().join(format!("herdr-voice-open-{tag}-{}", std::process::id()));
            std::fs::create_dir_all(&dir).unwrap();
            let path = dir.join("herdr");
            crate::script_fixture::write_executable(&path, body);
            path.display().to_string()
        }

        #[test]
        fn herdr_is_asked_to_open_the_mic_pane_of_this_plugin() {
            let record = std::env::temp_dir().join(format!(
                "herdr-voice-open-argv-record-{}",
                std::process::id()
            ));
            let herdr = script(
                "args",
                &format!("#!/bin/sh\necho \"$@\" > {}\n", record.display()),
            );
            assert_eq!(
                open_with(&herdr, "herdr-voice", Duration::from_secs(10)),
                Ok(())
            );
            assert_eq!(
                std::fs::read_to_string(&record).unwrap().trim(),
                "plugin pane open --plugin herdr-voice --entrypoint mic"
            );
        }

        #[test]
        fn a_refusal_from_herdr_is_reported_with_what_it_said_and_where_to_look() {
            let herdr = script("refuse", "#!/bin/sh\necho boom >&2\nexit 1\n");
            let said = open_with(&herdr, "herdr-voice", Duration::from_secs(10)).unwrap_err();
            assert!(said.contains("boom"), "{said}");
            assert!(
                said.contains("herdr plugin log list --plugin herdr-voice"),
                "{said}"
            );
        }

        #[test]
        fn what_herdr_said_is_cut_to_a_length_a_person_can_read() {
            let herdr = script("long", "#!/bin/sh\nyes x | head -c 5000 >&2\nexit 1\n");
            let said = open_with(&herdr, "herdr-voice", Duration::from_secs(10)).unwrap_err();
            assert!(said.len() < 600, "{} bytes", said.len());
        }

        #[test]
        fn a_herdr_that_does_not_answer_is_given_up_on_and_the_bound_is_named() {
            let herdr = script("slow", "#!/bin/sh\nsleep 5\n");
            let said = open_with(&herdr, "herdr-voice", Duration::from_secs(1)).unwrap_err();
            assert!(said.contains("did not answer within 1 seconds"), "{said}");
        }

        #[test]
        fn a_herdr_that_is_not_there_says_how_to_find_it() {
            let said = open_with(
                "/definitely/not/a/real/herdr",
                "herdr-voice",
                Duration::from_secs(1),
            )
            .unwrap_err();
            assert!(said.contains("cannot run"), "{said}");
            assert!(said.contains("PATH"), "{said}");
        }

        #[test]
        fn an_unreadable_configuration_is_noted_and_an_absent_one_is_not() {
            use std::os::unix::fs::PermissionsExt;
            let dir = std::env::temp_dir().join(format!("herdr-voice-note-{}", std::process::id()));
            std::fs::create_dir_all(&dir).unwrap();
            let file = dir.join("config.toml");
            std::fs::write(&file, "[audio]\n").unwrap();
            std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o000)).unwrap();
            let note = config_note(&crate::config::Source::Defaults(Some(file.clone())));
            std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600)).unwrap();
            assert!(note.expect("a note").contains("cannot be read"));
            assert_eq!(
                config_note(&crate::config::Source::Defaults(Some(
                    dir.join("absent.toml")
                ))),
                None
            );
        }
    }
}
