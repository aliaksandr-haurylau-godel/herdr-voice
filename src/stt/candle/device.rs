//! Which device the model runs on, chosen once and reported everywhere.
//!
//! The choice is made here rather than inside `CandleEngine::new`, so `doctor`
//! can report it without building an engine and a test can pin it without the
//! hardware. Falling back is never a refusal, and never silent: it costs a
//! factor of ten. See `tasks/15/DESIGN_15.md`, section 8.

// No caller until `check_with` and the engine land (Tasks 9 and 10).
#![allow(dead_code)]

/// What the engine will run on, and — when it is not the fast answer — why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Selection {
    Metal,
    Cpu { why: &'static str },
}

/// Ask for a device. Never fails: a slow transcript beats no transcript, and the
/// person is told which they are getting before they wait for it.
pub fn select() -> Selection {
    #[cfg(target_os = "macos")]
    {
        // `Cargo.toml`'s target-specific dependency block gives every macOS build
        // the metal feature, so asking is the whole test: if a device comes back,
        // the kernels are there.
        match candle_core::Device::new_metal(0) {
            Ok(_) => Selection::Metal,
            Err(_) => Selection::Cpu {
                why: "this machine refused a Metal device",
            },
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        Selection::Cpu {
            why: "Metal exists only on macOS",
        }
    }
}

/// The device a selection names. Asking Metal for a handle twice is cheap; it is
/// a handle, not a context full of compiled kernels.
pub fn device_for(selection: &Selection) -> candle_core::Device {
    match selection {
        // Not a swallowed failure: `select` has already reported which device the
        // person is on, and a refused handle here gives the same answer `select`
        // would have given.
        Selection::Metal => candle_core::Device::new_metal(0).unwrap_or(candle_core::Device::Cpu),
        Selection::Cpu { .. } => candle_core::Device::Cpu,
    }
}

/// The sentence `doctor` prints and the daemon writes at start.
pub fn describe(selection: &Selection) -> String {
    match selection {
        Selection::Metal => "on the GPU, through Metal".to_string(),
        Selection::Cpu { why } => format!(
            "on the CPU ({why}), which is about ten times slower than the GPU — a \
             minute of speech takes about a minute with the default model. Set \
             [stt] model to \"tiny\" if that is too slow"
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asking_for_a_device_always_answers() {
        // Never a failure: a slow transcript beats no transcript. This is the one
        // test here that touches the machine, and it asserts only that some answer
        // comes back, so it passes on a CI runner with no GPU.
        let _ = select();
    }

    #[test]
    fn metal_is_described_without_alarming_anybody() {
        let text = describe(&Selection::Metal);
        assert!(text.to_lowercase().contains("metal"), "got {text}");
        assert!(!text.contains("slow"), "nothing is wrong here: {text}");
    }

    #[test]
    fn the_cpu_says_why_and_what_it_costs_and_what_to_do() {
        let text = describe(&Selection::Cpu {
            why: "Metal exists only on macOS",
        });
        assert!(text.to_lowercase().contains("cpu"), "got {text}");
        assert!(
            text.contains("Metal exists only on macOS"),
            "the why must survive: {text}"
        );
        // Ten times slower is a different product, not a slower one. The person
        // must learn that before they wait a minute for a minute of speech.
        assert!(text.contains("ten times"), "it must name the cost: {text}");
        assert!(
            text.contains("tiny"),
            "it must name a model that stays usable: {text}"
        );
    }

    #[test]
    fn both_variants_are_describable_with_no_hardware_at_all() {
        // Selection is a plain enum precisely so this holds on every runner.
        for selection in [
            Selection::Metal,
            Selection::Cpu {
                why: "asked and refused",
            },
        ] {
            assert!(!describe(&selection).is_empty());
        }
    }

    #[test]
    fn a_cpu_selection_never_hands_back_a_gpu() {
        assert!(matches!(
            device_for(&Selection::Cpu { why: "asked for" }),
            candle_core::Device::Cpu
        ));
    }
}
