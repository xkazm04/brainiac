#!/usr/bin/env node
// bx — the ChainSonar field test's Brainiac client (load/README.md F-1/F-2).
//
// Every developer and scanner reaches Brainiac through this one CLI. It mirrors
// the MCP tool vocabulary 1:1 over REST, and — the whole reason it exists —
// logs every call with timing and outcome, so the harness can measure reach,
// unprompted use, travel, and friction without instrumenting the product.
//
// It is deliberately dumb: no retries, no caching, no cleverness. A wrapper
// that "helped" would hide the friction we are trying to observe.
//
// Auth + identity come from the environment, set per-agent by the launcher:
//   BX_API_URL   default http://127.0.0.1:8600
//   BX_TOKEN     the agent's scoped brk_ key (required)
//   BX_AGENT     the agent id written into every log line (required)
//   BX_LOG_DIR   where the two jsonl logs go (default load/chainsonar/logs)
//
// Usage:  node bx.mjs <command> [--flag value ...]
//   memory-search   --query "..." [--k 10]
//   memory-add      --content "..." [--kind fact|decision|pattern|pitfall|howto] [--entities "a,b,c"]
//   source-status   --id <uuid>
//   memory-context  --task "..."
//   memory-feedback --id <uuid> --verdict helpful|wrong|outdated [--note "..."]
//   doc-search      --query "..."
//   doc-get         --slug "..."
//   standards-for   [--stack rust] [--category errors]
//   standard-propose --name "..." --statement "..." [--stack --category --rationale --examples --evidence]
//   skill-propose   --name "..." --instructions "..." [--summary --domain]
//   skill-search    --query "..."
//   skill-fetch     --slug "..."
//   skill-report    --kind standard|skill --slug "..." --event check|apply
//   log             --note "..." [--phase "..."]   (activity log only; no API call)
//
// FILE INPUT (F-8): any --flag also accepts --flag-file <path>, whose contents
// become the value. Reach for it whenever the value is CODE or prose — backticks,
// `$`, and apostrophes all break shell quoting, and a runbook or examples block
// should never be reworded to survive a shell. Examples:
//   bx skill-propose   --name "add a provider" --instructions-file ./runbook.md
//   bx standard-propose --name "typed throttle" --statement "..." --examples-file ./ex.ts
//   bx memory-add      --content-file ./note.md --kind pitfall

import { appendFile, mkdir, readFile } from "node:fs/promises";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const HERE = dirname(fileURLToPath(import.meta.url));
const API = process.env.BX_API_URL || "http://127.0.0.1:8600";
const TOKEN = process.env.BX_TOKEN || "";
const AGENT = process.env.BX_AGENT || "unknown";
const LOG_DIR = process.env.BX_LOG_DIR || join(HERE, "logs");
const CALLS_LOG = join(LOG_DIR, "brainiac-calls.jsonl");
const ACTIVITY_LOG = join(LOG_DIR, "activity.jsonl");

// A stable clock the workflow forbids in scripts but a CLI may use.
const now = () => new Date().toISOString();

function parseArgs(argv) {
  const out = {};
  for (let i = 0; i < argv.length; i += 2) {
    const k = argv[i];
    if (!k?.startsWith("--")) continue;
    out[k.slice(2)] = argv[i + 1];
  }
  return out;
}

/**
 * F-8: resolve `--<field>-file <path>` into `<field>`. `bx` is deliberately
 * dumb about everything else, but the ONE friction worth removing is that a
 * code-oriented tool made agents reword real TypeScript (backticks, `$`,
 * apostrophes) into shell-safe prose. A file is read verbatim, so the value the
 * product receives is the value on disk — byte for byte, no shell in the middle.
 * A single trailing newline (every editor adds one) is dropped; nothing else is
 * touched. If both `--x` and `--x-file` are given, the file wins and says so.
 */
async function resolveFileArgs(args) {
  for (const key of Object.keys(args)) {
    if (!key.endsWith("-file")) continue;
    const base = key.slice(0, -"-file".length);
    const path = args[key];
    delete args[key];
    if (path == null) continue;
    let content;
    try {
      content = await readFile(path, "utf8");
    } catch (e) {
      console.error(`bx: cannot read --${key} ${path}: ${e.message}`);
      process.exit(2);
    }
    if (args[base] !== undefined) {
      console.error(`bx: --${key} overrides --${base} (the file wins)`);
    }
    args[base] = content.endsWith("\n") ? content.slice(0, -1) : content;
  }
  return args;
}

async function append(file, obj) {
  await mkdir(LOG_DIR, { recursive: true });
  await appendFile(file, JSON.stringify(obj) + "\n");
}

/** The telemetry line — the harness's primary evidence. */
async function logCall(cmd, args, res) {
  await append(CALLS_LOG, {
    ts: now(),
    agent: AGENT,
    cmd,
    // Args are logged trimmed: enough to see WHAT was asked, not a transcript.
    args: Object.fromEntries(
      Object.entries(args).map(([k, v]) => [k, typeof v === "string" && v.length > 120 ? v.slice(0, 120) + "…" : v]),
    ),
    status: res.status,
    outcome: res.outcome,
    latency_ms: res.latency_ms,
    bytes: res.bytes,
    ...(res.detail ? { detail: res.detail } : {}),
  });
}

