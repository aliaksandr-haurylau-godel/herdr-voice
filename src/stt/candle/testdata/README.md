# Reference mel filterbanks

`melfilters80.bytes` and `melfilters128.bytes` are the precomputed Whisper mel
filterbanks from huggingface/candle 0.11.0, `candle-examples/examples/whisper/`
(MIT/Apache-2.0). Each is a little-endian `f32` array of `n_mels x 201` values.

They are test fixtures only. `src/stt/candle/mel.rs` computes the same matrices
from the specification, and the test compares against these; nothing includes
them outside `#[cfg(test)]`, so the shipped binary contains neither.
