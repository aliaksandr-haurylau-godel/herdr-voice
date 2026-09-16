//! What the indicator says.
//!
//! Three states, two forms each: the steady form and the blink form, which is
//! the steady form without its second glyph. The token alternates between them
//! on every tick — that is the blink — and the tab label always carries the
//! steady form, because a tab bar that flashes a character twice a second is
//! noise where nobody chose to look.
//!
//! Every form begins with `MARKER`, which is what the start-up sweep cuts from
//! when a previous daemon was killed with a tab still decorated.

/// The glyph every value begins with, and the one the sweep looks for. Not
/// something a person types into a tab name by accident — which matters,
/// because the sweep renames every tab whose label contains it.
pub const MARKER: &str = "🎙️";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum State {
    Recording { elapsed_ms: u64 },
    Transcribing,
    Fixing,
}

/// The value for a state, in the steady form or the blink form.
pub fn value(state: &State, blink: bool) -> String {
    let (icon, text) = match state {
        State::Recording { elapsed_ms } => ("🔴", format!("REC {}", elapsed(*elapsed_ms))),
        State::Transcribing => ("📝", "TRANSCR".to_string()),
        State::Fixing => ("🪄", "FIX".to_string()),
    };
    if blink {
        format!("{MARKER} {text}")
    } else {
        format!("{MARKER}{icon} {text}")
    }
}

/// Minutes and seconds, minutes uncapped: a take that has run for an hour says
/// so rather than reading as though it had just begun.
pub fn elapsed(ms: u64) -> String {
    let seconds = ms / 1_000;
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

/// The label with the value appended. An empty label takes no leading space, so
/// that stripping it again gives the empty label back rather than a space.
pub fn decorate(label: &str, value: &str) -> String {
    if label.is_empty() {
        value.to_string()
    } else {
        format!("{label} {value}")
    }
}

/// The label with our decoration cut off, or unchanged if it carries none.
///
/// Cuts from the first `MARKER` and takes the space before it with it. A label
/// nobody decorated is returned as it is — including one that happens to
/// contain a lone microphone that is not `MARKER`.
pub fn strip(label: &str) -> String {
    match label.find(MARKER) {
        None => label.to_string(),
        Some(at) => label[..at].trim_end().to_string(),
    }
}

/// Why a paint did not happen. Mirrors `crate::delivery::DeliveryError`: the
/// two talk to the same binary and fail in the same two ways.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PaintError {
    /// The code alone, extracted from herdr's structured refusal, or the raw
    /// output when it did not parse as that shape.
    Rejected(String),
    /// `herdr` itself could not be started.
    NotFound { binary: String, path: String },
}

impl std::fmt::Display for PaintError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PaintError::Rejected(why) => write!(f, "{why}"),
            PaintError::NotFound { binary, path } => write!(
                f,
                "cannot run {binary:?}: it is not on the PATH this process has, which is \
                 {path:?}. Set HERDR_BIN_PATH to herdr's location, or start herdr from a shell \
                 where it is on the PATH"
            ),
        }
    }
}
impl std::error::Error for PaintError {}

pub trait Painter: Send + Sync {
    /// Set the pane's token, to expire after `ttl_ms` unless renewed.
    fn token(&self, pane: &str, value: &str, ttl_ms: u64) -> Result<(), PaintError>;
    /// Rename a tab.
    fn rename(&self, tab: &str, label: &str) -> Result<(), PaintError>;
    /// Every tab herdr knows about, as (tab_id, label).
    fn tabs(&self) -> Result<Vec<(String, String)>, PaintError>;
}

/// The name the token is reported under, and the name the sidebar shows it as.
const TOKEN_NAME: &str = "voice";

/// The argument list for `herdr pane report-metadata`, built as data so a test
/// can assert on it directly rather than only through a fake that never checks
/// what a real subcommand expects.
///
/// Unlike `delivery`'s argument builders this owns its strings: two of the
/// arguments — the `name=value` token and the time to live — are composed here
/// and so cannot be borrowed from the caller.
fn token_args(pane: &str, value: &str, ttl_ms: u64) -> Vec<String> {
    vec![
        "pane".to_string(),
        "report-metadata".to_string(),
        pane.to_string(),
        "--source".to_string(),
        crate::transport::PLUGIN_ID.to_string(),
        "--token".to_string(),
        format!("{TOKEN_NAME}={value}"),
        "--ttl-ms".to_string(),
        ttl_ms.to_string(),
    ]
}

/// The argument list for `herdr tab rename`.
fn rename_args(tab: &str, label: &str) -> Vec<String> {
    vec![
        "tab".to_string(),
        "rename".to_string(),
        tab.to_string(),
        label.to_string(),
    ]
}

/// The argument list for `herdr tab list`. No `--workspace`: the sweep wants
/// every tab in every workspace, and at daemon start there is no invocation
/// context to scope it to anyway.
fn list_args() -> Vec<String> {
    vec!["tab".to_string(), "list".to_string()]
}

/// What `herdr tab list` answers with: `result.tabs[]`, each with a `tab_id`
/// and a `label`. A tab whose label is absent counts as the empty label.
#[derive(serde::Deserialize)]
struct Listing {
    result: ListingResult,
}

#[derive(serde::Deserialize)]
struct ListingResult {
    #[serde(default)]
    tabs: Vec<ListedTab>,
}

#[derive(serde::Deserialize)]
struct ListedTab {
    tab_id: String,
    #[serde(default)]
    label: String,
}

/// Paints by running the herdr binary, the way `HerdrDeliverer` delivers by
/// running it.
pub struct HerdrPainter {
    binary: String,
}

impl HerdrPainter {
    pub fn new() -> Self {
        Self::with_binary(crate::delivery::herdr_binary())
    }

    /// Points at an arbitrary program rather than reading `HERDR_BIN_PATH` from
    /// the environment, for the same reason `HerdrDeliverer::with_binary` does.
    pub fn with_binary(binary: impl Into<String>) -> Self {
        HerdrPainter {
            binary: binary.into(),
        }
    }

    /// Runs the binary and hands back what it wrote to standard output.
    fn output(&self, args: &[String]) -> Result<Vec<u8>, PaintError> {
        match std::process::Command::new(&self.binary).args(args).output() {
            // herdr starts plugin commands with a minimal PATH — the same
            // reasoning src/delivery.rs states for delivery.
            Err(_) => Err(PaintError::NotFound {
                binary: self.binary.clone(),
                path: std::env::var("PATH").unwrap_or_default(),
            }),
            Ok(output) if output.status.success() => Ok(output.stdout),
            Ok(output) => {
                let text = if !output.stdout.is_empty() {
                    output.stdout
                } else {
                    output.stderr
                };
                Err(PaintError::Rejected(
                    String::from_utf8_lossy(&text).trim().to_string(),
                ))
            }
        }
    }

    fn run(&self, args: &[String]) -> Result<(), PaintError> {
        self.output(args).map(|_| ())
    }
}

impl Default for HerdrPainter {
    fn default() -> Self {
        Self::new()
    }
}

impl Painter for HerdrPainter {
    fn token(&self, pane: &str, value: &str, ttl_ms: u64) -> Result<(), PaintError> {
        self.run(&token_args(pane, value, ttl_ms))
    }

    fn rename(&self, tab: &str, label: &str) -> Result<(), PaintError> {
        self.run(&rename_args(tab, label))
    }

    fn tabs(&self) -> Result<Vec<(String, String)>, PaintError> {
        let output = self.output(&list_args())?;
        let text = String::from_utf8_lossy(&output);
        match serde_json::from_str::<Listing>(&text) {
            Ok(listing) => Ok(listing
                .result
                .tabs
                .into_iter()
                .map(|tab| (tab.tab_id, tab.label))
                .collect()),
            // A listing that did not parse is a refusal like any other: the
            // caller records it and carries on, naming what herdr said.
            Err(why) => Err(PaintError::Rejected(format!(
                "cannot read what 'herdr tab list' answered: {why}"
            ))),
        }
    }
}

