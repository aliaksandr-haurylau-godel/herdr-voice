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

// Nothing outside this module's own tests calls any of it yet: the painter
// arrives in Task 2 and the drawing loop in Task 5, which removes this line.
#![allow(dead_code)]

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
            if inner.failing {
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
}
