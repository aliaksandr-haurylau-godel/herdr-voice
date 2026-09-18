//! What `setup` does as a process. Two claims route the whole feature and
//! neither can be reached from a unit test in `src/setup.rs`: that standard
//! input not being a terminal is what sends the run down the pane-opening
//! branch, and that the code `setup` returns is the code the caller sees.
//! Both are about the built binary being started — its descriptors and its
//! exit status — so they are asserted by running it. `CLAUDE.md` puts unit
//! tests next to the code; this file is the departure `tasks/41/DESIGN_41.md`
//! section 10 states, because the alternative is leaving the two decisions
//! that route the feature unproven.
//!
//! Unix only: the stand-in for herdr is a shell script.
//!
//! The interactive branch is not covered here. It is chosen by standard input
//! being a terminal, and a child started from a test has a pipe; giving it a
//! terminal needs a pseudoterminal this crate has no dependency for. What that
//! branch does is covered by the unit tests, which call `run` directly.
#![cfg(unix)]

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// A scratch directory holding the stand-in for herdr, the file it records
/// what it was given in, and the configuration path the run is pointed at.
/// Unique per test and per process, and removed when it is dropped.
struct Scratch {
    dir: PathBuf,
    script: PathBuf,
    record: PathBuf,
    config: PathBuf,
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

impl Scratch {
    /// `code` is what the stand-in exits with, which is how a refused pane is
    /// staged.
    fn new(tag: &str, code: i32) -> Self {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!(
            "herdr-voice-setup-process-{tag}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("scratch dir");
        let scratch = Scratch {
            script: dir.join("herdr.sh"),
            record: dir.join("record.out"),
            config: dir.join("config.toml"),
            dir,
        };
        std::fs::write(
            &scratch.script,
            format!(
                "#!/bin/sh\nprintf '%s\\n' \"$@\" > {record:?}\n\
                 echo 'a popup pane is already open'\nexit {code}\n",
                record = scratch.record,
            ),
        )
        .expect("write the stand-in");
        let mut perms = std::fs::metadata(&scratch.script)
            .expect("stat")
            .permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&scratch.script, perms).expect("chmod");
        scratch
    }

    /// Runs the built binary with standard input a pipe — no terminal — and
    /// every call it makes to herdr going to the stand-in.
    fn run_setup(&self) -> std::process::Output {
        let child = Command::new(env!("CARGO_BIN_EXE_herdr-voice"))
            .arg("setup")
            .env("HERDR_BIN_PATH", &self.script)
            .env("HERDR_CONFIG_PATH", &self.config)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("the built binary must be runnable");
        // The pipe is closed with the child handle, so the read the
        // interactive branch would do could not block even if it were reached.
        child.wait_with_output().expect("the child must finish")
    }

    fn recorded(&self) -> Vec<String> {
        recorded(&self.record)
    }
}

fn recorded(path: &Path) -> Vec<String> {
    std::fs::read_to_string(path)
        .expect("the stand-in for herdr must have run and written what it was given")
        .lines()
        .map(|l| l.to_string())
        .collect()
}

/// With no terminal there is nobody to ask, so the run asks herdr to open the
/// pane that has one, and touches no configuration on the way.
#[test]
fn with_standard_input_a_pipe_it_opens_the_pane_and_writes_nothing() {
    let scratch = Scratch::new("pane", 0);
    let out = scratch.run_setup();
    assert!(
        out.status.success(),
        "exit {:?}, stdout {:?}, stderr {:?}",
        out.status.code(),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        scratch.recorded(),
        vec![
            "plugin",
            "pane",
            "open",
            "--plugin",
            "herdr-voice",
            "--entrypoint",
            "setup",
        ]
    );
    assert!(
        !scratch.config.exists(),
        "the branch with no terminal asks no question, so it writes no configuration"
    );
}

/// The code the run returns has to survive the trip out of `main`: a failure
/// that reports itself and then exits 0 is one nothing invoking this can act
/// on.
#[test]
fn a_failure_leaves_the_process_with_a_non_zero_exit_code() {
    let scratch = Scratch::new("refused", 1);
    let out = scratch.run_setup();
    assert!(
        !out.status.success(),
        "a refused pane must not look like success: stdout {:?}",
        String::from_utf8_lossy(&out.stdout)
    );
    let said = String::from_utf8_lossy(&out.stdout);
    assert!(
        said.contains("setup pane"),
        "and it must say what could not be done: {said}"
    );
}
