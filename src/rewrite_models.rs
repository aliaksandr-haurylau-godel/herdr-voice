//! The models a rewrite server says it serves.
//!
//! One bounded `GET`, nothing else: the popup never loads, unloads or downloads a
//! model on the server (`tasks/104/DESIGN_104.md`, section 2.4). The token goes in
//! the request and nowhere else, so no message here can print it.

use std::io::Read;
use std::time::Duration;

use crate::http_failure::{body_note, Cause};

/// How long the server has to answer. The same order as the other calls the plugin
/// makes to something it does not own (`docs/decisions.md`).
pub const LIST_BOUND: Duration = Duration::from_secs(10);

/// How much of the answer is read: a list of model names is small, and a server
/// that sends more is not answering the question.
const READ_LIMIT: u64 = 1_000_000;

/// Why there is no list of models to show. Each carries the sentence for the
/// person, which names what went wrong and ends in what to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ListFailure {
    /// The server did not answer, refused, or answered with an error status.
    Server(String),
    /// `[rewrite] url` does not end in `/chat/completions`, so the address of the
    /// list cannot be worked out from it.
    NoAddress(String),
    /// The answer is not `{"data":[{"id": ...}]}`.
    BadBody(String),
    /// The list has no entries.
    Empty,
}

impl std::fmt::Display for ListFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ListFailure::Server(sentence) => write!(f, "{sentence}"),
            ListFailure::NoAddress(url) => write!(
                f,
                "[rewrite] url is {url:?}, which does not end in \"/chat/completions\", so the \
                 address of the server's model list cannot be worked out from it. Correct \
                 [rewrite] url if it is wrong"
            ),
            ListFailure::BadBody(why) => write!(
                f,
                "the server answered with something that is not a list of models ({why}). \
                 Check that the address is an OpenAI-compatible server"
            ),
            ListFailure::Empty => write!(
                f,
                "the server's list of models is empty. Load a model on the server first"
            ),
        }
    }
}

/// The address of the model list for a chat-completions endpoint: the same server,
/// the `models` path beside `chat/completions`. `None` for an address that does not
/// end in `/chat/completions`.
pub fn models_url(url: &str) -> Option<String> {
    let base = url
        .trim_end_matches('/')
        .strip_suffix("/chat/completions")?;
    Some(format!("{base}/models"))
}

/// The names in an answer, in the order the server gave them.
pub fn names_in(text: &str) -> Result<Vec<String>, ListFailure> {
    let parsed: serde_json::Value = serde_json::from_str(text)
        .map_err(|e| ListFailure::BadBody(format!("{e}; {}", body_note(text))))?;
    let Some(entries) = parsed.get("data").and_then(|d| d.as_array()) else {
        return Err(ListFailure::BadBody(format!(
            "no `data` list in the answer; {}",
            body_note(text)
        )));
    };
    // A name is text from somebody else's server and goes to the terminal: control
    // characters, an escape sequence among them, are not passed on.
    let names: Vec<String> = entries
        .iter()
        .filter_map(|entry| entry.get("id").and_then(|id| id.as_str()))
        .map(|id| id.chars().filter(|c| !c.is_control()).collect::<String>())
        .filter(|id| !id.is_empty())
        .collect();
    if names.is_empty() {
        return Err(ListFailure::Empty);
    }
    Ok(names)
}

