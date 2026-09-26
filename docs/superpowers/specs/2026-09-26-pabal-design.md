# pabal design

Status: draft, awaiting review
Date: 2026-09-26

## Problem

Three projects parse coding-agent hook payloads and write hook responses, each with its own copy of the same vendor knowledge:

| Project | Where | What it duplicates |
|---|---|---|
| devkit | `crates/devkit-common/src/harness.rs`, `src/bin/devkit/hook/` | `Harness` enum (Claude Code, Codex, Cursor), harness inference from payload fields, shell payload parsing, subagent-vs-fork rule, deny and context envelopes per harness, `SHELL_TOOLS` |
| mcpls | `crates/mcpls-cli/src/hook.rs` | `Harness` enum (Claude Code, Codex), `HookEvent` serde wire names, `HookPayload`/`ToolCall`/`ToolInput` structs, `writes_a_file` tool set, `additionalContext` envelope |
| alacritree | `src/tasks/hook.rs`, `crates/alacritree_tasks/src/scope.rs` | `Harness` enum (Claude, Codex), `Event` with hand-written wire names, `Payload { session_id, cwd }`, `additionalContext` envelope |

The copies have already drifted: the harness flag is `claude-code` in devkit, `claude-code` with a `claude` alias in mcpls, and `claude` in alacritree. Each vendor change (Cursor starting to send `model`, Codex folding `Interrupt` into its stop handling, Claude forks with no `agent_type`) has to be found and fixed three times.

`pabal` holds that knowledge once: the wire protocol between a coding-agent harness and a hook command.

## Scope

In scope for v0.1.0:

- Parsing hook stdin payloads from Claude Code, Codex and Cursor.
- Identifying the harness, from a caller-supplied value or from the payload.
- A typed view of the tool call inside tool events.
- Building the response JSON each harness reads from hook stdout.
- A fixture corpus and CI that detects vendor drift.

Out of scope:

- Installing hooks into harness config files. `agent-config` exists for that; `pabal` does not compete with it.
- Reading stdin, choosing exit codes, logging, timeouts, fail-open policy. Those are each consumer's decisions.
- Migrating devkit, mcpls and alacritree. Each is a separate issue in its own repo (see Rollout).
- Harnesses beyond the three above.

## Prior art reviewed

| Crate / project | Why `pabal` does not use it |
|---|---|
| `agent-config` | Install side only; no payload or response handling. Used as reference data for per-harness event and matcher names. |
| `coding-agent-hooks` (empathic/clash) | Runtime crate, but its Codex adapter emits `decision: proceed/block/modify`, which Codex's schema does not accept; payload fields default to empty strings; rewrites tool names into Claude's spelling; no Cursor. Its `HookProtocol` trait split is a useful reference. |
| `agent-hooks` (weykon) | Install and bridge script; lists Codex events that do not exist. |
| `agent-hooks-sdk` (responsibleai) | In-process interception contract for agent frameworks (LangChain and similar), not vendor hook payloads. |
| herdr `src/integration/` | Per-harness shell scripts, no typed model. Its `tests/cli/hooks.rs` payloads are a fixture source. |

## Architecture

### Crate layout

```
pabal/
  src/
    lib.rs          # module wiring, crate docs
    prelude.rs      # glob-importable re-exports
    harness/        # Harness trait, ClaudeCode, Codex, Cursor, AnyHarness, inference
    event.rs        # per-harness event enums, AnyEvent
    payload.rs      # Payload<H>, common fields, raw access
    view.rs         # per-harness View enums, per-event view types, AnyView
    tool.rs         # Tool, Edit, per-harness tool-name tables, apply_patch header parsing
    response.rs     # Response, response traits per event view
    error.rs        # pabal::Error
  tests/
    fixtures/<harness>/<event>/*.json   # real payloads
    fixtures.rs     # every fixture parses and narrows to the expected view
  docs/superpowers/specs/
```

