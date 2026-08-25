//! Resolving `[context] source` into a checked `Source`, or refusing it.

use super::Source;

/// The values `[context] source` accepts, named in that order in a refusal
/// message.
const VALUES: &[&str] = &["auto", "transcript", "pane"];

/// Resolves `[context] source`. `Err` names the value given and the three
/// valid values, the same shape `EngineError::Unknown` already gives an
/// unrecognised `[stt] engine` (`src/stt.rs`).
///
/// Not called from `main` yet — Task 8 wires it into the daemon's start-up
/// once #22 merges (`tasks/21/PLAN_21.md`). CI runs clippy with
/// `-D warnings`, so an unreached `pub` item in a binary crate must be
/// allowed explicitly rather than left to warn.
#[allow(dead_code)]
pub fn resolve(value: &str) -> Result<Source, String> {
    match value {
        "auto" => Ok(Source::Auto),
        "transcript" => Ok(Source::Transcript),
        "pane" => Ok(Source::Pane),
        other => Err(format!(
            "unknown [context] source {other:?}; it is one of {}",
            VALUES
                .iter()
                .map(|v| format!("{v:?}"))
                .collect::<Vec<_>>()
                .join(", ")
        )),
    }
}

#[cfg(test)]
mod tests {
    use crate::bias::Source;

    #[test]
    fn transcript_resolves() {
        assert_eq!(super::resolve("transcript"), Ok(Source::Transcript));
    }

    #[test]
    fn pane_resolves() {
        assert_eq!(super::resolve("pane"), Ok(Source::Pane));
    }

    #[test]
    fn auto_resolves_to_a_named_member_not_an_absence() {
        assert_eq!(super::resolve("auto"), Ok(Source::Auto));
    }

    #[test]
    fn an_unrecognised_value_is_refused_naming_all_three() {
        let error = super::resolve("vosk").expect_err("must be refused");
        assert!(error.contains("vosk"), "got {error:?}");
        assert!(error.contains("auto"), "got {error:?}");
        assert!(error.contains("transcript"), "got {error:?}");
        assert!(error.contains("pane"), "got {error:?}");
    }
}
