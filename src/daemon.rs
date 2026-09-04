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

use crate::bias;
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

/// Everything resolved once, at daemon start, and needed everywhere a take can
/// finish.
pub struct Runtime {
    pub recognition: Recognition,
    /// `[context] source`, checked once — the same shape `recognition` has, for
    /// the same reason: a configuration value that can be wrong is validated at
    /// start and the `Result` consulted per take, never re-parsed
    /// (`tasks/21/DESIGN_21.md`, section 2a, "why resolved once, not per take").
    pub bias_source: Result<bias::Source, String>,
    /// Where the known agent keeps its transcripts. Absent when the
    /// environment names no home directory, which leaves the transcript source
    /// nowhere to look — an ordinary miss, not a failure (design section 3).
    pub transcript_root: Option<std::path::PathBuf>,
    /// `[context]`'s three numeric keys, for the per-take bias string. Read
    /// once with the rest of the configuration, never per take.
    pub context: config::Context,
    /// The herdr binary the pane source runs, resolved once from
    /// `HERDR_BIN_PATH` the same way delivery resolves it.
    pub herdr_binary: String,
    pub deliverer: Box<dyn crate::delivery::Deliverer>,
    pub delivery_settings: crate::delivery::Settings,
    pub journal: Box<dyn Journal>,
    /// The rewrite engine the configuration resolved to at start, or why none
    /// is available. Resolved once, the same reason `recognition` is
    /// (`tasks/36/DESIGN_36.md`, section 1).
    pub rewrite: crate::rewrite::Resolution,
    /// `[rewrite] skip_if_plain`, read once with the rest of the
    /// configuration.
    pub skip_if_plain: bool,
    /// Whether the "rewrite unavailable" notice has already been given once
    /// in this daemon's lifetime — `Relaxed` is enough: the worst case under
    /// concurrent takes is two notices instead of one, not a correctness
    /// failure (`tasks/36/DESIGN_36.md`, section 3).
    pub told: std::sync::atomic::AtomicBool,
}

