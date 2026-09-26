use std::path::Path;

use serde_json::Value;

use super::{AnyHarness, Harness, sealed};
use crate::payload::text;
use crate::tool::input;
use crate::{ClaudeCodeEvent, ShellKind, Tool};

/// Claude Code.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ClaudeCode;

impl sealed::Sealed for ClaudeCode {}

impl Harness for ClaudeCode {
    const KIND: AnyHarness = AnyHarness::ClaudeCode;
    type Event = ClaudeCodeEvent;

    fn tool<'a>(call: &'a Value, cwd: Option<&'a Path>) -> Option<Tool<'a>> {
        let name = text(call, "tool_name")?;
        let input = input(call);
        let known = match name {
            "Bash" => Tool::shell(input, cwd, Some(ShellKind::Bash)),
            "PowerShell" => Tool::shell(input, cwd, Some(ShellKind::PowerShell)),
            "Edit" | "Write" | "MultiEdit" => Tool::write(input, "file_path"),
            "NotebookEdit" => Tool::write(input, "notebook_path"),
            _ => {
                let server = call.get("mcp_server").and_then(|s| text(s, "name"));
                Tool::mcp(name, server, input)
            }
        };
        Some(known.unwrap_or(Tool::Other { name, input }))
    }
}
