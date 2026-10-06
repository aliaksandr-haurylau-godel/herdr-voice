//! Writing a script a test then runs.
//!
//! On Linux an `exec` fails with `ETXTBSY` ("Text file busy") while any file
//! description open for writing on the file exists. The unit tests run as
//! threads of one process; a thread that forks while another has the script
//! open for writing gives its child a copy of that description until the
//! child's own `exec`, so the first thread's spawn can land in that window.
//! macOS does not refuse (see docs/evidence.md, "Text file busy in test
//! fixtures"), which is why only Linux CI shows it.
//!
//! `write_executable` closes the window for the file it writes: after the write
//! it executes the script once with a probe variable set and retries while the
//! spawn fails with `ETXTBSY`. A successful probe means no description open for
//! writing exists any more, and nothing opens the file for writing again, so
//! every later execution of it cannot fail that way. The scripts are POSIX
//! shell: the probe depends on a line only `sh` understands.

use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Set only by the probe. The guard line makes a probed script exit before it
/// does anything.
const PROBE_VAR: &str = "HERDR_VOICE_FIXTURE_PROBE";
const GUARD: &str = "[ -n \"$HERDR_VOICE_FIXTURE_PROBE\" ] && exit 0";
/// The error number of `ETXTBSY`, the same on Linux and macOS. `ErrorKind`
/// does not name it before Rust 1.83 and this crate builds on 1.82.
const ETXTBSY: i32 = 26;

/// Writes `content` to `path` with mode `0o755` and returns once the script can
/// be executed. `content` must start with a `#!/bin/sh` line: the guard line
/// that is inserted after it, so the probe execution does nothing, is POSIX
/// shell.
pub fn write_executable(path: &Path, content: &str) {
    let (interpreter, rest) = content.split_once('\n').unwrap_or((content, ""));
    assert!(
        interpreter.starts_with("#!/bin/sh"),
        "a fixture script must start with #!/bin/sh, got {interpreter:?}"
    );
    {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o755)
            .open(path)
            .unwrap_or_else(|e| panic!("cannot create {}: {e}", path.display()));
        // The mode above applies only to a file this call creates.
        file.set_permissions(std::fs::Permissions::from_mode(0o755))
            .unwrap_or_else(|e| panic!("cannot make {} executable: {e}", path.display()));
        file.write_all(format!("{interpreter}\n{GUARD}\n{rest}").as_bytes())
            .unwrap_or_else(|e| panic!("cannot write {}: {e}", path.display()));
    } // The write descriptor is closed here, before anything is executed.
    wait_until_executable(path, Duration::from_secs(10));
}

