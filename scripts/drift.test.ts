import { expect, test } from "bun:test";
import { claudeEvents, claudeRequiredFields, codexEvents, diff } from "./drift";

test("codexEvents maps input schema file names", () => {
  expect(
    codexEvents([
      "pre-tool-use.command.input.schema.json",
      "session-end.command.input.schema.json",
      "stop.command.output.schema.json",
    ]),
  ).toEqual(["PreToolUse", "SessionEnd"]);
});

test("claudeEvents reads HOOK_EVENTS", () => {
  expect(claudeEvents("export declare const HOOK_EVENTS: readonly ['PreToolUse', 'Setup'];")).toEqual([
    "PreToolUse",
    "Setup",
  ]);
});

test("claudeRequiredFields reads a HookInput type", () => {
  const dts =
    "export declare type PreToolUseHookInput = BaseHookInput & {\n    hook_event_name: 'PreToolUse';\n    tool_name: string;\n    mcp_server?: X;\n};";
  expect(claudeRequiredFields(dts, "PreToolUseHookInput")).toEqual(["hook_event_name", "tool_name"]);
});

test("diff reports added and removed", () => {
  expect(diff(["A", "B"], ["B", "C"])).toEqual({ added: ["A"], removed: ["C"] });
});

test("claudeEvents reads the SDK 0.3.283 typings", async () => {
  const dts = await Bun.file(`${import.meta.dir}/testdata/sdk.d.ts`).text();
  expect(claudeEvents(dts).toSorted()).toEqual(
    [
      "SessionStart", "Setup", "UserPromptSubmit", "UserPromptExpansion", "PreToolUse",
      "PermissionRequest", "PermissionDenied", "PostToolUse", "PostToolUseFailure", "PostToolBatch",
      "Notification", "MessageDisplay", "SubagentStart", "SubagentStop", "TaskCreated", "TaskCompleted",
      "Stop", "StopFailure", "TeammateIdle", "InstructionsLoaded", "ConfigChange", "CwdChanged",
      "DirectoryAdded", "FileChanged", "WorktreeCreate", "WorktreeRemove", "PreCompact", "PostCompact",
      "PreModelSwitch", "PostModelSwitch", "Elicitation", "ElicitationResult", "SessionEnd",
    ].toSorted(),
  );
  expect(claudeRequiredFields(dts, "BaseHookInput")).toEqual(["session_id", "transcript_path", "cwd"]);
  expect(claudeRequiredFields(dts, "SessionEndHookInput")).toEqual(["hook_event_name", "reason"]);
});
