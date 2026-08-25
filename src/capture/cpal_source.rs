//! The real input: a `cpal` stream, built and dropped on the recorder's thread.
//!
//! This is the only file in the crate that knows what a sound card is. Everything
//! above it sees the `Source` interface and the events it delivers, which is what
//! lets the rest be tested without a microphone.

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use super::{Event, Format, Sink, Source};
use crate::audio::device::{self, Choice};
use crate::audio::resample;

/// Rates worth asking a device for, best first. Each is a whole multiple of the
/// 16 kHz recognition needs; see `tasks/8/DESIGN_8.md`, section 3.
const PREFERRED_RATES: &[u32] = &[48_000, 32_000, 64_000, 96_000];

#[derive(Default)]
pub struct CpalSource {
    stream: Option<cpal::platform::Stream>,
}

impl CpalSource {
    pub fn new() -> CpalSource {
        CpalSource::default()
    }
}

impl Source for CpalSource {
    fn start(&mut self, wanted: Option<&str>, sink: Sink) -> Result<Format, String> {
        let host = cpal::default_host();
        let devices: Vec<cpal::platform::Device> = host
            .input_devices()
            .map_err(|e| format!("cannot list input devices: {e}"))?
            .collect();
        let names: Vec<String> = devices.iter().map(|d| d.to_string()).collect();

        let choice = device::choose(wanted.unwrap_or(""), &names).map_err(|e| e.to_string())?;
        let picked = match choice {
            Choice::Default => host
                .default_input_device()
                .ok_or_else(|| "this machine has no default input device".to_string())?,
            Choice::Named { name, ambiguous } => {
                if ambiguous {
                    eprintln!("more than one input is called {name:?}; taking the first");
                }
                devices
                    .into_iter()
                    .find(|d| d.to_string() == name)
                    .ok_or_else(|| format!("the input {name:?} disappeared while opening it"))?
            }
        };
        let label = picked.to_string();

        let ranges: Vec<cpal::SupportedStreamConfigRange> = picked
            .supported_input_configs()
            .map_err(|e| format!("cannot read what {label:?} supports: {e}"))?
            .collect();
        let config = pick_config(&ranges).ok_or_else(|| {
            format!(
                "{label:?} offers no sample rate this build can use: {}. \
                 It records at 32000, 48000, 64000 or 96000 Hz",
                describe(&ranges)
            )
        })?;

        let format = Format {
            rate: config.sample_rate(),
            channels: config.channels(),
        };
        let sample_format = config.sample_format();
        let stream_config: cpal::StreamConfig = config.into();

        let failed = sink.clone();
        let on_error = move |e: cpal::Error| {
            failed.push(Event::Failed(e.to_string()));
        };

        let stream = match sample_format {
            cpal::SampleFormat::F32 => picked.build_input_stream::<f32, _, _>(
                stream_config,
                move |data, _| sink.push(Event::Samples(data.to_vec())),
                on_error,
                None,
            ),
            cpal::SampleFormat::I16 => picked.build_input_stream::<i16, _, _>(
                stream_config,
                move |data, _| {
                    sink.push(Event::Samples(
                        data.iter()
                            .map(|s| f32::from(*s) / f32::from(i16::MAX))
                            .collect(),
                    ))
                },
                on_error,
                None,
            ),
            other => {
                return Err(format!(
                    "{label:?} delivers {other:?} samples, which this build does not read"
                ))
            }
        }
        .map_err(|e| format!("cannot open {label:?}: {e}"))?;

        stream
            .play()
            .map_err(|e| format!("cannot start recording from {label:?}: {e}"))?;
        self.stream = Some(stream);
        Ok(format)
    }

    fn stop(&mut self) {
        // Dropping the stream releases the device. It happens on this thread,
        // which is the whole reason the recorder owns one.
        self.stream = None;
    }
}

/// The best configuration a device offers, or nothing usable.
fn pick_config(ranges: &[cpal::SupportedStreamConfigRange]) -> Option<cpal::SupportedStreamConfig> {
    for rate in PREFERRED_RATES {
        if let Some(range) = ranges
            .iter()
            .find(|r| r.min_sample_rate() <= *rate && *rate <= r.max_sample_rate())
        {
            return Some((*range).with_sample_rate(*rate));
        }
    }
    // Nothing preferred: take anything that divides into the target.
    ranges
        .iter()
        .find(|r| resample::ratio_for(r.max_sample_rate()).is_some())
        .map(|r| (*r).with_max_sample_rate())
}

fn describe(ranges: &[cpal::SupportedStreamConfigRange]) -> String {
    ranges
        .iter()
        .map(|r| format!("{}-{} Hz", r.min_sample_rate(), r.max_sample_rate()))
        .collect::<Vec<_>>()
        .join(", ")
}
