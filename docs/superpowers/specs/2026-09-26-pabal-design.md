# pabal design

Status: draft, revised after review
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

- Parsing hook stdin payloads from Claude Code, Codex and Cursor.
- Identifying the harness, from a caller-supplied value or from the payload.
- A typed view of the tool call inside tool events.
- Building the response JSON each harness reads from hook stdout.
- A fixture corpus and CI that detects vendor drift.

Out of scope:

- Installing hooks into harness config files. `agent-config` exists for that; `pabal` does not compete with it.
- Reading stdin, choosing exit codes, logging, timeouts, fail-open policy. Those are each consumer's decisions.
- Mapping events onto a consumer's own verbs (devkit runs Codex `Interrupt` through its stop handler; alacritree translates WSL paths). Those stay in the consumer.
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
    payload.rs      # Payload<H>, Fields, AnyPayload
    view.rs         # ClaudeCodeView, CodexView, CursorView, per-event view types, AnyView
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
pub struct Cursor;

pub trait Harness: Sized {
    const KIND: AnyHarness;
    type Event: for<'s> From<&'s str> + Display;
    type View<'a> where Self: 'a;
    fn view<'a>(payload: &'a Payload<Self>) -> Self::View<'a>;
    // plus crate-private hooks for the tool-name table and envelope builders
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
    Cursor,
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
pub enum AnyPayload { ClaudeCode(Payload<ClaudeCode>), Codex(Payload<Codex>), Cursor(Payload<Cursor>) }
```

`ambassador` is used over `enum_dispatch`: `enum_dispatch` links traits to enums through a global registry shared between macro invocations, which can silently generate no impls depending on expansion order.

### Identifying the harness

Both paths exist, and the explicit one wins:

1. **Explicit.** The consumer names the harness (typically a `--harness` flag in its hook manifest) and parses with `Payload::<Codex>::parse(stdin)` or `AnyPayload::parse(AnyHarness::Codex, stdin)`.
2. **Inferred.** `AnyHarness::infer(&raw)` reads positive evidence each harness sends, extracted from devkit's `infer_harness`: `cursor_version` means Cursor; `turn_id` or `model` without it means Codex; otherwise Claude Code.

Inference is best-effort, and its docs name the two payload shapes it gets wrong. Codex `SessionEnd` carries neither `turn_id` nor `model` (its input schema requires only `cwd`, `hook_event_name`, `reason`, `session_id`, `transcript_path`), so it infers as Claude Code. Claude Code `SessionStart` sometimes carries `model`, so it infers as Codex. Consumers whose manifests can pass `--harness` should.

Canonical names are `claude-code`, `codex`, `cursor`, with `claude` accepted as an alias for `claude-code` so alacritree's installed manifests keep working. `strum` derives the string forms; the optional `clap` feature adds `ValueEnum`.

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
| Claude Code | SessionStart, Setup, UserPromptSubmit, UserPromptExpansion, PreToolUse, PermissionRequest, PermissionDenied, PostToolUse, PostToolUseFailure, PostToolBatch, Notification, MessageDisplay, SubagentStart, SubagentStop, TaskCreated, TaskCompleted, Stop, StopFailure, TeammateIdle, InstructionsLoaded, ConfigChange, CwdChanged, DirectoryAdded, FileChanged, WorktreeCreate, WorktreeRemove, PreCompact, PostCompact, PreModelSwitch, PostModelSwitch, Elicitation, ElicitationResult, SessionEnd | [Claude Code hooks reference](https://code.claude.com/docs/en/hooks), read 2026-09-26 |
| Codex | PreToolUse, PostToolUse, PermissionRequest, SessionStart, SessionEnd, UserPromptSubmit, Stop, Interrupt, SubagentStart, SubagentStop, PreCompact, PostCompact | `codex-rs/hooks/schema/generated/*.command.input.schema.json` at rust-v0.155.1 |
| Cursor | sessionStart, sessionEnd, preToolUse, postToolUse, postToolUseFailure, subagentStart, subagentStop, beforeShellExecution, afterShellExecution, beforeMCPExecution, afterMCPExecution, beforeReadFile, afterFileEdit, beforeSubmitPrompt, preCompact, stop, beforeTabFileRead, afterTabFileEdit, afterAgentResponse, afterAgentThought, workspaceOpen | [Cursor hooks docs](https://cursor.com/docs/hooks), read 2026-09-26 |

Every event gets a variant. Only the events a consumer handles get a dedicated view (see Views); the rest narrow to a generic view over the raw payload.

`AnyEvent` is the shared form. It has a variant for each meaning at least two harnesses send, and `Other(String)` carrying the vendor name for the rest:

| `AnyEvent` | Claude Code | Codex | Cursor |
|---|---|---|---|
| `SessionStart` | `SessionStart` | `SessionStart` | `sessionStart` |
| `SessionEnd` | `SessionEnd` | `SessionEnd` | `sessionEnd` |
| `UserPromptSubmit` | `UserPromptSubmit` | `UserPromptSubmit` | `beforeSubmitPrompt` |
| `PreToolUse` | `PreToolUse` | `PreToolUse` | `preToolUse`, `beforeShellExecution`, `beforeMCPExecution` |
| `PostToolUse` | `PostToolUse` | `PostToolUse` | `postToolUse`, `afterShellExecution`, `afterMCPExecution` |
| `PostToolUseFailure` | `PostToolUseFailure` | none | `postToolUseFailure` |
| `PermissionRequest` | `PermissionRequest` | `PermissionRequest` | none |
| `SubagentStart` | `SubagentStart` | `SubagentStart` | `subagentStart` |
| `SubagentStop` | `SubagentStop` | `SubagentStop` | `subagentStop` |
| `Stop` | `Stop` | `Stop` | `stop` |
| `PreCompact` | `PreCompact` | `PreCompact` | `preCompact` |
| `PostCompact` | `PostCompact` | `PostCompact` | none |
| `Other(name)` | everything else | `Interrupt` | everything else, including `workspaceOpen` |

Grouping loses which Cursor event fired; `payload.event()` still has it. A consumer registering both `preToolUse` and `beforeShellExecution` sees one shell command twice, as it does today.

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

Every accessor returns `Option`. An absent field is `None`, never `""`. Codex sends `transcript_path: null` on some sessions (alacritree fixture `codex-session-start.json`). Anything `pabal` does not model stays reachable through `raw()`: `tool_response` (Claude, Codex; untyped in Codex's schema), `tool_output` (Cursor, a JSON string), `prompt`, `permission_suggestions`, `duration`, `exit_code`.

Per-harness field rules:

- `session_id()`: `session_id`; Cursor falls back to `conversation_id`. devkit's fallback to `parent_conversation_id` is not carried over. Cursor documents that field only on `subagentStart`, where it names the parent, not the session.
- `cwd()`: top-level `cwd`, falling back to `tool_input.working_directory`. Cursor's `preToolUse` has no top-level `cwd`, so this keeps devkit's `payload_cwd` behavior.
- `tool_use_id()`: all three harnesses send it on tool events. Claude omits it on `PermissionRequest`.

Every Cursor hook also carries `conversation_id`, `generation_id`, `model`, `model_id`, `model_params`, `cursor_version`, `workspace_roots`, `user_email` and `transcript_path` ([Cursor hooks docs](https://cursor.com/docs/hooks)).

`agent_id()` is the raw field. `agent()` applies the harness's subagent rule:

| Harness | `agent()` |
|---|---|
| Claude Code | `agent_id` only when `agent_type` is also present and non-empty. Claude forks side conversations (progress summaries, prompt suggestions) under a fresh `agent_id` with no `agent_type`, and a fork can end without `SubagentStop`, so a fork speaks for its session. |
| Codex | Same rule. Its schemas require `agent_id` and `agent_type` on `SubagentStart`/`SubagentStop` and make them optional elsewhere. |
| Cursor | `subagent_id` on `subagentStart` and `subagentStop`; `None` elsewhere, since no documented tool-event payload attributes a subagent. |

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
    Delete { path: &'a Path },
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
| Cursor | `None`. Neither `beforeShellExecution` nor the `preToolUse` Shell payload has a shell field. |

Per-harness mapping (the tables live in `tool.rs`):

| Variant | Claude Code | Codex | Cursor |
|---|---|---|---|
| `Shell` | `Bash`, `PowerShell` with `tool_input.command` | `Bash` (from `exec_command`) with `tool_input.command`. The hook input is `{command}` only (plus an optional `description`); `exec_command`'s own `workdir` argument is not passed, so `cwd` is the payload's `cwd` and misses a command run with a different `workdir`. | `preToolUse` `Shell` with `tool_input.command` and `tool_input.working_directory`; `beforeShellExecution` with top-level `command` and `cwd` |
| `Edit::Write` | `Write`, `Edit`, `MultiEdit` with `tool_input.file_path`; `NotebookEdit` with `tool_input.notebook_path` | none | `Write` |
| `Edit::Patch` | none | `apply_patch` with `tool_input.command` = raw patch | none |
| `Edit::Delete` | none | none (inside `Patch`) | `Delete` |
| `Mcp` | `mcp__<server>__<tool>`; `server` from the `mcp_server.name` object when present, else split from the name | `mcp__<server>__<tool>` | `preToolUse` `MCP:<tool>` with no server; `beforeMCPExecution` with `tool_name`, `mcp_server_name`, and `tool_input` as a JSON string |
| `Other` | everything else | everything else | everything else |

`Edit::paths()` for a `Patch` follows mcpls's parser, the stricter of the two existing ones:

- The first line must be `*** Begin Patch` and the envelope must reach `*** End Patch`; otherwise it names no paths.
- Only an unprefixed `*** <Verb>: <path>` line counts. Patch body lines start with `+`, `-` or a space, so a body line that adds `*** Update File: x` is never read as a header.
- `Add File`, `Update File` and `Delete File` name a path. `Move to` names one only directly after an `Update File` line.

`Tool` and `Edit` are not `#[non_exhaustive]`, matching events. There is no `Read` variant in v0.1.0: Codex has no text-file read tool (only `view_image`) and reads through `Bash`, so a `Read` variant would silently miss every Codex read. It can be added when a consumer needs it, documented as Claude and Cursor only.

Claude's `PostToolBatch` carries several tool calls. Its view has `tool_calls()`, an iterator of `ToolCall<'_> { tool_use_id: Option<&str>, tool: Tool<'_>, response: Option<&Value> }` read from `tool_calls[]`. `payload.tool()` is `None` on it.

### Views

`payload.view()` narrows the payload to a per-event view type. Each harness has its own view enum (`ClaudeCodeView`, `CodexView`, `CursorView`, reached generically as `H::View<'a>`) with only the events it sends, so matching on it is exhaustive:

```rust
match payload.view() {                     // CodexView<'_>
    CodexView::PreToolUse(pre) => pre.deny("locked by session abc"),
    CodexView::SessionStart(start) => start.add_context(text),
    CodexView::UserPromptSubmit(prompt) => prompt.add_context(text),
    // one arm per Codex event
    CodexView::Other(raw) => Response::none(),
}
```

Per-event view types are generic where the harnesses share an event (`PreToolUse<'a, H>`) and harness-specific where they do not (`BeforeShellExecution<'a>`). Events without a consumer yet share a generic view over the raw payload.

Shortcut accessors (`payload.pre_tool_use() -> Option<PreToolUse<'_, H>>`, one per shared event) serve consumers that handle a single event. They are bounded by per-event marker traits, so `Payload::<Cursor>::post_tool_batch()` does not exist rather than always returning `None`.

`AnyView`, from `AnyPayload::view()`, is the shared form, with one variant per `AnyEvent` variant. Each wraps a per-harness enum of that event's views; `AnyView::PreToolUse` covers Cursor's `preToolUse`, `beforeShellExecution` and `beforeMCPExecution`.

### Responses

Response methods live on the view types, through traits implemented only for the (harness, event) pairs that support them. A response the harness would not accept does not compile.

| Trait | Method | Implemented for |
|---|---|---|
| `Deny` | `deny(reason) -> Response` | Claude Code and Codex `PreToolUse`: `hookSpecificOutput.permissionDecision = "deny"` with `permissionDecisionReason`. Cursor `preToolUse`, `beforeShellExecution` and `beforeMCPExecution`: `permission: "deny"`, reason in `agent_message` (only that field reaches the agent), `continue: true`. |
| `AddContext` | `add_context(text) -> Response` | Codex `PreToolUse`, `PostToolUse`, `UserPromptSubmit`, `SessionStart`, `SubagentStart` (the Codex output schemas that declare it), as `hookSpecificOutput.additionalContext`. Claude Code, the same events plus `PostToolBatch`, same envelope. Cursor `sessionStart`, `postToolUse`, `postToolUseFailure`, as top-level `additional_context`. Claude accepts `additionalContext` on more events (`PostToolUseFailure`, `Stop`, `SubagentStop`, and others); v0.1.0 implements the subset consumers use. |
| `Ask` | `ask(reason) -> Response` | Claude Code and Codex `PreToolUse` (`permissionDecision = "ask"`; Codex's schema lists `allow`, `deny`, `ask`). |
| `Allow` | `allow_skipping_prompt() -> Response` | Claude Code and Codex `PreToolUse`. The name is deliberate: an explicit allow bypasses the user's own permission prompt, which devkit avoids on purpose. |

Cursor `preToolUse` and `beforeShellExecution` have no context channel: `agent_message` reaches the agent only on a deny.

`Response::deny_pre_tool_use(AnyHarness, reason)` builds the same deny without a payload. devkit denies when stdin is not JSON, and at that point it has only the harness from its flag. Claude Code and Codex get `hookEventName: "PreToolUse"`; Cursor's deny shape is the same for all three of its pre-tool events.

`PermissionRequest` has its own `decision.behavior` response shape and no current consumer, so v0.1.0 exposes its view without response traits.

`Response::none()` is an empty stdout. `Response` serializes to the exact JSON for its harness; `Display` writes it.

Responses carry only documented fields. Codex's output wire structs are `#[serde(deny_unknown_fields)]` and require `hookEventName` inside `hookSpecificOutput`, and Cursor blocks the action on a response that does not match the hook's schema. Cursor's `continue: true` on deny comes from Cursor's own examples, not its output tables, so the deny snapshot test pins it.

On `AnyView`, methods that only some harnesses support return `Option<Response>`.

Codex's legacy `decision: approve|block` field is not emitted; `pabal` writes the `hookSpecificOutput` form both Claude Code and Codex read.

### Errors

One `pabal::Error` (via `thiserror`) for input that is not a JSON object. Unknown events, unknown fields, missing fields and unknown tools never error. Consumers decide how to fail; all three current consumers fail open.

### Prelude

`pabal::prelude` re-exports what a hook command needs:

```rust
use pabal::prelude::*;
// ClaudeCode, Codex, Cursor, Harness, AnyHarness,
// Payload, AnyPayload, Fields,
// ClaudeCodeView, CodexView, CursorView, AnyView, AnyEvent,
// Tool, Edit, ToolCall, ShellKind,
// Response, Deny, AddContext, Ask, Allow
```

### Dependencies

| Crate | Why |
|---|---|
| `serde`, `serde_json` | payloads and responses |
| `strum` | event, harness and shell string forms |
| `ambassador` | `AnyPayload` delegation of `Fields` |
| `thiserror` | `Error` |
| `clap` (optional, `clap` feature) | `ValueEnum` on `AnyHarness` for consumers' `--harness` flags |

## Testing

- **Fixture corpus.** `tests/fixtures/<harness>/<event>/*.json`, seeded from alacritree `tests/fixtures/hook/`, herdr `tests/cli/hooks.rs`, devkit's hook tests, and Codex's generated schemas. Every fixture must parse and narrow to the expected view. Every fixture must infer to its harness, except fixtures marked ambiguous: a Codex `SessionEnd` and a Claude Code `SessionStart` with `model` assert the documented wrong answer, so a change to the inference rule shows up in review.
- **Response snapshots.** Each response trait impl has a test asserting the exact JSON, including the Cursor deny envelope, the Cursor `additional_context` envelope, and the absence of `permissionDecision` in context-only responses.
- **Schema validation.** Codex responses are validated against `*.command.output.schema.json` from the pinned Codex release.
- **Extracted behavior.** devkit's existing tests for `infer_harness`, `subagent_id` and `parse_shell_payload` move into `pabal` with their cases intact, except the `parent_conversation_id` case, which was hand-written and has no documented payload behind it. mcpls's `apply_patch` header tests move with them.
- **Doctests** for the prelude example and each public entry point.

## CI

Per push and PR, on ubuntu, macos and windows: `cargo nextest run`, `cargo test --doc`, `cargo clippy --all-targets -- -D warnings`, `cargo fmt --check`, and an MSRV build.

### Drift job

A scheduled workflow (weekly) checks each harness against its upstream source:

| Harness | Source | Check |
|---|---|---|
| Codex | latest `rust-v*` release tag of `openai/codex`, `codex-rs/hooks/schema/generated/` | Event list equals `CodexEvent`; fixtures validate against input schemas; responses validate against output schemas |
| Claude Code | the hooks reference page, plus the hook input types in the latest `@anthropic-ai/claude-agent-sdk` type declarations | Event list equals `ClaudeCodeEvent`; declared fields are covered by fixtures |
| Cursor | Cursor's hooks docs page | Event list equals `CursorEvent` |

On drift, the job opens or updates one issue per harness listing what changed, and tags `@claude` in it to open a PR adding fixtures and variants. Each such PR is reviewed by hand, since a new variant is a minor bump for every consumer.

## Release

- Published to crates.io as `pabal`. License `MIT OR Apache-2.0`, edition 2024, MSRV 1.85.
- 0.x semver: a new event, tool or view variant is a minor bump.
- v0.1.0 ships when every event the three consumers handle has a fixture and a view, and devkit's extracted tests pass inside `pabal`.

## Rollout

Each consumer migration is a separate issue in its own repo, after v0.1.0 is published:

1. **devkit** first. `pabal` is extracted from it, so devkit's existing hook tests passing unchanged against `pabal` is the parity check. `devkit-common::harness` keeps its config-policy functions (`resolve_rules`, `enforcement_enabled`, ...) and drops the protocol parts. Known behavior changes: `NotebookEdit` gains its path, the Cursor `parent_conversation_id` session fallback goes away, and `apply_patch` parsing adopts mcpls's rules.
2. **mcpls**, replacing `Harness`, `HookEvent`, `HookPayload`, `writes_a_file` and `apply_patch_paths`. Known behavior changes: `writes_a_file` gains `NotebookEdit`, and subagent attribution picks `agent_id()` or `agent()` explicitly.
3. **alacritree**, replacing `Harness`, `Event`, `Payload` and `output()`. Its WSL cwd translation and its treatment of non-JSON stdin as an empty payload stay local.

## Setup

- Install the Claude GitHub app on `AbysmalBiscuit/pabal` so the drift job's `@claude` tag acts.
- Publish the first crates.io release once the crate has working functionality. crates.io's policies prohibit a crate that "exists only to reserve a name for a prolonged period of time ... without having any genuine functionality, purpose, or significant development activity".

## Open questions

None.
