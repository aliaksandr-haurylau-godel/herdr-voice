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
}
