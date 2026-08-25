//! The long-lived half. It owns the listener and answers one frame per connection.
//!
//! Starting order matters and is the reverse of the intuitive one: connect first,
//! listen second. A stale socket file refuses connections while a live daemon
//! accepts them, so connecting is the only way to tell one from the other —
//! and reclaiming the name without checking would take it from a running daemon.
//! See `tasks/3/DESIGN_3.md`, section 3.

use std::io::BufReader;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;

use crate::capture::{Recorder, Started};
use crate::config;
use crate::context;
use crate::proto::{Reply, Request};
use crate::stt::{self, Engine};
use crate::transport::{self, Address, TransportError};

/// Whether the accept loop keeps going after this request.
#[derive(Debug, PartialEq, Eq)]
pub enum Control {
    Continue,
    Stop,
}

#[derive(Debug)]
pub enum Outcome {
    /// Another daemon holds the name; this process has nothing to do.
    AlreadyRunning(String),
    /// The loop ran and ended.
    Served,
}

/// Commands that deliver into a pane, and therefore need herdr to have named one.
/// `cancel` is not one of them: it stops whatever is running and clears what a
/// dead run left behind, neither of which needs a target.
pub fn needs_target_pane(command: &str) -> bool {
    matches!(command, "dictate" | "ptt")
}

/// What the daemon can transcribe with, or why it cannot. Resolved once at start,
/// reported when a take finishes rather than at start-up: the daemon still has to
/// run for `cancel` and for `doctor`, which is where somebody finds out what to fix.
pub type Recognition = Result<Box<dyn Engine + Send + Sync>, String>;

/// The four things resolved once, at daemon start, and needed everywhere a
/// take can finish.
pub struct Runtime {
    pub recognition: Recognition,
    pub deliverer: Box<dyn crate::delivery::Deliverer>,
    pub delivery_settings: crate::delivery::Settings,
    pub journal: Box<dyn Journal>,
}

pub fn answer(request: &Request, recorder: &Recorder, runtime: &Runtime) -> (Reply, Control) {
    match request.command.as_str() {
        "stop" => (Reply::Ok("stopping".to_string()), Control::Stop),
        "cancel" => (
            Reply::Ok("nothing to cancel".to_string()),
            Control::Continue,
        ),
        command if needs_target_pane(command) => match context::parse(&request.context) {
            Err(why) => (Reply::Error(why.to_string()), Control::Continue),
            Ok(invocation) => match invocation.target_pane() {
                None => (
                    Reply::Error(
                        "the invocation context names no focused pane; \
                         invoke this from a pane running an agent"
                            .to_string(),
                    ),
                    Control::Continue,
                ),
                Some(pane) if command == "dictate" => (
                    dictate(
                        recorder,
                        runtime,
                        pane,
                        invocation.focused_pane_agent.as_deref(),
                    ),
                    Control::Continue,
                ),
                Some(_) => (
                    Reply::Ok(format!("{command}: not implemented yet")),
                    Control::Continue,
                ),
            },
        },
        other => (
            Reply::Error(format!("unknown command: {other}")),
            Control::Continue,
        ),
    }
}

/// One keypress of the dictation toggle, against the pane herdr named.
///
/// The pane is pinned here, when the take begins, and kept with it until delivery.
/// Choosing it at the end would follow the focus: somebody speaks looking at one
/// agent, switches while thinking, and the text lands in another.
fn dictate(recorder: &Recorder, runtime: &Runtime, pane: &str, agent: Option<&str>) -> Reply {
    match recorder.start(pane, agent) {
        Started::Began => Reply::Ok(format!("recording for {pane}")),
        Started::CouldNotStart(why) => Reply::Error(why),
        Started::PreviousFailure(why) => Reply::Error(why),
        Started::AlreadyRunning => match recorder.stop() {
            Err(why) => Reply::Error(why.to_string()),
            Ok(take) => transcribe(runtime, &take),
        },
    }
}

