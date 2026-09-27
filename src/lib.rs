#![doc = include_str!("../README.md")]
#![warn(missing_docs)]

mod error;
mod event;
mod harness;
mod payload;
pub mod prelude;
mod response;
mod tool;
pub mod view;

pub use error::Error;
pub use event::{AnyEvent, ClaudeCodeEvent, CodexEvent, CursorEvent, EventKind};
pub use harness::{AnyHarness, ClaudeCode, Codex, Cursor, Harness};
pub use payload::{AnyPayload, Fields, Payload};
pub use response::{AddContext, Allow, Ask, Deny, Response};
pub use tool::{Edit, ShellKind, Tool, ToolCall};
pub use view::{AnyView, ClaudeCodeView, CodexView, CursorView};
