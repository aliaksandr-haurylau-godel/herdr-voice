# One settings popup Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** One popup, `herdr-voice settings`, opened by an action and a key, lists every section and key of `config.toml`, changes a key in place, and has its own choosers for the microphone, the speech model and the rewrite model; the separate microphone command, action, pane and binding are gone.

**Architecture:** Every flow talks to a `World` trait (the file, the daemon, the sound devices, the models directory, a download, a server), implemented for a real run by `popup::Real` and for tests by a fake, so each flow is tested by scripting the input and reading the output. The list of keys is read from the configuration type, which derives `Serialize`. The writer, the reload request and the recorder's reconfigure that this branch already holds are used as they are.

**Tech Stack:** Rust 2021; `toml` 1, `serde`, `serde_json`, `ureq` 2 (all already dependencies). No new crate.

**Spec:** `tasks/104/DESIGN_104.md` (evidence `tasks/104/DESIGN_104_evidence.md`), criteria `tasks/104/AC_104.md`. Read the design first: it states why each choice was made. The code in this plan was written and run in a scratch copy of this tree before the plan was written (`tasks/104/RUN_104.md`, "S3, how the plan was made"), so each snippet compiles and its tests pass; a failing run in S4 is a transcription difference.

## Global Constraints

- Everything in the repository is English: code, comments, messages, commits. Cite paths relative to the repository root. No absolute path, no home directory, no account name in any file (the leak gate rejects them).
- Device selection by name, never by index. Every user-visible failure names what to do next.
- No new dependency: `Cargo.toml` and `Cargo.lock` do not change.
- Every cargo command runs with `CARGO_BUILD_JOBS=6`, one cargo process at a time, wrapped in a time limit: `perl -e 'alarm shift; exec @ARGV' <seconds> cargo ...`.
- Never `git push` without naming the branch: `git push -u origin HEAD:feat/104-settings-popup`. Never change `git config`.
- Commit messages end with the line `Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>`.
- Before every commit, check the files you wrote for stray editing-tool tags (the four angle-bracket tags the editing tool uses around old and new text) and for conflict markers at the start of a line; the pre-commit hook rejects both. A document that quotes those tags must put them inside backticks.
- Test commands in this plan: `CARGO_BUILD_JOBS=6 perl -e 'alarm shift; exec @ARGV' 1500 cargo test --bin herdr-voice <filter>`; the whole suite is `CARGO_BUILD_JOBS=6 perl -e 'alarm shift; exec @ARGV' 2400 cargo test`.
- Four gates before the pull request: `cargo test`, `cargo clippy --all-targets -- -D warnings`, `cargo fmt --check`, `python3 scripts/check_manifest.py`, plus the Windows dead-code check in Task 8. Intermediate tasks may leave `dead_code` warnings that a later task removes; only Task 8 runs clippy.
- A test in `src/daemon.rs` (`a_hold_whose_delivery_times_out_ends_and_is_reported_once_naming_herdr`) is timing-sensitive and failed twice in a full run on a busy machine (load average above 7) and passed alone and on a quiet machine. If it is the only failure, rerun it alone and the suite once, and record it in `tasks/104/RUN_104.md`; any other failure is this change's.

## Review Focus

Failure modes the design implies that no task's ordinary tests would exercise; each is pinned by a test in the task named.

1. A token value must appear in no output of the popup, including the rewrite flow that sends it to a server (Task 5, `a_secret_is_never_printed_and_cannot_be_changed_here`; Task 2, `a_token_travels_as_a_bearer_header_and_appears_in_no_failure`).
2. Choosing a catalogue model with the default `command` engine must download nothing and write nothing, and must name the file the command looks for (Task 4).
3. A key that is in the file with the same value as its default is "set", not "default" (Task 5, `a_key_the_file_sets_shows_the_files_value_and_one_it_does_not_shows_its_default_marked`).
4. A configuration file that does not parse shows defaults with a note first and refuses a save (Task 3 and Task 5).
5. The end of the input must unwind every menu level once, with one message and exit code 1 (Task 3, Task 5).

## File structure

| File | Responsibility |
|---|---|
| `src/config.rs` | `Serialize` derived on the configuration types |
| `src/stt.rs`, `src/bias/source.rs` | `ENGINES` and `VALUES` visible to the popup |
| `src/rewrite_models.rs` (new) | The model list of a rewrite server: `models_url`, `fetch`, `ListFailure` |
| `src/popup.rs` (new) | `Io`, `World`, `Snapshot`, `Reached`, `change_note`, `save_and_tell`, answers, `pause`, `open_with`, `Real`, `Line`, and `tests_support::FakeWorld` |
| `src/chooser.rs` | The speech-model flow `choose_speech_model`, `list`, and `run` as a wrapper |
| `src/settings.rs` (new) | The list of keys, the menus, the editor, the microphone and rewrite flows, `run`, `open` |
| `src/mic.rs` | Removed |
| `src/main.rs`, `herdr-plugin.toml`, `src/setup.rs` | `settings` replaces `mic` |
| `docs/decisions.md`, `README.md`, `docs/evidence.md` | Three decisions, how to open the settings, a pointer |

Task order and dependencies: 1; 2; 3 needs 2; 4 needs 3; 5 needs 3 and 4; 6 needs 5; 7 needs 6; 8 needs all.

---

### Task 1: The configuration type serialises

**Files:**
- Modify: `src/config.rs`, `src/stt.rs` (line 46), `src/bias/source.rs` (line 7)

**Interfaces:**
- Consumes: nothing.
- Produces: `Config` and every section struct implement `serde::Serialize`; `stt::ENGINES` and `bias::source::VALUES` are `pub(crate)`.

- [ ] **Step 1: Write the failing test**

Append this test to the `tests` module of `src/config.rs`, before its closing brace:

````rust
    #[test]
    fn the_defaults_serialise_with_every_section_and_a_value_for_every_key() {
        let value = toml::Value::try_from(Config::default()).expect("the defaults serialise");
        let table = value.as_table().expect("a table");
        let mut names: Vec<&str> = table.keys().map(String::as_str).collect();
        names.sort_unstable();
        assert_eq!(
            names,
            ["audio", "context", "delivery", "ptt", "record", "rewrite", "stt", "ui"]
        );
        assert_eq!(table["audio"]["silence_db"].as_float(), Some(-60.0));
        assert_eq!(table["stt"]["engine"].as_str(), Some("command"));
        assert!(table["stt"]["command"].is_array());
        assert_eq!(table["stt"]["token"].as_str(), Some(""));
    }
````

- [ ] **Step 2: Run it to see it fail**

Run: `CARGO_BUILD_JOBS=6 perl -e 'alarm shift; exec @ARGV' 1500 cargo test --bin herdr-voice config::tests::the_defaults_serialise`
Expected: compile error, the trait bound `Config: Serialize` is not satisfied.

- [ ] **Step 3: Implement**

In `src/config.rs` change `use serde::Deserialize;` to `use serde::{Deserialize, Serialize};`, and add `Serialize` to every derive that has `Deserialize`: the nine lines `#[derive(Debug, Clone, ..., Deserialize)]` on `Config`, `Audio`, `Stt`, `Rewrite`, `Ui`, `Ptt`, `Delivery`, `Record` and `Context` become `#[derive(Debug, Clone, ..., Deserialize, Serialize)]` (keep the other derives each has). In `src/stt.rs` change `const ENGINES: &[&str]` to `pub(crate) const ENGINES: &[&str]`; in `src/bias/source.rs` change `const VALUES: &[&str]` to `pub(crate) const VALUES: &[&str]`.

- [ ] **Step 4: Run it to see it pass**

Run: `CARGO_BUILD_JOBS=6 perl -e 'alarm shift; exec @ARGV' 1500 cargo test --bin herdr-voice config::tests::the_defaults_serialise` then `CARGO_BUILD_JOBS=6 perl -e 'alarm shift; exec @ARGV' 1500 cargo test --bin herdr-voice config::`
Expected: both pass.

- [ ] **Step 5: Commit**

```bash
git add src/config.rs src/stt.rs src/bias/source.rs
git commit -m "Serialise the configuration types so the popup can list their keys

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 2: The model list of a rewrite server, `src/rewrite_models.rs`

**Files:**
- Create: `src/rewrite_models.rs`
- Modify: `src/main.rs` (add `mod rewrite_models;` after `mod rewrite;`)

**Interfaces:**
- Consumes: `http_failure::{body_note, Cause}` (existing).
- Produces (exact): `pub const LIST_BOUND: Duration`; `pub enum ListFailure { Server(String), NoAddress(String), BadBody(String), Empty }` with `Debug, Clone, PartialEq, Eq` and `Display`; `pub fn models_url(url: &str) -> Option<String>`; `pub fn names_in(text: &str) -> Result<Vec<String>, ListFailure>`; `pub fn fetch(url: &str, token: &str, bound: Duration) -> Result<Vec<String>, ListFailure>`.

- [ ] **Step 1: Write the failing tests**

Create `src/rewrite_models.rs` containing only this test module, and add `mod rewrite_models;` to `src/main.rs` after `mod rewrite;`:

````rust
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
}
````

- [ ] **Step 2: Run to see it fail**

Run: `CARGO_BUILD_JOBS=6 perl -e 'alarm shift; exec @ARGV' 1500 cargo test --bin herdr-voice rewrite_models::`
Expected: compile errors, `cannot find function models_url` and the other items.

- [ ] **Step 3: Implement**

Put this at the top of `src/rewrite_models.rs`, above the test module:

````rust
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
    let names: Vec<String> = entries
        .iter()
        .filter_map(|entry| entry.get("id").and_then(|id| id.as_str()))
        .map(str::to_string)
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
    let agent = ureq::AgentBuilder::new()
        .timeout(bound)
        .max_idle_connections_per_host(0)
        .build();
    let mut request = agent.get(&list_url);
    if !token.is_empty() {
        request = request.set("Authorization", &format!("Bearer {token}"));
    }
    let response = request
        .call()
        .map_err(|e| ListFailure::Server(Cause::from_ureq(e, bound).describe(&list_url)))?;
    let mut text = String::new();
    response
        .into_reader()
        .take(READ_LIMIT)
        .read_to_string(&mut text)
        .map_err(|e| ListFailure::BadBody(e.to_string()))?;
    names_in(&text)
}
````

- [ ] **Step 4: Run to see it pass**

Run: `CARGO_BUILD_JOBS=6 perl -e 'alarm shift; exec @ARGV' 1500 cargo test --bin herdr-voice rewrite_models::`
Expected: 11 tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/rewrite_models.rs src/main.rs
git commit -m "Add the model list of a rewrite server: one bounded GET, every failure a sentence

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 3: The popup's primitives and its seam, `src/popup.rs`

**Files:**
- Create: `src/popup.rs`
- Modify: `src/main.rs` (add `mod popup;` after `mod outward;`), `src/reload.rs` (`Applied` derives `Clone`)

**Interfaces:**
- Consumes: Task 2's `ListFailure`, `fetch`, `LIST_BOUND`; `config_edit::{Edit, WriteError, write_keys}`; `client::{exchange, outcome, ClientError}`; `outward`; `stt::fetch`, `stt::candle::store`.
- Produces (exact):
  - `pub const OPEN_BOUND: Duration`, `pub const LEAVE_HINT: &str`
  - `pub enum Reached { Applied(reload::Applied), NoDaemon, Failed(String) }` (`Debug, Clone, PartialEq, Eq`); `pub fn reached(Result<Reply, ClientError>) -> Reached`; `pub fn tell_daemon(&Address) -> Reached`
  - `pub enum Answer { Leave, Pick(usize), Invalid(String) }`; `pub fn parse_answer(&str, usize) -> Answer`; `pub fn not_in_list(&str, usize) -> String`; `pub fn prompt(&mut dyn Write, &str)`
  - `pub struct Io<'a> { pub input, pub out, pub failed }` with `new`, `say`, `fail`, `ask`
  - `pub struct Snapshot { pub text: String, pub loaded: Loaded }` with `table() -> Option<toml::Table>`
  - `pub trait World` with `snapshot, input_names, save, tell, models_dir, model_state, install, rewrite_models`
  - `pub fn change_note(&str, &Reached) -> (String, bool)`; `pub fn save_and_tell(&mut dyn World, &mut Io, section, key, value: String, shown: &str) -> bool`
  - `pub fn config_note(&Source) -> Option<String>`; `pub fn pause()`; `pub fn open_command(herdr, plugin, entrypoint) -> Vec<String>`; `pub fn open_with(herdr, plugin, entrypoint, bound) -> Result<(), String>`
  - `pub struct Line` (progress); `pub struct Real` with `from_env()` implementing `World`
  - `#[cfg(test)] pub mod tests_support` with `FakeWorld` (fields `dir, names, reached, told, saved, save_error, models, states, installs, install_error, list, list_calls`; `new(tag, text)`, `file()`)

- [ ] **Step 1: Write the failing tests**

Create `src/popup.rs` containing only this test module. Add `mod popup;` to `src/main.rs` after `mod outward;`. In `src/reload.rs` change the derive on `Applied` to `#[derive(Debug, Clone, PartialEq, Eq)]`.

````rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn answers_are_read_the_way_the_prompt_says() {
        assert_eq!(parse_answer("2\n", 3), Answer::Pick(1));
        assert_eq!(parse_answer(" 1 \r\n", 3), Answer::Pick(0));
        assert_eq!(parse_answer("\n", 3), Answer::Leave);
        assert_eq!(parse_answer("\u{1b}\n", 3), Answer::Leave);
        // An arrow key reaches a line read as an Esc-led sequence.
        assert_eq!(parse_answer("\u{1b}[A\n", 3), Answer::Leave);
        for bad in ["0\n", "4\n", "-1\n", "x\n", "2x\n"] {
            assert!(
                matches!(parse_answer(bad, 3), Answer::Invalid(_)),
                "{bad:?}"
            );
        }
    }

    #[test]
    fn an_answer_outside_the_list_names_the_range() {
        let said = not_in_list("7", 2);
        assert!(
            said.contains("\"7\"") && said.contains("between 1 and 2"),
            "{said}"
        );
    }

    #[test]
    fn a_question_is_flushed_before_anyone_waits_for_the_answer() {
        #[derive(Default)]
        struct Watch {
            written: Vec<u8>,
            flushed_at: usize,
        }
        impl Write for Watch {
            fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
                self.written.extend_from_slice(bytes);
                Ok(bytes.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                self.flushed_at = self.written.len();
                Ok(())
            }
        }
        let mut watch = Watch::default();
        prompt(&mut watch, "Type a number, then Enter: ");
        assert!(!watch.written.is_empty());
        assert_eq!(
            watch.flushed_at,
            watch.written.len(),
            "the whole question must be on screen"
        );
    }

    #[test]
    fn asking_prints_the_question_and_returns_the_line() {
        let mut input = Cursor::new(b"2\n".to_vec());
        let mut out: Vec<u8> = Vec::new();
        let mut io = Io::new(&mut input, &mut out);
        assert_eq!(io.ask("Which? ").as_deref(), Some("2\n"));
        assert!(!io.failed);
        assert_eq!(String::from_utf8(out).unwrap(), "Which? ");
    }

    #[test]
    fn the_end_of_the_input_is_said_once_and_every_later_question_answers_at_once() {
        let mut input = Cursor::new(Vec::new());
        let mut out: Vec<u8> = Vec::new();
        let mut io = Io::new(&mut input, &mut out);
        assert_eq!(io.ask("First? "), None);
        assert!(io.failed);
        assert_eq!(io.ask("Second? "), None);
        let said = String::from_utf8(out).unwrap();
        assert_eq!(
            said.matches("nothing was read from the terminal").count(),
            1,
            "{said}"
        );
        assert!(
            !said.contains("Second?"),
            "a question after the end is not printed: {said}"
        );
    }

    #[test]
    fn every_result_of_the_exchange_maps_to_what_the_popup_does() {
        let applied = |applied: &[&str], restart: &[&str]| {
            Reached::Applied(reload::Applied {
                applied: applied.iter().map(|s| s.to_string()).collect(),
                restart: restart.iter().map(|s| s.to_string()).collect(),
            })
        };
        assert_eq!(
            reached(Ok(Reply::Ok("applied: audio".to_string()))),
            applied(&["audio"], &[])
        );
        assert_eq!(
            reached(Ok(Reply::Ok(
                "applied: nothing; needs a restart: stt".to_string()
            ))),
            applied(&[], &["stt"])
        );
        assert!(matches!(
            reached(Ok(Reply::Ok("pong".to_string()))),
            Reached::Failed(_)
        ));
        assert_eq!(
            reached(Ok(Reply::Error("nothing was changed".to_string()))),
            Reached::Failed("nothing was changed".to_string())
        );
        assert_eq!(
            reached(Err(ClientError::NoDaemon("x".to_string()))),
            Reached::NoDaemon
        );
        let Reached::Failed(said) = reached(Err(ClientError::Timeout(Duration::from_secs(10))))
        else {
            panic!("a timeout is a failure");
        };
        assert!(said.contains("did not answer within 10 seconds"), "{said}");
        assert_eq!(
            reached(Err(ClientError::Transport("x".to_string()))),
            Reached::Failed("x".to_string())
        );
        assert!(matches!(
            reached(Err(ClientError::Protocol("odd".to_string()))),
            Reached::Failed(_)
        ));
    }

    #[test]
    fn telling_the_daemon_sends_a_reload_and_reads_the_answer() {
        let address = crate::transport::tests_support::probe_address("popup-tell");
        let listener = crate::transport::listen(&address).expect("listen");
        let server = std::thread::spawn(move || {
            let mut reader = std::io::BufReader::new(listener.accept().expect("accept"));
            let request = crate::proto::Request::read_from(&mut reader).expect("read");
            Reply::Ok("applied: audio".to_string())
                .write_to(reader.get_mut())
                .expect("write");
            request
        });
        let Reached::Applied(done) = tell_daemon(&address) else {
            panic!("the daemon applied it");
        };
        assert_eq!(done.applied, vec!["audio".to_string()]);
        assert_eq!(server.join().unwrap().command, "reload");
    }

    fn applied(applied: &[&str], restart: &[&str]) -> Reached {
        Reached::Applied(reload::Applied {
            applied: applied.iter().map(|s| s.to_string()).collect(),
            restart: restart.iter().map(|s| s.to_string()).collect(),
        })
    }

    #[test]
    fn a_change_to_audio_says_the_next_take_uses_it_when_the_daemon_applied_it() {
        let (said, failed) = change_note("audio", &applied(&["audio"], &[]));
        assert!(said.contains("next take"), "{said}");
        assert!(!failed);
        let (said, _) = change_note("audio", &applied(&[], &[]));
        assert!(said.contains("already uses it"), "{said}");
    }

    #[test]
    fn a_change_to_any_other_section_says_it_needs_a_restart_of_herdr() {
        let (said, failed) = change_note("stt", &applied(&[], &["stt"]));
        assert!(said.contains("needs a restart of herdr"), "{said}");
        assert!(said.contains("[stt]"), "{said}");
        assert!(!failed);
        let (said, _) = change_note("ui", &applied(&[], &[]));
        assert!(said.contains("already runs with this value"), "{said}");
    }

    #[test]
    fn other_sections_that_also_need_a_restart_are_named() {
        let (said, _) = change_note("audio", &applied(&["audio"], &["stt", "ui"]));
        assert!(said.contains("restart of herdr: stt, ui."), "{said}");
        let (said, _) = change_note("stt", &applied(&[], &["stt", "ui"]));
        assert!(said.contains("restart of herdr: ui."), "{said}");
        assert!(!said.contains("restart of herdr: stt"), "{said}");
    }

    #[test]
    fn no_daemon_means_the_change_applies_when_it_starts() {
        let (said, failed) = change_note("audio", &Reached::NoDaemon);
        assert!(said.contains("No dictation daemon is running"), "{said}");
        assert!(said.contains("applies when it starts"), "{said}");
        assert!(!failed);
    }

    #[test]
    fn a_daemon_that_did_not_take_it_is_a_failure_that_says_what_to_check() {
        let (said, failed) = change_note(
            "audio",
            &Reached::Failed("the daemon did not answer within 10 seconds".to_string()),
        );
        assert!(failed);
        assert!(said.contains("did not answer within 10 seconds"), "{said}");
        assert!(
            said.contains("herdr plugin log list --plugin herdr-voice"),
            "{said}"
        );
    }

    #[test]
    fn the_action_opens_the_named_pane_of_this_plugin_through_herdr() {
        assert_eq!(
            open_command("herdr", "herdr-voice", "settings"),
            vec![
                "herdr",
                "plugin",
                "pane",
                "open",
                "--plugin",
                "herdr-voice",
                "--entrypoint",
                "settings"
            ]
        );
    }

    #[test]
    fn a_file_that_does_not_parse_is_noted_and_the_snapshot_has_no_table() {
        let world = tests_support::FakeWorld::new("popup-invalid", "[audio\ninput = ");
        let snapshot = world.snapshot();
        assert!(snapshot.table().is_none());
        let note = config_note(&snapshot.loaded.source).expect("a note");
        assert!(
            note.contains("does not parse") && note.contains("defaults"),
            "{note}"
        );
        let fine = tests_support::FakeWorld::new("popup-fine", "[audio]\ninput = \"X\"\n");
        assert!(fine.snapshot().table().is_some());
        assert_eq!(config_note(&fine.snapshot().loaded.source), None);
        let absent = tests_support::FakeWorld::new("popup-absent", "");
        assert_eq!(config_note(&absent.snapshot().loaded.source), None);
    }

    #[cfg(unix)]
    mod opening {
        use super::super::*;

        fn script(tag: &str, body: &str) -> String {
            let dir = std::env::temp_dir().join(format!(
                "herdr-voice-popup-open-{tag}-{}",
                std::process::id()
            ));
            std::fs::create_dir_all(&dir).unwrap();
            let path = dir.join("herdr");
            crate::script_fixture::write_executable(&path, body);
            path.display().to_string()
        }

        #[test]
        fn herdr_is_asked_to_open_the_pane_of_this_plugin() {
            let record = std::env::temp_dir().join(format!(
                "herdr-voice-popup-argv-record-{}",
                std::process::id()
            ));
            let herdr = script(
                "args",
                &format!("#!/bin/sh\necho \"$@\" > {}\n", record.display()),
            );
            assert_eq!(
                open_with(&herdr, "herdr-voice", "settings", Duration::from_secs(10)),
                Ok(())
            );
            assert_eq!(
                std::fs::read_to_string(&record).unwrap().trim(),
                "plugin pane open --plugin herdr-voice --entrypoint settings"
            );
        }

        #[test]
        fn a_refusal_from_herdr_is_reported_with_what_it_said_and_where_to_look() {
            let herdr = script("refuse", "#!/bin/sh\necho boom >&2\nexit 1\n");
            let said =
                open_with(&herdr, "herdr-voice", "settings", Duration::from_secs(10)).unwrap_err();
            assert!(said.contains("boom"), "{said}");
            assert!(
                said.contains("herdr plugin log list --plugin herdr-voice"),
                "{said}"
            );
        }

        #[test]
        fn what_herdr_said_is_cut_to_a_length_a_person_can_read() {
            let herdr = script("long", "#!/bin/sh\nyes x | head -c 5000 >&2\nexit 1\n");
            let said =
                open_with(&herdr, "herdr-voice", "settings", Duration::from_secs(10)).unwrap_err();
            assert!(said.len() < 600, "{} bytes", said.len());
        }

        #[test]
        fn a_herdr_that_does_not_answer_is_given_up_on_and_the_bound_is_named() {
            let herdr = script("slow", "#!/bin/sh\nsleep 5\n");
            let said =
                open_with(&herdr, "herdr-voice", "settings", Duration::from_secs(1)).unwrap_err();
            assert!(said.contains("did not answer within 1 seconds"), "{said}");
        }

        #[test]
        fn a_herdr_that_is_not_there_says_how_to_find_it() {
            let said = open_with(
                "/definitely/not/a/real/herdr",
                "herdr-voice",
                "settings",
                Duration::from_secs(1),
            )
            .unwrap_err();
            assert!(said.contains("cannot run"), "{said}");
            assert!(said.contains("PATH"), "{said}");
        }

        #[test]
        fn an_unreadable_configuration_is_noted_and_an_absent_one_is_not() {
            use std::os::unix::fs::PermissionsExt;
            let dir =
                std::env::temp_dir().join(format!("herdr-voice-popup-note-{}", std::process::id()));
            std::fs::create_dir_all(&dir).unwrap();
            let file = dir.join("config.toml");
            std::fs::write(&file, "[audio]\n").unwrap();
            std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o000)).unwrap();
            let note = config_note(&Source::Defaults(Some(file.clone())));
            std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600)).unwrap();
            assert!(note.expect("a note").contains("cannot be read"));
            assert_eq!(
                config_note(&Source::Defaults(Some(dir.join("absent.toml")))),
                None
            );
        }
    }

    #[test]
    fn saving_writes_the_key_tells_the_daemon_and_says_what_it_does() {
        let mut world = tests_support::FakeWorld::new("save-ok", "[ui]\ntoasts = true\n");
        world.reached = applied(&[], &["ui"]);
        let mut input = Cursor::new(Vec::new());
        let mut out: Vec<u8> = Vec::new();
        let mut io = Io::new(&mut input, &mut out);
        let saved = save_and_tell(
            &mut world,
            &mut io,
            "ui",
            "toasts",
            "false".to_string(),
            "false",
        );
        assert!(saved && !io.failed);
        assert_eq!(world.told, 1);
        assert!(world.file().contains("toasts = false"), "{}", world.file());
        let said = String::from_utf8(out).unwrap();
        assert!(said.contains("[ui] toasts is now false"), "{said}");
        assert!(said.contains("needs a restart of herdr"), "{said}");
    }

    #[test]
    fn a_refused_write_does_not_tell_the_daemon_and_does_not_ask_for_a_hand_edit_that_would_fail_too(
    ) {
        let mut world = tests_support::FakeWorld::new("save-refused", "[ui]\nblink_ms = 250\n");
        let mut input = Cursor::new(Vec::new());
        let mut out: Vec<u8> = Vec::new();
        let mut io = Io::new(&mut input, &mut out);
        let saved = save_and_tell(
            &mut world,
            &mut io,
            "ui",
            "blink_ms",
            crate::config_edit::quote("fast"),
            "\"fast\"",
        );
        assert!(!saved && io.failed);
        assert_eq!(world.told, 0);
        assert_eq!(world.file(), "[ui]\nblink_ms = 250\n");
        let said = String::from_utf8(out).unwrap();
        assert!(said.contains("nothing was written"), "{said}");
        assert!(!said.contains("by hand"), "{said}");
    }

    #[test]
    fn a_write_that_could_not_happen_prints_the_line_to_add_by_hand() {
        let mut world = tests_support::FakeWorld::new("save-io", "");
        world.save_error = Some(WriteError::Io {
            path: "config.toml".to_string(),
            why: "permission denied".to_string(),
        });
        let mut input = Cursor::new(Vec::new());
        let mut out: Vec<u8> = Vec::new();
        let mut io = Io::new(&mut input, &mut out);
        let saved = save_and_tell(
            &mut world,
            &mut io,
            "audio",
            "input",
            crate::config_edit::quote("Mic \"B\""),
            "\"Mic \\\"B\\\"\"",
        );
        assert!(!saved && io.failed);
        let said = String::from_utf8(out).unwrap();
        assert!(said.contains("permission denied"), "{said}");
        assert!(said.contains("Add this under [audio]"), "{said}");
        assert!(
            said.contains("input = \"Mic \\\"B\\\"\""),
            "valid TOML: {said}"
        );
        assert_eq!(world.told, 0);
    }
}
````

- [ ] **Step 2: Run to see it fail**

Run: `CARGO_BUILD_JOBS=6 perl -e 'alarm shift; exec @ARGV' 1500 cargo test --bin herdr-voice popup::`
Expected: compile errors, `cannot find function parse_answer` and the other items.

- [ ] **Step 3: Implement**

Put this at the top of `src/popup.rs`, above the test module:

````rust
//! What a popup of this plugin needs: the terminal, an answer read from it, the
//! daemon, and the seam (`World`) every flow talks to so that it can be tested with
//! no file, no daemon, no sound device, no download and no server.
//!
//! See `tasks/104/DESIGN_104.md`, section 2.2.

use std::io::{BufRead, IsTerminal, Write};
use std::path::PathBuf;
use std::time::Duration;

use crate::client::{self, ClientError};
use crate::config::{Loaded, Source, FILE_NAME};
use crate::config_edit::{self, Edit, WriteError};
use crate::proto::Reply;
use crate::reload;
use crate::rewrite_models::{self, ListFailure};
use crate::stt::candle::store::{self, Glance};
use crate::stt::catalogue::Entry;
use crate::stt::fetch;
use crate::transport::Address;

/// The bound on asking herdr to open the popup, the same as every other call to
/// herdr (`docs/decisions.md`).
pub const OPEN_BOUND: Duration = Duration::from_secs(10);

/// What every question that can be left says, so the words are the same everywhere.
pub const LEAVE_HINT: &str = "Esc then Enter, or an empty line, leaves it as it is";

/// What came of telling the daemon about a change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reached {
    /// The daemon answered a reload.
    Applied(reload::Applied),
    /// Nothing is listening: the change applies when a daemon starts.
    NoDaemon,
    /// The daemon did not take it, with the reason in words.
    Failed(String),
}

/// Maps what `client::exchange` returned onto what a popup does. The wording of a
/// failure is the client's own, so a person sees the same sentence here as from
/// `herdr-voice cancel`.
pub fn reached(result: Result<Reply, ClientError>) -> Reached {
    match result {
        Err(ClientError::NoDaemon(_)) => Reached::NoDaemon,
        Ok(Reply::Ok(text)) => match reload::parse(&text) {
            Some(applied) => Reached::Applied(applied),
            None => Reached::Failed(format!(
                "the daemon answered {text:?}, which is not a reload"
            )),
        },
        Ok(Reply::Error(text)) => Reached::Failed(text),
        Err(other) => Reached::Failed(client::outcome(Err(other)).message.unwrap_or_default()),
    }
}

/// Asks the daemon at `address` to read the configuration again.
pub fn tell_daemon(address: &Address) -> Reached {
    reached(client::exchange(address, "reload", None, Vec::new()))
}

#[derive(Debug, PartialEq, Eq)]
pub enum Answer {
    /// Esc, or nothing: leave what is being asked.
    Leave,
    /// The zero-based position in the list.
    Pick(usize),
    Invalid(String),
}

/// A line is read, so Esc and the arrow keys arrive as text that begins with the
/// Esc character, and only after Enter. Anything that begins with it leaves.
pub fn parse_answer(line: &str, count: usize) -> Answer {
    let text = line.trim();
    if text.is_empty() || text.starts_with('\u{1b}') {
        return Answer::Leave;
    }
    match text.parse::<usize>() {
        Ok(n) if (1..=count).contains(&n) => Answer::Pick(n - 1),
        _ => Answer::Invalid(text.to_string()),
    }
}

/// The sentence for an answer that is not a number in the list.
pub fn not_in_list(text: &str, count: usize) -> String {
    format!("{text:?} is not one of the numbers above; type a number between 1 and {count}")
}

/// A question, flushed: standard output is line buffered and a question has no
/// newline, so without the flush it stays unseen while the process waits.
pub fn prompt(out: &mut dyn Write, question: &str) {
    let _ = write!(out, "{question}");
    let _ = out.flush();
}

/// The terminal a popup talks to.
pub struct Io<'a> {
    pub input: &'a mut dyn BufRead,
    pub out: &'a mut dyn Write,
    /// Set by any flow that ends in something the person has to act on. The popup's
    /// exit code is 1 when it is set.
    pub failed: bool,
    /// The end of the input was reached, and said.
    ended: bool,
}

