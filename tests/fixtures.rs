use std::path::{Path, PathBuf};

use pabal::{
    AntigravityView, AnyEvent, AnyHarness, AnyPayload, ClaudeCode, ClaudeCodeView, CodexView,
    CursorView, Edit, Fields, GrokView, Payload, Tool,
};
use serde_json::Value;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

/// A fixture's directory names its event, which Antigravity payloads do not
/// carry themselves.
struct Fixture {
    harness: AnyHarness,
    event: String,
    path: PathBuf,
    text: String,
}

impl Fixture {
    fn parse(&self) -> Result<AnyPayload, pabal::Error> {
        AnyPayload::parse_named(self.harness, &self.event, &self.text)
    }
}

fn dirs(path: &Path) -> Vec<PathBuf> {
    let mut out: Vec<_> = std::fs::read_dir(path)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    out.sort();
    out
}

fn fixtures() -> Vec<Fixture> {
    let mut out = Vec::new();
    for harness_dir in dirs(&root()) {
        let harness: AnyHarness = harness_dir
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .parse()
            .unwrap();
        for event_dir in dirs(&harness_dir) {
            let event = event_dir.file_name().unwrap().to_str().unwrap().to_owned();
            for path in dirs(&event_dir) {
                let text = std::fs::read_to_string(&path).unwrap();
                out.push(Fixture {
                    harness,
                    event: event.clone(),
                    path,
                    text,
                });
            }
        }
    }
    out
}

/// The fixture's path under the corpus root, `/`-separated on every platform.
fn name(path: &Path) -> String {
    let parts: Vec<_> = path
        .strip_prefix(root())
        .unwrap()
        .iter()
        .map(|c| c.to_str().unwrap())
        .collect();
    parts.join("/")
}

fn is_ambiguous(path: &Path) -> bool {
    path.file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .starts_with("ambiguous")
}

fn is_other(payload: &AnyPayload) -> bool {
    match payload {
        AnyPayload::ClaudeCode(p) => matches!(p.view(), ClaudeCodeView::Other(_)),
        AnyPayload::Codex(p) => matches!(p.view(), CodexView::Other(_)),
        AnyPayload::Cursor(p) => matches!(p.view(), CursorView::Other(_)),
        AnyPayload::Antigravity(p) => matches!(p.view(), AntigravityView::Other(_)),
        AnyPayload::Grok(p) => matches!(p.view(), GrokView::Other(_)),
    }
}

