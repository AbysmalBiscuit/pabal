use std::path::Path;

use serde_json::Value;

use crate::{ClaudeCodeEvent, CodexEvent, EventKind, Payload, Tool};

mod claude_code;
mod codex;

pub use claude_code::ClaudeCode;
pub use codex::Codex;

mod sealed {
    pub trait Sealed {}
}

/// A harness's wire vocabulary, used through generics as `Payload<H>`.
///
/// Sealed: only this crate's harness types implement it.
pub trait Harness: sealed::Sealed + Sized + 'static {
    /// The runtime name of this harness.
    const KIND: AnyHarness;
    /// The harness's event enum.
    type Event: EventKind;

    /// The tool view of `call`, an object with `tool_name` and `tool_input`:
    /// a single-tool payload, or one entry of a batch.
    #[doc(hidden)]
    fn tool<'a>(call: &'a Value, cwd: Option<&'a Path>) -> Option<Tool<'a>>;

    /// The harness's view enum, one variant per event it sends.
    type View<'a>;

    #[doc(hidden)]
    fn view(payload: &Payload<Self>) -> Self::View<'_>;
}

/// A harness chosen at runtime, from a `--harness` flag or from the payload.
///
/// Its string forms are `claude-code` (alias `claude`) and `codex`.
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
}

impl AnyHarness {
    /// Guesses the harness. `cursor_version` (Cursor running a Claude Code
    /// hook) means Claude Code, then an event only one harness sends decides,
    /// then `turn_id` or `model` means Codex. A null field counts as absent.
    ///
    /// Misreads Codex `SessionEnd` (neither field) as Claude Code, and Claude
    /// Code `SessionStart` with `model` as Codex. Prefer an explicit harness.
    ///
    /// ```
    /// use pabal::AnyHarness;
    /// let codex = serde_json::json!({"hook_event_name": "Stop", "turn_id": "t"});
    /// assert_eq!(AnyHarness::infer(&codex), AnyHarness::Codex);
    /// ```
    pub fn infer(raw: &Value) -> AnyHarness {
        let has = |key| raw.get(key).is_some_and(|v| !v.is_null());
        let event = raw.get("hook_event_name").and_then(Value::as_str);
        let claude = event.is_some_and(|e| !matches!(e.parse(), Ok(ClaudeCodeEvent::Other(_))));
        let codex = event.is_some_and(|e| !matches!(e.parse(), Ok(CodexEvent::Other(_))));
        let codex = match (claude, codex) {
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
        assert!("cursor".parse::<AnyHarness>().is_err());
        assert!("Claude".parse::<AnyHarness>().is_err());
        assert_eq!(AnyHarness::ClaudeCode.to_string(), "claude-code");
        assert_eq!(AnyHarness::Codex.to_string(), "codex");
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
    fn cursor_run_claude_hooks_infer_as_claude_code() {
        let devkit = json!({
            "hook_event_name": "preToolUse",
            "cursor_version": "1.7.0",
            "model": "claude-4.5-sonnet",
            "conversation_id": "c1",
            "tool_name": "Shell",
            "tool_input": {"command": "npm install", "working_directory": "/w"}
        });
        assert_eq!(AnyHarness::infer(&devkit), AnyHarness::ClaudeCode);
        let herdr = json!({
            "hook_event_name": "SessionStart",
            "session_id": "cursor-session",
            "cursor_version": "2026.08.11-e8db854"
        });
        assert_eq!(AnyHarness::infer(&herdr), AnyHarness::ClaudeCode);
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
