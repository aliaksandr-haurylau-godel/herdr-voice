//! The built-in engine: a Whisper model running in this process.
//!
//! `candle-transformers` supplies the network. Everything around it lives here,
//! split so that the tensor half is thin and every decision that can make a
//! transcript wrong lives in `plan`, which no test needs weights to reach.
//! See `tasks/15/DESIGN_15.md`, section 1.

pub mod mel;
pub mod store;
