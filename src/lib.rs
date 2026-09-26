//! Typed hook payloads and responses for coding-agent harnesses.

mod error;
mod event;
mod harness;
mod payload;

pub use error::Error;
pub use event::{AnyEvent, ClaudeCodeEvent, CodexEvent, EventKind};
pub use harness::{AnyHarness, ClaudeCode, Codex, Harness};
pub use payload::{AnyPayload, Fields, Payload};
