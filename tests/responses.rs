use pabal::{
    AddContext, Allow, Antigravity, AnyHarness, AnyPayload, AnyView, Ask, ClaudeCode, Codex,
    Cursor, Deny, Harness, Payload, Response,
};
use serde_json::{Value, json};

fn parsed(r: &Response) -> Value {
    serde_json::from_str(&r.to_string()).unwrap()
}

fn payload<H: Harness>(event: &str) -> Payload<H> {
    Payload::from_value(json!({"hook_event_name": event, "session_id": "s"})).unwrap()
}

fn any(harness: AnyHarness, event: &str) -> AnyPayload {
    AnyPayload::from_value(harness, json!({"hook_event_name": event})).unwrap()
}

#[test]
fn deny() {
    let expected = json!({"hookSpecificOutput": {
        "hookEventName": "PreToolUse",
        "permissionDecision": "deny",
        "permissionDecisionReason": "use devrun"
    }});
    let c = payload::<ClaudeCode>("PreToolUse");
    let x = payload::<Codex>("PreToolUse");
    assert_eq!(
        parsed(&c.pre_tool_use().unwrap().deny("use devrun")),
        expected
    );
    assert_eq!(
        parsed(&x.pre_tool_use().unwrap().deny("use devrun")),
        expected
    );
}

#[test]
fn claude_code_can_ask_and_allow() {
    let c = payload::<ClaudeCode>("PreToolUse");
    let pre = c.pre_tool_use().unwrap();
    assert_eq!(
        parsed(&pre.ask("sure?")),
        json!({"hookSpecificOutput": {
            "hookEventName": "PreToolUse",
            "permissionDecision": "ask",
            "permissionDecisionReason": "sure?"
        }})
    );
    assert_eq!(
        parsed(&pre.allow_skipping_prompt()),
        json!({"hookSpecificOutput": {"hookEventName": "PreToolUse", "permissionDecision": "allow"}})
    );
}

#[test]
fn any_view_asks_and_allows_only_on_claude_code() {
    let c = any(AnyHarness::ClaudeCode, "PreToolUse");
    let AnyView::PreToolUse(pre) = c.view() else {
        panic!()
    };
    assert_eq!(
        parsed(&pre.ask("sure?").unwrap())["hookSpecificOutput"]["permissionDecision"],
        "ask"
    );
    assert_eq!(
        parsed(&pre.allow_skipping_prompt().unwrap())["hookSpecificOutput"]["permissionDecision"],
        "allow"
    );
    let x = any(AnyHarness::Codex, "PreToolUse");
    let AnyView::PreToolUse(pre) = x.view() else {
        panic!()
    };
    assert_eq!(pre.ask("sure?"), None);
    assert_eq!(pre.allow_skipping_prompt(), None);
}

#[test]
fn a_blank_deny_reason_is_replaced() {
    let x = payload::<Codex>("PreToolUse");
    for r in [
        x.pre_tool_use().unwrap().deny(" \n"),
        Response::deny_pre_tool_use(AnyHarness::Codex, ""),
    ] {
        let reason = parsed(&r)["hookSpecificOutput"]["permissionDecisionReason"].clone();
        assert!(!reason.as_str().unwrap().trim().is_empty(), "{reason}");
    }
}

fn context(event: &str) -> Value {
    json!({"hookSpecificOutput": {"hookEventName": event, "additionalContext": "x"}})
}

macro_rules! contexts {
    ($h:ty) => {
        vec![
            (
                "PreToolUse",
                payload::<$h>("PreToolUse")
                    .pre_tool_use()
                    .unwrap()
                    .add_context("x"),
            ),
            (
                "PostToolUse",
                payload::<$h>("PostToolUse")
                    .post_tool_use()
                    .unwrap()
                    .add_context("x"),
            ),
            (
                "UserPromptSubmit",
                payload::<$h>("UserPromptSubmit")
                    .user_prompt_submit()
                    .unwrap()
                    .add_context("x"),
            ),
            (
                "SessionStart",
                payload::<$h>("SessionStart")
                    .session_start()
                    .unwrap()
                    .add_context("x"),
            ),
            (
                "SubagentStart",
                payload::<$h>("SubagentStart")
                    .subagent_start()
                    .unwrap()
                    .add_context("x"),
            ),
        ]
    };
}