### Harnesses

Each harness is a zero-sized type implementing `Harness`. The trait carries the harness's wire vocabulary as associated types.

```rust
pub struct ClaudeCode;
pub struct Codex;
pub struct Cursor;

#[ambassador::delegatable_trait]
pub trait Harness {
    type Event: EventKind;
    type View<'a>: ViewKind<'a>;
    fn name(&self) -> &'static str;
    // plus crate-private hooks for the tool-name table and envelope builders
}
```

`AnyHarness` is the runtime choice, for consumers that learn the harness from a flag or from the payload:

```rust
#[derive(ambassador::Delegate)]
#[delegate(Harness)]
pub enum AnyHarness { ClaudeCode(ClaudeCode), Codex(Codex), Cursor(Cursor) }
```

Associated types do not delegate through an enum, so `AnyHarness` exposes the shared forms `AnyEvent` and `AnyView` (below), and each harness's own types convert into them.

`ambassador` is used over `enum_dispatch`: `enum_dispatch` links traits to enums through a global registry shared between macro invocations, which can silently generate no impls depending on expansion order.

### Identifying the harness

Both paths exist, and the explicit one wins:

1. **Explicit.** The consumer names the harness (typically a `--harness` flag in its hook manifest) and parses with `Payload::<Codex>::parse` or `AnyHarness::from_str`.
2. **Inferred.** `AnyHarness::infer(&raw)` reads positive evidence each harness sends, extracted from devkit's `infer_harness`: `cursor_version` means Cursor; `turn_id` or `model` without it means Codex; otherwise Claude Code.

Canonical names are `claude-code`, `codex`, `cursor`, with `claude` accepted as an alias for `claude-code` so alacritree's installed manifests keep working. `strum` derives the string forms; the optional `clap` feature adds `ValueEnum`.

### Events

Each harness has its own event enum, spelled as that harness sends `hook_event_name`, with a catch-all:

```rust
#[derive(strum::EnumString, strum::IntoStaticStr, ...)]
pub enum CodexEvent {
    PreToolUse, PostToolUse, PermissionRequest,
    SessionStart, SessionEnd, UserPromptSubmit,
    Stop, Interrupt, SubagentStart, SubagentStop,
    PreCompact, PostCompact,
    #[strum(default)]
    Other(String),
}
```

Event enums are **not** `#[non_exhaustive]`. A new vendor event ships as a new variant, which is a breaking change (a 0.x minor bump) and makes every consumer's exhaustive `match` fail to compile at the spot that must handle it. Parsing never fails on an unknown event: it becomes `Other`.

`AnyEvent` is the shared form (`PreToolUse`, `SessionStart`, ..., `Other(String)`). Each harness event converts into it; Cursor's `sessionStart` and Claude's `SessionStart` both become `AnyEvent::SessionStart`.

v0.1.0 seed lists (the drift CI is the authority after release):