async function api(method, path, body) {
  const started = Date.now();
  const headers = { authorization: `Bearer ${TOKEN}` };
  if (body !== undefined) headers["content-type"] = "application/json";
  let status = 0;
  let text = "";
  try {
    const r = await fetch(`${API}${path}`, {
      method,
      headers,
      body: body !== undefined ? JSON.stringify(body) : undefined,
    });
    status = r.status;
    text = await r.text();
  } catch (e) {
    return { status: 0, outcome: "network_error", latency_ms: Date.now() - started, bytes: 0, detail: String(e) };
  }
  const latency_ms = Date.now() - started;
  const bytes = Buffer.byteLength(text);
  let json;
  try {
    json = text ? JSON.parse(text) : null;
  } catch {
    json = null;
  }
  const ok = status >= 200 && status < 300;
  // "empty" is a first-class outcome, not a failure: a search that finds
  // nothing is a fact about the corpus, and the harness counts it separately.
  const emptyArrays = ["hits", "memories", "standards", "skills", "pages", "documents"];
  const empty =
    ok && json && emptyArrays.some((k) => Array.isArray(json[k]) && json[k].length === 0);
  return {
    status,
    outcome: ok ? (empty ? "empty" : "ok") : "error",
    latency_ms,
    bytes,
    json,
    detail: ok ? undefined : (json?.error ?? text.slice(0, 200)),
  };
}

const q = (obj) => "?" + new URLSearchParams(Object.entries(obj).filter(([, v]) => v != null)).toString();

const COMMANDS = {
  "memory-search": (a) =>
    api("POST", "/v1/memories/search", { query: a.query, k: a.k ? Number(a.k) : 10 }),
  "memory-add": (a) =>
    api("POST", "/v1/memories", {
      content: a.content,
      // F-4: an optional kind + entity hints. For a `manual` add the kind is
      // authoritative — the memory is stored under exactly it, no guessing.
      ...(a.kind ? { kind: a.kind } : {}),
      ...(a.entities ? { entities: a.entities.split(",").map((s) => s.trim()).filter(Boolean) } : {}),
    }),
  // The loop-closer (F-1/F-2, fixed 2026-07-16): poll the source_id from
  // memory-add until status=processed; results.memory_ids are then real
  // memories to cite (standard-propose --evidence) or feed back on.
  "source-status": (a) => api("GET", `/v1/sources/${encodeURIComponent(a.id)}`),
  "memory-context": (a) =>
    api("POST", "/v1/memories/search", { query: a.task, k: 12 }), // context ≈ a broad search over REST
  "memory-feedback": (a) =>
    api("POST", `/v1/memories/${a.id}/feedback`, { verdict: a.verdict, ...(a.note ? { note: a.note } : {}) }),
  // F-5 (fixed 2026-07-16): REST now has real doc search. `--query` filters
  // server-side over title/slug/body; omit it to list every page.
  "doc-search": (a) => api("GET", `/v1/docs${a.query ? q({ q: a.query }) : ""}`),
  "doc-get": (a) => api("GET", `/v1/docs/${encodeURIComponent(a.slug)}`),
  "standards-for": (a) =>
    api("GET", `/v1/library/standards${q({ stack: a.stack, lifecycle: "adopted" })}`),
  "standard-propose": (a) =>
    api("POST", "/v1/library/standards/propose", {
      name: a.name,
      statement: a.statement,
      ...(a.stack ? { stack: a.stack } : {}),
      ...(a.category ? { category: a.category } : {}),
      ...(a.rationale ? { rationale: a.rationale } : {}),
      ...(a.examples ? { examples_md: a.examples } : {}),
      ...(a.evidence ? { evidence_memory_id: a.evidence } : {}),
    }),
  // F-4: propose a skill (a runbook/checklist) as a DRAFT. Reach for
  // --instructions-file for anything with code — that is the whole point of F-8.
  "skill-propose": (a) =>
    api("POST", "/v1/library/skills/propose", {
      name: a.name,
      instructions_md: a.instructions,
      ...(a.summary ? { summary: a.summary } : {}),
      ...(a.domain ? { domain: a.domain } : {}),
    }),
  "skill-search": (a) => api("GET", "/v1/library/skills"), // catalog; agent filters by a.query client-side
  "skill-fetch": (a) => api("GET", `/v1/library/skills/${encodeURIComponent(a.slug)}/download`),
  // F-5 (fixed 2026-07-16): REST /usage now accepts a slug, matching the MCP
  // tool — pass --slug (what standards-for/skill-fetch hand back). --id still
  // works for a caller that already has the UUID.
  "skill-report": (a) =>
    api("POST", "/v1/library/usage", {
      artifact_kind: a.kind,
      ...(a.slug ? { artifact_slug: a.slug } : { artifact_id: a.id }),
      event: a.event,
    }),
};

async function main() {
  const [cmd, ...rest] = process.argv.slice(2);
  const args = await resolveFileArgs(parseArgs(rest));

  // `log` is the activity channel: what the agent THINKS it is doing. No API
  // call, so it never touches the calls log — the two streams stay clean.
  if (cmd === "log") {
    await append(ACTIVITY_LOG, { ts: now(), agent: AGENT, phase: args.phase ?? null, note: args.note ?? "" });
    console.log("logged");
    return;
  }

  const fn = COMMANDS[cmd];
  if (!fn) {
    console.error(`unknown command: ${cmd}\ncommands: ${Object.keys(COMMANDS).join(", ")}, log`);
    process.exit(2);
  }
  if (!TOKEN) {
    console.error("BX_TOKEN is not set — the launcher must export a scoped key");
    process.exit(2);
  }

  const res = await fn(args);
  await logCall(cmd, args, res);

  // The agent sees the payload on stdout and the outcome on stderr, so a
  // wrapper script can branch on exit code without parsing.
  if (res.json !== undefined && res.json !== null) {
    console.log(JSON.stringify(res.json, null, 2));
  }
  if (res.outcome === "error" || res.outcome === "network_error") {
    console.error(`bx ${cmd}: ${res.outcome} (${res.status}) ${res.detail ?? ""}`);
    process.exit(1);
  }
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
