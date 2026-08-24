//! The frame the client and the daemon exchange.
//!
//! A header line of four tokens and a body of exactly the promised length. The
//! body is the bytes of `HERDR_PLUGIN_CONTEXT_JSON`, copied by the client without
//! inspection: holding a key starts the client about twelve times a second, and it
//! has no use for the fields. See `tasks/3/DESIGN_3.md`, section 2.

use std::fmt;
use std::io::{self, BufRead, Write};

/// The protocol token. A mismatch is refused by name rather than ignored.
pub const PROTOCOL: &str = "voice/1";

/// Stands for a token that was not set, so the token count never varies.
const ABSENT: &str = "-";

#[derive(Debug, PartialEq, Eq)]
pub struct Request {
    /// The subcommand, spelled as the manifest spells it.
    pub command: String,
    /// `HERDR_PLUGIN_ENTRYPOINT_ID`, which reaches the client only.
    pub entrypoint: Option<String>,
    /// `HERDR_PLUGIN_CONTEXT_JSON`, uninspected.
    pub context: Vec<u8>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Reply {
    Ok(String),
    Error(String),
}

#[derive(Debug)]
pub enum ProtoError {
    /// The peer connected and closed without sending anything. Not a failure: it
    /// is what a liveness probe looks like from the listening side, and both
    /// `doctor` and a second `daemon` start do exactly that.
    Empty,
    BadHeader(String),
    UnknownProtocol(String),
    BadToken(String),
    ShortBody {
        expected: usize,
        got: usize,
    },
    Io(io::Error),
}

impl fmt::Display for ProtoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProtoError::Empty => write!(f, "the peer sent nothing"),
            ProtoError::BadHeader(line) => write!(f, "malformed header: {line:?}"),
            ProtoError::UnknownProtocol(found) => {
                write!(
                    f,
                    "unknown protocol {found:?}, this build speaks {PROTOCOL}"
                )
            }
            ProtoError::BadToken(token) => write!(f, "token {token:?} contains whitespace"),
            ProtoError::ShortBody { expected, got } => {
                write!(f, "body of {got} bytes, header promised {expected}")
            }
            ProtoError::Io(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for ProtoError {}

impl From<io::Error> for ProtoError {
    fn from(e: io::Error) -> Self {
        ProtoError::Io(e)
    }
}

fn token(value: &str) -> Result<&str, ProtoError> {
    if value.is_empty() || value.split_whitespace().count() != 1 {
        return Err(ProtoError::BadToken(value.to_string()));
    }
    Ok(value)
}

impl Request {
    pub fn write_to<W: Write>(&self, w: &mut W) -> Result<(), ProtoError> {
        let command = token(&self.command)?;
        let entrypoint = match self.entrypoint.as_deref() {
            Some(value) => token(value)?,
            None => ABSENT,
        };
        writeln!(
            w,
            "{PROTOCOL} {command} {entrypoint} {}",
            self.context.len()
        )?;
        w.write_all(&self.context)?;
        w.flush()?;
        Ok(())
    }

    pub fn read_from<R: BufRead>(r: &mut R) -> Result<Request, ProtoError> {
        let mut header = String::new();
        if r.read_line(&mut header)? == 0 {
            return Err(ProtoError::Empty);
        }
        let line = header.trim_end_matches(['\r', '\n']);
        let parts: Vec<&str> = line.split(' ').collect();
        if parts.len() != 4 {
            return Err(ProtoError::BadHeader(line.to_string()));
        }
        if parts[0] != PROTOCOL {
            return Err(ProtoError::UnknownProtocol(parts[0].to_string()));
        }
        let length: usize = parts[3]
            .parse()
            .map_err(|_| ProtoError::BadHeader(line.to_string()))?;
        let mut context = vec![0u8; length];
        let mut filled = 0;
        while filled < length {
            let read = r.read(&mut context[filled..])?;
            if read == 0 {
                return Err(ProtoError::ShortBody {
                    expected: length,
                    got: filled,
                });
            }
            filled += read;
        }
        Ok(Request {
            command: parts[1].to_string(),
            entrypoint: (parts[2] != ABSENT).then(|| parts[2].to_string()),
            context,
        })
    }
}

impl Reply {
    pub fn write_to<W: Write>(&self, w: &mut W) -> Result<(), ProtoError> {
        match self {
            Reply::Ok(text) => writeln!(w, "ok {text}")?,
            Reply::Error(text) => writeln!(w, "error {text}")?,
        }
        w.flush()?;
        Ok(())
    }

    pub fn read_from<R: BufRead>(r: &mut R) -> Result<Reply, ProtoError> {
        let mut line = String::new();
        r.read_line(&mut line)?;
        let line = line.trim_end_matches(['\r', '\n']);
        match line.split_once(' ') {
            Some(("ok", rest)) => Ok(Reply::Ok(rest.to_string())),
            Some(("error", rest)) => Ok(Reply::Error(rest.to_string())),
            _ if line == "ok" => Ok(Reply::Ok(String::new())),
            _ => Err(ProtoError::BadHeader(line.to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::BufReader;

    fn round_trip(request: &Request) -> Request {
        let mut buffer = Vec::new();
        request.write_to(&mut buffer).expect("write");
        Request::read_from(&mut BufReader::new(&buffer[..])).expect("read")
    }

    #[test]
    fn a_request_survives_a_round_trip() {
        let request = Request {
            command: "cancel".to_string(),
            entrypoint: Some("cancel".to_string()),
            context: br#"{"tab_id":"t1"}"#.to_vec(),
        };
        assert_eq!(round_trip(&request), request);
    }

    #[test]
    fn an_absent_entrypoint_travels_as_a_dash() {
        let request = Request {
            command: "cancel".to_string(),
            entrypoint: None,
            context: Vec::new(),
        };
        let mut buffer = Vec::new();
        request.write_to(&mut buffer).expect("write");
        assert_eq!(buffer, b"voice/1 cancel - 0\n");
        assert_eq!(round_trip(&request), request);
    }

    #[test]
    fn a_peer_that_sends_nothing_is_not_a_malformed_header() {
        // A liveness probe connects and closes. It must be distinguishable from a
        // peer that sent something wrong, so the daemon can stay quiet about it.
        let mut input = BufReader::new(&b""[..]);
        let error = Request::read_from(&mut input).expect_err("must refuse");
        assert!(matches!(error, ProtoError::Empty), "got {error:?}");
    }

    #[test]
    fn a_header_with_the_wrong_token_count_is_refused() {
        let mut input = BufReader::new(&b"voice/1 cancel 0\n"[..]);
        let error = Request::read_from(&mut input).expect_err("must refuse");
        assert!(matches!(error, ProtoError::BadHeader(_)), "got {error:?}");
    }

    #[test]
    fn a_foreign_protocol_is_refused_by_name() {
        let mut input = BufReader::new(&b"voice/2 cancel - 0\n"[..]);
        let error = Request::read_from(&mut input).expect_err("must refuse");
        assert!(
            matches!(error, ProtoError::UnknownProtocol(_)),
            "got {error:?}"
        );
    }

    #[test]
    fn a_body_shorter_than_the_header_promises_is_refused() {
        let mut input = BufReader::new(&b"voice/1 cancel - 12\nshort"[..]);
        let error = Request::read_from(&mut input).expect_err("must refuse");
        assert!(
            matches!(error, ProtoError::ShortBody { .. }),
            "got {error:?}"
        );
    }

    #[test]
    fn a_token_containing_a_space_is_refused_on_write() {
        let request = Request {
            command: "cancel".to_string(),
            entrypoint: Some("two words".to_string()),
            context: Vec::new(),
        };
        let mut buffer = Vec::new();
        let error = request.write_to(&mut buffer).expect_err("must refuse");
        assert!(matches!(error, ProtoError::BadToken(_)), "got {error:?}");
    }

    #[test]
    fn replies_survive_a_round_trip() {
        for reply in [
            Reply::Ok("listening".into()),
            Reply::Error("no pane".into()),
        ] {
            let mut buffer = Vec::new();
            reply.write_to(&mut buffer).expect("write");
            let read = Reply::read_from(&mut BufReader::new(&buffer[..])).expect("read");
            assert_eq!(read, reply);
        }
    }
}
