//! Putting a take's text into a pane, or telling somebody it could not go
//! there. Mirrors the shape src/stt.rs uses for the transcriber.

use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeliveryError {
    /// The code alone, extracted from herdr's structured refusal — see
    /// docs/evidence.md, "Delivering into a pane that is gone" — or the raw
    /// output when it did not parse as that shape.
    Rejected(String),
    /// `herdr` itself was not found.
    NotFound { binary: String, path: String },
    /// `herdr` was found and this process is not allowed to run it.
    NotExecutable { binary: String },
    /// Starting `herdr` failed for another reason, which is the operating
    /// system's own text.
    StartFailed { binary: String, reason: String },
    /// `herdr` was started and did not finish; it has been stopped.
    TimedOut { binary: String, bound: Duration },
}

impl std::fmt::Display for DeliveryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DeliveryError::Rejected(why) => write!(f, "{why}"),
            // Mirrors CommandError::NotFound (src/stt/command.rs:81-86).
            DeliveryError::NotFound { binary, path } => write!(
                f,
                "cannot run {binary:?}: it is not on the PATH this process has, which is \
                 {path:?}. Set HERDR_BIN_PATH to herdr's location, or start herdr from a shell \
                 where it is on the PATH"
            ),
            DeliveryError::NotExecutable { binary } => write!(
                f,
                "cannot run {binary:?}: the file was found but this process is not allowed to \
                 run it. Make it executable (on Unix, chmod +x), or point HERDR_BIN_PATH at the \
                 herdr program itself"
            ),
            DeliveryError::TimedOut { binary, bound } => write!(
                f,
                "{binary:?} did not answer within {}, so the plugin stopped it. The text may \
                 already have reached the pane, so look there first. If herdr is not \
                 responding, restart it",
                crate::http_failure::bound_text(*bound)
            ),
            DeliveryError::StartFailed { binary, reason } => write!(
                f,
                "cannot run {binary:?}: the operating system reported {reason:?}. This is often \
                 temporary: try again, and if it keeps happening, report that text"
            ),
        }
    }
}
impl std::error::Error for DeliveryError {}

pub trait Deliverer: Send + Sync {
    fn insert(&self, pane: &str, text: &str) -> Result<(), DeliveryError>;
    fn submit(&self, pane: &str, text: &str) -> Result<(), DeliveryError>;
    fn notify(&self, title: &str, body: &str) -> Result<(), DeliveryError>;
}

/// `[delivery] submit` and `[ui] toasts`, resolved once at daemon start.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Settings {
    pub submit: bool,
    pub toasts: bool,
}

/// `submit` is `Settings.submit`; `agent` is `Take.agent` — neither is
/// looked up again here.
pub fn deliver(
    deliverer: &dyn Deliverer,
    submit: bool,
    agent: Option<&str>,
    pane: &str,
    text: &str,
) -> Result<(), DeliveryError> {
    // How herdr encodes "no agent" was never established: it may omit the
    // field or send an empty string. An empty agent is treated the same as
    // no agent, so it falls back to inserting rather than submitting.
    let agent = agent.filter(|a| !a.is_empty());
    if submit && agent.is_some() {
        deliverer.submit(pane, text)
    } else {
        deliverer.insert(pane, text)
    }
}

#[cfg(test)]
pub mod tests_support {
    use super::{Deliverer, DeliveryError};
    use std::sync::{Arc, Mutex};

