//! Taking a recording: the thread that owns the device, and the take it produces.
//!
//! A take spans two `dictate` invocations, which arrive on different connections
//! and different threads, so the stream cannot live in a connection handler. The
//! recorder owns one thread for the daemon's whole life and the stream is built,
//! held and dropped there and nowhere else. That also makes the question of
//! whether a `cpal` stream may cross threads irrelevant. See
//! `tasks/8/DESIGN_8.md`, section 2.

pub mod cpal_source;

use std::fmt;
use std::path::PathBuf;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::audio::{level, resample, wav};
use crate::config::Audio;

/// What a device delivers while a take is running.
#[derive(Debug, Clone)]
pub enum Event {
    Samples(Vec<f32>),
    Failed(String),
}

/// What the device turned out to be.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Format {
    pub rate: u32,
    pub channels: u16,
}

/// Where a source puts what it hears. Shared, because a device delivers from its
/// own thread and the recorder reads when the take ends.
#[derive(Clone, Default)]
pub struct Sink {
    samples: Arc<Mutex<Vec<f32>>>,
    failure: Arc<Mutex<Option<String>>>,
}

impl Sink {
    pub fn push(&self, event: Event) {
        match event {
            Event::Samples(more) => {
                if let Ok(mut samples) = self.samples.lock() {
                    samples.extend_from_slice(&more);
                }
            }
            Event::Failed(why) => {
                if let Ok(mut failure) = self.failure.lock() {
                    failure.get_or_insert(why);
                }
            }
        }
    }

    fn take_samples(&self) -> Vec<f32> {
        self.samples
            .lock()
            .map(|mut samples| std::mem::take(&mut *samples))
            .unwrap_or_default()
    }

    fn failure(&self) -> Option<String> {
        self.failure.lock().ok().and_then(|f| f.clone())
    }
}

/// A device, or something standing in for one.
pub trait Source {
    /// Open the input and start delivering into the sink.
    fn start(&mut self, device: Option<&str>, sink: Sink) -> Result<Format, String>;
    /// Stop delivering and release the device.
    fn stop(&mut self);
}

/// What `start` did.
#[derive(Debug, PartialEq, Eq)]
pub enum Started {
    /// A take began.
    Began,
    /// One was already running: this is the toggle's second half.
    AlreadyRunning,
    /// The previous take died with its device. The reason is reported here and
    /// forgotten; nothing was started.
    PreviousFailure(String),
    /// The device could not be opened at all.
    CouldNotStart(String),
}

/// A finished take.
#[derive(Debug, Clone, PartialEq)]
pub struct Take {
    pub path: PathBuf,
    pub level_dbfs: f32,
    /// The pane this take was started for. Pinned when it began, so that switching
    /// focus while speaking cannot change where the text lands.
    pub target: String,
}

#[derive(Debug)]
pub enum CaptureError {
    NothingRunning,
    DeviceLost {
        device: String,
        why: String,
    },
    TooQuiet {
        device: String,
        level_dbfs: f32,
        threshold_dbfs: f32,
    },
    Unusable(String),
}

impl fmt::Display for CaptureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CaptureError::NothingRunning => write!(
                f,
                "no recording is running; press the dictation key to start one"
            ),
            CaptureError::DeviceLost { device, why } => write!(
                f,
                "the input {device:?} stopped during the recording ({why}); \
                 the take was discarded. Reconnect it, or set [audio] input to another"
            ),
            CaptureError::TooQuiet {
                device,
                level_dbfs,
                threshold_dbfs,
            } => write!(
                f,
                "the take from {device:?} measured {level_dbfs:.1} dB, below the \
                 {threshold_dbfs:.1} dB floor, so it was discarded as the wrong input. \
                 Check that {device:?} is the microphone you speak into and is not muted"
            ),
            CaptureError::Unusable(why) => write!(f, "{why}"),
        }
    }
}

impl std::error::Error for CaptureError {}

enum Command {
    Start {
        target: String,
        device: Option<String>,
        reply: mpsc::Sender<Started>,
    },
    Stop {
        reply: mpsc::Sender<Result<Take, CaptureError>>,
    },
}

