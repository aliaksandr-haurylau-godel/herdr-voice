//! When a held key counts as released.
//!
//! Holding a key starts the client about twelve times a second, and each start
//! reaches the daemon as a request. The hold lives here, in memory: there is no
//! file to write and therefore no truncated read to defend against, which is
//! what the shell prototype lost recordings to.
//!
//! The decision is a pure function so that a one-second gap costs nothing to
//! test. The thread that calls it does nothing but wait and act.

/// Monotonic milliseconds since the clock's origin.
pub type Stamp = u64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hold {
    /// The pane pinned when the hold began, kept until delivery.
    pub target: String,
    pub cwd: Option<String>,
    pub agent: Option<String>,
    pub began: Stamp,
    pub last_poke: Stamp,
    /// How many repeats arrived. For the journal only.
    pub pokes: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    pub release_ms: u64,
    pub min_hold_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    /// No hold. Wait until one begins.
    Idle,
    /// The key is still down as far as anything here can tell.
    KeepWaiting { until: Stamp },
    /// The gap passed: end the take and deliver it.
    Release { held_ms: u64 },
    /// The gap passed and the hold was too short to be one.
    TooShort { held_ms: u64 },
}

/// What to do about the hold, if any, at `now`.
///
/// `held_ms` is saturating on purpose. A monotonic clock cannot put `last_poke`
/// before `began`, so a negative duration is not reachable here — but the
/// prototype discarded 17 live takes on exactly that arithmetic, and a take is
/// never discarded for a duration this function could not compute. An
/// impossible span releases with `held_ms` of zero and is reported, not thrown
/// away.
pub fn decide(now: Stamp, hold: Option<&Hold>, settings: &Settings) -> Decision {
    let Some(hold) = hold else {
        return Decision::Idle;
    };
    let deadline = hold.last_poke.saturating_add(settings.release_ms);
    if now < deadline {
        return Decision::KeepWaiting { until: deadline };
    }
    let held_ms = hold.last_poke.saturating_sub(hold.began);
    // A span that could not be computed is not evidence of a tap. The clock is
    // monotonic, so this is unreachable in the daemon; it is written because
    // the prototype threw away live takes when it was not.
    if hold.last_poke < hold.began {
        return Decision::Release { held_ms: 0 };
    }
    if held_ms < settings.min_hold_ms {
        Decision::TooShort { held_ms }
    } else {
        Decision::Release { held_ms }
    }
}

/// Time, and waiting for it, behind an interface.
///
/// The gap is a second and the minimum hold 300 ms. A suite that waited those
/// out would pay for them on every run, and a clock that cannot be moved
/// backwards cannot exercise the guard in `decide`.
///
/// **The contract, which both implementations must satisfy exactly.**
/// `wait_until` returns when `now()` has reached `deadline` **or** when `wake`
/// has been called since the last return, whichever happens first. A `wake` is
/// consumed by the return it causes, so two waits are not freed by one call.
/// A waiter that can only be freed by time is not enough: the watcher waits an
/// hour when there is no hold, and what ends that wait is a hold beginning.
pub trait Clock: Send + Sync {
    fn now(&self) -> Stamp;
    fn wait_until(&self, deadline: Stamp);
    fn wake(&self);
}

pub struct SystemClock {
    origin: std::time::Instant,
    woken: std::sync::Mutex<bool>,
    bell: std::sync::Condvar,
}

impl Default for SystemClock {
    fn default() -> Self {
        SystemClock {
            origin: std::time::Instant::now(),
            woken: std::sync::Mutex::new(false),
            bell: std::sync::Condvar::new(),
        }
    }
}

impl Clock for SystemClock {
    fn now(&self) -> Stamp {
        // `as u64` is safe for any run shorter than 584 million years.
        self.origin.elapsed().as_millis() as u64
    }

    fn wait_until(&self, deadline: Stamp) {
        let Ok(mut woken) = self.woken.lock() else {
            // A poisoned lock must not become a busy loop in the daemon.
            std::thread::sleep(std::time::Duration::from_millis(50));
            return;
        };
        loop {
            if *woken {
                *woken = false;
                return;
            }
            let remaining = deadline.saturating_sub(self.now());
            if remaining == 0 {
                return;
            }
            let waited = self
                .bell
                .wait_timeout(woken, std::time::Duration::from_millis(remaining));
            let Ok((guard, _)) = waited else { return };
            woken = guard;
        }
    }