/// How many renewal intervals a token outlives. Three gives an ordinary
/// scheduling delay room to happen without the token lapsing between renewals,
/// and it is not configurable, so no pair of settings can make it flicker.
const TTL_INTERVALS: u64 = 3;

/// The drawing thread: it wakes on the renewal interval, reads what the take
/// path published, and paints.
///
/// It never writes `Activity` and never touches the recorder, which is what
/// keeps `herdr` off the keypress path. Everything it remembers — what it
/// decorated, what that label said before, what it last wrote there, and
/// whether a paint has already failed for this take — is local to this
/// function.
///
/// `clock` is its own, not the daemon's: `Clock`'s contract consumes a wake
/// with the return it causes, so two waiters on one clock steal each other's
/// wakes, and both the start of a hold and the daemon's shutdown depend on a
/// wake reaching the watcher. Elapsed time is measured with `runtime.clock`
/// instead, because that is the clock `since` was stamped by, and the two count
/// from different origins.
pub fn draw(
    painter: std::sync::Arc<dyn Painter>,
    runtime: std::sync::Arc<crate::daemon::Runtime>,
    clock: std::sync::Arc<dyn crate::ptt::Clock>,
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
) {
    // A configured zero would otherwise be a loop with no wait in it.
    let interval = runtime.ui.blink_ms.max(1);
    let mut drawn = Drawn::default();
    // One interval after the clock's own origin, not after whenever this thread
    // happened to be scheduled. `Stamp` counts from that origin, and reading it
    // here instead would make the first deadline depend on how long the spawn
    // took — which is a tick silently skipped when something moved the clock
    // while this thread was still starting.
    let mut next = interval;
    loop {
        clock.wait_until(next);
        if stop.load(std::sync::atomic::Ordering::SeqCst) {
            // The daemon is going away. A decoration it made must not outlive
            // it, whether or not the take ever published `Idle`.
            drawn.finish(painter.as_ref(), &runtime);
            return;
        }
        // The next deadline is settled before anything is painted, and is
        // carried forward rather than re-read from the clock afterwards: a
        // caller drives this thread by moving the clock and then waiting for a
        // paint, so a deadline chosen after the paint could be chosen from a
        // time the caller had already moved past, and the next wait would never
        // end.
        next = next.max(clock.now()).saturating_add(interval);
        drawn.tick(painter.as_ref(), &runtime, interval);
    }
}

/// The tab this thread decorated, and what it needs to put it back.
struct Decoration {
    tab: String,
    /// What the label said before this thread wrote anything into it. Read
    /// once, on the first tick of the take: that is the only value that is
    /// certainly the person's.
    original: String,
    /// What this thread last wrote there, or nothing when it has written
    /// nothing yet. Both the "rename only when it would differ" rule and the
    /// restore compare against this.
    written: Option<String>,
}

/// How far the start-up sweep has got.
///
/// The sweep takes this plugin's own suffix off any tab a daemon that was
/// killed left decorated. It is the only recovery from that, so a failure is
/// worth one more attempt — and exactly one, because a retry with no bound is a
/// subprocess every interval for the life of a daemon whose herdr never answers.
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
enum Sweep {
    /// Not attempted yet: the daemon has only just started.
    #[default]
    Due,
    /// Attempted, and it failed. One more attempt is owed.
    Owed,
    /// Nothing further to do: it succeeded, or its one retry is spent.
    Settled,
}

/// What the drawing thread remembers between ticks.
#[derive(Default)]
struct Drawn {
    decorated: Option<Decoration>,
    sweep: Sweep,
    /// Some call to herdr has come back successfully since the daemon started.
    /// The sweep's retry waits for this: without it, a daemon started where
    /// herdr does not answer would spend its second attempt immediately and on
    /// nothing.
    answered: bool,
    /// Which form the next token takes. The tab never carries the blink form.
    blink: bool,
    /// A paint has already failed for this take and been recorded. Renewal
    /// carries on: a token that stopped being renewed because one call failed
    /// would say the take was over while it was still running, and the
    /// indicator is the whole point. Recording once rather than once a tick is
    /// what keeps a broken herdr from filling the journal.
    reported: bool,
}

impl Drawn {
    fn tick(&mut self, painter: &dyn Painter, runtime: &crate::daemon::Runtime, interval: u64) {
        if self.sweep == Sweep::Due {
            // Before this tick reads the activity or paints anything: whatever
            // is on the tabs now was left by a previous daemon, because this
            // one has decorated nothing yet. Nothing of its own can be stripped
            // by mistake, however early in the daemon's life a take begins.
            self.sweep_once(painter, runtime, false);
        }
        let activity = {
            let held = runtime
                .activity
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            held.clone()
        };
        let (target, tab, state) = match activity {
            crate::daemon::Activity::Idle => {
                self.finish(painter, runtime);
                // Nothing is being recorded and, after the restore, nothing is
                // decorated: the one tick a failed sweep may be retried on.
                // During a take it would strip a decoration this thread had
                // just written, and the restore would then find a label it did
                // not write and leave the tab alone.
                if self.sweep == Sweep::Owed && self.decorated.is_none() && self.answered {
                    self.sweep_once(painter, runtime, true);
                }
                return;
            }
            crate::daemon::Activity::Recording { target, tab, since } => (
                target,
                tab,
                State::Recording {
                    // The take's clock, never this thread's: `since` was
                    // stamped by that one, and subtracting across two origins
                    // is arithmetic on two different time bases.
                    elapsed_ms: runtime.clock.now().saturating_sub(since),
                },
            ),
            crate::daemon::Activity::Working { target, tab, stage } => (
                target,
                tab,
                match stage {
                    crate::daemon::Stage::Transcribing => State::Transcribing,
                    crate::daemon::Stage::Fixing => State::Fixing,
                },
            ),
        };
        // A take in a different tab than the one still decorated: that take is
        // over, whether or not anything ever said so, and its tab goes back
        // before this one is touched.
        if self
            .decorated
            .as_ref()
            .is_some_and(|held| Some(held.tab.as_str()) != tab.as_deref())
        {
            self.finish(painter, runtime);
        }
        if runtime.ui.sidebar_token {
            let value = value(&state, self.blink);
            let ttl = interval.saturating_mul(TTL_INTERVALS);
            if let Err(why) = self.answered(painter.token(&target, &value, ttl)) {
                self.report(runtime, &why);
            }
        }
        // Flipped whether or not the token was written, so that switching the
        // sidebar off does not also decide what the blink would have been.
        self.blink = !self.blink;
        if runtime.ui.tab_indicator {
            if let Some(tab) = tab.as_deref() {
                self.paint_tab(painter, runtime, tab, &state);
            }
        }
    }

    /// The tab carries the steady form, and is renamed only when that form
    /// differs from the one last written. In `REC` that comes out as once a
    /// second, because the clock is in the string; in the other two states as
    /// once, on entering them.
    fn paint_tab(
        &mut self,
        painter: &dyn Painter,
        runtime: &crate::daemon::Runtime,
        tab: &str,
        state: &State,
    ) {
        if self.decorated.is_none() {
            match self.answered(painter.tabs()) {
                Ok(tabs) => {
                    let original = tabs
                        .iter()
                        .find(|(id, _)| id == tab)
                        .map(|(_, label)| label.clone())
                        .unwrap_or_default();
                    self.decorated = Some(Decoration {
                        tab: tab.to_string(),
                        original,
                        written: None,
                    });
                }
                Err(why) => {
                    self.report(runtime, &why);
                    return;
                }
            }
        }
        let Some(held) = self.decorated.as_ref() else {
            return;
        };
        let steady = decorate(&held.original, &value(state, false));
        if held.written.as_deref() == Some(steady.as_str()) {
            return;
        }
        match self.answered(painter.rename(tab, &steady)) {
            Ok(()) => {
                if let Some(held) = self.decorated.as_mut() {
                    held.written = Some(steady);
                }
            }
            Err(why) => self.report(runtime, &why),
        }
    }

