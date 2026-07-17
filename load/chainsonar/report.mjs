#!/usr/bin/env node
// report.mjs — aggregate the field-test logs into the four questions.
//
// Reads logs/brainiac-calls.jsonl (+ activity.jsonl) and prints, and writes,
// the metrics that back the findings report. No scoring — evidence.

import { readFile, writeFile, mkdir } from "node:fs/promises";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const HERE = dirname(fileURLToPath(import.meta.url));
const LOG_DIR = process.env.BX_LOG_DIR || join(HERE, "logs");
const OUT_DIR = join(HERE, "runs", process.argv[2] || "2026-07-16");

async function readJsonl(file) {
  try {
    const text = await readFile(file, "utf8");
    return text.trim().split("\n").filter(Boolean).map((l) => JSON.parse(l));
  } catch {
    return [];
  }
}

const pct = (n, d) => (d ? Math.round((n / d) * 100) : 0);

async function main() {
  const calls = await readJsonl(join(LOG_DIR, "brainiac-calls.jsonl"));
  const activity = await readJsonl(join(LOG_DIR, "activity.jsonl"));

  const agents = [...new Set(calls.map((c) => c.agent))].sort();
  const cmds = [...new Set(calls.map((c) => c.cmd))].sort();

  // Q1 REACH — calls per agent × command.
  const reach = {};
  for (const a of agents) {
    reach[a] = {};
    for (const c of cmds) reach[a][c] = calls.filter((x) => x.agent === a && x.cmd === c).length;
    reach[a].total = calls.filter((x) => x.agent === a).length;
  }

  // module coverage: did each of the three modules get touched?
  const memoryCmds = ["memory-search", "memory-add", "memory-context", "memory-feedback"];
  const kbCmds = ["doc-search", "doc-get"];
  const libCmds = ["standards-for", "standard-propose", "skill-search", "skill-fetch", "skill-report"];
  const touched = (cmdset) => calls.filter((c) => cmdset.includes(c.cmd)).length;
  const modules = {
    memory: touched(memoryCmds),
    knowledge_base: touched(kbCmds),
    library: touched(libCmds),
  };

  // Q4 FRICTION — every non-ok outcome, plus empties (a fact, not a failure).
  const outcomes = {};
  for (const c of calls) outcomes[c.outcome] = (outcomes[c.outcome] || 0) + 1;
  const errors = calls
    .filter((c) => c.outcome === "error" || c.outcome === "network_error")
    .map((c) => ({ agent: c.agent, cmd: c.cmd, status: c.status, detail: c.detail }));
  const empties = calls
    .filter((c) => c.outcome === "empty")
    .map((c) => ({ agent: c.agent, cmd: c.cmd, args: c.args }));

  // latency, by command
  const latency = {};
  for (const c of cmds) {
    const ls = calls.filter((x) => x.cmd === c).map((x) => x.latency_ms).sort((a, b) => a - b);
    if (ls.length) latency[c] = { n: ls.length, p50: ls[Math.floor(ls.length / 2)], max: ls[ls.length - 1] };
  }

  // reads vs writes — the shape of use
  const reads = calls.filter((c) => /search|for|get|context|fetch/.test(c.cmd)).length;
  const writes = calls.filter((c) => /add|propose|feedback|report/.test(c.cmd)).length;

  const metrics = {
    generated_at: new Date().toISOString(),
    totals: { calls: calls.length, activity_notes: activity.length, agents: agents.length },
    reach,
    module_coverage: modules,
    read_write: { reads, writes, read_pct: pct(reads, reads + writes) },
    outcomes,
    empties_pct: pct(outcomes.empty || 0, calls.length),
    error_pct: pct((outcomes.error || 0) + (outcomes.network_error || 0), calls.length),
    latency_ms: latency,
    errors,
    empties,
  };

  await mkdir(OUT_DIR, { recursive: true });
  await writeFile(join(OUT_DIR, "metrics.json"), JSON.stringify(metrics, null, 2));

  // human summary to stdout
  console.log(`\n=== ChainSonar field test — ${metrics.totals.calls} Brainiac calls, ${agents.length} agents ===\n`);
  console.log("REACH (calls per agent):");
  for (const a of agents) console.log(`  ${a.padEnd(12)} ${reach[a].total}`);
  console.log("\nMODULE COVERAGE:");
  for (const [m, n] of Object.entries(modules)) console.log(`  ${m.padEnd(16)} ${n} calls`);
  console.log(`\nREAD/WRITE: ${reads} reads / ${writes} writes (${metrics.read_write.read_pct}% read)`);
  console.log("\nOUTCOMES:", outcomes);
  console.log(`  empty: ${metrics.empties_pct}%   error: ${metrics.error_pct}%`);
  if (errors.length) {
    console.log("\nFRICTION — errors:");
    for (const e of errors) console.log(`  ${e.agent} ${e.cmd} → ${e.status} ${e.detail ?? ""}`);
  }
  console.log(`\nwrote ${join(OUT_DIR, "metrics.json")}`);
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
