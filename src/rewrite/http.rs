//! Rewriting through an OpenAI-compatible chat-completions endpoint.
//!
//! One POST per take, blocking (`ureq`, no async runtime — `docs/decisions.md`,
//! 2026-09-04, #36). See `tasks/36/DESIGN_36.md`, section 6.

use std::fmt;
use std::time::Duration;

/// Ported from `spike/spike.sh`'s `rewrite()` prompt text, adapted: the
/// prototype's four separate `CTX_*` fields collapse into the one `bias`
/// string `bias::collect` already produces.
const PROMPT: &str = "You fix the form of a spoken transcript: file and directory names, \
flags, commands, foreign technical terms, punctuation and capitalization. You never change its \
meaning, length or intent. Recent context, which may be empty, may name terms or paths worth \
matching: use it only to correct terms, never to add content. Reply with the corrected \
transcript only, nothing else.";

/// A bound turns a hang into a message rather than a leaked thread — the same
/// reason #28 exists for delivery and transcription.
const TIMEOUT: Duration = Duration::from_secs(30);

pub struct HttpEngine {
    url: String,
    token: String,
    model: String,
    agent: ureq::Agent,
}

impl HttpEngine {
    pub fn new(url: String, token: String, model: String) -> HttpEngine {
        // A dedicated agent, not the crate's shared default one: with pooling
        // disabled, this take's connection is never reused and never confused
        // with a later one — one POST per take needing rewrite (`DESIGN_36.md`
        // §6), so pooling buys nothing here.
        let agent = ureq::AgentBuilder::new()
            .timeout(TIMEOUT)
            .max_idle_connections_per_host(0)
            .build();
        HttpEngine {
            url,
            token,
            model,
            agent,
        }
    }
}

#[derive(Debug)]
pub enum HttpError {
    Failed { url: String, detail: String },
    Unreadable { url: String, detail: String },
}

impl fmt::Display for HttpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HttpError::Failed { url, detail } => write!(
                f,
                "cannot reach {url:?}: {detail}; check the server is running and the address is \
                 correct"
            ),
            HttpError::Unreadable { url, detail } => write!(
                f,
                "{url:?} answered with something this could not read: {detail}"
            ),
        }
    }
}

impl std::error::Error for HttpError {}

