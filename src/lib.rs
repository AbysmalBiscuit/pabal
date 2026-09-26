//! Typed hook payloads and responses for coding-agent harnesses.

mod error;
mod event;
mod harness;
mod payload;
mod tool;
pub mod view;

pub use error::Error;
pub use event::{AnyEvent, ClaudeCodeEvent, CodexEvent, EventKind};
pub use harness::{AnyHarness, ClaudeCode, Codex, Harness};
pub use payload::{AnyPayload, Fields, Payload};
pub use tool::{Edit, ShellKind, Tool, ToolCall};
pub use view::{AnyView, ClaudeCodeView, CodexView};