/// A finished take becomes text, and the text is delivered — or, if either
/// step fails, the reply is a Reply::Error naming why and what to do next
/// (client::outcome maps Reply::Ok to exit 0, Reply::Error to exit 1).
fn transcribe(runtime: &Runtime, take: &crate::capture::Take) -> Reply {
    let engine = match &runtime.recognition {
        Ok(engine) => engine,
        // The take is on disk and named, so nothing is lost by the engine being
        // absent: somebody can fix the configuration and the file is still there.
        Err(why) => {
            return Reply::Error(format!(
                "{why} — the take is kept at {}",
                take.path.display()
            ))
        }
    };
    let text = match engine.transcribe(&take.path) {
        Ok(text) => text,
        Err(why) => {
            return Reply::Error(format!(
                "{why} — the take is kept at {}",
                take.path.display()
            ))
        }
    };

    // Written before the delivery attempt: the text must not be held only
    // in memory while the outward call to herdr runs.
    runtime.journal.write(&delivering_line(&text));

    match crate::delivery::deliver(
        runtime.deliverer.as_ref(),
        runtime.delivery_settings.submit,
        take.agent.as_deref(),
        &take.target,
        &text,
    ) {
        Ok(()) => Reply::Ok(format!(
            "delivered to {} [{:.1} dB]",
            take.target, take.level_dbfs
        )),
        Err(why) => {
            let why = why.to_string().replace('\n', " ");
            runtime
                .journal
                .write(&delivery_failed_line(&take.target, &why));
            if runtime.delivery_settings.toasts {
                // A toast that could not be shown must not stop the journal
                // line or the client's reply from getting through — but its
                // own failure is still a lost diagnostic, so it is journaled.
                if let Err(toast_why) = runtime.deliverer.notify(
                    "Delivery failed",
                    &format!("{}: the text is in the plugin log", take.target),
                ) {
                    runtime
                        .journal
                        .write(&toast_failed_line(&toast_why.to_string().replace('\n', " ")));
                }
            }
            Reply::Error(format!(
                "could not deliver to {} ({why}) — the take is kept at {}; text: {}",
                take.target,
                take.path.display(),
                text.replace('\n', " "),
            ))
        }
    }
}

/// What the daemon records about the body, if anything is wrong with it.
///
/// A command that needs no pane still records this. The body it could not read is
/// the same body the next pane-needing command will get, and a silent skip here is
/// exactly the failure mode the project treats as a defect: the prototype spent a
/// morning looking like a hang because a parse error produced no output at all.
pub fn context_note(request: &Request) -> Option<String> {
    match context::parse(&request.context) {
        Ok(_) => None,
        Err(why) => Some(format!("context unreadable: {why}")),
    }
}

/// One line per accepted request, on standard error. herdr captures a plugin's
/// standard error, so `herdr plugin log list --plugin haurylau.voice` shows it.
pub fn request_line(request: &Request) -> String {
    format!(
        "request command={} entrypoint={} context={} bytes",
        request.command,
        request.entrypoint.as_deref().unwrap_or("-"),
        request.context.len()
    )
}

/// Where a journal line goes. Production writes to standard error, the
/// channel request_line/context_note already use; a test substitutes
/// something it can read back, in order, without touching real stderr.
pub trait Journal: Send + Sync {
    fn write(&self, line: &str);
}

pub struct StderrJournal;
impl Journal for StderrJournal {
    fn write(&self, line: &str) {
        eprintln!("{line}");
    }
}

/// Written before a delivery attempt, so the text is not held only in
/// memory while the outward call to herdr runs.
pub fn delivering_line(text: &str) -> String {
    format!("delivering: {text}")
}

/// Written when a delivery attempt is rejected.
pub fn delivery_failed_line(target: &str, why: &str) -> String {
    format!("delivery failed: pane={target} reason={why}")
}

/// Written when the toast itself could not be raised — herdr's `notification
/// show` failed on top of the delivery it was reporting. The delivery-failed
/// line above already carries the pane and the reason; this line exists so
/// that a toast nobody saw is not also a diagnostic nobody sees.
pub fn toast_failed_line(why: &str) -> String {
    format!("toast failed: {why}")
}

