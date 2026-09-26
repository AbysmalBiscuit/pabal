use super::{AnyHarness, Harness, sealed};
use crate::CodexEvent;

/// OpenAI Codex.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Codex;

impl sealed::Sealed for Codex {}

impl Harness for Codex {
    const KIND: AnyHarness = AnyHarness::Codex;
    type Event = CodexEvent;
}
