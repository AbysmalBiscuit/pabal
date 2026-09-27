//! Per-event views of a payload, reached with [`Payload::view`] or
//! [`AnyPayload::view`].

use std::ops::Deref;

use serde_json::Value;

use crate::{
    AnyEvent, AnyPayload, ClaudeCode, Codex, EventKind, Fields, Harness, Payload, ToolCall,
    payload::text,
};

/// A view of an event this crate does not model; the payload is all it has.
#[derive(Debug, Clone, Copy)]
pub struct Raw<'a, H: Harness>(pub(crate) &'a Payload<H>);

impl<H: Harness> Deref for Raw<'_, H> {
    type Target = Payload<H>;

    fn deref(&self) -> &Payload<H> {
        self.0
    }
}

macro_rules! shared_views {
    ($($name:ident $any:ident $has:ident $short:ident;)*) => {
        $(
            #[doc = concat!("A `", stringify!($name), "` payload.")]
            #[derive(Debug, Clone, Copy)]
            pub struct $name<'a, H: Harness>(pub(crate) &'a Payload<H>);

            impl<H: Harness> Deref for $name<'_, H> {
                type Target = Payload<H>;
                fn deref(&self) -> &Payload<H> {
                    self.0
                }
            }

            #[doc = concat!("A harness that sends `", stringify!($name), "`.")]
            pub trait $has: Harness {}
            impl $has for ClaudeCode {}
            impl $has for Codex {}

            impl<H: $has> Payload<H> {
                #[doc = concat!("This payload as `", stringify!($name), "`, if it is one.")]
                pub fn $short(&self) -> Option<$name<'_, H>> {
                    (self.event().to_any() == AnyEvent::$name).then_some($name(self))
                }
            }

            #[doc = concat!("A `", stringify!($name), "` view from either harness.")]
            #[derive(Debug, Clone, Copy)]
            #[allow(missing_docs, reason = "each variant is its harness")]
            pub enum $any<'a> {
                ClaudeCode($name<'a, ClaudeCode>),
                Codex($name<'a, Codex>),
            }

            impl<'a> $any<'a> {
                /// The payload's accessors, whatever its harness.
                pub fn fields(&self) -> &'a dyn Fields {
                    match self {
                        Self::ClaudeCode(v) => v.0,
                        Self::Codex(v) => v.0,
                    }
                }
            }
        )*

        /// A payload narrowed to an event every harness sends.
        #[derive(Debug, Clone, Copy)]
        #[allow(missing_docs, reason = "each variant is its wire name")]
        pub enum AnyView<'a> {
            $($name($any<'a>),)*
            /// An event only some harnesses send, or one this crate does not know.
            Other(&'a AnyPayload),
        }

        impl AnyPayload {
            /// Narrows the payload to its event's view.
            pub fn view(&self) -> AnyView<'_> {
                match self {
                    AnyPayload::ClaudeCode(p) => match p.event().to_any() {
                        $(AnyEvent::$name => AnyView::$name($any::ClaudeCode($name(p))),)*
                        AnyEvent::Other(_) => AnyView::Other(self),
                    },
                    AnyPayload::Codex(p) => match p.event().to_any() {
                        $(AnyEvent::$name => AnyView::$name($any::Codex($name(p))),)*
                        AnyEvent::Other(_) => AnyView::Other(self),
                    },
                }
            }
        }
    };
}

shared_views! {
    SessionStart AnySessionStart HasSessionStart session_start;
    SessionEnd AnySessionEnd HasSessionEnd session_end;
    UserPromptSubmit AnyUserPromptSubmit HasUserPromptSubmit user_prompt_submit;
    PreToolUse AnyPreToolUse HasPreToolUse pre_tool_use;
    PostToolUse AnyPostToolUse HasPostToolUse post_tool_use;
    PermissionRequest AnyPermissionRequest HasPermissionRequest permission_request;
    SubagentStart AnySubagentStart HasSubagentStart subagent_start;
    SubagentStop AnySubagentStop HasSubagentStop subagent_stop;
    Stop AnyStop HasStop stop;
    PreCompact AnyPreCompact HasPreCompact pre_compact;
    PostCompact AnyPostCompact HasPostCompact post_compact;
}

/// A Claude Code `PostToolBatch` payload: several tool calls at once.
#[derive(Debug, Clone, Copy)]
pub struct PostToolBatch<'a>(pub(crate) &'a Payload<ClaudeCode>);