pub fn start() -> Result<Outcome, TransportError> {
    let address = transport::address(&transport::Vars::from_env())?;

    // Connect first. A successful connection means a live daemon owns the name.
    if transport::connect(&address).is_ok() {
        return Ok(Outcome::AlreadyRunning(address.display().to_string()));
    }

    let listener = transport::listen(&address)?;
    eprintln!("listening at {}", address.display());

    // The configuration is read once, here, and handed to the recorder's thread:
    // re-reading it per take would put file system access on the path that runs
    // while somebody is speaking.
    let loaded = config::load(config::directory(&config::Vars::from_env()).as_deref());
    let takes = transport::state_directory(&transport::Vars::from_env())
        .map(|state| state.join("takes"))
        .unwrap_or_else(|| std::path::PathBuf::from("takes"));
    let models = transport::state_directory(&transport::Vars::from_env())
        .map(|state| state.join("models"))
        .unwrap_or_else(|| std::path::PathBuf::from("models"));
    // Not fatal: the daemon still answers `cancel`, and `doctor` reports the same
    // thing this does. The reason is kept and given to whoever finishes a take.
    let recognition: Recognition = stt::resolve(&loaded.config.stt, &models).map_err(|e| {
        eprintln!("recognition unavailable: {e}");
        e.to_string()
    });

    let recorder = Recorder::spawn(
        || Box::new(crate::capture::cpal_source::CpalSource::new()),
        loaded.config.audio,
        takes,
    );

    let runtime = Runtime {
        recognition,
        deliverer: Box::new(crate::delivery::HerdrDeliverer::new()),
        delivery_settings: crate::delivery::Settings {
            submit: loaded.config.delivery.submit,
            toasts: loaded.config.ui.toasts,
        },
        journal: Box::new(StderrJournal),
    };
    serve(listener, address, Arc::new(recorder), Arc::new(runtime));
    Ok(Outcome::Served)
}

fn serve(
    listener: transport::Listener,
    address: Address,
    recorder: Arc<Recorder>,
    runtime: Arc<Runtime>,
) {
    let stop = Arc::new(AtomicBool::new(false));
    loop {
        let connection = match listener.accept() {
            Ok(connection) => connection,
            Err(e) => {
                eprintln!("accept failed: {e}");
                continue;
            }
        };
        if stop.load(Ordering::SeqCst) {
            break;
        }
        let stop = Arc::clone(&stop);
        let address = address.clone();
        let recorder = Arc::clone(&recorder);
        let runtime = Arc::clone(&runtime);
        thread::spawn(move || {
            if let Err(e) = serve_one(connection, &stop, &address, &recorder, &runtime) {
                eprintln!("connection failed: {e}");
            }
        });
    }
}