    /// A script that sleeps thirty seconds when its first argument is one of
    /// `subcommands` (`"*"` means any), and exits 0 otherwise. Written the way
    /// the recorder in this file's tests is: closed before it is run.
    #[cfg(unix)]
    pub fn herdr_that_hangs_on(tag: &str, subcommands: &[&str]) -> String {
        let dir =
            std::env::temp_dir().join(format!("herdr-voice-hang-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create scratch");
        let script = dir.join("herdr.sh");
        let patterns = subcommands.join("|");
        std::fs::write(
            &script,
            format!("#!/bin/sh\ncase \"$1\" in {patterns}) sleep 30;; esac\nexit 0\n"),
        )
        .expect("write script");
        let mut permissions = std::fs::metadata(&script).unwrap().permissions();
        std::os::unix::fs::PermissionsExt::set_mode(&mut permissions, 0o755);
        std::fs::set_permissions(&script, permissions).expect("chmod script");
        script.to_string_lossy().into_owned()
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum Call {
        Insert(String, String),
        Submit(String, String),
        Notify(String, String),
    }

    /// Records every call, in order, and returns one fixed result for every
    /// call. `Clone`, sharing its log through an inner `Arc`, so a test can
    /// keep one clone while another is moved into a `Runtime`.
    #[derive(Clone)]
    pub struct FakeDeliverer {
        result: Result<(), DeliveryError>,
        /// Independent of `result`: a test proving the toast's own failure is
        /// handled needs `insert`/`submit` to fail (so a toast is attempted at
        /// all) while `notify` fails for its own, separate reason.
        notify_result: Result<(), DeliveryError>,
        calls: Arc<Mutex<Vec<Call>>>,
    }

    impl FakeDeliverer {
        pub fn ok() -> Self {
            FakeDeliverer {
                result: Ok(()),
                notify_result: Ok(()),
                calls: Arc::new(Mutex::new(Vec::new())),
            }
        }
        pub fn failing(with: DeliveryError) -> Self {
            FakeDeliverer {
                result: Err(with),
                notify_result: Ok(()),
                calls: Arc::new(Mutex::new(Vec::new())),
            }
        }
        /// Makes `notify` fail too, on top of whatever `result` already governs.
        pub fn and_notify_fails(mut self, with: DeliveryError) -> Self {
            self.notify_result = Err(with);
            self
        }
        pub fn calls(&self) -> Vec<Call> {
            self.calls.lock().unwrap().clone()
        }
    }

    impl Deliverer for FakeDeliverer {
        fn insert(&self, pane: &str, text: &str) -> Result<(), DeliveryError> {
            self.calls
                .lock()
                .unwrap()
                .push(Call::Insert(pane.to_string(), text.to_string()));
            self.result.clone()
        }
        fn submit(&self, pane: &str, text: &str) -> Result<(), DeliveryError> {
            self.calls
                .lock()
                .unwrap()
                .push(Call::Submit(pane.to_string(), text.to_string()));
            self.result.clone()
        }
        fn notify(&self, title: &str, body: &str) -> Result<(), DeliveryError> {
            self.calls
                .lock()
                .unwrap()
                .push(Call::Notify(title.to_string(), body.to_string()));
            self.notify_result.clone()
        }
    }
}

/// The herdr binary to run: whatever `HERDR_BIN_PATH` names, or `herdr` on the
/// path. Every outward call to herdr resolves it here, so that a checkout
/// pointed at a specific build reaches delivery and the pane read alike.
pub fn herdr_binary() -> String {
    std::env::var("HERDR_BIN_PATH").unwrap_or_else(|_| "herdr".to_string())
}

fn extract_reason(output: &[u8]) -> String {
    #[derive(serde::Deserialize)]
    struct Envelope {
        error: ErrorBody,
    }
    #[derive(serde::Deserialize)]
    struct ErrorBody {
        code: String,
    }

    let text = String::from_utf8_lossy(output).trim().to_string();
    match serde_json::from_str::<Envelope>(&text) {
        Ok(envelope) => envelope.error.code,
        Err(_) => text,
    }
}

/// What starting the program failed with, read from the kind the operating
/// system gave. Only `NotFound` means the program is not on the `PATH`;
/// `PermissionDenied` means it was found and cannot be run (no execute bit, or a
/// directory); anything else — a file still open for writing, exhausted
/// processes — carries the system's own text.
fn start_failure(binary: &str, error: &std::io::Error) -> DeliveryError {
    match error.kind() {
        std::io::ErrorKind::NotFound => DeliveryError::NotFound {
            binary: binary.to_string(),
            path: std::env::var("PATH").unwrap_or_default(),
        },
        std::io::ErrorKind::PermissionDenied => DeliveryError::NotExecutable {
            binary: binary.to_string(),
        },
        _ => DeliveryError::StartFailed {
            binary: binary.to_string(),
            reason: error.to_string(),
        },
    }
}

/// How long one herdr call may take. `docs/decisions.md`, the entry for
/// delivery's herdr calls.
pub const BOUND: Duration = Duration::from_secs(10);

pub struct HerdrDeliverer {
    binary: String,
    bound: Duration,
}

impl HerdrDeliverer {
    pub fn new() -> Self {
        Self::with_binary(herdr_binary())
    }

    /// Points at an arbitrary program rather than reading `HERDR_BIN_PATH`
    /// from the environment. Production uses `new()`; a test uses this to
    /// pin the trait method, the argument builder it calls, and the process
    /// construction itself, against a small recorder script — without
    /// mutating an environment variable a parallel test suite shares.
    pub fn with_binary(binary: impl Into<String>) -> Self {
        HerdrDeliverer {
            binary: binary.into(),
            bound: BOUND,
        }
    }

    #[cfg(all(test, unix))]
    pub(crate) fn with_bound(mut self, bound: Duration) -> Self {
        self.bound = bound;
        self
    }

    fn run(&self, args: &[&str]) -> Result<(), DeliveryError> {
        let mut command = std::process::Command::new(&self.binary);
        command.args(args);
        match crate::outward::run(&mut command, self.bound) {
            Err(crate::outward::RunError::TimedOut) => Err(DeliveryError::TimedOut {
                binary: self.binary.clone(),
                bound: self.bound,
            }),
            // herdr starts plugin commands with a minimal PATH — the same
            // reasoning src/stt/command.rs:78-86 states for the transcriber.
            Err(crate::outward::RunError::Start(error)) => Err(start_failure(&self.binary, &error)),
            Ok(output) if output.status.success() => Ok(()),
            Ok(output) => {
                let text = if !output.stdout.is_empty() {
                    &output.stdout
                } else {
                    &output.stderr
                };
                Err(DeliveryError::Rejected(extract_reason(text)))
            }
        }
    }
}

impl Default for HerdrDeliverer {
    fn default() -> Self {
        Self::new()
    }
}

/// The argument list for `herdr pane send-text`, built as data so a test can
/// assert on it directly rather than only through a fake that never checks
/// what a real subcommand expects.
fn insert_args<'a>(pane: &'a str, text: &'a str) -> Vec<&'a str> {
    vec!["pane", "send-text", pane, text]
}

/// The argument list for `herdr agent prompt`.
fn submit_args<'a>(pane: &'a str, text: &'a str) -> Vec<&'a str> {
    vec!["agent", "prompt", pane, text]
}

/// The argument list for `herdr notification show ... --body ...`.
pub(crate) fn notify_args<'a>(title: &'a str, body: &'a str) -> Vec<&'a str> {
    vec!["notification", "show", title, "--body", body]
}

impl Deliverer for HerdrDeliverer {
    fn insert(&self, pane: &str, text: &str) -> Result<(), DeliveryError> {
        self.run(&insert_args(pane, text))
    }
    fn submit(&self, pane: &str, text: &str) -> Result<(), DeliveryError> {
        self.run(&submit_args(pane, text))
    }
    fn notify(&self, title: &str, body: &str) -> Result<(), DeliveryError> {
        self.run(&notify_args(title, body))
    }
}

#[cfg(test)]
mod tests {
    use super::tests_support::{Call, FakeDeliverer};
    use super::*;

    #[test]
    fn a_structured_rejection_yields_its_code() {
        let text = br#"{"error":{"code":"pane_not_found","message":"pane w99:p99 not found"}}"#;
        assert_eq!(extract_reason(text), "pane_not_found");
        let text =
            br#"{"error":{"code":"agent_not_found","message":"agent target w99:p99 not found"}}"#;
        assert_eq!(extract_reason(text), "agent_not_found");
    }

    #[test]
    fn text_that_is_not_the_structured_shape_is_kept_as_is() {
        assert_eq!(
            extract_reason(b"herdr: unknown flag --bogus\n"),
            "herdr: unknown flag --bogus"
        );
    }

    #[test]
    fn insert_args_call_herdr_pane_send_text() {
        assert_eq!(
            insert_args("w1:p2", "hello"),
            vec!["pane", "send-text", "w1:p2", "hello"]
        );
    }

    #[test]
    fn submit_args_call_herdr_agent_prompt() {
        assert_eq!(
            submit_args("w1:p2", "hello"),
            vec!["agent", "prompt", "w1:p2", "hello"]
        );
    }

    #[test]
    fn notify_args_call_herdr_notification_show_with_a_body_flag() {
        assert_eq!(
            notify_args("Delivery failed", "w1:p2: the text is in the plugin log"),
            vec![
                "notification",
                "show",
                "Delivery failed",
                "--body",
                "w1:p2: the text is in the plugin log"
            ]
        );
    }

    #[test]
    fn insert_only_never_calls_submit() {
        let fake = FakeDeliverer::ok();
        assert_eq!(
            deliver(&fake, false, Some("claude"), "w1:p2", "hello"),
            Ok(())
        );
        assert_eq!(
            fake.calls(),
            vec![Call::Insert("w1:p2".into(), "hello".into())]
        );
    }

    #[test]
    fn submit_calls_submit_when_an_agent_is_named() {
        let fake = FakeDeliverer::ok();
        assert_eq!(
            deliver(&fake, true, Some("claude"), "w1:p2", "hello"),
            Ok(())
        );
        assert_eq!(
            fake.calls(),
            vec![Call::Submit("w1:p2".into(), "hello".into())]
        );
    }

    #[test]
    fn submit_falls_back_to_insert_when_no_agent_is_named() {
        let fake = FakeDeliverer::ok();
        assert_eq!(deliver(&fake, true, None, "w1:p2", "hello"), Ok(()));
        assert_eq!(
            fake.calls(),
            vec![Call::Insert("w1:p2".into(), "hello".into())]
        );
    }

    #[test]
    fn submit_falls_back_to_insert_when_the_agent_is_the_empty_string() {
        // How herdr encodes "no agent" was never established: it may omit the
        // field (Option::None, already covered above) or send an empty one.
        // Defend against both, since Some("").is_some() alone would submit to
        // a pane the criterion requires only an insert into.
        let fake = FakeDeliverer::ok();
        assert_eq!(deliver(&fake, true, Some(""), "w1:p2", "hello"), Ok(()));
        assert_eq!(
            fake.calls(),
            vec![Call::Insert("w1:p2".into(), "hello".into())]
        );
    }

    #[test]
    fn a_rejected_call_is_propagated_unchanged() {
        let fake = FakeDeliverer::failing(DeliveryError::Rejected("pane_not_found".into()));
        let result = deliver(&fake, false, None, "w99:p99", "hello");
        assert_eq!(
            result,
            Err(DeliveryError::Rejected("pane_not_found".into()))
        );
    }

    /// A small program that writes the argv it was invoked with, one line
    /// each, to a file next to it — so a test can pin the whole chain: the
    /// trait method, the argument builder it calls, and the process
    /// construction that runs it, without mutating HERDR_BIN_PATH under a
    /// suite that runs in parallel. Unix needs the executable bit; Windows
    /// needs a `.cmd` (see the comment on its `recorder()` below for what
    /// actually makes that runnable).
    struct Recorder {
        dir: std::path::PathBuf,
        script: std::path::PathBuf,
        out: std::path::PathBuf,
    }

    /// Nothing removes the scratch directory otherwise: it is unique per
    /// process (the pid is in its name), so runs never collide, but a suite
    /// that leaves a directory behind on every invocation is one somebody
    /// eventually finds confusing.
    impl Drop for Recorder {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    impl Recorder {
        /// Creates the scratch directory and hands ownership of it to the
        /// `Recorder` returned, in the same step — nothing fallible runs in
        /// between. An earlier version created the directory in one
        /// function and built the owning `Recorder` only after the script
        /// was written and (on Unix) made executable; a panic in either of
        /// those — a full or read-only temp directory, say — left the
        /// directory on disk with no live `Drop` to remove it. Building
        /// only the paths here, and leaving `script`'s content and mode to
        /// the platform-specific `recorder()` below, keeps this
        /// constructor itself free of anything that can fail after the
        /// directory already exists.
        fn new(tag: &str, script_name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!(
                "herdr-voice-delivery-recorder-{tag}-{}",
                std::process::id()
            ));
            std::fs::create_dir_all(&dir).expect("scratch dir");
            let script = dir.join(script_name);
            let out = dir.join("record.out");
            Recorder { dir, script, out }
        }
    }

    #[cfg(unix)]
    fn recorder(tag: &str) -> Recorder {
        let recorder = Recorder::new(tag, "record.sh");
        std::fs::write(
            &recorder.script,
            format!("#!/bin/sh\nprintf '%s\\n' \"$@\" > {:?}\n", recorder.out),
        )
        .expect("write recorder script");
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&recorder.script)
            .expect("stat")
            .permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&recorder.script, perms).expect("chmod");
        recorder
    }

