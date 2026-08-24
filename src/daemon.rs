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

use crate::context;
use crate::proto::{Reply, Request};
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

pub fn answer(request: &Request) -> (Reply, Control) {
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

pub fn start() -> Result<Outcome, TransportError> {
    let address = transport::address(&transport::Vars::from_env())?;

    // Connect first. A successful connection means a live daemon owns the name.
    if transport::connect(&address).is_ok() {
        return Ok(Outcome::AlreadyRunning(address.display().to_string()));
    }

    let listener = transport::listen(&address)?;
    eprintln!("listening at {}", address.display());
    serve(listener, address);
    Ok(Outcome::Served)
}

fn serve(listener: transport::Listener, address: Address) {
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
        thread::spawn(move || {
            if let Err(e) = serve_one(connection, &stop, &address) {
                eprintln!("connection failed: {e}");
            }
        });
    }
}

fn serve_one(
    connection: transport::Stream,
    stop: &AtomicBool,
    address: &Address,
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
    let (reply, control) = answer(&request);
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

    fn request(command: &str, context: &[u8]) -> Request {
        Request {
            command: command.to_string(),
            entrypoint: Some(command.to_string()),
            context: context.to_vec(),
        }
    }

    #[test]
    fn cancel_needs_no_pane_and_dictate_does() {
        assert!(!needs_target_pane("cancel"));
        assert!(needs_target_pane("dictate"));
        assert!(needs_target_pane("ptt"));
    }

    #[test]
    fn cancel_works_with_no_context_at_all() {
        let (reply, control) = answer(&request("cancel", b""));
        assert!(matches!(reply, Reply::Ok(_)), "got {reply:?}");
        assert!(matches!(control, Control::Continue));
    }

    #[test]
    fn cancel_works_with_a_malformed_context() {
        let (reply, _) = answer(&request("cancel", b"{not json"));
        assert!(matches!(reply, Reply::Ok(_)), "got {reply:?}");
    }

    #[test]
    fn a_command_that_needs_a_pane_says_what_was_missing() {
        let (reply, _) = answer(&request("dictate", b""));
        match reply {
            Reply::Error(text) => assert!(
                text.contains("HERDR_PLUGIN_CONTEXT_JSON"),
                "the message must name what was missing, got {text:?}"
            ),
            other => panic!("expected an error, got {other:?}"),
        }
    }

    #[test]
    fn a_command_that_needs_a_pane_accepts_one() {
        let (reply, _) = answer(&request("dictate", br#"{"focused_pane_id":"w1:p2"}"#));
        assert!(matches!(reply, Reply::Ok(_)), "got {reply:?}");
    }

    #[test]
    fn an_unknown_command_is_refused_by_name() {
        let (reply, _) = answer(&request("transcribe", b""));
        match reply {
            Reply::Error(text) => assert!(text.contains("transcribe"), "got {text:?}"),
            other => panic!("expected an error, got {other:?}"),
        }
    }

    #[test]
    fn the_stop_request_ends_the_loop() {
        let (reply, control) = answer(&request("stop", b""));
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
                super::serve_one(connection, &stop, &address).map_err(|e| e.to_string())
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
        let served = {
            let address = address.clone();
            std::thread::spawn(move || super::serve(listener, address))
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

        served.join().expect("the loop must end");
    }
}
