//! What the daemon answers to a `reload` request, and how a popup reads it.
//!
//! One line, because a reply travels as one line (`src/proto.rs`). The writer and
//! the reader sit together so that one cannot change without the other's test
//! failing. See `tasks/103/DESIGN_103.md`, section 2.4.

/// What a reload reply says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Applied {
    /// The sections the daemon now uses the new values of.
    pub applied: Vec<String>,
    /// The sections whose values in the file differ from what the daemon runs and
    /// that only a restart applies.
    pub restart: Vec<String>,
}

const RESTART: &str = "; needs a restart: ";

pub fn reply(applied: &[&str], restart: &[&str]) -> String {
    let head = if applied.is_empty() {
        "applied: nothing".to_string()
    } else {
        format!("applied: {}", applied.join(", "))
    };
    if restart.is_empty() {
        head
    } else {
        format!("{head}{RESTART}{}", restart.join(", "))
    }
}

pub fn parse(text: &str) -> Option<Applied> {
    let (head, tail) = match text.split_once(RESTART) {
        Some((head, tail)) => (head, Some(tail)),
        None => (text, None),
    };
    let applied = head.strip_prefix("applied: ")?;
    let list = |s: &str| -> Vec<String> { s.split(", ").map(str::to_string).collect() };
    Some(Applied {
        applied: if applied == "nothing" {
            Vec::new()
        } else {
            list(applied)
        },
        restart: tail.map(list).unwrap_or_default(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_applied_and_nothing_to_restart() {
        assert_eq!(reply(&[], &[]), "applied: nothing");
    }

    #[test]
    fn audio_applied() {
        assert_eq!(reply(&["audio"], &[]), "applied: audio");
    }

    #[test]
    fn a_restart_is_named_after_what_was_applied() {
        assert_eq!(
            reply(&["audio"], &["stt", "rewrite"]),
            "applied: audio; needs a restart: stt, rewrite"
        );
        assert_eq!(
            reply(&[], &["stt"]),
            "applied: nothing; needs a restart: stt"
        );
    }

    #[test]
    fn what_is_written_is_read_back() {
        for (applied, restart) in [
            (vec![], vec![]),
            (vec!["audio"], vec![]),
            (vec![], vec!["stt"]),
            (vec!["audio"], vec!["stt", "rewrite", "ui"]),
        ] {
            let text = reply(&applied, &restart);
            let read = parse(&text).unwrap_or_else(|| panic!("{text:?} must parse"));
            assert_eq!(read.applied, applied, "{text:?}");
            assert_eq!(read.restart, restart, "{text:?}");
        }
    }

    #[test]
    fn what_is_not_a_reload_reply_is_not_read_as_one() {
        for text in [
            "pong",
            "",
            "applied:",
            "applied audio",
            "stopping",
            "nothing to cancel",
        ] {
            assert_eq!(parse(text), None, "{text:?}");
        }
    }
}