    // Not run on this machine — verified by reasoning about cmd.exe's
    // argument handling, not by executing it: `for %%A in (%*) do echo
    // %%~A` iterates the arguments Command::args passed, one per token,
    // with %%~A stripping the quotes cmd added around any that contain a
    // space (both "Delivery failed" and the notify body do).
    //
    // `CreateProcess` itself cannot run a `.cmd` at all — it only starts PE
    // binaries. It is Rust's standard library that special-cases `.bat` and
    // `.cmd`, routing the command through `cmd.exe` and quoting the
    // arguments for it, a behavior that gained its current escaping after a
    // security advisory in an earlier release. Relying on `Command::new(a
    // .cmd path).args(...)` delivering arguments correctly is safe here
    // because this crate's rust-version (Cargo.toml) is at least that fixed
    // release, not because of anything CreateProcess does on its own.
    //
    // The loop's %%~A quoting is good enough for what these three tests
    // pass and no further: an argument carrying a percent sign, an embedded
    // quote, a comma or a semicolon would very likely come out split or
    // mangled. A future test adding such an argument should know that
    // before spending an hour on it.
    #[cfg(windows)]
    fn recorder(tag: &str) -> Recorder {
        let recorder = Recorder::new(tag, "record.cmd");
        // recorder.out.display() rather than the {:?} Debug form used on
        // Unix above: Debug escapes '\' to '\\', and while Windows path
        // resolution tolerates doubled separators in practice, display()
        // sidesteps the question entirely by writing the path's native
        // text unescaped, quoted by hand instead of relying on Debug's
        // quoting.
        std::fs::write(
            &recorder.script,
            format!(
                "@echo off\r\n(for %%A in (%*) do echo %%~A) > \"{}\"\r\n",
                recorder.out.display()
            ),
        )
        .expect("write recorder script");
        recorder
    }

