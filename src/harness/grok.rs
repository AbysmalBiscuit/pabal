use std::path::Path;

use serde_json::Value;

use super::{AnyHarness, Harness, Keys, sealed};
use crate::{
    GrokEvent, Payload, Tool,
    payload::text,
    tool::field,
    view::{
        GrokView, PostCompact, PostToolUse, PreCompact, PreToolUse, Raw, SessionEnd, SessionStart,
        Stop, SubagentStart, SubagentStop, UserPromptSubmit,
    },
};

/// xAI's Grok CLI (grok-build), running hooks from its own `~/.grok/hooks/`,
/// `.grok/hooks/`, config files and plugins.
///
/// Grok also runs hooks from Claude Code's `settings.json` and Cursor's
/// `hooks.json`, and sends them this same payload.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Grok;

impl sealed::Sealed for Grok {}

impl Harness for Grok {
    type Event = GrokEvent;
    type View<'a> = GrokView<'a>;

    // Grok sends camelCase keys and snake_case copies of some of them; the
    // camelCase key is the one it documents as authoritative.
    const KEYS: Keys = Keys {
        session_id: &["sessionId", "session_id"],
        cwd: &["cwd"],
        transcript_path: &["transcriptPath", "transcript_path"],
        agent_id: &["subagentId"],
        agent_type: &["subagentType"],
        // A subagent's own events carry its id only as their session id.
        agent: &["subagentId", "sessionId"],
        tool_use_id: &["toolUseId", "tool_use_id"],
    };
    const KIND: AnyHarness = AnyHarness::Grok;

    fn tool<'a>(call: &'a Value, cwd: Option<&'a Path>) -> Option<Tool<'a>> {
        let name = text(call, "toolName")?;
        let input = field(call, "toolInput");
        let known = match name {
            "run_terminal_command" => Tool::shell(input, "command", cwd, None),
            "search_replace" | "write" => Tool::write(input, "file_path"),
            // `toolInput` of an MCP call is the `use_tool` dispatcher's input,
            // with the call's own arguments under `tool_input`.
            _ => name
                .split_once("__")
                .filter(|(server, tool)| !server.is_empty() && !tool.is_empty())
                .map(|(server, tool)| Tool::Mcp {
                    server: Some(server),
                    tool,
                    input: input.get("tool_input").unwrap_or(input),
                }),
        };
        Some(known.unwrap_or(Tool::Other { name, input }))
    }

    fn view(payload: &Payload<Self>) -> GrokView<'_> {
        match payload.event() {
            GrokEvent::SessionStart => GrokView::SessionStart(SessionStart(payload)),
            GrokEvent::UserPromptSubmit => GrokView::UserPromptSubmit(UserPromptSubmit(payload)),
            GrokEvent::PreToolUse => GrokView::PreToolUse(PreToolUse(payload)),
            GrokEvent::PostToolUse => GrokView::PostToolUse(PostToolUse(payload)),
            GrokEvent::PostToolUseFailure => GrokView::PostToolUseFailure(Raw(payload)),
            GrokEvent::PermissionDenied => GrokView::PermissionDenied(Raw(payload)),
            GrokEvent::Stop => GrokView::Stop(Stop(payload)),
            GrokEvent::StopFailure => GrokView::StopFailure(Raw(payload)),
            GrokEvent::StopCancelled => GrokView::StopCancelled(Raw(payload)),
            GrokEvent::Notification => GrokView::Notification(Raw(payload)),
            GrokEvent::SubagentStart => GrokView::SubagentStart(SubagentStart(payload)),
            GrokEvent::SubagentStop => GrokView::SubagentStop(SubagentStop(payload)),
            GrokEvent::PreCompact => GrokView::PreCompact(PreCompact(payload)),
            GrokEvent::PostCompact => GrokView::PostCompact(PostCompact(payload)),
            GrokEvent::SessionEnd => GrokView::SessionEnd(SessionEnd(payload)),
            GrokEvent::Other(_) => GrokView::Other(Raw(payload)),
        }
    }
}
