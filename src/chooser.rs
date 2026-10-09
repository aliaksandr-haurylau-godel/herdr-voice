//! Choosing a speech model, and downloading the one chosen.
//!
//! Two entry points because two questions are asked: `herdr-voice model` says
//! what exists, and `--choose` spends the gigabytes. The same flow,
//! `choose_speech_model`, is the speech-model entry of the settings popup. The
//! configuration edit is `config_edit`'s: it changes one key and leaves everything
//! else in the file. See `tasks/15/DESIGN_15.md`, section 5, and
//! `tasks/104/DESIGN_104.md`, section 2.4.

use std::path::Path;

use crate::popup::{
    config_note, not_in_list, parse_answer, save_and_tell, Answer, Io, Real, World, LEAVE_HINT,
};
use crate::stt::candle::store::Glance;
use crate::stt::catalogue::{self, Entry};

/// A size the way a person reads it, not the way a computer stores it.
fn human(bytes: u64) -> String {
    const MB: f64 = 1_000_000.0;
    const GB: f64 = 1_000_000_000.0;
    let b = bytes as f64;
    if b >= GB {
        format!("{:.2} GB", b / GB)
    } else {
        format!("{:.0} MB", b / MB)
    }
}

/// The catalogue, with each model's size and whether it is already there.
///
/// Presence is judged by `store::glance`, not `store::locate`: listing six
/// models must not hash six multi-gigabyte files. A person asking what is
/// installed is asking a question about names and sizes, and waiting a minute
/// for the answer would be absurd. The full check happens where it matters —
/// before a model is loaded, and again right after a download.
pub fn list(
    catalogue: &'static [Entry],
    configured: &str,
    state: &dyn Fn(&Entry) -> Glance,
    models: Option<&Path>,
) -> String {
    let mut out = String::from("Speech models this plugin can install:\n\n");
    for (i, entry) in catalogue.iter().enumerate() {
        let size = human(catalogue::weights(entry).bytes);
        let present = match state(entry) {
            Glance::Whole => "installed",
            Glance::Absent => "not installed",
            Glance::WrongSize => "installed, but the wrong size",
        };
        let marker = if entry.identifier == configured {
            "  (current)"
        } else {
            ""
        };
        out.push_str(&format!(
            "  {}. {:<16} {:>8}  {} mel bins  — {present}{marker}\n",
            i + 1,
            entry.identifier,
            size,
            entry.mel_bins
        ));
    }
    if let Some(models) = models {
        out.push_str("\nThey live in ");
        out.push_str(&models.join("candle").display().to_string());
        out.push('\n');
    }
    out
}

/// What to do next for an engine that does not use a model from this catalogue. The
/// catalogue installs weights for the built-in engine, `candle`, and a person who
/// uses another engine decides whether to move to it; the popup never does.
const MOVE_TO_CANDLE: &str = "To use a model from this list, set [stt] engine to \"candle\" \
     in the settings popup, then choose the model again";