    fn recorded_args(out: &std::path::Path) -> Vec<String> {
        std::fs::read_to_string(out)
            .expect("the recorder must have run and written its argv")
            .lines()
            .map(|l| l.to_string())
            .collect()
    }

    #[test]
    fn the_recorder_cleans_up_its_scratch_directory_when_dropped() {
        let recorder = recorder("cleanup");
        let dir = recorder.dir.clone();
        assert!(
            dir.exists(),
            "the recorder must have created its scratch directory"
        );
        drop(recorder);
        assert!(
            !dir.exists(),
            "the scratch directory must be gone once the recorder is dropped, got it still at {dir:?}"
        );
    }

    #[test]
    fn insert_runs_pane_send_text_not_agent_prompt() {
        let recorder = recorder("insert");
        let deliverer = HerdrDeliverer::with_binary(recorder.script.to_string_lossy().into_owned());
        deliverer
            .insert("w1:p2", "hello")
            .expect("the recorder always succeeds");
        assert_eq!(
            recorded_args(&recorder.out),
            vec!["pane", "send-text", "w1:p2", "hello"]
        );
    }

    #[test]
    fn submit_runs_agent_prompt_not_pane_send_text() {
        let recorder = recorder("submit");
        let deliverer = HerdrDeliverer::with_binary(recorder.script.to_string_lossy().into_owned());
        deliverer
            .submit("w1:p2", "hello")
            .expect("the recorder always succeeds");
        assert_eq!(
            recorded_args(&recorder.out),
            vec!["agent", "prompt", "w1:p2", "hello"]
        );
    }

