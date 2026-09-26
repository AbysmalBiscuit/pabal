//! Typed hook payloads and responses for coding-agent harnesses.

mod error;
mod event;
mod harness;

pub use error::Error;
pub use event::{AnyEvent, ClaudeCodeEvent, CodexEvent, EventKind};
pub use harness::AnyHarness;
