use std::path::Path;

use serde_json::Value;

use crate::{ClaudeCodeEvent, CodexEvent, CursorEvent, EventKind, Payload, Tool, payload::text};

mod antigravity;
mod claude_code;
mod codex;
mod cursor;

pub use antigravity::Antigravity;
pub use claude_code::ClaudeCode;
pub use codex::Codex;
pub use cursor::Cursor;

mod sealed {
    pub trait Sealed {}
}

/// A harness's wire vocabulary, used through generics as `Payload<H>`.
///
/// Sealed: only this crate's harness types implement it.
pub trait Harness: sealed::Sealed + Sized + 'static {
    /// The runtime name of this harness.
    const KIND: AnyHarness;
    /// The payload keys behind each [`Fields`](crate::Fields) accessor.
    #[doc(hidden)]
    const KEYS: Keys;
    /// The harness's event enum.
    type Event: EventKind;

    /// The tool view of `call`, in the harness's own shape: a single-tool
    /// payload, or one entry of a batch.
    #[doc(hidden)]
    fn tool<'a>(call: &'a Value, cwd: Option<&'a Path>) -> Option<Tool<'a>>;

    /// The harness's view enum, one variant per event it sends.
    type View<'a>;

    #[doc(hidden)]
    fn view(payload: &Payload<Self>) -> Self::View<'_>;
}

/// The payload keys a harness spells each [`Fields`](crate::Fields) accessor
/// with. The accessor reads the first key that holds a non-empty string.
#[doc(hidden)]
#[derive(Debug)]
pub struct Keys {
    pub session_id: &'static [&'static str],
    pub cwd: &'static [&'static str],
    pub transcript_path: &'static [&'static str],
    pub agent_id: &'static [&'static str],
    pub agent_type: &'static [&'static str],
    pub tool_use_id: &'static [&'static str],
}

impl Keys {
    /// The snake_case keys Claude Code and Codex send.
    pub(crate) const SNAKE_CASE: Keys = Keys {
        session_id: &["session_id"],
        cwd: &["cwd"],
        transcript_path: &["transcript_path"],
        agent_id: &["agent_id"],
        agent_type: &["agent_type"],
        tool_use_id: &["tool_use_id"],
    };
}

/// A harness chosen at runtime, from a `--harness` flag or from the payload.
///
/// Its string forms are `claude-code` (alias `claude`), `codex`, `cursor` and
/// `antigravity`.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, strum::EnumString, strum::Display, strum::IntoStaticStr,
)]
#[strum(serialize_all = "kebab-case")]
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
#[allow(missing_docs, reason = "each variant is its harness")]
pub enum AnyHarness {
    #[strum(to_string = "claude-code", serialize = "claude")]
    #[cfg_attr(feature = "clap", value(name = "claude-code", alias = "claude"))]
    ClaudeCode,
    Codex,
    Cursor,
    Antigravity,
}

impl AnyHarness {
    /// Guesses the harness. `conversationId` means Antigravity. A Cursor
    /// event name (camelCase), or any camelCase event with `cursor_version`,
    /// means Cursor. A PascalCase event with
    /// `cursor_version` comes from Cursor running a Claude Code hook and means
    /// Claude Code. Otherwise an event only one harness sends decides, then
    /// `turn_id` or `model` means Codex. A null field counts as absent.
    ///
    /// Misreads Codex `SessionEnd` (neither field) as Claude Code, and Claude
    /// Code `SessionStart` with `model` as Codex. Prefer an explicit harness.
    ///
    /// ```
    /// use pabal::AnyHarness;
    /// let codex = serde_json::json!({"hook_event_name": "Stop", "turn_id": "t"});
    /// assert_eq!(AnyHarness::infer(&codex), AnyHarness::Codex);
    /// let cursor = serde_json::json!({"hook_event_name": "stop", "cursor_version": "1.7.2"});
    /// assert_eq!(AnyHarness::infer(&cursor), AnyHarness::Cursor);
    /// ```
    pub fn infer(raw: &Value) -> AnyHarness {
        let has = |key| raw.get(key).is_some_and(|v| !v.is_null());
        if has("conversationId") {
            return AnyHarness::Antigravity;
        }
        let event = text(raw, "hook_event_name");
        let camel_case = event.is_some_and(|e| e.starts_with(|c: char| c.is_ascii_lowercase()));
        if knows::<CursorEvent>(event) || (camel_case && has("cursor_version")) {
            return AnyHarness::Cursor;
        }
        let codex = match (knows::<ClaudeCodeEvent>(event), knows::<CodexEvent>(event)) {
            (true, false) => false,
            (false, true) => true,
            _ => has("turn_id") || has("model"),
        };
        if has("cursor_version") || !codex {
            AnyHarness::ClaudeCode
        } else {
            AnyHarness::Codex
        }
    }
}

