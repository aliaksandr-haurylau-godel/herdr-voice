//! A daemon whose standard error has no reader still answers (issue #93).
//!
//! herdr connects the daemon's standard error to a pipe it reads, and a herdr
//! restart leaves the daemon running with nobody on the read end. That is a fact
//! about the descriptors of the built binary, so it is asserted by starting the
//! binary with a pipe whose reader is closed. No herdr is involved.
//!
//! Unix only: the socket is a file, and the stand-in for the closed reader is a
//! pipe.
#![cfg(unix)]

use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

const BINARY: &str = env!("CARGO_BIN_EXE_herdr-voice");

/// The daemon process and the directory it keeps its socket in; both are
/// removed when this is dropped, so a failed assertion leaves nothing running.
struct Daemon {
    child: Child,
    dir: PathBuf,
}

impl Drop for Daemon {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

impl Daemon {
    fn command(&self, arguments: &[&str]) -> Command {
        let mut command = Command::new(BINARY);
        command
            .args(arguments)
            .env("HERDR_PLUGIN_STATE_DIR", &self.dir)
            .env("HERDR_PLUGIN_CONFIG_DIR", &self.dir)
            .env("HERDR_BIN_PATH", self.dir.join("no-herdr-here"))
            .env_remove("HERDR_PLUGIN_CONTEXT_JSON");
        command
    }
}

/// Starts the daemon with standard error on a pipe and closes the read end at
/// once, then waits until it accepts a connection.
fn daemon_with_dead_stderr(tag: &str) -> Daemon {
    // Short on purpose: a socket path is limited to about a hundred bytes.
    let dir = std::env::temp_dir().join(format!("hv-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch dir");
    let mut daemon = Daemon {
        child: Command::new("true").spawn().expect("a placeholder child"),
        dir,
    };
    let _ = daemon.child.wait();
    let mut child = daemon
        .command(&["daemon"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("start the daemon");
    drop(child.stderr.take());
    daemon.child = child;
    let socket = daemon.dir.join("voice.sock");
    let deadline = Instant::now() + Duration::from_secs(10);
    while std::os::unix::net::UnixStream::connect(&socket).is_err() {
        assert!(
            Instant::now() < deadline,
            "the daemon never listened at {socket:?}"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
    daemon
}

#[test]
fn a_request_is_answered_when_nobody_reads_standard_error() {
    let daemon = daemon_with_dead_stderr("cancel");
    let output = daemon
        .command(&["cancel"])
        .output()
        .expect("run the client");
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        "nothing to cancel",
        "stderr was {:?}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.status.success(), "{:?}", output.status);
}

#[test]
fn doctor_finds_that_daemon_healthy_only_because_it_answers() {
    let daemon = daemon_with_dead_stderr("doctor");
    let output = daemon.command(&["doctor"]).output().expect("run doctor");
    let report = String::from_utf8_lossy(&output.stdout);
    let line = report
        .lines()
        .find(|line| line.starts_with("daemon"))
        .unwrap_or_else(|| panic!("no daemon line in {report:?}"));
    assert!(line.contains(" ok "), "got {line:?}");
}
