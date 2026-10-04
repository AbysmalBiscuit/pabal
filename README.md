# pabal (파발)

Typed hook payloads and responses for coding-agent harnesses.

The name is the Joseon-era relay courier system (擺撥): riders carrying dispatches between stations, as this crate carries messages between a harness and its hooks.

A hook command reads a JSON payload on stdin and may answer with JSON on stdout. `pabal` parses the payload, narrows it to its event, gives a typed view of the tool call, and builds the response the harness accepts. It does not read stdin, pick exit codes or install hooks; those stay with the hook command.

## Supported harnesses

| Harness | Type | `--harness` name |
|---|---|---|
| Claude Code | `ClaudeCode` | `claude-code` (alias `claude`) |
| Codex | `Codex` | `codex` |
| Cursor | `Cursor` | `cursor` |
| Google Antigravity | `Antigravity` | `antigravity` |

The Cursor CLI also runs hooks configured for Claude Code, and sends them its own payloads; those parse as `Cursor`.

Antigravity leaves the event out of its payload. Have each hook command in `hooks.json` name its event, for example `my-hook --event PreToolUse`, and parse with `Payload::<Antigravity>::parse_named(event, stdin)`.

## Example

Parse stdin for a known harness, narrow it to its event, and answer:

```rust
use pabal::prelude::*;

let stdin = r#"{"hook_event_name":"PreToolUse","turn_id":"t1","cwd":"/repo",
    "tool_name":"Bash","tool_input":{"command":"rm -rf target"}}"#;
let payload = Payload::<Codex>::parse(stdin).unwrap();
let response = match payload.view() {
    CodexView::PreToolUse(pre) => match pre.tool() {
        Some(Tool::Shell { command, .. }) if command.starts_with("rm ") => {
            pre.deny("locked by session abc")
        }
        _ => Response::none(),
    },
    CodexView::SessionStart(start) => start.add_context("hello"),
    _ => Response::none(),
};
assert_eq!(
    response.to_string(),
    r#"{"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"deny","permissionDecisionReason":"locked by session abc"}}"#
);
```

When the harness is known only at runtime, from a flag or from the payload itself, use `AnyPayload`:

```rust
use pabal::prelude::*;

let stdin = r#"{"hook_event_name":"SessionStart","session_id":"s1"}"#;
let payload = AnyPayload::parse(AnyHarness::ClaudeCode, stdin).unwrap();
let response = match payload.view() {
    AnyView::SessionStart(start) => start.add_context("tasks: none"),
    _ => Response::none(),
};
assert_eq!(
    response.json(),
    Some(&serde_json::json!({"hookSpecificOutput": {
        "hookEventName": "SessionStart",
        "additionalContext": "tasks: none"
    }}))
);
```

`AnyPayload::parse_inferred` guesses the harness from the payload. The guess is wrong for a few payload shapes (see `AnyHarness::infer`), so pass the harness explicitly when the hook command can be told it.

A response method exists only on the events whose harness accepts it: `deny` on a `SessionEnd` view does not compile.

The answers a `PreToolUse` view offers on each harness. On `AnyPreToolUse` the methods missing here return `None`.

| Answer | Claude Code | Codex | Cursor | Antigravity |
|---|---|---|---|---|
| `deny` | yes | yes | yes | yes |
| `ask` | yes | no | no | yes |
| `allow_skipping_prompt` | yes | no | no | no |
| `add_context` | yes | yes | yes | no |
| `rewrite_input` | yes | yes | yes | no |

`block` keeps the agent from ending its turn and gives it the reason as its next prompt. On `AnyStop` and `AnySubagentStop` it returns `None` where this table says no.

| Event | Claude Code | Codex | Cursor | Antigravity |
|---|---|---|---|---|
| `Stop` | yes | yes | yes | no |
| `SubagentStop` | yes | yes | no | not sent |

## Exhaustive enums and semver

Event, view and tool enums are not `#[non_exhaustive]`. An event, tool or view the crate does not know parses into an `Other` variant, and parsing fails only on input that is not a JSON object. When a harness adds an event, `pabal` adds a variant in a minor release (0.x), and every exhaustive `match` in a consumer fails to compile at the spot that has to handle it.

The optional `clap` feature derives `clap::ValueEnum` on `AnyHarness`.

## License

Licensed under either of [Apache License, Version 2.0](https://github.com/AbysmalBiscuit/pabal/blob/main/LICENSE-APACHE) or [MIT license](https://github.com/AbysmalBiscuit/pabal/blob/main/LICENSE-MIT) at your option.