/// The speech-model flow: list the catalogue, take a number, and by the engine in
/// the configuration either install and write `[stt] model`, or say why not and
/// what to change. Writes nothing for an engine that would not use the model.
pub fn choose_speech_model(world: &mut dyn World, io: &mut Io, catalogue: &'static [Entry]) {
    if catalogue.is_empty() {
        io.say(
            "No speech models are on offer in this build, which is a defect in the build: \
             report it, and set [stt] model by hand in the meantime.",
        );
        io.fail();
        return;
    }
    let snapshot = world.snapshot();
    if let Some(note) = config_note(&snapshot.loaded.source) {
        io.say(&note);
    }
    let stt = snapshot.loaded.config.stt.clone();
    let models = world.models_dir();
    let listing = list(
        catalogue,
        &stt.model,
        &|entry| world.model_state(entry),
        models.as_deref(),
    );
    io.say(&listing);

    let Some(line) = io.ask(&format!(
        "Type the number of the model, then Enter. {LEAVE_HINT}: "
    )) else {
        return;
    };
    let entry = match parse_answer(&line, catalogue.len()) {
        Answer::Leave => {
            io.say("Nothing was changed.");
            return;
        }
        Answer::Invalid(text) => {
            io.say(&not_in_list(&text, catalogue.len()));
            return;
        }
        Answer::Pick(at) => &catalogue[at],
    };
    let id = entry.identifier;

    match stt.engine.as_str() {
        "candle" => install_and_write(world, io, entry, &stt.model),
        "command" if stt.command.is_empty() => io.say(&format!(
            "[stt] engine is \"command\" but [stt] command is empty, so no speech engine is set \
             up yet, and nothing was changed. `herdr-voice doctor` says what is missing. \
             {MOVE_TO_CANDLE}."
        )),
        "command" if crate::stt::wants_our_model(&stt.command) => {
            let file = crate::stt::model::file_name(id);
            let place = match &models {
                Some(dir) => dir.join(&file).display().to_string(),
                None => format!("{file} in the models directory"),
            };
            io.say(&format!(
                "[stt] engine is \"command\" and its command uses {{model}}, so a take looks for \
                 the file {place}. The models in this list are weights for the built-in engine \
                 (\"candle\"), so installing {id} would not change what a take uses. Nothing was \
                 downloaded or written.\n\
                 To use {id}: {MOVE_TO_CANDLE}; or, to keep the command, put {file} into the \
                 models directory, then set [stt] model to \"{id}\" in the configuration file."
            ));
        }
        "command" => io.say(&format!(
            "[stt] engine is \"command\" and its command does not use {{model}}: it brings its \
             own model, so [stt] model is not used and nothing was changed. {MOVE_TO_CANDLE}."
        )),
        "http" => io.say(&format!(
            "[stt] engine is \"http\": the server's model is named by [stt] http_model, \
             [stt] model is not used, and nothing was changed. {MOVE_TO_CANDLE}."
        )),
        other => io.say(&format!(
            "[stt] engine is {other:?}, which is not one of {}; nothing was changed. \
             {MOVE_TO_CANDLE}.",
            crate::stt::ENGINES.join(", ")
        )),
    }
}

/// The `candle` branch: download what is missing, then write `[stt] model` when it
/// is not the configured one.
fn install_and_write(world: &mut dyn World, io: &mut Io, entry: &'static Entry, configured: &str) {
    let id = entry.identifier;
    let state = world.model_state(entry);
    let here = configured == id;
    if state == Glance::Whole && here {
        io.say(&format!("{id} is installed and already configured."));
        return;
    }
    if state == Glance::Whole {
        io.say(&format!("{id} is already installed."));
    } else {
        io.say(&format!(
            "Installing {id} ({})",
            human(catalogue::weights(entry).bytes)
        ));
        if let Err(why) = world.install(entry) {
            io.say(&why);
            io.fail();
            return;
        }
        io.say(&format!("{id} is installed."));
        if here {
            // The daemon built its speech engine at start, when this model was not
            // there, and nothing in the file changed for a reload to report.
            io.say(
                "The daemon loaded its speech engine before it was there, so restart herdr to \
                 use it.",
            );
        }
    }
    if here {
        return;
    }
    save_and_tell(
        world,
        io,
        "stt",
        "model",
        crate::config_edit::quote(id),
        &format!("{id:?}"),
    );
}

