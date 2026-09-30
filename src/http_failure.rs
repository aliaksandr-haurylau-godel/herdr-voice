//! What an HTTP failure was, read out of a `ureq::Error`, and the message each
//! cause produces. Shared by the two HTTP engines (`src/rewrite/http.rs` and
//! `src/stt/http.rs`) so the wording lives in one place (`tasks/52/DESIGN_52.md`).

use std::io::{self, Read};
use std::time::Duration;

/// How much of a refusing server's body is read.
const BODY_READ_BYTES: u64 = 2048;
/// How much of it reaches the message.
const EXPLANATION_CHARS: usize = 300;

#[derive(Debug)]
pub enum Cause {
    /// No reply within the engine's bound.
    TimedOut { bound: Duration },
    /// Nothing listens at the address and port.
    ConnectionRefused,
    /// The server answered with a non-2xx status.
    Answered { status: u16, explanation: String },
    /// Any other transport failure, for example a name that does not resolve.
    Other { detail: String },
}

impl Cause {
    /// Reads the cause out of what `ureq` returned. `bound` is the time bound the
    /// request was made with, carried into a timeout so the message can name it.
    pub fn from_ureq(error: ureq::Error, bound: Duration) -> Cause {
        match error {
            ureq::Error::Status(status, response) => Cause::Answered {
                status,
                explanation: explanation_of(response),
            },
            ureq::Error::Transport(transport) => {
                let kind = io_kind(&transport);
                from_io_kind(kind, bound, transport.to_string())
            }
        }
    }

    /// The message for this cause. It names `url` and ends in what to do next.
    pub fn describe(&self, url: &str) -> String {
        match self {
            Cause::TimedOut { bound } => format!(
                "{url:?} did not reply within {}. If the server is still loading a model, wait \
                 and try again",
                bound_text(*bound)
            ),
            Cause::ConnectionRefused => format!(
                "{url:?} refused the connection: nothing is listening at that address and port. \
                 Start the server, or correct the address in the configuration"
            ),
            Cause::Answered {
                status,
                explanation,
            } => {
                let said = if explanation.is_empty() {
                    "the server gave no explanation"
                } else {
                    explanation.as_str()
                };
                let advice = if *status >= 500 {
                    "The server failed on its side; its own log says why."
                } else {
                    "Correct the address, model or token in the configuration."
                };
                format!(
                    "{url:?} answered with status {status} and refused the request: {said}. \
                     {advice}"
                )
            }
            Cause::Other { detail } => format!(
                "cannot reach {url:?}: {detail}; check the server is running and the address is \
                 correct"
            ),
        }
    }
}

