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

/// What this daemon's take is doing, for display and for nothing else.
///
/// Written by the take path at every stage it enters, read by the drawing
/// thread. Nothing decides anything from it, so a stale or missed update costs
/// a wrong label for one interval and never a wrong take
/// (`tasks/40/DESIGN_40.md`, section 1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Activity {
    /// No take of this daemon's is running.
    Idle,
    /// A recording is open. `since` is stamped from `Runtime.clock`, so elapsed
    /// time is measured against that clock and no other.
    Recording {
        target: String,
        tab: Option<String>,
        since: crate::ptt::Stamp,
    },
    /// The recording is over and the take is in the pipeline.
    Working {
        target: String,
        tab: Option<String>,
        stage: Stage,
    },
}

/// The two pipeline stages worth displaying. Bias assembly and delivery are
/// not stages here: one takes fractions of a second, and the other announces
/// itself by the text appearing (`tasks/40/DESIGN_40.md`, section 10).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    Transcribing,
    Fixing,
}

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
    /// Where this mechanism's take is in its life — nothing, a device being
    /// opened, a key down, or a take in the pipeline. In memory on purpose: a
    /// file would bring back the truncated read the prototype lost recordings
    /// to. Four states rather than two because a recording exists for longer
    /// than a key is down (`crate::ptt::HoldState`).
    pub hold: std::sync::Mutex<crate::ptt::HoldState>,
    /// `[ptt]`, read once with the rest of the configuration. The watcher is
    /// what compares a hold against these two durations.
    pub ptt: crate::ptt::Settings,
    /// Behind an `Arc` because the watcher thread holds it too.
    pub clock: std::sync::Arc<dyn crate::ptt::Clock>,
    /// What the take path is doing, for the drawing thread to read. Display
    /// only: nothing in the take path reads it back.
    pub activity: std::sync::Mutex<Activity>,
    /// `[ui]`, read once with the rest of the configuration. The drawing thread
    /// reads all three of its indicator keys from here.
    ///
    /// `toasts` therefore exists in two places, here and on
    /// `delivery_settings`, which is where delivery reads it today. The
    /// duplication is deliberate and bounded: delivery goes on reading what it
    /// already reads, and unifying the two belongs to whoever next touches
    /// `delivery::Settings`.
    pub ui: crate::config::Ui,
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
        command if needs_target_pane(command) => {
            // A repeat that arrives is evidence the key is down, whatever its
            // payload turns out to say. The context answers where the take
            // goes, and that was settled when the hold began.
            let refreshed = if command == "ptt" {
                refresh_hold(runtime)
            } else {
                None
            };
            match context::parse(&request.context) {
                Err(why) => {
                    if let Some(target) = &refreshed {
                        runtime
                            .journal
                            .write(&unreadable_repeat_line(target, &why.to_string()));
                    }
                    (Reply::Error(why.to_string()), Control::Continue)
                }
                Ok(invocation) => match invocation.target_pane() {
                    None => {
                        if let Some(target) = &refreshed {
                            runtime.journal.write(&unreadable_repeat_line(
                                target,
                                "the invocation context names no focused pane",
                            ));
                        }
                        (
                            Reply::Error(
                                "the invocation context names no focused pane; \
                                 invoke this from a pane running an agent"
                                    .to_string(),
                            ),
                            Control::Continue,
                        )
                    }
                    Some(pane) if command == "dictate" => (
                        dictate(
                            recorder,
                            runtime,
                            pane,
                            invocation.focused_pane_cwd.as_deref(),
                            invocation.focused_pane_agent.as_deref(),
                            invocation.tab_id.as_deref(),
                        ),
                        Control::Continue,
                    ),
                    Some(pane) if command == "ptt" => (
                        ptt(
                            recorder,
                            runtime,
                            pane,
                            invocation.focused_pane_cwd.as_deref(),
                            invocation.focused_pane_agent.as_deref(),
                            invocation.tab_id.as_deref(),
                        ),
                        Control::Continue,
                    ),
                    Some(_) => (
                        Reply::Ok(format!("{command}: not implemented yet")),
                        Control::Continue,
                    ),
                },
            }
        }
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
    tab: Option<&str>,
) -> Reply {
    // A hold is not ended by the toggle. An action that ends a hold at once is
    // issue #55; doing it here would be that feature under another name.
    //
    // Refused for the whole life of the hold, the pipeline included. A
    // `dictate` accepted between the recorder being stopped and the take being
    // delivered finds the recorder idle, is told `AlreadyRunning` by nothing
    // and `Began` by the device, or — worse — takes the toggle's second half
    // and stops, transcribes and delivers the hold's own take, leaving the
    // watcher to report as failed a take that was in fact delivered.
    match &*hold_of(runtime) {
        crate::ptt::HoldState::Idle => {}
        crate::ptt::HoldState::Opening(hold) | crate::ptt::HoldState::Live(hold) => {
            return Reply::Error(format!(
                "holding for {}: a key is being held, and the recording ends \
                 on its own when the key comes up",
                hold.target
            ))
        }
        crate::ptt::HoldState::Ending(hold) => {
            return Reply::Error(format!(
                "holding for {}: the take that key produced is being \
                 transcribed, and lands in that pane on its own",
                hold.target
            ))
        }
    }
    match recorder.start(pane, cwd, agent, tab) {
        Started::Began => {
            publish(
                runtime,
                Activity::Recording {
                    target: pane.to_string(),
                    tab: tab.map(|t| t.to_string()),
                    since: runtime.clock.now(),
                },
            );
            Reply::Ok(format!("recording for {pane}"))
        }
        Started::CouldNotStart(why) => Reply::Error(why),
        Started::PreviousFailure(why) => Reply::Error(why),
        Started::AlreadyRunning => match recorder.stop() {
            Err(why) => {
                // The take is over, however badly. Nothing else will clear the
                // Recording published when it began.
                publish(runtime, Activity::Idle);
                Reply::Error(why.to_string())
            }
            Ok(take) => {
                publish(
                    runtime,
                    Activity::Working {
                        target: take.target.clone(),
                        tab: take.tab.clone(),
                        stage: Stage::Transcribing,
                    },
                );
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
                transcribe(runtime, &take, &collected.bias).0
            }
        },
    }
}

/// The one way the take path says what it is doing. A poisoned lock is
/// recovered rather than given up on, the same way `hold_of` recovers one: this
/// is display state, and refusing to publish it would strand a decoration on a
/// tab for the rest of the daemon's life.
fn publish(runtime: &Runtime, activity: Activity) {
    let mut held = runtime
        .activity
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    *held = activity;
}

/// Move the hold's stamp forward, if there is one, and say which pane it
/// belongs to. Called before a `ptt` request's context is examined, so that a
/// request the daemon cannot read never becomes evidence that the key came up
/// (`tasks/17/DESIGN_17.md`, section 2a).
///
/// The returned target is what lets the refusal that follows be journalled
/// against the hold it did not end.
fn refresh_hold(runtime: &Runtime) -> Option<String> {
    // Read before the guard is taken: the clock is nothing the hold's lock has
    // to cover, and this path runs twelve times a second.
    let now = runtime.clock.now();
    let mut state = hold_of(runtime);
    match &mut *state {
        // A device still being opened is as much a hold as a live one. The
        // repeats that arrive during the open are this hold's repeats, and the
        // stamp they leave is the one the deadline is measured from.
        crate::ptt::HoldState::Opening(hold) | crate::ptt::HoldState::Live(hold) => {
            hold.last_poke = now;
            hold.pokes = hold.pokes.saturating_add(1);
            Some(hold.target.clone())
        }
        // A hold whose take is already in the pipeline is not continued by a
        // repeat: its stamp decides nothing any more, and there is no hold for
        // a refusal to be journalled against.
        crate::ptt::HoldState::Idle | crate::ptt::HoldState::Ending(_) => None,
    }
}

/// The one way this file reads the hold, so that the four call sites cannot
/// drift apart. A poisoned lock is recovered with `into_inner()` everywhere: a
/// site that gave up instead would leave the recorder running with nothing left
/// to stop it and the watcher waiting an hour for a hold it can never see.
///
/// Poison is not reachable today — nothing fallible runs under this mutex, and
/// nothing may be added that does. Uniformity is what keeps that unreachability
/// from being the only thing standing between a lock and a stranded recording.
fn hold_of(runtime: &Runtime) -> std::sync::MutexGuard<'_, crate::ptt::HoldState> {
    runtime
        .hold
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// One repeat of a held key.
///
/// The stamp has already been moved by `refresh_hold`, before this request's
/// context was read. What is left to decide is whether this repeat begins a
/// hold — and beginning one is the only thing here that does real work.
///
/// The hold's state is claimed *before* the device is opened, not after. A real
/// input takes hundreds of milliseconds to open and the key repeats twelve
/// times a second, so a dozen repeats arrive while the first one is still
/// inside `recorder.start`. Claiming the state afterwards left every one of
/// them looking at no hold at all: each asked the recorder to start, was told
/// `AlreadyRunning`, and was answered that a take was recording for another
/// action — naming a cause that did not exist.
///
/// The guard is never held across `recorder.start`: it decides what this
/// request is, and the work happens after it is dropped.
fn ptt(
    recorder: &Recorder,
    runtime: &Runtime,
    pane: &str,
    cwd: Option<&str>,
    agent: Option<&str>,
    tab: Option<&str>,
) -> Reply {
    let now = runtime.clock.now();
    {
        let mut state = hold_of(runtime);
        match &*state {
            // A repeat. The target stays what it was: the pane is pinned when
            // the hold begins, so that text does not follow the focus while
            // somebody is still speaking.
            crate::ptt::HoldState::Opening(hold) | crate::ptt::HoldState::Live(hold) => {
                return Reply::Ok(format!("holding for {}", hold.target))
            }
            // The key came up a release gap ago and that take is being
            // transcribed. Refused rather than queued, for the reason section 7
            // of the design refuses the other collision: starting a recording
            // now would start it at a moment nobody is pressing anything, and
            // holding the request would make `ptt` a request that waits for a
            // transcription.
            crate::ptt::HoldState::Ending(hold) => {
                return Reply::Error(format!(
                    "the take held for {} is still being transcribed; \
                     hold the key again once it lands",
                    hold.target
                ))
            }
            crate::ptt::HoldState::Idle => {
                *state = crate::ptt::HoldState::Opening(crate::ptt::Hold {
                    target: pane.to_string(),
                    cwd: cwd.map(|c| c.to_string()),
                    agent: agent.map(|a| a.to_string()),
                    tab: tab.map(|t| t.to_string()),
                    began: now,
                    last_poke: now,
                    pokes: 1,
                });
            }
        }
    }
    match recorder.start(pane, cwd, agent, tab) {
        Started::Began => {
            {
                let mut state = hold_of(runtime);
                // Promoted, not rebuilt: the stamps the repeats that arrived
                // while the device was opening left behind are this hold's, and
                // the deadline is measured from them.
                let claimed = std::mem::take(&mut *state);
                *state = match claimed {
                    crate::ptt::HoldState::Opening(hold) => crate::ptt::HoldState::Live(hold),
                    // The daemon stopped while the device was opening and took
                    // the hold; there is nothing left to promote.
                    other => other,
                };
            }
            publish(
                runtime,
                Activity::Recording {
                    target: pane.to_string(),
                    tab: tab.map(|t| t.to_string()),
                    since: now,
                },
            );
            runtime.clock.wake();
            Reply::Ok(format!("holding for {pane}"))
        }
        refused => {
            {
                // No device opened, so nothing is recording. The claim this
                // request made has to go, or the watcher waits on a take that
                // does not exist and every repeat is answered as a hold.
                let mut state = hold_of(runtime);
                if matches!(&*state, crate::ptt::HoldState::Opening(_)) {
                    *state = crate::ptt::HoldState::Idle;
                }
            }
            runtime.clock.wake();
            match refused {
                Started::CouldNotStart(why) | Started::PreviousFailure(why) => Reply::Error(why),
                Started::AlreadyRunning => Reply::Error(
                    "a take is already recording for another action; \
                     end it with `herdr-voice dictate` before holding the key"
                        .to_string(),
                ),
                // The arm above is the only one that reaches here.
                Started::Began => Reply::Ok(format!("holding for {pane}")),
            }
        }
    }
}