    fn wake(&self) {
        if let Ok(mut woken) = self.woken.lock() {
            *woken = true;
        }
        self.bell.notify_all();
    }
}

#[cfg(test)]
pub mod tests_support {
    use super::*;
    use std::sync::{Condvar, Mutex};

    #[derive(Default)]
    struct State {
        now: Stamp,
        woken: bool,
    }

    /// A clock the test moves by hand. `wait_until` returns as soon as the
    /// test's `advance` puts `now` at or past the deadline, or as soon as
    /// `wake` is called — so a one-second gap costs nothing and an hour-long
    /// idle wait is not a hang.
    #[derive(Default)]
    pub struct TestClock {
        state: Mutex<State>,
        bell: Condvar,
    }

    impl TestClock {
        pub fn advance(&self, by: u64) {
            if let Ok(mut state) = self.state.lock() {
                state.now = state.now.saturating_add(by);
            }
            self.bell.notify_all();
        }
    }

    impl Clock for TestClock {
        fn now(&self) -> Stamp {
            self.state.lock().map(|state| state.now).unwrap_or(0)
        }

        fn wait_until(&self, deadline: Stamp) {
            let Ok(mut state) = self.state.lock() else {
                return;
            };
            loop {
                if state.woken {
                    state.woken = false;
                    return;
                }
                if state.now >= deadline {
                    return;
                }
                let Ok(next) = self.bell.wait(state) else {
                    return;
                };
                state = next;
            }
        }

        fn wake(&self) {
            if let Ok(mut state) = self.state.lock() {
                state.woken = true;
            }
            self.bell.notify_all();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings() -> Settings {
        Settings {
            release_ms: 1000,
            min_hold_ms: 300,
        }
    }

    fn hold_at(began: Stamp, last_poke: Stamp) -> Hold {
        Hold {
            target: "w1:p1".to_string(),
            cwd: None,
            agent: None,
            began,
            last_poke,
            pokes: 2,
        }
    }

    #[test]
    fn with_no_hold_there_is_nothing_to_decide() {
        assert_eq!(decide(500, None, &settings()), Decision::Idle);
    }

    #[test]
    fn a_gap_shorter_than_the_release_keeps_the_hold() {
        // Last poke at 1000, now 1600: 600 ms of silence, under the 1000 ms gap.
        let hold = hold_at(0, 1000);
        assert_eq!(
            decide(1600, Some(&hold), &settings()),
            Decision::KeepWaiting { until: 2000 }
        );
    }

    #[test]
    fn the_deadline_releases_the_hold() {
        let hold = hold_at(0, 1000);
        assert_eq!(
            decide(2000, Some(&hold), &settings()),
            Decision::Release { held_ms: 1000 }
        );
    }

    #[test]
    fn a_hold_shorter_than_the_minimum_is_a_tap() {
        // Held 120 ms, then released.
        let hold = hold_at(0, 120);
        assert_eq!(
            decide(1120, Some(&hold), &settings()),
            Decision::TooShort { held_ms: 120 }
        );
    }

    #[test]
    fn a_clock_that_went_backwards_never_discards_a_take() {
        // last_poke precedes began: impossible from a monotonic clock, and the
        // prototype discarded a live take on exactly this arithmetic.
        let hold = hold_at(5_000, 1_000);
        match decide(9_000, Some(&hold), &settings()) {
            Decision::Release { held_ms } => assert_eq!(held_ms, 0),
            other => panic!("an impossible duration must still release: {other:?}"),
        }
    }

    #[test]
    fn a_wake_frees_a_waiter_the_deadline_never_would() {
        use std::sync::Arc;
        let clock = Arc::new(tests_support::TestClock::default());
        let waiter = {
            let clock = Arc::clone(&clock);
            // An hour away: only `wake` can end this.
            std::thread::spawn(move || clock.wait_until(3_600_000))
        };
        // Give the waiter time to be inside `wait_until`, then free it.
        std::thread::sleep(std::time::Duration::from_millis(20));
        clock.wake();
        waiter
            .join()
            .expect("a wake must free a waiter the clock never will");
    }

    #[test]
    fn the_same_holds_for_the_real_clock() {
        use std::sync::Arc;
        let clock = Arc::new(SystemClock::default());
        let waiter = {
            let clock = Arc::clone(&clock);
            std::thread::spawn(move || clock.wait_until(3_600_000))
        };
        std::thread::sleep(std::time::Duration::from_millis(20));
        clock.wake();
        waiter
            .join()
            .expect("the contract is the trait's, not one implementation's");
    }
}