    /// The take is over: put the tab back, and forget it.
    ///
    /// The label is read again and compared with what this thread last wrote
    /// there. Equal means nothing else has touched it, so the original goes
    /// back — an empty original included, which is a value to restore and not a
    /// reason to skip. Different means somebody renamed the tab during the
    /// take, so it is left alone and that is recorded.
    ///
    /// A paint having failed earlier does not stop this: a decoration already
    /// made is exactly what a broken herdr would otherwise leave on a tab
    /// forever.
    fn finish(&mut self, painter: &dyn Painter, runtime: &crate::daemon::Runtime) {
        self.reported = false;
        self.blink = false;
        let Some(held) = self.decorated.take() else {
            return;
        };
        let Some(written) = held.written else {
            return;
        };
        let listed = match self.answered(painter.tabs()) {
            Ok(tabs) => tabs,
            Err(why) => {
                runtime.journal.write(&paint_failed_line(&why.to_string()));
                return;
            }
        };
        let now = listed
            .iter()
            .find(|(id, _)| *id == held.tab)
            .map(|(_, label)| label.clone())
            .unwrap_or_default();
        if now != written {
            runtime.journal.write(&renamed_elsewhere_line(&held.tab));
            return;
        }
        if let Err(why) = self.answered(painter.rename(&held.tab, &held.original)) {
            runtime.journal.write(&paint_failed_line(&why.to_string()));
        }
    }

    /// Take the suffix off every tab that still carries the marker, and settle
    /// what becomes of the attempt that is owed after this one.
    ///
    /// `retry` says which of the two attempts this is. It changes nothing about
    /// the work, only what the journal says when the work fails: the first
    /// attempt says another is coming, the second says none is.
    fn sweep_once(&mut self, painter: &dyn Painter, runtime: &crate::daemon::Runtime, retry: bool) {
        match self.sweep_tabs(painter) {
            Ok(()) => self.sweep = Sweep::Settled,
            Err(why) => {
                runtime
                    .journal
                    .write(&sweep_failed_line(&why.to_string(), retry));
                self.sweep = if retry { Sweep::Settled } else { Sweep::Owed };
            }
        }
    }

    /// One pass over every tab herdr knows about. A tab herdr refuses to rename
    /// does not cost the rest of the list its sweep, so the pass runs to the
    /// end and answers with the first refusal it met.
    fn sweep_tabs(&mut self, painter: &dyn Painter) -> Result<(), PaintError> {
        let mut refused = None;
        for (tab, label) in self.answered(painter.tabs())? {
            if !label.contains(MARKER) {
                continue;
            }
            if let Err(why) = self.answered(painter.rename(&tab, &strip(&label))) {
                refused.get_or_insert(why);
            }
        }
        match refused {
            None => Ok(()),
            Some(why) => Err(why),
        }
    }

    /// Pass a call's outcome through, noting whether herdr answered at all.
    fn answered<T>(&mut self, outcome: Result<T, PaintError>) -> Result<T, PaintError> {
        if outcome.is_ok() {
            self.answered = true;
        }
        outcome
    }

    /// The first failed paint of a take is recorded; the rest are not.
    fn report(&mut self, runtime: &crate::daemon::Runtime, why: &PaintError) {
        if self.reported {
            return;
        }
        self.reported = true;
        runtime.journal.write(&paint_failed_line(&why.to_string()));
    }
}

fn paint_failed_line(why: &str) -> String {
    format!(
        "indicator: cannot draw ({}); the take itself is unaffected — check that herdr answers, \
         or switch the indicator off with [ui] sidebar_token and tab_indicator",
        why.replace('\n', " ")
    )
}

fn sweep_failed_line(why: &str, last: bool) -> String {
    let next = if last {
        "it will not be tried again, so a tab that still reads as though a take were running \
         has to be renamed by hand or the daemon restarted"
    } else {
        "it will be tried once more, at the first moment nothing is being recorded"
    };
    format!(
        "indicator: cannot take off what an earlier run left on the tab labels ({}); {next}",
        why.replace('\n', " ")
    )
}

fn renamed_elsewhere_line(tab: &str) -> String {
    format!(
        "indicator: tab {tab} was renamed while the take ran, so its label is left as it is \
         rather than overwritten with what it said before"
    )
}

#[cfg(test)]
pub mod tests_support {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum Paint {
        Token(String, String, u64),
        Rename(String, String),
        Tabs,
    }

    #[derive(Default)]
    struct Inner {
        calls: Vec<Paint>,
        /// What `tabs()` answers, and what `rename` writes into.
        labels: Vec<(String, String)>,
        /// Every paint fails while this is set.
        failing: bool,
        /// Only `tabs()` fails while this is set.
        tabs_failing: bool,
        /// Renaming any tab named here fails; the rest succeed.
        refused_renames: Vec<String>,
    }

    /// Records every call in order and answers what it was told to.
    ///
    /// `Clone`, sharing everything through one inner `Arc`, the way
    /// `FakeDeliverer` shares its log, so a test can keep one clone while
    /// another is moved into the drawing thread.
    #[derive(Clone, Default)]
    pub struct RecordingPainter(Arc<Mutex<Inner>>);

    /// What a told-to-fail painter answers with.
    fn refusal() -> PaintError {
        PaintError::Rejected("the painter was told to fail".to_string())
    }

    impl RecordingPainter {
        /// Everything succeeds; `tabs()` answers with nothing.
        pub fn ok() -> Self {
            Self::default()
        }

        /// Everything succeeds; `tabs()` answers with these.
        pub fn with_tabs(tabs: &[(&str, &str)]) -> Self {
            let painter = Self::default();
            painter.0.lock().unwrap().labels = tabs
                .iter()
                .map(|(tab, label)| (tab.to_string(), label.to_string()))
                .collect();
            painter
        }

        /// Every paint fails from the start.
        pub fn failing() -> Self {
            let painter = Self::default();
            painter.start_failing();
            painter
        }

        /// From now on every paint fails.
        pub fn start_failing(&self) {
            self.0.lock().unwrap().failing = true;
        }

        /// From now on they succeed again.
        pub fn stop_failing(&self) {
            let mut inner = self.0.lock().unwrap();
            inner.failing = false;
            inner.tabs_failing = false;
        }

        /// From now on only `tabs()` fails.
        pub fn fail_tabs(&self) {
            self.0.lock().unwrap().tabs_failing = true;
        }

        /// From now on renaming this one tab fails and every other tab is
        /// renamed as usual — a tab herdr will not accept a new label for,
        /// among tabs it will.
        pub fn refuse_rename_of(&self, tab: &str) {
            self.0.lock().unwrap().refused_renames.push(tab.to_string());
        }

        /// Change a label behind the painter's back, as a person renaming a tab
        /// would. Adds no call to the log.
        pub fn set_label(&self, tab: &str, label: &str) {
            let mut inner = self.0.lock().unwrap();
            match inner.labels.iter_mut().find(|(id, _)| id == tab) {
                Some(entry) => entry.1 = label.to_string(),
                None => inner.labels.push((tab.to_string(), label.to_string())),
            }
        }

        pub fn calls(&self) -> Vec<Paint> {
            self.0.lock().unwrap().calls.clone()
        }

