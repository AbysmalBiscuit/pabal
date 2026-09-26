use super::{AnyHarness, Harness, sealed};
use crate::ClaudeCodeEvent;

/// Claude Code.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ClaudeCode;

impl sealed::Sealed for ClaudeCode {}

impl Harness for ClaudeCode {
    const KIND: AnyHarness = AnyHarness::ClaudeCode;
    type Event = ClaudeCodeEvent;
}