    #[test]
    fn notify_runs_notification_show_with_a_body_flag() {
        let recorder = recorder("notify");
        let deliverer = HerdrDeliverer::with_binary(recorder.script.to_string_lossy().into_owned());
        deliverer
            .notify("Delivery failed", "w1:p2: the text is in the plugin log")
            .expect("the recorder always succeeds");
        assert_eq!(
            recorded_args(&recorder.out),
            vec![
                "notification",
                "show",
                "Delivery failed",
                "--body",
                "w1:p2: the text is in the plugin log"
            ]
        );
    }

    #[test]
    fn a_program_that_does_not_exist_keeps_the_path_sentence() {
        let deliverer = HerdrDeliverer::with_binary("herdr-voice-no-such-program");
        let error = deliverer.insert("w1:p2", "hello").expect_err("must fail");
        assert!(
            matches!(error, DeliveryError::NotFound { .. }),
            "got {error:?}"
        );
        let message = error.to_string();
        assert!(
            message.contains("herdr-voice-no-such-program"),
            "got {message}"
        );
        assert!(message.contains("not on the PATH"), "got {message}");
        assert!(message.contains("HERDR_BIN_PATH"), "got {message}");
        // The PATH that was searched is named, not an empty one.
        assert!(
            message.contains(&format!("{:?}", std::env::var("PATH").unwrap_or_default())),
            "got {message}"
        );
    }

