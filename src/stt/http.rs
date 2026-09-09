//! Transcribing by posting a take to a Whisper-compatible endpoint.
//!
//! Mirrors `src/rewrite/http.rs`'s shape: a dedicated `ureq::Agent` per engine
//! instance, pooling disabled, a fixed timeout. Two differences the wire
//! contract requires: the request body is `multipart/form-data`, not JSON,
//! and the error enum has three variants, not two, so a connection failure
//! and a non-2xx response are told apart by message (`tasks/16/DESIGN_16.md`,
//! section 1).

use std::path::Path;
use std::time::Duration;

use super::{Engine, EngineError};

/// A bound turns a hang into a message rather than a leaked thread — the same
/// reason `src/rewrite/http.rs`'s own `TIMEOUT` exists.
const TIMEOUT: Duration = Duration::from_secs(30);

/// The fixed boundary string for the request's `multipart/form-data` body.
const BOUNDARY: &str = "----herdr-voice-boundary";

pub struct HttpEngine {
    url: String,
    token: String,
    model: String,
    language: String,
    agent: ureq::Agent,
}

impl HttpEngine {
    pub fn new(url: String, token: String, model: String, language: String) -> HttpEngine {
        // A dedicated agent, not the crate's shared default one: with pooling
        // disabled, this take's connection is never reused and never confused
        // with a later one, the same reason `rewrite::http::HttpEngine` does
        // this.
        let agent = ureq::AgentBuilder::new()
            .timeout(TIMEOUT)
            .max_idle_connections_per_host(0)
            .build();
        HttpEngine {
            url,
            token,
            model,
            language,
            agent,
        }
    }
}

#[derive(Debug)]
pub enum HttpError {
    /// A transport-level failure: the connection was refused, timed out, or
    /// is otherwise unreachable.
    Refused { url: String, detail: String },
    /// The server answered, but with a non-2xx status.
    Failed {
        url: String,
        status: u16,
        detail: String,
    },
    /// A 2xx response whose body does not parse as the expected JSON shape.
    Unreadable { url: String, detail: String },
}

impl std::fmt::Display for HttpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HttpError::Refused { url, detail } => write!(
                f,
                "cannot reach {url:?}: {detail}; check the server is running and the address is \
                 correct"
            ),
            HttpError::Failed { url, status, detail } => {
                write!(f, "{url:?} answered with status {status}: {detail}")
            }
            HttpError::Unreadable { url, detail } => write!(
                f,
                "{url:?} answered with something this could not read: {detail}"
            ),
        }
    }
}

impl std::error::Error for HttpError {}

/// One `multipart/form-data` field: a name, its bytes, and whether it needs a
/// filename and content type (the `file` field) or not (every text field).
fn push_field(body: &mut Vec<u8>, name: &str, value: &[u8], as_file: bool) {
    body.extend_from_slice(format!("--{BOUNDARY}\r\n").as_bytes());
    if as_file {
        body.extend_from_slice(
            format!(
                "Content-Disposition: form-data; name=\"{name}\"; filename=\"take.wav\"\r\n\
                 Content-Type: application/octet-stream\r\n\r\n"
            )
            .as_bytes(),
        );
    } else {
        body.extend_from_slice(
            format!("Content-Disposition: form-data; name=\"{name}\"\r\n\r\n").as_bytes(),
        );
    }
    body.extend_from_slice(value);
    body.extend_from_slice(b"\r\n");
}

/// Builds the request body: `file` always present, `model`/`language`/
/// `prompt` only when the design's own presence rules say so
/// (`tasks/16/DESIGN_16.md`, section 2).
fn build_body(audio_bytes: &[u8], model: &str, language: &str, bias: &str) -> Vec<u8> {
    let mut body = Vec::new();
    push_field(&mut body, "file", audio_bytes, true);
    if !model.is_empty() {
        push_field(&mut body, "model", model.as_bytes(), false);
    }
    if language != "auto" {
        push_field(&mut body, "language", language.as_bytes(), false);
    }
    if !bias.is_empty() {
        push_field(&mut body, "prompt", bias.as_bytes(), false);
    }
    body.extend_from_slice(format!("--{BOUNDARY}--\r\n").as_bytes());
    body
}