#[test]
fn hook_specific_context() {
    let batch = payload::<ClaudeCode>("PostToolBatch");
    let mut all = contexts!(ClaudeCode);
    all.extend(contexts!(Codex));
    all.push((
        "PostToolBatch",
        batch.post_tool_batch().unwrap().add_context("x"),
    ));
    for (event, response) in all {
        let v = parsed(&response);
        assert_eq!(v, context(event));
        assert!(v["hookSpecificOutput"].get("permissionDecision").is_none());
    }
}

#[test]
fn deny_without_a_payload() {
    let c = payload::<ClaudeCode>("PreToolUse");
    let x = payload::<Codex>("PreToolUse");
    assert_eq!(
        Response::deny_pre_tool_use(AnyHarness::ClaudeCode, "no"),
        c.pre_tool_use().unwrap().deny("no")
    );
    assert_eq!(
        Response::deny_pre_tool_use(AnyHarness::Codex, "no"),
        x.pre_tool_use().unwrap().deny("no")
    );
}

#[test]
fn none_writes_nothing() {
    assert_eq!(Response::none().to_string(), "");
    assert!(Response::none().is_none());
    assert!(Response::none().json().is_none());
}

#[test]
fn reasons_survive_json_escaping() {
    let reason = "a \"q\"\nü";
    let r = payload::<Codex>("PreToolUse")
        .pre_tool_use()
        .unwrap()
        .deny(reason);
    assert_eq!(
        parsed(&r)["hookSpecificOutput"]["permissionDecisionReason"],
        reason
    );
}

#[test]
fn the_warning_envelope_allows_without_a_permission_decision() {
    let p = any(AnyHarness::ClaudeCode, "PreToolUse");
    let AnyView::PreToolUse(pre) = p.view() else {
        panic!()
    };
    let w = parsed(&pre.add_context("not checked").unwrap());
    assert_eq!(w["hookSpecificOutput"]["additionalContext"], "not checked");
    assert!(w["hookSpecificOutput"].get("permissionDecision").is_none());
    let p = any(AnyHarness::Codex, "PreToolUse");
    let AnyView::PreToolUse(pre) = p.view() else {
        panic!()
    };
    assert_eq!(
        parsed(&pre.deny("no"))["hookSpecificOutput"]["permissionDecision"],
        "deny"
    );
}

#[test]
fn each_harness_gets_its_own_deny_envelope() {
    let p = any(AnyHarness::ClaudeCode, "PreToolUse");
    let AnyView::PreToolUse(pre) = p.view() else {
        panic!()
    };
    let cc = parsed(&pre.deny("use devrun"));
    assert_eq!(cc["hookSpecificOutput"]["permissionDecision"], "deny");
    assert_eq!(
        cc["hookSpecificOutput"]["permissionDecisionReason"],
        "use devrun"
    );
}

#[test]
fn any_view_context_on_every_shared_context_event() {
    for harness in [AnyHarness::ClaudeCode, AnyHarness::Codex] {
        for event in [
            "PostToolUse",
            "UserPromptSubmit",
            "SessionStart",
            "SubagentStart",
        ] {
            let p = any(harness, event);
            let r = match p.view() {
                AnyView::PostToolUse(v) => v.add_context("x").unwrap(),
                AnyView::UserPromptSubmit(v) => v.add_context("x").unwrap(),
                AnyView::SessionStart(v) => v.add_context("x"),
                AnyView::SubagentStart(v) => v.add_context("x").unwrap(),
                _ => panic!("{event}"),
            };
            assert_eq!(parsed(&r), context(event));
        }
    }
}