impl Deref for PostToolBatch<'_> {
    type Target = Payload<ClaudeCode>;

    fn deref(&self) -> &Payload<ClaudeCode> {
        self.0
    }
}

impl<'a> PostToolBatch<'a> {
    /// The calls in `tool_calls[]`, skipping entries without a `tool_name`.
    pub fn tool_calls(&self) -> impl Iterator<Item = ToolCall<'a>> + use<'a> {
        let payload: &'a Payload<ClaudeCode> = self.0;
        let cwd = payload.cwd();
        payload
            .raw()
            .get("tool_calls")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(move |call| {
                Some(ToolCall {
                    tool_use_id: text(call, "tool_use_id"),
                    tool: ClaudeCode::tool(call, cwd)?,
                    response: call.get("tool_response").filter(|v| !v.is_null()),
                })
            })
    }
}

impl Payload<ClaudeCode> {
    /// This payload as `PostToolBatch`, if it is one. Only Claude Code sends
    /// it:
    ///
    /// ```
    /// use pabal::{ClaudeCode, Payload};
    /// let p = Payload::<ClaudeCode>::parse(r#"{"hook_event_name":"PostToolBatch"}"#).unwrap();
    /// assert!(p.post_tool_batch().is_some());
    /// ```
    ///
    /// ```compile_fail
    /// use pabal::{Codex, Payload};
    /// let p = Payload::<Codex>::parse("{}").unwrap();
    /// p.post_tool_batch();
    /// ```
    pub fn post_tool_batch(&self) -> Option<PostToolBatch<'_>> {
        matches!(self.event(), crate::ClaudeCodeEvent::PostToolBatch).then_some(PostToolBatch(self))
    }
}