    #[test]
    fn start_failure_reads_the_kind_the_operating_system_gave() {
        use std::io::{Error, ErrorKind};
        assert!(matches!(
            start_failure("herdr", &Error::from(ErrorKind::NotFound)),
            DeliveryError::NotFound { .. }
        ));
        assert!(matches!(
            start_failure("herdr", &Error::from(ErrorKind::PermissionDenied)),
            DeliveryError::NotExecutable { .. }
        ));
        assert!(matches!(
            start_failure("herdr", &Error::other("Text file busy")),
            DeliveryError::StartFailed { .. }
        ));
    }

    #[test]
    fn another_failure_to_start_carries_the_operating_systems_text_and_not_the_path_sentence() {
        let error = start_failure(
            "/opt/herdr",
            &std::io::Error::other("Text file busy (os error 26)"),
        );
        assert_eq!(
            error,
            DeliveryError::StartFailed {
                binary: "/opt/herdr".to_string(),
                reason: "Text file busy (os error 26)".to_string(),
            }
        );
        let message = error.to_string();
        assert!(message.contains("/opt/herdr"), "got {message}");
        assert!(
            message.contains("Text file busy (os error 26)"),
            "got {message}"
        );
        assert!(message.contains("try again"), "got {message}");
        assert!(!message.contains("PATH"), "got {message}");
    }

    #[test]
    fn a_found_program_that_cannot_be_run_says_so_and_not_the_path_sentence() {
        let message = DeliveryError::NotExecutable {
            binary: "/opt/herdr".to_string(),
        }
        .to_string();
        assert!(message.contains("/opt/herdr"), "got {message}");
        assert!(message.contains("was found"), "got {message}");
        assert!(message.contains("chmod +x"), "got {message}");
        assert!(
            message.contains("point HERDR_BIN_PATH at the herdr program itself"),
            "got {message}"
        );
        assert!(!message.contains("not on the PATH"), "got {message}");
    }