#[test]
fn cursor_denies_with_its_permission_envelope() {
    let expected = json!({"permission": "deny", "agent_message": "use devrun"});
    let pre = payload::<Cursor>("preToolUse");
    let shell = payload::<Cursor>("beforeShellExecution");
    let mcp = payload::<Cursor>("beforeMCPExecution");
    for r in [
        pre.pre_tool_use().unwrap().deny("use devrun"),
        shell.before_shell_execution().unwrap().deny("use devrun"),
        mcp.before_mcp_execution().unwrap().deny("use devrun"),
        Response::deny_pre_tool_use(AnyHarness::Cursor, "use devrun"),
    ] {
        assert_eq!(parsed(&r), expected);
    }
    let blank = shell.before_shell_execution().unwrap().deny(" ");
    assert!(
        !parsed(&blank)["agent_message"]
            .as_str()
            .unwrap()
            .trim()
            .is_empty()
    );
}

#[test]
fn cursor_asks_on_shell_and_mcp_execution() {
    let expected = json!({"permission": "ask", "user_message": "sure?"});
    let shell = payload::<Cursor>("beforeShellExecution");
    let mcp = payload::<Cursor>("beforeMCPExecution");
    assert_eq!(
        parsed(&shell.before_shell_execution().unwrap().ask("sure?")),
        expected
    );
    assert_eq!(
        parsed(&mcp.before_mcp_execution().unwrap().ask("sure?")),
        expected
    );
}

#[test]
fn cursor_context_is_a_top_level_field() {
    let expected = json!({"additional_context": "x"});
    let start = payload::<Cursor>("sessionStart");
    let post = payload::<Cursor>("postToolUse");
    assert_eq!(
        parsed(&start.session_start().unwrap().add_context("x")),
        expected
    );
    assert_eq!(
        parsed(&post.post_tool_use().unwrap().add_context("x")),
        expected
    );
    let p = any(AnyHarness::Cursor, "sessionStart");
    let AnyView::SessionStart(v) = p.view() else {
        panic!()
    };
    assert_eq!(parsed(&v.add_context("x")), expected);
}

#[test]
fn any_view_offers_cursor_only_what_it_honors() {
    let p = any(AnyHarness::Cursor, "preToolUse");
    let AnyView::PreToolUse(pre) = p.view() else {
        panic!()
    };
    assert_eq!(parsed(&pre.deny("no"))["permission"], "deny");
    assert_eq!(pre.ask("sure?"), None);
    assert_eq!(pre.allow_skipping_prompt(), None);
    assert_eq!(pre.add_context("x"), None);
    for event in ["beforeSubmitPrompt", "subagentStart"] {
        let p = any(AnyHarness::Cursor, event);
        let r = match p.view() {
            AnyView::UserPromptSubmit(v) => v.add_context("x"),
            AnyView::SubagentStart(v) => v.add_context("x"),
            _ => panic!("{event}"),
        };
        assert_eq!(r, None, "{event}");
    }
}

#[test]
fn antigravity_decides_with_its_decision_envelope() {
    let p = payload::<Antigravity>("PreToolUse");
    let pre = p.pre_tool_use().unwrap();
    assert_eq!(
        parsed(&pre.deny("use devrun")),
        json!({"decision": "deny", "reason": "use devrun"})
    );
    assert_eq!(
        parsed(&pre.ask("sure?")),
        json!({"decision": "ask", "reason": "sure?"})
    );
    assert_eq!(
        parsed(&pre.allow_skipping_prompt()),
        json!({"decision": "allow"})
    );
    assert_eq!(
        Response::deny_pre_tool_use(AnyHarness::Antigravity, "use devrun"),
        pre.deny("use devrun")
    );
}

#[test]
fn any_view_offers_antigravity_only_what_it_honors() {
    let p = AnyPayload::parse_named(AnyHarness::Antigravity, "PreToolUse", "{}").unwrap();
    let AnyView::PreToolUse(pre) = p.view() else {
        panic!()
    };
    assert_eq!(parsed(&pre.ask("sure?").unwrap())["decision"], "ask");
    assert_eq!(
        parsed(&pre.allow_skipping_prompt().unwrap())["decision"],
        "allow"
    );
    assert_eq!(pre.add_context("x"), None);
    let p = AnyPayload::parse_named(AnyHarness::Antigravity, "PostToolUse", "{}").unwrap();
    let AnyView::PostToolUse(post) = p.view() else {
        panic!()
    };
    assert_eq!(post.add_context("x"), None);
}
