use std::path::Path;

use serde_json::Value;

use super::{AnyHarness, Harness, Keys, sealed};
use crate::{
    AntigravityEvent, Payload, Tool,
    payload::text,
    tool::field,
    view::{AntigravityView, PostToolUse, PreToolUse, Raw, Stop},
};

/// Google Antigravity.
///
/// Antigravity leaves the event out of the payload, so parse its stdin with
/// [`Payload::parse_named`], naming the event the hook was configured for.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Antigravity;

impl sealed::Sealed for Antigravity {}

impl Harness for Antigravity {
    type Event = AntigravityEvent;
    type View<'a> = AntigravityView<'a>;

    const KEYS: Keys = Keys {
        session_id: &["conversationId"],
        cwd: &[],
        transcript_path: &["transcriptPath"],
        agent_id: &[],
        agent_type: &[],
        tool_use_id: &[],
    };
    const KIND: AnyHarness = AnyHarness::Antigravity;

    fn tool<'a>(payload: &'a Value, _cwd: Option<&'a Path>) -> Option<Tool<'a>> {
        let call = payload.get("toolCall")?;
        let name = text(call, "name")?;
        let args = field(call, "args");
        let known = match name {
            "run_command" => {
                let cwd = text(args, "Cwd").map(Path::new);
                Tool::shell(args, "CommandLine", cwd, None)
            }
            "write_to_file" | "replace_file_content" | "multi_replace_file_content" => {
                Tool::write(args, "TargetFile")
            }
            _ => None,
        };
        Some(known.unwrap_or(Tool::Other { name, input: args }))
    }

    fn view(payload: &Payload<Self>) -> AntigravityView<'_> {
        match payload.event() {
            AntigravityEvent::PreToolUse => AntigravityView::PreToolUse(PreToolUse(payload)),
            AntigravityEvent::PostToolUse => AntigravityView::PostToolUse(PostToolUse(payload)),
            AntigravityEvent::PreInvocation => AntigravityView::PreInvocation(Raw(payload)),
            AntigravityEvent::PostInvocation => AntigravityView::PostInvocation(Raw(payload)),
            AntigravityEvent::Stop => AntigravityView::Stop(Stop(payload)),
            AntigravityEvent::Other(_) => AntigravityView::Other(Raw(payload)),
        }
    }
}
