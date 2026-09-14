//! A rendezvous for tests, so that a window in the daemon can be driven rather
//! than slept through.
//!
//! Two of this plugin's defects lived in windows of time: the hundreds of
//! milliseconds a real input device takes to open, and the seconds a take
//! spends in recognition and delivery. A test that reproduces either by
//! sleeping is a test that passes on a fast machine and fails on a loaded one.
//!
//! A gate turns the window into two events the test controls. The code under
//! test calls `enter`, which announces that it got there and then blocks; the
//! test calls `wait_until_entered` to know the window is open, does whatever it
//! wants to prove, and calls `open` to let the code finish. Nothing sleeps, and
//! the order is the same on every machine.
//!
//! Every wait is bounded. A gate nobody opens makes its test fail in seconds
//! rather than hang until the job's own cap kills the whole run.

use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};

/// How long either side waits before giving up and letting its caller fail.
const BOUND: Duration = Duration::from_secs(10);

#[derive(Default)]
struct State {
    entered: bool,
    open: bool,
}

#[derive(Default)]
pub struct Gate {
    state: Mutex<State>,
    bell: Condvar,
}

impl Gate {
    /// Called from the code under test: announce arrival, then wait to be let
    /// through.
    pub fn enter(&self) {
        let Ok(mut state) = self.state.lock() else {
            return;
        };
        state.entered = true;
        self.bell.notify_all();
        let deadline = Instant::now() + BOUND;
        while !state.open {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return;
            }
            let Ok((next, _)) = self.bell.wait_timeout(state, left) else {
                return;
            };
            state = next;
        }
    }

    /// Called from the test: block until the code under test is inside the
    /// window.
    pub fn wait_until_entered(&self) {
        let Ok(mut state) = self.state.lock() else {
            return;
        };
        let deadline = Instant::now() + BOUND;
        while !state.entered {
            let left = deadline.saturating_duration_since(Instant::now());
            assert!(
                !left.is_zero(),
                "nothing reached the gate within {BOUND:?}; the window this \
                 test is about never opened"
            );
            let Ok((next, _)) = self.bell.wait_timeout(state, left) else {
                return;
            };
            state = next;
        }
    }

    /// Called from the test: let the code under test finish.
    pub fn open(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.open = true;
        }
        self.bell.notify_all();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn the_test_side_learns_of_the_arrival_and_the_other_side_waits_to_be_let_go() {
        let gate = Arc::new(Gate::default());
        let done = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let worker = {
            let gate = Arc::clone(&gate);
            let done = Arc::clone(&done);
            std::thread::spawn(move || {
                gate.enter();
                done.store(true, std::sync::atomic::Ordering::SeqCst);
            })
        };
        gate.wait_until_entered();
        assert!(
            !done.load(std::sync::atomic::Ordering::SeqCst),
            "the window is open: nothing past `enter` has run yet"
        );
        gate.open();
        worker.join().expect("the worker must finish once let go");
        assert!(done.load(std::sync::atomic::Ordering::SeqCst));
    }

    #[test]
    fn a_gate_already_open_does_not_stop_anybody() {
        let gate = Gate::default();
        gate.open();
        gate.enter();
    }
}