#[test]
fn every_fixture_parses_infers_and_narrows() {
    let mut failures = Vec::new();
    for f in fixtures() {
        let name = name(&f.path);
        let payload = match f.parse() {
            Ok(p) => p,
            Err(e) => {
                failures.push(format!("{name}: {e}"));
                continue;
            }
        };
        if payload.event_name() != f.event {
            failures.push(format!("{name}: event {}", payload.event_name()));
        }
        if is_other(&payload) {
            failures.push(format!("{name}: narrows to Other"));
        }
        let inferred = AnyHarness::infer(payload.raw());
        if !is_ambiguous(&f.path) && inferred != f.harness {
            failures.push(format!("{name}: infers as {inferred}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn ambiguous_fixtures_infer_as_documented() {
    let infer = |rel: &str| {
        let text = std::fs::read_to_string(root().join(rel)).unwrap();
        AnyHarness::infer(&serde_json::from_str::<Value>(&text).unwrap())
    };
    assert_eq!(
        infer("codex/SessionEnd/ambiguous.json"),
        AnyHarness::ClaudeCode
    );
    assert_eq!(
        infer("claude-code/SessionStart/ambiguous-model.json"),
        AnyHarness::Codex
    );
}

#[test]
fn every_consumer_event_has_a_fixture() {
    let expected: &[(&str, &[&str])] = &[
        ("claude-code", &[
            "SessionStart",
            "UserPromptSubmit",
            "PreToolUse",
            "PostToolUse",
            "PostToolUseFailure",
            "PostToolBatch",
            "PermissionRequest",
            "PermissionDenied",
            "SessionEnd",
            "SubagentStart",
            "SubagentStop",
            "Stop",
            "StopFailure",
            "PreCompact",
            "PostCompact",
            "Notification",
            "CwdChanged",
            "WorktreeCreate",
            "WorktreeRemove",
        ]),
        ("codex", &[
            "SessionStart",
            "UserPromptSubmit",
            "PreToolUse",
            "PostToolUse",
            "PermissionRequest",
            "Stop",
            "Interrupt",
            "SubagentStart",
            "SubagentStop",
            "PreCompact",
            "PostCompact",
            "SessionEnd",
        ]),
        ("cursor", &[
            "sessionStart",
            "sessionEnd",
            "preToolUse",
            "postToolUse",
            "postToolUseFailure",
            "subagentStart",
            "subagentStop",
            "beforeShellExecution",
            "beforeMCPExecution",
            "beforeSubmitPrompt",
            "preCompact",
            "stop",
            "workspaceOpen",
        ]),
        ("antigravity", &[
            "PreToolUse",
            "PostToolUse",
            "PreInvocation",
            "PostInvocation",
            "Stop",
        ]),
        ("grok", &[
            "SessionStart",
            "UserPromptSubmit",
            "PreToolUse",
            "PostToolUse",
            "PostToolUseFailure",
            "PermissionDenied",
            "Stop",
            "StopFailure",
            "StopCancelled",
            "Notification",
            "SubagentStart",
            "SubagentStop",
            "PreCompact",
            "PostCompact",
            "SessionEnd",
        ]),
    ];
    let mut missing = Vec::new();
    for (harness, events) in expected {
        for event in *events {
            let dir = root().join(harness).join(event);
            let has_file = dir.is_dir() && std::fs::read_dir(&dir).unwrap().next().is_some();
            if !has_file {
                missing.push(format!("{harness}/{event}"));
            }
        }
    }
    assert!(missing.is_empty(), "no fixture for: {}", missing.join(", "));
}

fn kind(tool: Option<Tool>) -> String {
    match tool {
        None => "none".into(),
        Some(Tool::Shell { shell, .. }) => {
            format!("shell {}", shell.map_or("-".into(), |s| s.to_string()))
        }
        Some(Tool::Edit(Edit::Write { .. })) => "write".into(),
        Some(Tool::Edit(Edit::Patch { .. })) => "patch".into(),
        Some(Tool::Mcp { server, tool, .. }) => format!("mcp {}/{tool}", server.unwrap_or("-")),
        Some(Tool::Other { name, .. }) => format!("other {name}"),
    }
}

#[test]
fn every_tool_fixture_gives_its_tool_view() {
    let expected = [
        ("antigravity/PostToolUse/run-command.json", "shell -"),
        ("antigravity/PreToolUse/replace-file-content.json", "write"),
        ("antigravity/PreToolUse/run-command.json", "shell -"),
        ("antigravity/PreToolUse/write-to-file.json", "write"),
        ("claude-code/PermissionDenied/docs.json", "shell Bash"),
        ("claude-code/PermissionRequest/docs.json", "shell Bash"),
        ("claude-code/PostToolUse/docs.json", "write"),
        ("claude-code/PostToolUseFailure/docs.json", "shell Bash"),
        ("claude-code/PreToolUse/bash.json", "shell Bash"),
        ("claude-code/PreToolUse/edit.json", "write"),
        ("claude-code/PreToolUse/fork.json", "shell Bash"),
        (
            "claude-code/PreToolUse/mcp.json",
            "mcp memory/create_entities",
        ),
        ("claude-code/PreToolUse/notebook-edit.json", "write"),
        ("claude-code/PreToolUse/powershell.json", "shell PowerShell"),
        ("codex/PermissionRequest/schema.json", "shell -"),
        ("codex/PostToolUse/schema.json", "shell -"),
        ("codex/PreToolUse/apply-patch.json", "patch"),
        ("codex/PreToolUse/bash.json", "shell -"),
        ("codex/PreToolUse/mcp.json", "mcp memory/create_entities"),
        (
            "cursor/afterMCPExecution/docs.json",
            "mcp linear/save_issue",
        ),
        ("cursor/afterShellExecution/cli.json", "shell -"),
        (
            "cursor/beforeMCPExecution/docs.json",
            "mcp linear/save_issue",
        ),
        ("cursor/beforeShellExecution/cli.json", "shell -"),
        ("cursor/postToolUse/shell.json", "shell -"),
        ("cursor/postToolUse/write.json", "write"),
        ("cursor/postToolUseFailure/denied-shell.json", "shell -"),
        ("cursor/postToolUseFailure/read.json", "other Read"),
        ("cursor/preToolUse/delete.json", "other Delete"),
        ("cursor/preToolUse/read.json", "other Read"),
        ("cursor/preToolUse/shell.json", "shell -"),
        ("cursor/preToolUse/write.json", "write"),
        ("grok/PermissionDenied/cli.json", "shell -"),
        ("grok/PostToolUse/mcp.json", "mcp linear/save_issue"),
        ("grok/PostToolUse/run-terminal-command.json", "shell -"),
        ("grok/PostToolUse/search-replace.json", "write"),
        ("grok/PostToolUseFailure/mcp.json", "mcp linear/save_issue"),
        ("grok/PreToolUse/mcp.json", "mcp linear/save_issue"),
        ("grok/PreToolUse/read-file.json", "other read_file"),
        ("grok/PreToolUse/run-terminal-command.json", "shell -"),
        ("grok/PreToolUse/search-replace.json", "write"),
        (
            "grok/PreToolUse/spawn-subagent.json",
            "other spawn_subagent",
        ),
        ("grok/PreToolUse/subagent-read-file.json", "other read_file"),
        ("grok/PreToolUse/write.json", "write"),
    ];
    let mut failures = Vec::new();
    for f in fixtures() {
        let name = name(&f.path);
        let payload = f.parse().unwrap();
        let got = kind(payload.tool());
        let want = expected
            .iter()
            .find(|(path, _)| *path == name)
            .map(|(_, k)| *k);
        let names_a_tool = ["tool_name", "toolCall", "toolName"]
            .iter()
            .any(|key| payload.raw().get(key).is_some());
        if names_a_tool && want.is_none() {
            failures.push(format!("{name}: {got}, not in the table"));
        } else if want.is_some_and(|want| want != got) {
            failures.push(format!("{name}: {got}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn the_batch_fixture_gives_its_tool_calls() {
    let text = std::fs::read_to_string(root().join("claude-code/PostToolBatch/docs.json")).unwrap();
    let payload = Payload::<ClaudeCode>::parse(&text).unwrap();
    let calls: Vec<String> = payload
        .post_tool_batch()
        .unwrap()
        .tool_calls()
        .map(|call| kind(Some(call.tool)))
        .collect();
    assert_eq!(calls, ["other Read", "other Read"]);
}

fn grok(rel: &str) -> AnyPayload {
    let text = std::fs::read_to_string(root().join("grok").join(rel)).unwrap();
    AnyPayload::parse(AnyHarness::Grok, &text).unwrap()
}

#[test]
fn grok_events_map_onto_the_shared_events() {
    let cases = [
        ("SessionStart/cli.json", AnyEvent::SessionStart),
        ("UserPromptSubmit/cli.json", AnyEvent::UserPromptSubmit),
        ("PreToolUse/run-terminal-command.json", AnyEvent::PreToolUse),
        ("PostToolUse/mcp.json", AnyEvent::PostToolUse),
        ("Stop/end-turn.json", AnyEvent::Stop),
        ("SubagentStart/cli.json", AnyEvent::SubagentStart),
        ("SubagentStop/cli.json", AnyEvent::SubagentStop),
        ("PreCompact/manual.json", AnyEvent::PreCompact),
        ("PostCompact/manual.json", AnyEvent::PostCompact),
        ("SessionEnd/shutdown.json", AnyEvent::SessionEnd),
        (
            "StopCancelled/max-turns.json",
            AnyEvent::Other("StopCancelled".into()),
        ),
        (
            "StopFailure/invalid-request.json",
            AnyEvent::Other("StopFailure".into()),
        ),
        (
            "PermissionDenied/cli.json",
            AnyEvent::Other("PermissionDenied".into()),
        ),
    ];
    for (rel, event) in cases {
        assert_eq!(grok(rel).any_event(), event, "{rel}");
    }
    let AnyPayload::Grok(cancelled) = grok("StopCancelled/max-turns.json") else {
        panic!()
    };
    assert!(matches!(cancelled.view(), GrokView::StopCancelled(_)));
}

#[test]
fn grok_fields_read_its_camel_case_keys() {
    let pre = grok("PreToolUse/run-terminal-command.json");
    assert_eq!(
        pre.session_id(),
        Some("01a13168-584d-7843-9767-0ef65e885cd5")
    );
    assert_eq!(pre.cwd(), Some(Path::new("/home/user/project")));
    assert_eq!(
        pre.transcript_path(),
        Some(Path::new(
            "/home/user/.grok/sessions/%2Fhome%2Fuser%2Fproject/01a13168-584d-7843-9767-0ef65e885cd5/updates.jsonl"
        ))
    );
    assert_eq!(pre.tool_use_id(), Some("call_3_0"));
    assert_eq!((pre.agent_id(), pre.agent()), (None, None));
}

/// A subagent's own events carry its id as their session id, beside its
/// type. `SubagentStart` fires in the parent session and names the child.
#[test]
fn grok_names_the_subagent_a_payload_speaks_for() {
    let child = "01a13169-8c39-7383-98ff-c133574173d3";
    let start = grok("SubagentStart/cli.json");
    assert_eq!(
        (start.agent_id(), start.agent()),
        (Some(child), Some(child))
    );
    assert_ne!(start.session_id(), Some(child));
    assert_eq!(grok("SubagentStop/cli.json").agent(), Some(child));
    let call = grok("PreToolUse/subagent-read-file.json");
    assert_eq!((call.agent_id(), call.agent()), (None, Some(child)));
    assert_eq!(grok("PreToolUse/spawn-subagent.json").agent(), None);
}

#[test]
fn grok_tools_read_the_recorded_inputs() {
    assert_eq!(
        grok("PreToolUse/run-terminal-command.json").tool(),
        Some(Tool::Shell {
            command: "echo pabal-probe",
            cwd: Some(Path::new("/home/user/project")),
            shell: None
        })
    );
    for (rel, path) in [
        ("PreToolUse/search-replace.json", "a.txt"),
        ("PreToolUse/write.json", "b.txt"),
    ] {
        assert_eq!(
            grok(rel).tool(),
            Some(Tool::Edit(Edit::Write {
                path: Path::new(path)
            })),
            "{rel}"
        );
    }
    assert_eq!(
        grok("PreToolUse/mcp.json").tool(),
        Some(Tool::Mcp {
            server: Some("linear"),
            tool: "save_issue",
            input: &serde_json::json!({"title": "x"})
        })
    );
}
