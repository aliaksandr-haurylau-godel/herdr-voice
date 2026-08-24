# Daemon, client and doctor — implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make herdr's startup entry produce a daemon that stays alive, make every plugin action a short client run that reaches it over a socket, and make `doctor` report what is missing.

**Architecture:** One binary, two roles. `daemon` listens on a name derived from the environment; every other subcommand connects, writes one frame, reads one reply line and exits. The transport dependency is visible in exactly one module. Nothing of the dictation pipeline is built here.

**Tech Stack:** Rust 2021, `interprocess` 2.4.3 (local sockets: Unix domain socket on macOS and Linux, named pipe on Windows), `serde` 1 with `derive`, `serde_json` 1, `toml` 1. No asynchronous runtime.

**Spec:** `tasks/3/DESIGN_3.md`, which is built from `tasks/3/AC_3.md`. Read both; this plan argues from them and does not restate their reasoning.

## Global Constraints

- Rust edition 2021, `rust-version = "1.82"` (`Cargo.toml:6`). `interprocess` needs 1.75, so the floor stands.
- **No asynchronous runtime in this project** until a task appears that cannot be done without one. `interprocess` default features are empty; do not enable `async` or `tokio`.
- Every new dependency is one of the four named above. Adding a fifth is a design change, not an implementation choice.
- Everything inside the repository is in English: code, comments, output strings, commits (`CLAUDE.md`).
- Cite paths relative to the repository root. An absolute home path fails the leak gate (`CLAUDE.md`).
- A command the manifest names must be a command the binary accepts, or `python3 scripts/check_manifest.py` fails. It reads the known set out of `src/main.rs` with a regular expression over `Some("…")`.
- Commands not implemented in this issue keep exiting `NOT_IMPLEMENTED = 69` with `"<name>: not implemented yet"` (`src/main.rs:87`, `src/main.rs:109`). Only `daemon`, `doctor` and `cancel` leave that arm.
- No panic paths. A failure prints a message that names what to do next and exits non-zero.
- Five CI checks must pass: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test --all` on macOS, Linux and Windows, the manifest check, and the leak gate.
- Choose the transport name by `cfg(windows)`, never by `GenericNamespaced::is_supported()`: that predicate is **true on macOS**, where a namespaced name resolves to a file under `/tmp` instead of the plugin's state directory. Verified on macOS 25.6 with `interprocess` 2.4.3.

---

### Task 1: The frame

**Files:**
- Modify: `Cargo.toml` — add the four dependencies
- Create: `src/proto.rs`
- Modify: `src/main.rs` — declare the module

**Interfaces:**
- Consumes: nothing.
- Produces: `proto::Request { command: String, entrypoint: Option<String>, context: Vec<u8> }` with `write_to<W: Write>(&self, w: &mut W) -> Result<(), ProtoError>` and `read_from<R: BufRead>(r: &mut R) -> Result<Request, ProtoError>`; `proto::Reply::{Ok(String), Error(String)}` with the same two methods; `proto::ProtoError`; `proto::PROTOCOL: &str = "voice/1"`.

- [ ] **Step 1: Write the failing test**

Create `src/proto.rs` containing only the tests, so the module compiles as a test target and fails on missing items:

```rust
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
    fn a_header_with_the_wrong_token_count_is_refused() {
        let mut input = BufReader::new(&b"voice/1 cancel 0\n"[..]);
        let error = Request::read_from(&mut input).expect_err("must refuse");
        assert!(matches!(error, ProtoError::BadHeader(_)), "got {error:?}");
    }

    #[test]
    fn a_foreign_protocol_is_refused_by_name() {
        let mut input = BufReader::new(&b"voice/2 cancel - 0\n"[..]);
        let error = Request::read_from(&mut input).expect_err("must refuse");
        assert!(matches!(error, ProtoError::UnknownProtocol(_)), "got {error:?}");
    }

    #[test]
    fn a_body_shorter_than_the_header_promises_is_refused() {
        let mut input = BufReader::new(&b"voice/1 cancel - 12\nshort"[..]);
        let error = Request::read_from(&mut input).expect_err("must refuse");
        assert!(matches!(error, ProtoError::ShortBody { .. }), "got {error:?}");
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
        for reply in [Reply::Ok("listening".into()), Reply::Error("no pane".into())] {
            let mut buffer = Vec::new();
            reply.write_to(&mut buffer).expect("write");
            let read = Reply::read_from(&mut BufReader::new(&buffer[..])).expect("read");
            assert_eq!(read, reply);
        }
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test proto`
Expected: compilation failure naming `Request`, `Reply` and `ProtoError` as not found.

- [ ] **Step 3: Add the dependencies**

```bash
cargo add interprocess@2.4.3 --no-default-features
cargo add serde@1 --features derive
cargo add serde_json@1
cargo add toml@1
```

Then check that nothing asynchronous arrived: `cargo tree | grep -i -E 'tokio|futures'` must print nothing.

- [ ] **Step 4: Write the implementation above the tests in `src/proto.rs`**

```rust
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
    BadHeader(String),
    UnknownProtocol(String),
    BadToken(String),
    ShortBody { expected: usize, got: usize },
    Io(io::Error),
}

