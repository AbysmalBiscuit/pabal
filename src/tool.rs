use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::payload::text;

/// The tool call inside a tool event, normalized across harnesses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Tool<'a> {
    /// A shell command. `cwd` is where it runs: the tool's own working
    /// directory when the harness sends one (Cursor), else the payload's
    /// `cwd`. `shell` is set only when the harness names the shell.
    #[allow(missing_docs, reason = "described on the variant")]
    Shell {
        command: &'a str,
        cwd: Option<&'a Path>,
        shell: Option<ShellKind>,
    },
    /// A tool that writes files.
    Edit(Edit<'a>),
    /// An MCP tool: from a `mcp__<server>__<tool>` name, from Cursor's
    /// `mcp_server_name`, or from Cursor's `MCP:<tool>` name, which names no
    /// server. Cursor's `beforeMCPExecution` and `afterMCPExecution` send
    /// `input` as a JSON-encoded string.
    #[allow(missing_docs, reason = "described on the variant")]
    Mcp {
        server: Option<&'a str>,
        tool: &'a str,
        input: &'a Value,
    },
    /// Any other tool, with its `tool_name` and `tool_input` as sent.
    #[allow(missing_docs, reason = "described on the variant")]
    Other { name: &'a str, input: &'a Value },
}

/// A tool call that writes files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Edit<'a> {
    /// A tool that writes the one file at `path`.
    #[allow(missing_docs, reason = "described on the variant")]
    Write { path: &'a Path },
    /// A Codex `apply_patch` envelope.
    #[allow(missing_docs, reason = "described on the variant")]
    Patch { patch: &'a str },
}

impl Edit<'_> {
    /// Every file the edit writes, in the order it names them.
    ///
    /// A patch names the paths of its `*** Add File:`, `*** Update File:` and
    /// `*** Delete File:` headers, and of a `*** Move to:` directly after an
    /// update. Only unprefixed lines are headers, since patch body lines start
    /// with `+`, `-` or a space. A patch whose first line is not
    /// `*** Begin Patch`, or that never reaches `*** End Patch`, names nothing.
    ///
    /// ```
    /// let patch = "*** Begin Patch\n*** Add File: a.rs\n+x\n*** End Patch";
    /// assert_eq!(pabal::Edit::Patch { patch }.paths(), [
    ///     std::path::Path::new("a.rs")
    /// ]);
    /// ```
    pub fn paths(&self) -> Vec<PathBuf> {
        match self {
            Edit::Write { path } => vec![path.to_path_buf()],
            Edit::Patch { patch } => patch_paths(patch).into_iter().map(PathBuf::from).collect(),
        }
    }
}

/// The shell a harness names for a shell tool.
#[derive(Debug, Clone, PartialEq, Eq, Hash, strum::EnumString, strum::Display)]
#[allow(missing_docs, reason = "each variant is the shell's tool name")]
pub enum ShellKind {
    Bash,
    PowerShell,
    #[strum(default)]
    Other(String),
}

/// One call of a batch event such as Claude Code's `PostToolBatch`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolCall<'a> {
    /// The call's `tool_use_id`.
    pub tool_use_id: Option<&'a str>,
    /// The call's tool view.
    pub tool: Tool<'a>,
    /// The call's `tool_response`, when the harness sent one.
    pub response: Option<&'a Value>,
}

static NULL: Value = Value::Null;

/// The `key` field of a tool call, `null` when absent.
pub(crate) fn field<'a>(call: &'a Value, key: &str) -> &'a Value {
    call.get(key).unwrap_or(&NULL)
}

impl<'a> Tool<'a> {
    pub(crate) fn shell(
        input: &'a Value,
        key: &str,
        cwd: Option<&'a Path>,
        shell: Option<ShellKind>,
    ) -> Option<Self> {
        let command = input.get(key)?.as_str()?;
        if command.trim().is_empty() {
            return None;
        }
        Some(Tool::Shell {
            command,
            cwd,
            shell,
        })
    }

    pub(crate) fn write(input: &'a Value, key: &str) -> Option<Self> {
        let path = Path::new(text(input, key)?);
        Some(Tool::Edit(Edit::Write { path }))
    }

    pub(crate) fn patch(input: &'a Value) -> Option<Self> {
        let patch = text(input, "command")?;
        Some(Tool::Edit(Edit::Patch { patch }))
    }

    pub(crate) fn mcp(name: &'a str, server: Option<&'a str>, input: &'a Value) -> Option<Self> {
        let (server, tool) = split_mcp(name, server)?;
        Some(Tool::Mcp {
            server: Some(server),
            tool,
            input,
        })
    }
}