/// `herdr-voice model`, and `--choose`. Returns the process's exit code.
pub fn run(choosing: bool) -> u8 {
    let mut world = Real::from_env();
    if !choosing {
        let snapshot = world.snapshot();
        let models = world.models_dir();
        print!(
            "{}",
            list(
                &catalogue::MODELS,
                &snapshot.loaded.config.stt.model,
                &|entry| world.model_state(entry),
                models.as_deref(),
            )
        );
        return 0;
    }
    let code = {
        let stdin = std::io::stdin();
        let mut input = stdin.lock();
        let mut out = std::io::stdout();
        let mut io = Io::new(&mut input, &mut out);
        choose_speech_model(&mut world, &mut io, &catalogue::MODELS);
        u8::from(io.failed)
    };
    crate::popup::pause();
    code
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::popup::tests_support::FakeWorld;
    use crate::popup::Reached;
    use std::io::Cursor;
    use std::path::PathBuf;

    /// Runs the flow with `typed` as the person's input; returns the printed text.
    fn drive(world: &mut FakeWorld, typed: &str, catalogue: &'static [Entry]) -> (String, bool) {
        let mut input = Cursor::new(typed.as_bytes().to_vec());
        let mut out: Vec<u8> = Vec::new();
        let mut io = Io::new(&mut input, &mut out);
        choose_speech_model(world, &mut io, catalogue);
        let failed = io.failed;
        (String::from_utf8(out).unwrap(), failed)
    }

    fn state(glance: Glance) -> impl Fn(&Entry) -> Glance {
        move |_| glance
    }

    #[test]
    fn the_listing_shows_every_model_with_a_size_before_anything_is_downloaded() {
        let text = list(
            &catalogue::MODELS,
            "large-v3-turbo",
            &state(Glance::Absent),
            Some(&PathBuf::from("/nowhere-at-all")),
        );
        for entry in catalogue::MODELS.iter() {
            assert!(
                text.contains(entry.identifier),
                "{} is missing: {text}",
                entry.identifier
            );
        }
        assert!(text.contains("151 MB"), "tiny's size: {text}");
        assert!(text.contains("1.62 GB"), "the default's size: {text}");
        assert!(
            text.contains("not installed"),
            "it must say what is there: {text}"
        );
        assert!(text.contains("/nowhere-at-all/candle"), "{text}");
    }

    #[test]
    fn the_listing_says_what_is_installed_and_what_is_the_wrong_size() {
        let whole = list(&catalogue::MODELS, "", &state(Glance::Whole), None);
        assert!(whole.contains("— installed"), "{whole}");
        assert!(!whole.contains("not installed"), "{whole}");
        let wrong = list(&catalogue::MODELS, "", &state(Glance::WrongSize), None);
        assert!(wrong.contains("the wrong size"), "{wrong}");
        assert!(
            !wrong.contains("They live in"),
            "no directory, no line about it: {wrong}"
        );
    }

    #[test]
    fn the_configured_model_is_marked_in_the_listing() {
        let text = list(&catalogue::MODELS, "small", &state(Glance::Absent), None);
        let marked: Vec<&str> = text.lines().filter(|l| l.contains("small")).collect();
        assert_eq!(marked.len(), 1, "got {marked:?}");
        assert!(marked[0].contains("current"), "got {}", marked[0]);
    }

    #[test]
    fn sizes_are_rendered_the_way_a_person_reads_them() {
        assert_eq!(human(151_061_672), "151 MB");
        assert_eq!(human(1_617_824_864), "1.62 GB");
        assert_eq!(human(3_087_130_976), "3.09 GB");
    }

    #[test]
    fn with_the_candle_engine_a_model_that_is_not_there_is_installed_and_written() {
        let mut world = FakeWorld::new(
            "choose-candle",
            "[stt]\nengine = \"candle\"\nlanguage = \"ru\"\n",
        );
        world.reached = Reached::NoDaemon;
        let (said, failed) = drive(&mut world, "1\n", &catalogue::MODELS);
        assert!(!failed, "{said}");
        assert_eq!(world.installs, vec!["tiny".to_string()]);
        assert_eq!(world.saved, vec!["[stt] model = \"tiny\"".to_string()]);
        assert_eq!(world.told, 1);
        assert!(said.contains("Installing tiny (151 MB)"), "{said}");
        // The configuration now uses the model, and nothing else in it changed.
        let file = world.file();
        assert!(file.contains("model = \"tiny\""), "{file}");
        assert!(
            file.contains("engine = \"candle\"") && file.contains("language = \"ru\""),
            "{file}"
        );
    }

    #[test]
    fn with_the_candle_engine_a_model_that_is_there_but_not_configured_is_not_downloaded_again() {
        let mut world = FakeWorld::new("choose-candle-there", "[stt]\nengine = \"candle\"\n");
        world.states.push(("small".to_string(), Glance::Whole));
        let (said, _) = drive(&mut world, "3\n", &catalogue::MODELS);
        assert!(world.installs.is_empty(), "{said}");
        assert_eq!(world.saved, vec!["[stt] model = \"small\"".to_string()]);
        assert!(said.contains("small is already installed"), "{said}");
    }

    #[test]
    fn with_the_candle_engine_the_configured_installed_model_is_said_to_be_so_and_nothing_happens()
    {
        let mut world = FakeWorld::new(
            "choose-candle-configured",
            "[stt]\nengine = \"candle\"\nmodel = \"tiny\"\n",
        );
        world.states.push(("tiny".to_string(), Glance::Whole));
        let before = world.file();
        let (said, failed) = drive(&mut world, "1\n", &catalogue::MODELS);
        assert!(!failed);
        assert!(
            said.contains("tiny is installed and already configured"),
            "{said}"
        );
        assert!(world.installs.is_empty() && world.saved.is_empty());
        assert_eq!(world.told, 0);
        assert_eq!(world.file(), before);
    }

    #[test]
    fn with_the_candle_engine_the_configured_model_that_is_missing_is_installed_without_a_write() {
        let mut world = FakeWorld::new(
            "choose-candle-missing",
            "[stt]\nengine = \"candle\"\nmodel = \"tiny\"\n",
        );
        let (said, _) = drive(&mut world, "1\n", &catalogue::MODELS);
        assert_eq!(world.installs, vec!["tiny".to_string()]);
        assert!(world.saved.is_empty(), "the key already says tiny: {said}");
    }

    #[test]
    fn a_failed_download_says_so_writes_nothing_and_fails() {
        let mut world = FakeWorld::new("choose-install-fails", "[stt]\nengine = \"candle\"\n");
        world.install_error = Some("cannot reach huggingface.co; check the network".to_string());
        let (said, failed) = drive(&mut world, "1\n", &catalogue::MODELS);
        assert!(failed);
        assert!(said.contains("cannot reach huggingface.co"), "{said}");
        assert!(world.saved.is_empty());
        assert_eq!(world.told, 0);
    }

    #[test]
    fn with_a_command_that_uses_the_model_nothing_is_downloaded_or_written_and_the_file_is_named() {
        let mut world = FakeWorld::new(
            "choose-command-model",
            "[stt]\nengine = \"command\"\ncommand = [\"whisper-cli\", \"-m\", \"{model}\"]\n",
        );
        let before = world.file();
        let (said, failed) = drive(&mut world, "1\n", &catalogue::MODELS);
        assert!(!failed, "{said}");
        assert!(world.installs.is_empty() && world.saved.is_empty());
        assert_eq!(world.file(), before);
        assert!(said.contains("ggml-tiny.bin"), "{said}");
        assert!(
            said.contains("/models/ggml-tiny.bin"),
            "the models directory is named: {said}"
        );
        assert!(said.contains("set [stt] engine to \"candle\""), "{said}");
        assert!(said.contains("Nothing was downloaded or written"), "{said}");
    }

    #[test]
    fn with_the_http_engine_the_model_key_is_said_not_to_be_used() {
        let mut world = FakeWorld::new("choose-http", "[stt]\nengine = \"http\"\n");
        let (said, _) = drive(&mut world, "1\n", &catalogue::MODELS);
        assert!(world.installs.is_empty() && world.saved.is_empty());
        assert!(said.contains("[stt] http_model"), "{said}");
        assert!(said.contains("set [stt] engine to \"candle\""), "{said}");
    }

    #[test]
    fn an_engine_that_is_none_of_the_three_is_named_with_the_three() {
        let mut world = FakeWorld::new("choose-unknown", "[stt]\nengine = \"whisperx\"\n");
        let (said, _) = drive(&mut world, "1\n", &catalogue::MODELS);
        assert!(world.installs.is_empty() && world.saved.is_empty());
        assert!(
            said.contains("\"whisperx\"") && said.contains("candle, http, command"),
            "{said}"
        );
    }

    #[test]
    fn an_empty_catalogue_says_what_happened_and_fails() {
        let mut world = FakeWorld::new("choose-empty", "");
        let (said, failed) = drive(&mut world, "1\n", &[]);
        assert!(failed);
        assert!(said.contains("No speech models are on offer"), "{said}");
        assert!(said.contains("report it"), "{said}");
    }

    #[test]
    fn esc_an_empty_line_and_a_number_outside_the_list_change_nothing() {
        for (typed, expected) in [
            ("\u{1b}\n", "Nothing was changed"),
            ("\n", "Nothing was changed"),
            ("9\n", "between 1 and 6"),
        ] {
            let mut world = FakeWorld::new("choose-leave", "[stt]\nengine = \"candle\"\n");
            let (said, failed) = drive(&mut world, typed, &catalogue::MODELS);
            assert!(!failed, "{typed:?}: {said}");
            assert!(said.contains(expected), "{typed:?}: {said}");
            assert!(world.installs.is_empty() && world.saved.is_empty());
        }
    }

    #[test]
    fn the_end_of_the_input_is_said_and_fails() {
        let mut world = FakeWorld::new("choose-eof", "[stt]\nengine = \"candle\"\n");
        let (said, failed) = drive(&mut world, "", &catalogue::MODELS);
        assert!(failed);
        assert!(
            said.contains("nothing was read from the terminal"),
            "{said}"
        );
    }

    #[test]
    fn a_configuration_that_does_not_parse_is_said_before_the_list() {
        let mut world = FakeWorld::new("choose-invalid", "[stt\nengine = ");
        let (said, _) = drive(&mut world, "\n", &catalogue::MODELS);
        assert!(said.contains("does not load"), "{said}");
        assert!(said.find("does not load").unwrap() < said.find("1. tiny").unwrap());
    }

    #[test]
    fn installing_the_configured_model_that_was_missing_says_to_restart_herdr() {
        let mut world = FakeWorld::new(
            "choose-restart-after-install",
            "[stt]\nengine = \"candle\"\nmodel = \"tiny\"\n",
        );
        let (said, _) = drive(&mut world, "1\n", &catalogue::MODELS);
        assert_eq!(world.installs, vec!["tiny".to_string()]);
        assert!(said.contains("restart herdr"), "{said}");
        assert!(said.contains("before it was there"), "{said}");
    }

    #[test]
    fn the_shipped_default_is_said_to_have_no_command_at_all_and_not_to_bring_its_own_model() {
        let mut world = FakeWorld::new("choose-default", "");
        let (said, _) = drive(&mut world, "1\n", &catalogue::MODELS);
        assert!(world.installs.is_empty() && world.saved.is_empty());
        assert!(said.contains("[stt] command is empty"), "{said}");
        assert!(said.contains("herdr-voice doctor"), "{said}");
        assert!(!said.contains("brings its own model"), "{said}");
        assert!(said.contains("set [stt] engine to \"candle\""), "{said}");
    }

    #[test]
    fn a_command_without_the_model_placeholder_is_said_to_bring_its_own_model() {
        let mut world = FakeWorld::new(
            "choose-own-model",
            "[stt]\nengine = \"command\"\ncommand = [\"my-transcriber\", \"{audio}\"]\n",
        );
        let (said, _) = drive(&mut world, "1\n", &catalogue::MODELS);
        assert!(world.installs.is_empty() && world.saved.is_empty());
        assert!(said.contains("brings its own model"), "{said}");
    }

    #[test]
    fn the_advice_for_a_command_that_uses_the_model_includes_naming_it_in_the_configuration() {
        let mut world = FakeWorld::new(
            "choose-advice",
            "[stt]\nengine = \"command\"\ncommand = [\"whisper-cli\", \"-m\", \"{model}\"]\n",
        );
        let (said, _) = drive(&mut world, "1\n", &catalogue::MODELS);
        assert!(
            said.contains("then set [stt] model to \"tiny\" in the configuration file"),
            "{said}"
        );
    }
}