impl<'a> Io<'a> {
    pub fn new(input: &'a mut dyn BufRead, out: &'a mut dyn Write) -> Io<'a> {
        Io {
            input,
            out,
            failed: false,
            ended: false,
        }
    }

    pub fn say(&mut self, text: &str) {
        let _ = writeln!(self.out, "{text}");
    }

    pub fn fail(&mut self) {
        self.failed = true;
    }

    /// Asks `question` and returns the line read. `None` means there is nothing more
    /// to read; that is said once, the popup is marked failed, and every later
    /// question answers `None` at once, so every level of a menu unwinds.
    pub fn ask(&mut self, question: &str) -> Option<String> {
        if self.ended {
            return None;
        }
        prompt(self.out, question);
        let mut line = String::new();
        match self.input.read_line(&mut line) {
            Ok(0) | Err(_) => {
                self.ended = true;
                self.failed = true;
                self.say(
                    "\nnothing was read from the terminal; open the settings again from herdr",
                );
                None
            }
            Ok(_) => Some(line),
        }
    }
}

/// The configuration file as it stands, read again each time it is asked for.
pub struct Snapshot {
    /// The file's text; empty when there is none.
    pub text: String,
    /// What `config::load` makes of it.
    pub loaded: Loaded,
}

impl Snapshot {
    /// The file as a table, when it parses; which keys are set is read from it.
    pub fn table(&self) -> Option<toml::Table> {
        toml::from_str(&self.text).ok()
    }
}

/// Everything a flow asks of the outside. `settings::Real` is the world of a real
/// run; tests use `tests_support::FakeWorld`.
pub trait World {
    fn snapshot(&self) -> Snapshot;
    fn input_names(&self) -> Result<Vec<String>, String>;
    fn save(&mut self, edits: &[Edit]) -> Result<PathBuf, WriteError>;
    fn tell(&mut self) -> Reached;
    fn models_dir(&self) -> Option<PathBuf>;
    fn model_state(&self, entry: &Entry) -> Glance;
    /// Downloads and verifies a model. Shows its own progress.
    fn install(&mut self, entry: &Entry) -> Result<(), String>;
    fn rewrite_models(&mut self, url: &str, token: &str) -> Result<Vec<String>, ListFailure>;
}

/// What a saved change does, in words, and whether the person has to act on it.
pub fn change_note(section: &str, reached: &Reached) -> (String, bool) {
    match reached {
        Reached::NoDaemon => (
            "No dictation daemon is running, so nothing was told; the change applies when it starts."
                .to_string(),
            false,
        ),
        Reached::Failed(why) => (
            format!(
                "The file was changed, but the daemon did not take it: {why}. Restart herdr, or \
                 check `herdr plugin log list --plugin herdr-voice`."
            ),
            true,
        ),
        Reached::Applied(done) => {
            let applied = done.applied.iter().any(|s| s == section);
            let needs = done.restart.iter().any(|s| s == section);
            let mut text = if section == "audio" {
                if applied {
                    "The daemon applied it: the next take records from it.".to_string()
                } else {
                    "The daemon already uses it; nothing to apply.".to_string()
                }
            } else if needs {
                format!("This needs a restart of herdr to apply: [{section}].")
            } else {
                "The daemon already runs with this value; nothing to apply.".to_string()
            };
            let others: Vec<&str> = done
                .restart
                .iter()
                .filter(|s| s.as_str() != section)
                .map(String::as_str)
                .collect();
            if !others.is_empty() {
                text.push_str(&format!(
                    "\nOther changes in the file also need a restart of herdr: {}.",
                    others.join(", ")
                ));
            }
            (text, false)
        }
    }
}

/// Writes one key, tells the daemon, and says what happened and what to do next.
/// `value` is the TOML text written; `shown` is how the value is named to the person.
/// Returns whether the key was written. A failure sets `io.failed`.
pub fn save_and_tell(
    world: &mut dyn World,
    io: &mut Io,
    section: &str,
    key: &str,
    value: String,
    shown: &str,
) -> bool {
    let edits = [Edit {
        table: section,
        key,
        value: value.clone(),
    }];
    match world.save(&edits) {
        Ok(path) => {
            io.say(&format!(
                "[{section}] {key} is now {shown} in {}.",
                path.display()
            ));
            let (note, failed) = change_note(section, &world.tell());
            io.say(&note);
            if failed {
                io.fail();
            }
            true
        }
        Err(why) => {
            io.say(&why.to_string());
            if why.needs_hand_edit() {
                io.say(&format!(
                    "Add this under [{section}] in your configuration file by hand:"
                ));
                io.say(&format!("  {key} = {value}"));
            }
            io.fail();
            false
        }
    }
}

/// A line to print before anything else when the configuration cannot be taken at
/// its word: `config::load` returns every default for a file that does not parse or
/// cannot be read, and a list would then show defaults for settings the file may set.
pub fn config_note(source: &Source) -> Option<String> {
    match source {
        Source::Invalid { path, why } => Some(format!(
            "{} does not parse ({why}): the settings shown are the defaults, and saving a \
             change is refused until the file is fixed.\n",
            path.display()
        )),
        Source::Defaults(Some(path))
            if std::fs::read_to_string(path)
                .is_err_and(|e| e.kind() != std::io::ErrorKind::NotFound) =>
        {
            Some(format!(
                "{} cannot be read: the settings shown are the defaults, and saving a \
                 change is refused until it can be.\n",
                path.display()
            ))
        }
        _ => None,
    }
}

/// Waits for Enter when there is a person at a terminal, so a result printed just
/// before the process exits is not gone with the pane. Whether herdr closes a popup
/// the moment its command exits was not established (`tasks/104/DESIGN_104.md`,
/// section 5).
pub fn pause() {
    if std::io::stdin().is_terminal() {
        print!("\nPress Enter to close.");
        let _ = std::io::stdout().flush();
        let mut ignored = String::new();
        let _ = std::io::stdin().read_line(&mut ignored);
    }
}