impl HttpEngine {
    pub fn rewrite(&self, transcript: &str, bias: &str) -> Result<String, HttpError> {
        let system = if bias.is_empty() {
            PROMPT.to_string()
        } else {
            format!("{PROMPT}\n\nRecent context: {bias}")
        };
        let body = serde_json::json!({
            "model": self.model,
            "messages": [
                {"role": "system", "content": system},
                {"role": "user", "content": transcript},
            ],
            "temperature": 0,
        });

        let mut request = self.agent.post(&self.url);
        if !self.token.is_empty() {
            request = request.set("Authorization", &format!("Bearer {}", self.token));
        }

        let response = request.send_json(body).map_err(|e| match e {
            ureq::Error::Status(code, _) => HttpError::Failed {
                url: self.url.clone(),
                detail: format!("server answered with status {code}"),
            },
            ureq::Error::Transport(t) => HttpError::Failed {
                url: self.url.clone(),
                detail: t.to_string(),
            },
        })?;

        let parsed: serde_json::Value =
            response.into_json().map_err(|e| HttpError::Unreadable {
                url: self.url.clone(),
                detail: e.to_string(),
            })?;

        let content = parsed
            .get("choices")
            .and_then(|c| c.get(0))
            .and_then(|c| c.get("message"))
            .and_then(|m| m.get("content"))
            .and_then(|c| c.as_str());

        match content {
            Some(text) => Ok(text.trim().to_string()),
            None => Err(HttpError::Unreadable {
                url: self.url.clone(),
                detail: "no choices[0].message.content string in the response body".to_string(),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    /// Accepts exactly one connection, reads the whole request (headers and,
    /// per `Content-Length`, the body), writes back a fixed HTTP response,
    /// then returns. Runs on a background thread so the test can drive a real
    /// `ureq` call against `http://127.0.0.1:<port>`.
    ///
    /// Draining the request fully before closing matters: a stream dropped
    /// while the kernel still holds unread bytes for it can turn the close
    /// into a reset instead of an orderly shutdown, which showed up as an
    /// intermittent, unrelated-looking read failure on the client side when
    /// this used to read a single fixed-size chunk and stop.
    fn respond_once(response_body: &'static str) -> (String, std::thread::JoinHandle<String>) {
        respond_once_with_status("200 OK", response_body)
    }

    /// The same double as `respond_once`, with the status line as a
    /// parameter — so a test can drive `ureq::Error::Status` without a
    /// second, near-duplicate listener implementation.
    fn respond_once_with_status(
        status_line: &'static str,
        response_body: &'static str,
    ) -> (String, std::thread::JoinHandle<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let addr = listener.local_addr().expect("addr");
        let url = format!("http://{addr}/v1/chat/completions");
        let handle = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept");
            let mut buf = Vec::new();
            let mut chunk = [0u8; 4096];
            let header_end = loop {
                if let Some(pos) = find_subslice(&buf, b"\r\n\r\n") {
                    break pos + 4;
                }
                let n = stream.read(&mut chunk).unwrap_or(0);
                if n == 0 {
                    break buf.len();
                }
                buf.extend_from_slice(&chunk[..n]);
            };
            let headers = String::from_utf8_lossy(&buf[..header_end.min(buf.len())]).to_string();
            let content_length: usize = headers
                .lines()
                .find_map(|line| {
                    line.to_lowercase()
                        .strip_prefix("content-length:")
                        .map(|v| v.trim().to_string())
                })
                .and_then(|v| v.parse().ok())
                .unwrap_or(0);
            while buf.len() < header_end + content_length {
                let n = stream.read(&mut chunk).unwrap_or(0);
                if n == 0 {
                    break;
                }
                buf.extend_from_slice(&chunk[..n]);
            }
            let request = String::from_utf8_lossy(&buf).to_string();
            let response = format!(
                "HTTP/1.1 {status_line}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                response_body.len(),
                response_body
            );
            let _ = stream.write_all(response.as_bytes());
            let _ = stream.flush();
            request
        });
        (url, handle)
    }

    fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
        haystack.windows(needle.len()).position(|w| w == needle)
    }

    #[test]
    fn the_rewritten_text_is_read_back() {
        let (url, handle) = respond_once(
            r#"{"choices":[{"message":{"content":"pull request, not \"pulley quest\""}}]}"#,
        );
        let engine = HttpEngine::new(url, String::new(), "local-model".to_string());
        let rewritten = engine.rewrite("pulley quest", "").expect("text");
        assert_eq!(rewritten, "pull request, not \"pulley quest\"");
        let request = handle.join().expect("server thread");
        assert!(
            request.contains("\"model\":\"local-model\""),
            "got {request}"
        );
        assert!(request.contains("pulley quest"), "got {request}");
    }

    #[test]
    fn an_empty_token_sends_no_authorization_header() {
        let (url, handle) = respond_once(r#"{"choices":[{"message":{"content":"x"}}]}"#);
        let engine = HttpEngine::new(url, String::new(), String::new());
        engine.rewrite("x", "").expect("text");
        let request = handle.join().expect("server thread");
        assert!(
            !request.to_lowercase().contains("authorization"),
            "got {request}"
        );
    }

    #[test]
    fn a_token_is_sent_as_a_bearer_header() {
        let (url, handle) = respond_once(r#"{"choices":[{"message":{"content":"x"}}]}"#);
        let engine = HttpEngine::new(url, "secret-token".to_string(), String::new());
        engine.rewrite("x", "").expect("text");
        let request = handle.join().expect("server thread");
        assert!(request.contains("Bearer secret-token"), "got {request}");
    }

    #[test]
    fn an_unreadable_body_is_named_as_such() {
        let (url, handle) = respond_once("not json");
        let engine = HttpEngine::new(url, String::new(), String::new());
        let error = engine.rewrite("x", "").expect_err("must fail");
        assert!(
            matches!(error, HttpError::Unreadable { .. }),
            "got {error:?}"
        );
        handle.join().expect("server thread");
    }

    #[test]
    fn a_non_2xx_response_is_a_failure() {
        let (url, handle) = respond_once_with_status(
            "500 Internal Server Error",
            r#"{"error":"model not loaded"}"#,
        );
        let engine = HttpEngine::new(url.clone(), String::new(), String::new());
        let error = engine.rewrite("x", "").expect_err("must fail");
        assert!(matches!(error, HttpError::Failed { .. }), "got {error:?}");
        assert!(error.to_string().contains(&url), "got {error}");
        handle.join().expect("server thread");
    }

    #[test]
    fn a_response_with_no_readable_content_is_a_failure() {
        // Well-formed JSON, but no choices — one of the shapes an
        // OpenAI-compatible server can send back on top of a 200 without
        // that being an HTTP failure.
        let (url, handle) = respond_once(r#"{"choices":[]}"#);
        let engine = HttpEngine::new(url, String::new(), String::new());
        let error = engine.rewrite("x", "").expect_err("must fail");
        assert!(
            matches!(error, HttpError::Unreadable { .. }),
            "got {error:?}"
        );
        handle.join().expect("server thread");
    }

    #[test]
    fn a_connection_that_refuses_is_named_by_address() {
        // Nothing is listening on this port.
        let engine = HttpEngine::new(
            "http://127.0.0.1:1".to_string(),
            String::new(),
            String::new(),
        );
        let error = engine.rewrite("x", "").expect_err("must fail");
        let message = error.to_string();
        assert!(message.contains("127.0.0.1:1"), "got {message}");
    }
}
