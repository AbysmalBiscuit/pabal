use std::path::Path;

use serde_json::Value;

use super::{AnyHarness, Harness, Keys, sealed};
use crate::{
    CursorEvent, Payload, Tool,
    payload::text,
    tool::field,
    view::{
        BeforeMcpExecution, BeforeShellExecution, CursorView, PostToolUse, PreCompact, PreToolUse,
        Raw, SessionEnd, SessionStart, Stop, SubagentStart, SubagentStop, UserPromptSubmit,
    },
};

/// Cursor, running hooks from its own `hooks.json`.
///
/// Cursor also runs hooks configured for Claude Code and sends them Claude
/// Code's payloads; parse those as [`ClaudeCode`](crate::ClaudeCode).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Cursor;

impl sealed::Sealed for Cursor {}

impl Harness for Cursor {
    type Event = CursorEvent;
    type View<'a> = CursorView<'a>;

    const KEYS: Keys = Keys {
        session_id: &["session_id", "conversation_id"],
        agent_id: &["subagent_id"],
        agent_type: &["subagent_type"],
        ..Keys::SNAKE_CASE
    };
    const KIND: AnyHarness = AnyHarness::Cursor;

    fn tool<'a>(call: &'a Value, cwd: Option<&'a Path>) -> Option<Tool<'a>> {
        // `beforeShellExecution` and `afterShellExecution` carry the command
        // at the top level, with no tool name.
        let Some(name) = text(call, "tool_name") else {
            return Tool::shell(call, "command", cwd, None);
        };
        let input = field(call, "tool_input");
        if let Some(server) = text(call, "mcp_server_name") {
            return Some(Tool::Mcp {
                server: Some(server),
                tool: name,
                input,
            });
        }
        let known = match name {
            "Shell" => {
                let working_directory = text(input, "working_directory").map(Path::new);
                Tool::shell(input, "command", working_directory.or(cwd), None)
            }
            "Write" => Tool::write(input, "file_path"),
            _ => name.strip_prefix("MCP:").map(|tool| Tool::Mcp {
                server: None,
                tool,
                input,
            }),
        };
        Some(known.unwrap_or(Tool::Other { name, input }))
    }

    fn view(payload: &Payload<Self>) -> CursorView<'_> {
        match payload.event() {
            CursorEvent::SessionStart => CursorView::SessionStart(SessionStart(payload)),
            CursorEvent::SessionEnd => CursorView::SessionEnd(SessionEnd(payload)),
            CursorEvent::PreToolUse => CursorView::PreToolUse(PreToolUse(payload)),
            CursorEvent::PostToolUse => CursorView::PostToolUse(PostToolUse(payload)),
            CursorEvent::PostToolUseFailure => CursorView::PostToolUseFailure(Raw(payload)),
            CursorEvent::SubagentStart => CursorView::SubagentStart(SubagentStart(payload)),
            CursorEvent::SubagentStop => CursorView::SubagentStop(SubagentStop(payload)),
            CursorEvent::BeforeShellExecution => {
                CursorView::BeforeShellExecution(BeforeShellExecution(payload))
            }
            CursorEvent::AfterShellExecution => CursorView::AfterShellExecution(Raw(payload)),
            CursorEvent::BeforeMcpExecution => {
                CursorView::BeforeMcpExecution(BeforeMcpExecution(payload))
            }
            CursorEvent::AfterMcpExecution => CursorView::AfterMcpExecution(Raw(payload)),
            CursorEvent::BeforeReadFile => CursorView::BeforeReadFile(Raw(payload)),
            CursorEvent::AfterFileEdit => CursorView::AfterFileEdit(Raw(payload)),
            CursorEvent::BeforeSubmitPrompt => {
                CursorView::BeforeSubmitPrompt(UserPromptSubmit(payload))
            }
            CursorEvent::PreCompact => CursorView::PreCompact(PreCompact(payload)),
            CursorEvent::Stop => CursorView::Stop(Stop(payload)),
            CursorEvent::AfterAgentResponse => CursorView::AfterAgentResponse(Raw(payload)),
            CursorEvent::AfterAgentThought => CursorView::AfterAgentThought(Raw(payload)),
            CursorEvent::BeforeTabFileRead => CursorView::BeforeTabFileRead(Raw(payload)),
            CursorEvent::AfterTabFileEdit => CursorView::AfterTabFileEdit(Raw(payload)),
            CursorEvent::WorkspaceOpen => CursorView::WorkspaceOpen(Raw(payload)),
            CursorEvent::Other(_) => CursorView::Other(Raw(payload)),
        }
    }
}