/// The handle the daemon holds. The thread behind it lives as long as the daemon.
pub struct Recorder {
    // A `Sender` is `Send` but not `Sync`, and the daemon shares one recorder
    // between connection threads.
    commands: Mutex<mpsc::Sender<Command>>,
}

struct Running {
    target: String,
    device: String,
    format: Format,
    sink: Sink,
    path: PathBuf,
}

impl Recorder {
    /// Start the recorder's thread. The source is built on that thread, so
    /// anything it holds stays there.
    pub fn spawn<F>(make_source: F, audio: Audio, takes: PathBuf) -> Recorder
    where
        F: FnOnce() -> Box<dyn Source> + Send + 'static,
    {
        let (commands, orders) = mpsc::channel();
        std::thread::spawn(move || {
            let mut source = make_source();
            let mut running: Option<Running> = None;
            let mut remembered: Option<String> = None;
            let mut counter: u64 = 0;

            while let Ok(order) = orders.recv() {
                match order {
                    Command::Start {
                        target,
                        device,
                        reply,
                    } => {
                        let answer = start_one(
                            source.as_mut(),
                            &takes,
                            &mut running,
                            &mut remembered,
                            &mut counter,
                            target,
                            device,
                        );
                        let _ = reply.send(answer);
                    }
                    Command::Stop { reply } => {
                        let answer =
                            stop_one(source.as_mut(), &audio, &mut running, &mut remembered);
                        let _ = reply.send(answer);
                    }
                }
            }
        });
        Recorder {
            commands: Mutex::new(commands),
        }
    }

    pub fn start(&self, target: &str, device: Option<&str>) -> Started {
        let (reply, answer) = mpsc::channel();
        let sent = self.commands.lock().map(|commands| {
            commands.send(Command::Start {
                target: target.to_string(),
                device: device.map(str::to_string),
                reply,
            })
        });
        if !matches!(sent, Ok(Ok(()))) {
            return Started::CouldNotStart("the recorder thread is gone".to_string());
        }
        answer
            .recv()
            .unwrap_or_else(|_| Started::CouldNotStart("the recorder thread is gone".to_string()))
    }

    pub fn stop(&self) -> Result<Take, CaptureError> {
        let (reply, answer) = mpsc::channel();
        let sent = self
            .commands
            .lock()
            .map(|commands| commands.send(Command::Stop { reply }));
        if !matches!(sent, Ok(Ok(()))) {
            return Err(CaptureError::Unusable(
                "the recorder thread is gone".to_string(),
            ));
        }
        answer
            .recv()
            .unwrap_or_else(|_| Err(CaptureError::Unusable("the recorder thread is gone".into())))
    }
}

#[allow(clippy::too_many_arguments)]
fn start_one(
    source: &mut dyn Source,
    takes: &std::path::Path,
    running: &mut Option<Running>,
    remembered: &mut Option<String>,
    counter: &mut u64,
    target: String,
    device: Option<String>,
) -> Started {
    // A device that died while nobody was asking is reported here, at the first
    // moment there is anywhere to report to.
    if let Some(running_now) = running.as_ref() {
        if let Some(why) = running_now.sink.failure() {
            let device = running_now.device.clone();
            source.stop();
            discard(running.take());
            *remembered = None;
            return Started::PreviousFailure(CaptureError::DeviceLost { device, why }.to_string());
        }
    }
    if let Some(why) = remembered.take() {
        return Started::PreviousFailure(why);
    }
    if running.is_some() {
        return Started::AlreadyRunning;
    }

    let sink = Sink::default();
    let format = match source.start(device.as_deref(), sink.clone()) {
        Ok(format) => format,
        Err(why) => return Started::CouldNotStart(why),
    };
    if resample::ratio_for(format.rate).is_none() {
        source.stop();
        return Started::CouldNotStart(
            resample::ResampleError::UnsupportedRate(format.rate).to_string(),
        );
    }

    *counter += 1;
    let path = take_path(takes, *counter);
    *running = Some(Running {
        target,
        device: device.unwrap_or_else(|| "the default input".to_string()),
        format,
        sink,
        path,
    });
    Started::Began
}