impl Engine for HttpEngine {
    fn transcribe(&self, audio: &Path, bias: &str) -> Result<String, EngineError> {
        let audio_bytes = std::fs::read(audio).map_err(|e| {
            EngineError::Http(HttpError::Refused {
                url: self.url.clone(),
                detail: format!("could not read {}: {e}", audio.display()),
            })
        })?;

        let body = build_body(&audio_bytes, &self.model, &self.language, bias);

        let mut request = self
            .agent
            .post(&self.url)
            .set(
                "Content-Type",
                &format!("multipart/form-data; boundary={BOUNDARY}"),
            );
        if !self.token.is_empty() {
            request = request.set("Authorization", &format!("Bearer {}", self.token));
        }

        let response = request.send_bytes(&body).map_err(|e| match e {
            ureq::Error::Status(code, _) => EngineError::Http(HttpError::Failed {
                url: self.url.clone(),
                status: code,
                detail: format!("server answered with status {code}"),
            }),
            ureq::Error::Transport(t) => EngineError::Http(HttpError::Refused {
                url: self.url.clone(),
                detail: t.to_string(),
            }),
        })?;

        let parsed: serde_json::Value = response.into_json().map_err(|e| {
            EngineError::Http(HttpError::Unreadable {
                url: self.url.clone(),
                detail: e.to_string(),
            })
        })?;

        match parsed.get("text").and_then(|t| t.as_str()) {
            Some(text) => Ok(text.to_string()),
            None => Err(EngineError::Http(HttpError::Unreadable {
                url: self.url.clone(),
                detail: "no text string in the response body".to_string(),
            })),
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
    /// this used to read a single fixed-size chunk and stop (`src/rewrite/
    /// http.rs`, and issue #36).
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
        let url = format!("http://{addr}/v1/audio/transcriptions");
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

    fn wav_path() -> std::path::PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!("stt-http-test-{}.wav", std::process::id()));
        std::fs::write(&path, b"RIFF....WAVEfmt ").expect("write fixture wav");
        path
    }

