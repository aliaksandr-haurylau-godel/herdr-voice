//! Running somebody else's program with a bound on how long it may take, and
//! cutting what it printed. `docs/decisions.md` holds the bound for each call.

use std::io::Read;
use std::process::{Child, Command, Output, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

/// `text` cut to at most `limit` bytes on a character boundary.
///
/// `String::truncate` panics when the length is not on a boundary, and program
/// output reaches here through `String::from_utf8_lossy`, where any multi-byte
/// character, or the replacement character, can straddle the limit (issue #94).
pub fn shorten(text: &str, limit: usize) -> String {
    if text.len() <= limit {
        return text.to_string();
    }
    let mut end = limit;
    // Zero is always a boundary, so this ends.
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].to_string()
}

/// Why a run produced no output.
#[derive(Debug)]
pub enum RunError {
    /// The program could not be started, or its state could not be read.
    Start(std::io::Error),
    /// The program, or something it started, was still going at the bound. It
    /// has been stopped.
    TimedOut,
}

/// How often the child is looked at. Short enough that a bound of a fraction of a
/// second is met, long enough to cost nothing.
const POLL: Duration = Duration::from_millis(10);

/// The least a closed program's pipes are given to reach their end. A pipe that
/// is still open after that is held by something the program left behind.
const DRAIN: Duration = Duration::from_secs(1);

/// Reads `source` to its end on a thread of its own. Two of these, one per
/// stream, because a single reader blocks on one full pipe while the program
/// fills the other.
fn drain<R: Read + Send + 'static>(mut source: R) -> mpsc::Receiver<Vec<u8>> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let mut bytes = Vec::new();
        // What was read before an error is kept: it is all there is.
        let _ = source.read_to_end(&mut bytes);
        let _ = sender.send(bytes);
    });
    receiver
}

/// What a stream carried, or `None` when it did not end by `until`. One instant
/// for both streams: a wait given to each in turn would let a call last twice
/// its bound.
fn collect(stream: Option<mpsc::Receiver<Vec<u8>>>, until: Instant) -> Option<Vec<u8>> {
    match stream {
        None => Some(Vec::new()),
        Some(receiver) => receiver
            .recv_timeout(until.saturating_duration_since(Instant::now()))
            .ok(),
    }
}

/// How long a killed child is waited for. A process in uninterruptible sleep
/// does not die on a kill, and a wait with no end would make the timeout path
/// the thing that hangs.
const REAP: Duration = Duration::from_secs(2);

/// Stops the child and everything it started, then reaps it.
///
/// The standard library cannot signal a process group and `libc` is not a
/// dependency, so on Unix the group is signalled by running `kill`. The child
/// is its own group leader (see `run`), so the group id is its process id.
/// herdr starts plugin commands with a minimal PATH, so the usual absolute
/// locations are tried before a lookup. The direct kill after it covers a
/// machine where none of them is there.
fn stop(child: &mut Child) {
    #[cfg(unix)]
    {
        let group = format!("-{}", child.id());
        for program in ["/bin/kill", "/usr/bin/kill", "kill"] {
            let signalled = Command::new(program)
                .args(["-s", "KILL", "--", &group])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
            if signalled.is_ok() {
                break;
            }
        }
    }
    let _ = child.kill();
    let deadline = Instant::now() + REAP;
    while Instant::now() < deadline {
        match child.try_wait() {
            Ok(None) => thread::sleep(POLL),
            _ => break,
        }
    }
}