fn stop_one(
    source: &mut dyn Source,
    audio: &Audio,
    running: &mut Option<Running>,
    remembered: &mut Option<String>,
) -> Result<Take, CaptureError> {
    if let Some(why) = remembered.take() {
        return Err(CaptureError::Unusable(why));
    }
    let Some(take) = running.take() else {
        return Err(CaptureError::NothingRunning);
    };
    source.stop();

    if let Some(why) = take.sink.failure() {
        let device = take.device.clone();
        discard(Some(take));
        return Err(CaptureError::DeviceLost { device, why });
    }

    let samples = take.sink.take_samples();
    let mono = resample::to_mono(&samples, take.format.channels);
    let converted = resample::to_16k(&mono, take.format.rate)
        .map_err(|e| CaptureError::Unusable(e.to_string()))?;

    let level_dbfs = level::mean_dbfs(&converted);
    if level_dbfs < audio.silence_db {
        let device = take.device.clone();
        discard(Some(take));
        return Err(CaptureError::TooQuiet {
            device,
            level_dbfs,
            threshold_dbfs: audio.silence_db,
        });
    }

    wav::write(&take.path, &converted, resample::TARGET_RATE)
        .map_err(|e| CaptureError::Unusable(format!("cannot write the take: {e}")))?;
    Ok(Take {
        path: take.path,
        level_dbfs,
        target: take.target,
    })
}

/// A take nobody will read is a take nobody should find later.
fn discard(take: Option<Running>) {
    if let Some(take) = take {
        let _ = std::fs::remove_file(&take.path);
    }
}

fn take_path(takes: &std::path::Path, counter: u64) -> PathBuf {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or_default();
    takes.join(format!("{millis}-{}-{counter}.wav", std::process::id()))
}

/// Fakes the daemon's tests borrow, so a dispatch test needs no microphone.
#[cfg(test)]
pub mod tests_support {
    use super::{Event, Format, Sink, Source};

    /// Hears a moment of digital silence and nothing else.
    pub struct SilentSource;

    impl Source for SilentSource {
        fn start(&mut self, _device: Option<&str>, sink: Sink) -> Result<Format, String> {
            sink.push(Event::Samples(vec![0.0; 4_800]));
            Ok(Format {
                rate: 48_000,
                channels: 1,
            })
        }

        fn stop(&mut self) {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A device that never existed: it plays a script and stops.
    struct Fake {
        format: Format,
        script: Vec<Event>,
        started: usize,
        stopped: usize,
        refuse: Option<String>,
    }

    impl Fake {
        fn new(script: Vec<Event>) -> Fake {
            Fake {
                format: Format {
                    rate: 48_000,
                    channels: 1,
                },
                script,
                started: 0,
                stopped: 0,
                refuse: None,
            }
        }
    }

    impl Source for Fake {
        fn start(&mut self, _device: Option<&str>, sink: Sink) -> Result<Format, String> {
            if let Some(why) = &self.refuse {
                return Err(why.clone());
            }
            self.started += 1;
            for event in &self.script {
                sink.push(event.clone());
            }
            Ok(self.format)
        }

        fn stop(&mut self) {
            self.stopped += 1;
        }
    }

    fn tone(amplitude: f32, seconds: f32) -> Vec<f32> {
        let count = (48_000.0 * seconds) as usize;
        (0..count)
            .map(|i| amplitude * (i as f32 * std::f32::consts::TAU * 440.0 / 48_000.0).sin())
            .collect()
    }

    fn takes_dir(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!("herdr-voice-takes-{tag}-{}", std::process::id()))
    }

    fn recorder_with(tag: &str, script: Vec<Event>) -> (Recorder, PathBuf) {
        let takes = takes_dir(tag);
        let recorder = Recorder::spawn(
            move || Box::new(Fake::new(script)),
            Audio::default(),
            takes.clone(),
        );
        (recorder, takes)
    }

