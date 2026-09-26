use std::path::{Path, PathBuf};

use pabal::{AnyHarness, AnyPayload, ClaudeCodeView, CodexView, Fields};
use serde_json::Value;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

struct Fixture {
    harness: AnyHarness,
    event: String,
    path: PathBuf,
    text: String,
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
    }
}

#[test]
fn every_fixture_parses_infers_and_narrows() {
    let mut failures = Vec::new();
    for f in fixtures() {
        let name = f.path.strip_prefix(root()).unwrap().display().to_string();
        let payload = match AnyPayload::parse(f.harness, &f.text) {
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
        (
            "claude-code",
            &[
                "SessionStart",
                "UserPromptSubmit",
                "PreToolUse",
                "PostToolUse",
                "PostToolUseFailure",
                "PostToolBatch",
                "PermissionRequest",
                "SessionEnd",
                "SubagentStart",
                "SubagentStop",
                "Stop",
                "PreCompact",
                "PostCompact",
                "Notification",
                "CwdChanged",
                "WorktreeCreate",
                "WorktreeRemove",
            ],
        ),
        (
            "codex",
            &[
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
            ],
        ),
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
