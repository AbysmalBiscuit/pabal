use std::path::{Path, PathBuf};

use pabal::{
    AntigravityView, AnyHarness, AnyPayload, ClaudeCode, ClaudeCodeView, CodexView, CursorView,
    Edit, Fields, Payload, Tool,
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
        ("cursor/afterShellExecution/docs.json", "shell -"),
        (
            "cursor/beforeMCPExecution/docs.json",
            "mcp linear/save_issue",
        ),
        ("cursor/beforeShellExecution/docs.json", "shell -"),
        ("cursor/postToolUse/docs.json", "shell -"),
        ("cursor/postToolUseFailure/docs.json", "shell -"),
        ("cursor/preToolUse/shell.json", "shell -"),
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
        let names_a_tool = ["tool_name", "toolCall"]
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
