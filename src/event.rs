use std::fmt::{Debug, Display};

/// A harness's event enum, parsed from and displayed as `hook_event_name`.
pub trait EventKind: Clone + Debug + PartialEq + Eq + Display + for<'s> From<&'s str> {
    /// Every event this version knows, without `Other`.
    fn known() -> Vec<Self>;

    /// The harness-independent form of this event.
    fn to_any(&self) -> AnyEvent;
}

/// A Claude Code hook event, named as Claude Code sends `hook_event_name`.
#[derive(Debug, Clone, PartialEq, Eq, strum::EnumString, strum::Display, strum::EnumIter)]
pub enum ClaudeCodeEvent {
    SessionStart,
    Setup,
    UserPromptSubmit,
    UserPromptExpansion,
    PreToolUse,
    PermissionRequest,
    PermissionDenied,
    PostToolUse,
    PostToolUseFailure,
    PostToolBatch,
    Notification,
    MessageDisplay,
    SubagentStart,
    SubagentStop,
    TaskCreated,
    TaskCompleted,
    Stop,
    StopFailure,
    TeammateIdle,
    InstructionsLoaded,
    ConfigChange,
    CwdChanged,
    DirectoryAdded,
    FileChanged,
    WorktreeCreate,
    WorktreeRemove,
    PreCompact,
    PostCompact,
    PreModelSwitch,
    PostModelSwitch,
    Elicitation,
    ElicitationResult,
    SessionEnd,
    /// An event this version does not know, with its wire name.
    #[strum(default)]
    Other(String),
}

/// A Codex hook event, named as Codex sends `hook_event_name`.
#[derive(Debug, Clone, PartialEq, Eq, strum::EnumString, strum::Display, strum::EnumIter)]
pub enum CodexEvent {
    PreToolUse,
    PostToolUse,
    PermissionRequest,
    SessionStart,
    SessionEnd,
    UserPromptSubmit,
    Stop,
    Interrupt,
    SubagentStart,
    SubagentStop,
    PreCompact,
    PostCompact,
    /// An event this version does not know, with its wire name.
    #[strum(default)]
    Other(String),
}

/// An event every harness sends, or `Other` with the harness's own name.
#[derive(Debug, Clone, PartialEq, Eq, strum::Display)]
pub enum AnyEvent {
    SessionStart,
    SessionEnd,
    UserPromptSubmit,
    PreToolUse,
    PostToolUse,
    PermissionRequest,
    SubagentStart,
    SubagentStop,
    Stop,
    PreCompact,
    PostCompact,
    #[strum(to_string = "{0}")]
    Other(String),
}

macro_rules! impl_event_kind {
    ($($event:ident),*) => {$(
        impl EventKind for $event {
            fn known() -> Vec<Self> {
                <Self as strum::IntoEnumIterator>::iter()
                    .filter(|e| !matches!(e, Self::Other(_)))
                    .collect()
            }

            fn to_any(&self) -> AnyEvent {
                match self {
                    Self::SessionStart => AnyEvent::SessionStart,
                    Self::SessionEnd => AnyEvent::SessionEnd,
                    Self::UserPromptSubmit => AnyEvent::UserPromptSubmit,
                    Self::PreToolUse => AnyEvent::PreToolUse,
                    Self::PostToolUse => AnyEvent::PostToolUse,
                    Self::PermissionRequest => AnyEvent::PermissionRequest,
                    Self::SubagentStart => AnyEvent::SubagentStart,
                    Self::SubagentStop => AnyEvent::SubagentStop,
                    Self::Stop => AnyEvent::Stop,
                    Self::PreCompact => AnyEvent::PreCompact,
                    Self::PostCompact => AnyEvent::PostCompact,
                    other => AnyEvent::Other(other.to_string()),
                }
            }
        }
    )*};
}

impl_event_kind!(ClaudeCodeEvent, CodexEvent);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_names_round_trip() {
        for e in ClaudeCodeEvent::known() {
            assert_eq!(ClaudeCodeEvent::from(e.to_string().as_str()), e);
        }
        for e in CodexEvent::known() {
            assert_eq!(CodexEvent::from(e.to_string().as_str()), e);
        }
    }

    #[test]
    fn unknown_events_keep_their_name() {
        assert_eq!(CodexEvent::from("Brand"), CodexEvent::Other("Brand".into()));
        assert_eq!(CodexEvent::from("Brand").to_string(), "Brand");
        assert_eq!(
            CodexEvent::from("preToolUse"),
            CodexEvent::Other("preToolUse".into())
        );
    }

    #[test]
    fn seed_lists_have_the_spec_sizes() {
        assert_eq!(ClaudeCodeEvent::known().len(), 33);
        assert_eq!(CodexEvent::known().len(), 12);
        assert!(!CodexEvent::known().contains(&CodexEvent::Other(String::new())));
    }

    #[test]
    fn any_event_mapping() {
        assert_eq!(ClaudeCodeEvent::PreToolUse.to_any(), AnyEvent::PreToolUse);
        assert_eq!(CodexEvent::PostCompact.to_any(), AnyEvent::PostCompact);
        assert_eq!(
            CodexEvent::Interrupt.to_any(),
            AnyEvent::Other("Interrupt".into())
        );
        assert_eq!(
            ClaudeCodeEvent::PostToolBatch.to_any(),
            AnyEvent::Other("PostToolBatch".into())
        );
        assert_eq!(
            ClaudeCodeEvent::PostToolUseFailure.to_any(),
            AnyEvent::Other("PostToolUseFailure".into())
        );
    }
}