        pub fn total_calls(&self) -> usize {
            self.0.lock().unwrap().calls.len()
        }

        /// Every rename, as (tab, label), in order.
        pub fn renames(&self) -> Vec<(String, String)> {
            self.calls()
                .into_iter()
                .filter_map(|call| match call {
                    Paint::Rename(tab, label) => Some((tab, label)),
                    _ => None,
                })
                .collect()
        }

        /// Every token value, in order.
        pub fn tokens(&self) -> Vec<String> {
            self.calls()
                .into_iter()
                .filter_map(|call| match call {
                    Paint::Token(_, value, _) => Some(value),
                    _ => None,
                })
                .collect()
        }

        /// How many times `tabs()` was called.
        pub fn tab_listings(&self) -> usize {
            self.calls()
                .into_iter()
                .filter(|call| matches!(call, Paint::Tabs))
                .count()
        }
    }

    impl Painter for RecordingPainter {
        fn token(&self, pane: &str, value: &str, ttl_ms: u64) -> Result<(), PaintError> {
            // Record first, then answer: a test asserting on a failed call's
            // arguments needs the call to be in the log, and the drawing tests
            // wait on the call count rather than on a duration, so a painter
            // that failed without recording would hang them.
            let mut inner = self.0.lock().unwrap();
            inner
                .calls
                .push(Paint::Token(pane.to_string(), value.to_string(), ttl_ms));
            if inner.failing {
                return Err(refusal());
            }
            Ok(())
        }

        fn rename(&self, tab: &str, label: &str) -> Result<(), PaintError> {
            let mut inner = self.0.lock().unwrap();
            inner
                .calls
                .push(Paint::Rename(tab.to_string(), label.to_string()));
            if inner.failing || inner.refused_renames.iter().any(|id| id == tab) {
                // A rename herdr refused changed no label, so neither does this.
                return Err(refusal());
            }
            match inner.labels.iter_mut().find(|(id, _)| id == tab) {
                Some(entry) => entry.1 = label.to_string(),
                None => inner.labels.push((tab.to_string(), label.to_string())),
            }
            Ok(())
        }

        fn tabs(&self) -> Result<Vec<(String, String)>, PaintError> {
            let mut inner = self.0.lock().unwrap();
            inner.calls.push(Paint::Tabs);
            if inner.failing || inner.tabs_failing {
                return Err(refusal());
            }
            Ok(inner.labels.clone())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ptt::Clock;

    #[test]
    fn the_three_states_read_as_the_owner_settled_them() {
        assert_eq!(
            value(&State::Recording { elapsed_ms: 5_000 }, false),
            "🎙️🔴 REC 0:05"
        );
        assert_eq!(value(&State::Transcribing, false), "🎙️📝 TRANSCR");
        assert_eq!(value(&State::Fixing, false), "🎙️🪄 FIX");
    }

    #[test]
    fn the_blink_form_drops_the_second_glyph_and_nothing_else() {
        assert_eq!(
            value(&State::Recording { elapsed_ms: 5_000 }, true),
            "🎙️ REC 0:05"
        );
        assert_eq!(value(&State::Transcribing, true), "🎙️ TRANSCR");
        assert_eq!(value(&State::Fixing, true), "🎙️ FIX");
    }

    #[test]
    fn both_forms_begin_with_the_marker_the_sweep_cuts_from() {
        for blink in [false, true] {
            for state in [
                State::Recording { elapsed_ms: 0 },
                State::Transcribing,
                State::Fixing,
            ] {
                assert!(
                    value(&state, blink).starts_with(MARKER),
                    "the sweep finds nothing without it: {:?} blink={blink}",
                    state
                );
            }
        }
    }

    #[test]
    fn the_clock_is_minutes_and_seconds_and_does_not_wrap() {
        assert_eq!(elapsed(0), "0:00");
        assert_eq!(elapsed(5_000), "0:05");
        assert_eq!(elapsed(65_000), "1:05");
        assert_eq!(elapsed(600_000), "10:00");
        // A hold nobody ended must not read as though it had just begun.
        assert_eq!(elapsed(3_600_000), "60:00");
    }

    #[test]
    fn decorating_and_stripping_are_inverses() {
        for original in ["1", "", "review", "a name with spaces", "1 🎙 not ours"] {
            let decorated = decorate(original, "🎙️🔴 REC 0:05");
            assert_eq!(
                strip(&decorated),
                original,
                "round trip failed for {original:?}"
            );
        }
    }

    #[test]
    fn stripping_a_label_nobody_decorated_leaves_it_alone() {
        assert_eq!(strip("1"), "1");
        assert_eq!(strip(""), "");
        assert_eq!(strip("[thing] name"), "[thing] name");
    }

    #[test]
    fn an_empty_label_decorates_without_a_leading_space() {
        assert_eq!(decorate("", "🎙️🪄 FIX"), "🎙️🪄 FIX");
        assert_eq!(strip("🎙️🪄 FIX"), "");
    }

    #[test]
    fn the_token_arguments_are_what_herdr_expects() {
        assert_eq!(
            token_args("w1:p1", "🎙️🔴 REC 0:05", 1800),
            vec![
                "pane",
                "report-metadata",
                "w1:p1",
                "--source",
                "haurylau.voice",
                "--token",
                "voice=🎙️🔴 REC 0:05",
                "--ttl-ms",
                "1800",
            ]
        );
    }

    #[test]
    fn the_rename_and_list_arguments_are_what_herdr_expects() {
        assert_eq!(
            rename_args("w1:t1", "1 🎙️🪄 FIX"),
            vec!["tab", "rename", "w1:t1", "1 🎙️🪄 FIX"]
        );
        assert_eq!(list_args(), vec!["tab", "list"]);
    }

    #[test]
    fn a_recording_painter_records_in_order_and_answers_what_it_was_told_to() {
        let painter = tests_support::RecordingPainter::ok();
        painter.token("w1:p1", "🎙️🔴 REC 0:00", 1800).unwrap();
        painter.rename("w1:t1", "1 🎙️🔴 REC 0:00").unwrap();
        assert_eq!(
            painter.calls(),
            vec![
                tests_support::Paint::Token("w1:p1".into(), "🎙️🔴 REC 0:00".into(), 1800),
                tests_support::Paint::Rename("w1:t1".into(), "1 🎙️🔴 REC 0:00".into()),
            ]
        );
    }

    /// A stand-in `herdr`, for the tests that run `HerdrPainter` itself rather
    /// than a fake in its place.
    ///
    /// The same shape `src/delivery.rs` uses for its recorder script, and for
    /// the same reason: the parse, the exit-code classification and the process
    /// construction are only proved by a process that actually runs. The answer
    /// is written to a file and the script prints that file rather than
    /// carrying the JSON inline, so no shell or `cmd.exe` quoting rule ever
    /// touches the text under test.
    struct FakeHerdr {
        dir: std::path::PathBuf,
        script: std::path::PathBuf,
        answer: std::path::PathBuf,
        /// Where the script writes the arguments it was called with.
        argv: std::path::PathBuf,
    }

    /// Nothing else removes the scratch directory, and a suite that leaves one
    /// behind on every run is one somebody eventually finds confusing.
    impl Drop for FakeHerdr {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    impl FakeHerdr {
        /// Creates the directory and hands ownership of it over in the same
        /// step, so nothing fallible runs between the directory existing and
        /// there being a `Drop` to remove it.
        fn new(tag: &str, script_name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!(
                "herdr-voice-indicator-herdr-{tag}-{}",
                std::process::id()
            ));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).expect("scratch dir");
            FakeHerdr {
                script: dir.join(script_name),
                answer: dir.join("answer.json"),
                argv: dir.join("argv.out"),
                dir,
            }
        }

        fn binary(&self) -> String {
            self.script.to_string_lossy().into_owned()
        }

        fn painter(&self) -> HerdrPainter {
            HerdrPainter::with_binary(self.binary())
        }

        /// The arguments the script was called with, one per line.
        fn argv(&self) -> Vec<String> {
            std::fs::read_to_string(&self.argv)
                .expect("the script must have run and written its argv")
                .lines()
                .map(|line| line.to_string())
                .collect()
        }
    }