/// Runs `command` and waits at most `bound` for it and for its output.
///
/// Standard input is closed, so a program that reads it ends rather than waiting
/// for a person. Standard output and error are captured, as `Command::output`
/// does; the difference is that this returns.
pub fn run(command: &mut Command, bound: Duration) -> Result<Output, RunError> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // Its own group, so a timeout reaches what a shell wrapper started.
        command.process_group(0);
    }
    let mut child = command.spawn().map_err(RunError::Start)?;
    let stdout = child.stdout.take().map(drain);
    let stderr = child.stderr.take().map(drain);

    let deadline = Instant::now() + bound;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => thread::sleep(POLL),
            Ok(None) => {
                stop(&mut child);
                return Err(RunError::TimedOut);
            }
            Err(error) => {
                stop(&mut child);
                return Err(RunError::Start(error));
            }
        }
    };

    let until = deadline.max(Instant::now() + DRAIN);
    match (collect(stdout, until), collect(stderr, until)) {
        (Some(stdout), Some(stderr)) => Ok(Output {
            status,
            stdout,
            stderr,
        }),
        _ => {
            stop(&mut child);
            Err(RunError::TimedOut)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_that_fits_is_returned_unchanged() {
        assert_eq!(shorten("short", 400), "short");
        assert_eq!(shorten("exact", 5), "exact");
    }

    #[test]
    fn ascii_is_cut_at_the_limit() {
        assert_eq!(shorten("abcdef", 3), "abc");
    }

    #[test]
    fn a_limit_of_zero_gives_nothing() {
        assert_eq!(shorten("abc", 0), "");
    }

    #[test]
    fn a_two_byte_character_across_the_limit_is_dropped_whole() {
        let text = format!("{}é", "a".repeat(399));
        assert_eq!(text.len(), 401);
        assert_eq!(shorten(&text, 400), "a".repeat(399));
    }

    #[test]
    fn a_three_byte_character_across_the_limit_is_dropped_whole() {
        let text = format!("{}—", "a".repeat(398));
        assert_eq!(text.len(), 401);
        assert_eq!(shorten(&text, 400), "a".repeat(398));
        assert_eq!(shorten(&text, 399), "a".repeat(398));
    }

    #[test]
    fn a_four_byte_character_across_the_limit_is_dropped_whole() {
        let text = format!("{}😀", "a".repeat(397));
        assert_eq!(text.len(), 401);
        assert_eq!(shorten(&text, 400), "a".repeat(397));
    }

    #[test]
    fn no_limit_panics_on_cyrillic_text() {
        let text = "ошибка чтения файла".repeat(30);
        for limit in 0..=text.len() + 1 {
            let cut = shorten(&text, limit);
            assert!(cut.len() <= limit, "limit {limit}: {} bytes", cut.len());
            assert!(text.starts_with(&cut), "limit {limit}");
        }
    }

    /// No `truncate` on text from outside the process may come back (issue #94).
    /// The needle is assembled so this file does not match itself. The one
    /// allowed use cuts a byte vector a test fixture owns.
    #[test]
    fn no_source_file_cuts_a_string_with_truncate() {
        let needle = [".trunc", "ate("].concat();
        let fixture = ["files[0].2.trunc", "ate(8);"].concat();
        let allowed = [("stt/fetch.rs", fixture.as_str())];
        let mut found = Vec::new();
        let mut pending = vec![std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src")];
        while let Some(directory) = pending.pop() {
            for entry in std::fs::read_dir(&directory).expect("read src") {
                let path = entry.expect("entry").path();
                if path.is_dir() {
                    pending.push(path);
                } else if path.extension().is_some_and(|e| e == "rs") {
                    let text = std::fs::read_to_string(&path).expect("read source");
                    for (index, line) in text.lines().enumerate() {
                        if line.contains(&needle)
                            && !allowed
                                .iter()
                                .any(|(tail, code)| path.ends_with(tail) && line.trim() == *code)
                        {
                            found.push(format!("{}:{}", path.display(), index + 1));
                        }
                    }
                }
            }
        }
        assert!(found.is_empty(), "cut with a truncate: {found:?}");
    }

    #[test]
    fn a_stream_that_was_never_opened_has_nothing_to_say() {
        assert_eq!(collect(None, Instant::now()), Some(Vec::new()));
    }

    #[cfg(unix)]
    mod run_tests {
        use super::super::*;
        use std::process::Command;
        use std::time::{Duration, Instant};

        fn sh(script: &str) -> Command {
            let mut command = Command::new("sh");
            command.arg("-c").arg(script);
            command
        }

        /// Whether a process still exists, polled because a killed process is
        /// briefly a zombie until its parent reaps it.
        fn gone(pid: &str) -> bool {
            for _ in 0..40 {
                let alive = Command::new("kill")
                    .arg("-0")
                    .arg(pid)
                    .stderr(std::process::Stdio::null())
                    .status()
                    .expect("kill can be run to look for a process")
                    .success();
                if !alive {
                    return true;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            false
        }

        fn scratch(tag: &str) -> std::path::PathBuf {
            let dir = std::env::temp_dir()
                .join(format!("herdr-voice-outward-{tag}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).expect("create scratch");
            dir
        }

        #[test]
        fn a_program_that_finishes_returns_what_it_printed_and_its_status() {
            let output = run(
                &mut sh("printf out; printf err >&2; exit 3"),
                Duration::from_secs(5),
            )
            .expect("runs");
            assert_eq!(output.stdout, b"out");
            assert_eq!(output.stderr, b"err");
            assert_eq!(output.status.code(), Some(3));
        }

        #[test]
        fn a_program_that_is_not_there_is_a_start_failure() {
            let error = run(
                &mut Command::new("/definitely/not/a/real/program"),
                Duration::from_secs(5),
            )
            .expect_err("must fail");
            match error {
                RunError::Start(e) => assert_eq!(e.kind(), std::io::ErrorKind::NotFound),
                other => panic!("expected Start, got {other:?}"),
            }
        }

        #[test]
        fn standard_input_is_closed_so_a_program_that_reads_it_does_not_wait() {
            let started = Instant::now();
            let output = run(&mut sh("cat"), Duration::from_secs(5)).expect("runs");
            assert!(output.stdout.is_empty());
            assert!(started.elapsed() < Duration::from_secs(3));
        }

        #[test]
        fn output_larger_than_a_pipe_on_both_streams_does_not_deadlock() {
            let output = run(
                &mut sh("head -c 300000 /dev/zero | tr '\\0' x; \
                         head -c 300000 /dev/zero | tr '\\0' y >&2"),
                Duration::from_secs(10),
            )
            .expect("runs");
            assert_eq!(output.stdout.len(), 300_000);
            assert_eq!(output.stderr.len(), 300_000);
        }

        #[test]
        fn a_program_that_outlasts_the_bound_is_stopped_and_reported() {
            let started = Instant::now();
            let error =
                run(&mut sh("sleep 30"), Duration::from_millis(200)).expect_err("times out");
            assert!(matches!(error, RunError::TimedOut), "got {error:?}");
            assert!(
                started.elapsed() < Duration::from_secs(3),
                "returned after {:?}",
                started.elapsed()
            );
        }

        #[test]
        fn a_timeout_kills_what_the_program_started_not_only_the_program() {
            let dir = scratch("group");
            let pidfile = dir.join("pid");
            // `sh -c "program"` is how a transcriber is usually configured;
            // the grandchild is the real program.
            let script = format!("sleep 30 & echo $! > {pidfile:?}; wait");
            let error = run(&mut sh(&script), Duration::from_secs(2)).expect_err("times out");
            assert!(matches!(error, RunError::TimedOut), "got {error:?}");
            let pid = std::fs::read_to_string(&pidfile).expect("the script wrote it");
            assert!(gone(pid.trim()), "process {} is still running", pid.trim());
        }

        #[test]
        fn a_background_child_holding_the_pipe_open_is_a_timeout_not_a_hang() {
            let dir = scratch("pipe");
            let pidfile = dir.join("pid");
            let script = format!("sleep 30 & echo $! > {pidfile:?}; echo done");
            let started = Instant::now();
            let error = run(&mut sh(&script), Duration::from_millis(300)).expect_err("times out");
            assert!(matches!(error, RunError::TimedOut), "got {error:?}");
            assert!(
                started.elapsed() < Duration::from_secs(4),
                "returned after {:?}",
                started.elapsed()
            );
            let pid = std::fs::read_to_string(&pidfile).expect("the script wrote it");
            assert!(gone(pid.trim()), "process {} is still running", pid.trim());
        }
        #[test]
        fn a_program_that_exits_but_leaves_a_pipe_open_is_reported_within_the_bound_not_twice_it() {
            let dir = scratch("twice");
            let pidfile = dir.join("pid");
            let script = format!("sleep 30 & echo $! > {pidfile:?}");
            let started = Instant::now();
            let error = run(&mut sh(&script), Duration::from_secs(2)).expect_err("times out");
            assert!(matches!(error, RunError::TimedOut), "got {error:?}");
            // Each stream used to be given the whole remaining time in turn.
            assert!(
                started.elapsed() < Duration::from_millis(3200),
                "returned after {:?}",
                started.elapsed()
            );
            let pid = std::fs::read_to_string(&pidfile).expect("the script wrote it");
            assert!(gone(pid.trim()), "process {} is still running", pid.trim());
        }
        #[test]
        fn a_fast_program_returns_promptly() {
            let started = Instant::now();
            run(&mut sh("exit 0"), Duration::from_secs(5)).expect("runs");
            assert!(
                started.elapsed() < Duration::from_millis(600),
                "took {:?}",
                started.elapsed()
            );
        }

        #[test]
        fn a_program_that_ignores_the_polite_signal_is_still_stopped_with_what_it_started() {
            let dir = scratch("ignore-term");
            let pidfile = dir.join("pid");
            // Ignored signals are inherited, so the grandchild ignores TERM as
            // well: only KILL stops it.
            let script = format!("trap '' TERM; sleep 30 & echo $! > {pidfile:?}; wait");
            let error = run(&mut sh(&script), Duration::from_secs(2)).expect_err("times out");
            assert!(matches!(error, RunError::TimedOut), "got {error:?}");
            let pid = std::fs::read_to_string(&pidfile).expect("the script wrote it");
            assert!(gone(pid.trim()), "process {} is still running", pid.trim());
        }
    }
}
