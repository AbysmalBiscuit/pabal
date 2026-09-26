use std::path::Path;

use serde_json::Value;

use super::{AnyHarness, Harness, sealed};
use crate::payload::text;
use crate::tool::input;
use crate::view::{
    ClaudeCodeView, PermissionRequest, PostCompact, PostToolBatch, PostToolUse, PreCompact,
    PreToolUse, Raw, SessionEnd, SessionStart, Stop, SubagentStart, SubagentStop, UserPromptSubmit,
};
use crate::{ClaudeCodeEvent, Payload, ShellKind, Tool};

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

    type View<'a> = ClaudeCodeView<'a>;

    fn view(payload: &Payload<Self>) -> ClaudeCodeView<'_> {
        match payload.event() {
            ClaudeCodeEvent::SessionStart => ClaudeCodeView::SessionStart(SessionStart(payload)),
            ClaudeCodeEvent::Setup => ClaudeCodeView::Setup(Raw(payload)),
            ClaudeCodeEvent::UserPromptSubmit => {
                ClaudeCodeView::UserPromptSubmit(UserPromptSubmit(payload))
            }
            ClaudeCodeEvent::UserPromptExpansion => {
                ClaudeCodeView::UserPromptExpansion(Raw(payload))
            }
            ClaudeCodeEvent::PreToolUse => ClaudeCodeView::PreToolUse(PreToolUse(payload)),
            ClaudeCodeEvent::PermissionRequest => {
                ClaudeCodeView::PermissionRequest(PermissionRequest(payload))
            }
            ClaudeCodeEvent::PermissionDenied => ClaudeCodeView::PermissionDenied(Raw(payload)),
            ClaudeCodeEvent::PostToolUse => ClaudeCodeView::PostToolUse(PostToolUse(payload)),
            ClaudeCodeEvent::PostToolUseFailure => ClaudeCodeView::PostToolUseFailure(Raw(payload)),
            ClaudeCodeEvent::PostToolBatch => ClaudeCodeView::PostToolBatch(PostToolBatch(payload)),
            ClaudeCodeEvent::Notification => ClaudeCodeView::Notification(Raw(payload)),
            ClaudeCodeEvent::MessageDisplay => ClaudeCodeView::MessageDisplay(Raw(payload)),
            ClaudeCodeEvent::SubagentStart => ClaudeCodeView::SubagentStart(SubagentStart(payload)),
            ClaudeCodeEvent::SubagentStop => ClaudeCodeView::SubagentStop(SubagentStop(payload)),
            ClaudeCodeEvent::TaskCreated => ClaudeCodeView::TaskCreated(Raw(payload)),
            ClaudeCodeEvent::TaskCompleted => ClaudeCodeView::TaskCompleted(Raw(payload)),
            ClaudeCodeEvent::Stop => ClaudeCodeView::Stop(Stop(payload)),
            ClaudeCodeEvent::StopFailure => ClaudeCodeView::StopFailure(Raw(payload)),
            ClaudeCodeEvent::TeammateIdle => ClaudeCodeView::TeammateIdle(Raw(payload)),
            ClaudeCodeEvent::InstructionsLoaded => ClaudeCodeView::InstructionsLoaded(Raw(payload)),
            ClaudeCodeEvent::ConfigChange => ClaudeCodeView::ConfigChange(Raw(payload)),
            ClaudeCodeEvent::CwdChanged => ClaudeCodeView::CwdChanged(Raw(payload)),
            ClaudeCodeEvent::DirectoryAdded => ClaudeCodeView::DirectoryAdded(Raw(payload)),
            ClaudeCodeEvent::FileChanged => ClaudeCodeView::FileChanged(Raw(payload)),
            ClaudeCodeEvent::WorktreeCreate => ClaudeCodeView::WorktreeCreate(Raw(payload)),
            ClaudeCodeEvent::WorktreeRemove => ClaudeCodeView::WorktreeRemove(Raw(payload)),
            ClaudeCodeEvent::PreCompact => ClaudeCodeView::PreCompact(PreCompact(payload)),
            ClaudeCodeEvent::PostCompact => ClaudeCodeView::PostCompact(PostCompact(payload)),
            ClaudeCodeEvent::PreModelSwitch => ClaudeCodeView::PreModelSwitch(Raw(payload)),
            ClaudeCodeEvent::PostModelSwitch => ClaudeCodeView::PostModelSwitch(Raw(payload)),
            ClaudeCodeEvent::Elicitation => ClaudeCodeView::Elicitation(Raw(payload)),
            ClaudeCodeEvent::ElicitationResult => ClaudeCodeView::ElicitationResult(Raw(payload)),
            ClaudeCodeEvent::SessionEnd => ClaudeCodeView::SessionEnd(SessionEnd(payload)),
            ClaudeCodeEvent::Other(_) => ClaudeCodeView::Other(Raw(payload)),
        }
    }
}
