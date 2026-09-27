use std::path::Path;

use serde_json::Value;

use super::{AnyHarness, Harness, Keys, sealed};
use crate::{
    CodexEvent, Payload, Tool,
    payload::text,
    tool::input,
    view::{
        CodexView, PermissionRequest, PostCompact, PostToolUse, PreCompact, PreToolUse, Raw,
        SessionEnd, SessionStart, Stop, SubagentStart, SubagentStop, UserPromptSubmit,
    },
};

/// OpenAI Codex.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Codex;

impl sealed::Sealed for Codex {}

impl Harness for Codex {
    type Event = CodexEvent;
    type View<'a> = CodexView<'a>;

    const KEYS: Keys = Keys::SNAKE_CASE;
    const KIND: AnyHarness = AnyHarness::Codex;

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

    fn view(payload: &Payload<Self>) -> CodexView<'_> {
        match payload.event() {
            CodexEvent::PreToolUse => CodexView::PreToolUse(PreToolUse(payload)),
            CodexEvent::PostToolUse => CodexView::PostToolUse(PostToolUse(payload)),
            CodexEvent::PermissionRequest => {
                CodexView::PermissionRequest(PermissionRequest(payload))
            }
            CodexEvent::SessionStart => CodexView::SessionStart(SessionStart(payload)),
            CodexEvent::SessionEnd => CodexView::SessionEnd(SessionEnd(payload)),
            CodexEvent::UserPromptSubmit => CodexView::UserPromptSubmit(UserPromptSubmit(payload)),
            CodexEvent::Stop => CodexView::Stop(Stop(payload)),
            CodexEvent::Interrupt => CodexView::Interrupt(Raw(payload)),
            CodexEvent::SubagentStart => CodexView::SubagentStart(SubagentStart(payload)),
            CodexEvent::SubagentStop => CodexView::SubagentStop(SubagentStop(payload)),
            CodexEvent::PreCompact => CodexView::PreCompact(PreCompact(payload)),
            CodexEvent::PostCompact => CodexView::PostCompact(PostCompact(payload)),
            CodexEvent::Other(_) => CodexView::Other(Raw(payload)),
        }
    }
}
