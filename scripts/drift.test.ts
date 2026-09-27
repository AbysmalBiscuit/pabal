import { expect, test } from "bun:test";
import { claudeEvents, claudeRequiredFields, codexEvents, cursorEvents, diff } from "./drift";

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

test("claudeRequiredFields reads every object of an intersection", () => {
  const dts = [
    "export declare type PreModelSwitchHookInput = (BaseHookInput & {",
    "    hook_event_name: 'PreModelSwitch';",
    "}) & {",
    "    from_model: string;",
    "    requested_model: string | null;",
    "    reason?: string;",
    "};",
    "export declare type Next = {",
    "    other: string;",
    "};",
  ].join("\n");
  expect(claudeRequiredFields(dts, "PreModelSwitchHookInput")).toEqual([
    "hook_event_name",
    "from_model",
    "requested_model",
  ]);
});

test("claudeRequiredFields skips doc comments and nested fields", () => {
  const dts = [
    "export declare type BaseHookInput = {",
    "    session_id: string;",
    "    /**",
    "     * Absent until: the first prompt.",
    "     */",
    "    prompt_id?: string;",
    "    usage: {",
    "        tokens: number;",
    "    };",
    "    cwd: string;",
    "};",
  ].join("\n");
  expect(claudeRequiredFields(dts, "BaseHookInput")).toEqual(["session_id", "usage", "cwd"]);
});

test("cursorEvents reads the headings under Hook events", () => {
  const md = [
    "### Common schema",
    "#### Input (all hooks)",
    "### Hook events",
    "#### preToolUse",
    "Called before any tool execution.",
    "#### beforeShellExecution / beforeMCPExecution",
    "#### workspaceOpen",
    "## Environment Variables",
    "#### notAnEvent",
  ].join("\n");
  expect(cursorEvents(md)).toEqual(["preToolUse", "beforeShellExecution", "beforeMCPExecution", "workspaceOpen"]);
});
