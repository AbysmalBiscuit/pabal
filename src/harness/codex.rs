use std::path::Path;

use serde_json::Value;

use super::{AnyHarness, Harness, sealed};
use crate::payload::text;
use crate::tool::input;
use crate::{CodexEvent, Tool};

/// OpenAI Codex.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Codex;

impl sealed::Sealed for Codex {}

impl Harness for Codex {
    const KIND: AnyHarness = AnyHarness::Codex;
    type Event = CodexEvent;

    fn tool<'a>(call: &'a Value, cwd: Option<&'a Path>) -> Option<Tool<'a>> {
        let name = text(call, "tool_name")?;
        let input = input(call);
        let known = match name {
            // Codex names every shell tool `Bash` in hook payloads, so the
            // name says nothing about the shell.
            "Bash" => Tool::shell(input, cwd, None),
            "apply_patch" => Tool::patch(input),
            _ => Tool::mcp(name, None, input),
        };
        Some(known.unwrap_or(Tool::Other { name, input }))
    }
}