/// The operating system's own classification, read from the first `io::Error`
/// in the error's chain of sources. Matching on message text would break when
/// `ureq` rewords.
fn io_kind(error: &(dyn std::error::Error + 'static)) -> Option<io::ErrorKind> {
    let mut current = error.source();
    while let Some(source) = current {
        if let Some(io_error) = source.downcast_ref::<io::Error>() {
            return Some(io_error.kind());
        }
        current = source.source();
    }
    None
}

/// `WouldBlock` is what a socket timeout is on some Unix platforms; `ureq`
/// normalises it, and it is accepted here too so that a change there cannot turn
/// a timeout into the last row of the table.
fn from_io_kind(kind: Option<io::ErrorKind>, bound: Duration, detail: String) -> Cause {
    match kind {
        Some(io::ErrorKind::TimedOut) | Some(io::ErrorKind::WouldBlock) => {
            Cause::TimedOut { bound }
        }
        Some(io::ErrorKind::ConnectionRefused) => Cause::ConnectionRefused,
        _ => Cause::Other { detail },
    }
}

/// What the server said, bounded. A failure while reading the body leaves the
/// explanation empty: it must never replace the status, which is already known.
fn explanation_of(response: ureq::Response) -> String {
    let mut bytes = Vec::new();
    let _ = response
        .into_reader()
        .take(BODY_READ_BYTES)
        .read_to_end(&mut bytes);
    explanation_from_bytes(&bytes)
}

fn explanation_from_bytes(bytes: &[u8]) -> String {
    excerpt(&String::from_utf8_lossy(bytes))
}

/// One line, at most `EXPLANATION_CHARS` characters. Whitespace runs become one
/// space, because the message reaches the run journal and a toast, both of which
/// are a single line. Characters, not bytes, are counted so that a Cyrillic body
/// is never cut inside a character.
fn excerpt(text: &str) -> String {
    let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() <= EXPLANATION_CHARS {
        return flat;
    }
    let kept: String = flat.chars().take(EXPLANATION_CHARS).collect();
    format!("{}...", kept.trim_end())
}

fn bound_text(bound: Duration) -> String {
    if bound.subsec_nanos() == 0 {
        match bound.as_secs() {
            1 => "1 second".to_string(),
            seconds => format!("{seconds} seconds"),
        }
    } else {
        format!("{} milliseconds", bound.as_millis())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_connection_nothing_listens_on_is_a_refusal() {
        // Port 1: nothing listens there. Shows what ureq's error chain really
        // ends in on this platform, which the design could not verify.
        let error = ureq::get("http://127.0.0.1:1/")
            .call()
            .expect_err("nothing listens on port 1");
        let cause = Cause::from_ureq(error, Duration::from_secs(30));
        assert!(matches!(cause, Cause::ConnectionRefused), "got {cause:?}");
    }

    #[test]
    fn a_timed_out_kind_is_a_timeout_carrying_the_bound() {
        let bound = Duration::from_millis(200);
        for kind in [io::ErrorKind::TimedOut, io::ErrorKind::WouldBlock] {
            let cause = from_io_kind(Some(kind), bound, "detail".to_string());
            assert!(
                matches!(cause, Cause::TimedOut { bound: b } if b == bound),
                "got {cause:?} for {kind:?}"
            );
        }
    }

    #[test]
    fn a_refused_kind_is_a_refusal() {
        let cause = from_io_kind(
            Some(io::ErrorKind::ConnectionRefused),
            Duration::from_secs(1),
            "detail".to_string(),
        );
        assert!(matches!(cause, Cause::ConnectionRefused), "got {cause:?}");
    }

    #[test]
    fn any_other_kind_and_no_kind_are_other_and_keep_the_detail() {
        for kind in [Some(io::ErrorKind::UnexpectedEof), None] {
            let cause = from_io_kind(kind, Duration::from_secs(1), "the detail".to_string());
            assert!(
                matches!(&cause, Cause::Other { detail } if detail == "the detail"),
                "got {cause:?} for {kind:?}"
            );
        }
    }

    #[test]
    fn an_excerpt_at_the_limit_is_unchanged_and_one_over_is_cut() {
        let at_limit = "a".repeat(EXPLANATION_CHARS);
        assert_eq!(excerpt(&at_limit), at_limit);
        let over = "a".repeat(EXPLANATION_CHARS + 1);
        assert_eq!(excerpt(&over), format!("{at_limit}..."));
    }

    #[test]
    fn an_excerpt_is_one_line() {
        assert_eq!(
            excerpt("{\n  \"error\":\r\n\t\"bad model\"\n}\n"),
            "{ \"error\": \"bad model\" }"
        );
    }

    #[test]
    fn an_empty_or_blank_excerpt_is_empty() {
        assert_eq!(excerpt(""), "");
        assert_eq!(excerpt(" \n\t "), "");
    }

    #[test]
    fn a_cut_never_leaves_a_space_before_the_dots() {
        // Character 300 is a space: the cut must not end in " ...".
        let text = format!("{} {}", "a".repeat(EXPLANATION_CHARS - 1), "b".repeat(50));
        assert_eq!(
            excerpt(&text),
            format!("{}...", "a".repeat(EXPLANATION_CHARS - 1))
        );
    }

    #[test]
    fn a_long_cyrillic_body_is_cut_on_a_character_boundary() {
        let text = "ж".repeat(EXPLANATION_CHARS + 20);
        let cut = excerpt(&text);
        assert_eq!(cut, format!("{}...", "ж".repeat(EXPLANATION_CHARS)));
    }

    #[test]
    fn a_body_that_is_not_utf8_keeps_its_readable_part() {
        let explanation = explanation_from_bytes(b"model \xff\xfe not found");
        assert!(explanation.contains("model"), "got {explanation}");
        assert!(explanation.contains("not found"), "got {explanation}");
    }

    #[test]
    fn a_bound_is_written_in_seconds_when_whole_and_milliseconds_otherwise() {
        assert_eq!(bound_text(Duration::from_secs(30)), "30 seconds");
        assert_eq!(bound_text(Duration::from_secs(1)), "1 second");
        assert_eq!(bound_text(Duration::from_millis(200)), "200 milliseconds");
    }

    #[test]
    fn a_timeout_message_names_the_url_and_the_bound_and_says_to_wait() {
        let message = Cause::TimedOut {
            bound: Duration::from_secs(30),
        }
        .describe("http://h/v1");
        assert!(message.contains("\"http://h/v1\""), "got {message}");
        assert!(
            message.contains("did not reply within 30 seconds"),
            "got {message}"
        );
        assert!(message.contains("wait and try again"), "got {message}");
        assert!(
            !message.contains("check the server is running"),
            "got {message}"
        );
    }

    #[test]
    fn a_refusal_message_says_nothing_listens_and_what_to_do() {
        let message = Cause::ConnectionRefused.describe("http://h/v1");
        assert!(message.contains("\"http://h/v1\""), "got {message}");
        assert!(message.contains("refused the connection"), "got {message}");
        assert!(message.contains("Start the server"), "got {message}");
        assert!(!message.contains("did not reply"), "got {message}");
    }

    #[test]
    fn an_answer_message_names_status_and_explanation_and_not_reachability() {
        let message = Cause::Answered {
            status: 400,
            explanation: "unknown model".to_string(),
        }
        .describe("http://h/v1");
        assert!(message.contains("\"http://h/v1\""), "got {message}");
        assert!(
            message.contains("answered with status 400"),
            "got {message}"
        );
        assert!(message.contains("unknown model"), "got {message}");
        assert!(
            message.contains("Correct the address, model or token"),
            "got {message}"
        );
        assert!(!message.contains("cannot reach"), "got {message}");
        assert!(
            !message.contains("check the server is running"),
            "got {message}"
        );
    }

    #[test]
    fn a_status_of_500_and_above_sends_the_person_to_the_servers_log() {
        let describe = |status| {
            Cause::Answered {
                status,
                explanation: "boom".to_string(),
            }
            .describe("u")
        };
        assert!(
            describe(500).contains("its own log says why"),
            "got {}",
            describe(500)
        );
        assert!(
            !describe(499).contains("its own log says why"),
            "got {}",
            describe(499)
        );
        assert!(
            describe(499).contains("Correct the address, model or token"),
            "got {}",
            describe(499)
        );
        assert!(
            !describe(500).contains("Correct the address"),
            "got {}",
            describe(500)
        );
    }

    #[test]
    fn an_answer_with_no_explanation_says_so() {
        let message = Cause::Answered {
            status: 400,
            explanation: String::new(),
        }
        .describe("u");
        assert!(
            message.contains("the server gave no explanation"),
            "got {message}"
        );
    }

    #[test]
    fn another_failure_keeps_the_old_sentence_and_its_detail() {
        let message = Cause::Other {
            detail: "dns error".to_string(),
        }
        .describe("http://h/v1");
        assert!(
            message.contains("cannot reach \"http://h/v1\": dns error"),
            "got {message}"
        );
        assert!(
            message.contains("check the server is running and the address is correct"),
            "got {message}"
        );
    }
}