/// Ends a hold when the repeats stop.
///
/// One thread for the daemon's life: a hold is a singleton, because the
/// recorder serves one take at a time. The pipeline runs here too, and it takes
/// seconds. Those seconds are not a gap in which the mechanism holds nothing:
/// the hold moves to `Ending` before the recorder is stopped and back to `Idle`
/// only after delivery, so a `ptt` or a `dictate` arriving inside them is
/// refused instead of finding an idle recorder and starting a second take.
fn watch(recorder: Arc<Recorder>, runtime: Arc<Runtime>, stop: Arc<AtomicBool>) {
    loop {
        if stop.load(Ordering::SeqCst) {
            finish_on_shutdown(&recorder, &runtime);
            return;
        }
        let now = runtime.clock.now();
        let decision = crate::ptt::decide(now, &hold_of(&runtime), &runtime.ptt);
        match decision {
            crate::ptt::Decision::Idle => runtime.clock.wait_until(now.saturating_add(3_600_000)),
            crate::ptt::Decision::KeepWaiting { until } => runtime.clock.wait_until(until),
            crate::ptt::Decision::Release { held_ms } => {
                let Some(hold) = begin_ending(&runtime) else {
                    continue;
                };
                runtime
                    .journal
                    .write(&released_line(&hold.target, held_ms, hold.pokes));
                end_take(&recorder, &runtime, &hold);
                finish_ending(&runtime);
            }
            crate::ptt::Decision::TooShort { held_ms } => {
                let Some(hold) = begin_ending(&runtime) else {
                    continue;
                };
                runtime.journal.write(&too_short_line(
                    &hold.target,
                    held_ms,
                    runtime.ptt.min_hold_ms,
                ));
                discard_take(&recorder, &runtime, &hold, held_ms);
                finish_ending(&runtime);
            }
        }
    }
}

/// The daemon is going away with a key still down. This is not a release: no
/// repeat stopped arriving, so nothing here says the key came up. The recording
/// is stopped and kept, and the journal names where it is.
///
/// A hold whose device was still being opened is taken too. The `ptt` request
/// that claimed it finds nothing left to promote and answers as it would have,
/// and the recording it opened, if it opened one, outlives nothing: the process
/// is on its way out.
fn finish_on_shutdown(recorder: &Recorder, runtime: &Runtime) {
    let Some(hold) = take_hold(runtime) else {
        return;
    };
    // Abandoned is one of the ways a take ends, and the drawing thread is
    // joined after this: its last look must not find a take still running.
    publish(runtime, Activity::Idle);
    match recorder.stop() {
        Ok(take) => runtime.journal.write(&kept_on_shutdown_line(
            &hold.target,
            &take.path.display().to_string(),
        )),
        Err(why) => runtime
            .journal
            .write(&shutdown_lost_line(&hold.target, &why.to_string())),
    }
}

/// Clear the hold and return it, at whatever stage it had reached, so no second
/// path can act on the same one.
///
/// Only shutdown uses this. A release goes through `begin_ending`, which keeps
/// the state saying a take of this mechanism exists for as long as the pipeline
/// runs.
fn take_hold(runtime: &Runtime) -> Option<crate::ptt::Hold> {
    let mut state = hold_of(runtime);
    match std::mem::take(&mut *state) {
        crate::ptt::HoldState::Idle => None,
        crate::ptt::HoldState::Opening(hold)
        | crate::ptt::HoldState::Live(hold)
        | crate::ptt::HoldState::Ending(hold) => Some(hold),
    }
}

/// Move a live hold into the pipeline and hand back a copy of it.
///
/// The state stays non-idle for the whole of the stopping, the transcription
/// and the delivery. That is the window a `dictate` used to land in and take the
/// hold's own take for itself, and the one a `ptt` used to land in and start a
/// second recording that nothing could time.
fn begin_ending(runtime: &Runtime) -> Option<crate::ptt::Hold> {
    let mut state = hold_of(runtime);
    match std::mem::take(&mut *state) {
        crate::ptt::HoldState::Live(hold) => {
            *state = crate::ptt::HoldState::Ending(hold.clone());
            Some(hold)
        }
        other => {
            *state = other;
            None
        }
    }
}

/// The pipeline is done and this mechanism holds nothing again. Anything but
/// `Ending` is left alone: it was put there by something that is not this
/// take, and is not this call's to clear.
fn finish_ending(runtime: &Runtime) {
    let mut state = hold_of(runtime);
    if matches!(&*state, crate::ptt::HoldState::Ending(_)) {
        *state = crate::ptt::HoldState::Idle;
    }
}

/// A hold that was released: stop the recording and run the take through the
/// pipeline the toggle already runs.
///
/// `recorder.stop()` failing here is how a device that died during the hold is
/// discovered — the watcher has no way to learn it sooner
/// (`tasks/17/DESIGN_17.md`, section 3). That is a second fact, not a different
/// reason the hold ended: the release line is already written, and this adds
/// the failure beside it.
fn end_take(recorder: &Recorder, runtime: &Runtime, hold: &crate::ptt::Hold) {
    match recorder.stop() {
        Ok(take) => {
            publish(
                runtime,
                Activity::Working {
                    target: take.target.clone(),
                    tab: take.tab.clone(),
                    stage: Stage::Transcribing,
                },
            );
            let collected = take_bias(
                runtime,
                &take.target,
                take.cwd.as_deref(),
                take.agent.as_deref(),
            );
            // `transcribe` reports its own delivery failure, and only that one.
            // Reporting again here would put two journal lines and two toasts
            // on one failure; saying nothing would leave the other two silent,
            // because a hold has no keypress waiting to be told.
            match transcribe(runtime, &take, &collected.bias) {
                (_, Reported::Yes) => {}
                (Reply::Error(why), Reported::No) => report_failure(runtime, &hold.target, &why),
                (Reply::Ok(_), Reported::No) => {}
            }
        }
        Err(why) => {
            // No take to run through the pipeline, so nothing downstream will
            // publish the end of this one.
            publish(runtime, Activity::Idle);
            report_failure(runtime, &hold.target, &why.to_string());
        }
    }
}

/// A hold too short to be one: stop the recording, remove it, and say so.
///
/// Only the `Ok` path has a file to remove. When the recorder refuses a take
/// itself — a lost device, or a level under the floor — it removes the file
/// before returning the error.
fn discard_take(recorder: &Recorder, runtime: &Runtime, hold: &crate::ptt::Hold, held_ms: u64) {
    // A tap is a take that ended; the indicator has to stop saying otherwise
    // whichever way the recorder answers.
    publish(runtime, Activity::Idle);
    match recorder.stop() {
        Ok(take) => {
            let _ = std::fs::remove_file(&take.path);
            // The journal line was written by the caller; this is the part the
            // person sees without going to look.
            toast(
                runtime,
                "Too short to be a hold",
                &format!(
                    "{}: held {held_ms} ms, and a hold starts at {} ms",
                    hold.target, runtime.ptt.min_hold_ms
                ),
            );
        }
        Err(why) => report_failure(runtime, &hold.target, &why.to_string()),
    }
}

/// A failure the person has to know about: recorded, and raised where they are
/// looking when `[ui] toasts` is on.
fn report_failure(runtime: &Runtime, target: &str, why: &str) {
    runtime.journal.write(&take_failed_line(target, why));
    toast(runtime, "Dictation failed", &format!("{target}: {why}"));
}

