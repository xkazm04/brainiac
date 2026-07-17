#!/usr/bin/env node
// setup.mjs — provision the ChainSonar field-test org's five scoped keys.
//
// The org itself is bootstrapped by an admin env token (BX_ADMIN_TOKEN) bound
// to a fixed ChainSonar org UUID — see load/chainsonar/README-run.md for the
// BRAINIAC_TOKENS line and the one server restart it needs. This script then
// mints, through the REAL `POST /v1/tokens` endpoint (the path F-1 unblocked),
// the five keys the cast holds:
//
//   scan       scanners: read + write + lib:propose + kb:read + lib:read
//   dev-a      new development (Opus): read+write+lib:read+lib:propose+kb:read
//   dev-b      refactor      (Sonnet): same
//   dev-c      UI scale      (Opus):   same
//   maintainer me, the gate:          admin (mints nothing here; used to triage)
//
// No developer key carries lib:publish or admin (decision F3/F4): they propose,
// a human adopts. Each key acts as a DISTINCT user so the store attributes
// their proposals to different authors and the per-author rate limit is real.
//
//   BX_API_URL       default http://127.0.0.1:8600
//   BX_ADMIN_TOKEN   the ChainSonar-org admin env token (required)
//   CHAINSONAR_ORG   the org UUID the admin token is bound to (required)

import { writeFile, mkdir } from "node:fs/promises";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { randomUUID } from "node:crypto";

const HERE = dirname(fileURLToPath(import.meta.url));
const API = process.env.BX_API_URL || "http://127.0.0.1:8600";
const ADMIN = process.env.BX_ADMIN_TOKEN;
const ORG = process.env.CHAINSONAR_ORG;

if (!ADMIN || !ORG) {
  console.error("BX_ADMIN_TOKEN and CHAINSONAR_ORG must be set (see README-run.md)");
  process.exit(2);
}

// The cast. Distinct user ids so proposals attribute to distinct authors and
// the per-author proposal budget is per-developer, not shared.
const CAST = [
  { agent: "scan", user: randomUUID(), scopes: ["read", "write", "kb:read", "lib:read", "lib:propose"] },
  { agent: "dev-a", user: randomUUID(), scopes: ["read", "write", "kb:read", "lib:read", "lib:propose"] },
  { agent: "dev-b", user: randomUUID(), scopes: ["read", "write", "kb:read", "lib:read", "lib:propose"] },
  { agent: "dev-c", user: randomUUID(), scopes: ["read", "write", "kb:read", "lib:read", "lib:propose"] },
  { agent: "maintainer", user: randomUUID(), scopes: ["admin"] },
];

async function mint({ agent, user, scopes }) {
  const r = await fetch(`${API}/v1/tokens`, {
    method: "POST",
    headers: { authorization: `Bearer ${ADMIN}`, "content-type": "application/json" },
    body: JSON.stringify({ name: `chainsonar-${agent}`, user_id: user, scopes }),
  });
  const text = await r.text();
  if (r.status !== 201) {
    throw new Error(`mint ${agent} failed (${r.status}): ${text}`);
  }
  const body = JSON.parse(text);
  return { agent, user, scopes, token: body.token, prefix: body.prefix };
}

async function main() {
  const keys = {};
  for (const member of CAST) {
    const k = await mint(member);
    keys[member.agent] = k;
    console.log(`minted ${member.agent}: ${k.prefix}… [${member.scopes.join(" ")}]`);
  }
  const out = { org: ORG, api: API, minted_at: new Date().toISOString(), keys };
  await mkdir(HERE, { recursive: true });
  await writeFile(join(HERE, ".keys.json"), JSON.stringify(out, null, 2));
  console.log(`\nwrote ${join(HERE, ".keys.json")} — gitignored, do not commit`);
}

main().catch((e) => {
  console.error(e.message);
  process.exit(1);
});
