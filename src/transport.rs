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
    /// A filesystem path. Unix only: on Windows the name lives in the pipe
    /// namespace, so this constructor would have no caller and `-D warnings`
    /// rejects one that does not exist.
    #[cfg(unix)]
    pub fn path(value: String) -> Address {
        Address {
            value,
            namespaced: false,
        }
    }

    /// A name in the pipe namespace. Windows only, for the same reason reversed.
    #[cfg(windows)]
    pub fn namespaced(value: String) -> Address {
        Address {
            value,
            namespaced: true,
        }
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
    Io {
        what: &'static str,
        address: String,
        source: io::Error,
    },
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
            TransportError::Io {
                what,
                address,
                source,
            } => {
                write!(f, "cannot {what} {address}: {source}")
            }
        }
    }
}

impl std::error::Error for TransportError {}

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
    vars.home.as_ref().map(|home| {
        PathBuf::from(home)
            .join(".local/state/herdr/plugins")
            .join(PLUGIN_ID)
    })
}

#[cfg(windows)]
pub fn address(_vars: &Vars) -> Result<Address, TransportError> {
    // The pipe namespace is machine-wide, which is issue #6.
    Ok(Address::namespaced(PLUGIN_ID.to_string()))
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

#[derive(Debug)]
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

#[derive(Debug)]
pub struct Listener(interprocess::local_socket::Listener);

impl Listener {
    pub fn accept(&self) -> Result<Stream, TransportError> {
        self.0
            .accept()
            .map(Stream)
            .map_err(|source| TransportError::Io {
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

#[cfg(test)]
mod tests {
    use super::tests_support::probe_address;
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
        assert_eq!(
            address(&vars).unwrap().display(),
            "/tmp/herdr-state/voice.sock"
        );
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

        let vars = Vars {
            state_dir: None,
            xdg_state_home: None,
            home: Some("/tmp/home".into()),
        };
        assert_eq!(
            address(&vars).unwrap().display(),
            "/tmp/home/.local/state/herdr/plugins/haurylau.voice/voice.sock"
        );
    }

    #[cfg(unix)]
    #[test]
    fn with_nothing_to_go_on_it_says_so() {
        let vars = Vars {
            state_dir: None,
            xdg_state_home: None,
            home: None,
        };
        let error = address(&vars).expect_err("must refuse");
        assert!(
            error.to_string().contains("HERDR_PLUGIN_STATE_DIR"),
            "got {error}"
        );
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
        // A listener dropped in an orderly way removes its own socket file, so a
        // leftover is only produced by a process that died holding it. That is what
        // this reproduces: a file sitting at the name, with nothing behind it.
        let address = probe_address("stale");
        let path = std::path::Path::new(address.display());
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("create the directory");
        std::fs::write(path, b"a dead daemon was here").expect("plant a leftover");
        assert!(path.exists());

        let listener = listen(&address).expect("a leftover must not block a listener");
        assert!(connect(&address).is_ok(), "the reclaimed name must accept");
        drop(listener);
    }
}