/// A Claude Code payload narrowed to its event.
#[derive(Debug, Clone, Copy)]
#[allow(missing_docs, reason = "each variant is its wire name")]
pub enum ClaudeCodeView<'a> {
    SessionStart(SessionStart<'a, ClaudeCode>),
    Setup(Raw<'a, ClaudeCode>),
    UserPromptSubmit(UserPromptSubmit<'a, ClaudeCode>),
    UserPromptExpansion(Raw<'a, ClaudeCode>),
    PreToolUse(PreToolUse<'a, ClaudeCode>),
    PermissionRequest(PermissionRequest<'a, ClaudeCode>),
    PermissionDenied(Raw<'a, ClaudeCode>),
    PostToolUse(PostToolUse<'a, ClaudeCode>),
    PostToolUseFailure(Raw<'a, ClaudeCode>),
    PostToolBatch(PostToolBatch<'a>),
    Notification(Raw<'a, ClaudeCode>),
    MessageDisplay(Raw<'a, ClaudeCode>),
    SubagentStart(SubagentStart<'a, ClaudeCode>),
    SubagentStop(SubagentStop<'a, ClaudeCode>),
    TaskCreated(Raw<'a, ClaudeCode>),
    TaskCompleted(Raw<'a, ClaudeCode>),
    Stop(Stop<'a, ClaudeCode>),
    StopFailure(Raw<'a, ClaudeCode>),
    TeammateIdle(Raw<'a, ClaudeCode>),
    InstructionsLoaded(Raw<'a, ClaudeCode>),
    ConfigChange(Raw<'a, ClaudeCode>),
    CwdChanged(Raw<'a, ClaudeCode>),
    DirectoryAdded(Raw<'a, ClaudeCode>),
    FileChanged(Raw<'a, ClaudeCode>),
    WorktreeCreate(Raw<'a, ClaudeCode>),
    WorktreeRemove(Raw<'a, ClaudeCode>),
    PreCompact(PreCompact<'a, ClaudeCode>),
    PostCompact(PostCompact<'a, ClaudeCode>),
    PreModelSwitch(Raw<'a, ClaudeCode>),
    PostModelSwitch(Raw<'a, ClaudeCode>),
    Elicitation(Raw<'a, ClaudeCode>),
    ElicitationResult(Raw<'a, ClaudeCode>),
    SessionEnd(SessionEnd<'a, ClaudeCode>),
    Other(Raw<'a, ClaudeCode>),
}

/// A Codex payload narrowed to its event.
#[derive(Debug, Clone, Copy)]
#[allow(missing_docs, reason = "each variant is its wire name")]
pub enum CodexView<'a> {
    PreToolUse(PreToolUse<'a, Codex>),
    PostToolUse(PostToolUse<'a, Codex>),
    PermissionRequest(PermissionRequest<'a, Codex>),
    SessionStart(SessionStart<'a, Codex>),
    SessionEnd(SessionEnd<'a, Codex>),
    UserPromptSubmit(UserPromptSubmit<'a, Codex>),
    Stop(Stop<'a, Codex>),
    Interrupt(Raw<'a, Codex>),
    SubagentStart(SubagentStart<'a, Codex>),
    SubagentStop(SubagentStop<'a, Codex>),
    PreCompact(PreCompact<'a, Codex>),
    PostCompact(PostCompact<'a, Codex>),
    Other(Raw<'a, Codex>),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AnyHarness, Edit, Tool};

    #[test]
    fn codex_views_are_exhaustive_per_event() {
        let p = Payload::<Codex>::parse(
            r#"{"hook_event_name":"PreToolUse","tool_name":"Bash","tool_input":{"command":"ls"}}"#,
        )
        .unwrap();
        let CodexView::PreToolUse(pre) = p.view() else {
            panic!()
        };
        assert!(matches!(
            pre.tool(),
            Some(Tool::Shell { command: "ls", .. })
        ));
    }

    #[test]
    fn unmodeled_events_narrow_to_raw() {
        let p = Payload::<ClaudeCode>::parse(r#"{"hook_event_name":"FileChanged"}"#).unwrap();
        assert!(matches!(p.view(), ClaudeCodeView::FileChanged(Raw(_))));
        let p = Payload::<ClaudeCode>::parse(r#"{"hook_event_name":"Brand"}"#).unwrap();
        let ClaudeCodeView::Other(raw) = p.view() else {
            panic!()
        };
        assert_eq!(raw.event_name(), "Brand");
    }

    #[test]
    fn shortcuts_return_none_on_other_events() {
        let p = Payload::<Codex>::parse(r#"{"hook_event_name":"SessionStart"}"#).unwrap();
        assert!(p.pre_tool_use().is_none());
        assert!(p.session_start().is_some());
        let q = Payload::<ClaudeCode>::parse(r#"{"hook_event_name":"Stop"}"#).unwrap();
        assert!(q.post_tool_batch().is_none());
        assert!(q.stop().is_some());
    }

    #[test]
    fn post_tool_batch_iterates_its_calls() {
        let p = Payload::<ClaudeCode>::parse(
            r#"{"hook_event_name":"PostToolBatch","cwd":"/r","tool_calls":[
            {"tool_name":"Edit","tool_input":{"file_path":"/r/a.rs"},"tool_use_id":"t1","tool_response":{}},
            {"tool_name":"Bash","tool_input":{"command":"ls"},"tool_use_id":"t2"},
            {"tool_input":{"command":"no name"}}]}"#,
        )
        .unwrap();
        let calls: Vec<_> = p.post_tool_batch().unwrap().tool_calls().collect();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].tool_use_id, Some("t1"));
        assert!(matches!(calls[0].tool, Tool::Edit(Edit::Write { .. })));
        assert!(calls[0].response.is_some() && calls[1].response.is_none());
        assert!(matches!(calls[1].tool, Tool::Shell { cwd: Some(_), .. }));
        assert!(p.tool().is_none());
    }

    #[test]
    fn any_view_wraps_the_harness_view() {
        let p = AnyPayload::parse(
            AnyHarness::Codex,
            r#"{"hook_event_name":"PreToolUse","session_id":"s"}"#,
        )
        .unwrap();
        let AnyView::PreToolUse(pre) = p.view() else {
            panic!()
        };
        assert!(matches!(pre, AnyPreToolUse::Codex(_)));
        assert_eq!(pre.fields().session_id(), Some("s"));
    }

    #[test]
    fn any_view_other_keeps_the_payload() {
        let p = AnyPayload::parse(AnyHarness::Codex, r#"{"hook_event_name":"Interrupt"}"#).unwrap();
        let AnyView::Other(other) = p.view() else {
            panic!()
        };
        assert_eq!(other.event_name(), "Interrupt");
        let q = AnyPayload::parse(
            AnyHarness::ClaudeCode,
            r#"{"hook_event_name":"PostToolBatch"}"#,
        )
        .unwrap();
        assert!(matches!(q.view(), AnyView::Other(_)));
    }
}