    /// A `herdr` that prints `answer` and exits with `code`.
    #[cfg(unix)]
    fn fake_herdr(tag: &str, answer: &str, code: i32) -> FakeHerdr {
        let fake = FakeHerdr::new(tag, "herdr.sh");
        std::fs::write(&fake.answer, answer).expect("write the answer");
        std::fs::write(
            &fake.script,
            format!(
                "#!/bin/sh\nprintf '%s\\n' \"$@\" > {:?}\ncat {:?}\nexit {code}\n",
                fake.argv, fake.answer
            ),
        )
        .expect("write the script");
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&fake.script).expect("stat").permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&fake.script, perms).expect("chmod");
        fake
    }

    /// Not run on this machine. `type` prints the answer file byte for byte, so
    /// the JSON never passes through `cmd.exe`'s quoting, and `exit /b` sets the
    /// exit code `Command::output` reads. The argv loop is the one
    /// `src/delivery.rs` documents, with the same limits: an argument carrying
    /// a percent sign, an embedded quote, a comma or a semicolon would very
    /// likely come out split or mangled, and nothing these tests pass does.
    #[cfg(windows)]
    fn fake_herdr(tag: &str, answer: &str, code: i32) -> FakeHerdr {
        let fake = FakeHerdr::new(tag, "herdr.cmd");
        std::fs::write(&fake.answer, answer).expect("write the answer");
        std::fs::write(
            &fake.script,
            format!(
                "@echo off\r\n(for %%A in (%*) do echo %%~A) > \"{}\"\r\ntype \"{}\"\r\nexit /b {code}\r\n",
                fake.argv.display(),
                fake.answer.display()
            ),
        )
        .expect("write the script");
        fake
    }

    /// What a live herdr answered `tab list` with on 2026-09-16, trimmed to
    /// three tabs and recorded in `tasks/40/PLAN_40.md`: an envelope carrying
    /// `result.tabs`, each tab with `tab_id`, `label`, `number`,
    /// `workspace_id`, `focused`, `pane_count` and `agent_status`. The third
    /// tab omits `label` entirely, which no live tab did — the parse has to
    /// survive it anyway, because a field that went missing must not cost the
    /// rest of the list its sweep.
    const LIVE_TAB_LIST: &str = r#"{
      "id": 7,
      "result": {
        "type": "tab_list",
        "tabs": [
          {"tab_id": "w1:t1", "label": "review", "number": 1, "workspace_id": "w1",
           "focused": true, "pane_count": 2, "agent_status": "idle"},
          {"tab_id": "w1:t2", "label": "", "number": 2, "workspace_id": "w1",
           "focused": false, "pane_count": 1, "agent_status": "working"},
          {"tab_id": "w2:t1", "number": 1, "workspace_id": "w2",
           "focused": false, "pane_count": 1, "agent_status": "idle"}
        ]
      }
    }"#;

    #[test]
    fn the_real_painter_gets_the_tabs_out_of_what_herdr_answers() {
        let herdr = fake_herdr("list", LIVE_TAB_LIST, 0);
        let tabs = herdr
            .painter()
            .tabs()
            .expect("the answer is the shape a live herdr gave");
        assert_eq!(
            tabs,
            vec![
                ("w1:t1".to_string(), "review".to_string()),
                ("w1:t2".to_string(), String::new()),
                ("w2:t1".to_string(), String::new()),
            ],
            "an unnamed tab is the empty label, and an absent one is too"
        );
        assert_eq!(herdr.argv(), vec!["tab", "list"]);
    }

    #[test]
    fn an_answer_that_is_not_that_shape_is_a_refusal_naming_what_could_not_be_read() {
        // The envelope without its `result`: the parse fails, and a failed
        // parse is a refusal the caller records rather than a panic.
        let herdr = fake_herdr("shape", r#"{"id": 7, "tabs": []}"#, 0);
        match herdr.painter().tabs() {
            Err(PaintError::Rejected(why)) => assert!(
                why.contains("herdr tab list"),
                "the refusal names the call: {why:?}"
            ),
            other => panic!("expected a rejection, got {other:?}"),
        }
    }

    #[test]
    fn a_non_zero_exit_from_tab_list_is_a_refusal_carrying_what_herdr_said() {
        let herdr = fake_herdr("refused", r#"{"error": {"code": "no_such_workspace"}}"#, 1);
        match herdr.painter().tabs() {
            Err(PaintError::Rejected(why)) => assert!(
                why.contains("no_such_workspace"),
                "what herdr said is what is carried: {why:?}"
            ),
            other => panic!("expected a rejection, got {other:?}"),
        }
    }

    #[test]
    fn a_herdr_that_cannot_be_started_is_not_found_and_names_the_path() {
        let painter = HerdrPainter::with_binary(crate::daemon::tests_support::MISSING_HERDR);
        for outcome in [
            painter.tabs().map(|_| ()),
            painter.rename("w1:t1", "1"),
            painter.token("w1:p1", "🎙️ REC 0:00", 1_800),
        ] {
            match outcome {
                Err(PaintError::NotFound { binary, path }) => {
                    assert_eq!(binary, crate::daemon::tests_support::MISSING_HERDR);
                    assert_eq!(path, std::env::var("PATH").unwrap_or_default());
                }
                other => panic!("expected NotFound, got {other:?}"),
            }
        }
    }

    #[test]
    fn the_real_painter_runs_rename_and_report_metadata_with_their_own_arguments() {
        let herdr = fake_herdr("args", "", 0);
        herdr
            .painter()
            .rename("w1:t1", "1 🎙️🪄 FIX")
            .expect("the script always succeeds");
        assert_eq!(
            herdr.argv(),
            vec!["tab", "rename", "w1:t1", "1 🎙️🪄 FIX"],
            "rename must not be wired to some other subcommand"
        );
        herdr
            .painter()
            .token("w1:p1", "🎙️🔴 REC 0:05", 1_800)
            .expect("the script always succeeds");
        assert_eq!(
            herdr.argv(),
            vec![
                "pane",
                "report-metadata",
                "w1:p1",
                "--source",
                "haurylau.voice",
                "--token",
                "voice=🎙️🔴 REC 0:05",
                "--ttl-ms",
                "1800",
            ]
        );
    }

    use super::tests_support::{Paint, RecordingPainter};
    use crate::config::Ui;
    use crate::daemon::tests_support::{runtime_with_clocks, RecordingJournal, TestJournal};
    use std::sync::atomic::{AtomicBool, Ordering};

    /// The drawing thread, started the way `serve` starts it, with everything a
    /// test needs to drive it.
    ///
    /// Two clocks with two different jobs. Advancing `draw_clock` makes the
    /// thread act; advancing `take_clock` makes time pass for the take. A test
    /// that confuses them proves nothing.
    struct Drawing {
        painter: RecordingPainter,
        runtime: std::sync::Arc<crate::daemon::Runtime>,
        take_clock: std::sync::Arc<crate::ptt::tests_support::TestClock>,
        draw_clock: std::sync::Arc<crate::ptt::tests_support::TestClock>,
        stop: std::sync::Arc<AtomicBool>,
        thread: Option<std::thread::JoinHandle<()>>,
    }

    impl Drawing {
        /// Everything defaults; the journal is the runtime's own.
        fn start(painter: RecordingPainter) -> Drawing {
            Drawing::build(painter, Ui::default(), None)
        }

        /// A given `[ui]`, for the test about the configuration keys.
        fn with_ui(painter: RecordingPainter, ui: Ui) -> Drawing {
            Drawing::build(painter, ui, None)
        }

        /// A journal the test can read, for the test about what is recorded.
        fn with_journal(
            painter: RecordingPainter,
            journal: &std::sync::Arc<RecordingJournal>,
        ) -> Drawing {
            Drawing::build(painter, Ui::default(), Some(std::sync::Arc::clone(journal)))
        }

        fn build(
            painter: RecordingPainter,
            ui: Ui,
            journal: Option<std::sync::Arc<RecordingJournal>>,
        ) -> Drawing {
            let take_clock = std::sync::Arc::new(crate::ptt::tests_support::TestClock::default());
            let draw_clock = std::sync::Arc::new(crate::ptt::tests_support::TestClock::default());
            let mut built = runtime_with_clocks(&take_clock, ui);
            if let Some(journal) = journal {
                built.journal = Box::new(TestJournal(journal));
            }
            let runtime = std::sync::Arc::new(built);
            let stop = std::sync::Arc::new(AtomicBool::new(false));
            let thread = {
                let painter: std::sync::Arc<dyn Painter> = std::sync::Arc::new(painter.clone());
                let runtime = std::sync::Arc::clone(&runtime);
                let clock: std::sync::Arc<dyn crate::ptt::Clock> =
                    std::sync::Arc::clone(&draw_clock) as _;
                let stop = std::sync::Arc::clone(&stop);
                std::thread::spawn(move || draw(painter, runtime, clock, stop))
            };
            // Wait for the thread to park before handing the harness back, so
            // that a test's first action cannot land while it is still starting
            // and so that `tick`'s before-and-after counts line up.
            Drawing::settle(&draw_clock, 0);
            Drawing {
                painter,
                runtime,
                take_clock,
                draw_clock,
                stop,
                thread: Some(thread),
            }
        }

        /// Spin until the thread has entered `wait_until` more than `after`
        /// times, which is the moment it has finished the tick that woke it and
        /// parked again. Bounded so a thread that never parks fails the test
        /// rather than hanging the suite.
        fn settle(clock: &std::sync::Arc<crate::ptt::tests_support::TestClock>, after: u64) {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            while clock.waits() <= after && std::time::Instant::now() < deadline {
                std::thread::yield_now();
            }
            assert!(
                clock.waits() > after,
                "the drawing thread did not finish a tick within the bound"
            );
        }

        /// Advance the drawing clock by `times` intervals and wait until the
        /// thread has actually acted on each and gone back to waiting, so no
        /// test ever sleeps for a duration or races the thread it is driving.
        ///
        /// Waiting for the whole tick rather than for its first paint is what
        /// lets a test change what the thread reads — a painter that starts
        /// failing, a label somebody else renamed — without that change landing
        /// halfway through a tick.
        fn tick(&self, times: u64) {
            for _ in 0..times {
                let painted = self.painter.total_calls();
                self.tick_quiet(1);
                assert!(
                    self.painter.total_calls() != painted,
                    "the thread painted nothing on a tick, so the tick proved \
                     nothing: {:?}",
                    self.painter.calls()
                );
            }
        }

        /// The same, for the ticks on which the thread is meant to paint
        /// nothing at all. An idle daemon whose sweep has already run touches
        /// herdr on no tick, and the sweep's tests spend ticks there precisely
        /// to prove that nothing happens on them.
        fn tick_quiet(&self, times: u64) {
            for _ in 0..times {
                let parked = self.draw_clock.waits();
                self.draw_clock.advance(self.runtime.ui.blink_ms);
                Drawing::settle(&self.draw_clock, parked);
            }
        }

        fn set(&self, activity: crate::daemon::Activity) {
            *self.runtime.activity.lock().unwrap() = activity;
        }

        fn stop(mut self) {
            self.stop.store(true, Ordering::SeqCst);
            self.draw_clock.wake();
            if let Some(thread) = self.thread.take() {
                thread.join().expect("the drawing thread ended badly");
            }
        }
    }

    fn recording(target: &str, tab: &str, since: u64) -> crate::daemon::Activity {
        crate::daemon::Activity::Recording {
            target: target.to_string(),
            tab: Some(tab.to_string()),
            since,
        }
    }

    #[test]
    fn the_token_is_renewed_every_tick_and_alternates() {
        let d = Drawing::start(RecordingPainter::ok());
        d.set(recording("w1:p1", "w1:t1", 0));
        d.tick(4);
        let values: Vec<String> = d
            .painter
            .calls()
            .into_iter()
            .filter_map(|c| match c {
                Paint::Token(_, value, _) => Some(value),
                _ => None,
            })
            .collect();
        assert_eq!(values.len(), 4, "one token per tick: {values:?}");
        assert!(
            values[0].contains('🔴') != values[1].contains('🔴'),
            "it alternates: {values:?}"
        );
        assert!(
            values[0].contains('🔴') == values[2].contains('🔴'),
            "and alternates back: {values:?}"
        );
        for value in &values {
            assert!(
                value.starts_with(MARKER),
                "every form carries the marker: {value:?}"
            );
        }
        d.stop();
    }

    #[test]
    fn the_token_carries_a_time_to_live_longer_than_the_interval_and_not_much_longer() {
        let d = Drawing::start(RecordingPainter::ok());
        d.set(recording("w1:p1", "w1:t1", 0));
        d.tick(1);
        // The first call of all is the start-up sweep's listing, so it is the
        // first token that is looked for rather than the first call.
        match d
            .painter
            .calls()
            .into_iter()
            .find(|call| matches!(call, Paint::Token(..)))
        {
            Some(Paint::Token(_, _, ttl)) => {
                assert!(
                    ttl > Ui::default().blink_ms,
                    "a token that lapses between renewals flickers: {ttl}"
                );
                // The upper bound is the half that matters on the way out. The
                // token is never cleared: a daemon that is killed stops
                // renewing, and the token is gone only once it expires. A time
                // to live of minutes would leave a pane in the sidebar saying
                // REC long after nothing was recording.
                assert_eq!(
                    ttl,
                    Ui::default().blink_ms * 3,
                    "three renewals, no more: the token outlives a killed daemon by exactly \
                     this long"
                );
            }
            other => panic!("expected a token, got {other:?}"),
        }
        d.stop();
    }

    #[test]
    fn the_tab_is_renamed_once_a_second_while_recording_and_not_on_a_blink() {
        let d = Drawing::start(RecordingPainter::with_tabs(&[("w1:t1", "1")]));
        d.set(recording("w1:p1", "w1:t1", 0));
        // Two ticks inside the same second: one rename, not two.
        d.tick(1);
        d.take_clock.advance(300);
        d.tick(1);
        assert_eq!(
            d.painter.renames().len(),
            1,
            "the blink must not rename: {:?}",
            d.painter.renames()
        );
        // Past the second boundary: a second rename, carrying the new clock.
        d.take_clock.advance(800);
        d.tick(1);
        let renames = d.painter.renames();
        assert_eq!(renames.len(), 2, "{renames:?}");
        assert!(
            renames[1].1.ends_with("REC 0:01"),
            "the clock moved: {:?}",
            renames[1].1
        );
        assert!(
            renames[1].1.starts_with("1 🎙️🔴"),
            "steady form, and a suffix: {:?}",
            renames[1].1
        );
        d.stop();
    }

    #[test]
    fn a_state_with_no_clock_renames_the_tab_once_and_then_leaves_it() {
        let d = Drawing::start(RecordingPainter::with_tabs(&[("w1:t1", "1")]));
        d.set(crate::daemon::Activity::Working {
            target: "w1:p1".into(),
            tab: Some("w1:t1".into()),
            stage: crate::daemon::Stage::Transcribing,
        });
        d.tick(5);
        let renames = d.painter.renames();
        assert_eq!(renames.len(), 1, "nothing in TRANSCR changes: {renames:?}");
        assert_eq!(renames[0].1, "1 🎙️📝 TRANSCR");
        d.stop();
    }

    #[test]
    fn the_restore_puts_the_original_back() {
        let d = Drawing::start(RecordingPainter::with_tabs(&[("w1:t1", "1")]));
        d.set(recording("w1:p1", "w1:t1", 0));
        d.tick(2);
        d.set(crate::daemon::Activity::Idle);
        d.tick(1);
        assert_eq!(
            d.painter.renames().last().map(|r| r.1.clone()),
            Some("1".to_string()),
            "the tab goes back to what it was: {:?}",
            d.painter.renames()
        );
        d.stop();
    }

    #[test]
    fn the_daemon_going_away_puts_the_tab_back() {
        // A decoration must not outlive the daemon that made it. Nothing
        // publishes `Idle` on the way out of a daemon that is being stopped
        // while a take runs, so the stop branch of the drawing loop is the only
        // thing that can restore the label.
        let painter = RecordingPainter::with_tabs(&[("w1:t1", "1")]);
        let d = Drawing::start(painter.clone());
        d.set(recording("w1:p1", "w1:t1", 0));
        d.tick(2);
        assert!(
            painter
                .renames()
                .last()
                .is_some_and(|(_, label)| label.contains(MARKER)),
            "the tab is decorated before the daemon stops: {:?}",
            painter.renames()
        );
        // The activity is left saying `Recording`, exactly as a killed daemon
        // would leave it.
        d.stop();
        assert_eq!(
            painter.renames().last().map(|(_, label)| label.clone()),
            Some("1".to_string()),
            "the last thing the thread does on its way out is put the label back: {:?}",
            painter.renames()
        );
    }

    #[test]
    fn a_take_that_moved_to_another_tab_gives_the_first_one_its_label_back() {
        let d = Drawing::start(RecordingPainter::with_tabs(&[
            ("w1:t1", "1"),
            ("w1:t2", "2"),
        ]));
        d.set(recording("w1:p1", "w1:t1", 0));
        d.tick(2);
        // A second take, in another tab, with nothing in between saying the
        // first one ended.
        d.set(recording("w1:p2", "w1:t2", 0));
        d.tick(2);
        assert_eq!(
            renames_of(&d.painter, "w1:t1").last().cloned(),
            Some("1".to_string()),
            "the tab the activity left keeps no decoration: {:?}",
            d.painter.renames()
        );
        assert!(
            renames_of(&d.painter, "w1:t2")
                .iter()
                .all(|label| label.starts_with("2 ")),
            "and the tab it moved to is decorated from its own label, not the first one's: {:?}",
            d.painter.renames()
        );
        d.stop();
    }

    #[test]
    fn a_tab_renamed_by_somebody_else_during_a_take_is_left_alone() {
        let painter = RecordingPainter::with_tabs(&[("w1:t1", "1")]);
        let d = Drawing::start(painter);
        d.set(recording("w1:p1", "w1:t1", 0));
        d.tick(2);
        // Somebody renames the tab themselves, to something that is not ours.
        d.painter.set_label("w1:t1", "mine now");
        d.set(crate::daemon::Activity::Idle);
        d.tick(1);
        assert!(
            !d.painter.renames().iter().any(|r| r.1 == "1"),
            "restoring over somebody's own rename is the defect: {:?}",
            d.painter.renames()
        );
        d.stop();
    }

    #[test]
    fn an_empty_original_label_is_restored_as_empty() {
        let d = Drawing::start(RecordingPainter::with_tabs(&[("w1:t1", "")]));
        d.set(recording("w1:p1", "w1:t1", 0));
        d.tick(1);
        assert_eq!(
            d.painter.renames()[0].1,
            "🎙️🔴 REC 0:00",
            "no leading space"
        );
        d.set(crate::daemon::Activity::Idle);
        d.tick(1);
        assert_eq!(
            d.painter.renames().last().map(|r| r.1.clone()),
            Some(String::new()),
            "empty is a value to restore, not a reason to skip: {:?}",
            d.painter.renames()
        );
        d.stop();
    }

    #[test]
    fn a_failed_paint_is_recorded_once_and_does_not_fail_the_take() {
        let journal = std::sync::Arc::new(RecordingJournal::default());
        let d = Drawing::with_journal(RecordingPainter::failing(), &journal);
        d.set(recording("w1:p1", "w1:t1", 0));
        d.tick(5);
        let lines = journal.0.lock().unwrap();
        assert_eq!(
            lines.iter().filter(|l| l.contains("cannot draw")).count(),
            1,
            "once per take, not once per tick: {lines:?}"
        );
        drop(lines);
        d.stop();
    }

    #[test]
    fn a_failed_paint_does_not_disable_the_restore() {
        // The paints fail only after the tab has been decorated, so there is a
        // decoration to put back and a disabled painter to put it back with.
        let painter = RecordingPainter::with_tabs(&[("w1:t1", "1")]);
        let d = Drawing::start(painter);
        d.set(recording("w1:p1", "w1:t1", 0));
        d.tick(1);
        d.painter.start_failing();
        d.tick(2);
        d.painter.stop_failing();
        d.set(crate::daemon::Activity::Idle);
        d.tick(1);
        assert_eq!(
            d.painter.renames().last().map(|r| r.1.clone()),
            Some("1".to_string()),
            "a broken painter must not leave a tab decorated forever: {:?}",
            d.painter.renames()
        );
        d.stop();
    }

    #[test]
    fn elapsed_comes_from_the_takes_clock_not_the_drawing_clock() {
        let d = Drawing::start(RecordingPainter::ok());
        d.set(recording("w1:p1", "w1:t1", 0));
        // Five drawing ticks, no time passing for the take: the number must not move.
        d.tick(5);
        for call in d.painter.tokens() {
            assert!(
                call.contains("0:00"),
                "the drawing clock must not be the elapsed clock: {call:?}"
            );
        }
        // Time passes for the take, one more tick: now it moves.
        d.take_clock.advance(7_000);
        d.tick(1);
        assert!(
            d.painter.tokens().last().unwrap().contains("0:07"),
            "and the take's clock must be: {:?}",
            d.painter.tokens().last()
        );
        d.stop();
    }

    #[test]
    fn neither_half_is_painted_when_its_configuration_key_is_off() {
        let d = Drawing::with_ui(
            RecordingPainter::with_tabs(&[("w1:t1", "1")]),
            Ui {
                sidebar_token: false,
                tab_indicator: true,
                ..Ui::default()
            },
        );
        d.set(recording("w1:p1", "w1:t1", 0));
        // The take's clock moves between the ticks so that each of the three
        // has a rename to make: with the token off, a tick whose label would
        // not change paints nothing at all, and `tick` would then have nothing
        // to wait for.
        d.tick(1);
        d.take_clock.advance(1_000);
        d.tick(1);
        d.take_clock.advance(1_000);
        d.tick(1);
        assert!(
            d.painter.tokens().is_empty(),
            "sidebar_token off: {:?}",
            d.painter.tokens()
        );
        assert!(!d.painter.renames().is_empty(), "tab_indicator on");
        d.stop();

        let d = Drawing::with_ui(
            RecordingPainter::with_tabs(&[("w1:t1", "1")]),
            Ui {
                sidebar_token: true,
                tab_indicator: false,
                ..Ui::default()
            },
        );
        d.set(recording("w1:p1", "w1:t1", 0));
        d.tick(3);
        assert!(!d.painter.tokens().is_empty(), "sidebar_token on");
        assert!(
            d.painter.renames().is_empty(),
            "tab_indicator off: {:?}",
            d.painter.renames()
        );
        d.stop();
    }

    /// Every label this thread wrote onto one tab, in order.
    fn renames_of(painter: &RecordingPainter, tab: &str) -> Vec<String> {
        painter
            .renames()
            .into_iter()
            .filter(|(id, _)| id == tab)
            .map(|(_, label)| label)
            .collect()
    }

    #[test]
    fn the_sweep_strips_what_a_previous_daemon_left() {
        let d = Drawing::start(RecordingPainter::with_tabs(&[
            ("w1:t1", "1 🎙️🔴 REC 0:12"),
            ("w1:t2", "review 🎙️📝 TRANSCR"),
            ("w1:t3", "🎙️🪄 FIX"),
        ]));
        d.tick(1);
        let renames = d.painter.renames();
        assert_eq!(renames.len(), 3, "every decorated tab: {renames:?}");
        assert!(
            renames.contains(&("w1:t1".to_string(), "1".to_string())),
            "{renames:?}"
        );
        assert!(
            renames.contains(&("w1:t2".to_string(), "review".to_string())),
            "{renames:?}"
        );
        assert!(
            renames.contains(&("w1:t3".to_string(), String::new())),
            "{renames:?}"
        );
        d.stop();
    }

    #[test]
    fn the_sweep_leaves_tabs_nobody_decorated_alone() {
        let d = Drawing::start(RecordingPainter::with_tabs(&[
            ("w1:t1", "1"),
            ("w1:t2", "[thing] name"),
            ("w1:t3", ""),
        ]));
        d.tick(1);
        assert!(
            d.painter.renames().is_empty(),
            "nothing to strip: {:?}",
            d.painter.renames()
        );
        d.stop();
    }

    #[test]
    fn the_sweep_runs_with_the_tab_indicator_off() {
        let d = Drawing::with_ui(
            RecordingPainter::with_tabs(&[("w1:t1", "1 🎙️🔴 REC 0:12")]),
            Ui {
                tab_indicator: false,
                ..Ui::default()
            },
        );
        d.tick(1);
        assert_eq!(
            d.painter.renames(),
            vec![("w1:t1".to_string(), "1".to_string())],
            "switching the indicator off is when its leftovers go, not when they are frozen"
        );
        d.stop();
    }

    #[test]
    fn a_failed_sweep_is_retried_once_and_only_once() {
        // The take runs in a tab of its own, so that every rename of `w1:t1` is
        // the sweep's and none of them is the decoration's.
        let painter = RecordingPainter::with_tabs(&[("w1:t1", "1 🎙️🔴 REC 0:12"), ("w1:t9", "9")]);
        painter.fail_tabs();
        let d = Drawing::start(painter);
        d.tick(1);
        assert_eq!(d.painter.tab_listings(), 1, "attempted at start");
        assert!(renames_of(&d.painter, "w1:t1").is_empty(), "and it failed");
        // No paint has succeeded yet, so no retry however many ticks pass. With
        // nothing recording and nothing decorated there is also nothing else
        // the thread could call, so the listing count is the sweep's alone.
        d.tick_quiet(5);
        assert_eq!(
            d.painter.tab_listings(),
            1,
            "no retry without evidence herdr answers"
        );
        // A take paints successfully, then ends: now the one retry may run.
        d.painter.stop_failing();
        d.set(recording("w1:p9", "w1:t9", 0));
        d.tick(2);
        assert!(
            renames_of(&d.painter, "w1:t1").is_empty(),
            "not while the take runs"
        );
        d.set(crate::daemon::Activity::Idle);
        d.tick(1);
        assert_eq!(
            renames_of(&d.painter, "w1:t1"),
            vec!["1".to_string()],
            "retried once: {:?}",
            d.painter.renames()
        );
        d.tick_quiet(10);
        assert_eq!(
            renames_of(&d.painter, "w1:t1"),
            vec!["1".to_string()],
            "and only once, whatever happens after: {:?}",
            d.painter.renames()
        );
        d.stop();
    }

    #[test]
    fn a_retry_that_fails_too_settles_rather_than_running_for_ever() {
        // Only `tabs()` fails, so paints still succeed and the sweep's retry
        // gets the evidence it waits for that herdr answers at all.
        let journal = std::sync::Arc::new(RecordingJournal::default());
        let painter = RecordingPainter::with_tabs(&[("w1:t1", "1 🎙️🔴 REC 0:12")]);
        painter.fail_tabs();
        let d = Drawing::with_journal(painter, &journal);
        d.tick(1);
        // A take paints its token successfully — that is the evidence — and
        // ends, which is the one moment the owed retry may run.
        d.set(recording("w1:p9", "w1:t9", 0));
        d.tick(2);
        d.set(crate::daemon::Activity::Idle);
        d.tick(1);
        let after_retry = d.painter.tab_listings();
        assert!(after_retry > 1, "the retry ran: {:?}", d.painter.calls());
        // Every idle tick after it would be another attempt if the retry had
        // not settled the sweep, and each of those is a subprocess, for the
        // life of a daemon whose herdr never answers.
        d.tick_quiet(5);
        assert_eq!(
            d.painter.tab_listings(),
            after_retry,
            "a retry that failed is still the last attempt: {:?}",
            d.painter.calls()
        );
        let lines = journal.0.lock().unwrap();
        assert_eq!(
            lines
                .iter()
                .filter(|line| line.contains("will not be tried again"))
                .count(),
            1,
            "and it says so once, naming what is left to do by hand: {lines:?}"
        );
        drop(lines);
        d.stop();
    }

    #[test]
    fn a_tab_herdr_will_not_rename_does_not_cost_the_rest_of_the_list_its_sweep() {
        let journal = std::sync::Arc::new(RecordingJournal::default());
        let painter = RecordingPainter::with_tabs(&[
            ("w1:t1", "1 🎙️🔴 REC 0:12"),
            ("w1:t2", "review 🎙️📝 TRANSCR"),
            ("w1:t3", "🎙️🪄 FIX"),
        ]);
        // The first tab of the three: a tab that was closed between the listing
        // and the rename looks exactly like this.
        painter.refuse_rename_of("w1:t1");
        let d = Drawing::with_journal(painter, &journal);
        d.tick(1);
        let renames = d.painter.renames();
        assert!(
            renames.contains(&("w1:t2".to_string(), "review".to_string()))
                && renames.contains(&("w1:t3".to_string(), String::new())),
            "the pass runs to the end of the list: {renames:?}"
        );
        let lines = journal.0.lock().unwrap();
        assert!(
            lines.iter().any(|line| line.contains("cannot take off")),
            "and the pass still answers with the refusal it met, rather than reporting \
             success: {lines:?}"
        );
        drop(lines);
        d.stop();
    }

    #[test]
    fn the_retry_never_runs_during_a_take() {
        let painter = RecordingPainter::with_tabs(&[("w1:t1", "1")]);
        painter.fail_tabs();
        let d = Drawing::start(painter);
        d.tick(1);
        d.painter.stop_failing();
        // A take is running and the thread holds a decoration: the retry must
        // wait, or it strips the decoration it just wrote — and the restore
        // would then find a label it did not write and leave the tab alone.
        d.set(recording("w1:p1", "w1:t1", 0));
        d.tick(4);
        assert!(
            d.painter
                .renames()
                .iter()
                .all(|(_, label)| label.contains(MARKER)),
            "a sweep during a take strips its own fresh decoration: {:?}",
            d.painter.renames()
        );
        d.stop();
    }
}