    #[cfg(unix)]
    #[test]
    fn a_file_without_the_execute_bit_is_not_executable() {
        let recorder = Recorder::new("not-executable", "herdr-without-x-bit");
        // Written and left with the mode the umask gives it: no execute bit.
        std::fs::write(&recorder.script, "#!/bin/sh\nexit 0\n").expect("write");
        let binary = recorder.script.to_string_lossy().into_owned();
        let deliverer = HerdrDeliverer::with_binary(binary.clone());
        let error = deliverer.insert("w1:p2", "hello").expect_err("must fail");
        assert_eq!(
            error,
            DeliveryError::NotExecutable { binary },
            "got {error:?}"
        );
        assert!(
            !error.to_string().contains("not on the PATH"),
            "got {error}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_directory_named_as_the_program_is_not_executable() {
        let recorder = Recorder::new("directory", "unused");
        let binary = recorder.dir.to_string_lossy().into_owned();
        let deliverer = HerdrDeliverer::with_binary(binary.clone());
        let error = deliverer.insert("w1:p2", "hello").expect_err("must fail");
        assert_eq!(
            error,
            DeliveryError::NotExecutable { binary },
            "got {error:?}"
        );
        assert!(
            error
                .to_string()
                .contains("point HERDR_BIN_PATH at the herdr program itself"),
            "got {error}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_program_that_starts_and_fails_is_still_a_rejection() {
        let recorder = Recorder::new("rejects", "herdr-rejecting");
        std::fs::write(
            &recorder.script,
            "#!/bin/sh\necho '{\"error\":{\"code\":\"pane_not_found\",\"message\":\"gone\"}}'\nexit 1\n",
        )
        .expect("write");
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&recorder.script)
            .expect("stat")
            .permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&recorder.script, perms).expect("chmod");
        let deliverer = HerdrDeliverer::with_binary(recorder.script.to_string_lossy().into_owned());
        assert_eq!(
            deliverer.insert("w1:p2", "hello"),
            Err(DeliveryError::Rejected("pane_not_found".to_string()))
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_herdr_that_never_answers_is_stopped_and_named() {
        let binary = tests_support::herdr_that_hangs_on("hang-insert", &["pane"]);
        let deliverer =
            HerdrDeliverer::with_binary(binary.clone()).with_bound(Duration::from_millis(200));
        let started = std::time::Instant::now();
        let error = deliverer.insert("w1:p2", "hello").expect_err("must fail");
        assert!(started.elapsed() < Duration::from_secs(3));
        assert_eq!(
            error,
            DeliveryError::TimedOut {
                binary: binary.clone(),
                bound: Duration::from_millis(200)
            }
        );
        let message = error.to_string();
        assert!(message.contains(&binary), "names herdr: {message}");
        assert!(
            message.contains("200 milliseconds"),
            "names the bound: {message}"
        );
        assert!(
            message.contains("restart"),
            "says what to do next: {message}"
        );
        assert!(
            message.contains("look there first"),
            "the text may have landed, so it says to look before dictating again: {message}"
        );
        assert!(!message.contains("daemon"), "{message}");
    }

    #[cfg(unix)]
    #[test]
    fn each_of_the_three_subcommands_is_bounded() {
        let binary = tests_support::herdr_that_hangs_on("hang-all", &["*"]);
        let deliverer = HerdrDeliverer::with_binary(binary).with_bound(Duration::from_millis(200));
        for result in [
            deliverer.insert("w1:p2", "hello"),
            deliverer.submit("w1:p2", "hello"),
            deliverer.notify("title", "body"),
        ] {
            assert!(
                matches!(result, Err(DeliveryError::TimedOut { .. })),
                "{result:?}"
            );
        }
    }

    #[test]
    fn the_bound_is_ten_seconds() {
        assert_eq!(BOUND, Duration::from_secs(10));
        assert_eq!(HerdrDeliverer::with_binary("herdr").bound, BOUND);
    }
}
