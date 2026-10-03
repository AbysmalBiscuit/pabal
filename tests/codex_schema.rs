use std::path::PathBuf;

use pabal::{AddContext, Codex, Deny, Payload, Response, RewriteInput};
use serde_json::{Map, Value, json};

/// The Codex schema directory: `PABAL_CODEX_SCHEMAS`, else the vendored copy.
fn schema_dir() -> PathBuf {
    std::env::var_os("PABAL_CODEX_SCHEMAS").map_or_else(
        || PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/schemas/codex/rust-v0.155.1"),
        PathBuf::from,
    )
}

fn schema(event: &str, kind: &str) -> Value {
    let kebab: String = event
        .chars()
        .enumerate()
        .flat_map(|(i, c)| {
            let dash = (i > 0 && c.is_ascii_uppercase()).then_some('-');
            dash.into_iter().chain(c.to_lowercase())
        })
        .collect();
    let path = schema_dir().join(format!("{kebab}.command.{kind}.schema.json"));
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    serde_json::from_str(&text).unwrap()
}

fn errors(schema: &Value, instance: &Value) -> Vec<String> {
    jsonschema::validator_for(schema)
        .unwrap()
        .iter_errors(instance)
        .map(|e| format!("{} at {}", e, e.instance_path()))
        .collect()
}

fn payload(event: &str) -> Payload<Codex> {
    Payload::from_value(json!({"hook_event_name": event})).unwrap()
}

#[test]
fn codex_responses_match_the_output_schemas() {
    let pre = payload("PreToolUse");
    let pre = pre.pre_tool_use().unwrap();
    let input = Map::from_iter([("command".to_owned(), json!("ls"))]);
    let mut responses: Vec<(&str, Response)> = vec![
        ("PreToolUse", pre.deny("no")),
        ("PreToolUse", pre.add_context("x")),
        ("PreToolUse", pre.rewrite_input(input.clone(), None)),
        ("PreToolUse", pre.rewrite_input(input, Some("x"))),
    ];
    let post = payload("PostToolUse");
    let prompt = payload("UserPromptSubmit");
    let start = payload("SessionStart");
    let sub = payload("SubagentStart");
    responses.extend([
        (
            "PostToolUse",
            post.post_tool_use().unwrap().add_context("x"),
        ),
        (
            "UserPromptSubmit",
            prompt.user_prompt_submit().unwrap().add_context("x"),
        ),
        (
            "SessionStart",
            start.session_start().unwrap().add_context("x"),
        ),
        (
            "SubagentStart",
            sub.subagent_start().unwrap().add_context("x"),
        ),
    ]);
    assert!(
        !errors(&schema("PreToolUse", "output"), &json!({"bogus": 1})).is_empty(),
        "the schema rejects unknown fields"
    );
    let mut failures = Vec::new();
    for (event, response) in responses {
        let instance = response.json().unwrap();
        for e in errors(&schema(event, "output"), instance) {
            failures.push(format!("{event} {instance}: {e}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn codex_fixtures_match_the_input_schemas() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/codex");
    let mut failures = Vec::new();
    let mut checked = 0;
    for event_dir in std::fs::read_dir(&root).unwrap() {
        let event_dir = event_dir.unwrap().path();
        let event = event_dir.file_name().unwrap().to_str().unwrap().to_owned();
        let schema = schema(&event, "input");
        for file in std::fs::read_dir(&event_dir).unwrap() {
            let path = file.unwrap().path();
            let instance: Value =
                serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
            for e in errors(&schema, &instance) {
                failures.push(format!("{}: {e}", path.display()));
            }
            checked += 1;
        }
    }
    assert!(checked > 0, "no Codex fixtures under {}", root.display());
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