/// The two context fields `Runtime` holds, resolved from the loaded
/// configuration and the environment. Separate from `start()` so that the
/// resolution can be tested without binding a socket.
pub fn bias_settings(
    context: &config::Context,
    vars: &config::Vars,
) -> (Result<bias::Source, String>, Option<std::path::PathBuf>) {
    let source = bias::source::resolve(&context.source);
    let root = vars.home.as_ref().map(|home| {
        std::path::PathBuf::from(home)
            .join(".claude")
            .join("projects")
    });
    (source, root)
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
                        invocation.focused_pane_cwd.as_deref(),
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
///
/// `cwd` and `agent` are the target pane's working directory and the agent
/// running in it, read from the `Invocation` `answer` has already parsed — this
/// function never parses the invocation context itself.
fn dictate(
    recorder: &Recorder,
    runtime: &Runtime,
    pane: &str,
    cwd: Option<&str>,
    agent: Option<&str>,
) -> Reply {
    match recorder.start(pane, cwd, agent) {
        Started::Began => Reply::Ok(format!("recording for {pane}")),
        Started::CouldNotStart(why) => Reply::Error(why),
        Started::PreviousFailure(why) => Reply::Error(why),
        Started::AlreadyRunning => match recorder.stop() {
            Err(why) => Reply::Error(why.to_string()),
            Ok(take) => {
                // Assembled here, where the take has just finished and
                // recognition is about to run on it. The collected string is
                // reported on without being written down
                // (`tasks/21/DESIGN_21.md`, section 9), then handed both to
                // recognition (issue #26) and to the rewrite step between
                // recognition and delivery (issue #36).
                let collected = take_bias(
                    runtime,
                    &take.target,
                    take.cwd.as_deref(),
                    take.agent.as_deref(),
                );
                transcribe(runtime, &take, &collected.bias)
            }
        },
    }
}

/// Assembles this take's bias string and writes one line about it — counts and
/// flags only, never the string (AC-9, design section 8).
///
/// Returns the `Collected` so a test can assert on what was assembled while the
/// log line is checked for what it must not contain. Never fails: an
/// unresolved `[context] source` and a miss on every source both leave the take
/// running on whatever was found (design section 2a).
fn take_bias(
    runtime: &Runtime,
    pane: &str,
    cwd: Option<&str>,
    agent: Option<&str>,
) -> bias::Collected {
    let context = &runtime.context;
    // An absent working directory is not a directory to look in: the
    // conversation sources still run, the file-names component comes back
    // empty.
    let cwd = cwd.unwrap_or_default();
    match &runtime.bias_source {
        Ok(source) => {
            let collected = bias::collect(bias::CollectInput {
                source: *source,
                cwd,
                agent,
                pane,
                // No home directory means no transcript root, and an empty
                // root matches no project directory — so the transcript
                // source misses, which is an outcome this path already
                // handles (design section 3).
                transcript_root: runtime
                    .transcript_root
                    .as_deref()
                    .unwrap_or_else(|| std::path::Path::new("")),
                herdr_binary: &runtime.herdr_binary,
                conversation_turns: context.conversation_turns,
                file_names: context.file_names,
                prompt_chars: context.prompt_chars,
            });
            runtime
                .journal
                .write(&bias_line(&collected, context.prompt_chars));
            collected
        }
        Err(why) => {
            let collected = files_only(cwd, context);
            runtime
                .journal
                .write(&bias_refused_line(why, &collected, context.prompt_chars));
            collected
        }
    }
}

/// The bias string an unresolved `[context] source` still gets: file names
/// alone, capped the same way, with no conversation component and nothing
/// attempted — collecting file names does not depend on `source` (design
/// section 2a, "the refusal's effect on the take").
fn files_only(cwd: &str, context: &config::Context) -> bias::Collected {
    let names = bias::files::collect(cwd, context.file_names);
    let file_line = names.join(" ");
    let raw = format!("{file_line}\n");
    let raw_len = raw.chars().count();
    bias::Collected {
        bias: raw.chars().take(context.prompt_chars).collect(),
        attempted: Vec::new(),
        file_count: names.len(),
        file_chars: file_line.chars().count(),
        conversation_chars: 0,
        truncated: raw_len > context.prompt_chars,
        // No source was tried, so there is no failure to report.
        pane_error: None,
    }
}

/// The counts and flags a log line may carry about a bias string. Deliberately
/// everything `Collected` holds except `bias` itself.
fn bias_counts(collected: &bias::Collected, prompt_chars: usize) -> String {
    let attempted = if collected.attempted.is_empty() {
        "-".to_string()
    } else {
        collected
            .attempted
            .iter()
            .map(|(source, found)| {
                let name = match source {
                    bias::Source::Transcript => "transcript",
                    bias::Source::Pane => "pane",
                    // `attempted` never carries Auto — it names the mode
                    // `collect` ran under, not a call it made (design 2a).
                    bias::Source::Auto => "auto",
                };
                format!("{name}:{}", if *found { "hit" } else { "miss" })
            })
            .collect::<Vec<_>>()
            .join(",")
    };
    // The reason a pane read failed is not the bias string: it names the
    // program and the exit code, which is the only thing that says what to
    // fix. Newlines are collapsed because a journal line is one line.
    let why = match &collected.pane_error {
        Some(why) => format!(" pane_error={:?}", why.replace('\n', " ")),
        None => String::new(),
    };
    format!(
        "attempted={attempted} file_count={} file_chars={} conversation_chars={} \
         prompt_chars={prompt_chars} truncated={}{why}",
        collected.file_count,
        collected.file_chars,
        collected.conversation_chars,
        collected.truncated
    )
}

/// Written once per take, on the channel `request_line` and `context_note`
/// already write to. It never contains `Collected.bias` — on a hit or a miss.
pub fn bias_line(collected: &bias::Collected, prompt_chars: usize) -> String {
    format!("bias {}", bias_counts(collected, prompt_chars))
}

/// Written once per take instead of `bias_line` when `[context] source` never
/// resolved. It names the value that was refused and the three that are
/// accepted, since that is what somebody has to fix.
pub fn bias_refused_line(why: &str, collected: &bias::Collected, prompt_chars: usize) -> String {
    format!(
        "bias unavailable: {why}; the take runs on file names alone — {}",
        bias_counts(collected, prompt_chars)
    )
}

/// A finished take becomes text, and the text is delivered — or, if either
/// step fails, the reply is a Reply::Error naming why and what to do next
/// (client::outcome maps Reply::Ok to exit 0, Reply::Error to exit 1).
fn transcribe(runtime: &Runtime, take: &crate::capture::Take, bias: &str) -> Reply {
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
    let text = match engine.transcribe(&take.path, bias) {
        Ok(text) => text,
        Err(why) => {
            return Reply::Error(format!(
                "{why} — the take is kept at {}",
                take.path.display()
            ))
        }
    };

    // Between recognition producing `text` and delivery: `off` (never
    // represented here — see `Resolution::Off`) and a working engine both
    // leave the take on the reply path below unaffected either way; only a
    // configured engine that is unavailable, or one that fails, ever calls
    // `tell_once` (`tasks/36/DESIGN_36.md`, section 2).
    let text = match &runtime.rewrite {
        crate::rewrite::Resolution::Off => text,
        crate::rewrite::Resolution::Unavailable(why) => {
            tell_once(runtime, why);
            text
        }
        crate::rewrite::Resolution::Engine(engine) => {
            if crate::rewrite::skip::plain(&text, bias, runtime.skip_if_plain) {
                text
            } else {
                match engine.rewrite(&text, bias) {
                    Ok(rewritten) => rewritten,
                    Err(why) => {
                        tell_once(runtime, &why.to_string());
                        text
                    }
                }
            }
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
            // Whether a pane id or a path can carry a newline was never
            // established either way, and both sit ahead of the transcript in
            // the reply below — collapsing them costs nothing and defends the
            // reply regardless.
            let target = take.target.replace('\n', " ");
            let path = take.path.display().to_string().replace('\n', " ");
            let why = why.to_string().replace('\n', " ");
            runtime.journal.write(&delivery_failed_line(&target, &why));
            if runtime.delivery_settings.toasts {
                // A toast that could not be shown must not stop the journal
                // line or the client's reply from getting through — but its
                // own failure is still a lost diagnostic, so it is journaled.
                if let Err(toast_why) = runtime.deliverer.notify(
                    "Delivery failed",
                    &format!("{target}: the text is in the plugin log"),
                ) {
                    runtime.journal.write(&toast_failed_line(
                        &toast_why.to_string().replace('\n', " "),
                    ));
                }
            }
            Reply::Error(format!(
                "could not deliver to {target} ({why}) — the take is kept at {path}; text: {}",
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

/// Written when no rewrite engine is available to run this take through —
/// `Resolution::Unavailable`, or a live `Resolution::Engine` call that
/// failed. Never written for `Resolution::Off`, which is not "unavailable",
/// it is turned off.
fn rewrite_unavailable_line(why: &str) -> String {
    format!("rewrite unavailable: {}", why.replace('\n', " "))
}

/// Tells the person once per daemon lifetime that no rewrite engine ran this
/// take, reusing the same journal-line-plus-toast shape the failed-delivery
/// path above already uses (`Deliverer::notify`, no new mechanism). `Relaxed`
/// on the swap is enough: the worst case under two takes finishing at nearly
/// the same moment on different connections is two notices instead of one, a
/// cosmetic risk, not a correctness one (`tasks/36/DESIGN_36.md`, section 3).
fn tell_once(runtime: &Runtime, why: &str) {
    if runtime.told.swap(true, Ordering::Relaxed) {
        return;
    }
    runtime.journal.write(&rewrite_unavailable_line(why));
    if runtime.delivery_settings.toasts {
        if let Err(toast_why) = runtime.deliverer.notify("Rewrite unavailable", why) {
            runtime.journal.write(&toast_failed_line(
                &toast_why.to_string().replace('\n', " "),
            ));
        }
    }
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
    let vars = config::Vars::from_env();
    let loaded = config::load(config::directory(&vars).as_deref());
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

    // Not fatal either: an unrecognised `[context] source` costs the take its
    // conversation component, never the recording (design section 2a, "the
    // refusal's effect on the take").
    let (bias_source, transcript_root) = bias_settings(&loaded.config.context, &vars);
    if let Err(why) = &bias_source {
        eprintln!("{why}");
    }

    let rewrite = crate::rewrite::resolve(&loaded.config.rewrite);

    let runtime = Runtime {
        recognition,
        bias_source,
        transcript_root,
        context: loaded.config.context,
        herdr_binary: crate::delivery::herdr_binary(),
        deliverer: Box::new(crate::delivery::HerdrDeliverer::new()),
        delivery_settings: crate::delivery::Settings {
            submit: loaded.config.delivery.submit,
            toasts: loaded.config.ui.toasts,
        },
        journal: Box::new(StderrJournal),
        rewrite,
        skip_if_plain: loaded.config.rewrite.skip_if_plain,
        told: std::sync::atomic::AtomicBool::new(false),
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
            bias_source: Ok(bias::Source::Auto),
            transcript_root: None,
            context: crate::config::Context::default(),
            herdr_binary: MISSING_HERDR.to_string(),
            deliverer: Box::new(crate::delivery::tests_support::FakeDeliverer::ok()),
            delivery_settings: crate::delivery::Settings {
                submit: false,
                toasts: false,
            },
            journal: Box::new(StderrJournal),
            rewrite: crate::rewrite::Resolution::Off,
            skip_if_plain: true,
            told: std::sync::atomic::AtomicBool::new(false),
        }
    }

    #[test]
    fn the_bias_source_and_the_transcript_root_are_resolved_once_at_start() {
        let home = std::env::temp_dir();
        let vars = crate::config::Vars {
            config_dir: None,
            xdg_config_home: None,
            home: Some(home.to_string_lossy().into_owned()),
        };

        let named = crate::config::Context {
            source: "pane".to_string(),
            ..Default::default()
        };
        let (source, root) = bias_settings(&named, &vars);
        let mut runtime = fake_runtime("x");
        runtime.bias_source = source;
        runtime.transcript_root = root;
        assert_eq!(runtime.bias_source, Ok(bias::Source::Pane));
        assert_eq!(
            runtime.transcript_root,
            Some(home.join(".claude").join("projects"))
        );

        let refused = crate::config::Context {
            source: "vosk".to_string(),
            ..Default::default()
        };
        let (why, _) = bias_settings(&refused, &vars);
        let why = why.expect_err("an unrecognised source must be refused");
        assert!(why.contains("vosk"), "got {why:?}");

        // No home, no root: the transcript source has nowhere to look, which
        // section 3 of the design treats as an ordinary miss.
        let homeless = crate::config::Vars {
            config_dir: None,
            xdg_config_home: None,
            home: None,
        };
        let (_, absent) = bias_settings(&crate::config::Context::default(), &homeless);
        assert_eq!(absent, None);
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
            bias_source: Ok(bias::Source::Auto),
            transcript_root: None,
            context: crate::config::Context::default(),
            herdr_binary: MISSING_HERDR.to_string(),
            deliverer: Box::new(deliverer),
            delivery_settings: crate::delivery::Settings {
                submit,
                toasts: false,
            },
            journal: Box::new(StderrJournal),
            rewrite: crate::rewrite::Resolution::Off,
            skip_if_plain: true,
            told: std::sync::atomic::AtomicBool::new(false),
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

    #[test]
    fn the_failure_reply_survives_one_read_line_when_the_transcript_and_the_reason_carry_newlines()
    {
        let recorder = tone_recorder("newlines");
        let fake = crate::delivery::tests_support::FakeDeliverer::failing(
            crate::delivery::DeliveryError::Rejected("pane\nnot\nfound".into()),
        );
        let runtime = Runtime {
            recognition: Ok(Box::new(crate::stt::tests_support::Fake(Ok(
                "line one\nline two".to_string(),
            )))),
            bias_source: Ok(bias::Source::Auto),
            transcript_root: None,
            context: crate::config::Context::default(),
            herdr_binary: MISSING_HERDR.to_string(),
            deliverer: Box::new(fake),
            delivery_settings: crate::delivery::Settings {
                submit: false,
                toasts: false,
            },
            journal: Box::new(StderrJournal),
            rewrite: crate::rewrite::Resolution::Off,
            skip_if_plain: true,
            told: std::sync::atomic::AtomicBool::new(false),
        };
        let request = dictate_request();
        answer(&request, &recorder, &runtime);
        let (reply, _) = answer(&request, &recorder, &runtime);

        // No embedded newline may reach the wire: writeln! adds exactly one
        // trailing '\n', and if either collapse were dropped, an embedded one
        // would create a second line the protocol's single read_line can never
        // see.
        let mut buffer = Vec::new();
        reply.write_to(&mut buffer).expect("write");
        assert_eq!(
            buffer.iter().filter(|&&b| b == b'\n').count(),
            1,
            "the frame must carry exactly one newline, got {:?}",
            String::from_utf8_lossy(&buffer)
        );

        let mut reader = std::io::BufReader::new(&buffer[..]);
        let round_tripped = Reply::read_from(&mut reader).expect("read");
        let text = match round_tripped {
            Reply::Error(text) => text,
            other => panic!("expected Reply::Error, got {other:?}"),
        };
        assert!(text.contains("line one line two"), "got {text:?}");
        assert!(text.contains("pane not found"), "got {text:?}");
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

    #[test]
    fn the_pane_and_the_path_are_collapsed_too_although_they_precede_the_transcript() {
        // Whether herdr can hand back a pane id with a newline in it is not
        // established either way; the cost of defending against it is nothing,
        // so both it and the take's path (also ahead of the transcript) are
        // collapsed the same way the reason and the transcript already are.
        let take = crate::capture::Take {
            path: std::path::PathBuf::from("/tmp/oddly\nnamed.wav"),
            level_dbfs: -10.0,
            target: "w1\n:p2".to_string(),
            agent: None,
            cwd: None,
        };
        let fake = crate::delivery::tests_support::FakeDeliverer::failing(
            crate::delivery::DeliveryError::Rejected("pane_not_found".into()),
        );
        let runtime = runtime_with(fake, false);
        let reply = transcribe(&runtime, &take, "");
        let text = match reply {
            Reply::Error(text) => text,
            other => panic!("expected Reply::Error, got {other:?}"),
        };
        assert!(!text.contains('\n'), "got {text:?}");
        assert!(text.contains("w1 :p2"), "got {text:?}");
        assert!(text.contains("/tmp/oddly named.wav"), "got {text:?}");
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

        // Three lines now: the take's bias line comes first, before anything
        // is transcribed or delivered. Its own content is asserted by
        // `the_bias_line_carries_counts_and_never_the_conversation_it_was_built_from`.
        let lines = journal.0.lock().unwrap();
        assert_eq!(lines.len(), 3, "got {lines:?}");
        assert!(lines[0].starts_with("bias "), "got {lines:?}");
        assert!(lines[1].contains("fix the worklog entry"), "got {lines:?}");
        assert!(
            lines[2].contains("w1:p2") && lines[2].contains("pane_not_found"),
            "got {lines:?}"
        );
    }

    /// A journal that pushes into a trace shared with a deliverer, so a test
    /// can see the two interleaved rather than only each one's own order.
    struct TracingJournal(std::sync::Arc<std::sync::Mutex<Vec<String>>>);
    impl Journal for TracingJournal {
        fn write(&self, line: &str) {
            self.0.lock().unwrap().push(format!("journal:{line}"));
        }
    }

    /// Wraps a `FakeDeliverer`, recording each call into the same trace a
    /// `TracingJournal` writes to, before delegating to the fake's own
    /// behavior (result and call log). This is what lets a test tell "the
    /// journal line was written" apart from "the journal line was written
    /// before delivery was attempted" — asserting only the two journal lines'
    /// order relative to each other proves neither, since both are written
    /// only after `deliver` returns.
    struct TracingDeliverer {
        trace: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
        inner: crate::delivery::tests_support::FakeDeliverer,
    }
    impl crate::delivery::Deliverer for TracingDeliverer {
        fn insert(&self, pane: &str, text: &str) -> Result<(), crate::delivery::DeliveryError> {
            self.trace
                .lock()
                .unwrap()
                .push(format!("deliver:insert:{pane}"));
            self.inner.insert(pane, text)
        }
        fn submit(&self, pane: &str, text: &str) -> Result<(), crate::delivery::DeliveryError> {
            self.trace
                .lock()
                .unwrap()
                .push(format!("deliver:submit:{pane}"));
            self.inner.submit(pane, text)
        }
        fn notify(&self, title: &str, body: &str) -> Result<(), crate::delivery::DeliveryError> {
            self.trace
                .lock()
                .unwrap()
                .push("deliver:notify".to_string());
            self.inner.notify(title, body)
        }
    }

    #[test]
    fn the_delivering_line_is_written_before_delivery_is_attempted_not_merely_before_the_failure_line(
    ) {
        let recorder = tone_recorder("journal-pinned");
        let trace = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let deliverer = TracingDeliverer {
            trace: std::sync::Arc::clone(&trace),
            inner: crate::delivery::tests_support::FakeDeliverer::failing(
                crate::delivery::DeliveryError::Rejected("pane_not_found".into()),
            ),
        };
        let mut runtime = runtime_with(crate::delivery::tests_support::FakeDeliverer::ok(), false);
        runtime.deliverer = Box::new(deliverer);
        runtime.journal = Box::new(TracingJournal(std::sync::Arc::clone(&trace)));
        let request = dictate_request();
        answer(&request, &recorder, &runtime);
        answer(&request, &recorder, &runtime);

        // The take's bias line is written on this journal too, ahead of
        // everything here; this test is about the delivering line's position
        // relative to the delivery attempt, so the bias line is filtered out
        // rather than pinned into the expected sequence.
        let trace: Vec<String> = trace
            .lock()
            .unwrap()
            .iter()
            .filter(|line| !line.starts_with("journal:bias"))
            .cloned()
            .collect();
        assert_eq!(
            trace,
            vec![
                "journal:delivering: fix the worklog entry".to_string(),
                "deliver:insert:w1:p2".to_string(),
                "journal:delivery failed: pane=w1:p2 reason=pane_not_found".to_string(),
            ],
            "the delivering line must precede the delivery attempt itself, not just the failure line"
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
        assert_eq!(
            lines.len(),
            4,
            "the toast's own failure must be journaled too, got {lines:?}"
        );
        assert!(lines[0].starts_with("bias "), "got {lines:?}");
        assert!(lines[1].contains("fix the worklog entry"), "got {lines:?}");
        assert!(
            lines[2].contains("w1:p2") && lines[2].contains("pane_not_found"),
            "got {lines:?}"
        );
        assert!(lines[3].contains("toast"), "got {lines:?}");
    }

    #[test]
    fn a_working_rewrite_engine_changes_the_delivered_text() {
        let fake = crate::delivery::tests_support::FakeDeliverer::ok();
        let mut runtime = runtime_with(fake.clone(), false);
        runtime.rewrite =
            crate::rewrite::Resolution::Engine(Box::new(crate::rewrite::tests_support::Fake(Ok(
                "rewritten text with plenty of words so the skip heuristic never applies here"
                    .to_string(),
            ))));
        let recorder = tone_recorder("rewrite-hit");
        let request = dictate_request();
        answer(&request, &recorder, &runtime);
        answer(&request, &recorder, &runtime);

        assert!(
            fake.calls().iter().any(|c| matches!(
                c,
                crate::delivery::tests_support::Call::Insert(pane, text)
                    if pane == "w1:p2"
                        && text == "rewritten text with plenty of words so the skip heuristic never applies here"
            )),
            "got {:?}",
            fake.calls()
        );
    }

    #[test]
    fn a_failed_rewrite_engine_delivers_the_original_text_and_tells_once() {
        let journal = std::sync::Arc::new(RecordingJournal::default());
        let fake = crate::delivery::tests_support::FakeDeliverer::ok();
        let mut runtime = runtime_with(fake.clone(), false);
        runtime.rewrite = crate::rewrite::Resolution::Engine(Box::new(
            crate::rewrite::tests_support::Fake(Err("engine refused the connection".to_string())),
        ));
        runtime.journal = Box::new(TestJournal(std::sync::Arc::clone(&journal)));
        let recorder = tone_recorder("rewrite-fail");

        // Two takes; the skip heuristic does not matter here since the canned
        // recognition text is short and plain — a short circuit still delivers
        // the same unrewritten text this test asserts on either way.
        let request = dictate_request();
        answer(&request, &recorder, &runtime);
        answer(&request, &recorder, &runtime);
        answer(&request, &recorder, &runtime);
        answer(&request, &recorder, &runtime);

        assert!(
            fake.calls().iter().all(|c| !matches!(
                c,
                crate::delivery::tests_support::Call::Insert(_, text) if text != "fix the worklog entry"
            )),
            "got {:?}",
            fake.calls()
        );

        let lines = journal.0.lock().unwrap();
        let notices = lines
            .iter()
            .filter(|l| l.contains("rewrite unavailable"))
            .count();
        assert_eq!(notices, 1, "got {lines:?}");
    }

    #[test]
    fn an_unavailable_rewrite_resolution_delivers_the_original_text_and_tells_once() {
        let journal = std::sync::Arc::new(RecordingJournal::default());
        let fake = crate::delivery::tests_support::FakeDeliverer::ok();
        let mut runtime = runtime_with(fake.clone(), false);
        runtime.rewrite = crate::rewrite::Resolution::Unavailable("agent not invoked".to_string());
        runtime.journal = Box::new(TestJournal(std::sync::Arc::clone(&journal)));
        let recorder = tone_recorder("rewrite-unavailable");
        let request = dictate_request();
        answer(&request, &recorder, &runtime);
        answer(&request, &recorder, &runtime);
        answer(&request, &recorder, &runtime);
        answer(&request, &recorder, &runtime);

        assert!(
            fake.calls().iter().any(|c| matches!(
                c,
                crate::delivery::tests_support::Call::Insert(_, text) if text == "fix the worklog entry"
            )),
            "got {:?}",
            fake.calls()
        );

        // Distinguishes Unavailable from Off: Unavailable tells once, Off never
        // tells at all (see resolution_off_never_tells below). Two takes, one
        // notice — the same shape a_failed_rewrite_engine_delivers_the_original_text_and_tells_once
        // proves.
        let lines = journal.0.lock().unwrap();
        let notices = lines
            .iter()
            .filter(|l| l.contains("rewrite unavailable"))
            .count();
        assert_eq!(notices, 1, "got {lines:?}");
    }

    #[test]
    fn resolution_off_never_tells() {
        let journal = std::sync::Arc::new(RecordingJournal::default());
        let fake = crate::delivery::tests_support::FakeDeliverer::ok();
        let mut runtime = runtime_with(fake.clone(), false);
        runtime.rewrite = crate::rewrite::Resolution::Off;
        runtime.journal = Box::new(TestJournal(std::sync::Arc::clone(&journal)));
        let recorder = tone_recorder("rewrite-off");
        let request = dictate_request();
        answer(&request, &recorder, &runtime);
        answer(&request, &recorder, &runtime);

        assert!(
            fake.calls().iter().any(|c| matches!(
                c,
                crate::delivery::tests_support::Call::Insert(_, text) if text == "fix the worklog entry"
            )),
            "got {:?}",
            fake.calls()
        );

        // The distinguishing assertion versus the Unavailable test above: Off
        // produces zero notices, not one.
        let lines = journal.0.lock().unwrap();
        assert!(
            !lines.iter().any(|l| l.contains("rewrite unavailable")),
            "Off must never tell, got {lines:?}"
        );
    }

    #[test]
    fn a_short_plain_transcript_with_a_configured_engine_still_skips_it() {
        // Every other Resolution::Engine test above uses either a long
        // string ("...so the skip heuristic never applies here") or the
        // canned "fix the worklog entry", which is all-Latin and so fails
        // has_latin_run before the wiring is even reached — neither could
        // catch a reversed or missing skip check. "открой файл" has no Latin
        // letters, is two words (well under the 8-word limit), and shares no
        // word with an empty bias string — it genuinely satisfies all three
        // of rewrite::skip::plain's checks (src/rewrite/skip.rs).
        let fake = crate::delivery::tests_support::FakeDeliverer::ok();
        let mut runtime = runtime_with(fake.clone(), false);
        runtime.recognition = Ok(Box::new(crate::stt::tests_support::Fake(Ok(
            "открой файл".to_string(),
        ))));
        // Bypasses the pane/transcript sources entirely so the bias string
        // handed to the skip check is deterministically empty, not whatever
        // a live-herdr-less Auto attempt happens to collect.
        runtime.bias_source = Err("no pane source in this test".to_string());
        // A Fake that WOULD change the delivered text if it were ever
        // called — proving the skip really did bypass the engine, not just
        // that plain() returns true in isolation.
        runtime.rewrite = crate::rewrite::Resolution::Engine(Box::new(
            crate::rewrite::tests_support::Fake(Ok("this must never be delivered".to_string())),
        ));
        let recorder = tone_recorder("skip-with-engine");
        let request = dictate_request();
        answer(&request, &recorder, &runtime);
        answer(&request, &recorder, &runtime);

        assert_eq!(
            fake.calls(),
            vec![crate::delivery::tests_support::Call::Insert(
                "w1:p2".into(),
                "открой файл".into()
            )]
        );
    }

    /// A program that is certainly not there, so `bias::pane::read` misses
    /// without a live herdr — the same fixture `bias`'s own tests use.
    const MISSING_HERDR: &str = "/definitely/not/a/real/herdr-binary";

    fn bias_scratch(tag: &str) -> std::path::PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!("herdr-voice-daemon-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("create scratch");
        path
    }

    /// The one file in the scratch repository below. Distinctive enough that a
    /// log line carrying the bias string can be caught doing it.
    const REPOSITORY_FILE: &str = "kettlehouse.txt";

    /// A scratch git repository, so the file-names component finds something
    /// real without depending on the checkout the tests run in.
    fn git_repo(dir: &std::path::Path) {
        for args in [
            vec!["init", "-q"],
            vec!["config", "user.email", "test@example.invalid"],
            vec!["config", "user.name", "Test"],
        ] {
            let status = std::process::Command::new("git")
                .current_dir(dir)
                .args(&args)
                .status()
                .expect("run git");
            assert!(status.success(), "git {args:?} failed");
        }
        std::fs::write(dir.join(REPOSITORY_FILE), "content").unwrap();
        for args in [vec!["add", "."], vec!["commit", "-q", "-m", "add a file"]] {
            let status = std::process::Command::new("git")
                .current_dir(dir)
                .args(&args)
                .status()
                .expect("run git");
            assert!(status.success(), "git {args:?} failed");
        }
    }

    /// A transcript root holding one turn, under the project directory `cwd`
    /// slugifies to — the derivation `bias::transcript::find` performs.
    fn transcript_fixture(root: &std::path::Path, cwd: &str, text: &str) {
        let slug: String = cwd
            .chars()
            .map(|c| {
                if c == '/' || c == '.' || c == '@' {
                    '-'
                } else {
                    c
                }
            })
            .collect();
        let project = root.join(slug);
        std::fs::create_dir_all(&project).unwrap();
        std::fs::write(
            project.join("session.jsonl"),
            format!(r#"{{"type":"user","message":{{"content":{text:?}}}}}"#),
        )
        .unwrap();
    }

    fn cwd_request(cwd: &std::path::Path) -> Request {
        pane_request("w1:p2", cwd)
    }

    fn pane_request(pane: &str, cwd: &std::path::Path) -> Request {
        request(
            "dictate",
            format!(
                r#"{{"focused_pane_id":{pane:?},"focused_pane_cwd":{:?},"focused_pane_agent":"claude"}}"#,
                cwd.to_string_lossy()
            )
            .as_bytes(),
        )
    }

    #[test]
    fn the_bias_line_carries_counts_and_never_the_conversation_it_was_built_from() {
        // AC-9's proof, as `tasks/21/DESIGN_21.md` section 11 asks for it: a
        // positive assertion that logging happened is not evidence the string
        // was left out of it.
        const SENTENCE: &str = "the kettle argues with the lighthouse about tuesday";
        let cwd = bias_scratch("bias-log-cwd");
        git_repo(&cwd);
        let root = bias_scratch("bias-log-root");
        transcript_fixture(&root, &cwd.to_string_lossy(), SENTENCE);

        let journal = std::sync::Arc::new(RecordingJournal::default());
        let mut runtime = runtime_with(crate::delivery::tests_support::FakeDeliverer::ok(), false);
        runtime.bias_source = Ok(bias::Source::Auto);
        runtime.transcript_root = Some(root);
        runtime.journal = Box::new(TestJournal(std::sync::Arc::clone(&journal)));

        let recorder = tone_recorder("bias-log");
        let request = cwd_request(&cwd);
        answer(&request, &recorder, &runtime);
        answer(&request, &recorder, &runtime);

        let lines = journal.0.lock().unwrap();
        let line = lines
            .iter()
            .find(|line| line.starts_with("bias "))
            .unwrap_or_else(|| panic!("no bias line, got {lines:?}"));
        assert!(line.contains("attempted=transcript:hit"), "got {line:?}");
        assert!(line.contains("file_chars="), "got {line:?}");
        assert!(line.contains("conversation_chars="), "got {line:?}");
        assert!(line.contains("prompt_chars=600"), "got {line:?}");
        assert!(line.contains("truncated="), "got {line:?}");
        assert!(
            !lines.iter().any(|line| line.contains(SENTENCE)),
            "the bias string must never reach the log, got {lines:?}"
        );
    }

    #[test]
    fn an_unresolved_source_logs_the_configuration_error_and_still_biases_on_file_names() {
        let cwd = bias_scratch("bias-refused-cwd");
        git_repo(&cwd);

        let journal = std::sync::Arc::new(RecordingJournal::default());
        let mut runtime = fake_runtime("x");
        runtime.bias_source = crate::bias::source::resolve("vosk");
        runtime.journal = Box::new(TestJournal(std::sync::Arc::clone(&journal)));

        let collected = take_bias(
            &runtime,
            "w1:p2",
            Some(&cwd.to_string_lossy()),
            Some("claude"),
        );
        assert!(collected.attempted.is_empty(), "got {collected:?}");
        assert_eq!(collected.conversation_chars, 0);
        assert!(collected.file_count > 0, "got {collected:?}");
        assert!(
            collected.bias.contains(REPOSITORY_FILE),
            "got {collected:?}"
        );

        let lines = journal.0.lock().unwrap();
        assert_eq!(lines.len(), 1, "got {lines:?}");
        assert!(lines[0].contains("vosk"), "got {lines:?}");
        assert!(lines[0].contains("file_count="), "got {lines:?}");
        // AC-9 on this path too: the refusal's line carries a `Collected`
        // whose `bias` holds the file names, and must still print none of it.
        assert!(
            !lines[0].contains(REPOSITORY_FILE),
            "the bias string must never reach the log, got {lines:?}"
        );
    }

    #[test]
    fn the_files_only_bias_is_capped_at_prompt_chars_too() {
        let cwd = bias_scratch("bias-refused-cap-cwd");
        git_repo(&cwd);

        let mut runtime = fake_runtime("x");
        runtime.bias_source = crate::bias::source::resolve("vosk");
        runtime.context.prompt_chars = 8;

        let collected = take_bias(
            &runtime,
            "w1:p2",
            Some(&cwd.to_string_lossy()),
            Some("claude"),
        );
        assert!(collected.file_count > 0, "got {collected:?}");
        assert_eq!(collected.bias.chars().count(), 8, "got {collected:?}");
        assert!(collected.truncated, "got {collected:?}");
    }

    #[test]
    fn the_bias_is_built_from_the_pane_the_take_was_pinned_to() {
        // The same rule `Take::target` exists for: somebody speaks looking at
        // one agent and switches while thinking. The text goes to the pane the
        // take began in, and so must the context it is recognised with.
        const SENTENCE: &str = "the kettle argues with the lighthouse about tuesday";
        let pinned = bias_scratch("bias-pinned-cwd");
        git_repo(&pinned);
        let switched = bias_scratch("bias-switched-cwd");
        let root = bias_scratch("bias-pinned-root");
        transcript_fixture(&root, &pinned.to_string_lossy(), SENTENCE);

        let journal = std::sync::Arc::new(RecordingJournal::default());
        let mut runtime = runtime_with(crate::delivery::tests_support::FakeDeliverer::ok(), false);
        runtime.bias_source = Ok(bias::Source::Auto);
        runtime.transcript_root = Some(root);
        runtime.journal = Box::new(TestJournal(std::sync::Arc::clone(&journal)));

        let recorder = tone_recorder("bias-pinned");
        // The take begins in one pane, and the focus has moved by the time the
        // second keypress arrives.
        answer(&pane_request("w1:p2", &pinned), &recorder, &runtime);
        answer(&pane_request("w9:p9", &switched), &recorder, &runtime);

        let lines = journal.0.lock().unwrap();
        let line = lines
            .iter()
            .find(|line| line.starts_with("bias "))
            .unwrap_or_else(|| panic!("no bias line, got {lines:?}"));
        assert!(
            line.contains("attempted=transcript:hit"),
            "the bias must come from the pinned working directory, got {line:?}"
        );
        assert!(
            !line.contains("file_count=0"),
            "the pinned working directory is a repository, got {line:?}"
        );
    }

    #[test]
    fn the_collected_bias_string_reaches_the_engine() {
        let cwd = bias_scratch("bias-to-engine-cwd");
        git_repo(&cwd);

        let (fake, received) =
            crate::stt::tests_support::CapturingFake::new(Ok("a transcript".to_string()));
        let mut runtime = runtime_with(crate::delivery::tests_support::FakeDeliverer::ok(), false);
        runtime.recognition = Ok(Box::new(fake));

        let recorder = tone_recorder("bias-to-engine");
        let request = pane_request("w1:p2", &cwd);
        answer(&request, &recorder, &runtime);
        answer(&request, &recorder, &runtime);

        let bias = received
            .lock()
            .unwrap()
            .clone()
            .expect("the engine was called");
        assert!(bias.contains(REPOSITORY_FILE), "got {bias:?}");
    }

    #[test]
    fn a_pane_read_that_fails_names_what_to_do_next_in_the_journal() {
        let recorder = tone_recorder("bias-pane-why");
        let journal = std::sync::Arc::new(RecordingJournal::default());
        let mut runtime = runtime_with(crate::delivery::tests_support::FakeDeliverer::ok(), false);
        runtime.bias_source = Ok(bias::Source::Pane);
        runtime.journal = Box::new(TestJournal(std::sync::Arc::clone(&journal)));
        let request = dictate_request();
        answer(&request, &recorder, &runtime);
        answer(&request, &recorder, &runtime);

        let lines = journal.0.lock().unwrap();
        let line = lines
            .iter()
            .find(|line| line.starts_with("bias "))
            .unwrap_or_else(|| panic!("no bias line, got {lines:?}"));
        assert!(line.contains("attempted=pane:miss"), "got {line:?}");
        assert!(
            line.contains("HERDR_BIN_PATH"),
            "a failed pane read must name what to do next, got {line:?}"
        );
    }

    #[test]
    fn a_miss_on_every_source_still_lets_the_take_succeed() {
        let recorder = tone_recorder("bias-miss");
        let mut runtime = runtime_with(crate::delivery::tests_support::FakeDeliverer::ok(), false);
        runtime.bias_source = Ok(bias::Source::Auto);
        // An empty root and a program that is not there: both sources miss.
        runtime.transcript_root = Some(bias_scratch("bias-miss-root"));
        let request = dictate_request();
        answer(&request, &recorder, &runtime);
        let (reply, _) = answer(&request, &recorder, &runtime);
        assert!(matches!(reply, Reply::Ok(_)), "got {reply:?}");
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