fn serve_one(
    connection: transport::Stream,
    stop: &AtomicBool,
    address: &Address,
    recorder: &Recorder,
    runtime: &Runtime,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut reader = BufReader::new(connection);
    let request = match Request::read_from(&mut reader) {
        // A liveness probe: `doctor` and a second `daemon` start both connect and
        // close without sending a frame. Reporting that as a failure would fill the
        // log with alarming lines about the normal case.
        Err(crate::proto::ProtoError::Empty) => return Ok(()),
        other => other?,
    };
    eprintln!("{}", request_line(&request));
    if let Some(note) = context_note(&request) {
        eprintln!("{note}");
    }
    let (reply, control) = answer(&request, recorder, runtime);
    reply.write_to(reader.get_mut())?;
    if control == Control::Stop {
        stop.store(true, Ordering::SeqCst);
        // Unblock the accept that is waiting, so the loop can see the flag.
        let _ = transport::connect(address);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A recorder that hears one quiet moment and stops. Enough for the dispatch
    /// tests, which are about which branch runs, not about audio.
    fn silent_recorder() -> Recorder {
        Recorder::spawn(
            || Box::new(crate::capture::tests_support::SilentSource),
            crate::config::Audio::default(),
            std::env::temp_dir().join(format!("daemon-takes-{}", std::process::id())),
        )
    }

    /// A runtime that always produces the same transcript, delivers against a
    /// fake that never fails, and never submits or toasts — so a dispatch
    /// test is about which branch runs rather than about speech or delivery.
    fn fake_runtime(text: &str) -> Runtime {
        Runtime {
            recognition: Ok(Box::new(crate::stt::tests_support::Fake(Ok(
                text.to_string()
            )))),
            deliverer: Box::new(crate::delivery::tests_support::FakeDeliverer::ok()),
            delivery_settings: crate::delivery::Settings {
                submit: false,
                toasts: false,
            },
            journal: Box::new(StderrJournal),
        }
    }

    fn request(command: &str, context: &[u8]) -> Request {
        Request {
            command: command.to_string(),
            entrypoint: Some(command.to_string()),
            context: context.to_vec(),
        }
    }

    /// A recorder that hears one loud moment and stops — clears the silence
    /// floor, so a test can reach transcription and delivery.
    fn tone_recorder(tag: &str) -> Recorder {
        Recorder::spawn(
            || Box::new(crate::capture::tests_support::ToneSource),
            crate::config::Audio::default(),
            std::env::temp_dir().join(format!("daemon-takes-{tag}-{}", std::process::id())),
        )
    }

    fn dictate_request() -> Request {
        request(
            "dictate",
            br#"{"focused_pane_id":"w1:p2","focused_pane_agent":"claude"}"#,
        )
    }

    fn runtime_with(
        deliverer: crate::delivery::tests_support::FakeDeliverer,
        submit: bool,
    ) -> Runtime {
        Runtime {
            recognition: Ok(Box::new(crate::stt::tests_support::Fake(Ok(
                "fix the worklog entry".to_string(),
            )))),
            deliverer: Box::new(deliverer),
            delivery_settings: crate::delivery::Settings {
                submit,
                toasts: false,
            },
            journal: Box::new(StderrJournal),
        }
    }

    #[test]
    fn a_finished_take_reaches_delivery_and_the_reply_confirms_the_pane_not_the_text() {
        let recorder = tone_recorder("delivered");
        let runtime = runtime_with(crate::delivery::tests_support::FakeDeliverer::ok(), false);
        let request = dictate_request();
        answer(&request, &recorder, &runtime);
        let (reply, _) = answer(&request, &recorder, &runtime);
        match reply {
            Reply::Ok(text) => {
                assert!(text.contains("delivered to w1:p2"), "got {text:?}");
                assert!(text.contains("dB"), "got {text:?}");
                assert!(!text.contains("fix the worklog entry"), "got {text:?}");
            }
            other => panic!("expected a confirmation, got {other:?}"),
        }
    }

    #[test]
    fn submit_off_inserts_and_never_submits() {
        let recorder = tone_recorder("insert-only");
        let fake = crate::delivery::tests_support::FakeDeliverer::ok();
        let runtime = runtime_with(fake.clone(), false);
        let request = dictate_request();
        answer(&request, &recorder, &runtime);
        answer(&request, &recorder, &runtime);
        assert_eq!(
            fake.calls(),
            vec![crate::delivery::tests_support::Call::Insert(
                "w1:p2".into(),
                "fix the worklog entry".into()
            )]
        );
    }

    #[test]
    fn submit_on_with_an_agent_submits() {
        let recorder = tone_recorder("submit");
        let fake = crate::delivery::tests_support::FakeDeliverer::ok();
        let runtime = runtime_with(fake.clone(), true); // dictate_request() names "claude"
        let request = dictate_request();
        answer(&request, &recorder, &runtime);
        answer(&request, &recorder, &runtime);
        assert_eq!(
            fake.calls(),
            vec![crate::delivery::tests_support::Call::Submit(
                "w1:p2".into(),
                "fix the worklog entry".into()
            )]
        );
    }

    #[test]
    fn submit_on_with_no_agent_falls_back_to_insert() {
        let recorder = tone_recorder("fallback");
        let fake = crate::delivery::tests_support::FakeDeliverer::ok();
        let runtime = runtime_with(fake.clone(), true);
        let request = request("dictate", br#"{"focused_pane_id":"w1:p2"}"#); // no agent
        answer(&request, &recorder, &runtime);
        answer(&request, &recorder, &runtime);
        assert_eq!(
            fake.calls(),
            vec![crate::delivery::tests_support::Call::Insert(
                "w1:p2".into(),
                "fix the worklog entry".into()
            )]
        );
    }

    #[test]
    fn a_rejected_call_fails_the_delivery_keeps_the_audio_and_carries_the_text_and_the_reason() {
        let recorder = tone_recorder("rejected");
        let fake = crate::delivery::tests_support::FakeDeliverer::failing(
            crate::delivery::DeliveryError::Rejected("pane_not_found".into()),
        );
        let runtime = runtime_with(fake, false);
        let request = dictate_request();
        answer(&request, &recorder, &runtime);
        let (reply, _) = answer(&request, &recorder, &runtime);
        let text = match reply {
            Reply::Error(text) => text,
            other => panic!("expected a failed delivery (Reply::Error), got {other:?}"),
        };
        assert!(
            text.contains("could not deliver to w1:p2 (pane_not_found)"),
            "got {text:?}"
        );
        assert!(text.contains("text: fix the worklog entry"), "got {text:?}");
        let path = text
            .split("kept at ")
            .nth(1)
            .unwrap()
            .split(';')
            .next()
            .unwrap();
        assert!(std::path::Path::new(path).exists(), "AC-10: {path}");
        std::fs::remove_file(path).ok();
    }

    /// Records every line written, in order.
    #[derive(Default)]
    struct RecordingJournal(std::sync::Mutex<Vec<String>>);
    impl Journal for RecordingJournal {
        fn write(&self, line: &str) {
            self.0.lock().unwrap().push(line.to_string());
        }
    }
    /// Lets a Runtime own a Journal while the test keeps its own handle to read
    /// what was written — the same shape FakeDeliverer::clone() gives above.
    struct TestJournal(std::sync::Arc<RecordingJournal>);
    impl Journal for TestJournal {
        fn write(&self, line: &str) {
            self.0.write(line);
        }
    }

    #[test]
    fn the_delivering_line_precedes_the_failure_line_and_names_the_reason() {
        let recorder = tone_recorder("journal-order");
        let fake = crate::delivery::tests_support::FakeDeliverer::failing(
            crate::delivery::DeliveryError::Rejected("pane_not_found".into()),
        );
        let journal = std::sync::Arc::new(RecordingJournal::default());
        let mut runtime = runtime_with(fake, false);
        runtime.journal = Box::new(TestJournal(std::sync::Arc::clone(&journal)));
        let request = dictate_request();
        answer(&request, &recorder, &runtime);
        answer(&request, &recorder, &runtime);

        let lines = journal.0.lock().unwrap();
        assert_eq!(lines.len(), 2, "got {lines:?}");
        assert!(lines[0].contains("fix the worklog entry"), "got {lines:?}");
        assert!(
            lines[1].contains("w1:p2") && lines[1].contains("pane_not_found"),
            "got {lines:?}"
        );
    }

    #[test]
    fn a_toast_is_raised_on_a_failed_delivery_only_when_ui_toasts_is_on() {
        for (toasts, expect_notify) in [(true, true), (false, false)] {
            let recorder = tone_recorder(&format!("toast-{toasts}"));
            let fake = crate::delivery::tests_support::FakeDeliverer::failing(
                crate::delivery::DeliveryError::Rejected("pane_not_found".into()),
            );
            let mut runtime = runtime_with(fake.clone(), false);
            runtime.delivery_settings.toasts = toasts;
            let request = dictate_request();
            answer(&request, &recorder, &runtime);
            answer(&request, &recorder, &runtime);
            let notified = fake
                .calls()
                .iter()
                .any(|c| matches!(c, crate::delivery::tests_support::Call::Notify(..)));
            assert_eq!(notified, expect_notify, "toasts = {toasts}");
            if notified {
                assert!(matches!(
                    fake.calls().last(),
                    Some(crate::delivery::tests_support::Call::Notify(title, body))
                        if title == "Delivery failed" && body == "w1:p2: the text is in the plugin log"
                ));
            }
        }
    }

    #[test]
    fn a_toast_that_cannot_be_raised_is_still_recorded_in_the_journal() {
        let recorder = tone_recorder("toast-fails");
        let fake = crate::delivery::tests_support::FakeDeliverer::failing(
            crate::delivery::DeliveryError::Rejected("pane_not_found".into()),
        )
        .and_notify_fails(crate::delivery::DeliveryError::NotFound {
            binary: "herdr".into(),
            path: "/usr/bin".into(),
        });
        let journal = std::sync::Arc::new(RecordingJournal::default());
        let mut runtime = runtime_with(fake, false);
        runtime.delivery_settings.toasts = true;
        runtime.journal = Box::new(TestJournal(std::sync::Arc::clone(&journal)));
        let request = dictate_request();
        answer(&request, &recorder, &runtime);
        answer(&request, &recorder, &runtime);

        let lines = journal.0.lock().unwrap();
        assert_eq!(lines.len(), 3, "the toast's own failure must be journaled too, got {lines:?}");
        assert!(lines[0].contains("fix the worklog entry"), "got {lines:?}");
        assert!(
            lines[1].contains("w1:p2") && lines[1].contains("pane_not_found"),
            "got {lines:?}"
        );
        assert!(lines[2].contains("toast"), "got {lines:?}");
    }

    #[test]
    fn cancel_needs_no_pane_and_dictate_does() {
        assert!(!needs_target_pane("cancel"));
        assert!(needs_target_pane("dictate"));
        assert!(needs_target_pane("ptt"));
    }

    #[test]
    fn cancel_works_with_no_context_at_all() {
        let (reply, control) = answer(
            &request("cancel", b""),
            &silent_recorder(),
            &fake_runtime("x"),
        );
        assert!(matches!(reply, Reply::Ok(_)), "got {reply:?}");
        assert!(matches!(control, Control::Continue));
    }

    #[test]
    fn cancel_works_with_a_malformed_context() {
        let (reply, _) = answer(
            &request("cancel", b"{not json"),
            &silent_recorder(),
            &fake_runtime("x"),
        );
        assert!(matches!(reply, Reply::Ok(_)), "got {reply:?}");
    }

    #[test]
    fn a_command_that_needs_a_pane_says_what_was_missing() {
        let (reply, _) = answer(
            &request("dictate", b""),
            &silent_recorder(),
            &fake_runtime("x"),
        );
        match reply {
            Reply::Error(text) => assert!(
                text.contains("HERDR_PLUGIN_CONTEXT_JSON"),
                "the message must name what was missing, got {text:?}"
            ),
            other => panic!("expected an error, got {other:?}"),
        }
    }

    #[test]
    fn dictate_with_a_pane_starts_a_take_and_names_the_pane() {
        let recorder = silent_recorder();
        let (reply, _) = answer(
            &request("dictate", br#"{"focused_pane_id":"w1:p2"}"#),
            &recorder,
            &fake_runtime("hello"),
        );
        match reply {
            Reply::Ok(text) => assert!(text.contains("w1:p2"), "got {text:?}"),
            other => panic!("expected the take to begin, got {other:?}"),
        }
    }

    #[test]
    fn the_second_dictate_finishes_the_take_rather_than_starting_another() {
        let recorder = silent_recorder();
        let request = request("dictate", br#"{"focused_pane_id":"w1:p2"}"#);
        answer(&request, &recorder, &fake_runtime("hello"));
        let (reply, _) = answer(&request, &recorder, &fake_runtime("hello"));
        // The take is silence, so it is refused — which is itself proof that the
        // second keypress finished it instead of starting a second one.
        match reply {
            Reply::Error(text) => assert!(
                text.contains("below") && text.contains("dB"),
                "got {text:?}"
            ),
            other => panic!("expected the silent take to be refused, got {other:?}"),
        }
    }

    #[test]
    fn an_unknown_command_is_refused_by_name() {
        let (reply, _) = answer(
            &request("transcribe", b""),
            &silent_recorder(),
            &fake_runtime("x"),
        );
        match reply {
            Reply::Error(text) => assert!(text.contains("transcribe"), "got {text:?}"),
            other => panic!("expected an error, got {other:?}"),
        }
    }

    #[test]
    fn the_stop_request_ends_the_loop() {
        let (reply, control) = answer(
            &request("stop", b""),
            &silent_recorder(),
            &fake_runtime("x"),
        );
        assert!(matches!(reply, Reply::Ok(_)), "got {reply:?}");
        assert!(matches!(control, Control::Stop));
    }

    #[test]
    fn a_body_that_will_not_parse_is_recorded_even_for_cancel() {
        let absent = context_note(&request("cancel", b"")).expect("a note");
        assert!(
            absent.contains("HERDR_PLUGIN_CONTEXT_JSON"),
            "got {absent:?}"
        );

        let malformed = context_note(&request("cancel", b"{not json")).expect("a note");
        assert!(malformed.contains("unreadable"), "got {malformed:?}");

        assert_eq!(
            context_note(&request("cancel", br#"{"tab_id":"t1"}"#)),
            None
        );
    }

    #[test]
    fn the_recorded_line_names_the_command_and_the_entrypoint() {
        let line = request_line(&request("cancel", b"{}"));
        assert!(line.contains("cancel"), "got {line:?}");
        assert!(line.contains("entrypoint=cancel"), "got {line:?}");

        let anonymous = Request {
            command: "cancel".into(),
            entrypoint: None,
            context: vec![],
        };
        assert!(request_line(&anonymous).contains("entrypoint=-"));
    }

    #[test]
    fn the_delivering_line_carries_the_text() {
        assert!(delivering_line("fix the worklog entry").contains("fix the worklog entry"));
    }

    #[test]
    fn the_delivery_failed_line_names_the_pane_and_the_reason() {
        let line = delivery_failed_line("w99:p99", "pane_not_found");
        assert!(line.contains("w99:p99"));
        assert!(line.contains("pane_not_found"));
    }

    #[test]
    fn the_toast_failed_line_names_the_reason() {
        let line = toast_failed_line("herdr not found");
        assert!(line.contains("herdr not found"), "got {line:?}");
    }

    #[test]
    fn a_liveness_probe_is_not_reported_as_a_failure() {
        // Connect, close, and let the daemon handle it. The assertion is that
        // serve_one treats it as nothing to do rather than as a broken peer.
        let address = crate::transport::tests_support::probe_address("probe");
        let listener = crate::transport::listen(&address).expect("listen");
        let (accepted, has_accepted) = std::sync::mpsc::channel();
        let served = {
            let address = address.clone();
            std::thread::spawn(move || {
                let connection = listener.accept().expect("accept");
                accepted.send(()).expect("announce the accept");
                let stop = AtomicBool::new(false);
                // The error type is not Send, so the verdict crosses the join, not it.
                super::serve_one(
                    connection,
                    &stop,
                    &address,
                    &silent_recorder(),
                    &fake_runtime("x"),
                )
                .map_err(|e| e.to_string())
            })
        };

        // The probe holds the connection open until the daemon has accepted it, and
        // only then goes away. Dropping it earlier is a Unix-shaped test: a socket
        // queues a connection whose client has already left, and a Windows named
        // pipe does not, so `accept` would wait for a client that never comes.
        let probe = crate::transport::connect(&address).expect("connect");
        has_accepted
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("the daemon must accept the probe");
        drop(probe);
        assert_eq!(
            served.join().expect("the handler must finish"),
            Ok(()),
            "a probe must not be an error"
        );
    }

    #[test]
    fn a_second_start_finds_the_first_and_a_stop_ends_it() {
        let address = crate::transport::tests_support::probe_address("daemon-start");
        let listener = crate::transport::listen(&address).expect("listen");
        let (ended, has_ended) = std::sync::mpsc::channel();
        let served = {
            let address = address.clone();
            std::thread::spawn(move || {
                super::serve(
                    listener,
                    address,
                    Arc::new(silent_recorder()),
                    Arc::new(fake_runtime("x")),
                );
                let _ = ended.send(());
            })
        };

        // A live daemon accepts a connection, which is what start() checks for.
        assert!(crate::transport::connect(&address).is_ok());

        // Stop it through the wire request, which is the only sender of `stop`.
        let mut client =
            std::io::BufReader::new(crate::transport::connect(&address).expect("connect"));
        Request {
            command: "stop".into(),
            entrypoint: None,
            context: vec![],
        }
        .write_to(client.get_mut())
        .expect("write");
        let reply = Reply::read_from(&mut client).expect("reply");
        assert!(matches!(reply, Reply::Ok(_)), "got {reply:?}");

        // A bounded wait, so a loop that will not stop fails this test in seconds
        // instead of sitting until the CI job's cap kills the whole run.
        has_ended
            .recv_timeout(std::time::Duration::from_secs(10))
            .expect("the accept loop must end after a stop request");
        served.join().expect("the loop must end");
    }
}