/// Executes the script once with the probe variable set, retrying every 5 ms
/// while the spawn fails with `ETXTBSY`, and returns the number of retries.
/// Any other spawn error, or `ETXTBSY` still present after `deadline`, panics
/// with the path and the error.
pub fn wait_until_executable(path: &Path, deadline: Duration) -> u32 {
    let start = Instant::now();
    let mut retries = 0;
    loop {
        let spawned = Command::new(path)
            .env(PROBE_VAR, "1")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        match spawned {
            Ok(_) => return retries,
            Err(e) if e.raw_os_error() == Some(ETXTBSY) && start.elapsed() < deadline => {
                retries += 1;
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(e) => panic!("{} cannot be executed: {e}", path.display()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    struct Scratch(std::path::PathBuf);

    impl Scratch {
        fn new(tag: &str) -> Self {
            let dir = std::env::temp_dir().join(format!(
                "herdr-voice-script-fixture-{tag}-{}",
                std::process::id()
            ));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).expect("scratch dir");
            Scratch(dir)
        }

        fn path(&self, name: &str) -> std::path::PathBuf {
            self.0.join(name)
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// A script that already carries the guard, created the way
    /// `write_executable` creates it but without waiting, so a test can keep it
    /// open.
    fn guarded_script(path: &Path) {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o755)
            .open(path)
            .expect("create the script");
        file.write_all(format!("#!/bin/sh\n{GUARD}\n").as_bytes())
            .expect("write the script");
    }

    #[test]
    fn the_probe_runs_nothing_of_the_script_and_a_real_run_runs_all_of_it() {
        let scratch = Scratch::new("probe");
        let out = scratch.path("out.txt");
        let script = scratch.path("s.sh");
        write_executable(&script, &format!("#!/bin/sh\nprintf 'ran\\n' > {out:?}\n"));
        assert!(
            !out.exists(),
            "the probe must exit before the script's own commands"
        );
        let status = Command::new(&script).status().expect("run the script");
        assert!(status.success());
        assert_eq!(std::fs::read_to_string(&out).unwrap(), "ran\n");
    }

    #[test]
    fn a_script_still_open_for_writing_is_waited_for_and_not_reported_as_busy() {
        let scratch = Scratch::new("held");
        let path = scratch.path("s.sh");
        guarded_script(&path);
        let held = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .expect("hold the script open for writing");
        let released = Arc::new(AtomicBool::new(false));
        let release = released.clone();
        let (go, started) = std::sync::mpsc::channel::<()>();
        let releaser = std::thread::spawn(move || {
            started.recv().expect("the wait is about to begin");
            std::thread::sleep(Duration::from_millis(400));
            // Set before the handle goes, so a call that has returned can only
            // have seen it set.
            release.store(true, Ordering::SeqCst);
            drop(held);
        });
        go.send(()).unwrap();
        let retries = wait_until_executable(&path, Duration::from_secs(10));
        let released_when_it_returned = released.load(Ordering::SeqCst);
        releaser.join().unwrap();
        if cfg!(target_os = "linux") {
            assert!(
                released_when_it_returned,
                "it returned while the script was still open for writing"
            );
            assert!(
                retries >= 1,
                "Linux refuses a busy file; got {retries} retries"
            );
        } else {
            // macOS executes a file that is open for writing: there is nothing
            // to wait for, so it returns at once, before the handle is released.
            assert_eq!(retries, 0);
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    #[should_panic(expected = "cannot be executed")]
    fn a_script_that_stays_open_past_the_deadline_panics_naming_the_script() {
        let scratch = Scratch::new("deadline");
        let path = scratch.path("s.sh");
        guarded_script(&path);
        let _held = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .expect("hold the script open for writing");
        wait_until_executable(&path, Duration::from_millis(100));
    }

    #[test]
    #[should_panic(expected = "cannot be executed")]
    fn a_script_that_cannot_be_executed_for_another_reason_panics_at_once() {
        let scratch = Scratch::new("noexec");
        let path = scratch.path("s.sh");
        // `fs::write` creates the file without the execute bit.
        std::fs::write(&path, format!("#!/bin/sh\n{GUARD}\n")).unwrap();
        wait_until_executable(&path, Duration::from_secs(10));
    }

    #[test]
    #[should_panic(expected = "must start with #!/bin/sh")]
    fn a_script_for_another_interpreter_is_refused() {
        // The guard line is POSIX shell; under another interpreter it would be a
        // syntax error, or the probe would run the whole script.
        let scratch = Scratch::new("otherinterpreter");
        write_executable(
            &scratch.path("s.py"),
            "#!/usr/bin/env python3\nprint('hello')\n",
        );
    }

    #[test]
    fn a_file_that_already_exists_without_the_execute_bit_is_made_executable() {
        let scratch = Scratch::new("existing");
        let path = scratch.path("s.sh");
        // `fs::write` creates the file without the execute bit; the mode of
        // `OpenOptions` applies only to a file it creates.
        std::fs::write(&path, "stale\n").unwrap();
        write_executable(&path, "#!/bin/sh\nexit 0\n");
        let status = Command::new(&path).status().expect("run the script");
        assert!(status.success());
    }

    #[test]
    #[should_panic(expected = "must start with #!")]
    fn a_script_without_an_interpreter_line_is_refused() {
        let scratch = Scratch::new("nointerpreter");
        write_executable(&scratch.path("s.sh"), "echo hello\nexit 0\n");
    }
}