/// `[ui] toasts` decides whether the person is interrupted. It never decides
/// whether a failure is recorded — the journal line is written by the caller in
/// every case, including this one.
fn toast(runtime: &Runtime, title: &str, body: &str) {
    if !runtime.delivery_settings.toasts {
        return;
    }
    if let Err(why) = runtime.deliverer.notify(title, body) {
        runtime
            .journal
            .write(&toast_failed_line(&why.to_string().replace('\n', " ")));
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

/// Whether the failure inside a `Reply::Error` has already been journalled and
/// toasted by the code that produced it.
///
/// Only the delivery branch of `transcribe` reports its own failure. This is
/// how a caller with nobody waiting for the reply — the watcher, ending a hold
/// — knows which failures it still has to announce, without reading the
/// message to guess.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Reported {
    Yes,
    No,
}

/// A finished take becomes text, and the text is delivered — or, if either
/// step fails, the reply is a Reply::Error naming why and what to do next
/// (client::outcome maps Reply::Ok to exit 0, Reply::Error to exit 1).
///
/// The second half of the return says whether the failure in a `Reply::Error`
/// was already journalled and toasted here. The toggle drops it, because the
/// client prints the reply; the watcher reads it, because a hold has nobody
/// waiting for a reply at all.
fn transcribe(runtime: &Runtime, take: &crate::capture::Take, bias: &str) -> (Reply, Reported) {
    let outcome = transcribe_take(runtime, take, bias);
    // Every way out of the pipeline is a take that ended, the two that give up
    // before delivery included. Published here rather than at each return so
    // that a path added later cannot forget it: a take left saying TRANSCR is a
    // token renewed forever and a tab decorated forever.
    publish(runtime, Activity::Idle);
    outcome
}

fn transcribe_take(
    runtime: &Runtime,
    take: &crate::capture::Take,
    bias: &str,
) -> (Reply, Reported) {
    let engine = match &runtime.recognition {
        Ok(engine) => engine,
        // The take is on disk and named, so nothing is lost by the engine being
        // absent: somebody can fix the configuration and the file is still there.
        Err(why) => {
            return (
                Reply::Error(format!(
                    "{why} — the take is kept at {}",
                    take.path.display()
                )),
                Reported::No,
            )
        }
    };
    let text = match engine.transcribe(&take.path, bias) {
        Ok(text) => text,
        Err(why) => {
            return (
                Reply::Error(format!(
                    "{why} — the take is kept at {}",
                    take.path.display()
                )),
                Reported::No,
            )
        }
    };

    // Between recognition producing `text` and delivery: `off` (never
    // represented here — see `Resolution::Off`) and a working engine both
    // leave the take on the reply path below unaffected either way; only a
    // configured engine that is unavailable, or one that fails, ever calls
    // `tell_once` (`tasks/36/DESIGN_36.md`, section 2).
    publish(
        runtime,
        Activity::Working {
            target: take.target.clone(),
            tab: take.tab.clone(),
            stage: Stage::Fixing,
        },
    );
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
        Ok(()) => (
            Reply::Ok(format!(
                "delivered to {} [{:.1} dB]",
                take.target, take.level_dbfs
            )),
            Reported::No,
        ),
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
            (
                Reply::Error(format!(
                    "could not deliver to {target} ({why}) — the take is kept at {path}; text: {}",
                    text.replace('\n', " "),
                )),
                // The journal line and the toast above are this branch's own
                // report; a caller that reported again would double it.
                Reported::Yes,
            )
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

/// A repeat whose context could not be read, while a hold was open. Journal
/// only: the keypress's own reply already carries the failure, and the hold
/// continuing is the correct outcome rather than something to act on.
fn unreadable_repeat_line(target: &str, why: &str) -> String {
    format!("ptt {target}: a repeat could not be read ({why}); the hold continues")
}

/// Written when the repeats stopped and the hold ended on its own. Journal
/// only: the text arriving in the pane is what the person sees, and a hold
/// ending on time is not something to act on.
fn released_line(target: &str, held_ms: u64, pokes: u32) -> String {
    format!("ptt {target}: released after {held_ms} ms and {pokes} repeats")
}

/// Written when the hold was shorter than `[ptt] min_hold_ms`. Both durations
/// are named, so the minimum can be judged against the hold that missed it.
fn too_short_line(target: &str, held_ms: u64, min_hold_ms: u64) -> String {
    format!(
        "ptt {target}: too short to be a hold — held {held_ms} ms, and a hold \
         starts at {min_hold_ms} ms; hold the key while you speak"
    )
}

/// Written when a take a hold produced could not be finished — the recorder
/// refused it, recognition was unavailable, or recognition failed. Nobody is
/// waiting for a reply, so this line and the toast beside it are the only way
/// the person learns of it.
fn take_failed_line(target: &str, why: &str) -> String {
    format!(
        "ptt {target}: the take failed ({}); nothing was delivered",
        why.replace('\n', " ")
    )
}

/// Written when the daemon stopped while a key was still down. The take is not
/// transcribed and not delivered — nobody is there to receive it — so the path
/// is what makes it recoverable by hand.
fn kept_on_shutdown_line(target: &str, path: &str) -> String {
    format!(
        "ptt {target}: the daemon stopped with the key still down; the \
         recording is kept at {}",
        path.replace('\n', " ")
    )
}

/// Written when the daemon stopped while a key was still down and the take
/// could not even be kept — the recorder refused it.
fn shutdown_lost_line(target: &str, why: &str) -> String {
    format!(
        "ptt {target}: the daemon stopped with the key still down and the \
         recording could not be kept ({}); hold the key again once the daemon \
         is back",
        why.replace('\n', " ")
    )
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
    // Which device the built-in engine will run on, said out loud at start: the
    // CPU is about ten times slower, and somebody who is on it should learn that
    // now rather than after waiting a minute for a minute of speech
    // (`tasks/15/DESIGN_15.md`, section 8). This costs one more `check_with`
    // than strictly needed, which reads no weights, and it keeps the reporting
    // out of `resolve_with`, which the daemon and `doctor` share.
    let state = stt::locate_configured_model(&loaded.config.stt, &models);
    if let Ok(stt::Ready::Candle { device, .. }) = stt::check_with(&loaded.config.stt, &state) {
        eprintln!(
            "recognition: the built-in engine, {}",
            crate::stt::candle::device::describe(&device)
        );
    }
    // Not fatal: the daemon still answers `cancel`, and `doctor` reports the same
    // thing this does. The reason is kept and given to whoever finishes a take.
    let recognition: Recognition = stt::resolve_with(&loaded.config.stt, state).map_err(|e| {
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
        hold: std::sync::Mutex::new(crate::ptt::HoldState::Idle),
        ptt: crate::ptt::Settings {
            release_ms: loaded.config.ptt.release_ms,
            min_hold_ms: loaded.config.ptt.min_hold_ms,
        },
        clock: std::sync::Arc::new(crate::ptt::SystemClock::default()),
        activity: std::sync::Mutex::new(Activity::Idle),
        ui: loaded.config.ui.clone(),
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
    let watcher = {
        let recorder = Arc::clone(&recorder);
        let runtime = Arc::clone(&runtime);
        let stop = Arc::clone(&stop);
        thread::spawn(move || watch(recorder, runtime, stop))
    };
    // The drawing thread waits on a clock of its own. `Clock`'s contract
    // consumes a wake with the return it causes, so two waiters on one clock
    // steal each other's wakes — and the hold's start and this shutdown both
    // depend on a wake reaching the watcher.
    let drawing_clock: Arc<dyn crate::ptt::Clock> = Arc::new(crate::ptt::SystemClock::default());
    let drawer = {
        let painter: Arc<dyn crate::indicator::Painter> =
            Arc::new(crate::indicator::HerdrPainter::new());
        let runtime = Arc::clone(&runtime);
        let clock = Arc::clone(&drawing_clock);
        let stop = Arc::clone(&stop);
        thread::spawn(move || crate::indicator::draw(painter, runtime, clock, stop))
    };
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

    // The watcher is waiting on the clock, not on the accept loop, so it has to
    // be woken before it can see the stop flag and end.
    runtime.clock.wake();
    if let Err(e) = watcher.join() {
        eprintln!("the watcher thread ended badly: {e:?}");
    }
    // Woken and joined for the same reason, and after the watcher: a thread
    // that could still draw once the daemon had decided to stop would leave a
    // decoration behind it.
    drawing_clock.wake();
    if let Err(e) = drawer.join() {
        eprintln!("the drawing thread ended badly: {e:?}");
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
pub mod tests_support {
    use super::*;

    /// A program that is certainly not there, so `bias::pane::read` misses
    /// without a live herdr — the same fixture `bias`'s own tests use.
    pub const MISSING_HERDR: &str = "/definitely/not/a/real/herdr-binary";

    /// Records every line written, in order.
    #[derive(Default)]
    pub struct RecordingJournal(pub std::sync::Mutex<Vec<String>>);
    impl Journal for RecordingJournal {
        fn write(&self, line: &str) {
            self.0.lock().unwrap().push(line.to_string());
        }
    }

    /// Lets a Runtime own a Journal while the test keeps its own handle to read
    /// what was written — the same shape FakeDeliverer::clone() gives.
    pub struct TestJournal(pub std::sync::Arc<RecordingJournal>);
    impl Journal for TestJournal {
        fn write(&self, line: &str) {
            self.0.write(line);
        }
    }

    /// The runtime the drawing tests need: `activity` at `Idle`, the given
    /// `[ui]`, and the take's clock as `runtime.clock`.
    ///
    /// The drawing thread waits on a clock of its own, which the caller builds
    /// separately — the two count from different origins, and this one is the
    /// one elapsed time is measured against.
    pub fn runtime_with_clocks(
        take_clock: &std::sync::Arc<crate::ptt::tests_support::TestClock>,
        ui: crate::config::Ui,
    ) -> Runtime {
        Runtime {
            recognition: Ok(Box::new(crate::stt::tests_support::Fake(Ok(
                "a transcript".to_string(),
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
            hold: std::sync::Mutex::new(crate::ptt::HoldState::Idle),
            ptt: crate::ptt::Settings {
                release_ms: 1000,
                min_hold_ms: 300,
            },
            clock: std::sync::Arc::clone(take_clock) as std::sync::Arc<dyn crate::ptt::Clock>,
            activity: std::sync::Mutex::new(Activity::Idle),
            ui,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::tests_support::{RecordingJournal, TestJournal, MISSING_HERDR};
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
    /// The same runtime `fake_runtime` builds, plus the clock it was built
    /// with. Tests that move time need the concrete type; `Runtime` only ever
    /// holds the trait object, so the concrete `Arc` is handed back here rather
    /// than stored and downcast.
    fn fake_runtime_with_clock(
        text: &str,
    ) -> (
        Runtime,
        std::sync::Arc<crate::ptt::tests_support::TestClock>,
    ) {
        let clock = std::sync::Arc::new(crate::ptt::tests_support::TestClock::default());
        let runtime = Runtime {
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
            hold: std::sync::Mutex::new(crate::ptt::HoldState::Idle),
            ptt: crate::ptt::Settings {
                release_ms: 1000,
                min_hold_ms: 300,
            },
            clock: std::sync::Arc::clone(&clock) as std::sync::Arc<dyn crate::ptt::Clock>,
            activity: std::sync::Mutex::new(Activity::Idle),
            ui: crate::config::Ui::default(),
        };
        (runtime, clock)
    }

    fn fake_runtime(text: &str) -> Runtime {
        fake_runtime_with_clock(text).0
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

    /// Where a recorder built with `tag` writes its takes. A test that has to
    /// look at the files needs the same path the recorder was given, and one
    /// per tag keeps two tests from reading each other's takes.
    fn takes_dir(tag: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("daemon-takes-{tag}-{}", std::process::id()))
    }

    /// The wav files a take left behind in `dir`. An absent directory is no
    /// files, which is what it means.
    fn wavs_in(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return Vec::new();
        };
        entries
            .filter_map(|entry| entry.ok().map(|entry| entry.path()))
            .filter(|path| path.extension().is_some_and(|ext| ext == "wav"))
            .collect()
    }

    /// A recorder that hears one loud moment and stops — clears the silence
    /// floor, so a test can reach transcription and delivery.
    fn tone_recorder(tag: &str) -> Recorder {
        Recorder::spawn(
            || Box::new(crate::capture::tests_support::ToneSource),
            crate::config::Audio::default(),
            takes_dir(tag),
        )
    }

    /// A recorder whose device goes away during the take, so the mid-take
    /// failure can be driven without hardware.
    fn losing_recorder(tag: &str) -> Recorder {
        Recorder::spawn(
            || Box::new(crate::capture::tests_support::LosingSource),
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

    /// As `runtime_with`, plus the clock. `runtime_with` keeps its signature —
    /// `FakeDeliverer` unboxed, `submit: bool` — because the existing suite
    /// calls it a dozen times.
    fn runtime_with_clock(
        deliverer: crate::delivery::tests_support::FakeDeliverer,
        submit: bool,
    ) -> (
        Runtime,
        std::sync::Arc<crate::ptt::tests_support::TestClock>,
    ) {
        let clock = std::sync::Arc::new(crate::ptt::tests_support::TestClock::default());
        let runtime = Runtime {
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
            hold: std::sync::Mutex::new(crate::ptt::HoldState::Idle),
            ptt: crate::ptt::Settings {
                release_ms: 1000,
                min_hold_ms: 300,
            },
            clock: std::sync::Arc::clone(&clock) as std::sync::Arc<dyn crate::ptt::Clock>,
            activity: std::sync::Mutex::new(Activity::Idle),
            ui: crate::config::Ui::default(),
        };
        (runtime, clock)
    }

    fn runtime_with(
        deliverer: crate::delivery::tests_support::FakeDeliverer,
        submit: bool,
    ) -> Runtime {
        runtime_with_clock(deliverer, submit).0
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
            hold: std::sync::Mutex::new(crate::ptt::HoldState::Idle),
            ptt: crate::ptt::Settings {
                release_ms: 1000,
                min_hold_ms: 300,
            },
            // This test never holds a key; the real clock is the plain choice.
            clock: std::sync::Arc::new(crate::ptt::SystemClock::default()),
            activity: std::sync::Mutex::new(Activity::Idle),
            ui: crate::config::Ui::default(),
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
            tab: None,
        };
        let fake = crate::delivery::tests_support::FakeDeliverer::failing(
            crate::delivery::DeliveryError::Rejected("pane_not_found".into()),
        );
        let runtime = runtime_with(fake, false);
        let (reply, reported) = transcribe(&runtime, &take, "");
        assert_eq!(
            reported,
            Reported::Yes,
            "the delivery branch reports its own failure; that is what lets a \
             hold know which failures it still has to announce"
        );
        let text = match reply {
            Reply::Error(text) => text,
            other => panic!("expected Reply::Error, got {other:?}"),
        };
        assert!(!text.contains('\n'), "got {text:?}");
        assert!(text.contains("w1 :p2"), "got {text:?}");
        assert!(text.contains("/tmp/oddly named.wav"), "got {text:?}");
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
            "открой файл".to_string()
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

    const PANE_1: &[u8] = br#"{"focused_pane_id":"w1:p1"}"#;

    /// How long a test waits for something the watcher thread produces before
    /// failing. Generous, because it is a bound on a broken run rather than a
    /// duration any healthy run pays: every wait returns as soon as the thing
    /// it waits for appears.
    const WITHIN: std::time::Duration = std::time::Duration::from_secs(5);

    #[test]
    fn a_first_ptt_begins_a_hold_and_names_the_pane() {
        let (runtime, _clock) = fake_runtime_with_clock("a transcript");
        let recorder = tone_recorder("ptt-first");
        let (reply, _) = answer(&request("ptt", PANE_1), &recorder, &runtime);
        assert_eq!(reply, Reply::Ok("holding for w1:p1".to_string()));
        let held = runtime.hold.lock().unwrap();
        let hold = held.hold().expect("a hold");
        assert_eq!(hold.target, "w1:p1");
        assert_eq!(hold.pokes, 1);
    }

    #[test]
    fn a_repeat_refreshes_the_stamp_and_does_not_start_a_second_take() {
        let (runtime, clock) = fake_runtime_with_clock("a transcript");
        let recorder = tone_recorder("ptt-repeat");
        answer(&request("ptt", PANE_1), &recorder, &runtime);
        let first = runtime.hold.lock().unwrap().hold().unwrap().last_poke;
        clock.advance(90);
        answer(&request("ptt", PANE_1), &recorder, &runtime);
        let held = runtime.hold.lock().unwrap();
        let hold = held.hold().expect("still one hold");
        assert!(hold.last_poke > first, "the repeat must move the stamp");
        assert_eq!(hold.pokes, 2);
        assert_eq!(hold.target, "w1:p1");
    }

    #[test]
    fn a_repeat_from_another_pane_does_not_move_the_target() {
        let (runtime, clock) = fake_runtime_with_clock("a transcript");
        let recorder = tone_recorder("ptt-other-pane");
        answer(&request("ptt", PANE_1), &recorder, &runtime);
        clock.advance(90);
        let (reply, _) = answer(
            &request("ptt", br#"{"focused_pane_id":"w1:p9"}"#),
            &recorder,
            &runtime,
        );
        assert_eq!(reply, Reply::Ok("holding for w1:p1".to_string()));
        assert_eq!(runtime.hold.lock().unwrap().hold().unwrap().target, "w1:p1");
    }

    #[test]
    fn an_unreadable_repeat_refreshes_the_hold_and_is_still_refused() {
        let (runtime, clock) = fake_runtime_with_clock("a transcript");
        let recorder = tone_recorder("ptt-unreadable");
        answer(&request("ptt", PANE_1), &recorder, &runtime);
        let first = runtime.hold.lock().unwrap().hold().unwrap().last_poke;
        clock.advance(90);
        // Not JSON at all: `context::parse` refuses it.
        let (reply, _) = answer(&request("ptt", b"not json"), &recorder, &runtime);
        assert!(
            matches!(reply, Reply::Error(_)),
            "the keypress itself failed"
        );
        let held = runtime.hold.lock().unwrap();
        let hold = held.hold().expect("the hold survives an unreadable repeat");
        assert!(
            hold.last_poke > first,
            "a signal the daemon could not read is not evidence the key came up"
        );
    }

    #[test]
    fn an_unreadable_request_with_no_hold_open_is_refused_as_before() {
        let (runtime, _clock) = fake_runtime_with_clock("a transcript");
        let recorder = tone_recorder("ptt-unreadable-idle");
        let (reply, _) = answer(&request("ptt", b"not json"), &recorder, &runtime);
        assert!(matches!(reply, Reply::Error(_)));
        assert!(
            runtime.hold.lock().unwrap().is_idle(),
            "nothing to pin a hold to"
        );
    }

    #[test]
    fn an_unreadable_repeat_is_recorded_against_the_hold_it_did_not_end() {
        let fake = crate::delivery::tests_support::FakeDeliverer::ok();
        let (mut runtime, clock) = runtime_with_clock(fake, false);
        let journal = std::sync::Arc::new(RecordingJournal::default());
        runtime.journal = Box::new(TestJournal(std::sync::Arc::clone(&journal)));
        let recorder = tone_recorder("ptt-unreadable-line");
        answer(&request("ptt", PANE_1), &recorder, &runtime);
        clock.advance(90);
        answer(&request("ptt", b"not json"), &recorder, &runtime);
        let lines = journal.0.lock().unwrap();
        assert!(
            lines.iter().any(|line| line.contains("the hold continues")),
            "the refusal is recorded against the hold it did not end: got {lines:?}"
        );
    }

    #[test]
    fn an_unreadable_request_with_no_hold_writes_no_ptt_line() {
        let fake = crate::delivery::tests_support::FakeDeliverer::ok();
        let (mut runtime, _clock) = runtime_with_clock(fake, false);
        let journal = std::sync::Arc::new(RecordingJournal::default());
        runtime.journal = Box::new(TestJournal(std::sync::Arc::clone(&journal)));
        let recorder = tone_recorder("ptt-unreadable-line-idle");
        answer(&request("ptt", b"not json"), &recorder, &runtime);
        let lines = journal.0.lock().unwrap();
        assert!(
            lines.is_empty(),
            "with no hold there is nothing a refusal failed to end: got {lines:?}"
        );
    }

    /// Poll the fake's log until it has a call or the bound expires. Waits on
    /// the condition rather than on a duration, so it neither slows the suite
    /// nor goes flaky on a loaded machine.
    fn wait_for_calls(
        fake: &crate::delivery::tests_support::FakeDeliverer,
        within: std::time::Duration,
    ) -> Vec<crate::delivery::tests_support::Call> {
        let deadline = std::time::Instant::now() + within;
        loop {
            let calls = fake.calls();
            if !calls.is_empty() || std::time::Instant::now() >= deadline {
                return calls;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }

    /// Poll the journal until a line contains `needle`, or the bound expires,
    /// and hand back everything written by then.
    ///
    /// A test waits for the thing it asserts on. Waiting for the hold to clear
    /// instead used to pass by luck: the hold was cleared before the journal
    /// line, the toast and the delivery that four tests then read, so the
    /// assertions raced the watcher and the suite went red about one run in
    /// twelve. The state machine has since made "the hold is clear" a point
    /// after all of them — but a test that waits on the observable stays honest
    /// if the code moves again, and a test that waits on a proxy does not.
    fn wait_for_journal(
        journal: &RecordingJournal,
        needle: &str,
        within: std::time::Duration,
    ) -> Vec<String> {
        let deadline = std::time::Instant::now() + within;
        loop {
            let lines = journal.0.lock().unwrap().clone();
            if lines.iter().any(|line| line.contains(needle)) {
                return lines;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "no line containing {needle:?} within {within:?}: got {lines:?}"
            );
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }

    #[test]
    fn the_deadline_ends_the_hold_and_the_take_is_delivered() {
        let fake = crate::delivery::tests_support::FakeDeliverer::ok();
        let (runtime, clock) = runtime_with_clock(fake.clone(), false);
        let runtime = std::sync::Arc::new(runtime);
        let recorder = std::sync::Arc::new(tone_recorder("ptt-deadline"));
        let stop = std::sync::Arc::new(AtomicBool::new(false));

        let watcher = {
            let recorder = std::sync::Arc::clone(&recorder);
            let runtime = std::sync::Arc::clone(&runtime);
            let stop = std::sync::Arc::clone(&stop);
            thread::spawn(move || watch(recorder, runtime, stop))
        };

        answer(&request("ptt", PANE_1), &recorder, &runtime);
        // A hold of 400 ms, over the 300 ms minimum: advance, then let a repeat
        // stamp the new time the way a real one would.
        clock.advance(400);
        answer(&request("ptt", PANE_1), &recorder, &runtime);
        // The gap passes with no further repeat.
        clock.advance(1_000);

        let calls = wait_for_calls(&fake, std::time::Duration::from_secs(5));
        assert!(
            matches!(
                calls.first(),
                Some(crate::delivery::tests_support::Call::Insert(pane, _)) if pane == "w1:p1"
            ),
            "the take is inserted into the pinned pane: got {calls:?}"
        );
        wait_for_idle(&runtime);
        assert!(
            runtime.hold.lock().unwrap().is_idle(),
            "the hold is cleared"
        );

        stop.store(true, Ordering::SeqCst);
        runtime.clock.wake();
        clock.advance(1);
        watcher.join().unwrap();
    }

    #[test]
    fn a_tap_is_discarded_and_says_so() {
        let fake = crate::delivery::tests_support::FakeDeliverer::ok();
        let (mut runtime, clock) = runtime_with_clock(fake.clone(), false);
        let journal = std::sync::Arc::new(RecordingJournal::default());
        runtime.journal = Box::new(TestJournal(std::sync::Arc::clone(&journal)));
        let runtime = std::sync::Arc::new(runtime);
        let takes = takes_dir("ptt-tap");
        let _ = std::fs::remove_dir_all(&takes);
        let recorder = std::sync::Arc::new(tone_recorder("ptt-tap"));
        let stop = std::sync::Arc::new(AtomicBool::new(false));

        let watcher = {
            let recorder = std::sync::Arc::clone(&recorder);
            let runtime = std::sync::Arc::clone(&runtime);
            let stop = std::sync::Arc::clone(&stop);
            thread::spawn(move || watch(recorder, runtime, stop))
        };

        // One repeat and nothing more: held for 0 ms, far under the minimum.
        answer(&request("ptt", PANE_1), &recorder, &runtime);
        clock.advance(1_000);

        // The line this test is about, not the hold: the hold is not what is
        // asserted, and waiting on it is what made this test flake.
        wait_for_journal(&journal, "too short", WITHIN);
        wait_for_idle(&runtime);

        assert!(
            fake.calls().is_empty(),
            "a tap delivers nothing: got {:?}",
            fake.calls()
        );
        // The recording itself, not only the line about it. A tap that left its
        // wav behind would leave one for every accidental tap, forever — and
        // the journal line saying it was discarded would be false.
        assert_eq!(
            wavs_in(&takes),
            Vec::<std::path::PathBuf>::new(),
            "a tap's recording is removed, not merely reported as discarded"
        );

        stop.store(true, Ordering::SeqCst);
        runtime.clock.wake();
        clock.advance(1);
        watcher.join().unwrap();
    }

    #[test]
    fn a_device_lost_during_a_hold_gives_a_release_line_and_a_failure_line() {
        let fake = crate::delivery::tests_support::FakeDeliverer::ok();
        let (mut runtime, clock) = runtime_with_clock(fake.clone(), false);
        let journal = std::sync::Arc::new(RecordingJournal::default());
        runtime.journal = Box::new(TestJournal(std::sync::Arc::clone(&journal)));
        let runtime = std::sync::Arc::new(runtime);
        let recorder = std::sync::Arc::new(losing_recorder("ptt-device-lost"));
        let stop = std::sync::Arc::new(AtomicBool::new(false));
        let watcher = {
            let recorder = std::sync::Arc::clone(&recorder);
            let runtime = std::sync::Arc::clone(&runtime);
            let stop = std::sync::Arc::clone(&stop);
            thread::spawn(move || watch(recorder, runtime, stop))
        };

        answer(&request("ptt", PANE_1), &recorder, &runtime);
        clock.advance(400);
        answer(&request("ptt", PANE_1), &recorder, &runtime);
        clock.advance(1_000);

        // Both lines this test reads, waited for by name.
        wait_for_journal(&journal, "released", WITHIN);
        let lines = wait_for_journal(&journal, "the device went away", WITHIN);
        wait_for_idle(&runtime);

        assert!(
            fake.calls().is_empty(),
            "a take whose device went away delivers nothing: {:?}",
            fake.calls()
        );
        let released = lines
            .iter()
            .find(|line| line.contains("released"))
            .unwrap_or_else(|| panic!("the hold was released, and says so: {lines:?}"));
        assert!(
            !released.contains("device"),
            "the release line says why the hold ended, not why the take failed: {released}"
        );
        assert!(
            lines
                .iter()
                .any(|line| line.contains("the device went away")),
            "and the failure is its own line: {lines:?}"
        );

        stop.store(true, Ordering::SeqCst);
        runtime.clock.wake();
        clock.advance(1);
        watcher.join().unwrap();
    }

    #[test]
    fn a_delivery_failure_during_a_hold_is_reported_once_and_not_twice() {
        let fake = crate::delivery::tests_support::FakeDeliverer::failing(
            crate::delivery::DeliveryError::Rejected("pane_not_found".into()),
        );
        let (mut runtime, clock) = runtime_with_clock(fake.clone(), false);
        runtime.delivery_settings.toasts = true;
        let journal = std::sync::Arc::new(RecordingJournal::default());
        runtime.journal = Box::new(TestJournal(std::sync::Arc::clone(&journal)));
        let runtime = std::sync::Arc::new(runtime);
        let recorder = std::sync::Arc::new(tone_recorder("ptt-delivery-failure"));
        let stop = std::sync::Arc::new(AtomicBool::new(false));
        let watcher = {
            let recorder = std::sync::Arc::clone(&recorder);
            let runtime = std::sync::Arc::clone(&runtime);
            let stop = std::sync::Arc::clone(&stop);
            thread::spawn(move || watch(recorder, runtime, stop))
        };

        answer(&request("ptt", PANE_1), &recorder, &runtime);
        clock.advance(400);
        answer(&request("ptt", PANE_1), &recorder, &runtime);
        clock.advance(1_000);

        // The failure this test counts, then the watcher being done with the
        // take — so "once" is counted after everything that could report a
        // second time has run, rather than at the first sighting of the first.
        wait_for_journal(&journal, "pane_not_found", WITHIN);
        wait_for_idle(&runtime);

        let notifies = fake
            .calls()
            .into_iter()
            .filter(|call| matches!(call, crate::delivery::tests_support::Call::Notify(_, _)))
            .count();
        assert_eq!(notifies, 1, "one failure, one toast: {:?}", fake.calls());
        let lines = journal.0.lock().unwrap().clone();
        let failures = lines
            .iter()
            .filter(|line| line.contains("pane_not_found"))
            .count();
        assert_eq!(failures, 1, "one failure, one journal line: {lines:?}");

        stop.store(true, Ordering::SeqCst);
        runtime.clock.wake();
        clock.advance(1);
        watcher.join().unwrap();
    }

    #[test]
    fn stopping_the_daemon_with_a_hold_open_keeps_the_take_and_names_it() {
        let fake = crate::delivery::tests_support::FakeDeliverer::ok();
        let (mut runtime, clock) = runtime_with_clock(fake.clone(), false);
        let journal = std::sync::Arc::new(RecordingJournal::default());
        runtime.journal = Box::new(TestJournal(std::sync::Arc::clone(&journal)));
        let runtime = std::sync::Arc::new(runtime);
        let recorder = std::sync::Arc::new(tone_recorder("ptt-shutdown"));
        let stop = std::sync::Arc::new(AtomicBool::new(false));

        let watcher = {
            let recorder = std::sync::Arc::clone(&recorder);
            let runtime = std::sync::Arc::clone(&runtime);
            let stop = std::sync::Arc::clone(&stop);
            thread::spawn(move || watch(recorder, runtime, stop))
        };

        // A key is down, well inside the gap, when the daemon is told to stop.
        answer(&request("ptt", PANE_1), &recorder, &runtime);
        clock.advance(400);
        answer(&request("ptt", PANE_1), &recorder, &runtime);
        stop.store(true, Ordering::SeqCst);
        runtime.clock.wake();
        clock.advance(1);
        watcher.join().unwrap();

        assert!(
            fake.calls().is_empty(),
            "a daemon going away delivers nothing: got {:?}",
            fake.calls()
        );
        assert!(
            runtime.hold.lock().unwrap().is_idle(),
            "the hold is cleared rather than left for nobody"
        );
        let lines = journal.0.lock().unwrap();
        let kept = lines
            .iter()
            .find(|line| line.contains(".wav"))
            .unwrap_or_else(|| panic!("the kept take is named: got {lines:?}"));
        assert!(
            !kept.contains("released"),
            "stopping is not a release: nothing here says the key came up: {kept}"
        );
        // The path the line names, on disk. A line pointing at a file that is
        // not there is worse than no line: it sends somebody looking for a
        // recording that was never kept.
        let path = kept
            .split_once("kept at ")
            .map(|(_, path)| path.trim())
            .unwrap_or_else(|| panic!("the line names where the take is: {kept}"));
        assert!(
            std::path::Path::new(path).exists(),
            "the take named by the journal line is on disk: {path}"
        );
    }

    #[test]
    fn a_dictate_during_a_hold_is_refused_and_says_what_is_running() {
        let runtime = fake_runtime("a transcript");
        let recorder = tone_recorder("collide-dictate");
        answer(&request("ptt", PANE_1), &recorder, &runtime);
        let (reply, _) = answer(&request("dictate", PANE_1), &recorder, &runtime);
        match reply {
            Reply::Error(why) => {
                assert!(why.contains("holding"), "names what is running: {why}");
                assert!(why.contains("w1:p1"), "names the pane: {why}");
            }
            other => panic!("a hold must not be ended by dictate: {other:?}"),
        }
        assert!(
            runtime.hold.lock().unwrap().hold().is_some(),
            "the hold survives"
        );
    }

    #[test]
    fn a_ptt_while_a_toggle_take_runs_is_refused_and_says_how_it_ends() {
        let runtime = fake_runtime("a transcript");
        let recorder = tone_recorder("collide-ptt");
        answer(&request("dictate", PANE_1), &recorder, &runtime);
        let (reply, _) = answer(&request("ptt", PANE_1), &recorder, &runtime);
        match reply {
            Reply::Error(why) => assert!(why.contains("dictate"), "names how it ends: {why}"),
            other => panic!("a toggle take must not be taken over by a hold: {other:?}"),
        }
        assert!(runtime.hold.lock().unwrap().is_idle(), "no hold was begun");
    }

    /// A recorder whose device takes a while to open, with the gate that says
    /// when it is inside `start` and when it may finish.
    fn opening_recorder(tag: &str, gate: &std::sync::Arc<crate::gate::Gate>) -> Recorder {
        let gate = std::sync::Arc::clone(gate);
        Recorder::spawn(
            move || Box::new(crate::capture::tests_support::OpeningSource(gate)),
            crate::config::Audio::default(),
            std::env::temp_dir().join(format!("daemon-takes-{tag}-{}", std::process::id())),
        )
    }

    /// Wait for the watcher to be done with the hold — the state idle again —
    /// rather than for a duration.
    ///
    /// Never the only thing a test waits on. What a test asserts is a journal
    /// line, a delivery call or a file, and it waits for that; this is for the
    /// end of the test, where the watcher has to be finished before it is
    /// stopped and joined.
    fn wait_for_idle(runtime: &Runtime) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !runtime.hold.lock().unwrap().is_idle() && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }

    /// A hold whose take has been released and is inside the recogniser, with
    /// everything the caller needs to act in that window and to close it.
    struct InThePipeline {
        runtime: std::sync::Arc<Runtime>,
        recorder: std::sync::Arc<Recorder>,
        clock: std::sync::Arc<crate::ptt::tests_support::TestClock>,
        gate: std::sync::Arc<crate::gate::Gate>,
        journal: std::sync::Arc<RecordingJournal>,
        stop: std::sync::Arc<AtomicBool>,
        watcher: thread::JoinHandle<()>,
    }

    /// Drive a hold to the point where the deadline has passed, the recorder
    /// has been stopped and the take is inside recognition — and stop it there.
    ///
    /// The window this opens is the one two defects lived in: a `dictate`
    /// landing in it took the hold's own take for itself, and a `ptt` landing
    /// in it started a second recording nothing could time. It is held open by
    /// a gate rather than by a sleep, so the tests that use it are the same on
    /// every machine.
    fn a_take_in_the_pipeline(
        tag: &str,
        fake: &crate::delivery::tests_support::FakeDeliverer,
    ) -> InThePipeline {
        let gate = std::sync::Arc::new(crate::gate::Gate::default());
        let (mut runtime, clock) = runtime_with_clock(fake.clone(), false);
        runtime.recognition = Ok(Box::new(crate::stt::tests_support::BlockingFake {
            gate: std::sync::Arc::clone(&gate),
            text: "fix the worklog entry".to_string(),
        }));
        let journal = std::sync::Arc::new(RecordingJournal::default());
        runtime.journal = Box::new(TestJournal(std::sync::Arc::clone(&journal)));
        let runtime = std::sync::Arc::new(runtime);
        let recorder = std::sync::Arc::new(tone_recorder(tag));
        let stop = std::sync::Arc::new(AtomicBool::new(false));
        let watcher = {
            let recorder = std::sync::Arc::clone(&recorder);
            let runtime = std::sync::Arc::clone(&runtime);
            let stop = std::sync::Arc::clone(&stop);
            thread::spawn(move || watch(recorder, runtime, stop))
        };

        answer(&request("ptt", PANE_1), &recorder, &runtime);
        clock.advance(400);
        answer(&request("ptt", PANE_1), &recorder, &runtime);
        clock.advance(1_000);
        // The watcher has taken the hold, stopped the recorder and reached the
        // recogniser. It stays there until `gate.open()`.
        gate.wait_until_entered();

        InThePipeline {
            runtime,
            recorder,
            clock,
            gate,
            journal,
            stop,
            watcher,
        }
    }

    impl InThePipeline {
        /// Let the take finish, wait for the watcher to be done with it, and
        /// stop the watcher.
        fn finish(self) {
            self.gate.open();
            wait_for_idle(&self.runtime);
            self.stop.store(true, Ordering::SeqCst);
            self.runtime.clock.wake();
            self.clock.advance(1);
            self.watcher.join().unwrap();
        }
    }

    #[test]
    fn repeats_arriving_while_the_device_opens_are_repeats_and_not_errors() {
        let gate = std::sync::Arc::new(crate::gate::Gate::default());
        let (runtime, _clock) = fake_runtime_with_clock("a transcript");
        let runtime = std::sync::Arc::new(runtime);
        let recorder = std::sync::Arc::new(opening_recorder("ptt-opening", &gate));

        // The first keypress: it is inside `recorder.start` for as long as the
        // gate is shut, which is what a real input device does for hundreds of
        // milliseconds.
        let first = {
            let recorder = std::sync::Arc::clone(&recorder);
            let runtime = std::sync::Arc::clone(&runtime);
            thread::spawn(move || answer(&request("ptt", PANE_1), &recorder, &runtime).0)
        };
        gate.wait_until_entered();

        // Twelve a second means a dozen of these arrive while the device is
        // still opening. Each is a repeat of the hold that is being opened, not
        // a collision with some other action.
        for _ in 0..3 {
            let (reply, _) = answer(&request("ptt", PANE_1), &recorder, &runtime);
            assert_eq!(
                reply,
                Reply::Ok("holding for w1:p1".to_string()),
                "a repeat arriving while the device opens is a repeat"
            );
        }
        assert_eq!(
            runtime.hold.lock().unwrap().hold().unwrap().pokes,
            4,
            "and each of them counts against the hold it belongs to"
        );

        gate.open();
        assert_eq!(
            first.join().unwrap(),
            Reply::Ok("holding for w1:p1".to_string())
        );
        assert!(
            matches!(
                &*runtime.hold.lock().unwrap(),
                crate::ptt::HoldState::Live(_)
            ),
            "the device opened, so the hold is live"
        );
    }

    #[test]
    fn a_hold_that_could_not_open_its_device_leaves_nothing_behind() {
        let (runtime, _clock) = fake_runtime_with_clock("a transcript");
        // A recorder whose thread is gone refuses every start, which is the
        // shape of any device that will not open.
        let recorder = tone_recorder("ptt-refused");
        answer(&request("dictate", PANE_1), &recorder, &runtime);
        // A take is running for the toggle, so the hold's start is refused.
        let (reply, _) = answer(&request("ptt", PANE_1), &recorder, &runtime);
        assert!(matches!(reply, Reply::Error(_)), "got {reply:?}");
        assert!(
            runtime.hold.lock().unwrap().is_idle(),
            "a hold whose device never opened must not be left for the watcher"
        );
    }

    #[test]
    fn a_dictate_arriving_while_the_take_is_in_the_pipeline_is_refused() {
        let fake = crate::delivery::tests_support::FakeDeliverer::ok();
        let held = a_take_in_the_pipeline("ptt-pipeline-dictate", &fake);

        let (reply, _) = answer(&request("dictate", PANE_1), &held.recorder, &held.runtime);
        match reply {
            Reply::Error(why) => assert!(
                why.contains("w1:p1"),
                "the refusal names the take that is still being finished: {why}"
            ),
            // Accepted, it would find the recorder idle and take the hold's own
            // take for itself: the transcript would be delivered by this
            // keypress and the watcher would report the take as failed.
            other => panic!("a take in the pipeline must not be taken over: {other:?}"),
        }

        held.gate.open();
        let calls = wait_for_calls(&fake, std::time::Duration::from_secs(5));
        assert_eq!(
            calls.len(),
            1,
            "one take, one delivery, and no second recording: {calls:?}"
        );
        let lines = held.journal.0.lock().unwrap();
        assert!(
            !lines.iter().any(|line| line.contains("the take failed")),
            "a take that was delivered is not also reported as failed: {lines:?}"
        );
        drop(lines);
        held.finish();
    }

    #[test]
    fn a_ptt_arriving_while_the_take_is_in_the_pipeline_starts_no_second_recording() {
        let fake = crate::delivery::tests_support::FakeDeliverer::ok();
        let held = a_take_in_the_pipeline("ptt-pipeline-ptt", &fake);

        // Answered while the recogniser is still blocked: a keypress never
        // waits for a transcription, which is why the guard is not held across
        // the pipeline.
        let (reply, _) = answer(&request("ptt", PANE_1), &held.recorder, &held.runtime);
        match reply {
            Reply::Error(why) => assert!(
                why.contains("w1:p1") && why.contains("transcribed"),
                "the refusal says what is still running: {why}"
            ),
            // Accepted, it would start a recording the watcher cannot time
            // until the first take's pipeline returns — so the second take
            // keeps recording for the length of the first transcription.
            other => panic!("a second recording must not begin here: {other:?}"),
        }
        assert!(
            matches!(
                &*held.runtime.hold.lock().unwrap(),
                crate::ptt::HoldState::Ending(_)
            ),
            "the refused keypress left the take that is finishing alone"
        );

        held.gate.open();
        let calls = wait_for_calls(&fake, std::time::Duration::from_secs(5));
        assert_eq!(calls.len(), 1, "one take, one delivery: {calls:?}");
        wait_for_idle(&held.runtime);
        assert!(
            held.runtime.hold.lock().unwrap().is_idle(),
            "and the mechanism holds nothing once the take has landed"
        );
        held.finish();
    }

    /// Drive one hold from keypress to end with the given recogniser, and hand
    /// back everything the person was told: the journal lines and the calls
    /// made against herdr.
    ///
    /// The hold is over the minimum and released by the deadline, so the take
    /// reaches recognition. What recognition does with it is the argument.
    fn a_hold_with_recognition(
        tag: &str,
        recognition: Recognition,
    ) -> (Vec<String>, Vec<crate::delivery::tests_support::Call>) {
        let fake = crate::delivery::tests_support::FakeDeliverer::ok();
        let (mut runtime, clock) = runtime_with_clock(fake.clone(), false);
        runtime.recognition = recognition;
        runtime.delivery_settings.toasts = true;
        let journal = std::sync::Arc::new(RecordingJournal::default());
        runtime.journal = Box::new(TestJournal(std::sync::Arc::clone(&journal)));
        let runtime = std::sync::Arc::new(runtime);
        let recorder = std::sync::Arc::new(tone_recorder(tag));
        let stop = std::sync::Arc::new(AtomicBool::new(false));
        let watcher = {
            let recorder = std::sync::Arc::clone(&recorder);
            let runtime = std::sync::Arc::clone(&runtime);
            let stop = std::sync::Arc::clone(&stop);
            thread::spawn(move || watch(recorder, runtime, stop))
        };

        answer(&request("ptt", PANE_1), &recorder, &runtime);
        clock.advance(400);
        answer(&request("ptt", PANE_1), &recorder, &runtime);
        clock.advance(1_000);

        let lines = wait_for_journal(&journal, "the take failed", WITHIN);
        wait_for_idle(&runtime);

        stop.store(true, Ordering::SeqCst);
        runtime.clock.wake();
        clock.advance(1);
        watcher.join().unwrap();
        (lines, fake.calls())
    }

    #[test]
    fn a_hold_whose_recogniser_fails_is_announced_by_the_watcher() {
        // The person holds the key, speaks, releases — and without this branch
        // nothing at all happens: the reply that would have carried the failure
        // went out to a keypress a second before the take was even stopped.
        let (lines, calls) = a_hold_with_recognition(
            "ptt-recognition-fails",
            Ok(Box::new(crate::stt::tests_support::Fake(Err(
                "the model file is not a model".to_string(),
            )))),
        );
        let failed = lines
            .iter()
            .find(|line| line.contains("the take failed"))
            .unwrap_or_else(|| panic!("the failure is recorded: {lines:?}"));
        assert!(
            failed.contains("the model file is not a model"),
            "and it names what to fix: {failed}"
        );
        assert!(
            failed.contains("w1:p1"),
            "and the pane it was held over: {failed}"
        );
        assert!(
            calls
                .iter()
                .any(|call| matches!(call, crate::delivery::tests_support::Call::Notify(_, _))),
            "and it is raised where the person is looking: {calls:?}"
        );
        assert!(
            !calls
                .iter()
                .any(|call| matches!(call, crate::delivery::tests_support::Call::Insert(_, _))),
            "nothing was delivered: {calls:?}"
        );
    }

    #[test]
    fn a_hold_with_no_recogniser_at_all_is_announced_the_same_way() {
        // `[stt] engine` misconfigured: the take is on disk and named, and the
        // person has to be told, because there is no reply to tell.
        let (lines, calls) = a_hold_with_recognition(
            "ptt-recognition-absent",
            Err("recognition unavailable: no model is configured".to_string()),
        );
        let failed = lines
            .iter()
            .find(|line| line.contains("the take failed"))
            .unwrap_or_else(|| panic!("the failure is recorded: {lines:?}"));
        assert!(
            failed.contains("no model is configured"),
            "and it names what to fix: {failed}"
        );
        assert!(
            failed.contains(".wav"),
            "and where the take is, since it was kept: {failed}"
        );
        assert_eq!(
            calls.len(),
            1,
            "one failure, one toast, and no delivery: {calls:?}"
        );
    }

    #[test]
    fn repeats_spread_over_time_keep_the_hold_alive_past_the_release_gap() {
        // The pure test in `src/ptt.rs` proves the arithmetic. This one proves
        // the daemon: a watcher on its own thread, repeats arriving four
        // hundred milliseconds apart, and a hold that survives a total span
        // twice the release gap because no single silence reached it.
        let fake = crate::delivery::tests_support::FakeDeliverer::ok();
        let (mut runtime, clock) = runtime_with_clock(fake.clone(), false);
        let journal = std::sync::Arc::new(RecordingJournal::default());
        runtime.journal = Box::new(TestJournal(std::sync::Arc::clone(&journal)));
        let runtime = std::sync::Arc::new(runtime);
        let recorder = std::sync::Arc::new(tone_recorder("ptt-long-hold"));
        let stop = std::sync::Arc::new(AtomicBool::new(false));
        let watcher = {
            let recorder = std::sync::Arc::clone(&recorder);
            let runtime = std::sync::Arc::clone(&runtime);
            let stop = std::sync::Arc::clone(&stop);
            thread::spawn(move || watch(recorder, runtime, stop))
        };

        answer(&request("ptt", PANE_1), &recorder, &runtime);
        // Five gaps of 400 ms: two seconds in all, twice the one-second
        // release gap, and not one of them long enough to be a release.
        for round in 1..=5 {
            clock.advance(400);
            answer(&request("ptt", PANE_1), &recorder, &runtime);
            assert!(
                matches!(
                    &*runtime.hold.lock().unwrap(),
                    crate::ptt::HoldState::Live(_)
                ),
                "the key is still down after {round} gaps of 400 ms"
            );
            assert_eq!(
                runtime.hold.lock().unwrap().hold().unwrap().pokes,
                round + 1,
                "and it is the same hold, counting every repeat, not a new one"
            );
            assert!(
                fake.calls().is_empty(),
                "and nothing has been delivered yet: {:?}",
                fake.calls()
            );
        }

        // Now the repeats stop.
        clock.advance(1_000);
        let calls = wait_for_calls(&fake, WITHIN);
        assert!(
            matches!(
                calls.first(),
                Some(crate::delivery::tests_support::Call::Insert(pane, _)) if pane == "w1:p1"
            ),
            "one take, delivered once the repeats stopped: {calls:?}"
        );
        let lines = wait_for_journal(&journal, "released", WITHIN);
        let released = lines
            .iter()
            .find(|line| line.contains("released"))
            .expect("checked above");
        assert!(
            released.contains("2000 ms") && released.contains("6 repeats"),
            "the hold lasted the whole span and counted every repeat: {released}"
        );
        assert!(
            !lines.iter().any(|line| line.contains("too short")),
            "and no gap inside it was ever taken for a release: {lines:?}"
        );
        assert_eq!(
            calls.len(),
            1,
            "one hold, one take, one delivery: {calls:?}"
        );
        wait_for_idle(&runtime);

        stop.store(true, Ordering::SeqCst);
        runtime.clock.wake();
        clock.advance(1);
        watcher.join().unwrap();
    }

    #[test]
    fn every_ptt_line_names_what_happened_and_what_to_do() {
        let lines = vec![
            too_short_line("w1:p1", 120, 300),
            kept_on_shutdown_line("w1:p1", "/takes/1789-1-2.wav"),
            shutdown_lost_line("w1:p1", "the device went away"),
        ];
        for line in lines {
            assert!(!line.is_empty());
            assert!(line.contains("w1:p1"), "names the pane: {line}");
        }
        assert!(
            too_short_line("w1:p1", 120, 300).contains("120"),
            "a tap names its own length, so the minimum can be judged"
        );
        assert!(
            kept_on_shutdown_line("w1:p1", "/takes/1789-1-2.wav").contains("/takes/1789-1-2.wav"),
            "a kept take names where it is, or it is lost in practice"
        );
    }

    #[test]
    fn a_tap_raises_a_toast_so_it_is_not_mistaken_for_a_broken_plugin() {
        let fake = crate::delivery::tests_support::FakeDeliverer::ok();
        let (mut runtime, clock) = runtime_with_clock(fake.clone(), false);
        // `runtime_with` builds with toasts off; this case is about them being on.
        runtime.delivery_settings.toasts = true;
        let runtime = std::sync::Arc::new(runtime);
        let recorder = std::sync::Arc::new(tone_recorder("ptt-tap-toast"));
        let stop = std::sync::Arc::new(AtomicBool::new(false));
        let watcher = {
            let recorder = std::sync::Arc::clone(&recorder);
            let runtime = std::sync::Arc::clone(&runtime);
            let stop = std::sync::Arc::clone(&stop);
            thread::spawn(move || watch(recorder, runtime, stop))
        };

        answer(&request("ptt", PANE_1), &recorder, &runtime);
        clock.advance(1_000);
        let calls = wait_for_calls(&fake, std::time::Duration::from_secs(5));

        assert!(
            calls
                .iter()
                .any(|call| matches!(call, crate::delivery::tests_support::Call::Notify(_, _))),
            "a tap is announced, or it is indistinguishable from a broken plugin: {calls:?}"
        );
        assert!(
            !calls
                .iter()
                .any(|call| matches!(call, crate::delivery::tests_support::Call::Insert(_, _))),
            "and it delivers nothing: {calls:?}"
        );

        stop.store(true, Ordering::SeqCst);
        runtime.clock.wake();
        clock.advance(1);
        watcher.join().unwrap();
    }

    #[test]
    fn a_tap_with_toasts_off_still_writes_the_journal_line() {
        let fake = crate::delivery::tests_support::FakeDeliverer::ok();
        let (mut runtime, clock) = runtime_with_clock(fake.clone(), false);
        // Toasts off is the default this helper builds; stated rather than
        // implied, because the whole point of the test is that key's value.
        runtime.delivery_settings.toasts = false;
        let journal = std::sync::Arc::new(RecordingJournal::default());
        runtime.journal = Box::new(TestJournal(std::sync::Arc::clone(&journal)));
        let runtime = std::sync::Arc::new(runtime);
        let recorder = std::sync::Arc::new(tone_recorder("ptt-tap-quiet"));
        let stop = std::sync::Arc::new(AtomicBool::new(false));
        let watcher = {
            let recorder = std::sync::Arc::clone(&recorder);
            let runtime = std::sync::Arc::clone(&runtime);
            let stop = std::sync::Arc::clone(&stop);
            thread::spawn(move || watch(recorder, runtime, stop))
        };

        answer(&request("ptt", PANE_1), &recorder, &runtime);
        clock.advance(1_000);
        // The line is what this test is about: waiting for it is what proves
        // `[ui] toasts` decides interruption and never whether a failure is
        // recorded at all.
        wait_for_journal(&journal, "too short", WITHIN);
        wait_for_idle(&runtime);

        assert!(
            fake.calls().is_empty(),
            "no toast when toasts are off: {:?}",
            fake.calls()
        );

        stop.store(true, Ordering::SeqCst);
        runtime.clock.wake();
        clock.advance(1);
        watcher.join().unwrap();
    }
    #[test]
    fn a_repeat_is_served_fast_enough_to_sustain_twelve_a_second() {
        let runtime = fake_runtime("a transcript");
        let recorder = tone_recorder("ptt-rate");
        let request = request("ptt", PANE_1);
        answer(&request, &recorder, &runtime);
        let started = std::time::Instant::now();
        let repeats = 120;
        for _ in 0..repeats {
            answer(&request, &recorder, &runtime);
        }
        let each = started.elapsed() / repeats;
        // Twelve a second is one every ~83 ms. A bound of 8 ms is an order of
        // magnitude of headroom and still fails loudly if a repeat ever starts
        // doing real work — a file write, an allocation that grows with the hold.
        assert!(
            each < std::time::Duration::from_millis(8),
            "a repeat took {each:?}; twelve a second needs one every 83 ms"
        );
        eprintln!("repeat served in {each:?}");
    }

    #[test]
    fn a_hold_publishes_recording_with_the_pane_and_the_tab_it_was_pinned_to() {
        let (runtime, _clock) = fake_runtime_with_clock("a transcript");
        let recorder = tone_recorder("activity-recording");
        answer(
            &request("ptt", br#"{"focused_pane_id":"w1:p1","tab_id":"w1:t1"}"#),
            &recorder,
            &runtime,
        );
        let activity = runtime.activity.lock().unwrap();
        match &*activity {
            Activity::Recording { target, tab, .. } => {
                assert_eq!(target, "w1:p1");
                assert_eq!(tab.as_deref(), Some("w1:t1"));
            }
            other => panic!("expected Recording, got {other:?}"),
        }
    }

    #[test]
    fn a_toggle_take_publishes_the_same_states_as_a_hold() {
        // dictate's first half publishes Recording; its second half runs the
        // same pipeline, so the stages come from the same code.
        let (runtime, _clock) = fake_runtime_with_clock("a transcript");
        let recorder = tone_recorder("activity-toggle");
        let request = request(
            "dictate",
            br#"{"focused_pane_id":"w1:p2","tab_id":"w1:t2"}"#,
        );
        answer(&request, &recorder, &runtime);
        assert!(matches!(
            &*runtime.activity.lock().unwrap(),
            Activity::Recording { .. }
        ));
        answer(&request, &recorder, &runtime);
        assert!(
            matches!(&*runtime.activity.lock().unwrap(), Activity::Idle),
            "a finished take publishes Idle, or the token never lapses"
        );
    }

    /// What the drawing thread would read right now.
    fn activity_of(runtime: &Runtime) -> Activity {
        runtime.activity.lock().unwrap().clone()
    }

    #[test]
    fn a_tap_too_short_to_be_a_hold_leaves_the_indicator_saying_nothing_is_recording() {
        // The sharpest of the four. A tap runs no pipeline at all, so nothing
        // downstream can publish the end of it: if the one line in
        // `discard_take` goes, the indicator says REC for the rest of the
        // daemon's life, on a keypress that produced no take.
        let fake = crate::delivery::tests_support::FakeDeliverer::ok();
        let (mut runtime, clock) = runtime_with_clock(fake.clone(), false);
        let journal = std::sync::Arc::new(RecordingJournal::default());
        runtime.journal = Box::new(TestJournal(std::sync::Arc::clone(&journal)));
        let runtime = std::sync::Arc::new(runtime);
        let recorder = std::sync::Arc::new(tone_recorder("activity-tap"));
        let stop = std::sync::Arc::new(AtomicBool::new(false));
        let watcher = {
            let recorder = std::sync::Arc::clone(&recorder);
            let runtime = std::sync::Arc::clone(&runtime);
            let stop = std::sync::Arc::clone(&stop);
            thread::spawn(move || watch(recorder, runtime, stop))
        };

        answer(&request("ptt", PANE_1), &recorder, &runtime);
        assert!(
            matches!(activity_of(&runtime), Activity::Recording { .. }),
            "the keypress published Recording, or this test proves nothing"
        );
        // One repeat and nothing more: held for 0 ms, far under the minimum.
        clock.advance(1_000);
        wait_for_journal(&journal, "too short", WITHIN);
        // The hold going idle is the watcher having finished with the tap, and
        // `finish_ending` runs after `discard_take`: no test here sleeps.
        wait_for_idle(&runtime);
        assert_eq!(
            activity_of(&runtime),
            Activity::Idle,
            "a tap is a take that ended before it began"
        );

        stop.store(true, Ordering::SeqCst);
        runtime.clock.wake();
        clock.advance(1);
        watcher.join().unwrap();
    }

    #[test]
    fn a_hold_whose_device_went_away_leaves_the_indicator_saying_nothing_is_recording() {
        // `end_take`'s failing half: there is no take, so nothing downstream
        // publishes the end of this one either.
        let fake = crate::delivery::tests_support::FakeDeliverer::ok();
        let (mut runtime, clock) = runtime_with_clock(fake.clone(), false);
        let journal = std::sync::Arc::new(RecordingJournal::default());
        runtime.journal = Box::new(TestJournal(std::sync::Arc::clone(&journal)));
        let runtime = std::sync::Arc::new(runtime);
        let recorder = std::sync::Arc::new(losing_recorder("activity-lost"));
        let stop = std::sync::Arc::new(AtomicBool::new(false));
        let watcher = {
            let recorder = std::sync::Arc::clone(&recorder);
            let runtime = std::sync::Arc::clone(&runtime);
            let stop = std::sync::Arc::clone(&stop);
            thread::spawn(move || watch(recorder, runtime, stop))
        };

        answer(&request("ptt", PANE_1), &recorder, &runtime);
        assert!(matches!(activity_of(&runtime), Activity::Recording { .. }));
        clock.advance(400);
        answer(&request("ptt", PANE_1), &recorder, &runtime);
        clock.advance(1_000);
        wait_for_journal(&journal, "the device went away", WITHIN);
        wait_for_idle(&runtime);
        assert_eq!(
            activity_of(&runtime),
            Activity::Idle,
            "a take whose device went away is still a take that ended"
        );

        stop.store(true, Ordering::SeqCst);
        runtime.clock.wake();
        clock.advance(1);
        watcher.join().unwrap();
    }

    #[test]
    fn a_daemon_stopped_with_a_key_still_down_leaves_the_indicator_idle() {
        // The drawing thread is joined after the watcher, so its last look must
        // not find a take still running: a decoration it then held would be put
        // back by nothing.
        let fake = crate::delivery::tests_support::FakeDeliverer::ok();
        let (runtime, clock) = runtime_with_clock(fake.clone(), false);
        let runtime = std::sync::Arc::new(runtime);
        let recorder = std::sync::Arc::new(tone_recorder("activity-shutdown"));
        let stop = std::sync::Arc::new(AtomicBool::new(false));
        let watcher = {
            let recorder = std::sync::Arc::clone(&recorder);
            let runtime = std::sync::Arc::clone(&runtime);
            let stop = std::sync::Arc::clone(&stop);
            thread::spawn(move || watch(recorder, runtime, stop))
        };

        answer(&request("ptt", PANE_1), &recorder, &runtime);
        assert!(matches!(activity_of(&runtime), Activity::Recording { .. }));
        // The key is still down — no release, no deadline — and the daemon
        // stops anyway.
        stop.store(true, Ordering::SeqCst);
        runtime.clock.wake();
        clock.advance(1);
        watcher.join().unwrap();
        assert_eq!(
            activity_of(&runtime),
            Activity::Idle,
            "abandoned is one of the ways a take ends"
        );
    }

    #[test]
    fn a_toggle_whose_take_could_not_be_stopped_leaves_the_indicator_idle() {
        // `dictate`'s `AlreadyRunning` arm with a recorder that refuses the
        // take. The take is silence, so stopping it fails, and the second
        // keypress reaches neither the pipeline nor anything else that
        // publishes an end.
        let runtime = fake_runtime_with_clock("unused").0;
        let recorder = silent_recorder();
        let request = request("dictate", br#"{"focused_pane_id":"w1:p2"}"#);
        answer(&request, &recorder, &runtime);
        assert!(matches!(activity_of(&runtime), Activity::Recording { .. }));
        let (reply, _) = answer(&request, &recorder, &runtime);
        assert!(
            matches!(&reply, Reply::Error(text) if text.contains("dB")),
            "the silent take is refused, which is what puts this on the failing arm: {reply:?}"
        );
        assert_eq!(
            activity_of(&runtime),
            Activity::Idle,
            "a take the recorder would not give up is still a take that ended"
        );
    }

    #[test]
    fn a_take_that_fails_still_ends_at_idle() {
        // A recognition failure must not leave the indicator saying TRANSCR
        // forever: the token would go on being renewed by a thread that thinks
        // work is in progress, and the tab would stay decorated.
        let mut runtime = fake_runtime_with_clock("unused").0;
        runtime.recognition = Err("no engine".to_string());
        let runtime = std::sync::Arc::new(runtime);
        let recorder = tone_recorder("activity-failed");
        let request = request(
            "dictate",
            br#"{"focused_pane_id":"w1:p3","tab_id":"w1:t3"}"#,
        );
        answer(&request, &recorder, &runtime);
        answer(&request, &recorder, &runtime);
        let activity = runtime.activity.lock().unwrap().clone();
        assert!(
            matches!(activity, Activity::Idle),
            "a take that failed is still a take that ended: {activity:?}"
        );
    }
}
