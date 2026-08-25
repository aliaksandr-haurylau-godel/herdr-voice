//! Biasing recognition with what the agent is talking about.
//!
//! `[context] source` selects how the conversation component is gathered: from
//! the target agent's transcript, from the pane's screen, or automatically —
//! transcript first, falling back to the pane on a miss. File and directory
//! names are collected independent of `source`. See `tasks/21/DESIGN_21.md`,
//! sections 1 and 2.

pub mod files;
pub mod pane;
pub mod source;
pub mod transcript;

/// The three values `[context] source` can resolve to. `Auto` is a member in
/// its own right, not an absence of one — `bias::source::resolve("auto")`
/// returns `Ok(Source::Auto)`, and nothing downstream reconstructs a third
/// state some other way (`tasks/21/DESIGN_21.md`, section 2a).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Transcript,
    Pane,
    Auto,
}