    #[test]
    fn the_transcript_is_read_back() {
        let (url, handle) = respond_once(r#"{"text":"pull request, not \"pulley quest\""}"#);
        let engine = HttpEngine::new(url, String::new(), String::new(), "auto".to_string());
        let text = engine.transcribe(&wav_path(), "").expect("text");
        assert_eq!(text, "pull request, not \"pulley quest\"");
        let request = handle.join().expect("server thread");
        assert!(request.contains("multipart/form-data"), "got {request}");
        assert!(
            request.contains("name=\"file\""),
            "the file field must be present: {request}"
        );
    }

    #[test]
    fn the_model_field_is_sent_when_configured() {
        let (url, handle) = respond_once(r#"{"text":"x"}"#);
        let engine = HttpEngine::new(url, String::new(), "whisper-1".to_string(), "auto".to_string());
        engine.transcribe(&wav_path(), "").expect("text");
        let request = handle.join().expect("server thread");
        assert!(
            request.contains("name=\"model\"") && request.contains("whisper-1"),
            "got {request}"
        );
    }

    #[test]
    fn the_model_field_is_absent_when_not_configured() {
        let (url, handle) = respond_once(r#"{"text":"x"}"#);
        let engine = HttpEngine::new(url, String::new(), String::new(), "auto".to_string());
        engine.transcribe(&wav_path(), "").expect("text");
        let request = handle.join().expect("server thread");
        assert!(!request.contains("name=\"model\""), "got {request}");
    }

    #[test]
    fn the_language_field_is_sent_unless_auto() {
        let (url, handle) = respond_once(r#"{"text":"x"}"#);
        let engine = HttpEngine::new(url, String::new(), String::new(), "ru".to_string());
        engine.transcribe(&wav_path(), "").expect("text");
        let request = handle.join().expect("server thread");
        assert!(
            request.contains("name=\"language\"") && request.contains("\r\nru\r\n"),
            "got {request}"
        );
    }

    #[test]
    fn the_language_field_is_absent_when_auto() {
        let (url, handle) = respond_once(r#"{"text":"x"}"#);
        let engine = HttpEngine::new(url, String::new(), String::new(), "auto".to_string());
        engine.transcribe(&wav_path(), "").expect("text");
        let request = handle.join().expect("server thread");
        assert!(!request.contains("name=\"language\""), "got {request}");
    }

    #[test]
    fn the_prompt_field_carries_the_bias_string_when_non_empty() {
        let (url, handle) = respond_once(r#"{"text":"x"}"#);
        let engine = HttpEngine::new(url, String::new(), String::new(), "auto".to_string());
        engine
            .transcribe(&wav_path(), "recent terms: pull request")
            .expect("text");
        let request = handle.join().expect("server thread");
        assert!(
            request.contains("name=\"prompt\"") && request.contains("recent terms: pull request"),
            "got {request}"
        );
    }

    #[test]
    fn an_empty_bias_sends_no_prompt_field() {
        let (url, handle) = respond_once(r#"{"text":"x"}"#);
        let engine = HttpEngine::new(url, String::new(), String::new(), "auto".to_string());
        engine.transcribe(&wav_path(), "").expect("text");
        let request = handle.join().expect("server thread");
        assert!(!request.contains("name=\"prompt\""), "got {request}");
    }

    #[test]
    fn an_empty_token_sends_no_authorization_header() {
        let (url, handle) = respond_once(r#"{"text":"x"}"#);
        let engine = HttpEngine::new(url, String::new(), String::new(), "auto".to_string());
        engine.transcribe(&wav_path(), "").expect("text");
        let request = handle.join().expect("server thread");
        assert!(!request.to_lowercase().contains("authorization"), "got {request}");
    }

    #[test]
    fn a_token_is_sent_as_a_bearer_header() {
        let (url, handle) = respond_once(r#"{"text":"x"}"#);
        let engine = HttpEngine::new(url, "secret-token".to_string(), String::new(), "auto".to_string());
        engine.transcribe(&wav_path(), "").expect("text");
        let request = handle.join().expect("server thread");
        assert!(request.contains("Bearer secret-token"), "got {request}");
    }

    #[test]
    fn a_non_2xx_response_is_a_failure_naming_the_status() {
        let (url, handle) = respond_once_with_status("500 Internal Server Error", r#"{"error":"model not loaded"}"#);
        let engine = HttpEngine::new(url.clone(), String::new(), String::new(), "auto".to_string());
        let error = engine.transcribe(&wav_path(), "").expect_err("must fail");
        assert!(matches!(error, EngineError::Http(HttpError::Failed { status: 500, .. })), "got {error:?}");
        handle.join().expect("server thread");
    }

    #[test]
    fn a_response_with_no_readable_text_is_unreadable() {
        let (url, handle) = respond_once(r#"{"choices":[]}"#);
        let engine = HttpEngine::new(url, String::new(), String::new(), "auto".to_string());
        let error = engine.transcribe(&wav_path(), "").expect_err("must fail");
        assert!(matches!(error, EngineError::Http(HttpError::Unreadable { .. })), "got {error:?}");
        handle.join().expect("server thread");
    }

    #[test]
    fn a_connection_that_refuses_is_named_by_address() {
        let engine = HttpEngine::new(
            "http://127.0.0.1:1/v1/audio/transcriptions".to_string(),
            String::new(),
            String::new(),
            "auto".to_string(),
        );
        let error = engine.transcribe(&wav_path(), "").expect_err("must fail");
        assert!(matches!(error, EngineError::Http(HttpError::Refused { .. })), "got {error:?}");
    }
}