/// The command that asks herdr to open the pane `entrypoint` of `plugin`.
pub fn open_command(herdr: &str, plugin: &str, entrypoint: &str) -> Vec<String> {
    [
        herdr,
        "plugin",
        "pane",
        "open",
        "--plugin",
        plugin,
        "--entrypoint",
        entrypoint,
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

/// Asks `herdr` to open the pane, waiting at most `bound`. The `Err` is the message
/// for the person.
pub fn open_with(
    herdr: &str,
    plugin: &str,
    entrypoint: &str,
    bound: Duration,
) -> Result<(), String> {
    let argv = open_command(herdr, plugin, entrypoint);
    let mut command = std::process::Command::new(&argv[0]);
    command.args(&argv[1..]);
    match crate::outward::run(&mut command, bound) {
        Ok(output) if output.status.success() => Ok(()),
        Ok(output) => Err(format!(
            "herdr could not open the settings popup: {}. Check \
             `herdr plugin log list --plugin herdr-voice`",
            crate::outward::shorten(String::from_utf8_lossy(&output.stderr).trim(), 300)
        )),
        Err(crate::outward::RunError::TimedOut) => Err(format!(
            "herdr did not answer within {} seconds when asked to open the popup. Check \
             that herdr is running, then try again",
            bound.as_secs()
        )),
        Err(crate::outward::RunError::Start(e)) => Err(format!(
            "cannot run {herdr:?}: {e}. Check that herdr is installed and on the PATH"
        )),
    }
}

/// A progress line that overwrites itself, so a 3 GB download is one line.
#[derive(Default)]
pub struct Line {
    name: String,
    total: u64,
}

impl fetch::Progress for Line {
    fn file(&mut self, name: &str, total: u64) {
        self.name = name.to_string();
        self.total = total;
    }
    fn bytes(&mut self, done: u64) {
        // A zero total means the catalogue said the file is empty, which it
        // never does; checked_div keeps that from being a division by zero
        // anyway, because a progress line is not worth a panic.
        let percent = (done * 100).checked_div(self.total).unwrap_or(0);
        print!("\r  {} {percent:>3}%  ", self.name);
        let _ = std::io::stdout().flush();
    }
    fn done(&mut self, name: &str) {
        println!("\r  {name} done            ");
    }
}

/// The world of a real run: the real file, daemon, devices, models directory, download
/// and server.
pub struct Real {
    directory: Option<PathBuf>,
    models: Option<PathBuf>,
}

impl Real {
    pub fn from_env() -> Real {
        let vars = crate::config::Vars::from_env();
        Real {
            directory: crate::config::directory(&vars),
            models: crate::transport::state_directory(&crate::transport::Vars::from_env())
                .map(|state| state.join("models")),
        }
    }
}

impl World for Real {
    fn snapshot(&self) -> Snapshot {
        let text = self
            .directory
            .as_ref()
            .and_then(|dir| std::fs::read_to_string(dir.join(FILE_NAME)).ok())
            .unwrap_or_default();
        Snapshot {
            text,
            loaded: crate::config::load(self.directory.as_deref()),
        }
    }

    fn input_names(&self) -> Result<Vec<String>, String> {
        crate::capture::cpal_source::input_names()
    }

    fn save(&mut self, edits: &[config_edit::Edit]) -> Result<PathBuf, WriteError> {
        config_edit::write_keys(self.directory.as_deref(), edits)
    }

    fn tell(&mut self) -> Reached {
        match crate::transport::address(&crate::transport::Vars::from_env()) {
            Ok(address) => tell_daemon(&address),
            Err(e) => Reached::Failed(e.to_string()),
        }
    }

    fn models_dir(&self) -> Option<PathBuf> {
        self.models.clone()
    }

    fn model_state(&self, entry: &Entry) -> Glance {
        match &self.models {
            Some(models) => store::glance(models, entry.identifier, Some(entry)),
            None => Glance::Absent,
        }
    }

    fn install(&mut self, entry: &Entry) -> Result<(), String> {
        let Some(models) = &self.models else {
            return Err(
                "cannot tell where models live: neither HERDR_PLUGIN_STATE_DIR nor a home \
                 directory is set, so there is nowhere to put one. Set HERDR_PLUGIN_STATE_DIR \
                 and try again"
                    .to_string(),
            );
        };
        let mut line = Line::default();
        fetch::model(entry.identifier, models, &mut line).map_err(|e| e.to_string())?;
        // Verify what was just downloaded with the real check, not the glance the
        // listing uses: this is the moment a bad download must be caught.
        store::locate(models, entry.identifier, Some(entry)).map_err(|e| e.to_string())?;
        Ok(())
    }

    fn rewrite_models(&mut self, url: &str, token: &str) -> Result<Vec<String>, ListFailure> {
        rewrite_models::fetch(url, token, rewrite_models::LIST_BOUND)
    }
}

/// A `World` for tests: a real configuration directory under the temporary directory,
/// the real writer, and everything else recorded and answered from fields.
#[cfg(test)]
pub mod tests_support {
    use super::*;

    pub struct FakeWorld {
        pub dir: PathBuf,
        pub names: Result<Vec<String>, String>,
        pub reached: Reached,
        pub told: usize,
        /// `[table] key = value` for every edit saved, in order.
        pub saved: Vec<String>,
        pub save_error: Option<WriteError>,
        pub models: Option<PathBuf>,
        pub states: Vec<(String, Glance)>,
        pub installs: Vec<String>,
        pub install_error: Option<String>,
        pub list: Result<Vec<String>, ListFailure>,
        pub list_calls: Vec<(String, String)>,
    }

    impl FakeWorld {
        /// A world whose configuration file holds `text` (no file when it is empty).
        pub fn new(tag: &str, text: &str) -> FakeWorld {
            let dir = std::env::temp_dir()
                .join(format!("herdr-voice-world-{tag}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            if !text.is_empty() {
                std::fs::write(dir.join("config.toml"), text).unwrap();
            }
            FakeWorld {
                dir,
                names: Ok(Vec::new()),
                reached: Reached::NoDaemon,
                told: 0,
                saved: Vec::new(),
                save_error: None,
                models: Some(PathBuf::from("/models")),
                states: Vec::new(),
                installs: Vec::new(),
                install_error: None,
                list: Err(ListFailure::Empty),
                list_calls: Vec::new(),
            }
        }

        pub fn file(&self) -> String {
            std::fs::read_to_string(self.dir.join("config.toml")).unwrap_or_default()
        }
    }

    impl World for FakeWorld {
        fn snapshot(&self) -> Snapshot {
            Snapshot {
                text: self.file(),
                loaded: crate::config::load(Some(&self.dir)),
            }
        }

        fn input_names(&self) -> Result<Vec<String>, String> {
            self.names.clone()
        }

        fn save(&mut self, edits: &[Edit]) -> Result<PathBuf, WriteError> {
            for edit in edits {
                self.saved
                    .push(format!("[{}] {} = {}", edit.table, edit.key, edit.value));
            }
            if let Some(error) = self.save_error.take() {
                return Err(error);
            }
            crate::config_edit::write_keys(Some(&self.dir), edits)
        }

        fn tell(&mut self) -> Reached {
            self.told += 1;
            self.reached.clone()
        }

        fn models_dir(&self) -> Option<PathBuf> {
            self.models.clone()
        }

        fn model_state(&self, entry: &Entry) -> Glance {
            self.states
                .iter()
                .find(|(id, _)| id == entry.identifier)
                .map(|(_, state)| *state)
                .unwrap_or(Glance::Absent)
        }

        fn install(&mut self, entry: &Entry) -> Result<(), String> {
            self.installs.push(entry.identifier.to_string());
            if let Some(why) = self.install_error.take() {
                return Err(why);
            }
            self.states.retain(|(id, _)| id != entry.identifier);
            self.states
                .push((entry.identifier.to_string(), Glance::Whole));
            Ok(())
        }

        fn rewrite_models(&mut self, url: &str, token: &str) -> Result<Vec<String>, ListFailure> {
            self.list_calls.push((url.to_string(), token.to_string()));
            self.list.clone()
        }
    }
}
````

- [ ] **Step 4: Run to see it pass**

Run: `CARGO_BUILD_JOBS=6 perl -e 'alarm shift; exec @ARGV' 1500 cargo test --bin herdr-voice popup::`
Expected: 23 tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/popup.rs src/main.rs src/reload.rs
git commit -m "Add the popup's primitives and its seam: Io, World, change_note, Real

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 4: The speech-model flow, `src/chooser.rs`

**Files:**
- Modify: `src/chooser.rs` (replaced)

**Interfaces:**
- Consumes: Task 3's `Io`, `World`, `Real`, `Answer`, `parse_answer`, `not_in_list`, `save_and_tell`, `config_note`, `LEAVE_HINT`, `tests_support::FakeWorld`.
- Produces (exact): `pub fn list(catalogue: &'static [Entry], configured: &str, state: &dyn Fn(&Entry) -> Glance, models: Option<&Path>) -> String`; `pub fn choose_speech_model(world: &mut dyn World, io: &mut Io, catalogue: &'static [Entry])`; `pub fn run(choosing: bool) -> u8`. The old `pick`, the old `list(models, configured)` and `Line` are gone (`Line` is in `popup`).

- [ ] **Step 1: Write the failing tests**

In `src/chooser.rs` replace everything from `#[cfg(test)]` to the end of the file with this test module (the old tests of `list`, `pick` and `human` are replaced by these):

````rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::popup::tests_support::FakeWorld;
    use crate::popup::Reached;
    use std::io::Cursor;
    use std::path::PathBuf;

    /// Runs the flow with `typed` as the person's input; returns the printed text.
    fn drive(world: &mut FakeWorld, typed: &str, catalogue: &'static [Entry]) -> (String, bool) {
        let mut input = Cursor::new(typed.as_bytes().to_vec());
        let mut out: Vec<u8> = Vec::new();
        let mut io = Io::new(&mut input, &mut out);
        choose_speech_model(world, &mut io, catalogue);
        let failed = io.failed;
        (String::from_utf8(out).unwrap(), failed)
    }

    fn state(glance: Glance) -> impl Fn(&Entry) -> Glance {
        move |_| glance
    }

    #[test]
    fn the_listing_shows_every_model_with_a_size_before_anything_is_downloaded() {
        let text = list(
            &catalogue::MODELS,
            "large-v3-turbo",
            &state(Glance::Absent),
            Some(&PathBuf::from("/nowhere-at-all")),
        );
        for entry in catalogue::MODELS.iter() {
            assert!(
                text.contains(entry.identifier),
                "{} is missing: {text}",
                entry.identifier
            );
        }
        assert!(text.contains("151 MB"), "tiny's size: {text}");
        assert!(text.contains("1.62 GB"), "the default's size: {text}");
        assert!(
            text.contains("not installed"),
            "it must say what is there: {text}"
        );
        assert!(text.contains("/nowhere-at-all/candle"), "{text}");
    }

    #[test]
    fn the_listing_says_what_is_installed_and_what_is_the_wrong_size() {
        let whole = list(&catalogue::MODELS, "", &state(Glance::Whole), None);
        assert!(whole.contains("— installed"), "{whole}");
        assert!(!whole.contains("not installed"), "{whole}");
        let wrong = list(&catalogue::MODELS, "", &state(Glance::WrongSize), None);
        assert!(wrong.contains("the wrong size"), "{wrong}");
        assert!(
            !wrong.contains("They live in"),
            "no directory, no line about it: {wrong}"
        );
    }

    #[test]
    fn the_configured_model_is_marked_in_the_listing() {
        let text = list(&catalogue::MODELS, "small", &state(Glance::Absent), None);
        let marked: Vec<&str> = text.lines().filter(|l| l.contains("small")).collect();
        assert_eq!(marked.len(), 1, "got {marked:?}");
        assert!(marked[0].contains("current"), "got {}", marked[0]);
    }

    #[test]
    fn sizes_are_rendered_the_way_a_person_reads_them() {
        assert_eq!(human(151_061_672), "151 MB");
        assert_eq!(human(1_617_824_864), "1.62 GB");
        assert_eq!(human(3_087_130_976), "3.09 GB");
    }

    #[test]
    fn with_the_candle_engine_a_model_that_is_not_there_is_installed_and_written() {
        let mut world = FakeWorld::new(
            "choose-candle",
            "[stt]\nengine = \"candle\"\nlanguage = \"ru\"\n",
        );
        world.reached = Reached::NoDaemon;
        let (said, failed) = drive(&mut world, "1\n", &catalogue::MODELS);
        assert!(!failed, "{said}");
        assert_eq!(world.installs, vec!["tiny".to_string()]);
        assert_eq!(world.saved, vec!["[stt] model = \"tiny\"".to_string()]);
        assert_eq!(world.told, 1);
        assert!(said.contains("Installing tiny (151 MB)"), "{said}");
        // The configuration now uses the model, and nothing else in it changed.
        let file = world.file();
        assert!(file.contains("model = \"tiny\""), "{file}");
        assert!(
            file.contains("engine = \"candle\"") && file.contains("language = \"ru\""),
            "{file}"
        );
    }

    #[test]
    fn with_the_candle_engine_a_model_that_is_there_but_not_configured_is_not_downloaded_again() {
        let mut world = FakeWorld::new("choose-candle-there", "[stt]\nengine = \"candle\"\n");
        world.states.push(("small".to_string(), Glance::Whole));
        let (said, _) = drive(&mut world, "3\n", &catalogue::MODELS);
        assert!(world.installs.is_empty(), "{said}");
        assert_eq!(world.saved, vec!["[stt] model = \"small\"".to_string()]);
        assert!(said.contains("small is already installed"), "{said}");
    }

    #[test]
    fn with_the_candle_engine_the_configured_installed_model_is_said_to_be_so_and_nothing_happens()
    {
        let mut world = FakeWorld::new(
            "choose-candle-configured",
            "[stt]\nengine = \"candle\"\nmodel = \"tiny\"\n",
        );
        world.states.push(("tiny".to_string(), Glance::Whole));
        let before = world.file();
        let (said, failed) = drive(&mut world, "1\n", &catalogue::MODELS);
        assert!(!failed);
        assert!(
            said.contains("tiny is installed and already configured"),
            "{said}"
        );
        assert!(world.installs.is_empty() && world.saved.is_empty());
        assert_eq!(world.told, 0);
        assert_eq!(world.file(), before);
    }

    #[test]
    fn with_the_candle_engine_the_configured_model_that_is_missing_is_installed_without_a_write() {
        let mut world = FakeWorld::new(
            "choose-candle-missing",
            "[stt]\nengine = \"candle\"\nmodel = \"tiny\"\n",
        );
        let (said, _) = drive(&mut world, "1\n", &catalogue::MODELS);
        assert_eq!(world.installs, vec!["tiny".to_string()]);
        assert!(world.saved.is_empty(), "the key already says tiny: {said}");
    }

    #[test]
    fn a_failed_download_says_so_writes_nothing_and_fails() {
        let mut world = FakeWorld::new("choose-install-fails", "[stt]\nengine = \"candle\"\n");
        world.install_error = Some("cannot reach huggingface.co; check the network".to_string());
        let (said, failed) = drive(&mut world, "1\n", &catalogue::MODELS);
        assert!(failed);
        assert!(said.contains("cannot reach huggingface.co"), "{said}");
        assert!(world.saved.is_empty());
        assert_eq!(world.told, 0);
    }

    #[test]
    fn with_a_command_that_uses_the_model_nothing_is_downloaded_or_written_and_the_file_is_named() {
        let mut world = FakeWorld::new(
            "choose-command-model",
            "[stt]\nengine = \"command\"\ncommand = [\"whisper-cli\", \"-m\", \"{model}\"]\n",
        );
        let before = world.file();
        let (said, failed) = drive(&mut world, "1\n", &catalogue::MODELS);
        assert!(!failed, "{said}");
        assert!(world.installs.is_empty() && world.saved.is_empty());
        assert_eq!(world.file(), before);
        assert!(said.contains("ggml-tiny.bin"), "{said}");
        assert!(
            said.contains("/models/ggml-tiny.bin"),
            "the models directory is named: {said}"
        );
        assert!(said.contains("set [stt] engine to \"candle\""), "{said}");
        assert!(said.contains("Nothing was downloaded or written"), "{said}");
    }

    #[test]
    fn with_a_command_that_brings_its_own_model_the_model_key_is_said_not_to_be_used() {
        // The shipped default: engine "command" and an empty command.
        let mut world = FakeWorld::new("choose-command-own", "");
        let (said, _) = drive(&mut world, "1\n", &catalogue::MODELS);
        assert!(world.installs.is_empty() && world.saved.is_empty());
        assert!(said.contains("does not use {model}"), "{said}");
        assert!(said.contains("[stt] model is not used"), "{said}");
        assert!(said.contains("set [stt] engine to \"candle\""), "{said}");
    }

    #[test]
    fn with_the_http_engine_the_model_key_is_said_not_to_be_used() {
        let mut world = FakeWorld::new("choose-http", "[stt]\nengine = \"http\"\n");
        let (said, _) = drive(&mut world, "1\n", &catalogue::MODELS);
        assert!(world.installs.is_empty() && world.saved.is_empty());
        assert!(said.contains("[stt] http_model"), "{said}");
        assert!(said.contains("set [stt] engine to \"candle\""), "{said}");
    }

    #[test]
    fn an_engine_that_is_none_of_the_three_is_named_with_the_three() {
        let mut world = FakeWorld::new("choose-unknown", "[stt]\nengine = \"whisperx\"\n");
        let (said, _) = drive(&mut world, "1\n", &catalogue::MODELS);
        assert!(world.installs.is_empty() && world.saved.is_empty());
        assert!(
            said.contains("\"whisperx\"") && said.contains("candle, http, command"),
            "{said}"
        );
    }

    #[test]
    fn an_empty_catalogue_says_what_happened_and_fails() {
        let mut world = FakeWorld::new("choose-empty", "");
        let (said, failed) = drive(&mut world, "1\n", &[]);
        assert!(failed);
        assert!(said.contains("No speech models are on offer"), "{said}");
        assert!(said.contains("report it"), "{said}");
    }

    #[test]
    fn esc_an_empty_line_and_a_number_outside_the_list_change_nothing() {
        for (typed, expected) in [
            ("\u{1b}\n", "Nothing was changed"),
            ("\n", "Nothing was changed"),
            ("9\n", "between 1 and 6"),
        ] {
            let mut world = FakeWorld::new("choose-leave", "[stt]\nengine = \"candle\"\n");
            let (said, failed) = drive(&mut world, typed, &catalogue::MODELS);
            assert!(!failed, "{typed:?}: {said}");
            assert!(said.contains(expected), "{typed:?}: {said}");
            assert!(world.installs.is_empty() && world.saved.is_empty());
        }
    }

    #[test]
    fn the_end_of_the_input_is_said_and_fails() {
        let mut world = FakeWorld::new("choose-eof", "[stt]\nengine = \"candle\"\n");
        let (said, failed) = drive(&mut world, "", &catalogue::MODELS);
        assert!(failed);
        assert!(
            said.contains("nothing was read from the terminal"),
            "{said}"
        );
    }

    #[test]
    fn a_configuration_that_does_not_parse_is_said_before_the_list() {
        let mut world = FakeWorld::new("choose-invalid", "[stt\nengine = ");
        let (said, _) = drive(&mut world, "\n", &catalogue::MODELS);
        assert!(said.contains("does not parse"), "{said}");
        assert!(said.find("does not parse").unwrap() < said.find("1. tiny").unwrap());
    }
}
````

- [ ] **Step 2: Run to see it fail**

Run: `CARGO_BUILD_JOBS=6 perl -e 'alarm shift; exec @ARGV' 1500 cargo test --bin herdr-voice chooser::`
Expected: compile errors (the new `list` signature, `choose_speech_model`).

- [ ] **Step 3: Implement**

Replace everything above `#[cfg(test)]` in `src/chooser.rs` with:

````rust
//! Choosing a speech model, and downloading the one chosen.
//!
//! Two entry points because two questions are asked: `herdr-voice model` says
//! what exists, and `--choose` spends the gigabytes. The same flow,
//! `choose_speech_model`, is the speech-model entry of the settings popup. The
//! configuration edit is `config_edit`'s: it changes one key and leaves everything
//! else in the file. See `tasks/15/DESIGN_15.md`, section 5, and
//! `tasks/104/DESIGN_104.md`, section 2.4.

use std::path::Path;

use crate::popup::{
    config_note, not_in_list, parse_answer, save_and_tell, Answer, Io, Real, World, LEAVE_HINT,
};
use crate::stt::candle::store::Glance;
use crate::stt::catalogue::{self, Entry};

/// A size the way a person reads it, not the way a computer stores it.
fn human(bytes: u64) -> String {
    const MB: f64 = 1_000_000.0;
    const GB: f64 = 1_000_000_000.0;
    let b = bytes as f64;
    if b >= GB {
        format!("{:.2} GB", b / GB)
    } else {
        format!("{:.0} MB", b / MB)
    }
}

/// The catalogue, with each model's size and whether it is already there.
///
/// Presence is judged by `store::glance`, not `store::locate`: listing six
/// models must not hash six multi-gigabyte files. A person asking what is
/// installed is asking a question about names and sizes, and waiting a minute
/// for the answer would be absurd. The full check happens where it matters —
/// before a model is loaded, and again right after a download.
pub fn list(
    catalogue: &'static [Entry],
    configured: &str,
    state: &dyn Fn(&Entry) -> Glance,
    models: Option<&Path>,
) -> String {
    let mut out = String::from("Speech models this plugin can install:\n\n");
    for (i, entry) in catalogue.iter().enumerate() {
        let size = human(catalogue::weights(entry).bytes);
        let present = match state(entry) {
            Glance::Whole => "installed",
            Glance::Absent => "not installed",
            Glance::WrongSize => "installed, but the wrong size",
        };
        let marker = if entry.identifier == configured {
            "  (current)"
        } else {
            ""
        };
        out.push_str(&format!(
            "  {}. {:<16} {:>8}  {} mel bins  — {present}{marker}\n",
            i + 1,
            entry.identifier,
            size,
            entry.mel_bins
        ));
    }
    if let Some(models) = models {
        out.push_str("\nThey live in ");
        out.push_str(&models.join("candle").display().to_string());
        out.push('\n');
    }
    out
}

/// What to do next for an engine that does not use a model from this catalogue. The
/// catalogue installs weights for the built-in engine, `candle`, and a person who
/// uses another engine decides whether to move to it; the popup never does.
const MOVE_TO_CANDLE: &str = "To use a model from this list, set [stt] engine to \"candle\" \
     in the settings popup, then choose the model again";

/// The speech-model flow: list the catalogue, take a number, and by the engine in
/// the configuration either install and write `[stt] model`, or say why not and
/// what to change. Writes nothing for an engine that would not use the model.
pub fn choose_speech_model(world: &mut dyn World, io: &mut Io, catalogue: &'static [Entry]) {
    if catalogue.is_empty() {
        io.say(
            "No speech models are on offer in this build, which is a defect in the build: \
             report it, and set [stt] model by hand in the meantime.",
        );
        io.fail();
        return;
    }
    let snapshot = world.snapshot();
    if let Some(note) = config_note(&snapshot.loaded.source) {
        io.say(&note);
    }
    let stt = snapshot.loaded.config.stt.clone();
    let models = world.models_dir();
    let listing = list(
        catalogue,
        &stt.model,
        &|entry| world.model_state(entry),
        models.as_deref(),
    );
    io.say(&listing);

    let Some(line) = io.ask(&format!(
        "Type the number of the model, then Enter. {LEAVE_HINT}: "
    )) else {
        return;
    };
    let entry = match parse_answer(&line, catalogue.len()) {
        Answer::Leave => {
            io.say("Nothing was changed.");
            return;
        }
        Answer::Invalid(text) => {
            io.say(&not_in_list(&text, catalogue.len()));
            return;
        }
        Answer::Pick(at) => &catalogue[at],
    };
    let id = entry.identifier;

    match stt.engine.as_str() {
        "candle" => install_and_write(world, io, entry, &stt.model),
        "command" if crate::stt::wants_our_model(&stt.command) => {
            let file = crate::stt::model::file_name(id);
            let place = match &models {
                Some(dir) => dir.join(&file).display().to_string(),
                None => format!("{file} in the models directory"),
            };
            io.say(&format!(
                "[stt] engine is \"command\" and its command uses {{model}}, so a take looks for \
                 the file {place}. The models in this list are weights for the built-in engine \
                 (\"candle\"), so installing {id} would not change what a take uses. Nothing was \
                 downloaded or written.\n\
                 To use {id}: {MOVE_TO_CANDLE}; or, to keep the command, put {file} into the \
                 models directory."
            ));
        }
        "command" => io.say(&format!(
            "[stt] engine is \"command\" and its command does not use {{model}}: it brings its \
             own model, so [stt] model is not used and nothing was changed. {MOVE_TO_CANDLE}."
        )),
        "http" => io.say(&format!(
            "[stt] engine is \"http\": the server's model is named by [stt] http_model, \
             [stt] model is not used, and nothing was changed. {MOVE_TO_CANDLE}."
        )),
        other => io.say(&format!(
            "[stt] engine is {other:?}, which is not one of {}; nothing was changed. \
             {MOVE_TO_CANDLE}.",
            crate::stt::ENGINES.join(", ")
        )),
    }
}

/// The `candle` branch: download what is missing, then write `[stt] model` when it
/// is not the configured one.
fn install_and_write(world: &mut dyn World, io: &mut Io, entry: &'static Entry, configured: &str) {
    let id = entry.identifier;
    let state = world.model_state(entry);
    let here = configured == id;
    if state == Glance::Whole && here {
        io.say(&format!("{id} is installed and already configured."));
        return;
    }
    if state == Glance::Whole {
        io.say(&format!("{id} is already installed."));
    } else {
        io.say(&format!(
            "Installing {id} ({})",
            human(catalogue::weights(entry).bytes)
        ));
        if let Err(why) = world.install(entry) {
            io.say(&why);
            io.fail();
            return;
        }
        io.say(&format!("{id} is installed."));
    }
    if here {
        return;
    }
    save_and_tell(
        world,
        io,
        "stt",
        "model",
        crate::config_edit::quote(id),
        &format!("{id:?}"),
    );
}

/// `herdr-voice model`, and `--choose`. Returns the process's exit code.
pub fn run(choosing: bool) -> u8 {
    let mut world = Real::from_env();
    if !choosing {
        let snapshot = world.snapshot();
        let models = world.models_dir();
        print!(
            "{}",
            list(
                &catalogue::MODELS,
                &snapshot.loaded.config.stt.model,
                &|entry| world.model_state(entry),
                models.as_deref(),
            )
        );
        return 0;
    }
    let code = {
        let stdin = std::io::stdin();
        let mut input = stdin.lock();
        let mut out = std::io::stdout();
        let mut io = Io::new(&mut input, &mut out);
        choose_speech_model(&mut world, &mut io, &catalogue::MODELS);
        u8::from(io.failed)
    };
    crate::popup::pause();
    code
}
````

- [ ] **Step 4: Run to see it pass**

Run: `CARGO_BUILD_JOBS=6 perl -e 'alarm shift; exec @ARGV' 1500 cargo test --bin herdr-voice chooser::`
Expected: 17 tests pass. (`src/settings.rs` does not exist yet and `chooser::run` does not use it; it uses `popup::Real`.)

- [ ] **Step 5: Commit**

```bash
git add src/chooser.rs
git commit -m "Make the speech-model choice one flow over World, and stop it breaking a working setup

With the candle engine it installs and writes [stt] model; with command or http it
changes nothing and says what to change (#46). The pane model and the settings popup
share it.

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 5: The settings popup, `src/settings.rs`

**Files:**
- Create: `src/settings.rs`
- Modify: `src/main.rs` (add `mod settings;` before `mod setup;`)

**Interfaces:**
- Consumes: Tasks 3 and 4 (`popup::*`, `chooser::choose_speech_model`), `stt::ENGINES`, `bias::source::VALUES`, `config_edit::quote`, `stt::catalogue::MODELS`.
- Produces (exact): `pub struct KeyInfo { name, default }`, `pub struct SectionInfo { name, keys }`, `pub const SECRETS`, `pub fn sections() -> Vec<SectionInfo>`, `pub fn is_secret`, `pub fn shown`, `pub fn render_sections`, `pub fn render_keys`, `pub fn render_devices`, `pub fn run_menu(&mut dyn World, &mut Io) -> u8`, `pub fn run() -> u8`, `pub fn wants_open(&[String]) -> bool`, `pub fn open() -> u8`.

- [ ] **Step 1: Write the failing tests**

Create `src/settings.rs` containing only this test module, and add `mod settings;` to `src/main.rs` before `mod setup;`:

````rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::config_edit::WriteError;
    use crate::popup::tests_support::FakeWorld;
    use crate::popup::Reached;
    use crate::rewrite_models::ListFailure;
    use std::io::Cursor;

    /// Runs the menu with `typed` as the person's input.
    fn drive(world: &mut FakeWorld, typed: &str) -> (u8, String) {
        let mut input = Cursor::new(typed.as_bytes().to_vec());
        let mut out: Vec<u8> = Vec::new();
        let code = {
            let mut io = Io::new(&mut input, &mut out);
            run_menu(world, &mut io)
        };
        (code, String::from_utf8(out).unwrap())
    }

    /// The number the menu gives a section.
    fn s(name: &str) -> usize {
        sections()
            .iter()
            .position(|x| x.name == name)
            .expect("a section")
            + 1
    }

    /// The number the keys menu gives a key.
    fn k(section: &str, key: &str) -> usize {
        sections()
            .iter()
            .find(|x| x.name == section)
            .expect("a section")
            .keys
            .iter()
            .position(|x| x.name == key)
            .expect("a key")
            + 1
    }

    fn applied(applied: &[&str], restart: &[&str]) -> Reached {
        Reached::Applied(crate::reload::Applied {
            applied: applied.iter().map(|s| s.to_string()).collect(),
            restart: restart.iter().map(|s| s.to_string()).collect(),
        })
    }

    // --- the list of keys

    #[test]
    fn the_sections_are_the_serialised_ones_in_the_fixed_order() {
        let names: Vec<&str> = sections().iter().map(|x| x.name).collect();
        assert_eq!(
            names,
            ["audio", "stt", "rewrite", "ui", "delivery", "context", "ptt", "record"]
        );
        // The order names exactly the sections the type has: a section added to the
        // configuration fails here until it is placed.
        let value = Value::try_from(Config::default()).unwrap();
        let mut serialised: Vec<&str> = value
            .as_table()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        serialised.sort_unstable();
        let mut ordered = ORDER.to_vec();
        ordered.sort_unstable();
        assert_eq!(serialised, ordered);
    }

    #[test]
    fn every_serialised_key_is_listed_with_its_type_and_default() {
        let value = Value::try_from(Config::default()).unwrap();
        for section in sections() {
            let table = value[section.name].as_table().unwrap();
            let listed: Vec<&str> = section.keys.iter().map(|k| k.name.as_str()).collect();
            let expected: Vec<&str> = table.keys().map(String::as_str).collect();
            assert_eq!(listed, expected, "[{}]", section.name);
        }
        let total: usize = sections().iter().map(|x| x.keys.len()).sum();
        assert!(total >= 25, "{total} keys");
    }

    #[test]
    fn the_secrets_are_every_key_called_token() {
        let value = Value::try_from(Config::default()).unwrap();
        let mut named: Vec<(&str, &str)> = Vec::new();
        for section in sections() {
            for key in &section.keys {
                if key.name == "token" {
                    named.push((section.name, "token"));
                }
            }
        }
        let mut declared = SECRETS.to_vec();
        declared.sort_unstable();
        named.sort_unstable();
        assert_eq!(declared, named, "{value:?}");
    }

    #[test]
    fn a_key_the_file_sets_shows_the_files_value_and_one_it_does_not_shows_its_default_marked() {
        let table: toml::Table = toml::from_str("[ui]\ntoasts = false\nblink_ms = 600\n").unwrap();
        let ui = sections().into_iter().find(|x| x.name == "ui").unwrap();
        let rendered = render_keys(&ui, Some(&table));
        let line = |key: &str| {
            rendered
                .lines()
                .find(|l| l.contains(key))
                .unwrap()
                .to_string()
        };
        assert!(line("toasts").contains("= false") && !line("toasts").contains("(default)"));
        // Set to the same value as the default is still set (reading C3).
        assert!(line("blink_ms").contains("= 600") && !line("blink_ms").contains("(default)"));
        assert!(
            line("sidebar_token").contains("= true") && line("sidebar_token").contains("(default)")
        );
    }

    #[test]
    fn secrets_are_only_ever_set_or_not_set() {
        let table: toml::Table =
            toml::from_str("[stt]\ntoken = \"s3cret\"\n[rewrite]\ntoken = \"\"\n").unwrap();
        let find = |section: &str| sections().into_iter().find(|x| x.name == section).unwrap();
        let stt = render_keys(&find("stt"), Some(&table));
        let rewrite = render_keys(&find("rewrite"), Some(&table));
        assert!(
            stt.lines()
                .any(|l| l.contains("token") && l.ends_with("= set")),
            "{stt}"
        );
        assert!(!stt.contains("s3cret"));
        assert!(
            rewrite
                .lines()
                .any(|l| l.contains("token") && l.ends_with("= not set")),
            "{rewrite}"
        );
    }

    #[test]
    fn a_long_value_is_cut_to_one_line() {
        let long = "x".repeat(200);
        let table: toml::Table =
            toml::from_str(&format!("[stt]\ncommand = [\"{long}\"]\n")).unwrap();
        let stt = sections().into_iter().find(|x| x.name == "stt").unwrap();
        let rendered = render_keys(&stt, Some(&table));
        let line = rendered.lines().find(|l| l.contains(". command ")).unwrap();
        assert!(line.ends_with("..."), "{line}");
        assert!(line.chars().count() < 100, "{line}");
    }

    #[test]
    fn the_sections_menu_counts_the_keys_the_file_sets() {
        let table: toml::Table = toml::from_str("[ui]\ntoasts = false\n").unwrap();
        let rendered = render_sections(&sections(), Some(&table));
        let ui = rendered.lines().find(|l| l.contains("ui")).unwrap();
        assert!(ui.contains("1 of 4 keys set"), "{ui}");
        let audio = rendered.lines().find(|l| l.contains("audio")).unwrap();
        assert!(audio.contains("0 of 2 keys set"), "{audio}");
    }

    // --- the menus

    #[test]
    fn the_popup_lists_every_section_and_leaves_on_an_empty_line() {
        let mut world = FakeWorld::new("menu-leave", "");
        let (code, said) = drive(&mut world, "\n");
        assert_eq!(code, 0, "{said}");
        assert!(said.contains("herdr-voice: settings"), "{said}");
        for section in ORDER {
            assert!(said.contains(&format!(". {section}")), "{section}: {said}");
        }
        assert!(world.saved.is_empty());
    }

    #[test]
    fn every_key_of_every_section_can_be_reached_and_is_listed() {
        for (n, section) in sections().iter().enumerate() {
            let mut world = FakeWorld::new(&format!("menu-keys-{n}"), "");
            let (code, said) = drive(&mut world, &format!("{}\n\n\n", n + 1));
            assert_eq!(code, 0, "{said}");
            for key in &section.keys {
                assert!(
                    said.lines()
                        .any(|l| l.contains(&format!(". {} ", key.name))),
                    "[{}] {} is missing: {said}",
                    section.name,
                    key.name
                );
            }
        }
    }

    #[test]
    fn esc_leaves_one_level_at_a_time() {
        let mut world = FakeWorld::new("menu-esc", "");
        let (code, said) = drive(&mut world, "1\n\u{1b}\n\u{1b}\n");
        assert_eq!(code, 0, "{said}");
        assert!(said.matches("[audio]").count() >= 1, "{said}");
    }

    #[test]
    fn a_number_outside_the_list_is_said_and_asked_again_at_each_level() {
        let mut world = FakeWorld::new("menu-invalid", "");
        let (code, said) = drive(&mut world, "99\n1\n99\n\n\n");
        assert_eq!(code, 0, "{said}");
        assert!(said.contains("between 1 and 8"), "{said}");
        assert!(said.contains("between 1 and 2"), "{said}");
    }

    #[test]
    fn the_end_of_the_input_ends_the_popup_with_a_failure_and_one_message() {
        let mut world = FakeWorld::new("menu-eof", "");
        let (code, said) = drive(&mut world, "1\n");
        assert_eq!(code, 1, "{said}");
        assert_eq!(
            said.matches("nothing was read from the terminal").count(),
            1,
            "{said}"
        );
    }

    #[test]
    fn a_configuration_that_does_not_parse_is_said_and_the_defaults_are_shown() {
        let mut world = FakeWorld::new("menu-invalid-file", "[ui\ntoasts = ");
        let (_, said) = drive(&mut world, "\n");
        assert!(said.contains("does not parse"), "{said}");
        assert!(said.contains("0 of 4 keys set"), "{said}");
    }

    // --- the scalar editor

    #[test]
    fn a_boolean_is_changed_in_place_and_a_restart_is_named() {
        let mut world = FakeWorld::new("scalar-bool", "# mine\n[ui]\ntoasts = true\n");
        world.reached = applied(&[], &["ui"]);
        let typed = format!("{}\n{}\nfalse\n\n\n", s("ui"), k("ui", "toasts"));
        let (code, said) = drive(&mut world, &typed);
        assert_eq!(code, 0, "{said}");
        assert_eq!(world.file(), "# mine\n[ui]\ntoasts = false\n");
        assert_eq!(world.saved, vec!["[ui] toasts = false".to_string()]);
        assert_eq!(world.told, 1);
        assert!(said.contains("needs a restart of herdr"), "{said}");
        // The list afterwards shows what was written.
        assert!(said.matches("toasts").count() >= 2, "{said}");
    }

    #[test]
    fn a_number_is_changed_in_place_as_a_number() {
        let mut world = FakeWorld::new("scalar-int", "");
        let typed = format!("{}\n{}\n250\n\n\n", s("ui"), k("ui", "blink_ms"));
        let (code, said) = drive(&mut world, &typed);
        assert_eq!(code, 0, "{said}");
        assert_eq!(world.file(), "[ui]\nblink_ms = 250\n");
        let typed = format!("{}\n{}\n-60\n\n\n", s("audio"), k("audio", "silence_db"));
        let (_, said) = drive(&mut world, &typed);
        assert!(
            world.file().contains("silence_db = -60.0"),
            "{said}\n{}",
            world.file()
        );
    }

    #[test]
    fn a_text_is_changed_in_place_as_a_quoted_string() {
        let mut world = FakeWorld::new("scalar-text", "[stt]\nlanguage = \"auto\"\n");
        let typed = format!("{}\n{}\nru\n\n\n", s("stt"), k("stt", "language"));
        let (code, said) = drive(&mut world, &typed);
        assert_eq!(code, 0, "{said}");
        assert!(
            world.file().contains("language = \"ru\""),
            "{}",
            world.file()
        );
    }

    #[test]
    fn an_answer_that_is_not_the_type_is_said_and_nothing_is_written() {
        let mut world = FakeWorld::new("scalar-bad", "[ui]\ntoasts = true\n");
        let before = world.file();
        for (key, value) in [("toasts", "yes"), ("blink_ms", "fast")] {
            let typed = format!("{}\n{}\n{value}\n\n\n", s("ui"), k("ui", key));
            let (code, said) = drive(&mut world, &typed);
            assert_eq!(code, 0, "{said}");
            assert!(said.contains("nothing was changed"), "{key}: {said}");
        }
        assert!(world.saved.is_empty());
        assert_eq!(world.file(), before);
    }

    #[test]
    fn a_number_the_key_cannot_hold_is_refused_by_the_writer_and_the_file_is_untouched() {
        let mut world = FakeWorld::new("scalar-negative", "[ui]\nblink_ms = 250\n");
        let before = world.file();
        let typed = format!("{}\n{}\n-5\n\n\n", s("ui"), k("ui", "blink_ms"));
        let (code, said) = drive(&mut world, &typed);
        assert_eq!(code, 1, "{said}");
        assert!(said.contains("nothing was written"), "{said}");
        assert!(said.contains("[ui] blink_ms = -5"), "{said}");
        assert_eq!(world.file(), before);
        assert_eq!(world.told, 0);
    }

    #[test]
    fn a_key_with_a_fixed_set_of_values_refuses_others_and_names_the_set() {
        let mut world = FakeWorld::new("scalar-engine", "");
        let typed = format!("{}\n{}\nwhisperx\n\n\n", s("stt"), k("stt", "engine"));
        let (_, said) = drive(&mut world, &typed);
        assert!(said.contains("not one of candle, http, command"), "{said}");
        assert!(world.saved.is_empty());
        let typed = format!("{}\n{}\ncandle\n\n\n", s("stt"), k("stt", "engine"));
        drive(&mut world, &typed);
        assert!(
            world.file().contains("engine = \"candle\""),
            "{}",
            world.file()
        );
        let typed = format!("{}\n{}\npane\n\n\n", s("context"), k("context", "source"));
        drive(&mut world, &typed);
        assert!(
            world.file().contains("source = \"pane\""),
            "{}",
            world.file()
        );
    }

    #[test]
    fn an_empty_line_or_esc_at_the_value_leaves_the_key_as_it_is() {
        for typed_value in ["\n", "\u{1b}\n"] {
            let mut world = FakeWorld::new("scalar-leave", "[ui]\ntoasts = true\n");
            let typed = format!("{}\n{}\n{typed_value}\n\n", s("ui"), k("ui", "toasts"));
            let (code, said) = drive(&mut world, &typed);
            assert_eq!(code, 0, "{said}");
            assert!(said.contains("Nothing was changed"), "{said}");
            assert!(world.saved.is_empty());
        }
    }

    #[test]
    fn a_change_to_audio_says_the_next_take_uses_it() {
        let mut world = FakeWorld::new("scalar-audio", "");
        world.reached = applied(&["audio"], &[]);
        let typed = format!("{}\n{}\n-40\n\n\n", s("audio"), k("audio", "silence_db"));
        let (_, said) = drive(&mut world, &typed);
        assert!(said.contains("next take"), "{said}");
    }

    #[test]
    fn a_write_that_could_not_happen_prints_the_line_to_add_by_hand_and_fails() {
        let mut world = FakeWorld::new("scalar-io", "");
        world.save_error = Some(WriteError::Io {
            path: "config.toml".to_string(),
            why: "permission denied".to_string(),
        });
        let typed = format!("{}\n{}\nfalse\n\n\n", s("ui"), k("ui", "toasts"));
        let (code, said) = drive(&mut world, &typed);
        assert_eq!(code, 1, "{said}");
        assert!(said.contains("Add this under [ui]"), "{said}");
        assert!(said.contains("toasts = false"), "{said}");
    }

    // --- secrets and lists

    #[test]
    fn a_secret_is_never_printed_and_cannot_be_changed_here() {
        let mut world = FakeWorld::new(
            "secrets",
            "[stt]\ntoken = \"stt-s3cret\"\n[rewrite]\ntoken = \"rw-s3cret\"\nurl = \"http://h/v1/chat/completions\"\n",
        );
        world.list = Ok(vec!["m".to_string()]);
        let before = world.file();
        for (section, key) in SECRETS {
            let typed = format!("{}\n{}\nanything\n\n\n", s(section), k(section, key));
            let (code, said) = drive(&mut world, &typed);
            assert_eq!(code, 0, "{said}");
            assert!(said.contains("is a secret"), "{said}");
            assert!(said.contains("set"), "{said}");
            for secret in ["stt-s3cret", "rw-s3cret"] {
                assert!(!said.contains(secret), "{section}: {said}");
            }
        }
        // The rewrite model flow sends the token to the server and prints none of it.
        let typed = format!("{}\n{}\n1\n\n\n", s("rewrite"), k("rewrite", "model"));
        let (_, said) = drive(&mut world, &typed);
        assert!(!said.contains("rw-s3cret"), "{said}");
        assert_eq!(world.list_calls[0].1, "rw-s3cret");
        assert!(
            world.file().contains("stt-s3cret"),
            "the file is untouched: {before}"
        );
    }

    #[test]
    fn a_list_valued_key_is_said_to_be_changed_in_the_file() {
        let mut world = FakeWorld::new("lists", "");
        for (section, key) in [("stt", "command"), ("rewrite", "command")] {
            let typed = format!("{}\n{}\n\n\n", s(section), k(section, key));
            let (code, said) = drive(&mut world, &typed);
            assert_eq!(code, 0, "{said}");
            assert!(said.contains("is a list of values"), "{said}");
            assert!(said.contains(&format!("[{section}] {key}")), "{said}");
        }
        assert!(world.saved.is_empty());
    }

    // --- the microphone

    fn mic_world(tag: &str, text: &str, names: &[&str]) -> FakeWorld {
        let mut world = FakeWorld::new(tag, text);
        world.names = Ok(names.iter().map(|n| n.to_string()).collect());
        world
    }

    #[test]
    fn the_list_numbers_every_input_and_marks_the_configured_one() {
        let text = render_devices(&["Built-in".to_string(), "Headset".to_string()], "Headset");
        assert!(text.contains("1. Built-in"), "{text}");
        let headset: Vec<&str> = text.lines().filter(|l| l.contains("Headset")).collect();
        assert_eq!(headset.len(), 1, "{text}");
        assert!(
            headset[0].contains("2. Headset") && headset[0].contains("(current)"),
            "{text}"
        );
        assert!(
            !text
                .lines()
                .any(|l| l.contains("Built-in") && l.contains("(current)")),
            "{text}"
        );
    }

    #[test]
    fn a_configured_name_that_matches_nothing_is_said_and_names_the_setting() {
        let text = render_devices(&["Built-in".to_string()], "Old headset");
        assert!(text.contains("[audio] input"), "{text}");
        assert!(text.contains("\"Old headset\""), "{text}");
        assert!(text.contains("matches none"), "{text}");
        assert!(!text.contains("(current)"), "{text}");
    }

    #[test]
    fn an_unset_input_says_the_default_is_used_and_an_empty_name_is_never_current() {
        let text = render_devices(&["Built-in".to_string()], "");
        assert!(
            text.contains("not set") && text.contains("default input"),
            "{text}"
        );
        assert!(!render_devices(&[String::new()], "").contains("(current)"));
    }

    #[test]
    fn two_inputs_with_one_name_are_both_listed_and_the_second_says_the_first_is_used() {
        let names: Vec<String> = ["USB Mic", "Built-in", "USB Mic"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let text = render_devices(&names, "USB Mic");
        assert!(text.contains("3. USB Mic"), "{text}");
        let marked: Vec<&str> = text.lines().filter(|l| l.contains("(current)")).collect();
        assert_eq!(
            marked.len(),
            1,
            "only the first is the one selected: {text}"
        );
        let third = text.lines().find(|l| l.contains("3. USB Mic")).unwrap();
        assert!(
            third.contains("same name as 1") && third.contains("uses the first"),
            "{text}"
        );
    }

    #[test]
    fn choosing_an_input_by_number_saves_its_name_and_says_the_next_take_uses_it() {
        let mut world = mic_world("mic-pick", "", &["Built-in", "Headset"]);
        world.reached = applied(&["audio"], &[]);
        let typed = format!("{}\n{}\n2\n\n\n", s("audio"), k("audio", "input"));
        let (code, said) = drive(&mut world, &typed);
        assert_eq!(code, 0, "{said}");
        assert_eq!(world.saved, vec!["[audio] input = \"Headset\"".to_string()]);
        assert_eq!(world.file(), "[audio]\ninput = \"Headset\"\n");
        assert!(
            said.contains("\"Headset\"") && said.contains("next take"),
            "{said}"
        );
        assert!(said.contains("2. Headset"), "{said}");
    }

    #[test]
    fn esc_at_the_microphone_leaves_the_file_alone_and_says_so() {
        for typed_value in ["\u{1b}\n", "\n", "\u{1b}[B\n"] {
            let mut world = mic_world("mic-esc", "", &["A", "B"]);
            let typed = format!("{}\n{}\n{typed_value}\n\n", s("audio"), k("audio", "input"));
            let (code, said) = drive(&mut world, &typed);
            assert_eq!(code, 0, "{typed_value:?}: {said}");
            assert!(said.contains("Nothing was changed"), "{said}");
            assert!(world.saved.is_empty());
        }
    }

    #[test]
    fn no_input_devices_says_so_and_what_to_do_and_changes_nothing() {
        let mut world = mic_world("mic-none", "", &[]);
        let typed = format!("{}\n{}\n1\n\n\n", s("audio"), k("audio", "input"));
        let (code, said) = drive(&mut world, &typed);
        assert_eq!(code, 1, "{said}");
        assert!(said.contains("No input devices"), "{said}");
        assert!(said.contains("Connect a microphone"), "{said}");
        assert!(world.saved.is_empty());
    }

    #[test]
    fn a_failure_to_list_the_inputs_names_what_to_check() {
        let mut world = FakeWorld::new("mic-list-fails", "");
        world.names = Err("cannot list input devices: denied".to_string());
        let typed = format!("{}\n{}\n\n\n", s("audio"), k("audio", "input"));
        let (code, said) = drive(&mut world, &typed);
        assert_eq!(code, 1, "{said}");
        assert!(said.contains("cannot list input devices: denied"), "{said}");
        assert!(said.contains("may use the microphone"), "{said}");
    }

    #[test]
    fn choosing_the_second_of_two_inputs_with_one_name_says_the_first_is_used() {
        let mut world = mic_world("mic-dup", "", &["USB Mic", "Built-in", "USB Mic"]);
        let typed = format!("{}\n{}\n3\n\n\n", s("audio"), k("audio", "input"));
        let (_, said) = drive(&mut world, &typed);
        assert_eq!(world.saved, vec!["[audio] input = \"USB Mic\"".to_string()]);
        let after = said
            .split("[audio] input is now")
            .nth(1)
            .expect("the confirmation");
        assert!(
            after.contains("input 1") && after.contains("uses the first"),
            "{after}"
        );
    }

    #[test]
    fn a_name_with_a_quote_and_a_backslash_is_written_so_that_it_reads_back_equal() {
        let name = "Mic \"A\" \\ B";
        let mut world = mic_world("mic-quote", "", &[name]);
        let typed = format!("{}\n{}\n1\n\n\n", s("audio"), k("audio", "input"));
        drive(&mut world, &typed);
        let loaded = crate::config::load(Some(&world.dir));
        assert_eq!(loaded.config.audio.input, name);
    }

    // --- the rewrite model

    fn rewrite_world(tag: &str, url: &str) -> FakeWorld {
        let text = if url.is_empty() {
            String::new()
        } else {
            format!("[rewrite]\nurl = \"{url}\"\nmodel = \"m2\"\n")
        };
        FakeWorld::new(tag, &text)
    }

    #[test]
    fn a_list_the_server_serves_is_shown_numbered_and_a_number_writes_that_name_to_rewrite_model_only(
    ) {
        let mut world = rewrite_world("rw-list", "http://127.0.0.1:4000/v1/chat/completions");
        world.list = Ok(vec!["m1".to_string(), "m2".to_string(), "m3".to_string()]);
        let typed = format!("{}\n{}\n3\n\n\n", s("rewrite"), k("rewrite", "model"));
        let (code, said) = drive(&mut world, &typed);
        assert_eq!(code, 0, "{said}");
        assert!(
            said.contains("1. m1") && said.contains("2. m2  (current)"),
            "{said}"
        );
        assert_eq!(world.saved, vec!["[rewrite] model = \"m3\"".to_string()]);
        let loaded = crate::config::load(Some(&world.dir));
        assert_eq!(loaded.config.rewrite.model, "m3");
        assert_eq!(
            loaded.config.stt.model, "large-v3-turbo",
            "no other model key moved"
        );
        assert_eq!(
            world.list_calls,
            vec![(
                "http://127.0.0.1:4000/v1/chat/completions".to_string(),
                String::new()
            )]
        );
    }

    #[test]
    fn a_server_that_cannot_give_a_list_is_said_and_the_name_can_be_typed() {
        for failure in [
            ListFailure::Server(
                "\"http://h/v1/models\" refused the connection. Start the server".to_string(),
            ),
            ListFailure::NoAddress("http://h/v1".to_string()),
            ListFailure::BadBody("not json".to_string()),
            ListFailure::Empty,
        ] {
            let mut world = rewrite_world("rw-fail", "http://h/v1/chat/completions");
            world.list = Err(failure.clone());
            let typed = format!(
                "{}\n{}\nmy-model\n\n\n",
                s("rewrite"),
                k("rewrite", "model")
            );
            let (code, said) = drive(&mut world, &typed);
            assert_eq!(code, 0, "{failure:?}: {said}");
            assert!(said.contains(&failure.to_string()), "{failure:?}: {said}");
            assert!(said.contains("Type the model's name instead"), "{said}");
            assert_eq!(
                world.saved,
                vec!["[rewrite] model = \"my-model\"".to_string()]
            );
        }
    }

    #[test]
    fn with_no_url_nobody_is_asked_and_the_name_can_be_typed() {
        let mut world = rewrite_world("rw-no-url", "");
        let typed = format!("{}\n{}\nlocal\n\n\n", s("rewrite"), k("rewrite", "model"));
        let (_, said) = drive(&mut world, &typed);
        assert!(world.list_calls.is_empty(), "no server, no request");
        assert!(said.contains("[rewrite] url is not set"), "{said}");
        assert_eq!(world.saved, vec!["[rewrite] model = \"local\"".to_string()]);
    }

    #[test]
    fn esc_at_the_rewrite_model_changes_nothing() {
        for typed_value in ["\u{1b}\n", "\n"] {
            let mut world = rewrite_world("rw-esc", "http://h/v1/chat/completions");
            world.list = Ok(vec!["m1".to_string()]);
            let typed = format!(
                "{}\n{}\n{typed_value}\n\n",
                s("rewrite"),
                k("rewrite", "model")
            );
            let (_, said) = drive(&mut world, &typed);
            assert!(said.contains("Nothing was changed"), "{said}");
            assert!(world.saved.is_empty());
            let mut world = rewrite_world("rw-esc-typed", "");
            let typed = format!(
                "{}\n{}\n{typed_value}\n\n",
                s("rewrite"),
                k("rewrite", "model")
            );
            drive(&mut world, &typed);
            assert!(world.saved.is_empty());
        }
    }

    // --- the speech model, reached from the menu

    #[test]
    fn the_speech_model_entry_opens_the_catalogue_flow() {
        let mut world = FakeWorld::new("menu-speech", "[stt]\nengine = \"candle\"\n");
        let typed = format!("{}\n{}\n1\n\n\n", s("stt"), k("stt", "model"));
        let (code, said) = drive(&mut world, &typed);
        assert_eq!(code, 0, "{said}");
        assert_eq!(world.installs, vec!["tiny".to_string()]);
        assert_eq!(world.saved, vec!["[stt] model = \"tiny\"".to_string()]);
    }

    // --- opening

    #[test]
    fn the_flag_chooses_between_being_the_popup_and_opening_it() {
        let args = |list: &[&str]| -> Vec<String> { list.iter().map(|s| s.to_string()).collect() };
        assert!(wants_open(&args(&["settings", "--open"])));
        assert!(!wants_open(&args(&["settings"])));
    }

    #[test]
    fn the_manifest_opens_the_settings_and_has_no_separate_microphone_entry() {
        let manifest: toml::Value =
            toml::from_str(&std::fs::read_to_string("herdr-plugin.toml").unwrap()).unwrap();
        let entries =
            |kind: &str| -> Vec<toml::Value> { manifest[kind].as_array().unwrap().to_vec() };
        for kind in ["actions", "panes"] {
            let ids: Vec<String> = entries(kind)
                .iter()
                .map(|e| e["id"].as_str().unwrap().to_string())
                .collect();
            assert!(
                !ids.iter().any(|id| id == "mic"),
                "no `mic` {kind} entry may remain"
            );
        }
        let action = entries("actions")
            .into_iter()
            .find(|e| e["id"].as_str() == Some("settings"))
            .expect("an action `settings`");
        assert_eq!(action["title"].as_str(), Some("herdr-voice: settings"));
        let argv: Vec<&str> = action["command"]
            .as_array()
            .unwrap()
            .iter()
            .map(|a| a.as_str().unwrap())
            .collect();
        assert_eq!(&argv[1..], ["settings", "--open"]);
        let pane = entries("panes")
            .into_iter()
            .find(|e| e["id"].as_str() == Some("settings"))
            .expect("a pane `settings`");
        assert_eq!(pane["title"].as_str(), Some("herdr-voice: settings"));
        assert_eq!(pane["placement"].as_str(), Some("popup"));
        let argv: Vec<&str> = pane["command"]
            .as_array()
            .unwrap()
            .iter()
            .map(|a| a.as_str().unwrap())
            .collect();
        assert_eq!(&argv[1..], ["settings"]);
        // The pane that opens when the key is pressed is the one the action names.
        assert_eq!(
            crate::popup::open_command("herdr", "herdr-voice", "settings")[7],
            "settings"
        );
    }
}
````

- [ ] **Step 2: Run to see it fail**

Run: `CARGO_BUILD_JOBS=6 perl -e 'alarm shift; exec @ARGV' 1500 cargo test --bin herdr-voice settings::`
Expected: compile errors, `cannot find function sections` and the other items.

- [ ] **Step 3: Implement**

Put this at the top of `src/settings.rs`, above the test module:

````rust
//! The settings popup: every section and key of the configuration, the microphone,
//! the speech model and the rewrite model, from one place.
//!
//! The list of keys is read from the configuration type (`Serialize` is derived on
//! it), so a field added there is listed with no other change. Every flow talks to a
//! `World` (`crate::popup`), so it is tested with no file, daemon, device, download or
//! server. See `tasks/104/DESIGN_104.md`.

use toml::Value;

use crate::config::Config;
use crate::config_edit;
use crate::popup::{
    config_note, not_in_list, parse_answer, pause, save_and_tell, Answer, Io, Real, World,
    LEAVE_HINT, OPEN_BOUND,
};
use crate::stt::catalogue;

/// The sections in the order a person meets them. A test fails when the serialised
/// defaults hold a section that is not here, and when this names one they lack.
const ORDER: [&str; 8] = [
    "audio", "stt", "rewrite", "ui", "delivery", "context", "ptt", "record",
];

/// The keys that hold a secret. They are shown as set or not set and never changed
/// here. A test checks that this holds every key named `token`.
pub const SECRETS: [(&str, &str); 2] = [("stt", "token"), ("rewrite", "token")];

/// `[rewrite] engine` is matched by name in `src/rewrite.rs`; its names are kept here
/// once.
const REWRITE_ENGINES: &[&str] = &["off", "agent", "http", "command"];

/// How much of a value is shown on one line.
const SHOWN_CHARS: usize = 60;

pub struct KeyInfo {
    pub name: String,
    pub default: Value,
}

pub struct SectionInfo {
    pub name: &'static str,
    pub keys: Vec<KeyInfo>,
}

/// The sections of the configuration, each with its keys and their defaults, read
/// from the type.
pub fn sections() -> Vec<SectionInfo> {
    let Ok(value) = Value::try_from(Config::default()) else {
        return Vec::new();
    };
    let Some(table) = value.as_table() else {
        return Vec::new();
    };
    ORDER
        .iter()
        .filter_map(|name| {
            let keys = table.get(*name)?.as_table()?;
            Some(SectionInfo {
                name,
                keys: keys
                    .iter()
                    .map(|(key, default)| KeyInfo {
                        name: key.clone(),
                        default: default.clone(),
                    })
                    .collect(),
            })
        })
        .collect()
}

pub fn is_secret(section: &str, key: &str) -> bool {
    SECRETS.iter().any(|(s, k)| *s == section && *k == key)
}

/// The values a string key accepts, when the code accepts only some.
fn allowed(section: &str, key: &str) -> Option<Vec<&'static str>> {
    match (section, key) {
        ("stt", "engine") => Some(crate::stt::ENGINES.to_vec()),
        ("rewrite", "engine") => Some(REWRITE_ENGINES.to_vec()),
        ("context", "source") => Some(crate::bias::source::VALUES.to_vec()),
        _ => None,
    }
}

/// A key's value as the list shows it.
pub struct Shown {
    pub text: String,
    pub is_default: bool,
}

fn value_text(value: &Value) -> String {
    let text = value.to_string().replace('\n', " ");
    if text.chars().count() > SHOWN_CHARS {
        let cut: String = text.chars().take(SHOWN_CHARS - 3).collect();
        format!("{cut}...")
    } else {
        text
    }
}

fn set_in<'a>(file: Option<&'a toml::Table>, section: &str, key: &str) -> Option<&'a Value> {
    file?.get(section)?.as_table()?.get(key)
}

/// What the list shows for a key: the file's value when the file has the key,
/// otherwise the default, marked. A secret is only ever `set` or `not set`.
pub fn shown(section: &str, key: &KeyInfo, file: Option<&toml::Table>) -> Shown {
    let in_file = set_in(file, section, &key.name);
    if is_secret(section, &key.name) {
        let set = matches!(in_file, Some(Value::String(text)) if !text.is_empty());
        return Shown {
            text: if set { "set" } else { "not set" }.to_string(),
            is_default: false,
        };
    }
    match in_file {
        Some(value) => Shown {
            text: value_text(value),
            is_default: false,
        },
        None => Shown {
            text: value_text(&key.default),
            is_default: true,
        },
    }
}

/// The sections, numbered, with how many of their keys the file sets.
pub fn render_sections(sections: &[SectionInfo], file: Option<&toml::Table>) -> String {
    let mut out = String::new();
    for (i, section) in sections.iter().enumerate() {
        let set = section
            .keys
            .iter()
            .filter(|key| set_in(file, section.name, &key.name).is_some())
            .count();
        out.push_str(&format!(
            "  {}. {:<9} {set} of {} keys set\n",
            i + 1,
            section.name,
            section.keys.len()
        ));
    }
    out
}

/// The keys of one section, numbered, with their values.
pub fn render_keys(section: &SectionInfo, file: Option<&toml::Table>) -> String {
    let width = section.keys.iter().map(|k| k.name.len()).max().unwrap_or(0);
    let mut out = format!("[{}]\n", section.name);
    for (i, key) in section.keys.iter().enumerate() {
        let shown = shown(section.name, key, file);
        let mark = if shown.is_default { "  (default)" } else { "" };
        out.push_str(&format!(
            "  {}. {:<width$} = {}{mark}\n",
            i + 1,
            key.name,
            shown.text
        ));
    }
    out
}

/// The inputs, numbered, with the one the configuration names marked, and the lines
/// about the setting.
pub fn render_devices(names: &[String], configured: &str) -> String {
    let mut out = String::from("Microphones this machine offers:\n\n");
    for (i, name) in names.iter().enumerate() {
        // The first input of a name is the one a take selects.
        let first = names.iter().position(|n| n == name).unwrap_or(i);
        let current = if !configured.is_empty() && name == configured && first == i {
            "  (current)"
        } else {
            ""
        };
        let same = if first != i {
            format!(
                "  (same name as {}; the plugin selects by name and uses the first)",
                first + 1
            )
        } else {
            String::new()
        };
        out.push_str(&format!("  {}. {name}{current}{same}\n", i + 1));
    }
    out.push('\n');
    if names.is_empty() {
        out.push_str("No input devices were found. Connect a microphone and open this again.\n");
    } else if configured.is_empty() {
        out.push_str("[audio] input is not set, so the system default input is used.\n");
    } else if !names.iter().any(|n| n == configured) {
        out.push_str(&format!(
            "[audio] input is {configured:?}, which matches none of these inputs; a take is \
             refused until you choose one of them.\n"
        ));
    }
    out
}

/// The settings, from the sections level.
pub fn run_menu(world: &mut dyn World, io: &mut Io) -> u8 {
    let sections = sections();
    io.say("herdr-voice: settings");
    loop {
        let snapshot = world.snapshot();
        let file = snapshot.table();
        io.say("");
        if let Some(note) = config_note(&snapshot.loaded.source) {
            io.say(&note);
        }
        io.say(render_sections(&sections, file.as_ref()).trim_end());
        let Some(line) = io.ask(&format!(
            "Type a section number, then Enter. {LEAVE_HINT}: "
        )) else {
            break;
        };
        match parse_answer(&line, sections.len()) {
            Answer::Leave => break,
            Answer::Invalid(text) => io.say(&not_in_list(&text, sections.len())),
            Answer::Pick(at) => keys_menu(world, io, &sections[at]),
        }
    }
    u8::from(io.failed)
}

fn keys_menu(world: &mut dyn World, io: &mut Io, section: &SectionInfo) {
    loop {
        let snapshot = world.snapshot();
        let file = snapshot.table();
        io.say("");
        io.say(render_keys(section, file.as_ref()).trim_end());
        let Some(line) = io.ask(&format!("Type a key number, then Enter. {LEAVE_HINT}: ")) else {
            return;
        };
        match parse_answer(&line, section.keys.len()) {
            Answer::Leave => return,
            Answer::Invalid(text) => io.say(&not_in_list(&text, section.keys.len())),
            Answer::Pick(at) => edit_key(world, io, section.name, &section.keys[at]),
        }
    }
}

fn edit_key(world: &mut dyn World, io: &mut Io, section: &str, key: &KeyInfo) {
    io.say("");
    match (section, key.name.as_str()) {
        ("audio", "input") => microphone(world, io),
        ("stt", "model") => crate::chooser::choose_speech_model(world, io, &catalogue::MODELS),
        ("rewrite", "model") => rewrite_model(world, io),
        _ if is_secret(section, &key.name) => io.say(&format!(
            "[{section}] {} is a secret: it is not shown or changed here. Change it in the \
             configuration file.",
            key.name
        )),
        _ => match &key.default {
            Value::Boolean(_) | Value::Integer(_) | Value::Float(_) | Value::String(_) => {
                scalar(world, io, section, key)
            }
            _ => io.say(&format!(
                "[{section}] {} is a list of values. It is changed in the configuration file, \
                 not here.",
                key.name
            )),
        },
    }
}

/// A boolean, a number or a text, typed.
fn scalar(world: &mut dyn World, io: &mut Io, section: &str, key: &KeyInfo) {
    let snapshot = world.snapshot();
    let file = snapshot.table();
    let current = shown(section, key, file.as_ref());
    let choices = allowed(section, &key.name);
    let hint = match (&key.default, &choices) {
        (Value::Boolean(_), _) => "true or false".to_string(),
        (Value::Integer(_), _) => "a whole number".to_string(),
        (Value::Float(_), _) => "a number".to_string(),
        (_, Some(values)) => values.join(", "),
        _ => "text".to_string(),
    };
    io.say(&format!(
        "[{section}] {} is {}{}.",
        key.name,
        current.text,
        if current.is_default { " (default)" } else { "" }
    ));
    let Some(line) = io.ask(&format!("New value ({hint}), then Enter. {LEAVE_HINT}: ")) else {
        return;
    };
    let typed = line.trim();
    if typed.is_empty() || typed.starts_with('\u{1b}') {
        io.say("Nothing was changed.");
        return;
    }
    let value = match &key.default {
        Value::Boolean(_) => match typed {
            "true" | "false" => Some(typed.to_string()),
            _ => None,
        },
        Value::Integer(_) => typed.parse::<i64>().ok().map(|n| n.to_string()),
        Value::Float(_) => typed.parse::<f64>().ok().map(|n| format!("{n:?}")),
        _ => Some(config_edit::quote(typed)),
    };
    let Some(value) = value else {
        io.say(&format!(
            "{typed:?} is not {hint}; nothing was changed. Open the key again and type it as \
             {hint}."
        ));
        return;
    };
    if let Some(values) = &choices {
        if !values.contains(&typed) {
            io.say(&format!(
                "{typed:?} is not one of {}; nothing was changed.",
                values.join(", ")
            ));
            return;
        }
    }
    let shown = match &key.default {
        Value::String(_) => format!("{typed:?}"),
        _ => value.clone(),
    };
    save_and_tell(world, io, section, &key.name, value, &shown);
}

/// `[audio] input`, by name.
fn microphone(world: &mut dyn World, io: &mut Io) {
    let snapshot = world.snapshot();
    let configured = snapshot.loaded.config.audio.input.clone();
    let names = match world.input_names() {
        Ok(names) => names,
        Err(why) => {
            io.say(&format!(
                "{why}. Check that an input is connected and that this program may use the \
                 microphone"
            ));
            io.fail();
            return;
        }
    };
    io.say(&render_devices(&names, &configured));
    if names.is_empty() {
        io.fail();
        return;
    }
    let Some(line) = io.ask(&format!(
        "Type the number of the input, then Enter. {LEAVE_HINT}: "
    )) else {
        return;
    };
    let at = match parse_answer(&line, names.len()) {
        Answer::Leave => {
            io.say("Nothing was changed.");
            return;
        }
        Answer::Invalid(text) => {
            io.say(&not_in_list(&text, names.len()));
            return;
        }
        Answer::Pick(at) => at,
    };
    let name = &names[at];
    if save_and_tell(
        world,
        io,
        "audio",
        "input",
        config_edit::quote(name),
        &format!("{name:?}"),
    ) {
        // A take selects by name and uses the first input that has it.
        if let Some(first) = names.iter().position(|n| n == name) {
            if first != at {
                io.say(&format!(
                    "Two inputs are called {name:?}: the plugin uses the first of them, input {}.",
                    first + 1
                ));
            }
        }
    }
}

/// `[rewrite] model`: from the list the server serves, or typed when there is none.
fn rewrite_model(world: &mut dyn World, io: &mut Io) {
    let snapshot = world.snapshot();
    let rewrite = snapshot.loaded.config.rewrite.clone();
    let list = if rewrite.url.is_empty() {
        None
    } else {
        Some(world.rewrite_models(&rewrite.url, &rewrite.token))
    };
    match list {
        Some(Ok(names)) => {
            io.say("Models the server serves:\n");
            for (i, name) in names.iter().enumerate() {
                let mark = if *name == rewrite.model {
                    "  (current)"
                } else {
                    ""
                };
                io.say(&format!("  {}. {name}{mark}", i + 1));
            }
            io.say("");
            let Some(line) = io.ask(&format!(
                "Type the number of the model, then Enter. {LEAVE_HINT}: "
            )) else {
                return;
            };
            match parse_answer(&line, names.len()) {
                Answer::Leave => io.say("Nothing was changed."),
                Answer::Invalid(text) => io.say(&not_in_list(&text, names.len())),
                Answer::Pick(at) => {
                    let name = &names[at];
                    save_and_tell(
                        world,
                        io,
                        "rewrite",
                        "model",
                        config_edit::quote(name),
                        &format!("{name:?}"),
                    );
                }
            }
        }
        other => {
            match other {
                Some(Err(why)) => io.say(&why.to_string()),
                _ => io.say(
                    "[rewrite] url is not set, so there is no server to ask for a list of models.",
                ),
            }
            let Some(line) = io.ask(&format!(
                "Type the model's name instead, then Enter. {LEAVE_HINT}: "
            )) else {
                return;
            };
            let typed = line.trim();
            if typed.is_empty() || typed.starts_with('\u{1b}') {
                io.say("Nothing was changed.");
                return;
            }
            save_and_tell(
                world,
                io,
                "rewrite",
                "model",
                config_edit::quote(typed),
                &format!("{typed:?}"),
            );
        }
    }
}

/// `herdr-voice settings`: the popup. Returns the process's exit code.
pub fn run() -> u8 {
    let mut world = Real::from_env();
    let code = {
        let stdin = std::io::stdin();
        let mut input = stdin.lock();
        let mut out = std::io::stdout();
        let mut io = Io::new(&mut input, &mut out);
        run_menu(&mut world, &mut io)
    };
    // The lock on standard input is released before `pause` reads from it again: the
    // lock is not re-entrant, and a second one taken on the same thread waits for the
    // first for ever, which in a popup is a pane that never closes.
    pause();
    code
}

/// Whether `herdr-voice settings` was asked to open the popup (the action) and not to
/// be it.
pub fn wants_open(args: &[String]) -> bool {
    args.iter().any(|a| a == "--open")
}

/// `herdr-voice settings --open`: what the manifest's `settings` action runs.
pub fn open() -> u8 {
    let herdr = crate::delivery::herdr_binary();
    let plugin = std::env::var("HERDR_PLUGIN_ID")
        .unwrap_or_else(|_| crate::transport::PLUGIN_ID.to_string());
    match crate::popup::open_with(&herdr, &plugin, "settings", OPEN_BOUND) {
        Ok(()) => 0,
        Err(why) => {
            eprintln!("{why}");
            1
        }
    }
}
````

- [ ] **Step 4: Run to see it pass**

Run: `CARGO_BUILD_JOBS=6 perl -e 'alarm shift; exec @ARGV' 1500 cargo test --bin herdr-voice settings::`
Expected: 40 of the 41 tests pass. The one that fails is `the_manifest_opens_the_settings_and_has_no_separate_microphone_entry`: it reads `herdr-plugin.toml`, which still names `mic` until Task 7. That failure is expected here, and Task 7 step 4 runs it again; any other failure is a transcription difference.

- [ ] **Step 5: Commit**

```bash
git add src/settings.rs src/main.rs
git commit -m "Add the settings popup: every key, the microphone, both models

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---
### Task 6: `settings` replaces `mic` in the binary

**Files:**
- Modify: `src/main.rs`
- Delete: `src/mic.rs`

**Interfaces:**
- Consumes: Task 5's `settings::{run, open, wants_open}`.
- Produces: the command `herdr-voice settings` (the popup) and `herdr-voice settings --open` (what the manifest's action runs); `herdr-voice mic` is an unknown command.

- [ ] **Step 1: Write the failing tests**

In the `tests` module of `src/main.rs`: in `every_manifest_command_is_accepted` and in `the_commands_this_issue_implements_are_not_in_the_unimplemented_arm` replace the name `"mic"` by `"settings"` in the lists; and append this test before the module's closing brace:

````rust
    #[test]
    fn the_separate_microphone_command_is_gone() {
        // The microphone is a section of the settings popup.
        assert_eq!(parse(&args(&["mic"])), Command::Unknown("mic".to_string()));
        assert!(!IMPLEMENTED.contains(&"mic"));
        assert!(!USAGE.contains("herdr-voice mic"));
        assert!(USAGE.contains("herdr-voice settings"));
    }
````

- [ ] **Step 2: Run to see them fail**

Run: `CARGO_BUILD_JOBS=6 perl -e 'alarm shift; exec @ARGV' 1500 cargo test --bin herdr-voice tests::`
Expected: the three tests above fail (`settings` is not a command, `mic` still is).

- [ ] **Step 3: Implement**

Apply these edits to `src/main.rs` (shown as a unified diff; the `mod popup;`, `mod rewrite_models;` and `mod settings;` lines from earlier tasks are already in place), and delete `src/mic.rs` with `git rm src/mic.rs`:

````diff
@@ -25,7 +25,6 @@
 mod gate;
 mod http_failure;
 mod indicator;
-mod mic;
 mod outward;
 mod popup;
 mod proto;
@@ -71,8 +70,8 @@
     Status,
     /// Choose the speech model.
     Model,
-    /// Choose the microphone.
-    Mic,
+    /// Change the configuration; `--open` opens the popup in herdr.
+    Settings,
     /// Print the version and exit.
     Version,
     /// Print usage.
@@ -93,7 +92,7 @@
             Command::Setup => "setup",
             Command::Status => "status",
             Command::Model => "model",
-            Command::Mic => "mic",
+            Command::Settings => "settings",
             Command::Version => "version",
             Command::Help => "help",
             Command::Unknown(_) => "unknown",
@@ -112,7 +111,7 @@
         Some("setup") => Command::Setup,
         Some("status") => Command::Status,
         Some("model") => Command::Model,
-        Some("mic") => Command::Mic,
+        Some("settings") => Command::Settings,
         Some("--version") | Some("-V") | Some("version") => Command::Version,
         Some(other) => Command::Unknown(other.to_string()),
     }
@@ -132,7 +131,7 @@
 /// nowhere else would trip `dead_code`, and CI runs clippy with `-D warnings`.
 #[cfg(test)]
 const IMPLEMENTED: &[&str] = &[
-    "daemon", "doctor", "cancel", "dictate", "model", "ptt", "setup", "mic",
+    "daemon", "doctor", "cancel", "dictate", "model", "ptt", "setup", "settings",
 ];
 
 const USAGE: &str = "\
@@ -145,7 +144,7 @@
   herdr-voice dictate    start a recording, or finish the one running
   herdr-voice ptt        one keypress of hold-to-talk; bind it to a key
   herdr-voice model      list the speech models, or --choose to install one
-  herdr-voice mic        list the microphones, or --choose to switch the input
+  herdr-voice settings   change the configuration; --open opens the popup in herdr
   herdr-voice setup      print the keybindings to add, and offer to add them
   herdr-voice --version  print the version
 ";
@@ -191,11 +190,13 @@
             ExitCode::from(chooser::run(choosing))
         }
         Command::Setup => ExitCode::from(setup::main()),
-        Command::Mic => match mic::mode(&args) {
-            mic::Mode::Open => ExitCode::from(mic::open()),
-            mic::Mode::Choose => ExitCode::from(mic::run(true)),
-            mic::Mode::List => ExitCode::from(mic::run(false)),
-        },
+        Command::Settings => {
+            if settings::wants_open(&args) {
+                ExitCode::from(settings::open())
+            } else {
+                ExitCode::from(settings::run())
+            }
+        }
         other @ Command::Status => {
             eprintln!("{}: not implemented yet", other.name());
             ExitCode::from(NOT_IMPLEMENTED)
@@ -233,7 +234,7 @@
         // The manifest names these; rejecting one would produce a plugin that
         // installs and then does nothing when the action is invoked.
         for name in [
-            "daemon", "dictate", "ptt", "cancel", "setup", "status", "model", "mic", "doctor",
+            "daemon", "dictate", "ptt", "cancel", "setup", "status", "model", "settings", "doctor",
         ] {
             assert!(
                 !matches!(parse(&args(&[name])), Command::Unknown(_)),
@@ -246,7 +247,7 @@
     fn the_commands_this_issue_implements_are_not_in_the_unimplemented_arm() {
         // A guard against a later change quietly folding one back into the 69 arm.
         for name in [
-            "daemon", "doctor", "cancel", "dictate", "model", "ptt", "setup", "mic",
+            "daemon", "doctor", "cancel", "dictate", "model", "ptt", "setup", "settings",
         ] {
             assert!(
                 IMPLEMENTED.contains(&name),
@@ -273,4 +274,13 @@
             Command::Unknown("transcribe".to_string())
         );
     }
+
+    #[test]
+    fn the_separate_microphone_command_is_gone() {
+        // The microphone is a section of the settings popup.
+        assert_eq!(parse(&args(&["mic"])), Command::Unknown("mic".to_string()));
+        assert!(!IMPLEMENTED.contains(&"mic"));
+        assert!(!USAGE.contains("herdr-voice mic"));
+        assert!(USAGE.contains("herdr-voice settings"));
+    }
 }
````

- [ ] **Step 4: Run to see them pass**

Run: `CARGO_BUILD_JOBS=6 perl -e 'alarm shift; exec @ARGV' 1500 cargo test --bin herdr-voice tests::`
Expected: pass. Then the whole suite once: `CARGO_BUILD_JOBS=6 perl -e 'alarm shift; exec @ARGV' 2400 cargo test`. Expected: every test passes except `settings::tests::the_manifest_opens_the_settings_and_has_no_separate_microphone_entry`, which still fails until Task 7 changes the manifest (the `setup` tests do not fail yet, because `BINDINGS` and the manifest still name `mic` and that is what they check).

- [ ] **Step 5: Commit**

```bash
git add -A src
git commit -m "Replace the mic command with settings

herdr-voice settings is the popup and settings --open opens it from herdr; the
separate microphone command is removed with its module.

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 7: The manifest, the key, the decisions and the README

**Files:**
- Modify: `herdr-plugin.toml`, `src/setup.rs`, `docs/decisions.md`, `README.md`, `docs/evidence.md`

**Interfaces:**
- Consumes: Task 6's command names.
- Produces: an action `settings` ("herdr-voice: settings", command `settings --open`) and a pane `settings` (popup, 70% wide, 24 high, command `settings`) in place of the action and pane `mic`; the fourth binding in `setup` is `settings` on `prefix+shift+s`.

- [ ] **Step 1: Write the failing tests**

In the `tests` module of `src/setup.rs`:

1. In `the_bindings_are_the_ones_the_owner_chose`, replace the line `("mic", "prefix+shift+i"),` with `("settings", "prefix+shift+s"),`.
2. In `with_every_key_taken_it_says_nothing_was_added_rather_than_nothing_to_add`, in the string that writes the fourth holder, replace `key = \"prefix+shift+i\"` with `key = \"prefix+shift+s\"`.
3. Rename `the_snippet_for_the_microphone_carries_its_action_and_description` to `the_snippet_for_the_settings_carries_its_action_and_description` and make its body:

````rust
        let rendered = render(&[BINDINGS.last().unwrap()]);
        assert!(rendered.contains("herdr-voice.settings"), "{rendered}");
        assert!(
            rendered.contains("description = \"herdr-voice: settings\""),
            "{rendered}"
        );
````

- [ ] **Step 2: Run to see them fail**

Run: `CARGO_BUILD_JOBS=6 perl -e 'alarm shift; exec @ARGV' 1500 cargo test --bin herdr-voice setup::`
Expected: exactly these three fail (`the_bindings_are_the_ones_the_owner_chose`, `with_every_key_taken_it_says_nothing_was_added_rather_than_nothing_to_add`, `the_snippet_for_the_settings_carries_its_action_and_description`).

- [ ] **Step 3: Implement**

1. `src/setup.rs`: replace the fourth `Binding` (the one with `action: "mic"`, with the comment line above it that says the key is a proposal) with:

````rust
    Binding {
        action: "settings",
        key: "prefix+shift+s",
        description: "herdr-voice: settings",
    },
````

2. `herdr-plugin.toml`: replace the action `mic` with

````toml
[[actions]]
id = "settings"
title = "herdr-voice: settings"
description = "Open the popup that shows every setting and changes it, including the microphone and both models"
contexts = ["global"]
command = ["target/release/herdr-voice", "settings", "--open"]
````

and replace the pane `mic` with

````toml
[[panes]]
id = "settings"
title = "herdr-voice: settings"
placement = "popup"
width = "70%"
height = 24
command = ["target/release/herdr-voice", "settings"]
````

3. `docs/decisions.md`: append these three rows to the table, each on one line:

````
| The settings popup lists its sections and keys from the configuration type, which derives `Serialize`, in a fixed order of sections that a test checks against it | A table of keys kept by hand drifts from the type the day a key is added, and the popup would then silently not offer it; read from the type, an added field is listed with no other change, and only a new section fails the test until it is placed | 2026-10-09, #104 |
| Choosing a catalogue speech model installs it and writes `[stt] model` only when `[stt] engine` is `candle`; for `command` or `http` it changes nothing and says which engine serves the take and what to change, and the popup never switches the engine itself | The catalogue holds weights for the built-in engine; with the default `command` the model name is a whisper.cpp file name, so a download plus a write breaks a setup that worked (#46), and writing the engine as well would move a `whisper-cli` setup to an engine measured about ten times slower for the same model (`src/config.rs`) | 2026-10-09, #46, #104 |
| A change outside `[audio]` is applied by a restart of herdr, and the popup says so; the daemon does not replace a running engine | The speech and rewrite engines are built once at start and kept; swapping one adds a failure mode for a change made a few times in a year, and the daemon is started by herdr, so herdr's restart is the only restart there is | 2026-10-09, #104 |
````

4. `README.md`: after the paragraph that ends "Running it twice changes nothing the second time." add this paragraph:

````
The settings are one popup. Press the key `setup` offered for it (`prefix+shift+s`),
or run `herdr plugin action invoke herdr-voice.settings`. It lists every section and
key of `config.toml` with its value and marks the ones you have not set; you change a
key by typing its number and the new value. The microphone is chosen there by name
and the next take uses it at once. The speech model is chosen from the catalogue, and
the rewrite model from the list your server serves. A change to anything but the
microphone takes effect when herdr is restarted, and the popup says so. Tokens are
never shown, only whether they are set, and the two `command` lists are changed in
the file.
````

5. `docs/evidence.md`: directly under the heading `## The microphone popup and the reload request, for issue #103` insert this paragraph:

````
The popup described here was built as a command of its own (`herdr-voice mic`) and became the
microphone entry of the settings popup (`herdr-voice settings`); the command, its action and
its pane no longer exist. What was run below is the code that the settings popup still uses:
the writer, the reload request, the recorder's reconfigure and the input list. The menu in
front of it is new and is recorded in the section for issue #104.
````

- [ ] **Step 4: Run to see them pass**

Run: `python3 scripts/check_manifest.py` (expected: `manifest: 13 entries, all commands known`), then `CARGO_BUILD_JOBS=6 perl -e 'alarm shift; exec @ARGV' 1500 cargo test --bin herdr-voice setup::` and `CARGO_BUILD_JOBS=6 perl -e 'alarm shift; exec @ARGV' 1500 cargo test --bin herdr-voice settings::the_manifest_opens_the_settings`.
Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add herdr-plugin.toml src/setup.rs docs/decisions.md README.md docs/evidence.md
git commit -m "Open the settings from an action and a key; remove the microphone entry

The manifest has an action and a pane settings, setup's fourth binding is
prefix+shift+s, and three decisions are recorded.

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 8: The gates, the Windows check, and what is left for S4 and S5

**Files:** none changed unless a gate finds something.

- [ ] **Step 1: The four gates, fresh**

```
export CARGO_BUILD_JOBS=6
cargo fmt
perl -e 'alarm shift; exec @ARGV' 2400 cargo test
perl -e 'alarm shift; exec @ARGV' 1500 cargo clippy --all-targets -- -D warnings
cargo fmt --check
python3 scripts/check_manifest.py
```
Expected: every test passes, no clippy warning, `cargo fmt --check` prints nothing, the manifest line. `cargo fmt` may reformat code from this plan; look at what it changed and commit it as `Format`.

- [ ] **Step 2: The Windows dead-code check**

On a scratch copy, never in the worktree:

```sh
export CARGO_BUILD_JOBS=6
W=$(mktemp -d) && rsync -a --exclude target --exclude .git ./ "$W/" && cd "$W" &&
find src -name '*.rs' -exec sed -i '' \
  -e 's/#\[cfg(all(test, unix))\]/#[cfg(any())]/g' \
  -e 's/#\[cfg(unix)\]/#[cfg(any())]/g' \
  -e 's/#\[cfg(not(unix))\]/#[cfg(not(any()))]/g' \
  -e 's/#\[cfg(windows)\]/#[cfg(not(any()))]/g' {} + &&
CARGO_TARGET_DIR="$TMPDIR/hv-104-target" perl -e 'alarm shift; exec @ARGV' 2400 cargo clippy --all-targets -- -D warnings
```
Expected: no warning. A finding is an item reachable only from a `#[cfg(unix)]` path; move it under the same attribute as its user, rerun the gates, and commit as `Keep unix-only test helpers out of the Windows build`.

- [ ] **Step 3: Leave the stages that follow**

Do not push and do not open the pull request here. S4 (a fresh reviewer over the diff, then the mutation tester, one after the other) and S5 (the real thing: the popup in a terminal against an isolated daemon, the list from the owner's LM Studio read-only, no model loaded or unloaded) follow. The by-hand step for the owner is `herdr plugin action invoke herdr-voice.settings`.

---

## Self-review

**Spec coverage** (design section 4 against tasks): AC-1 to 9 and AC-10 to 14's mechanism are in the tree already and keep their tests; AC-13 and 14 are `change_note` (Task 3) and its use in every flow (Tasks 4 and 5); AC-15 to 20 are the microphone flow (Task 5); AC-21 and 42 are Tasks 6 and 7; AC-22, 39, 40 are Task 7 and S5; AC-23 and 24 are `sections`, `shown` and the menus (Task 5, with Task 1); AC-25 is the scalar editor (Task 5); AC-26 is Review Focus 1; AC-27 is the writer's own tests; AC-43 is `a_list_valued_key_is_said_to_be_changed_in_the_file` (Task 5); AC-44 is `settings::run` calling `pause` once (read in Task 5, by hand in S5); AC-28 to 31 are Task 4; AC-33 to 37 are Task 2 and the rewrite flow (Task 5); AC-41 is Task 7 and Task 8; AC-32 and 38 are by hand in S5.

**Placeholders:** none; every code block is the text that ran in the scratch copy. The edits in Tasks 1, 6 and 7 name the exact lines.

**Types:** `Reached`, `World`, `Io`, `Snapshot`, `ListFailure`, `Real` and `FakeWorld` are defined in Tasks 2 and 3 and used with the same names and signatures in Tasks 4 and 5. `catalogue` is `&'static [Entry]` in `list`, `choose_speech_model` and `install_and_write`, because `catalogue::weights` needs a static reference.

**Review focus:** each of the five lines has its test.
