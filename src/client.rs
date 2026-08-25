//! The short-lived half: one connection, one frame, one reply line, exit.
//!
//! Holding a key starts this about twelve times a second (`docs/evidence.md`), so
//! it reads the context out of the environment and copies it without looking at
//! it. See `tasks/3/DESIGN_3.md`, section 2.

use std::io::BufReader;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use crate::proto::{Reply, Request};
use crate::transport::{self, Address};

/// How long the client waits for a reply before it gives up. A daemon that
/// answers a few bytes has no reason to take longer, and a hang is the failure
/// this bound exists to prevent.
pub const REPLY_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Debug)]
pub enum ClientError {
    NoDaemon(String),
    Timeout,
    Transport(String),
    Protocol(String),
}

#[derive(Debug, PartialEq, Eq)]
pub struct Outcome {
    pub code: u8,
    pub message: Option<String>,
}

pub fn outcome(result: Result<Reply, ClientError>) -> Outcome {
    match result {
        // What the daemon says about a success is not noise: for `dictate` it is
        // where the take went and how loud it was, and throwing it away leaves the
        // caller with a silent exit 0.
        Ok(Reply::Ok(text)) => Outcome {
            code: 0,
            message: (!text.is_empty()).then_some(text),
        },
        Ok(Reply::Error(text)) => Outcome {
            code: 1,
            message: Some(text),
        },
        Err(ClientError::NoDaemon(address)) => Outcome {
            code: 1,
            message: Some(format!(
                "no dictation daemon is listening at {address}; \
                 start it with `herdr-voice daemon`, or restart herdr so the plugin's \
                 startup entry does"
            )),
        },
        Err(ClientError::Timeout) => Outcome {
            code: 1,
            message: Some(format!(
                "the daemon did not answer within {} seconds; \
                 check `herdr plugin log list --plugin haurylau.voice`",
                REPLY_TIMEOUT.as_secs()
            )),
        },
        Err(ClientError::Transport(why)) => Outcome {
            code: 1,
            message: Some(why),
        },
        Err(ClientError::Protocol(why)) => Outcome {
            code: 1,
            message: Some(format!("the daemon spoke something unexpected: {why}")),
        },
    }
}

pub fn send(command: &str) -> Outcome {
    let address = match transport::address(&transport::Vars::from_env()) {
        Ok(address) => address,
        Err(e) => return outcome(Err(ClientError::Transport(e.to_string()))),
    };
    let entrypoint = std::env::var("HERDR_PLUGIN_ENTRYPOINT_ID").ok();
    let context = std::env::var("HERDR_PLUGIN_CONTEXT_JSON")
        .map(String::into_bytes)
        .unwrap_or_default();
    send_to(&address, command, entrypoint, context)
}

pub fn send_to(
    address: &Address,
    command: &str,
    entrypoint: Option<String>,
    context: Vec<u8>,
) -> Outcome {
    let mut stream = match transport::connect(address) {
        Ok(stream) => stream,
        Err(_) => return outcome(Err(ClientError::NoDaemon(address.display().to_string()))),
    };

    let request = Request {
        command: command.to_string(),
        entrypoint,
        context,
    };
    if let Err(e) = request.write_to(&mut stream) {
        return outcome(Err(ClientError::Transport(e.to_string())));
    }

    // The reply is read on another thread so a daemon that never answers costs a
    // bounded wait rather than a hang. The process exits right after, so the
    // abandoned thread has nothing to clean up.
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let mut reader = BufReader::new(stream);
        let _ = sender.send(Reply::read_from(&mut reader).map_err(|e| e.to_string()));
    });

    match receiver.recv_timeout(REPLY_TIMEOUT) {
        Ok(Ok(reply)) => outcome(Ok(reply)),
        Ok(Err(why)) => outcome(Err(ClientError::Protocol(why))),
        Err(_) => outcome(Err(ClientError::Timeout)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_ok_reply_succeeds_and_passes_on_what_the_daemon_said() {
        let outcome = outcome(Ok(Reply::Ok("nothing to cancel".into())));
        assert_eq!(outcome.code, 0);
        assert_eq!(outcome.message.as_deref(), Some("nothing to cancel"));
    }

    #[test]
    fn an_ok_reply_with_nothing_to_say_says_nothing() {
        let outcome = outcome(Ok(Reply::Ok(String::new())));
        assert_eq!(outcome.code, 0);
        assert_eq!(outcome.message, None);
    }

    #[test]
    fn an_error_reply_is_printed_and_fails() {
        let outcome = outcome(Ok(Reply::Error("no pane".into())));
        assert_eq!(outcome.code, 1);
        assert_eq!(outcome.message.as_deref(), Some("no pane"));
    }

    #[test]
    fn no_daemon_names_how_to_start_one() {
        let outcome = outcome(Err(ClientError::NoDaemon("/tmp/voice.sock".into())));
        assert_ne!(outcome.code, 0);
        let message = outcome.message.expect("a message");
        assert!(message.contains("/tmp/voice.sock"), "got {message:?}");
        assert!(
            message.contains("herdr-voice daemon"),
            "the message must name how to start the daemon, got {message:?}"
        );
    }

    #[test]
    fn a_silent_daemon_is_a_timeout_rather_than_a_hang() {
        let outcome = outcome(Err(ClientError::Timeout));
        assert_ne!(outcome.code, 0);
        let message = outcome.message.expect("a message");
        assert!(message.contains("did not answer"), "got {message:?}");
    }

    #[test]
    fn the_reply_timeout_is_bounded_and_short() {
        assert!(REPLY_TIMEOUT <= std::time::Duration::from_secs(5));
    }

    #[test]
    fn a_daemon_answers_a_real_client() {
        let address = crate::transport::tests_support::probe_address("client-round-trip");
        let listener = crate::transport::listen(&address).expect("listen");
        let server = std::thread::spawn(move || {
            let mut reader = std::io::BufReader::new(listener.accept().expect("accept"));
            let request = Request::read_from(&mut reader).expect("read");
            Reply::Ok("nothing to cancel".into())
                .write_to(reader.get_mut())
                .expect("write");
            request
        });

        let outcome = send_to(&address, "cancel", Some("cancel".into()), Vec::new());
        assert_eq!(outcome.code, 0);

        let request = server.join().unwrap();
        assert_eq!(request.command, "cancel");
        assert_eq!(request.entrypoint.as_deref(), Some("cancel"));
    }

    #[test]
    fn the_context_travels_verbatim() {
        let address = crate::transport::tests_support::probe_address("client-context");
        let listener = crate::transport::listen(&address).expect("listen");
        let server = std::thread::spawn(move || {
            let mut reader = std::io::BufReader::new(listener.accept().expect("accept"));
            let request = Request::read_from(&mut reader).expect("read");
            Reply::Ok(String::new())
                .write_to(reader.get_mut())
                .expect("write");
            request
        });

        let body = br#"{"focused_pane_id":"w1:p2","tab_label":"a \"quoted\" label"}"#.to_vec();
        send_to(&address, "cancel", None, body.clone());
        assert_eq!(server.join().unwrap().context, body);
    }
}