| Harness | Events | Source |
|---|---|---|
| Claude Code | PreToolUse, PostToolUse, PostToolUseFailure, PostToolBatch, UserPromptSubmit, SessionStart, SessionEnd, SubagentStart, SubagentStop, PermissionRequest, PermissionDenied, Stop, StopFailure, PreCompact, PostCompact, Notification, CwdChanged, WorktreeCreate, WorktreeRemove | devkit `hooks/hooks.json`, mcpls `HookEvent`, herdr fixtures |
| Codex | PreToolUse, PostToolUse, PermissionRequest, SessionStart, SessionEnd, UserPromptSubmit, Stop, Interrupt, SubagentStart, SubagentStop, PreCompact, PostCompact | `codex-rs/hooks/schema/generated/*.command.input.schema.json` at rust-v0.155.1 |
| Cursor | sessionStart, sessionEnd, preToolUse, postToolUse, postToolUseFailure, subagentStart, subagentStop, beforeShellExecution, afterShellExecution, beforeMCPExecution, afterMCPExecution, beforeReadFile, afterFileEdit, beforeSubmitPrompt, preCompact, stop, beforeTabFileRead, afterTabFileEdit, afterAgentResponse, afterAgentThought, workspaceOpen | [Cursor hooks docs](https://cursor.com/docs/hooks), read 2026-09-26 |

### Payloads

`Payload<H>` holds the parsed event, the common fields, and the raw JSON:

```rust
pub struct Payload<H: Harness> {
    event: H::Event,
    raw: serde_json::Value,
    // common fields, all Option
}

impl<H: Harness> Payload<H> {
    pub fn parse(stdin: &str) -> Result<Self, Error>;
    pub fn event(&self) -> &H::Event;
    pub fn session_id(&self) -> Option<&str>;   // Cursor: conversation_id
    pub fn cwd(&self) -> Option<&Path>;
    pub fn transcript_path(&self) -> Option<&Path>;
    pub fn agent(&self) -> Option<&str>;        // subagent id, see below
    pub fn tool(&self) -> Option<Tool<'_>>;
    pub fn view(&self) -> H::View<'_>;
    pub fn raw(&self) -> &serde_json::Value;
}
```

Every field is `Option`. An absent field is `None`, never `""`. Codex sends `transcript_path: null` on some sessions (alacritree fixture `codex-session-start.json`). Anything `pabal` does not model stays reachable through `raw()`.

Field fallbacks per harness move here from devkit's `parse_shell_payload`: Cursor names the session `conversation_id` and the parent `parent_conversation_id`. Every Cursor hook also carries `generation_id`, `model`, `model_id`, `model_params`, `cursor_version`, `workspace_roots`, `user_email` and `transcript_path` ([Cursor hooks docs](https://cursor.com/docs/hooks)).

`agent()` carries devkit's subagent rule: a Claude Code payload speaks for a subagent only when it has both `agent_id` and a non-empty `agent_type`. Claude forks side conversations (progress summaries, prompt suggestions) under a fresh `agent_id` with no `agent_type`, and a fork can end without `SubagentStop`, so a fork speaks for its session.

### Tool view

`payload.tool()` returns a typed view of the tool call on tool events, normalized across harnesses. The original `tool_name` is never rewritten.

```rust
pub enum Tool<'a> {
    Shell { command: &'a str, cwd: Option<&'a Path>, shell: Option<ShellKind> },
    Edit(Edit<'a>),
    Mcp { server: Option<&'a str>, tool: &'a str, input: &'a Value },
    Other { name: &'a str, input: &'a Value },
}

pub enum Edit<'a> {
    Write { path: &'a Path },   // Claude Write/Edit/MultiEdit/NotebookEdit, Cursor Write/Edit
    Patch { patch: &'a str },   // Codex apply_patch
    Delete { path: &'a Path },  // Cursor Delete
}

impl Edit<'_> {
    pub fn paths(&self) -> Vec<PathBuf>;  // Patch: parsed from *** Add/Update/Delete File: and *** Move to: headers
}

#[derive(strum::EnumString, strum::IntoStaticStr, ...)]
pub enum ShellKind {
    Bash,
    PowerShell,
    #[strum(default)]
    Other(String),
}
```

`shell` is `Some` only when the payload names the shell directly, and `None` otherwise. `pabal` never guesses it from the platform or the hook process's environment. On Claude Code's Windows `PowerShell` tool the hook itself runs under Git Bash, so the environment describes the hook, not the command.

| Harness | `shell` |
|---|---|
| Claude Code | From `tool_name`: `Bash` gives `Bash`, `PowerShell` gives `PowerShell` |
| Codex | `None`. Its `Bash` tool name is the historical label for every shell tool (`core/src/tools/hook_names.rs`), and it runs PowerShell on Windows. The `shell` argument of `exec_command` is not in the hook input. |
| Cursor | `None`. Neither `beforeShellExecution` nor the `preToolUse` Shell payload has a shell field. |

Per-harness mapping (the tables live in `tool.rs`):

| Variant | Claude Code | Codex | Cursor |
|---|---|---|---|
| `Shell` | `Bash`, `PowerShell` with `tool_input.command` | `Bash` (from `exec_command`) with `tool_input.command`; `cwd` from the payload, since `exec_command` has no workdir argument | `preToolUse` `Shell` with `tool_input.command` and `tool_input.working_directory`; `beforeShellExecution` with top-level `command` and `cwd` |
| `Edit::Write` | `Write`, `Edit`, `MultiEdit`, `NotebookEdit` | none | `Write`, `Edit` |
| `Edit::Patch` | none | `apply_patch` with `tool_input.command` = raw patch | none |
| `Edit::Delete` | none | none (inside `Patch`) | `Delete` |
| `Mcp` | `mcp__<server>__<tool>` | `mcp__<server>__<tool>` | `MCP:<tool>` (no server) |
| `Other` | everything else | everything else | everything else |

`Tool` and `Edit` are not `#[non_exhaustive]`, matching events. There is no `Read` variant in v0.1.0: Codex has no file-read tool and reads through `Bash`, so a `Read` variant would silently miss every Codex read. It can be added when a consumer needs it, documented as Claude and Cursor only.

### Views

`payload.view()` narrows the payload to a per-event view type. Each harness has its own `View` enum with only the events it sends, so matching on it is exhaustive:

```rust
match payload.view() {                     // View<'_, Codex>
    View::PreToolUse(pre) => pre.deny("locked by session abc"),
    View::SessionStart(start) => start.add_context(text),
    View::UserPromptSubmit(prompt) => prompt.add_context(text),
    // one arm per Codex event
    View::Other(raw) => Response::none(),
}
```

Shortcut accessors (`payload.pre_tool_use() -> Option<PreToolUse<'_, H>>`, one per event) serve consumers that handle a single event.

`AnyView` is the shared form for `AnyHarness`, with the same relationship to per-harness views that `AnyEvent` has to per-harness events.

### Responses

Response methods live on the view types, through traits implemented only for the (harness, event) pairs that support them. A response the harness would not accept does not compile.

| Trait | Method | Implemented for |
|---|---|---|
| `Deny` | `deny(reason) -> Response` | Claude Code and Codex `PreToolUse`: `hookSpecificOutput.permissionDecision = "deny"` with `permissionDecisionReason`. Cursor `preToolUse` and `beforeShellExecution`: `permission: "deny"`, reason in `agent_message` (only that field reaches the agent), `continue: true`. |
| `AddContext` | `add_context(text) -> Response` | `hookSpecificOutput.additionalContext` on Codex `PreToolUse`, `PostToolUse`, `UserPromptSubmit`, `SessionStart`, `SubagentStart` (the Codex output schemas that declare it), and on the same events plus `PostToolBatch` for Claude Code. Not Cursor `preToolUse`, which has no such channel. |
| `Ask` | `ask(reason) -> Response` | Claude Code and Codex `PreToolUse` (`permissionDecision = "ask"`; Codex's schema lists `allow`, `deny`, `ask`). |
| `Allow` | `allow_skipping_prompt() -> Response` | Claude Code and Codex `PreToolUse`. The name is deliberate: an explicit allow bypasses the user's own permission prompt, which devkit avoids on purpose. |

`PermissionRequest` has its own `decision.behavior` response shape and no current consumer, so v0.1.0 exposes its view without response traits.

`Response::none()` is an empty stdout. `Response` serializes to the exact JSON for its harness; `Display` writes it.

On `AnyView`, methods that only some harnesses support return `Option<Response>`.

Codex's legacy `decision: approve|block` field is not emitted; `pabal` writes the `hookSpecificOutput` form both Claude Code and Codex read.

### Errors

One `pabal::Error` (via `thiserror`) for input that is not a JSON object. Unknown events, unknown fields, missing fields and unknown tools never error. Consumers decide how to fail; all three current consumers fail open.

### Prelude

`pabal::prelude` re-exports what a hook command needs:

```rust
use pabal::prelude::*;
// ClaudeCode, Codex, Cursor, AnyHarness, Harness,
// Payload, View, AnyView, AnyEvent, Tool, Edit, ShellKind,
// Response, Deny, AddContext, Ask, Allow
```

### Dependencies

| Crate | Why |
|---|---|
| `serde`, `serde_json` | payloads and responses |
| `strum` | event and harness string forms |
| `ambassador` | `AnyHarness` delegation |
| `thiserror` | `Error` |
| `clap` (optional, `clap` feature) | `ValueEnum` on harness types for consumers' `--harness` flags |

## Testing

- **Fixture corpus.** `tests/fixtures/<harness>/<event>/*.json`, seeded from alacritree `tests/fixtures/hook/`, herdr `tests/cli/hooks.rs`, devkit's hook tests, and Codex's generated schemas. Every fixture must parse, infer to its harness, and narrow to the expected view.
- **Response snapshots.** Each response trait impl has a test asserting the exact JSON, including the Cursor deny envelope and the absence of `permissionDecision` in context-only responses.
- **Schema validation.** Codex responses are validated against `*.command.output.schema.json` from the pinned Codex release.
- **Extracted behavior.** devkit's existing tests for `infer_harness`, `subagent_id` and `parse_shell_payload` move into `pabal` with their cases intact.
- **Doctests** for the prelude example and each public entry point.

## CI

Per push and PR, on ubuntu, macos and windows: `cargo nextest run`, `cargo test --doc`, `cargo clippy --all-targets -- -D warnings`, `cargo fmt --check`, and an MSRV build.

### Drift job

A scheduled workflow (weekly) checks each harness against its upstream source:

| Harness | Source | Check |
|---|---|---|
| Codex | latest `rust-v*` release tag of `openai/codex`, `codex-rs/hooks/schema/generated/` | Event list equals `CodexEvent`; fixtures validate against input schemas; responses validate against output schemas |
| Claude Code | hook input types in the latest `@anthropic-ai/claude-agent-sdk` `.d.ts`, plus the hooks docs page | Event list equals `ClaudeCodeEvent`; declared fields are covered by fixtures |
| Cursor | Cursor's hooks docs page | Event list equals `CursorEvent` |

On drift, the job opens or updates one issue per harness listing what changed, and tags `@claude` in it to open a PR adding fixtures and variants. Each such PR is reviewed by hand, since a new variant is a minor bump for every consumer.

## Release

- Published to crates.io as `pabal`. License `MIT OR Apache-2.0`, edition 2024, MSRV 1.85.
- 0.x semver: a new event, tool or view variant is a minor bump.
- v0.1.0 ships when the three harnesses' seed lists are covered by fixtures and devkit's extracted tests pass inside `pabal`.

## Rollout

Each consumer migration is a separate issue in its own repo, after v0.1.0 is published:

1. **devkit** first. `pabal` is extracted from it, so devkit's existing hook tests passing unchanged against `pabal` is the parity check. `devkit-common::harness` keeps its config-policy functions (`resolve_rules`, `enforcement_enabled`, ...) and drops the protocol parts.
2. **mcpls**, replacing `Harness`, `HookEvent`, `HookPayload` and `writes_a_file`.
3. **alacritree**, replacing `Harness`, `Event`, `Payload` and `output()`.

## Setup

- Install the Claude GitHub app on `AbysmalBiscuit/pabal` so the drift job's `@claude` tag acts.
- Publish the first crates.io release once the crate has working functionality. crates.io's policies prohibit a crate that "exists only to reserve a name for a prolonged period of time ... without having any genuine functionality, purpose, or significant development activity".

## Open questions

None.
