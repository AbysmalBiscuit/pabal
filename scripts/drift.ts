// Checks the crate's events and fixtures against each harness's upstream source
// and files one `drift` issue per harness. `--dry-run` prints the issues instead.

import { $ } from "bun";
import { mkdtemp, readdir } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

const root = join(import.meta.dir, "..");
const spec = "docs/superpowers/specs/2026-09-26-pabal-design.md";

export function codexEvents(fileNames: string[]): string[] {
  return fileNames.flatMap((name) => {
    const kebab = name.match(/^(.+)\.command\.input\.schema\.json$/)?.[1];
    if (!kebab) return [];
    return [kebab.split("-").map((w) => w[0].toUpperCase() + w.slice(1)).join("")];
  });
}

export function claudeEvents(dts: string): string[] {
  const list = dts.match(/export declare const HOOK_EVENTS: readonly \[([^\]]*)\]/)?.[1];
  if (list === undefined) throw new Error("HOOK_EVENTS not found in sdk.d.ts");
  return [...list.matchAll(/'([^']+)'/g)].map((m) => m[1]);
}

/** The non-optional top-level fields of every `{ ... }` in `export declare type <type> = ...;`. */
export function claudeRequiredFields(dts: string, type: string): string[] {
  const start = dts.indexOf(`export declare type ${type} = `);
  if (start < 0) return [];
  let depth = 0;
  let lineDepth = 0;
  let line = "";
  const fields: string[] = [];
  for (const ch of dts.slice(start)) {
    if (ch === ";" && depth === 0) break;
    if (ch === "{") depth++;
    if (ch === "}") depth--;
    if (ch !== "\n") {
      line += ch;
      continue;
    }
    const field = lineDepth === 1 ? line.match(/^\s*(\w+):/)?.[1] : undefined;
    if (field) fields.push(field);
    line = "";
    lineDepth = depth;
  }
  return fields;
}

