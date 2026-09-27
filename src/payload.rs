use std::marker::PhantomData;
use std::path::Path;

use serde_json::Value;

use crate::{AnyEvent, AnyHarness, ClaudeCode, Codex, Error, EventKind, Harness, Tool};

/// A hook payload from harness `H`: its event and the raw JSON.
#[derive(Debug, Clone)]
pub struct Payload<H: Harness> {
    event: H::Event,
    raw: Value,
    _h: PhantomData<fn() -> H>,
}

impl<H: Harness> Payload<H> {
    /// Parses hook stdin. Fails only when it is not a JSON object.
    ///
    /// ```
    /// use pabal::{Codex, CodexEvent, Payload};
    /// let p = Payload::<Codex>::parse(r#"{"hook_event_name":"Stop"}"#).unwrap();
    /// assert_eq!(p.event(), &CodexEvent::Stop);
    /// assert!(Payload::<Codex>::parse("[]").is_err());
    /// ```
    pub fn parse(stdin: &str) -> Result<Self, Error> {
        Self::from_value(serde_json::from_str(stdin)?)
    }

    /// Wraps an already parsed payload. Fails only when it is not an object.
    pub fn from_value(raw: Value) -> Result<Self, Error> {
        if !raw.is_object() {
            return Err(Error::NotObject);
        }
        let event = H::Event::from(text(&raw, "hook_event_name").unwrap_or(""));
        Ok(Self {
            event,
            raw,
            _h: PhantomData,
        })
    }

    /// The event, from `hook_event_name`.
    pub fn event(&self) -> &H::Event {
        &self.event
    }

    /// Narrows the payload to its event's view.
    pub fn view(&self) -> H::View<'_> {
        H::view(self)
    }
}

/// The accessors every payload has, whatever its harness.
///
/// An absent field, a JSON `null`, a wrong-typed value and an empty string
/// all read as `None`.
#[ambassador::delegatable_trait]
pub trait Fields {
    /// The harness that sent the payload.
    fn harness(&self) -> AnyHarness;
    /// The event in its harness-independent form.
    fn any_event(&self) -> AnyEvent;
    /// The `hook_event_name` as sent, including events this crate does not know.
    fn event_name(&self) -> String;
    /// The `session_id`.
    fn session_id(&self) -> Option<&str>;
    /// The `cwd`, verbatim.
    fn cwd(&self) -> Option<&Path>;
    /// The `transcript_path`, verbatim.
    fn transcript_path(&self) -> Option<&Path>;
    /// The raw `agent_id`, set for subagents and for Claude Code forks alike.
    fn agent_id(&self) -> Option<&str>;
    /// The subagent this payload speaks for: `agent_id` when `agent_type` is
    /// also set. A Claude Code fork has no `agent_type` and speaks for its
    /// session.
    fn agent(&self) -> Option<&str>;
    /// The `tool_use_id` of a single-tool event.
    fn tool_use_id(&self) -> Option<&str>;
    /// The tool call of a single-tool event; `None` without a `tool_name`.
    fn tool(&self) -> Option<Tool<'_>>;
    /// The payload as parsed, for fields this crate does not model.
    fn raw(&self) -> &Value;
}

impl<H: Harness> Fields for Payload<H> {
    fn harness(&self) -> AnyHarness {
        H::KIND
    }
    fn any_event(&self) -> AnyEvent {
        self.event.to_any()
    }
    fn event_name(&self) -> String {
        self.event.to_string()
    }
    fn session_id(&self) -> Option<&str> {
        text(&self.raw, "session_id")
    }
    fn cwd(&self) -> Option<&Path> {
        text(&self.raw, "cwd").map(Path::new)
    }
    fn transcript_path(&self) -> Option<&Path> {
        text(&self.raw, "transcript_path").map(Path::new)
    }
    fn agent_id(&self) -> Option<&str> {
        text(&self.raw, "agent_id")
    }
    fn agent(&self) -> Option<&str> {
        text(&self.raw, "agent_type").and(text(&self.raw, "agent_id"))
    }
    fn tool_use_id(&self) -> Option<&str> {
        text(&self.raw, "tool_use_id")
    }
    fn tool(&self) -> Option<Tool<'_>> {
        H::tool(&self.raw, self.cwd())
    }
    fn raw(&self) -> &Value {
        &self.raw
    }
}

/// A payload whose harness is chosen at runtime.
#[derive(Debug, Clone, ambassador::Delegate)]
#[delegate(Fields)]
#[allow(missing_docs, reason = "each variant is its harness")]
pub enum AnyPayload {
    ClaudeCode(Payload<ClaudeCode>),
    Codex(Payload<Codex>),
}

impl AnyPayload {
    /// Parses hook stdin as coming from `harness`.
    pub fn parse(harness: AnyHarness, stdin: &str) -> Result<Self, Error> {
        Self::from_value(harness, serde_json::from_str(stdin)?)
    }

