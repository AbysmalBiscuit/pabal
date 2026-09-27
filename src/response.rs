//! Hook responses, built from the view of the event they answer. A response
//! the harness would not accept for an event does not compile.

use std::fmt;

use serde_json::{Map, Value, json};

use crate::view::{
    AnyPostToolUse, AnyPreToolUse, AnySessionStart, AnySubagentStart, AnyUserPromptSubmit,
    PostToolBatch, PostToolUse, PreToolUse, SessionStart, SubagentStart, UserPromptSubmit,
};
use crate::{AnyHarness, ClaudeCode, Harness};

/// What a hook writes to stdout: a JSON object, or nothing.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Response(Option<Value>);

impl Response {
    /// An empty stdout.
    pub fn none() -> Self {
        Self(None)
    }

    /// Whether the hook writes nothing.
    pub fn is_none(&self) -> bool {
        self.0.is_none()
    }

    /// The JSON the hook writes, if any.
    pub fn json(&self) -> Option<&Value> {
        self.0.as_ref()
    }

    /// Denies a `PreToolUse` call when no payload could be parsed.
    ///
    /// ```
    /// use pabal::{AnyHarness, Response};
    /// let r = Response::deny_pre_tool_use(AnyHarness::Codex, "stdin was not JSON");
    /// assert_eq!(r.json().unwrap()["hookSpecificOutput"]["permissionDecision"], "deny");
    /// ```
    pub fn deny_pre_tool_use(harness: AnyHarness, reason: &str) -> Self {
        match harness {
            AnyHarness::ClaudeCode | AnyHarness::Codex => deny(reason),
        }
    }
}

impl fmt::Display for Response {
    /// Writes compact JSON, or nothing for [`Response::none`].
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            Some(v) => write!(f, "{v}"),
            None => Ok(()),
        }
    }
}

fn hook_specific(event: &str, fields: Map<String, Value>) -> Response {
    let mut output = Map::from_iter([("hookEventName".to_owned(), json!(event))]);
    output.extend(fields);
    Response(Some(json!({ "hookSpecificOutput": output })))
}

fn permission(decision: &str, reason: Option<&str>) -> Response {
    let mut fields = Map::from_iter([("permissionDecision".to_owned(), json!(decision))]);
    if let Some(reason) = reason {
        fields.insert("permissionDecisionReason".to_owned(), json!(reason));
    }
    hook_specific("PreToolUse", fields)
}

/// Codex rejects a blank deny reason and runs the tool anyway.
fn deny(reason: &str) -> Response {
    let reason = if reason.trim().is_empty() {
        "Denied by a hook."
    } else {
        reason
    };
    permission("deny", Some(reason))
}

fn context(event: &str, text: &str) -> Response {
    hook_specific(
        event,
        Map::from_iter([("additionalContext".to_owned(), json!(text))]),
    )
}

/// Blocks the tool call, telling the agent why. Only `PreToolUse` has it:
///
/// ```compile_fail
/// use pabal::{Codex, Deny, Payload};
/// let p = Payload::<Codex>::parse("{}").unwrap();
/// p.session_end().unwrap().deny("no");
/// ```
pub trait Deny {
    /// A deny with `reason`; a blank reason becomes a fixed one.
    fn deny(&self, reason: &str) -> Response;
}

/// Adds text to the agent's context without changing the outcome.
///
/// ```compile_fail
/// use pabal::{AddContext, ClaudeCode, Payload};
/// let p = Payload::<ClaudeCode>::parse("{}").unwrap();
/// p.stop().unwrap().add_context("x");
/// ```
pub trait AddContext {
    /// The response that adds `text`.
    fn add_context(&self, text: &str) -> Response;
}

/// Asks the user to confirm the tool call. Only Claude Code's `PreToolUse`
/// has it; Codex fails open on `ask`:
///
/// ```compile_fail
/// use pabal::{Ask, Codex, Payload};
/// let p = Payload::<Codex>::parse("{}").unwrap();
/// p.pre_tool_use().unwrap().ask("sure?");
/// ```
pub trait Ask {
    /// The response that asks, showing `reason`.
    fn ask(&self, reason: &str) -> Response;
}

/// Allows the tool call, skipping the user's own permission prompt. Only
/// Claude Code's `PreToolUse` has it:
///
/// ```compile_fail
/// use pabal::{Allow, Codex, Payload};
/// let p = Payload::<Codex>::parse("{}").unwrap();
/// p.pre_tool_use().unwrap().allow_skipping_prompt();
/// ```
pub trait Allow {
    /// The response that allows the call.
    fn allow_skipping_prompt(&self) -> Response;
}

impl<H: Harness> Deny for PreToolUse<'_, H> {
    fn deny(&self, reason: &str) -> Response {
        deny(reason)
    }
}

impl Ask for PreToolUse<'_, ClaudeCode> {
    fn ask(&self, reason: &str) -> Response {
        permission("ask", Some(reason))
    }
}

impl Allow for PreToolUse<'_, ClaudeCode> {
    fn allow_skipping_prompt(&self) -> Response {
        permission("allow", None)
    }
}

impl AddContext for PostToolBatch<'_> {
    fn add_context(&self, text: &str) -> Response {
        context("PostToolBatch", text)
    }
}

macro_rules! add_context {
    ($($view:ident $any:ident),*) => {$(
        impl<H: Harness> AddContext for $view<'_, H> {
            fn add_context(&self, text: &str) -> Response {
                context(stringify!($view), text)
            }
        }

        impl AddContext for $any<'_> {
            fn add_context(&self, text: &str) -> Response {
                match self {
                    $any::ClaudeCode(v) => v.add_context(text),
                    $any::Codex(v) => v.add_context(text),
                }
            }
        }
    )*};
}

add_context!(
    PreToolUse AnyPreToolUse,
    PostToolUse AnyPostToolUse,
    UserPromptSubmit AnyUserPromptSubmit,
    SessionStart AnySessionStart,
    SubagentStart AnySubagentStart
);

impl Deny for AnyPreToolUse<'_> {
    fn deny(&self, reason: &str) -> Response {
        deny(reason)
    }
}

impl AnyPreToolUse<'_> {
    /// [`Ask::ask`], or `None` on a harness without it.
    pub fn ask(&self, reason: &str) -> Option<Response> {
        match self {
            AnyPreToolUse::ClaudeCode(v) => Some(v.ask(reason)),
            AnyPreToolUse::Codex(_) => None,
        }
    }

    /// [`Allow::allow_skipping_prompt`], or `None` on a harness without it.
    pub fn allow_skipping_prompt(&self) -> Option<Response> {
        match self {
            AnyPreToolUse::ClaudeCode(v) => Some(v.allow_skipping_prompt()),
            AnyPreToolUse::Codex(_) => None,
        }
    }
}