    #[test]
    fn a_take_starts_stops_and_lands_on_disk() {
        let (recorder, _) = recorder_with("ok", vec![Event::Samples(tone(0.3, 1.0))]);
        assert_eq!(recorder.start("w1:p2", None), Started::Began);
        let take = recorder.stop().expect("a take");
        assert!(take.path.exists(), "the file must be where the take says");
        assert_eq!(take.target, "w1:p2", "the pane is pinned to the take");
        assert!(
            take.level_dbfs > -60.0,
            "a tone at 0.3 is not silence, got {}",
            take.level_dbfs
        );
        let written = std::fs::read(&take.path).expect("read");
        assert_eq!(&written[0..4], b"RIFF");
        // One second at 48 kHz becomes one second at 16 kHz.
        assert_eq!(
            u32::from_le_bytes(written[40..44].try_into().unwrap()),
            32_000
        );
        std::fs::remove_file(&take.path).ok();
    }

    #[test]
    fn starting_twice_is_the_toggles_second_half() {
        let (recorder, _) = recorder_with("twice", vec![Event::Samples(tone(0.3, 0.1))]);
        assert_eq!(recorder.start("w1:p2", None), Started::Began);
        assert_eq!(recorder.start("w1:p2", None), Started::AlreadyRunning);
        let take = recorder.stop().expect("a take");
        std::fs::remove_file(&take.path).ok();
    }

    #[test]
    fn stopping_with_nothing_running_says_so() {
        let (recorder, _) = recorder_with("nothing", vec![]);
        match recorder.stop() {
            Err(CaptureError::NothingRunning) => {}
            other => panic!("expected NothingRunning, got {other:?}"),
        }
    }

    #[test]
    fn a_device_that_goes_away_loses_the_take_and_says_why_once() {
        let (recorder, _) = recorder_with(
            "lost",
            vec![
                Event::Samples(tone(0.3, 0.2)),
                Event::Failed("the device was unplugged".to_string()),
            ],
        );
        assert_eq!(recorder.start("w1:p2", None), Started::Began);

        let error = recorder.stop().expect_err("the take is gone");
        let message = error.to_string();
        assert!(message.contains("unplugged"), "got {message}");
        assert!(message.contains("discarded"), "got {message}");

        // And the next start is a fresh one: the failure was reported at the stop.
        assert_eq!(recorder.start("w1:p2", None), Started::Began);
    }

    #[test]
    fn a_take_that_captured_nothing_is_refused_by_level_and_device() {
        let (recorder, _) = recorder_with("quiet", vec![Event::Samples(tone(0.00002, 1.0))]);
        assert_eq!(recorder.start("w1:p2", Some("Headset")), Started::Began);
        let error = recorder.stop().expect_err("too quiet");
        let message = error.to_string();
        assert!(
            message.contains("Headset"),
            "the device must be named, got {message}"
        );
        assert!(
            message.contains("-60.0 dB"),
            "the floor must be named, got {message}"
        );
        assert!(
            message.contains("muted"),
            "it must say what to check, got {message}"
        );
    }

    #[test]
    fn a_refused_take_leaves_no_file_behind() {
        let (recorder, takes) = recorder_with("cleanup", vec![Event::Samples(tone(0.00002, 0.5))]);
        assert_eq!(recorder.start("w1:p2", None), Started::Began);
        recorder.stop().expect_err("too quiet");
        let left = std::fs::read_dir(&takes)
            .map(|entries| entries.count())
            .unwrap_or(0);
        assert_eq!(left, 0, "a discarded take must not be left on disk");
    }

    #[test]
    fn two_takes_never_share_a_path() {
        let (recorder, _) = recorder_with("unique", vec![Event::Samples(tone(0.3, 0.1))]);
        recorder.start("w1:p2", None);
        let first = recorder.stop().expect("first");
        recorder.start("w1:p2", None);
        let second = recorder.stop().expect("second");
        assert_ne!(first.path, second.path);
        std::fs::remove_file(&first.path).ok();
        std::fs::remove_file(&second.path).ok();
    }

    #[test]
    fn a_device_that_will_not_open_says_so_rather_than_pretending() {
        let takes = takes_dir("refuse");
        let recorder = Recorder::spawn(
            move || {
                let mut fake = Fake::new(vec![]);
                fake.refuse = Some("no such device".to_string());
                Box::new(fake)
            },
            Audio::default(),
            takes,
        );
        match recorder.start("w1:p2", Some("Studio")) {
            Started::CouldNotStart(why) => assert!(why.contains("no such device"), "got {why}"),
            other => panic!("expected CouldNotStart, got {other:?}"),
        }
    }
}