/// Whether `event` is one `E` names.
fn knows<E: EventKind>(event: Option<&str>) -> bool {
    event.is_some_and(|e| E::known().contains(&E::from(e)))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn names_round_trip() {
        assert_eq!(
            "claude-code".parse::<AnyHarness>().unwrap(),
            AnyHarness::ClaudeCode
        );
        assert_eq!(
            "claude".parse::<AnyHarness>().unwrap(),
            AnyHarness::ClaudeCode
        );
        assert_eq!("codex".parse::<AnyHarness>().unwrap(), AnyHarness::Codex);
        assert_eq!("cursor".parse::<AnyHarness>().unwrap(), AnyHarness::Cursor);
        assert!("Claude".parse::<AnyHarness>().is_err());
        assert_eq!(AnyHarness::ClaudeCode.to_string(), "claude-code");
        assert_eq!(AnyHarness::Codex.to_string(), "codex");
        assert_eq!(AnyHarness::Cursor.to_string(), "cursor");
        assert_eq!(
            "antigravity".parse::<AnyHarness>().unwrap(),
            AnyHarness::Antigravity
        );
    }

    #[cfg(feature = "clap")]
    #[test]
    fn clap_accepts_the_alias() {
        use clap::Parser;
        #[derive(clap::Parser)]
        struct Cli {
            #[arg(long)]
            harness: AnyHarness,
        }
        assert_eq!(
            Cli::parse_from(["x", "--harness", "claude"]).harness,
            AnyHarness::ClaudeCode
        );
        assert_eq!(
            Cli::parse_from(["x", "--harness", "claude-code"]).harness,
            AnyHarness::ClaudeCode
        );
    }

    #[test]
    fn codex_is_still_codex() {
        let p = json!({
            "hook_event_name": "PreToolUse", "turn_id": "t1", "model": "gpt-5",
            "tool_name": "Bash", "tool_input": {"command": "ls"}, "cwd": "/w"
        });
        assert_eq!(AnyHarness::infer(&p), AnyHarness::Codex);
    }

    #[test]
    fn claude_code_is_still_claude_code() {
        let p = json!({
            "hook_event_name": "PreToolUse",
            "tool_name": "Bash", "tool_input": {"command": "ls"}, "cwd": "/w"
        });
        assert_eq!(AnyHarness::infer(&p), AnyHarness::ClaudeCode);
    }

    #[test]
    fn a_codex_payload_is_told_apart_by_its_turn_fields() {
        let p = json!({
            "hook_event_name": "PreToolUse", "tool_name": "Bash", "turn_id": "t1", "model": "m",
            "session_id": "S", "tool_input": { "command": "ls" }, "cwd": "/repo"
        });
        assert_eq!(AnyHarness::infer(&p), AnyHarness::Codex);
    }

    #[test]
    fn cursor_events_infer_as_cursor() {
        let devkit = json!({
            "hook_event_name": "preToolUse",
            "cursor_version": "1.7.0",
            "model": "claude-4.5-sonnet",
            "conversation_id": "c1",
            "tool_name": "Shell",
            "tool_input": {"command": "npm install", "working_directory": "/w"}
        });
        assert_eq!(AnyHarness::infer(&devkit), AnyHarness::Cursor);
        let stripped = json!({"hook_event_name": "beforeShellExecution", "command": "ls"});
        assert_eq!(AnyHarness::infer(&stripped), AnyHarness::Cursor);
        let future = json!({"hook_event_name": "beforeBrand", "cursor_version": "9.0.0"});
        assert_eq!(AnyHarness::infer(&future), AnyHarness::Cursor);
    }

    #[test]
    fn antigravity_is_told_apart_by_its_camel_case_conversation_id() {
        let p = json!({"conversationId": "c1", "toolCall": {"name": "run_command", "args": {}}});
        assert_eq!(AnyHarness::infer(&p), AnyHarness::Antigravity);
        let cursor = json!({"hook_event_name": "stop", "conversation_id": "c1"});
        assert_eq!(AnyHarness::infer(&cursor), AnyHarness::Cursor);
    }

    #[test]
    fn claude_hooks_run_by_cursor_infer_as_claude_code() {
        let herdr = json!({
            "hook_event_name": "SessionStart",
            "session_id": "cursor-session",
            "cursor_version": "2026.08.11-e8db854"
        });
        assert_eq!(AnyHarness::infer(&herdr), AnyHarness::ClaudeCode);
        let brand = json!({"hook_event_name": "Brand", "cursor_version": "9.0.0", "model": "m"});
        assert_eq!(AnyHarness::infer(&brand), AnyHarness::ClaudeCode);
    }

    #[test]
    fn codex_session_end_infers_as_claude_code() {
        let p = json!({
            "hook_event_name": "SessionEnd", "session_id": "s", "cwd": "/w",
            "reason": "exit", "transcript_path": "/t.jsonl"
        });
        assert_eq!(AnyHarness::infer(&p), AnyHarness::ClaudeCode);
    }

    #[test]
    fn an_event_only_one_harness_sends_decides() {
        let message = json!({
            "hook_event_name": "MessageDisplay", "session_id": "s", "cwd": "/w",
            "turn_id": "t1", "message_id": "m1", "index": 0, "final": true, "delta": ""
        });
        assert_eq!(AnyHarness::infer(&message), AnyHarness::ClaudeCode);
        let interrupt = json!({"hook_event_name": "Interrupt", "session_id": "s"});
        assert_eq!(AnyHarness::infer(&interrupt), AnyHarness::Codex);
    }

    #[test]
    fn null_fields_count_as_absent() {
        let p = json!({
            "hook_event_name": "PreToolUse", "session_id": "s",
            "model": null, "turn_id": null, "cursor_version": null
        });
        assert_eq!(AnyHarness::infer(&p), AnyHarness::ClaudeCode);
        let p = json!({"hook_event_name": "PreToolUse", "turn_id": "t", "cursor_version": null});
        assert_eq!(AnyHarness::infer(&p), AnyHarness::Codex);
    }

    #[test]
    fn claude_session_start_with_model_infers_as_codex() {
        let p = json!({
            "hook_event_name": "SessionStart", "session_id": "s", "cwd": "/w",
            "source": "startup", "model": "claude-opus-5-5"
        });
        assert_eq!(AnyHarness::infer(&p), AnyHarness::Codex);
    }
}
