//! Which input to record from.
//!
//! By name, never by index: indices shift the moment a headset is connected or
//! removed, and a shifted index sends the recording to a different input in
//! silence (`spike/spike.sh:55`). This function never touches `cpal` — it takes
//! names, which is what makes it testable without a microphone.

use std::fmt;

#[derive(Debug, PartialEq, Eq)]
pub enum Choice<'a> {
    /// Whatever the host calls its default input.
    Default,
    /// The device with this name.
    Named { name: &'a str, ambiguous: bool },
}

#[derive(Debug)]
pub enum DeviceError {
    NoDevices {
        configured: String,
    },
    NoSuchName {
        configured: String,
        available: Vec<String>,
    },
}

impl fmt::Display for DeviceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DeviceError::NoDevices { configured } => write!(
                f,
                "no input devices at all, so {configured:?} cannot be opened; \
                 check that the system sees a microphone"
            ),
            DeviceError::NoSuchName {
                configured,
                available,
            } => write!(
                f,
                "no input device named {configured:?}; the ones that exist are {}. \
                 Set [audio] input to one of them, or leave it empty for the default",
                available
                    .iter()
                    .map(|name| format!("{name:?}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        }
    }
}

impl std::error::Error for DeviceError {}

/// The device to record from, given what was configured and what the host offers.
///
/// A name that matches nothing is an error rather than a fall back to the default:
/// falling back is silent, and silence is the failure this whole rule exists to
/// prevent.
pub fn choose<'a>(configured: &str, available: &'a [String]) -> Result<Choice<'a>, DeviceError> {
    if configured.is_empty() {
        return Ok(Choice::Default);
    }
    if available.is_empty() {
        return Err(DeviceError::NoDevices {
            configured: configured.to_string(),
        });
    }
    let mut matches = available.iter().filter(|name| name.as_str() == configured);
    match matches.next() {
        None => Err(DeviceError::NoSuchName {
            configured: configured.to_string(),
            available: available.to_vec(),
        }),
        Some(first) => Ok(Choice::Named {
            name: first.as_str(),
            ambiguous: matches.next().is_some(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn an_empty_name_means_the_system_default() {
        let available = names(&["Built-in", "Headset"]);
        assert_eq!(choose("", &available).unwrap(), Choice::Default);
    }

    #[test]
    fn a_name_that_exists_is_taken_as_it_is() {
        let available = names(&["Built-in", "Headset"]);
        assert_eq!(
            choose("Headset", &available).unwrap(),
            Choice::Named {
                name: "Headset",
                ambiguous: false
            }
        );
    }

    #[test]
    fn a_name_that_matches_nothing_lists_what_does_exist() {
        let available = names(&["Built-in", "Headset"]);
        let error = choose("Studio", &available).expect_err("must refuse");
        let message = error.to_string();
        assert!(message.contains("Studio"), "got {message}");
        assert!(message.contains("Built-in"), "got {message}");
        assert!(message.contains("Headset"), "got {message}");
    }

    #[test]
    fn a_name_that_matches_nothing_does_not_fall_back_to_the_default() {
        let available = names(&["Built-in"]);
        assert!(choose("Studio", &available).is_err());
    }

    #[test]
    fn two_devices_of_one_name_take_the_first_and_say_so() {
        let available = names(&["Headset", "Headset"]);
        assert_eq!(
            choose("Headset", &available).unwrap(),
            Choice::Named {
                name: "Headset",
                ambiguous: true
            }
        );
    }

    #[test]
    fn an_empty_host_with_a_configured_name_says_there_are_none() {
        let error = choose("Headset", &[]).expect_err("must refuse");
        assert!(
            error.to_string().contains("no input devices"),
            "got {error}"
        );
    }
}
