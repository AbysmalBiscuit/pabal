# pabal design

Status: revised after review; Cursor and Antigravity added after v0.1.0 (see Cursor, Antigravity)
Date: 2026-09-26

## Problem

Three projects parse coding-agent hook payloads and write hook responses, each with its own copy of the same vendor knowledge:

| Project | Where | What it duplicates |
|---|---|---|
| devkit | `crates/devkit-common/src/harness.rs`, `src/bin/devkit/hook/` | `Harness` enum (Claude Code, Codex, Cursor), harness inference from payload fields, shell payload parsing, subagent-vs-fork rule, deny and context envelopes per harness, `SHELL_TOOLS` |
| mcpls | `crates/mcpls-cli/src/hook.rs` | `Harness` enum (Claude Code, Codex), `HookEvent` serde wire names, `HookPayload`/`ToolCall`/`ToolInput` structs, `writes_a_file` tool set, `apply_patch` header parsing, `additionalContext` envelope |
| alacritree | `src/tasks/hook.rs`, `crates/alacritree_tasks/src/scope.rs` | `Harness` enum (Claude, Codex), `Event` with hand-written wire names, `Payload { session_id, cwd }`, `additionalContext` envelope |

The copies have already drifted. The harness flag is `claude-code` in devkit, `claude-code` with a `claude` alias in mcpls, and `claude` in alacritree. devkit and mcpls parse `apply_patch` headers by different rules. devkit reads `file_path` for Claude's `NotebookEdit`, whose path field is `notebook_path`. Each vendor change (a new event, a new payload field, Claude forks with no `agent_type`) has to be found and fixed three times.

`pabal` holds that knowledge once: the wire protocol between a coding-agent harness and a hook command.

## Scope

In scope for v0.1.0:

- Parsing hook stdin payloads from Claude Code and Codex.
- Identifying the harness, from a caller-supplied value or from the payload.
- A typed view of the tool call inside tool events.
- Building the response JSON each harness reads from hook stdout.
- A fixture corpus and CI that detects vendor drift.

Added after v0.1.0: Cursor and Google Antigravity (see Cursor, Antigravity).

Out of scope:

- Installing hooks into harness config files. `agent-config` exists for that; `pabal` does not compete with it.
- Reading stdin, choosing exit codes, logging, timeouts, fail-open policy. Those are each consumer's decisions.
- Mapping events onto a consumer's own verbs (devkit runs Codex `Interrupt` through its stop handler; alacritree translates WSL paths). Those stay in the consumer.
- Migrating devkit, mcpls and alacritree. Each is a separate issue in its own repo (see Rollout).

## Prior art reviewed