/// Splits `mcp__<server>__<tool>` into `(server, tool)`. A known `server`
/// is stripped in the form Claude Code writes it into tool names; otherwise
/// the name splits at the first `__`.
fn split_mcp<'a>(name: &'a str, server: Option<&'a str>) -> Option<(&'a str, &'a str)> {
    let rest = name.strip_prefix("mcp__")?;
    let known = server.and_then(|s| {
        let tool = rest.strip_prefix(&tool_name_form(s))?.strip_prefix("__")?;
        Some((s, tool))
    });
    known
        .or_else(|| rest.split_once("__"))
        .filter(|(server, tool)| !server.is_empty() && !tool.is_empty())
}

/// Claude Code's `mcp__` form of a server name: characters outside
/// `[A-Za-z0-9_-]` become `_`, and `claude.ai ` connectors collapse and trim
/// their underscores.
fn tool_name_form(server: &str) -> String {
    let form: String = server
        .chars()
        .map(|c| match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '_' | '-' => c,
            _ => '_',
        })
        .collect();
    if !server.starts_with("claude.ai ") {
        return form;
    }
    form.split('_')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("_")
}

fn patch_paths(envelope: &str) -> Vec<&str> {
    let mut lines = envelope.lines();
    if lines.next() != Some("*** Begin Patch") {
        return Vec::new();
    }
    let mut paths = Vec::new();
    let mut after_update = false;
    for line in lines {
        if line == "*** End Patch" {
            return paths;
        }
        let Some((verb, path)) = line
            .strip_prefix("*** ")
            .and_then(|header| header.split_once(": "))
            .filter(|(_, path)| !path.is_empty())
        else {
            after_update = false;
            continue;
        };
        match verb {
            "Add File" | "Delete File" => after_update = false,
            "Update File" => after_update = true,
            "Move to" if after_update => after_update = false,
            _ => {
                after_update = false;
                continue;
            }
        }
        paths.push(path);
    }
    Vec::new()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::{Antigravity, AnyHarness, AnyPayload, ClaudeCode, Codex, Cursor, Fields, Payload};

    fn claude(v: Value) -> Payload<ClaudeCode> {
        Payload::from_value(v).unwrap()
    }

    fn cursor(v: Value) -> Payload<Cursor> {
        Payload::from_value(v).unwrap()
    }

    fn antigravity(name: &str, args: Value) -> Payload<Antigravity> {
        let v = json!({"conversationId": "c1", "toolCall": {"name": name, "args": args}});
        Payload::parse_named("PreToolUse", &v.to_string()).unwrap()
    }

    fn codex(v: Value) -> Payload<Codex> {
        Payload::from_value(v).unwrap()
    }

    fn pre(name: &str, input: Value) -> Value {
        json!({"hook_event_name": "PreToolUse", "tool_name": name, "tool_input": input, "cwd": "/repo"})
    }

    #[test]
    fn claude_bash_and_powershell_name_their_shell() {
        let p = claude(pre("Bash", json!({"command": "vite dev"})));
        assert_eq!(
            p.tool(),
            Some(Tool::Shell {
                command: "vite dev",
                cwd: Some(Path::new("/repo")),
                shell: Some(ShellKind::Bash)
            })
        );
        let p = claude(pre("PowerShell", json!({"command": "Get-ChildItem"})));
        assert!(matches!(
            p.tool(),
            Some(Tool::Shell {
                command: "Get-ChildItem",
                shell: Some(ShellKind::PowerShell),
                ..
            })
        ));
    }

    #[test]
    fn claude_file_tools_are_writes() {
        for name in ["Edit", "Write", "MultiEdit"] {
            let p = claude(pre(name, json!({"file_path": "/repo/a.rs"})));
            assert_eq!(
                p.tool(),
                Some(Tool::Edit(Edit::Write {
                    path: Path::new("/repo/a.rs")
                })),
                "{name}"
            );
        }
        let p = claude(pre(
            "NotebookEdit",
            json!({"notebook_path": "/repo/n.ipynb"}),
        ));
        assert_eq!(
            p.tool(),
            Some(Tool::Edit(Edit::Write {
                path: Path::new("/repo/n.ipynb")
            }))
        );
    }

    #[test]
    fn claude_mcp_prefers_the_server_object() {
        let mut v = pre("mcp__my__srv__do_thing", json!({"q": 1}));
        v["mcp_server"] = json!({"name": "my__srv", "source": "user"});
        let p = claude(v);
        assert_eq!(
            p.tool(),
            Some(Tool::Mcp {
                server: Some("my__srv"),
                tool: "do_thing",
                input: &json!({"q": 1})
            })
        );
    }

    #[test]
    fn claude_mcp_names_the_server_by_its_config_key() {
        for (name, key) in [
            ("mcp__claude_ai_Linear__save_issue", "claude.ai Linear"),
            ("mcp__my_srv_v2__save_issue", "my srv.v2"),
        ] {
            let mut v = pre(name, json!({}));
            v["mcp_server"] = json!({"name": key});
            assert!(matches!(
                claude(v).tool(),
                Some(Tool::Mcp { server: Some(s), tool: "save_issue", .. }) if s == key
            ));
        }
    }

    #[test]
    fn mcp_names_split_at_the_first_separator() {
        let c = claude(pre("mcp__srv__do__thing", json!({})));
        let x = codex(pre("mcp__srv__do__thing", json!({})));
        for tool in [c.tool(), x.tool()] {
            assert!(matches!(
                tool,
                Some(Tool::Mcp {
                    server: Some("srv"),
                    tool: "do__thing",
                    ..
                })
            ));
        }
    }

    #[test]
    fn unsplittable_mcp_names_are_other() {
        let p = codex(pre("mcp__srv", json!({})));
        assert!(matches!(
            p.tool(),
            Some(Tool::Other {
                name: "mcp__srv",
                ..
            })
        ));
    }

    #[test]
    fn codex_bash_takes_the_payload_cwd() {
        let p = codex(
            json!({"hook_event_name": "PreToolUse", "tool_name": "Bash", "tool_input": {"command": "ls"}, "cwd": "/w"}),
        );
        assert_eq!(
            p.tool(),
            Some(Tool::Shell {
                command: "ls",
                cwd: Some(Path::new("/w")),
                shell: None
            })
        );
    }

    #[test]
    fn codex_apply_patch_is_a_patch() {
        let patch = "*** Begin Patch\n*** Add File: a.rs\n+x\n*** End Patch\n";
        let p = codex(pre("apply_patch", json!({"command": patch})));
        assert_eq!(p.tool(), Some(Tool::Edit(Edit::Patch { patch })));
    }

    #[test]
    fn blank_or_missing_commands_are_other() {
        for input in [json!({}), json!({"command": "  "})] {
            let p = claude(pre("Bash", input));
            assert!(matches!(p.tool(), Some(Tool::Other { name: "Bash", .. })));
        }
    }

    #[test]
    fn a_string_tool_input_is_other() {
        let p = claude(pre("Bash", json!("ls")));
        assert_eq!(
            p.tool(),
            Some(Tool::Other {
                name: "Bash",
                input: &json!("ls")
            })
        );
    }

    #[test]
    fn other_tools_keep_their_name() {
        let r = claude(pre("Read", json!({"file_path": "/a"})));
        assert!(matches!(r.tool(), Some(Tool::Other { name: "Read", .. })));
        let s = codex(pre("spawn_agent", json!({})));
        assert!(matches!(
            s.tool(),
            Some(Tool::Other {
                name: "spawn_agent",
                ..
            })
        ));
    }

    #[test]
    fn windows_paths_are_verbatim() {
        let p = claude(
            json!({"hook_event_name": "PreToolUse", "tool_name": "Write", "cwd": "C:\\repo", "tool_input": {"file_path": "C:\\repo\\a.rs"}}),
        );
        assert_eq!(
            p.tool(),
            Some(Tool::Edit(Edit::Write {
                path: Path::new("C:\\repo\\a.rs")
            }))
        );
        assert_eq!(p.cwd(), Some(Path::new("C:\\repo")));
    }

    #[test]
    fn no_tool_name_is_no_tool() {
        let p = claude(json!({"hook_event_name": "SessionStart", "session_id": "s"}));
        assert_eq!(p.tool(), None);
    }

    #[test]
    fn shell_kind_strings() {
        assert_eq!("Bash".parse::<ShellKind>().unwrap(), ShellKind::Bash);
        assert_eq!(
            "PowerShell".parse::<ShellKind>().unwrap(),
            ShellKind::PowerShell
        );
        assert_eq!(
            "zsh".parse::<ShellKind>().unwrap(),
            ShellKind::Other("zsh".into())
        );
        assert_eq!(ShellKind::Other("zsh".into()).to_string(), "zsh");
    }

    #[test]
    fn write_paths_are_their_path() {
        let e = Edit::Write {
            path: Path::new("/r/a.rs"),
        };
        assert_eq!(e.paths(), vec![PathBuf::from("/r/a.rs")]);
    }

    fn patch_paths(patch: &str) -> Vec<PathBuf> {
        Edit::Patch { patch }.paths()
    }

    fn bufs(paths: &[&str]) -> Vec<PathBuf> {
        paths.iter().map(PathBuf::from).collect()
    }

    #[test]
    fn test_apply_patch_paths_names_every_verb_and_both_ends_of_a_rename() {
        let patch = "*** Begin Patch\n\
                     *** Add File: src/new.rs\n\
                     +*** Update File: not/a/header.rs\n\
                     *** Update File: src/old.rs\n\
                     *** Move to: src/renamed.rs\n\
                     @@\n\
                     -a\n\
                     +b\n\
                     *** Delete File: src/gone.rs\n\
                     *** End Patch\n";
        assert_eq!(
            patch_paths(patch),
            bufs(&["src/new.rs", "src/old.rs", "src/renamed.rs", "src/gone.rs"])
        );
    }

    /// A producer that strips trailing whitespace leaves a blank context
    /// line bare, with no leading space. The files the patch names are
    /// still the files it wrote.
    #[test]
    fn test_a_bare_blank_context_line_keeps_the_patch_attributed() {
        let patch = "*** Begin Patch\n\
                     *** Update File: src/a.rs\n\
                     @@\n\
                     -old\n\
                     \n\
                     +new\n\
                     *** End Patch\n";
        assert_eq!(patch_paths(patch), bufs(&["src/a.rs"]));
    }

    #[test]
    fn test_crlf_and_spaces_in_paths_survive() {
        let patch = "*** Begin Patch\r\n\
                     *** Update File: src/old name.rs\r\n\
                     *** Move to: src/new name.rs\r\n\
                     @@\r\n\
                     -a\r\n\
                     +b\r\n\
                     *** End Patch\r\n";
        assert_eq!(
            patch_paths(patch),
            bufs(&["src/old name.rs", "src/new name.rs"])
        );
    }

    #[test]
    fn test_an_unclosed_or_unopened_envelope_names_nothing() {
        assert!(patch_paths("*** Update File: outside-an-envelope.rs").is_empty());
        assert!(patch_paths("*** Begin Patch\n*** Update File: src/a.rs\n@@\n+b\n").is_empty());
    }

    #[test]
    fn a_claude_code_payload_carries_its_command_under_tool_input() {
        let p = AnyPayload::from_value(
            AnyHarness::infer(&json!({})),
            json!({
                "hook_event_name": "PreToolUse",
                "tool_name": "Bash",
                "tool_input": { "command": "vite dev" },
                "cwd": "/repo"
            }),
        )
        .unwrap();
        assert_eq!(p.harness(), AnyHarness::ClaudeCode);
        let Some(Tool::Shell { command, cwd, .. }) = p.tool() else {
            panic!()
        };
        assert_eq!(command, "vite dev");
        assert_eq!(cwd.unwrap(), Path::new("/repo"));
    }

    #[test]
    fn codex_is_still_codex() {
        let p = AnyPayload::parse_inferred(
            r#"{"hook_event_name": "PreToolUse", "turn_id": "t1", "model": "gpt-5",
                "tool_name": "Bash", "tool_input": {"command": "ls"}, "cwd": "/w"}"#,
        )
        .unwrap();
        assert_eq!(p.harness(), AnyHarness::Codex);
        assert!(matches!(p.tool(), Some(Tool::Shell { command: "ls", .. })));
    }

    #[test]
    fn an_explicit_cwd_beats_the_tool_inputs_working_directory() {
        let p = claude(json!({
            "hook_event_name": "PreToolUse", "tool_name": "Bash", "cwd": "/repo",
            "tool_input": {"command": "ls", "working_directory": "/elsewhere"}
        }));
        let Some(Tool::Shell { cwd, .. }) = p.tool() else {
            panic!()
        };
        assert_eq!(cwd, Some(Path::new("/repo")));
    }

    #[test]
    fn a_non_bash_tool_is_not_a_shell_payload() {
        let p = claude(json!({
            "hook_event_name": "PreToolUse",
            "tool_name": "Edit",
            "tool_input": { "file_path": "/repo/a.rs" }
        }));
        assert!(!matches!(p.tool(), Some(Tool::Shell { .. })));
    }

    #[test]
    fn claudes_powershell_tool_is_a_shell_payload() {
        let p = AnyPayload::parse_inferred(
            r#"{"hook_event_name": "PreToolUse", "tool_name": "PowerShell", "prompt_id": "p",
                "session_id": "S", "agent_id": "a1", "agent_type": "general-purpose",
                "tool_input": { "command": "Get-ChildItem" }}"#,
        )
        .unwrap();
        assert_eq!(p.harness(), AnyHarness::ClaudeCode);
        assert!(matches!(
            p.tool(),
            Some(Tool::Shell {
                shell: Some(ShellKind::PowerShell),
                ..
            })
        ));
        assert_eq!(p.agent(), Some("a1"));
    }

    #[test]
    fn cursor_shell_runs_in_its_tool_input_cwd() {
        let p = cursor(json!({
            "hook_event_name": "preToolUse", "tool_name": "Shell", "cwd": "/project",
            "tool_input": {"command": "npm install", "cwd": "/project/web"}
        }));
        assert_eq!(
            p.tool(),
            Some(Tool::Shell {
                command: "npm install",
                cwd: Some(Path::new("/project/web")),
                shell: None
            })
        );
        let p = cursor(json!({
            "hook_event_name": "preToolUse", "tool_name": "Shell", "cwd": "/project",
            "tool_input": {"command": "ls"}
        }));
        assert!(
            matches!(p.tool(), Some(Tool::Shell { cwd: Some(c), .. }) if c == Path::new("/project"))
        );
    }

    #[test]
    fn cursor_shell_execution_carries_its_command_at_the_top_level() {
        let p = cursor(json!({
            "hook_event_name": "beforeShellExecution", "command": "vite dev", "cwd": "/repo"
        }));
        assert_eq!(
            p.tool(),
            Some(Tool::Shell {
                command: "vite dev",
                cwd: Some(Path::new("/repo")),
                shell: None
            })
        );
    }

    #[test]
    fn cursor_mcp_names_its_server_when_it_can() {
        let p = cursor(json!({
            "hook_event_name": "beforeMCPExecution", "tool_name": "save_issue",
            "tool_input": "{\"title\":\"x\"}", "mcp_server_name": "linear", "command": "npx linear"
        }));
        assert_eq!(
            p.tool(),
            Some(Tool::Mcp {
                server: Some("linear"),
                tool: "save_issue",
                input: &json!("{\"title\":\"x\"}")
            })
        );
        let p = cursor(pre("MCP:save_issue", json!({})));
        assert!(matches!(
            p.tool(),
            Some(Tool::Mcp {
                server: None,
                tool: "save_issue",
                ..
            })
        ));
    }

    #[test]
    fn cursor_write_is_a_write_and_the_rest_are_other() {
        let p = cursor(pre("Write", json!({"file_path": "/repo/a.rs"})));
        assert_eq!(
            p.tool(),
            Some(Tool::Edit(Edit::Write {
                path: Path::new("/repo/a.rs")
            }))
        );
        for name in ["Read", "Delete", "Grep", "Task"] {
            let p = cursor(pre(name, json!({"file_path": "/repo/a.rs"})));
            assert!(matches!(p.tool(), Some(Tool::Other { name: n, .. }) if n == name));
        }
        let p = cursor(json!({"hook_event_name": "sessionStart", "session_id": "s"}));
        assert_eq!(p.tool(), None);
    }

    #[test]
    fn antigravity_run_command_is_a_shell_in_its_cwd() {
        let p = antigravity(
            "run_command",
            json!({"CommandLine": "npm test", "Cwd": "/workspace/project", "WaitMsBeforeAsync": 5000}),
        );
        assert_eq!(
            p.tool(),
            Some(Tool::Shell {
                command: "npm test",
                cwd: Some(Path::new("/workspace/project")),
                shell: None
            })
        );
    }

    #[test]
    fn antigravity_file_writers_are_writes() {
        for name in [
            "write_to_file",
            "replace_file_content",
            "multi_replace_file_content",
        ] {
            let p = antigravity(name, json!({"TargetFile": "/w/a.rs"}));
            assert_eq!(
                p.tool(),
                Some(Tool::Edit(Edit::Write {
                    path: Path::new("/w/a.rs")
                })),
                "{name}"
            );
        }
        let p = antigravity("view_file", json!({"AbsolutePath": "/w/a.rs"}));
        assert_eq!(
            p.tool(),
            Some(Tool::Other {
                name: "view_file",
                input: &json!({"AbsolutePath": "/w/a.rs"})
            })
        );
    }
}