impl fmt::Display for ProtoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProtoError::BadHeader(line) => write!(f, "malformed header: {line:?}"),
            ProtoError::UnknownProtocol(found) => {
                write!(f, "unknown protocol {found:?}, this build speaks {PROTOCOL}")
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
        writeln!(w, "{PROTOCOL} {command} {entrypoint} {}", self.context.len())?;
        w.write_all(&self.context)?;
        w.flush()?;
        Ok(())
    }

    pub fn read_from<R: BufRead>(r: &mut R) -> Result<Request, ProtoError> {
        let mut header = String::new();
        r.read_line(&mut header)?;
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
                return Err(ProtoError::ShortBody { expected: length, got: filled });
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
```

Add to `src/main.rs`, above the `use` lines:

```rust
mod proto;
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test proto`
Expected: seven tests pass.

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml Cargo.lock src/proto.rs src/main.rs
git commit -m "Add the frame the client and the daemon exchange"
```

---

### Task 2: The transport

**Files:**
- Create: `src/transport.rs`
- Modify: `src/main.rs` — declare the module

**Interfaces:**
- Consumes: nothing from Task 1.
- Produces: `transport::Address` with `display(&self) -> &str`; `transport::Vars { state_dir, xdg_state_home, home }` with `from_env() -> Vars`; `transport::state_directory(&Vars) -> Option<PathBuf>`; `transport::address(&Vars) -> Result<Address, TransportError>`; `transport::connect(&Address) -> Result<Stream, TransportError>`; `transport::listen(&Address) -> Result<Listener, TransportError>`; `transport::Listener::accept(&self) -> Result<Stream, TransportError>`; `transport::Stream` implementing `Read` and `Write`; `transport::TransportError`.

- [ ] **Step 1: Write the failing test**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Write};

    #[cfg(unix)]
    #[test]
    fn the_state_directory_from_herdr_wins() {
        let vars = Vars {
            state_dir: Some("/tmp/herdr-state".into()),
            xdg_state_home: Some("/tmp/xdg".into()),
            home: Some("/tmp/home".into()),
        };
        assert_eq!(address(&vars).unwrap().display(), "/tmp/herdr-state/voice.sock");
    }

    #[cfg(unix)]
    #[test]
    fn without_it_xdg_then_home_are_used() {
        let vars = Vars {
            state_dir: None,
            xdg_state_home: Some("/tmp/xdg".into()),
            home: Some("/tmp/home".into()),
        };
        assert_eq!(
            address(&vars).unwrap().display(),
            "/tmp/xdg/herdr/plugins/haurylau.voice/voice.sock"
        );

        let vars = Vars { state_dir: None, xdg_state_home: None, home: Some("/tmp/home".into()) };
        assert_eq!(
            address(&vars).unwrap().display(),
            "/tmp/home/.local/state/herdr/plugins/haurylau.voice/voice.sock"
        );
    }

    #[cfg(unix)]
    #[test]
    fn with_nothing_to_go_on_it_says_so() {
        let vars = Vars { state_dir: None, xdg_state_home: None, home: None };
        let error = address(&vars).expect_err("must refuse");
        assert!(error.to_string().contains("HERDR_PLUGIN_STATE_DIR"), "got {error}");
    }

    #[cfg(unix)]
    #[test]
    fn the_state_directory_is_the_socket_path_without_the_file_name() {
        let vars = Vars {
            state_dir: Some("/tmp/herdr-state".into()),
            xdg_state_home: None,
            home: None,
        };
        let directory = state_directory(&vars).expect("a directory");
        assert_eq!(directory, std::path::PathBuf::from("/tmp/herdr-state"));
        assert_eq!(
            address(&vars).unwrap().display(),
            directory.join("voice.sock").to_string_lossy()
        );
    }

    #[test]
    fn a_client_and_a_listener_meet() {
        // A name of this test's own, so a developer's live daemon is untouched.
        let address = probe_address("round-trip");
        let listener = listen(&address).expect("listen");
        let server = std::thread::spawn(move || {
            let mut reader = BufReader::new(listener.accept().expect("accept"));
            let mut line = String::new();
            reader.read_line(&mut line).expect("read");
            reader.get_mut().write_all(b"ok\n").expect("write");
            line
        });

        let mut client = BufReader::new(connect(&address).expect("connect"));
        client.get_mut().write_all(b"hello\n").expect("write");
        let mut reply = String::new();
        client.read_line(&mut reply).expect("read");

        assert_eq!(server.join().unwrap(), "hello\n");
        assert_eq!(reply, "ok\n");
    }

    #[test]
    fn connecting_with_nobody_listening_fails_rather_than_hangs() {
        let address = probe_address("nobody-home");
        let error = connect(&address).expect_err("must fail");
        assert!(!error.to_string().is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn a_stale_socket_file_does_not_block_a_listener() {
        let address = probe_address("stale");
        drop(listen(&address).expect("first listen"));
        // The file survives the listener on Unix; the second listen must reclaim it.
        assert!(std::path::Path::new(address.display()).exists());
        listen(&address).expect("second listen must reclaim the name");
    }
}
```

Every test needs a name of its own, and no test may touch a real state directory. The helper lives in its own module rather than inside `mod tests`, because the daemon's and the client's tests use it too:

```rust
#[cfg(test)]
pub mod tests_support {
    use super::Address;

    /// A socket name unique to one test in one process.
    pub fn probe_address(tag: &str) -> Address {
        let unique = format!("herdr-voice-test-{tag}-{}", std::process::id());
        #[cfg(windows)]
        {
            Address::namespaced(unique)
        }
        #[cfg(unix)]
        {
            let mut path = std::env::temp_dir();
            path.push(format!("{unique}.sock"));
            Address::path(path.to_string_lossy().into_owned())
        }
    }
}
```

The tests in this module call it as `tests_support::probe_address`; add `use super::tests_support;` inside `mod tests`.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test transport`
Expected: compilation failure naming `Address`, `Vars`, `address`, `listen` and `connect`.

- [ ] **Step 3: Write the implementation**

```rust
//! Where the client and the daemon meet, and the only place that knows how.
//!
//! On macOS and Linux that is a Unix domain socket under the plugin's state
//! directory; on Windows, a named pipe in the machine's pipe namespace. Nothing
//! above this module knows which. See `tasks/3/DESIGN_3.md`, section 1.
//!
//! The platform is chosen by `cfg(windows)` and never by
//! `GenericNamespaced::is_supported()`: that predicate is true on macOS too, where
//! a namespaced name resolves to a file under the temporary directory instead of
//! the state directory.

use interprocess::local_socket::{prelude::*, GenericFilePath, GenericNamespaced, ListenerOptions};
use std::fmt;
use std::io::{self, Read, Write};
use std::path::PathBuf;

/// The plugin id, which is also the last component of every derived path.
pub const PLUGIN_ID: &str = "haurylau.voice";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Address {
    value: String,
    namespaced: bool,
}

impl Address {
    pub fn path(value: String) -> Address {
        Address { value, namespaced: false }
    }

    pub fn namespaced(value: String) -> Address {
        Address { value, namespaced: true }
    }

    /// What `doctor` prints and what a message names.
    pub fn display(&self) -> &str {
        &self.value
    }
}

/// The environment the name is derived from, captured so the derivation can be
/// tested without touching the process environment.
#[derive(Debug, Default, Clone)]
pub struct Vars {
    pub state_dir: Option<String>,
    pub xdg_state_home: Option<String>,
    pub home: Option<String>,
}

impl Vars {
    pub fn from_env() -> Vars {
        Vars {
            state_dir: std::env::var("HERDR_PLUGIN_STATE_DIR").ok(),
            xdg_state_home: std::env::var("XDG_STATE_HOME").ok(),
            home: std::env::var("HOME").ok(),
        }
    }
}

#[derive(Debug)]
pub enum TransportError {
    NoStateDirectory,
    Name(io::Error),
    Io { what: &'static str, address: String, source: io::Error },
}

impl fmt::Display for TransportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TransportError::NoStateDirectory => write!(
                f,
                "cannot tell where to put the socket: neither HERDR_PLUGIN_STATE_DIR, \
                 XDG_STATE_HOME nor HOME is set"
            ),
            TransportError::Name(e) => write!(f, "the socket name is not usable: {e}"),
            TransportError::Io { what, address, source } => {
                write!(f, "cannot {what} {address}: {source}")
            }
        }
    }
}

impl std::error::Error for TransportError {}

#[cfg(windows)]
pub fn address(_vars: &Vars) -> Result<Address, TransportError> {
    // The pipe namespace is machine-wide, which is issue #6.
    Ok(Address::namespaced(PLUGIN_ID.to_string()))
}

/// The plugin's state directory, on every platform. The socket lives here on
/// macOS and Linux; the models directory `doctor` reports on lives here on all
/// three, which is why this is separate from `address`.
pub fn state_directory(vars: &Vars) -> Option<PathBuf> {
    if let Some(given) = &vars.state_dir {
        return Some(PathBuf::from(given));
    }
    if let Some(xdg) = &vars.xdg_state_home {
        return Some(PathBuf::from(xdg).join("herdr/plugins").join(PLUGIN_ID));
    }
    vars.home
        .as_ref()
        .map(|home| PathBuf::from(home).join(".local/state/herdr/plugins").join(PLUGIN_ID))
}

#[cfg(unix)]
pub fn address(vars: &Vars) -> Result<Address, TransportError> {
    let directory = state_directory(vars).ok_or(TransportError::NoStateDirectory)?;
    let path = directory.join("voice.sock");
    Ok(Address::path(path.to_string_lossy().into_owned()))
}

fn name(address: &Address) -> Result<interprocess::local_socket::Name<'_>, TransportError> {
    if address.namespaced {
        address.value.as_str().to_ns_name::<GenericNamespaced>()
    } else {
        address.value.as_str().to_fs_name::<GenericFilePath>()
    }
    .map_err(TransportError::Name)
}

pub struct Stream(interprocess::local_socket::Stream);

impl Read for Stream {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        self.0.read(buffer)
    }
}

impl Write for Stream {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        self.0.write(buffer)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}

pub struct Listener(interprocess::local_socket::Listener);

impl Listener {
    pub fn accept(&self) -> Result<Stream, TransportError> {
        self.0.accept().map(Stream).map_err(|source| TransportError::Io {
            what: "accept a connection on",
            address: String::new(),
            source,
        })
    }
}

pub fn connect(address: &Address) -> Result<Stream, TransportError> {
    interprocess::local_socket::Stream::connect(name(address)?)
        .map(Stream)
        .map_err(|source| TransportError::Io {
            what: "connect to",
            address: address.value.clone(),
            source,
        })
}

pub fn listen(address: &Address) -> Result<Listener, TransportError> {
    if !address.namespaced {
        if let Some(parent) = std::path::Path::new(&address.value).parent() {
            std::fs::create_dir_all(parent).map_err(|source| TransportError::Io {
                what: "create the directory for",
                address: address.value.clone(),
                source,
            })?;
        }
    }
    ListenerOptions::new()
        .name(name(address)?)
        // A socket file outlives the daemon that died holding it. Reclaiming it is
        // safe here because the caller connects first: see daemon::start.
        .try_overwrite(true)
        .create_sync()
        .map(Listener)
        .map_err(|source| TransportError::Io {
            what: "listen on",
            address: address.value.clone(),
            source,
        })
}
```

Add to `src/main.rs`:

```rust
mod transport;
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test transport`
Expected: on macOS and Linux, six tests pass; on Windows, the three `cfg(unix)` tests are absent and the rest pass.

- [ ] **Step 5: Commit**

```bash
git add src/transport.rs src/main.rs
git commit -m "Add the transport, and the only module that knows the platform"
```

---

### Task 3: The invocation context

**Files:**
- Create: `src/context.rs`
- Modify: `src/main.rs` — declare the module

**Interfaces:**
- Consumes: nothing.
- Produces: `context::Invocation` with the fields `focused_pane_id`, `focused_pane_cwd`, `focused_pane_agent`, `tab_id`, `tab_label`, each `Option<String>`, and `target_pane(&self) -> Option<&str>`; `context::parse(&[u8]) -> Result<Invocation, ContextError>`; `context::ContextError`.

- [ ] **Step 1: Write the failing test**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_fields_the_target_needs_are_read() {
        let body = br#"{
            "workspace_id": "w1",
            "tab_id": "t1",
            "tab_label": "notes",
            "focused_pane_id": "w1:p2",
            "focused_pane_cwd": "/repo",
            "focused_pane_agent": "claude",
            "focused_pane_status": "idle",
            "worktree": { "repo_root": "/repo", "checkout_path": "/repo" },
            "selected_text": "",
            "invocation_source": "keybinding"
        }"#;
        let invocation = parse(body).expect("parse");
        assert_eq!(invocation.focused_pane_id.as_deref(), Some("w1:p2"));
        assert_eq!(invocation.focused_pane_cwd.as_deref(), Some("/repo"));
        assert_eq!(invocation.focused_pane_agent.as_deref(), Some("claude"));
        assert_eq!(invocation.tab_id.as_deref(), Some("t1"));
        assert_eq!(invocation.tab_label.as_deref(), Some("notes"));
        assert_eq!(invocation.target_pane(), Some("w1:p2"));
    }

    #[test]
    fn a_field_that_is_absent_is_absent_rather_than_an_error() {
        let invocation = parse(br#"{"tab_id":"t1"}"#).expect("parse");
        assert_eq!(invocation.tab_id.as_deref(), Some("t1"));
        assert_eq!(invocation.focused_pane_id, None);
        assert_eq!(invocation.target_pane(), None);
    }

    #[test]
    fn a_field_herdr_adds_later_does_not_break_an_action() {
        let invocation = parse(br#"{"tab_id":"t1","something_new":{"a":[1,2]}}"#).expect("parse");
        assert_eq!(invocation.tab_id.as_deref(), Some("t1"));
    }

    #[test]
    fn escapes_and_unicode_survive() {
        let invocation = parse(br#"{"tab_label":"a \"quoted\" тест"}"#)
            .expect("parse");
        assert_eq!(invocation.tab_label.as_deref(), Some("a \"quoted\" тест"));
    }

    #[test]
    fn an_empty_body_says_the_variable_was_not_set() {
        let error = parse(b"").expect_err("must refuse");
        assert!(matches!(error, ContextError::Absent), "got {error:?}");
        assert!(error.to_string().contains("HERDR_PLUGIN_CONTEXT_JSON"));
    }

    #[test]
    fn a_body_that_is_not_an_object_is_refused_with_the_reason() {
        let error = parse(b"[1,2,3]").expect_err("must refuse");
        assert!(matches!(error, ContextError::Malformed(_)), "got {error:?}");
        let error = parse(b"{not json").expect_err("must refuse");
        assert!(matches!(error, ContextError::Malformed(_)), "got {error:?}");
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test context`
Expected: compilation failure naming `Invocation`, `parse` and `ContextError`.

- [ ] **Step 3: Write the implementation**

```rust
//! The invocation context herdr passes to a plugin command.
//!
//! The target pane is the one herdr names here; nothing is derived. Unknown fields
//! are ignored, so a herdr release that adds one does not break an action. See
//! `tasks/3/DESIGN_3.md`, section 2, and `docs/design.md`, section 3.

use serde::Deserialize;
use std::fmt;

#[derive(Debug, Default, Clone, PartialEq, Eq, Deserialize)]
pub struct Invocation {
    pub focused_pane_id: Option<String>,
    pub focused_pane_cwd: Option<String>,
    pub focused_pane_agent: Option<String>,
    pub tab_id: Option<String>,
    pub tab_label: Option<String>,
}

impl Invocation {
    /// The pane a command delivers to, or nothing when herdr named none.
    pub fn target_pane(&self) -> Option<&str> {
        self.focused_pane_id.as_deref()
    }
}

#[derive(Debug)]
pub enum ContextError {
    Absent,
    Malformed(String),
}

impl fmt::Display for ContextError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ContextError::Absent => write!(
                f,
                "HERDR_PLUGIN_CONTEXT_JSON was not set, so there is no pane to work with; \
                 invoke this through a herdr keybinding or action"
            ),
            ContextError::Malformed(why) => write!(f, "the invocation context is unreadable: {why}"),
        }
    }
}

impl std::error::Error for ContextError {}

pub fn parse(body: &[u8]) -> Result<Invocation, ContextError> {
    if body.is_empty() {
        return Err(ContextError::Absent);
    }
    serde_json::from_slice(body).map_err(|e| ContextError::Malformed(e.to_string()))
}
```

Add to `src/main.rs`:

```rust
mod context;
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test context`
Expected: six tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/context.rs src/main.rs
git commit -m "Read the invocation context herdr passes, and ignore what it adds"
```

---

### Task 4: The configuration

**Files:**
- Create: `src/config.rs`
- Modify: `src/main.rs` — declare the module

**Interfaces:**
- Consumes: nothing.
- Produces: `config::Config { stt: Stt, rewrite: Rewrite }`, `config::Stt { model: String }`, `config::Rewrite { engine: String, agent: String }`; `config::Vars { config_dir, xdg_config_home, home }` with `from_env()`; `config::directory(&Vars) -> Option<PathBuf>`; `config::load(Option<&Path>) -> Loaded`; `config::Loaded { config: Config, source: Source }`; `config::Source::{File(PathBuf), Defaults(Option<PathBuf>), Invalid { path: PathBuf, why: String }}`.

- [ ] **Step 1: Write the failing test**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!("herdr-voice-config-{tag}-{}", std::process::id()));
        std::fs::create_dir_all(&path).expect("create scratch");
        path
    }

    #[test]
    fn every_key_has_a_default() {
        let defaults = Config::default();
        assert_eq!(defaults.stt.model, "large-v3-turbo");
        assert_eq!(defaults.rewrite.engine, "agent");
        assert_eq!(defaults.rewrite.agent, "auto");
    }

    #[test]
    fn no_file_is_a_valid_state() {
        let directory = scratch("absent");
        let loaded = load(Some(&directory));
        assert_eq!(loaded.config, Config::default());
        match loaded.source {
            Source::Defaults(Some(path)) => assert_eq!(path, directory.join("config.toml")),
            other => panic!("expected defaults with a path, got {other:?}"),
        }
    }

    #[test]
    fn a_partial_file_keeps_the_other_defaults() {
        let directory = scratch("partial");
        std::fs::write(directory.join("config.toml"), "[stt]\nmodel = \"small\"\n").unwrap();
        let loaded = load(Some(&directory));
        assert_eq!(loaded.config.stt.model, "small");
        assert_eq!(loaded.config.rewrite.engine, "agent");
        assert!(matches!(loaded.source, Source::File(_)), "got {:?}", loaded.source);
    }

    #[test]
    fn a_key_of_a_later_stage_is_ignored_rather_than_fatal() {
        let directory = scratch("future");
        std::fs::write(
            directory.join("config.toml"),
            "[ptt]\nrelease_ms = 250\n\n[audio]\ninput = \"\"\n\n[stt]\nmodel = \"small\"\n",
        )
        .unwrap();
        let loaded = load(Some(&directory));
        assert_eq!(loaded.config.stt.model, "small");
        assert!(matches!(loaded.source, Source::File(_)), "got {:?}", loaded.source);
    }

    #[test]
    fn a_broken_file_is_reported_and_the_defaults_are_used() {
        let directory = scratch("broken");
        std::fs::write(directory.join("config.toml"), "[stt\nmodel =").unwrap();
        let loaded = load(Some(&directory));
        assert_eq!(loaded.config, Config::default());
        match loaded.source {
            Source::Invalid { why, .. } => assert!(!why.is_empty()),
            other => panic!("expected invalid, got {other:?}"),
        }
    }

    #[test]
    fn the_directory_herdr_gives_wins_then_xdg_then_home() {
        let given = Vars {
            config_dir: Some("/tmp/from-herdr".into()),
            xdg_config_home: Some("/tmp/xdg".into()),
            home: Some("/tmp/home".into()),
        };
        assert_eq!(directory(&given).unwrap(), PathBuf::from("/tmp/from-herdr"));

        let xdg = Vars { config_dir: None, ..given.clone() };
        assert_eq!(
            directory(&xdg).unwrap(),
            PathBuf::from("/tmp/xdg/herdr/plugins/config/haurylau.voice")
        );

        let home = Vars { config_dir: None, xdg_config_home: None, home: Some("/tmp/home".into()) };
        assert_eq!(
            directory(&home).unwrap(),
            PathBuf::from("/tmp/home/.config/herdr/plugins/config/haurylau.voice")
        );

        let nothing = Vars { config_dir: None, xdg_config_home: None, home: None };
        assert_eq!(directory(&nothing), None);
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test config`
Expected: compilation failure naming `Config`, `load`, `directory`, `Vars`, `Source`.

- [ ] **Step 3: Write the implementation**

```rust
//! The configuration file, and a default for every key.
//!
//! Only the keys this issue needs are read: `doctor` reports on the model and the
//! rewrite engine. Keys belonging to later stages are ignored rather than refused,
//! so a file written for a later version does not stop the daemon. The location is
//! the directory herdr itself computes — `herdr plugin config-dir haurylau.voice`
//! prints it even for a plugin that is not installed. See `tasks/3/DESIGN_3.md`,
//! section 4, and `docs/design.md`, section 7.

use serde::Deserialize;
use std::path::{Path, PathBuf};

use crate::transport::PLUGIN_ID;

pub const FILE_NAME: &str = "config.toml";

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct Config {
    pub stt: Stt,
    pub rewrite: Rewrite,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct Stt {
    pub model: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct Rewrite {
    pub engine: String,
    pub agent: String,
}

impl Default for Stt {
    fn default() -> Self {
        Stt { model: "large-v3-turbo".to_string() }
    }
}

impl Default for Rewrite {
    fn default() -> Self {
        Rewrite { engine: "agent".to_string(), agent: "auto".to_string() }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Vars {
    pub config_dir: Option<String>,
    pub xdg_config_home: Option<String>,
    pub home: Option<String>,
}

impl Vars {
    pub fn from_env() -> Vars {
        Vars {
            config_dir: std::env::var("HERDR_PLUGIN_CONFIG_DIR").ok(),
            xdg_config_home: std::env::var("XDG_CONFIG_HOME").ok(),
            home: std::env::var("HOME").ok(),
        }
    }
}

pub fn directory(vars: &Vars) -> Option<PathBuf> {
    if let Some(given) = &vars.config_dir {
        return Some(PathBuf::from(given));
    }
    if let Some(xdg) = &vars.xdg_config_home {
        return Some(PathBuf::from(xdg).join("herdr/plugins/config").join(PLUGIN_ID));
    }
    vars.home
        .as_ref()
        .map(|home| PathBuf::from(home).join(".config/herdr/plugins/config").join(PLUGIN_ID))
}

#[derive(Debug)]
pub enum Source {
    File(PathBuf),
    Defaults(Option<PathBuf>),
    Invalid { path: PathBuf, why: String },
}

#[derive(Debug)]
pub struct Loaded {
    pub config: Config,
    pub source: Source,
}

pub fn load(directory: Option<&Path>) -> Loaded {
    let Some(directory) = directory else {
        return Loaded { config: Config::default(), source: Source::Defaults(None) };
    };
    let path = directory.join(FILE_NAME);
    match std::fs::read_to_string(&path) {
        Err(_) => Loaded { config: Config::default(), source: Source::Defaults(Some(path)) },
        Ok(text) => match toml::from_str::<Config>(&text) {
            Ok(config) => Loaded { config, source: Source::File(path) },
            Err(e) => Loaded {
                config: Config::default(),
                source: Source::Invalid { path, why: e.message().to_string() },
            },
        },
    }
}
```

Add to `src/main.rs`:

```rust
mod config;
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test config`
Expected: six tests pass. If `toml::de::Error::message()` is not available in the version resolved, use `e.to_string()` instead — the test only requires a non-empty reason.

- [ ] **Step 5: Commit**

```bash
git add src/config.rs src/main.rs
git commit -m "Read the configuration, with a default for every key"
```

---

### Task 5: The daemon

**Files:**
- Create: `src/daemon.rs`
- Modify: `src/main.rs` — declare the module

**Interfaces:**
- Consumes: `proto::{Request, Reply}`, `transport::{Address, Listener, Stream, address, connect, listen}`, `context::parse`.
- Produces: `daemon::needs_target_pane(&str) -> bool`; `daemon::answer(&Request) -> (Reply, Control)`; `daemon::Control::{Continue, Stop}`; `daemon::request_line(&Request) -> String`; `daemon::start() -> Result<Outcome, TransportError>`; `daemon::Outcome::{AlreadyRunning(String), Served}`.

- [ ] **Step 1: Write the failing test**

```rust
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
    fn the_recorded_line_names_the_command_and_the_entrypoint() {
        let line = request_line(&request("cancel", b"{}"));
        assert!(line.contains("cancel"), "got {line:?}");
        assert!(line.contains("entrypoint=cancel"), "got {line:?}");

        let anonymous = Request { command: "cancel".into(), entrypoint: None, context: vec![] };
        assert!(request_line(&anonymous).contains("entrypoint=-"));
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test daemon`
Expected: compilation failure naming `needs_target_pane`, `answer`, `Control`, `request_line`.

- [ ] **Step 3: Write the implementation**

```rust
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
        "cancel" => (Reply::Ok("nothing to cancel".to_string()), Control::Continue),
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
    let request = Request::read_from(&mut reader)?;
    eprintln!("{}", request_line(&request));
    let (reply, control) = answer(&request);
    reply.write_to(reader.get_mut())?;
    if control == Control::Stop {
        stop.store(true, Ordering::SeqCst);
        // Unblock the accept that is waiting, so the loop can see the flag.
        let _ = transport::connect(address);
    }
    Ok(())
}
```

Add to `src/main.rs`:

```rust
mod daemon;
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test daemon`
Expected: eight tests pass.

- [ ] **Step 5: Add the end-to-end test for the two start paths**

Append to the `tests` module in `src/daemon.rs`:

```rust
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
        let mut client = std::io::BufReader::new(
            crate::transport::connect(&address).expect("connect"),
        );
        Request { command: "stop".into(), entrypoint: None, context: vec![] }
            .write_to(client.get_mut())
            .expect("write");
        let reply = Reply::read_from(&mut client).expect("reply");
        assert!(matches!(reply, Reply::Ok(_)), "got {reply:?}");

        served.join().expect("the loop must end");
    }
```

The probe helper this uses is `transport::tests_support::probe_address`, added in Task 2. Nothing else is needed here.

- [ ] **Step 6: Run the tests to verify they pass**

Run: `cargo test daemon`
Expected: nine tests pass, and the run finishes — a hang here means the wake-up connection after the stop flag is missing.

- [ ] **Step 7: Commit**

```bash
git add src/daemon.rs src/transport.rs src/main.rs
git commit -m "Add the daemon: connect before listening, one thread per connection"
```

---

### Task 6: The client

**Files:**
- Create: `src/client.rs`
- Modify: `src/main.rs` — declare the module

**Interfaces:**
- Consumes: `proto::{Request, Reply}`, `transport::{address, connect, Vars}`.
- Produces: `client::Outcome { code: u8, message: Option<String> }`; `client::outcome(Result<Reply, ClientError>) -> Outcome`; `client::ClientError::{NoDaemon(String), Timeout, Transport(String), Protocol(String)}`; `client::send(&str) -> Outcome`; `client::REPLY_TIMEOUT: Duration`.

- [ ] **Step 1: Write the failing test**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_ok_reply_is_success_and_says_nothing() {
        let outcome = outcome(Ok(Reply::Ok("nothing to cancel".into())));
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
            Reply::Ok(String::new()).write_to(reader.get_mut()).expect("write");
            request
        });

        let body = br#"{"focused_pane_id":"w1:p2","tab_label":"a \"quoted\" label"}"#.to_vec();
        send_to(&address, "cancel", None, body.clone());
        assert_eq!(server.join().unwrap().context, body);
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test client`
Expected: compilation failure naming `outcome`, `ClientError`, `REPLY_TIMEOUT`, `send_to`.

- [ ] **Step 3: Write the implementation**

```rust
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
        Ok(Reply::Ok(_)) => Outcome { code: 0, message: None },
        Ok(Reply::Error(text)) => Outcome { code: 1, message: Some(text) },
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
        Err(ClientError::Transport(why)) => Outcome { code: 1, message: Some(why) },
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

    let request = Request { command: command.to_string(), entrypoint, context };
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
```

Add to `src/main.rs`:

```rust
mod client;
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test client`
Expected: seven tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/client.rs src/main.rs
git commit -m "Add the client: one frame out, one reply line back, bounded wait"
```

---

### Task 7: doctor

**Files:**
- Create: `src/doctor.rs`
- Modify: `src/main.rs` — declare the module and add `MIN_HERDR_VERSION`
- Modify: `scripts/check_manifest.py` — compare the constant with the manifest

**Interfaces:**
- Consumes: `config::{load, directory, Vars, Source}`, `transport::{address, connect, Vars as TransportVars}`.
- Produces: `doctor::State::{Ok, Default, Missing}`; `doctor::Finding { name: &'static str, state: State, detail: String }`; `doctor::parse_version(&str) -> Option<(u32, u32, u32)>`; `doctor::at_least(&str, &str) -> Option<bool>`; `doctor::render(&[Finding]) -> String`; `doctor::exit_code(&[Finding]) -> u8`; `doctor::run() -> u8`.

- [ ] **Step 1: Write the failing test**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_version_is_read_out_of_what_herdr_prints() {
        assert_eq!(parse_version("herdr 0.8.2"), Some((0, 8, 2)));
        assert_eq!(parse_version("0.8.2"), Some((0, 8, 2)));
        assert_eq!(parse_version("herdr 1.0.0-rc1"), Some((1, 0, 0)));
        assert_eq!(parse_version("nothing here"), None);
    }

    #[test]
    fn versions_compare_by_three_numbers() {
        assert_eq!(at_least("herdr 0.8.2", "0.8.0"), Some(true));
        assert_eq!(at_least("herdr 0.8.0", "0.8.0"), Some(true));
        assert_eq!(at_least("herdr 0.7.9", "0.8.0"), Some(false));
        assert_eq!(at_least("herdr 1.0.0", "0.8.0"), Some(true));
        assert_eq!(at_least("nonsense", "0.8.0"), None);
    }

    #[test]
    fn every_line_that_is_not_ok_names_a_next_action() {
        let findings = vec![
            Finding { name: "herdr", state: State::Ok, detail: "0.8.2".into() },
            Finding {
                name: "model",
                state: State::Missing,
                detail: "put a speech model in /tmp/models".into(),
            },
        ];
        let text = render(&findings);
        assert!(text.contains("herdr"), "got {text}");
        assert!(text.contains("ok"), "got {text}");
        assert!(text.contains("missing"), "got {text}");
        assert!(text.contains("/tmp/models"), "got {text}");
        assert_eq!(text.lines().count(), 2);
    }

    #[test]
    fn the_exit_code_follows_the_worst_line() {
        let ok = vec![Finding { name: "herdr", state: State::Ok, detail: String::new() }];
        assert_eq!(exit_code(&ok), 0);

        let defaults = vec![Finding { name: "config", state: State::Default, detail: String::new() }];
        assert_eq!(exit_code(&defaults), 0);

        let missing = vec![Finding { name: "model", state: State::Missing, detail: String::new() }];
        assert_eq!(exit_code(&missing), 1);
    }

    #[test]
    fn defaults_are_reported_as_defaults_not_as_a_failure() {
        let directory = std::env::temp_dir().join(format!("doctor-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let finding = config_finding(&crate::config::load(Some(&directory)));
        assert_eq!(finding.state, State::Default);
        assert!(finding.detail.contains("defaults"), "got {}", finding.detail);
        assert!(finding.detail.contains("config.toml"), "got {}", finding.detail);
    }

    #[test]
    fn a_model_is_present_when_a_file_carries_its_name() {
        let directory = std::env::temp_dir().join(format!("doctor-model-{}", std::process::id()));
        let models = directory.join("models");
        std::fs::create_dir_all(&models).unwrap();
        assert_eq!(model_finding(&models, "large-v3-turbo").state, State::Missing);
        std::fs::write(models.join("ggml-large-v3-turbo.bin"), b"x").unwrap();
        assert_eq!(model_finding(&models, "large-v3-turbo").state, State::Ok);
    }

    #[test]
    fn the_rewrite_engine_is_looked_for_by_name() {
        assert_eq!(rewrite_finding("agent", "definitely-not-installed").state, State::Missing);
        assert_eq!(rewrite_finding("off", "auto").state, State::Ok);
        assert_eq!(
            rewrite_finding("agent", "auto").name,
            "rewrite",
            "auto resolves against the candidate list"
        );
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test doctor`
Expected: compilation failure naming `parse_version`, `at_least`, `render`, `exit_code`, `Finding`, `State`, `config_finding`, `model_finding`, `rewrite_finding`.

- [ ] **Step 3: Write the implementation**

```rust
//! What is missing, and what to do about it.
//!
//! Five lines in a fixed order. The prototype's worst failure was silence: a parse
//! error produced no output and looked like a hang for a morning, so every line
//! that is not `ok` names the next action. See `tasks/3/DESIGN_3.md`, section 4.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::config::{self, Source};
use crate::transport;
use crate::MIN_HERDR_VERSION;

/// The one agent command-line tool `agent = "auto"` looks for. It is the only one
/// the prototype used (`spike/spike.sh`) and the only one the rewrite measurements
/// in `docs/evidence.md` were made with. A second name goes here when a second
/// tool is measured.
const AGENT_CANDIDATES: &[&str] = &["claude"];

#[derive(Debug, PartialEq, Eq)]
pub enum State {
    Ok,
    Default,
    Missing,
}

impl State {
    fn word(&self) -> &'static str {
        match self {
            State::Ok => "ok",
            State::Default => "default",
            State::Missing => "missing",
        }
    }
}

#[derive(Debug)]
pub struct Finding {
    pub name: &'static str,
    pub state: State,
    pub detail: String,
}

pub fn parse_version(text: &str) -> Option<(u32, u32, u32)> {
    for word in text.split_whitespace() {
        let candidate = word.trim_start_matches('v');
        let numbers: Vec<&str> = candidate.split('.').collect();
        if numbers.len() < 3 {
            continue;
        }
        let major = numbers[0].parse().ok()?;
        let minor = numbers[1].parse().ok()?;
        let patch: u32 = numbers[2]
            .chars()
            .take_while(char::is_ascii_digit)
            .collect::<String>()
            .parse()
            .ok()?;
        return Some((major, minor, patch));
    }
    None
}

pub fn at_least(found: &str, required: &str) -> Option<bool> {
    Some(parse_version(found)? >= parse_version(required)?)
}

pub fn render(findings: &[Finding]) -> String {
    let mut out = String::new();
    for finding in findings {
        out.push_str(&format!(
            "{:<8} {:<8} {}\n",
            finding.name,
            finding.state.word(),
            finding.detail
        ));
    }
    out
}

pub fn exit_code(findings: &[Finding]) -> u8 {
    u8::from(findings.iter().any(|f| f.state == State::Missing))
}

fn on_path(program: &str) -> bool {
    Command::new(program)
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok()
}

fn herdr_finding() -> Finding {
    let binary = std::env::var("HERDR_BIN_PATH").unwrap_or_else(|_| "herdr".to_string());
    match Command::new(&binary).arg("--version").output() {
        Err(_) => Finding {
            name: "herdr",
            state: State::Missing,
            detail: format!(
                "cannot run {binary}; install herdr, or set HERDR_BIN_PATH to it"
            ),
        },
        Ok(output) => {
            let printed = String::from_utf8_lossy(&output.stdout).trim().to_string();
            match at_least(&printed, MIN_HERDR_VERSION) {
                Some(true) => Finding {
                    name: "herdr",
                    state: State::Ok,
                    detail: format!("{printed}, this plugin needs {MIN_HERDR_VERSION} or newer"),
                },
                Some(false) => Finding {
                    name: "herdr",
                    state: State::Missing,
                    detail: format!(
                        "{printed} is older than {MIN_HERDR_VERSION}; upgrade herdr"
                    ),
                },
                None => Finding {
                    name: "herdr",
                    state: State::Missing,
                    detail: format!(
                        "cannot read a version out of {printed:?}; run `{binary} --version`"
                    ),
                },
            }
        }
    }
}

fn daemon_finding() -> Finding {
    match transport::address(&transport::Vars::from_env()) {
        Err(e) => Finding { name: "daemon", state: State::Missing, detail: e.to_string() },
        Ok(address) => {
            if transport::connect(&address).is_ok() {
                Finding {
                    name: "daemon",
                    state: State::Ok,
                    detail: format!("listening at {}", address.display()),
                }
            } else {
                Finding {
                    name: "daemon",
                    state: State::Missing,
                    detail: format!(
                        "nothing is listening at {}; start it with `herdr-voice daemon`, \
                         or restart herdr",
                        address.display()
                    ),
                }
            }
        }
    }
}

pub fn config_finding(loaded: &config::Loaded) -> Finding {
    match &loaded.source {
        Source::File(path) => {
            Finding { name: "config", state: State::Ok, detail: format!("{}", path.display()) }
        }
        Source::Defaults(Some(path)) => Finding {
            name: "config",
            state: State::Default,
            detail: format!("no file at {}, defaults used", path.display()),
        },
        Source::Defaults(None) => Finding {
            name: "config",
            state: State::Default,
            detail: "no configuration directory could be derived, defaults used".to_string(),
        },
        Source::Invalid { path, why } => Finding {
            name: "config",
            state: State::Missing,
            detail: format!("{} will not parse ({why}); fix it or delete it", path.display()),
        },
    }
}

pub fn model_finding(models: &Path, model: &str) -> Finding {
    let found = std::fs::read_dir(models).ok().and_then(|entries| {
        entries.filter_map(Result::ok).find(|entry| {
            entry.file_name().to_string_lossy().contains(model)
        })
    });
    match found {
        Some(entry) => Finding {
            name: "model",
            state: State::Ok,
            detail: format!("{}", entry.path().display()),
        },
        None => Finding {
            name: "model",
            state: State::Missing,
            detail: format!(
                "no file naming {model} in {}; put a speech model there — \
                 the chooser is not built yet",
                models.display()
            ),
        },
    }
}

pub fn rewrite_finding(engine: &str, agent: &str) -> Finding {
    match engine {
        "off" => Finding {
            name: "rewrite",
            state: State::Ok,
            detail: "turned off in the configuration".to_string(),
        },
        "agent" => {
            let candidates: Vec<&str> =
                if agent == "auto" { AGENT_CANDIDATES.to_vec() } else { vec![agent] };
            match candidates.iter().find(|name| on_path(name)) {
                Some(found) => Finding {
                    name: "rewrite",
                    state: State::Ok,
                    detail: format!("found {found:?} in PATH"),
                },
                None => Finding {
                    name: "rewrite",
                    state: State::Missing,
                    detail: format!(
                        "none of {candidates:?} is in PATH; install one, or set \
                         [rewrite] engine = \"off\" to insert transcripts unchanged"
                    ),
                },
            }
        }
        "http" | "command" => Finding {
            name: "rewrite",
            state: State::Missing,
            detail: format!("the {engine:?} engine is not built yet; use \"agent\" or \"off\""),
        },
        other => Finding {
            name: "rewrite",
            state: State::Missing,
            detail: format!(
                "unknown engine {other:?} in [rewrite]; use \"agent\", \"http\", \
                 \"command\" or \"off\""
            ),
        },
    }
}

/// Where a speech model lives. Derived from the state directory rather than from
/// the socket name: on Windows the socket is a pipe name and has no parent
/// directory at all.
fn models_directory() -> Option<PathBuf> {
    transport::state_directory(&transport::Vars::from_env()).map(|state| state.join("models"))
}

pub fn run() -> u8 {
    let loaded = config::load(config::directory(&config::Vars::from_env()).as_deref());
    let findings = vec![
        herdr_finding(),
        daemon_finding(),
        config_finding(&loaded),
        match models_directory() {
            Some(models) => model_finding(&models, &loaded.config.stt.model),
            None => Finding {
                name: "model",
                state: State::Missing,
                detail: "cannot tell where models live: neither HERDR_PLUGIN_STATE_DIR, \
                         XDG_STATE_HOME nor HOME is set"
                    .to_string(),
            },
        },
        rewrite_finding(&loaded.config.rewrite.engine, &loaded.config.rewrite.agent),
    ];
    print!("{}", render(&findings));
    exit_code(&findings)
}
```

Add to `src/main.rs`, next to `NOT_IMPLEMENTED`:

```rust
/// The oldest herdr this plugin works with. `scripts/check_manifest.py` fails if
/// this and `min_herdr_version` in `herdr-plugin.toml` disagree, so the number
/// cannot drift between the two files.
pub const MIN_HERDR_VERSION: &str = "0.8.0";
```

and declare the module:

```rust
mod doctor;
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test doctor`
Expected: seven tests pass.

- [ ] **Step 5: Teach the manifest check about the constant**

In `scripts/check_manifest.py`, after the crate-version comparison, add:

```python
    declared = re.search(r'MIN_HERDR_VERSION: &str = "([^"]+)"', source)
    if not declared:
        fail("src/main.rs does not declare MIN_HERDR_VERSION")
    if manifest.get("min_herdr_version") != declared.group(1):
        fail(
            f"min_herdr_version {manifest.get('min_herdr_version')} does not match "
            f"MIN_HERDR_VERSION {declared.group(1)} in src/main.rs"
        )
```

`source` is read further down in the current file; move the `source = (ROOT / "src" / "main.rs").read_text()` line above this block so both checks use it.

- [ ] **Step 6: Run the manifest check**

Run: `python3 scripts/check_manifest.py`
Expected: no output, exit 0. Then prove it bites: change the constant to `"0.9.0"`, run again, expect a failure naming both numbers, and change it back.

- [ ] **Step 7: Commit**

```bash
git add src/doctor.rs src/main.rs scripts/check_manifest.py
git commit -m "Add doctor, and stop the herdr version from drifting between two files"
```

---

### Task 8: Wire the three commands and verify the whole thing

**Files:**
- Modify: `src/main.rs` — dispatch `daemon`, `doctor` and `cancel`; extend the usage text; keep the rest on 69

**Interfaces:**
- Consumes: `daemon::{start, Outcome}`, `doctor::run`, `client::send`.
- Produces: the finished binary.

- [ ] **Step 1: Write the failing test**

Add to the existing `mod tests` in `src/main.rs`:

```rust
    #[test]
    fn the_commands_this_issue_implements_are_not_in_the_unimplemented_arm() {
        // A guard against a later change quietly folding one back into the 69 arm.
        for name in ["daemon", "doctor", "cancel"] {
            assert!(
                IMPLEMENTED.contains(&name),
                "{name} is implemented and must not report 'not implemented yet'"
            );
        }
        for name in ["dictate", "ptt", "setup", "status", "model", "mic"] {
            assert!(
                !IMPLEMENTED.contains(&name),
                "{name} is not implemented in this issue"
            );
        }
    }

    #[test]
    fn the_usage_text_names_every_implemented_command() {
        for name in IMPLEMENTED {
            assert!(USAGE.contains(name), "usage does not mention {name}");
        }
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --bin herdr-voice`
Expected: compilation failure naming `IMPLEMENTED`.

- [ ] **Step 3: Write the implementation**

In `src/main.rs`, add next to the other constants:

```rust
/// The commands this build actually performs. Everything else reports
/// `not implemented yet` and exits `NOT_IMPLEMENTED`. Test-only: a constant used
/// nowhere else would trip `dead_code`, and CI runs clippy with `-D warnings`.
#[cfg(test)]
const IMPLEMENTED: &[&str] = &["daemon", "doctor", "cancel"];
```

Replace the usage text with:

```rust
const USAGE: &str = "\
herdr-voice — voice dictation for herdr

usage:
  herdr-voice daemon     run the long-lived process
  herdr-voice doctor     report what is missing
  herdr-voice cancel     stop and discard the current recording
  herdr-voice --version  print the version
";
```

Replace the dispatch arms so the three implemented commands leave the 69 arm:

```rust
        Command::Daemon => match daemon::start() {
            Ok(daemon::Outcome::AlreadyRunning(address)) => {
                eprintln!("a daemon is already listening at {address}");
                ExitCode::SUCCESS
            }
            Ok(daemon::Outcome::Served) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("cannot start the daemon: {e}");
                ExitCode::FAILURE
            }
        },
        Command::Doctor => ExitCode::from(doctor::run()),
        Command::Cancel => {
            let outcome = client::send("cancel");
            if let Some(message) = outcome.message {
                eprintln!("{message}");
            }
            ExitCode::from(outcome.code)
        }
        other @ (Command::Dictate
        | Command::Ptt
        | Command::Setup
        | Command::Status
        | Command::Model
        | Command::Mic) => {
            eprintln!("{}: not implemented yet", other.name());
            ExitCode::from(NOT_IMPLEMENTED)
        }
```

- [ ] **Step 4: Run the whole suite and the four local checks**

```bash
cargo test --all
cargo clippy --all-targets -- -D warnings
cargo fmt --check
python3 scripts/check_manifest.py
```

Expected: all four clean. `cargo fmt` first if the last one complains.

- [ ] **Step 5: Verify the criteria that need a real herdr, by hand**

Run each of these and keep the output; it goes into `docs/evidence.md`.

```bash
cargo build --release

# AC-9, AC-10: doctor on a machine with no configuration file.
./target/release/herdr-voice doctor; echo "exit=$?"

# AC-7: a client with no daemon.
./target/release/herdr-voice cancel; echo "exit=$?"

# AC-1, AC-3: the daemon stays alive and says where it listens.
./target/release/herdr-voice daemon &
sleep 1; ./target/release/herdr-voice doctor; echo "exit=$?"

# AC-2: a second start finds the first and exits 0.
./target/release/herdr-voice daemon; echo "exit=$?"

# AC-5, AC-6: an action reaches the daemon through herdr.
herdr plugin link .
herdr plugin action invoke cancel; echo "exit=$?"
herdr plugin log list --plugin haurylau.voice | tail -5

# AC-4: a stale socket file does not block the next start.
kill -9 %1
ls -l "$(./target/release/herdr-voice doctor | awk '/^daemon/ {print $NF}')" || true
./target/release/herdr-voice daemon &
sleep 1; ./target/release/herdr-voice doctor; echo "exit=$?"
kill %1
```

- [ ] **Step 6: Write the hand-verified results into `docs/evidence.md`**

Add a section named "Daemon, client and doctor" holding: the platform (macOS, the herdr version), each command run, its exit code and the line that matters from its output. Anything that did not behave as the criteria require is written down as it happened, not as it should have been.

- [ ] **Step 7: Commit**

```bash
git add src/main.rs docs/evidence.md
git commit -m "Wire the daemon, doctor and cancel, and record what was verified by hand"
```

- [ ] **Step 8: Open the pull request**

```bash
git push -u origin feat/3-daemon-client-doctor
gh pr create --fill
```

Then wait for the five checks — the three-platform matrix, the manifest job and the leak gate — and read the Windows job in particular: it is the only thing that exercises the named pipe.

---

## Notes for the executor

- **The order.** Tasks 1 to 4 are independent of each other after Task 1 adds the dependencies. Task 5 needs 1, 2 and 3. Task 6 needs 1 and 2. Task 7 needs 2 and 4. Task 8 needs everything.
- **`answer` treats `dictate` and `ptt` as pane-needing** even though `src/main.rs` never sends them: the table is the daemon's, and it is what AC-8's two halves are tested through. This does not contradict AC-13 — the client is never reached for those commands, because the dispatch returns 69 first.
- **If a test hangs**, the likely cause is the accept loop: after a `stop` request the daemon sets the flag and then connects to itself to unblock `accept`. Without that connection the loop waits forever.
- **Do not enable any `interprocess` feature.** `cargo tree | grep -i -E 'tokio|futures'` must stay empty.