/// Asks the server behind `url` (a `[rewrite] url`) which models it serves, with the
/// bearer `token` when there is one.
pub fn fetch(url: &str, token: &str, bound: Duration) -> Result<Vec<String>, ListFailure> {
    let list_url = models_url(url).ok_or_else(|| ListFailure::NoAddress(url.to_string()))?;
    // No redirect is followed: one `GET` is all the popup sends, and a server that
    // answers with another address is said to have done so.
    let agent = ureq::AgentBuilder::new()
        .timeout(bound)
        .redirects(0)
        .max_idle_connections_per_host(0)
        .build();
    let mut request = agent.get(&list_url);
    if !token.is_empty() {
        request = request.set("Authorization", &format!("Bearer {token}"));
    }
    // What a server says back is shown to the person, and a server or a proxy may
    // echo the header it was sent: the token is taken out of whatever is shown.
    let scrub = |text: String| {
        if token.is_empty() {
            text
        } else {
            text.replace(token, "***")
        }
    };
    let response = request
        .call()
        .map_err(|e| ListFailure::Server(scrub(Cause::from_ureq(e, bound).describe(&list_url))))?;
    // With redirects off, a 3xx answer is returned as a response and not as an error.
    let status = response.status();
    if !(200..300).contains(&status) {
        let explanation = response
            .header("Location")
            .map(|to| format!("it points to {to}"))
            .unwrap_or_default();
        return Err(ListFailure::Server(scrub(
            Cause::Answered {
                status,
                explanation,
            }
            .describe(&list_url),
        )));
    }
    let mut text = String::new();
    response
        .into_reader()
        .take(READ_LIMIT)
        .read_to_string(&mut text)
        .map_err(|e| ListFailure::BadBody(scrub(e.to_string())))?;
    names_in(&text).map_err(|failure| match failure {
        ListFailure::BadBody(why) => ListFailure::BadBody(scrub(why)),
        other => other,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Write};
    use std::net::TcpListener;

    /// A server that answers one request with `response` and hands the request back.
    fn serve(response: &str) -> (String, std::thread::JoinHandle<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let address = listener.local_addr().expect("address");
        let response = response.to_string();
        let handle = std::thread::spawn(move || {
            let (stream, _) = listener.accept().expect("accept");
            let mut reader = BufReader::new(stream.try_clone().expect("clone"));
            let mut request = String::new();
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line).expect("read") == 0 {
                    break;
                }
                let blank = line == "\r\n";
                request.push_str(&line);
                if blank {
                    break;
                }
            }
            let mut stream = stream;
            stream.write_all(response.as_bytes()).expect("write");
            request
        });
        (format!("http://{address}/v1/chat/completions"), handle)
    }

    fn ok(body: &str) -> String {
        format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\
             Connection: close\r\n\r\n{body}",
            body.len()
        )
    }

    #[test]
    fn the_list_is_beside_the_chat_completions_path() {
        assert_eq!(
            models_url("http://127.0.0.1:4000/v1/chat/completions").as_deref(),
            Some("http://127.0.0.1:4000/v1/models")
        );
        assert_eq!(
            models_url("http://host/v1/chat/completions/").as_deref(),
            Some("http://host/v1/models")
        );
        assert_eq!(
            models_url("https://h/chat/completions").as_deref(),
            Some("https://h/models")
        );
    }

    #[test]
    fn an_address_that_is_not_a_chat_completions_endpoint_has_no_list_address() {
        for url in [
            "",
            "http://host",
            "http://host/v1",
            "http://host/v1/completions",
            "http://host/v1/chat",
        ] {
            assert_eq!(models_url(url), None, "{url:?}");
        }
    }

    #[test]
    fn the_names_come_in_the_order_the_server_gave_them_and_entries_without_an_id_are_skipped() {
        let names = names_in(r#"{"data":[{"id":"b"},{"object":"model"},{"id":"a"}]}"#).unwrap();
        assert_eq!(names, vec!["b".to_string(), "a".to_string()]);
    }

    #[test]
    fn an_answer_that_is_not_a_list_says_what_it_was() {
        for body in ["not json", r#"{"foo":1}"#, r#"{"data":"x"}"#, "[]"] {
            let failure = names_in(body).unwrap_err();
            assert!(
                matches!(failure, ListFailure::BadBody(_)),
                "{body}: {failure:?}"
            );
        }
        assert_eq!(names_in(r#"{"data":[]}"#), Err(ListFailure::Empty));
        assert_eq!(
            names_in(r#"{"data":[{"object":"model"}]}"#),
            Err(ListFailure::Empty)
        );
    }

    #[test]
    fn a_reachable_server_gives_its_names_with_one_get_and_no_credentials_when_there_is_no_token() {
        let (url, server) = serve(&ok(r#"{"data":[{"id":"m1"},{"id":"m2"}]}"#));
        let names = fetch(&url, "", Duration::from_secs(5)).expect("a list");
        assert_eq!(names, vec!["m1".to_string(), "m2".to_string()]);
        let request = server.join().unwrap();
        assert!(request.starts_with("GET /v1/models HTTP/1.1"), "{request}");
        assert!(
            !request.to_ascii_lowercase().contains("authorization"),
            "no token, no header: {request}"
        );
    }

    #[test]
    fn a_token_travels_as_a_bearer_header_and_appears_in_no_failure() {
        let (url, server) = serve(&ok(r#"{"data":[{"id":"m1"}]}"#));
        fetch(&url, "s3cret-token", Duration::from_secs(5)).expect("a list");
        assert!(server.join().unwrap().contains("Bearer s3cret-token"));

        let refusal = "HTTP/1.1 401 Unauthorized\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
        let (url, server) = serve(refusal);
        let failure = fetch(&url, "s3cret-token", Duration::from_secs(5)).unwrap_err();
        server.join().unwrap();
        assert!(!failure.to_string().contains("s3cret-token"), "{failure}");
    }

    #[test]
    fn an_error_status_is_a_server_failure_that_names_the_status_and_the_address() {
        let refusal = "HTTP/1.1 401 Unauthorized\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
        let (url, server) = serve(refusal);
        let failure = fetch(&url, "", Duration::from_secs(5)).unwrap_err();
        server.join().unwrap();
        let ListFailure::Server(said) = failure else {
            panic!("an error status is a server failure");
        };
        assert!(said.contains("401"), "{said}");
        assert!(said.contains("/v1/models"), "{said}");
    }

    #[test]
    fn a_server_that_is_not_there_is_a_server_failure() {
        let address = {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            listener.local_addr().unwrap()
        };
        let url = format!("http://{address}/v1/chat/completions");
        let failure = fetch(&url, "", Duration::from_secs(5)).unwrap_err();
        let ListFailure::Server(said) = failure else {
            panic!("nothing listening is a server failure");
        };
        assert!(said.contains("refused"), "{said}");
    }

    #[test]
    fn a_server_that_never_answers_is_given_up_on_within_the_bound() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!(
            "http://{}/v1/chat/completions",
            listener.local_addr().unwrap()
        );
        let started = std::time::Instant::now();
        let failure = fetch(&url, "", Duration::from_millis(300)).unwrap_err();
        assert!(started.elapsed() < Duration::from_secs(5));
        let ListFailure::Server(said) = failure else {
            panic!("silence is a server failure");
        };
        assert!(said.contains("did not reply within"), "{said}");
        drop(listener);
    }

    #[test]
    fn an_address_without_a_list_address_asks_nobody() {
        // Nothing listens at this address; a request would fail differently.
        let failure = fetch(
            "http://127.0.0.1:9/v1/completions",
            "",
            Duration::from_secs(1),
        )
        .unwrap_err();
        assert_eq!(
            failure,
            ListFailure::NoAddress("http://127.0.0.1:9/v1/completions".to_string())
        );
        assert!(failure.to_string().contains("/chat/completions"));
    }

    #[test]
    fn every_failure_says_what_to_do() {
        for failure in [
            ListFailure::NoAddress("http://h/v1".to_string()),
            ListFailure::BadBody("no `data` list".to_string()),
            ListFailure::Empty,
        ] {
            let said = failure.to_string();
            assert!(
                said.contains("Correct") || said.contains("Check") || said.contains("Load"),
                "{said}"
            );
        }
    }

    #[test]
    fn a_redirect_is_not_followed_so_one_get_is_all_that_is_ever_sent() {
        let redirect = "HTTP/1.1 302 Found\r\nLocation: http://127.0.0.1:9/elsewhere\r\n\
                        Content-Length: 0\r\nConnection: close\r\n\r\n";
        let (url, server) = serve(redirect);
        let failure = fetch(&url, "tok", Duration::from_secs(5)).unwrap_err();
        let request = server.join().unwrap();
        assert!(request.starts_with("GET /v1/models"), "{request}");
        let ListFailure::Server(said) = failure else {
            panic!("a redirect is a server failure");
        };
        assert!(said.contains("302"), "{said}");
    }

    #[test]
    fn a_server_that_echoes_the_token_does_not_put_it_on_the_screen() {
        let body = r#"{"echo":"Authorization: Bearer s3cret-token"}"#;
        let (url, server) = serve(&ok(body));
        let failure = fetch(&url, "s3cret-token", Duration::from_secs(5)).unwrap_err();
        server.join().unwrap();
        assert!(!failure.to_string().contains("s3cret-token"), "{failure}");

        let refusal = "HTTP/1.1 500 Oops\r\nContent-Length: 44\r\nConnection: close\r\n\r\n\
                       {\"error\":\"bad header Bearer s3cret-token\"}";
        let (url, server) = serve(refusal);
        let failure = fetch(&url, "s3cret-token", Duration::from_secs(5)).unwrap_err();
        server.join().unwrap();
        assert!(!failure.to_string().contains("s3cret-token"), "{failure}");
    }

    #[test]
    fn a_name_with_control_characters_reaches_the_screen_without_them() {
        let names =
            names_in("{\"data\":[{\"id\":\"a\\u001b[31mb\"},{\"id\":\"\\n\"},{\"id\":\"ok\"}]}")
                .unwrap();
        assert_eq!(names, vec!["a[31mb".to_string(), "ok".to_string()]);
    }
}