/** Every `#### <event>` heading under `### Hook events` in Cursor's hooks reference, split at ` / `. */
export function cursorEvents(md: string): string[] {
  const start = md.indexOf("\n### Hook events");
  if (start < 0) throw new Error("### Hook events not found in Cursor's hooks reference");
  const end = md.indexOf("\n## ", start);
  const section = md.slice(start, end < 0 ? undefined : end);
  return [...section.matchAll(/^#### (.+)$/gm)].flatMap((m) => m[1].split(" / ").map((e) => e.trim()));
}

/** The first-column events of the table under `## Supported events` in Antigravity's hooks docs. */
export function antigravityEvents(md: string): string[] {
  const start = md.search(/\n## Supported events/i);
  if (start < 0) throw new Error("## Supported events not found in Antigravity's hooks docs");
  const end = md.slice(start + 1).search(/\n#{2,3} /);
  const section = md.slice(start, end < 0 ? undefined : start + 1 + end);
  return [...section.matchAll(/^\| `(\w+)` \|/gm)].map((m) => m[1]);
}

/**
 * The first alias of each event in grok-build's `hook_events! { ... }` list: the PascalCase name
 * Grok sends as `hook_event_name`. An event whose `display` repeats an earlier one is an alias
 * Grok collapses before dispatch (`SubagentEnd` for `SubagentStop`), so it is skipped.
 */
export function grokEvents(rs: string): string[] {
  const start = rs.indexOf("\nhook_events! {");
  if (start < 0) throw new Error("hook_events! { not found in xai-grok-hooks/src/event.rs");
  const end = rs.indexOf("\n}", start);
  const list = rs.slice(start, end < 0 ? undefined : end);
  const seen = new Set<string>();
  return [...list.matchAll(/^ {4}\w+ \{\s*display: "([^"]+)",\s*aliases: \[\s*"([^"]+)"/gm)].flatMap(
    ([, display, name]) => {
      if (seen.has(display)) return [];
      seen.add(display);
      return [name];
    },
  );
}

export function diff(upstream: string[], crate: string[]): { added: string[]; removed: string[] } {
  return {
    added: upstream.filter((e) => !crate.includes(e)),
    removed: crate.filter((e) => !upstream.includes(e)),
  };
}

type Harness = "claude-code" | "codex" | "cursor" | "antigravity" | "grok";

function eventFindings(upstream: string[], crate: string[], source: string): string[] {
  const { added, removed } = diff(upstream, crate);
  return [
    ...added.map((e) => `- \`${e}\` in ${source} has no variant.`),
    ...removed.map((e) => `- Variant \`${e}\` is not in ${source}.`),
  ];
}

async function codex(crate: string[]): Promise<string[]> {
  const tag = (await $`gh api repos/openai/codex/releases/latest --jq .tag_name`.text()).trim();
  if (!tag.startsWith("rust-v")) throw new Error(`latest openai/codex release ${tag} is not rust-v*`);
  const dir = await mkdtemp(join(tmpdir(), "pabal-codex-"));
  const path = "codex-rs/hooks/schema/generated";
  const entries: { name: string; download_url: string }[] =
    await $`gh api ${`repos/openai/codex/contents/${path}?ref=${tag}`}`.json();
  for (const entry of entries.filter((e) => e.name.endsWith(".json"))) {
    const res = await fetch(entry.download_url);
    if (!res.ok) throw new Error(`${entry.download_url}: ${res.status}`);
    await Bun.write(join(dir, entry.name), await res.text());
  }
  const findings = eventFindings(codexEvents(await readdir(dir)), crate, tag);
  const schema =
    await $`cargo nextest run --all-features -E ${"binary(codex_schema)"}`.cwd(root).env({ ...process.env, PABAL_CODEX_SCHEMAS: dir }).nothrow().quiet();
  if (schema.exitCode !== 0) {
    const tail = schema.stderr.toString().split("\n").slice(-40).join("\n");
    findings.push(`- Fixtures or responses fail the ${tag} schemas:\n\n\`\`\`\n${tail}\n\`\`\``);
  }
  return findings;
}

async function claude(crate: string[]): Promise<string[]> {
  const latest = await fetch("https://registry.npmjs.org/@anthropic-ai/claude-agent-sdk/latest");
  if (!latest.ok) throw new Error(`npm registry: ${latest.status}`);
  const { version, dist } = (await latest.json()) as { version: string; dist: { tarball: string } };
  const dir = await mkdtemp(join(tmpdir(), "pabal-claude-"));
  const tgz = await fetch(dist.tarball);
  if (!tgz.ok) throw new Error(`${dist.tarball}: ${tgz.status}`);
  await Bun.write(join(dir, "sdk.tgz"), tgz);
  await $`tar -xzf ${join(dir, "sdk.tgz")} -C ${dir} package/sdk.d.ts`;
  const dts = await Bun.file(join(dir, "package/sdk.d.ts")).text();

  const findings = eventFindings(claudeEvents(dts), crate, `SDK ${version}`);
  const base = claudeRequiredFields(dts, "BaseHookInput");
  const fixtures = join(root, "tests/fixtures/claude-code");
  for (const event of await readdir(fixtures)) {
    const payloads = await Promise.all(
      (await readdir(join(fixtures, event))).map(async (f) => Bun.file(join(fixtures, event, f)).json()),
    );
    const required = [...base, ...claudeRequiredFields(dts, `${event}HookInput`)];
    const missing = required.filter((field) => !payloads.some((p) => field in p));
    for (const field of missing) {
      findings.push(`- \`${event}\` fixtures lack required field \`${field}\` (SDK ${version}).`);
    }
  }
  return findings;
}

async function cursor(crate: string[]): Promise<string[]> {
  const url = "https://cursor.com/docs/hooks.md";
  const res = await fetch(url);
  if (!res.ok) throw new Error(`${url}: ${res.status}`);
  return eventFindings(cursorEvents(await res.text()), crate, url);
}

async function antigravity(crate: string[]): Promise<string[]> {
  const url = "https://antigravity.google/docs/hooks.md";
  const res = await fetch(url);
  if (!res.ok) throw new Error(`${url}: ${res.status}`);
  return eventFindings(antigravityEvents(await res.text()), crate, url);
}

async function grok(crate: string[]): Promise<string[]> {
  const path = "crates/codegen/xai-grok-hooks/src/event.rs";
  const sha = (await $`gh api repos/xai-org/grok-build/commits/main --jq .sha`.text()).trim();
  const url = `https://raw.githubusercontent.com/xai-org/grok-build/${sha}/${path}`;
  const res = await fetch(url);
  if (!res.ok) throw new Error(`${url}: ${res.status}`);
  return eventFindings(grokEvents(await res.text()), crate, `grok-build ${sha.slice(0, 7)} ${path}`);
}

async function report(harness: Harness, findings: string[], dryRun: boolean) {
  const title = `drift: ${harness}`;
  const body = [
    `The weekly drift check found differences for ${harness}:`,
    "",
    ...findings,
    "",
    `@claude please open a PR that adds fixtures and enum variants for these changes, following ${spec}.`,
  ].join("\n");
  if (dryRun) {
    console.log(`## ${title}\n\n${body}\n`);
    return;
  }
  const open: { number: number; title: string }[] =
    await $`gh issue list --label drift --state open --json number,title`.json();
  const existing = open.find((issue) => issue.title === title);
  if (existing) {
    await $`gh issue edit ${existing.number} --body ${body}`;
  } else {
    await $`gh issue create --label drift --title ${title} --body ${body}`;
  }
}

async function main() {
  const dryRun = process.argv.includes("--dry-run");
  const crate: Record<Harness, string[]> = await $`cargo run -q --example events`.cwd(root).json();
  const results: [Harness, string[]][] = [
    ["codex", await codex(crate.codex)],
    ["claude-code", await claude(crate["claude-code"])],
    ["cursor", await cursor(crate.cursor)],
    ["antigravity", await antigravity(crate.antigravity)],
    ["grok", await grok(crate.grok)],
  ];
  for (const [harness, findings] of results) {
    if (findings.length > 0) await report(harness, findings, dryRun);
    else console.log(`${harness}: no drift`);
  }
}

if (import.meta.main) {
  main().catch((e) => {
    console.error(e);
    process.exit(1);
  });
}
