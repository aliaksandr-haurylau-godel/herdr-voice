//! The invocation context herdr passes to a plugin command.
//!
//! The target pane is the one herdr names here; nothing is derived. Unknown fields
//! are ignored, so a herdr release that adds one does not break an action. See
//! `tasks/3/DESIGN_3.md`, section 2, and `docs/design.md`, section 3.

use serde::Deserialize;
use std::fmt;

#[derive(Debug, Default, Clone, PartialEq, Eq, Deserialize)]
pub struct Invocation {
    pub focused_pane_id: Option<String>,
    pub focused_pane_cwd: Option<String>,
    pub focused_pane_agent: Option<String>,
    pub tab_id: Option<String>,
    pub tab_label: Option<String>,
}

impl Invocation {
    /// The pane a command delivers to, or nothing when herdr named none.
    pub fn target_pane(&self) -> Option<&str> {
        self.focused_pane_id.as_deref()
    }
}

#[derive(Debug)]
pub enum ContextError {
    Absent,
    Malformed(String),
}

impl fmt::Display for ContextError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ContextError::Absent => write!(
                f,
                "HERDR_PLUGIN_CONTEXT_JSON was not set, so there is no pane to work with; \
                 invoke this through a herdr keybinding or action"
            ),
            ContextError::Malformed(why) => {
                write!(f, "the invocation context is unreadable: {why}")
            }
        }
    }
}

impl std::error::Error for ContextError {}

pub fn parse(body: &[u8]) -> Result<Invocation, ContextError> {
    if body.is_empty() {
        return Err(ContextError::Absent);
    }
    serde_json::from_slice(body).map_err(|e| ContextError::Malformed(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_fields_the_target_needs_are_read() {
        let body = br#"{
            "workspace_id": "w1",
            "tab_id": "t1",
            "tab_label": "notes",
            "focused_pane_id": "w1:p2",
            "focused_pane_cwd": "/repo",
            "focused_pane_agent": "claude",
            "focused_pane_status": "idle",
            "worktree": { "repo_root": "/repo", "checkout_path": "/repo" },
            "selected_text": "",
            "invocation_source": "keybinding"
        }"#;
        let invocation = parse(body).expect("parse");
        assert_eq!(invocation.focused_pane_id.as_deref(), Some("w1:p2"));
        assert_eq!(invocation.focused_pane_cwd.as_deref(), Some("/repo"));
        assert_eq!(invocation.focused_pane_agent.as_deref(), Some("claude"));
        assert_eq!(invocation.tab_id.as_deref(), Some("t1"));
        assert_eq!(invocation.tab_label.as_deref(), Some("notes"));
        assert_eq!(invocation.target_pane(), Some("w1:p2"));
    }

    #[test]
    fn a_field_that_is_absent_is_absent_rather_than_an_error() {
        let invocation = parse(br#"{"tab_id":"t1"}"#).expect("parse");
        assert_eq!(invocation.tab_id.as_deref(), Some("t1"));
        assert_eq!(invocation.focused_pane_id, None);
        assert_eq!(invocation.target_pane(), None);
    }

    #[test]
    fn a_field_herdr_adds_later_does_not_break_an_action() {
        let invocation = parse(br#"{"tab_id":"t1","something_new":{"a":[1,2]}}"#).expect("parse");
        assert_eq!(invocation.tab_id.as_deref(), Some("t1"));
    }

    #[test]
    fn escapes_and_unicode_survive() {
        // Two ways the same label can arrive: JSON escapes, and raw UTF-8 bytes.
        let escaped = parse(br#"{"tab_label":"a \"quoted\" caf\u00e9"}"#).expect("parse");
        assert_eq!(escaped.tab_label.as_deref(), Some("a \"quoted\" caf\u{e9}"));

        let raw = parse(r#"{"tab_label":"a \"quoted\" café"}"#.as_bytes()).expect("parse");
        assert_eq!(raw.tab_label.as_deref(), Some("a \"quoted\" café"));
    }

    #[test]
    fn an_empty_body_says_the_variable_was_not_set() {
        let error = parse(b"").expect_err("must refuse");
        assert!(matches!(error, ContextError::Absent), "got {error:?}");
        assert!(error.to_string().contains("HERDR_PLUGIN_CONTEXT_JSON"));
    }

    #[test]
    fn a_body_that_is_not_an_object_is_refused_with_the_reason() {
        let error = parse(b"[1,2,3]").expect_err("must refuse");
        assert!(matches!(error, ContextError::Malformed(_)), "got {error:?}");
        let error = parse(b"{not json").expect_err("must refuse");
        assert!(matches!(error, ContextError::Malformed(_)), "got {error:?}");
    }
}