| Crate / project | Why `pabal` does not use it |
|---|---|
| `agent-config` | Install side only; no payload or response handling. Used as reference data for per-harness event and matcher names. |
| `coding-agent-hooks` (empathic/clash) | Runtime crate, but its Codex adapter emits `decision: proceed/block/modify`, which Codex's schema does not accept; payload fields default to empty strings; rewrites tool names into Claude's spelling. Its `HookProtocol` trait split is a useful reference. |
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
    harness/        # Harness trait, ClaudeCode, Codex, Cursor, Antigravity, AnyHarness, inference
    event.rs        # per-harness event enums, AnyEvent
    payload.rs      # Payload<H>, Fields, AnyPayload
    view.rs         # per-harness view enums, per-event view types, AnyView
    tool.rs         # Tool, Edit, ToolCall, per-harness tool-name tables, apply_patch header parsing
    response.rs     # Response, response traits per event view
    error.rs        # pabal::Error
  tests/
    fixtures/<harness>/<event>/*.json   # real payloads
    fixtures.rs     # every fixture parses and narrows to the expected view
  docs/superpowers/specs/
```

### Harnesses

Each harness is a zero-sized type implementing `Harness`. The trait carries the harness's wire vocabulary as associated types and is used only through generics.

```rust
pub struct ClaudeCode;
pub struct Codex;

pub trait Harness: Sized {
    const KIND: AnyHarness;
    type Event: for<'s> From<&'s str> + Display;
    type View<'a> where Self: 'a;
    fn view<'a>(payload: &'a Payload<Self>) -> Self::View<'a>;
    // plus crate-private hooks for the tool-name table and the subagent rule
}
```

`AnyHarness` is the runtime choice, for consumers that learn the harness from a flag or from the payload. It is a plain unit enum, so `strum` and `clap::ValueEnum` derive on it directly:

```rust
#[derive(Clone, Copy, strum::EnumString, strum::Display, ...)]
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
pub enum AnyHarness {
    #[strum(serialize = "claude-code", serialize = "claude")]
    #[cfg_attr(feature = "clap", value(alias = "claude"))]
    ClaudeCode,
    Codex,
}
```

`Harness` is not delegated through `AnyHarness`. Each harness has different `Event` and `View` types, and neither ambassador nor `enum_dispatch` can give one enum several associated types. ambassador also rejects enums delegating to a trait with a GAT.

Runtime dispatch happens on the parsed payload instead. `AnyPayload` holds one `Payload<H>` per harness and delegates the harness-independent accessors to it through ambassador:

```rust
#[ambassador::delegatable_trait]
pub trait Fields {
    fn harness(&self) -> AnyHarness;
    fn event_name(&self) -> String;
    fn session_id(&self) -> Option<&str>;
    fn cwd(&self) -> Option<&Path>;
    // ... every accessor listed under Payloads
}

impl<H: Harness> Fields for Payload<H> { ... }

#[derive(ambassador::Delegate)]
#[delegate(Fields)]
pub enum AnyPayload { ClaudeCode(Payload<ClaudeCode>), Codex(Payload<Codex>) }
```

`ambassador` is used over `enum_dispatch`: `enum_dispatch` links traits to enums through a global registry shared between macro invocations, which can silently generate no impls depending on expansion order.

### Identifying the harness

Both paths exist, and the explicit one wins:

1. **Explicit.** The consumer names the harness (typically a `--harness` flag in its hook manifest) and parses with `Payload::<Codex>::parse(stdin)` or `AnyPayload::parse(AnyHarness::Codex, stdin)`.
2. **Inferred.** `AnyHarness::infer(&raw)` reads positive evidence each harness sends, extracted from devkit's `infer_harness`. `conversationId` (camelCase) means Antigravity. A Cursor event name means Cursor, and so does any camelCase event with `cursor_version`. A PascalCase event with `cursor_version` reads as Claude Code: Cursor's docs say it sends Claude's event names to hooks configured for Claude Code, although the Cursor CLI sends them its own payload (see Cursor). Otherwise an event only one harness sends decides, then `turn_id` or `model` means Codex, and anything else is Claude Code.

Inference is best-effort, and its docs name the two payload shapes it gets wrong. Codex `SessionEnd` carries neither `turn_id` nor `model` (its input schema requires only `cwd`, `hook_event_name`, `reason`, `session_id`, `transcript_path`), so it infers as Claude Code. Claude Code `SessionStart` sometimes carries `model`, so it infers as Codex. Consumers whose manifests can pass `--harness` should.

Canonical names are `claude-code`, `codex`, `cursor` and `antigravity`, with `claude` accepted as an alias for `claude-code` so alacritree's installed manifests keep working. `strum` derives the string forms; the optional `clap` feature adds `ValueEnum`.

### Events

Each harness has its own event enum, spelled as that harness sends `hook_event_name`, with a catch-all:

```rust
#[derive(strum::EnumString, strum::Display, ...)]
pub enum CodexEvent {
    PreToolUse, PostToolUse, PermissionRequest,
    SessionStart, SessionEnd, UserPromptSubmit,
    Stop, Interrupt, SubagentStart, SubagentStop,
    PreCompact, PostCompact,
    #[strum(default)]
    Other(String),
}
```

`Display` writes the wire name, including the inner string of `Other`. `IntoStaticStr` is not derived: it would turn `Other(s)` into the literal `"Other"`.

Event enums are **not** `#[non_exhaustive]`. A new vendor event ships as a new variant, which is a breaking change (a 0.x minor bump) and makes every consumer's exhaustive `match` fail to compile at the spot that must handle it. Parsing never fails on an unknown event: it becomes `Other`.

v0.1.0 seed lists come from each vendor's own source, so the first drift run does not start with a breaking bump. The drift CI is the authority after release.

| Harness | Events | Source |
|---|---|---|
| Claude Code | SessionStart, Setup, UserPromptSubmit, UserPromptExpansion, PreToolUse, PermissionRequest, PermissionDenied, PostToolUse, PostToolUseFailure, PostToolBatch, Notification, MessageDisplay, SubagentStart, SubagentStop, TaskCreated, TaskCompleted, Stop, StopFailure, TeammateIdle, InstructionsLoaded, ConfigChange, CwdChanged, DirectoryAdded, FileChanged, WorktreeCreate, WorktreeRemove, PreCompact, PostCompact, PreModelSwitch, PostModelSwitch, Elicitation, ElicitationResult, SessionEnd | [Claude Code hooks reference](https://code.claude.com/docs/en/hooks) and `HOOK_EVENTS` in `@anthropic-ai/claude-agent-sdk` 0.3.283, read 2026-09-26 |
| Codex | PreToolUse, PostToolUse, PermissionRequest, SessionStart, SessionEnd, UserPromptSubmit, Stop, Interrupt, SubagentStart, SubagentStop, PreCompact, PostCompact | `codex-rs/hooks/schema/generated/*.command.input.schema.json` at rust-v0.155.1 |
| Antigravity | PreToolUse, PostToolUse, PreInvocation, PostInvocation, Stop | [Antigravity hooks docs](https://antigravity.google/docs/hooks.md), read 2026-09-27 |
| Cursor | sessionStart, sessionEnd, preToolUse, postToolUse, postToolUseFailure, subagentStart, subagentStop, beforeShellExecution, afterShellExecution, beforeMCPExecution, afterMCPExecution, beforeReadFile, afterFileEdit, beforeSubmitPrompt, preCompact, stop, afterAgentResponse, afterAgentThought, beforeTabFileRead, afterTabFileEdit, workspaceOpen | [Cursor hooks reference](https://cursor.com/docs/hooks.md), read 2026-09-27 |

Every event gets a variant. Only the events a consumer handles get a dedicated view (see Views); the rest narrow to a generic view over the raw payload.

`AnyEvent` is the shared form: one variant per event more than one harness sends, and `Other(String)` carrying the vendor name for the rest. Claude Code and Codex spell shared events the same way; Cursor spells them in camelCase and calls `UserPromptSubmit` `beforeSubmitPrompt`.

| `AnyEvent` | Claude Code and Codex event | Cursor event |
|---|---|---|
| `SessionStart`, `SessionEnd`, `PreToolUse`, `PostToolUse`, `SubagentStart`, `SubagentStop`, `Stop`, `PreCompact` | the event of the same name | the same name in camelCase |
| `UserPromptSubmit` | `UserPromptSubmit` | `beforeSubmitPrompt` |
| `PermissionRequest`, `PostCompact` | the event of the same name | none |
| `Other(name)` | Codex `Interrupt` and every Claude-only event | every other Cursor event |

### Payloads

`Payload<H>` holds the parsed event and the raw JSON. Its accessors read from the raw JSON through the per-harness field tables, and are the `Fields` trait that `AnyPayload` delegates:

```rust
pub struct Payload<H: Harness> {
    event: H::Event,
    raw: serde_json::Value,
}

impl<H: Harness> Payload<H> {
    pub fn parse(stdin: &str) -> Result<Self, Error>;
    pub fn event(&self) -> &H::Event;
    pub fn view(&self) -> H::View<'_>;
}

// Fields, implemented for every Payload<H>:
fn harness(&self) -> AnyHarness;
fn any_event(&self) -> AnyEvent;
fn event_name(&self) -> String;
fn session_id(&self) -> Option<&str>;
fn cwd(&self) -> Option<&Path>;
fn transcript_path(&self) -> Option<&Path>;
fn agent_id(&self) -> Option<&str>;      // raw agent_id
fn agent(&self) -> Option<&str>;         // subagent the payload speaks for, see below
fn tool_use_id(&self) -> Option<&str>;
fn tool(&self) -> Option<Tool<'_>>;
fn raw(&self) -> &serde_json::Value;
```

Every accessor returns `Option`. An absent field, a JSON `null`, a wrong-typed value and an empty string are all `None`. Codex sends `transcript_path: null` on some sessions (alacritree fixture `codex-session-start.json`). Anything `pabal` does not model stays reachable through `raw()`: `tool_response` (untyped in Codex's schema), `prompt`, `permission_suggestions`, `exit_code`.

Each accessor reads the harness's own spelling of its field, from a per-harness key table (`Harness::KEYS`, hidden). Claude Code and Codex use the field of the accessor's name. `tool_use_id()` is on tool events for all three harnesses; Claude omits it on `PermissionRequest`.

| Accessor | Cursor keys |
|---|---|
| `session_id()` | `session_id`, else `conversation_id`. Cursor sends `session_id` only on `sessionStart` and `sessionEnd`, where it equals `conversation_id`. |
| `agent_id()` | `subagent_id` |
| `agent()` | `subagent_id` when `subagent_type` is also set |
| `cwd()`, `transcript_path()`, `tool_use_id()` | the field of the same name. Cursor sends `cwd` only on tool and shell events. |

`agent_id()` is the raw field. `agent()` applies the subagent rule, the same for both harnesses: `agent_id` only when `agent_type` is also present and non-empty. Claude forks side conversations (progress summaries, prompt suggestions) under a fresh `agent_id` with no `agent_type`, and a fork can end without `SubagentStop`, so a fork speaks for its session. Codex's schemas require `agent_id` and `agent_type` on `SubagentStart`/`SubagentStop` and make them optional elsewhere.

mcpls reads raw `agent_id` today. Its migration picks between `agent_id()` and `agent()` deliberately, since `agent()` stops attributing Claude forks.

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
    Write { path: &'a Path },
    Patch { patch: &'a str },
}

impl Edit<'_> {
    pub fn paths(&self) -> Vec<PathBuf>;
}

#[derive(strum::EnumString, strum::Display, ...)]
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
| Codex | `None`. Its `Bash` tool name is the historical label for every shell tool (`core/src/tools/hook_names.rs`). The `shell` argument of `exec_command` is not in the hook input. |
| Cursor | `None`. Its `Shell` tool names no shell. |

Per-harness mapping (the tables live in `tool.rs`):

| Variant | Claude Code | Codex | Cursor |
|---|---|---|---|
| `Shell` | `Bash`, `PowerShell` with `tool_input.command`; `cwd` from the payload | `Bash` (from `exec_command`) with `tool_input.command`. The hook input is `{command}` only (plus an optional `description`); `exec_command`'s own `workdir` argument is not passed, so `cwd` is the payload's `cwd` and misses a command run with a different `workdir`. | `Shell` with `tool_input.command`; `cwd` is `tool_input.cwd`, else the payload's `cwd`. `beforeShellExecution` and `afterShellExecution` with top-level `command` and `cwd`. |
| `Edit::Write` | `Write`, `Edit`, `MultiEdit` with `tool_input.file_path`; `NotebookEdit` with `tool_input.notebook_path` | none | `Write` with `tool_input.file_path`. The CLI has no separate edit tool, so every edit is a `Write`. |
| `Edit::Patch` | none | `apply_patch` with `tool_input.command` = raw patch | none |
| `Mcp` | `mcp__<server>__<tool>`; `server` from the `mcp_server.name` object when present, else split from the name | `mcp__<server>__<tool>` | `beforeMCPExecution` and `afterMCPExecution`: `tool_name` with `server` from `mcp_server_name`, `input` the JSON string Cursor sends. `MCP:<tool>` on `preToolUse`, with no server. |
| `Other` | everything else | everything else | everything else, including `Read`, `Grep`, `Delete`, `Task` |

MCP names split at the first `__` after `mcp__`, so `mcp__srv__do__thing` is server `srv`, tool `do__thing`. When Claude's `mcp_server.name` is present, it is the server and the tool is what follows `mcp__<name>__`.

`Shell` needs a non-blank `command`; a shell tool without one is `Other`.

`Edit::paths()` for a `Patch` follows mcpls's parser, the stricter of the two existing ones:

- The first line must be `*** Begin Patch` and the envelope must reach `*** End Patch`; otherwise it names no paths.
- Only an unprefixed `*** <Verb>: <path>` line counts. Patch body lines start with `+`, `-` or a space, so a body line that adds `*** Update File: x` is never read as a header.
- `Add File`, `Update File` and `Delete File` name a path. `Move to` names one only directly after an `Update File` line.

`Tool` and `Edit` are not `#[non_exhaustive]`, matching events. There is no `Read` variant in v0.1.0: Codex has no text-file read tool (only `view_image`) and reads through `Bash`, so a `Read` variant would silently miss every Codex read. It can be added when a consumer needs it, documented as Claude only. There is no `Edit::Delete` either: Claude Code and Codex have no delete tool (Codex deletes inside a patch), and Cursor's `Delete` (`tool_input.file_path`) stays `Other` until a consumer needs it.

Claude's `PostToolBatch` carries several tool calls. Its view has `tool_calls()`, an iterator of `ToolCall<'_> { tool_use_id: Option<&str>, tool: Tool<'_>, response: Option<&Value> }` read from `tool_calls[]`. `payload.tool()` is `None` on it.

### Views

`payload.view()` narrows the payload to a per-event view type. Each harness has its own view enum (`ClaudeCodeView`, `CodexView`, reached generically as `H::View<'a>`) with only the events it sends, so matching on it is exhaustive:

```rust
match payload.view() {                     // CodexView<'_>
    CodexView::PreToolUse(pre) => pre.deny("locked by session abc"),
    CodexView::SessionStart(start) => start.add_context(text),
    CodexView::UserPromptSubmit(prompt) => prompt.add_context(text),
    // one arm per Codex event
    CodexView::Other(raw) => Response::none(),
}
```

Per-event view types are generic where the harnesses share an event (`PreToolUse<'a, H>`) and harness-specific where they do not (`PostToolBatch<'a>`, Cursor's `BeforeShellExecution<'a>` and `BeforeMcpExecution<'a>`). Events without a consumer yet share a generic view over the raw payload.

Shortcut accessors (`payload.pre_tool_use() -> Option<PreToolUse<'_, H>>`, one per shared event) serve consumers that handle a single event. They are bounded by per-event marker traits, so an accessor for an event a harness never sends does not exist, rather than always returning `None`; `Payload::<Codex>::post_tool_batch()` does not compile.

`AnyView`, from `AnyPayload::view()`, is the shared form, with one variant per `AnyEvent` variant. Each wraps a per-harness enum of that event's views, with a variant only for the harnesses that send it: `AnyPermissionRequest` has no Cursor variant.

### Responses

Response methods live on the view types, through traits implemented only for the (harness, event) pairs that support them. A response the harness would not accept does not compile.

| Trait | Method | Implemented for |
|---|---|---|
| `Deny` | `deny(reason) -> Response` | Claude Code and Codex `PreToolUse`: `hookSpecificOutput.permissionDecision = "deny"` with `permissionDecisionReason`. A blank reason is replaced with a fixed one, because Codex treats a blank deny reason as invalid and runs the tool. |
| `AddContext` | `add_context(text) -> Response` | Codex `PreToolUse`, `PostToolUse`, `UserPromptSubmit`, `SessionStart`, `SubagentStart` (the Codex output schemas that declare it), as `hookSpecificOutput.additionalContext`. Claude Code, the same events plus `PostToolBatch`, same envelope. Claude accepts `additionalContext` on more events (`PostToolUseFailure`, `Stop`, `SubagentStop`, and others); v0.1.0 implements the subset consumers use. |
| `Ask` | `ask(reason) -> Response` | Claude Code `PreToolUse` (`permissionDecision = "ask"`). Codex's output schema lists `ask`, but its parser rejects it and runs the tool (`hooks/src/engine/output_parser.rs`). |
| `Allow` | `allow_skipping_prompt() -> Response` | Claude Code `PreToolUse`. Codex accepts `allow` only with `updatedInput` and otherwise runs the tool as if no hook answered. The name is deliberate: an explicit allow bypasses the user's own permission prompt, which devkit avoids on purpose. |
| `RewriteInput` | `rewrite_input(input, context) -> Response` | Claude Code `PreToolUse`: `hookSpecificOutput.updatedInput`, with `additionalContext` when `context` is given and no `permissionDecision`, so Claude Code's permission check runs on the new input. Source: the [Claude Code hooks reference](https://code.claude.com/docs/en/hooks), PreToolUse decision control. Codex `PreToolUse`: the same envelope with `permissionDecision = "allow"`, the only decision beside which Codex applies `updatedInput` (`hooks/src/engine/output_parser.rs`). The call still goes through Codex's own approval. Codex shell and `apply_patch` calls take only `command` from the input and keep their other arguments (`core/src/tools/handlers/mod.rs`). |
| `Block` | `block(reason) -> Response` | Claude Code and Codex `Stop` and `SubagentStop`: `{"decision": "block", "reason": reason}`, which keeps the agent running and gives it the reason as its next prompt. Sources: the [Claude Code hooks reference](https://code.claude.com/docs/en/hooks), Stop decision control, and the vendored Codex `stop` and `subagent-stop` output schemas. A blank reason is replaced with a fixed one, because Codex requires a reason beside `block`. |

Cursor answers with top-level fields instead of `hookSpecificOutput`:

| Trait | Cursor events | Envelope |
|---|---|---|
| `Deny` | `preToolUse`, `beforeShellExecution`, `beforeMCPExecution` | `{"permission": "deny", "user_message": reason, "agent_message": reason}`. The CLI reads only `user_message` and passes it to the agent; hooks Cursor's server runs also read `agent_message`. |
| `Ask` | `beforeShellExecution` | The same envelope with `"permission": "ask"`, which forces the approval prompt and shows `user_message`. The CLI accepts `ask` on `preToolUse` and `beforeMCPExecution` but ignores it, so neither has `Ask`. |
| `AddContext` | `sessionStart`, `preToolUse`, `postToolUse` | `{"additional_context": text}`. Cursor drops text longer than 10,000 characters. |
| `RewriteInput` | `preToolUse` | `{"updated_input": input}`, plus `additional_context` when `context` is given. Source: the [Cursor hooks reference](https://cursor.com/docs/hooks.md), `preToolUse` output. The CLI source applies `updated_input` whenever the hook did not deny, reading only the fields it knows for each tool, such as a shell call's `command`, `cwd` and `timeout`. This answer has not been run against the CLI: the reference's example pairs `updated_input` with `permission`, which `pabal` does not send. |
| `Block` | `stop` | `{"followup_message": reason}`, which Cursor submits as the next prompt, up to the `loop_limit` in `hooks.json`. A blank reason is replaced as on Codex. `subagentStop` has no `Block`: its `followup_message` starts the next iteration once the sub-agent has completed, and does not keep it running. |

devkit sends `continue: true` beside a Cursor deny, copied from Cursor's shell examples. `pabal` does not: Cursor's `preToolUse` schema does not list it, and a response that does not match a permission hook's schema blocks the action.

`Response::deny_pre_tool_use(AnyHarness, reason)` builds the same deny without a payload. devkit denies when stdin is not JSON, and at that point it has only the harness from its flag. Claude Code and Codex get the same envelope, with `hookEventName: "PreToolUse"`; Cursor gets its permission envelope, which also answers `beforeShellExecution` and `beforeMCPExecution`.

`PermissionRequest` has its own `decision.behavior` response shape and no current consumer, so v0.1.0 exposes its view without response traits.

`Response::none()` is an empty stdout. `Response` serializes to the exact JSON for its harness; `Display` writes it.

Responses carry only documented fields. Codex's output wire structs are `#[serde(deny_unknown_fields)]` and require `hookEventName` inside `hookSpecificOutput`.

On `AnyView`, methods that only some harnesses support return `Option<Response>`.

Codex's legacy `decision: approve|block` field is not emitted; `pabal` writes the `hookSpecificOutput` form both Claude Code and Codex read.

### Errors

One `pabal::Error` (via `thiserror`) for input that is not a JSON object. Unknown events, unknown fields, missing fields and unknown tools never error. Consumers decide how to fail; all three current consumers fail open.

### Prelude

`pabal::prelude` re-exports what a hook command needs:

```rust
use pabal::prelude::*;
// ClaudeCode, Codex, Cursor, Antigravity, Harness, AnyHarness,
// Payload, AnyPayload, Fields,
// ClaudeCodeView, CodexView, CursorView, AntigravityView, AnyView, AnyEvent,
// Tool, Edit, ToolCall, ShellKind,
// Response, Deny, AddContext, Ask, Allow, RewriteInput, Block
```

### Dependencies

| Crate | Why |
|---|---|
| `serde_json` | payloads and responses, both handled as `serde_json::Value` |
| `strum` | event, harness and shell string forms |
| `ambassador` | `AnyPayload` delegation of `Fields` |
| `thiserror` | `Error` |
| `clap` (optional, `clap` feature) | `ValueEnum` on `AnyHarness` for consumers' `--harness` flags |

## Testing

- **Fixture corpus.** `tests/fixtures/<harness>/<event>/*.json`, seeded from alacritree `tests/fixtures/hook/`, herdr `tests/cli/hooks.rs`, devkit's hook tests, and Codex's generated schemas. Every fixture must parse and narrow to the expected view. Every fixture must infer to its harness, except fixtures marked ambiguous: a Codex `SessionEnd` and a Claude Code `SessionStart` with `model` assert the documented wrong answer, so a change to the inference rule shows up in review.
- **Response snapshots.** Each response trait impl has a test asserting the exact JSON, including the absence of `permissionDecision` in context-only responses.
- **Schema validation.** Codex responses are validated against `*.command.output.schema.json` from the pinned Codex release.
- **Extracted behavior.** devkit's existing tests for `infer_harness`, `subagent_id` and `parse_shell_payload` move into `pabal` with their Claude Code and Codex cases intact; their Cursor cases move with the Cursor harness. mcpls's `apply_patch` header tests move with them.
- **Doctests** for the prelude example and each public entry point.

## CI

Per push and PR, on ubuntu, macos and windows: `cargo nextest run`, `cargo test --doc`, `cargo clippy --all-targets -- -D warnings`, `cargo fmt --check`, and an MSRV build.

### Drift job

A scheduled workflow (weekly) checks each harness against its upstream source:

| Harness | Source | Check |
|---|---|---|
| Codex | latest `rust-v*` release tag of `openai/codex`, `codex-rs/hooks/schema/generated/` | Event list equals `CodexEvent`; fixtures validate against input schemas; responses validate against output schemas |
| Claude Code | `sdk.d.ts` in the latest `@anthropic-ai/claude-agent-sdk`: the `HOOK_EVENTS` constant and the `<Event>HookInput` types | Event list equals `ClaudeCodeEvent`; each required field of a `<Event>HookInput` appears in that event's fixtures |
| Cursor | `https://cursor.com/docs/hooks.md`, the markdown form of the hooks reference. Cursor publishes no typings or schema. | The `#### <event>` headings under `### Hook events` equal `CursorEvent` |
| Antigravity | `https://antigravity.google/docs/hooks.md`. Antigravity publishes no typings or schema. | The events in the `## Supported Events` table equal `AntigravityEvent` |

On drift, the job opens or updates one issue per harness listing what changed, and tags `@claude` in it to open a PR adding fixtures and variants. Each such PR is reviewed by hand, since a new variant is a minor bump for every consumer.

## Release

- Published to crates.io as `pabal`. License `MIT OR Apache-2.0`, edition 2024, MSRV 1.85.
- 0.x semver: a new event, tool or view variant is a minor bump.
- v0.1.0 ships when every Claude Code and Codex event the three consumers handle has a fixture and a view, and devkit's extracted tests pass inside `pabal`.

## Rollout

Each consumer migration is a separate issue in its own repo, after v0.1.0 is published:

1. **devkit** first. `pabal` is extracted from it, so devkit's existing Claude Code and Codex hook tests passing unchanged against `pabal` is the parity check. `devkit-common::harness` keeps its config-policy functions (`resolve_rules`, `enforcement_enabled`, ...) and drops the protocol parts. Its Cursor path (the Cursor branches of `parse_shell_payload` and `deny_shell_json`, `brief`'s Cursor context) moves onto `pabal`'s `Cursor` harness; `hooks-cursor.json` stays in devkit. Known behavior changes: `NotebookEdit` gains its path, and `apply_patch` parsing adopts mcpls's rules.
2. **mcpls**, replacing `Harness`, `HookEvent`, `HookPayload`, `writes_a_file` and `apply_patch_paths`. Known behavior changes: `writes_a_file` gains `NotebookEdit`, and subagent attribution picks `agent_id()` or `agent()` explicitly.
3. **alacritree**, replacing `Harness`, `Event`, `Payload` and `output()`. Its WSL cwd translation and its treatment of non-JSON stdin as an empty payload stay local.

## Cursor

Cursor is the first harness added after v0.1.0: a `Cursor` type, `CursorEvent`, `CursorView`, `AnyHarness::Cursor`, and Cursor columns in the tables above. Its facts come from the [Cursor hooks reference](https://cursor.com/docs/hooks.md) and the bundled source of the Cursor CLI (`cursor-agent` 2026.09.26-dd393fe), both read 2026-09-27. Most fixtures under `tests/fixtures/cursor/` are payloads captured from the CLI, with paths and email anonymized. Events the capture run did not reach (subagents, compaction, MCP, `afterAgentResponse` and the tab events) keep fixtures built from the reference and the CLI source. Deny, `ask` on `beforeShellExecution` and `additional_context` on `preToolUse` were tried against the CLI.

What the CLI source settles:

- **Tool input.** `Shell` is `{command, cwd, timeout?}`, with `cwd` also at the top level. A command run in the workspace root sends `cwd: ""`, which reads as no cwd. `Write` is `{file_path, content}`, `Delete` and `Read` are `{file_path}`.
- **MCP naming.** `preToolUse` names MCP tools `MCP:<tool>`, with no server field. Only `beforeMCPExecution` and `afterMCPExecution` carry `mcp_server_name`, beside the bare tool name.
- **Ids.** `tool_use_id` is a random UUID for Shell and MCP calls, so it does not match the model's tool-call id. A subagent's tool events carry no subagent id; `subagent_id` appears only on `subagentStart` and `subagentStop`.

The Cursor IDE is separate code from the CLI and is unverified.

What the CLI leaves to the caller:

- **Failure mode.** Exit 2 denies, with stdout or stderr as the message. Other non-zero exits, an empty stdout and timeouts (60 seconds by default) fail open unless the hook sets `failClosed: true`. A permission hook's invalid JSON, or a response that does not match its schema, blocks the action.
- **Claude hooks under Cursor.** The CLI runs hooks configured for Claude Code (`~/.claude/settings.json`, the project's `.claude/settings.json` and `.claude/settings.local.json`), mapping Claude's event names onto its own, and sends them the same camelCase payload as its own hooks, so they parse as Cursor. It reads a Claude `hookSpecificOutput` response back, with `permissionDecisionReason` as `user_message`. The [third-party hooks docs](https://cursor.com/docs/reference/third-party-hooks) say Cursor sends Claude's event names instead, so a PascalCase event with `cursor_version` still reads as Claude Code.
- **Unmodeled responses.** `updated_mcp_tool_output` on `postToolUse`, `env` on `sessionStart`, `followup_message` on `subagentStop`, `continue: false` on `beforeSubmitPrompt`, deny on `beforeReadFile` and `subagentStart`, and `additional_context` on `postToolUseFailure` have no consumer yet, so they have no trait.

## Antigravity

Google Antigravity is the second harness added after v0.1.0: an `Antigravity` type, `AntigravityEvent`, `AntigravityView` and `AnyHarness::Antigravity`. Its facts come from the [Antigravity hooks docs](https://antigravity.google/docs/hooks.md), read 2026-09-27. The fixtures under `tests/fixtures/antigravity/` are payloads captured from the Antigravity CLI (`agy` 1.2.12), with paths anonymized, and the responses below were tried against it.

**The event is not in the payload.** No stdin field names the event, and the only environment variable Antigravity sets for a hook is `ANTIGRAVITY_CONVERSATION_ID`. A hook command learns its event from its own `hooks.json` entry instead, for example `my-hook --event PreToolUse`, and passes it to `Payload::parse_named(event, stdin)` or `AnyPayload::parse_named(harness, event, stdin)`. A `hook_event_name` in the payload still wins, so a harness that sends one reads the same either way, and `raw()` stays the payload as sent. Payloads cannot stand in for the name: `PreInvocation` and `PostInvocation` carry identical fields.

**Fields.** The payload is camelCase. `session_id()` reads `conversationId` and `transcript_path()` reads `transcriptPath`. There is no top-level `cwd`, subagent id or tool-use id, so `cwd()`, `agent_id()`, `agent()` and `tool_use_id()` are `None`; `workspacePaths`, `stepIdx` and the rest stay in `raw()`.

**Events.** `PreToolUse`, `PostToolUse` and `Stop` map to the `AnyEvent` of the same name. `PreInvocation` and `PostInvocation` fire around each model call and are `Other`.

**Tools.** The call is `toolCall: {name, args}`, with PascalCase argument names.

| Variant | Antigravity tool |
|---|---|
| `Shell` | `run_command` with `args.CommandLine`; `cwd` from `args.Cwd`; `shell` is `None` |
| `Edit::Write` | `write_to_file`, `replace_file_content`, `multi_replace_file_content` with `args.TargetFile` |
| `Other` | everything else, with `args` as `input`. The docs name no MCP tools. |

**Responses.** `PreToolUse` answers with `decision` and `reason`:

| Trait | Envelope |
|---|---|
| `Deny` | `{"decision": "deny", "reason": reason}`. The agent sees the reason. |
| `Ask` | `{"decision": "ask", "reason": reason}`. Antigravity shows its approval prompt with the reason. |

`force_ask`, `deny_unless_prior_grant`, `permissionOverrides`, `injectSteps`, `terminationBehavior` and `Stop`'s `decision: "continue"` have no consumer yet, so they have no trait. `allow` has no trait either: the docs say it allows without a prompt, but the CLI still shows its approval prompt after `allow`, with or without `permissionOverrides`. `PostToolUse` has no response fields, so `AnyPostToolUse::add_context` returns `None` for Antigravity. `PreToolUse` has no input field, so there is no `RewriteInput` and `AnyPreToolUse::rewrite_input` returns `None`. Without `decision: "continue"` there is no `Block` on `Stop`, and `AnyStop::block` returns `None`.

**Failure mode.** An empty stdout is no opinion: the tool falls through to Antigravity's own approval policy, so `Response::none()` is safe. Any non-zero exit, and stdout that is not JSON, block the tool with an error the agent sees, so a hook that crashes on Antigravity blocks every tool call.

## Setup

- Install the Claude GitHub app on `AbysmalBiscuit/pabal` so the drift job's `@claude` tag acts.
- Publish the first crates.io release once the crate has working functionality. crates.io's policies prohibit a crate that "exists only to reserve a name for a prolonged period of time ... without having any genuine functionality, purpose, or significant development activity".

## Open questions

None.