    /// Wraps an already parsed payload from `harness`.
    pub fn from_value(harness: AnyHarness, raw: Value) -> Result<Self, Error> {
        Ok(match harness {
            AnyHarness::ClaudeCode => Self::ClaudeCode(Payload::from_value(raw)?),
            AnyHarness::Codex => Self::Codex(Payload::from_value(raw)?),
        })
    }

    /// Parses hook stdin, guessing the harness with [`AnyHarness::infer`].
    ///
    /// ```
    /// use pabal::{AnyHarness, AnyPayload, Fields};
    /// let p = AnyPayload::parse_inferred(r#"{"hook_event_name":"Stop","turn_id":"t"}"#);
    /// assert_eq!(p.unwrap().harness(), AnyHarness::Codex);
    /// ```
    pub fn parse_inferred(stdin: &str) -> Result<Self, Error> {
        let raw: Value = serde_json::from_str(stdin)?;
        Self::from_value(AnyHarness::infer(&raw), raw)
    }
}

/// A non-empty string field, else `None`.
pub(crate) fn text<'a>(v: &'a Value, key: &str) -> Option<&'a str> {
    v.get(key)?.as_str().filter(|s| !s.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CodexEvent;

    #[test]
    fn not_an_object_is_the_only_error() {
        assert!(matches!(
            Payload::<Codex>::parse("[]"),
            Err(Error::NotObject)
        ));
        assert!(matches!(
            Payload::<Codex>::parse("nope"),
            Err(Error::Json(_))
        ));
        let empty = Payload::<Codex>::parse("{}").unwrap();
        assert_eq!(empty.event(), &CodexEvent::Other(String::new()));
    }

    #[test]
    fn blank_and_wrong_typed_fields_are_none() {
        let p = Payload::<ClaudeCode>::parse(
            r#"{"session_id":"","cwd":null,"transcript_path":7,"tool_use_id":["x"]}"#,
        )
        .unwrap();
        assert_eq!(
            (
                p.session_id(),
                p.cwd(),
                p.transcript_path(),
                p.tool_use_id()
            ),
            (None, None, None, None)
        );
    }

    #[test]
    fn codex_null_transcript_path_is_none() {
        let p = Payload::<Codex>::parse(r#"{"session_id":"0199a-root","transcript_path":null,"cwd":"CWD","hook_event_name":"SessionStart","model":"gpt-5","permission_mode":"default","source":"resume"}"#).unwrap();
        assert_eq!(p.event(), &CodexEvent::SessionStart);
        assert_eq!(p.session_id(), Some("0199a-root"));
        assert_eq!(p.cwd(), Some(Path::new("CWD")));
        assert_eq!(p.transcript_path(), None);
    }

    #[test]
    fn missing_identity_stays_missing() {
        let p = Payload::<ClaudeCode>::parse(
            r#"{ "hook_event_name": "PreToolUse", "tool_name": "Bash", "tool_input": { "command": "ls" } }"#,
        )
        .unwrap();
        assert!(p.session_id().is_none() && p.cwd().is_none());
    }

    #[test]
    fn claude_forks_speak_for_their_session() {
        let fork = Payload::<ClaudeCode>::parse(r#"{"agent_id":"f1"}"#).unwrap();
        assert_eq!((fork.agent_id(), fork.agent()), (Some("f1"), None));
        let sub =
            Payload::<ClaudeCode>::parse(r#"{"agent_id":"a1","agent_type":"Explore"}"#).unwrap();
        assert_eq!(sub.agent(), Some("a1"));
        let blank = Payload::<ClaudeCode>::parse(r#"{"agent_id":"a1","agent_type":""}"#).unwrap();
        assert_eq!(blank.agent(), None);
    }

    #[test]
    fn codex_uses_the_same_subagent_rule() {
        let fork = Payload::<Codex>::parse(r#"{"agent_id":"f1"}"#).unwrap();
        assert_eq!((fork.agent_id(), fork.agent()), (Some("f1"), None));
        let sub = Payload::<Codex>::parse(r#"{"agent_id":"a1","agent_type":"worker"}"#).unwrap();
        assert_eq!(sub.agent(), Some("a1"));
        let blank = Payload::<Codex>::parse(r#"{"agent_id":"a1","agent_type":""}"#).unwrap();
        assert_eq!(blank.agent(), None);
    }

    #[test]
    fn any_payload_delegates() {
        let p = AnyPayload::parse(
            AnyHarness::Codex,
            r#"{"hook_event_name":"PreToolUse","session_id":"s","cwd":"/w"}"#,
        )
        .unwrap();
        assert_eq!(p.harness(), AnyHarness::Codex);
        assert_eq!(p.any_event(), AnyEvent::PreToolUse);
        assert_eq!(p.event_name(), "PreToolUse");
        assert_eq!(
            (p.session_id(), p.cwd()),
            (Some("s"), Some(Path::new("/w")))
        );
        let q = AnyPayload::parse_inferred(r#"{"hook_event_name":"PreToolUse","turn_id":"t"}"#)
            .unwrap();
        assert_eq!(q.harness(), AnyHarness::Codex);
        assert!(matches!(
            AnyPayload::parse_inferred("[]"),
            Err(Error::NotObject)
        ));
    }
}
