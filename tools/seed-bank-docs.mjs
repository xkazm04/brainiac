/*
 * Seed the bank's knowledge base — ~1,050 pages. See docs/BANK-CORPUS.md §4.
 *
 * WHY THIS IS NOT A COMPOSE. A page in this product is a projection: the
 * composer reads the canonical memories bound to a section and writes the prose
 * (compose_sweep). Doing that for a thousand pages is a thousand model calls,
 * which is not a thing to spend on a density test. So this writes the rows the
 * composer would have left behind — real bindings, real section structure, a
 * real revision — with generated prose in the body.
 *
 * What that means for anyone reading a seeded page: the BINDINGS and the COUNTS
 * are honest, the PROSE is not composed. Every surface that lists, filters,
 * paginates or navigates the KB is therefore under genuine load; the composer
 * itself is not exercised and has its own profile (`eval --profile docs`).
 *
 * Reads the org/team/entity ids back out of the database rather than deriving
 * them, so it can only ever attach pages to a corpus that is actually seeded.
 *
 *   node tools/gen-bank-corpus.mjs
 *   cargo run -p brainiac-server -- eval --fixtures fixtures/bank --profile retrieval
 *   node tools/seed-bank-docs.mjs        # <- this
 */

import { execFileSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

import { h01, hInt, pick, PRACTICES, TEAMS } from "./bank-org.mjs";

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const NOW = new Date("2026-07-15T00:00:00Z");
const daysBefore = (n) => new Date(NOW.getTime() - n * 86400000).toISOString();

const psql = (sql) =>
  execFileSync(
    "docker",
    ["compose", "exec", "-T", "postgres", "psql", "-U", "brainiac", "-d", "brainiac", "-At", "-F", "\t", "-c", sql],
    { cwd: ROOT, encoding: "utf8", maxBuffer: 64 * 1024 * 1024 },
  ).trim();

const sqlStr = (s) => `'${String(s).replace(/'/g, "''")}'`;

// ── read the seeded corpus back ──────────────────────────────────────────
const org = psql("select id from orgs limit 1");
if (!org) throw new Error("no org — seed fixtures/bank first");

const teams = psql("select id, name from teams order by name")
  .split("\n")
  .map((l) => {
    const [id, name] = l.split("\t");
    return { id, name };
  });

const canon = psql("select id, name, kind from canonical_entities order by name")
  .split("\n")
  .filter(Boolean)
  .map((l) => {
    const [id, name, kind] = l.split("\t");
    return { id, name, kind };
  });

if (canon.length === 0) throw new Error("no canonical entities — seed fixtures/bank first");

const teamByName = new Map(teams.map((t) => [t.name, t]));

/** Deterministic uuid5-ish from a key — stable across re-runs, so re-seeding
 *  updates rather than duplicating. */
function uuidFor(key) {
  const hx = (salt) =>
    Math.floor(h01(key, salt) * 0xffffffff)
      .toString(16)
      .padStart(8, "0");
  const raw = (hx(1) + hx(2) + hx(3) + hx(4)).slice(0, 32);
  return [raw.slice(0, 8), raw.slice(8, 12), "5" + raw.slice(13, 16), "a" + raw.slice(17, 20), raw.slice(20, 32)].join("-");
}

// ── the pages ────────────────────────────────────────────────────────────
const docs = [];

const owningTeam = (seed) => {
  const t = TEAMS[hInt(seed, TEAMS.length, 71)];
  return teamByName.get(t.name) ?? teams[0];
};

/** Domain namespace for the slug — 1,050 flat slugs is a phone book (§4). */
const nsFor = (team) => team.name;

// entity_page — one per canonical entity.
for (const c of canon) {
  const team = owningTeam(c.name);
  docs.push({
    key: `entity:${c.name}`,
    slug: `${nsFor(team)}/${c.name.replace(/\s+/g, "-")}`,
    title: c.name,
    kind: "entity_page",
    team,
    sections: [
      { heading: "What we know", mode: "composed", binding: { entities: [c.id], query: `${c.name} behaviour incidents decisions` } },
      { heading: "Ownership", mode: "pinned", pinned: `Owned by ${team.name}. Page #${team.name}-oncall before changing behaviour here.` },
    ],
  });
}

// topic_page — the cross-cutting reads: each practice × each team, plus the
// regulatory topics a bank actually maintains.
const REGS = [
  "PSD2 strong customer authentication",
  "GDPR data subject requests",
  "AML transaction monitoring thresholds",
  "sanctions screening obligations",
  "IFRS9 staging rules",
  "PCI-DSS cardholder data scope",
  "Basel III liquidity reporting",
  "MiFID II best execution",
  "open banking consent lifecycle",
  "operational resilience impact tolerances",
];

for (const t of TEAMS) {
  const team = teamByName.get(t.name);
  if (!team) continue;
  for (const p of PRACTICES) {
    docs.push({
      key: `topic:${t.name}:${p.id}`,
      slug: `${nsFor(team)}/${p.id}-policy`,
      title: `${p.practice} — ${t.name}`,
      kind: "topic_page",
      team,
      sections: [
        { heading: "The standard", mode: "composed", binding: { kinds: ["decision"], query: `${p.practice} ${t.name}` } },
        { heading: "Why", mode: "composed", binding: { query: `${p.practice} rationale incidents` } },
      ],
    });
  }
  for (const r of REGS) {
    docs.push({
      key: `reg:${t.name}:${r}`,
      slug: `${nsFor(team)}/${r.toLowerCase().replace(/[^a-z0-9]+/g, "-")}`,
      title: r,
      kind: "topic_page",
      team,
      sections: [
        { heading: "Obligation", mode: "pinned", pinned: `${r}. Reviewed by compliance; changes require a control owner sign-off.` },
        { heading: "How we meet it", mode: "composed", binding: { query: `${r} implementation ${t.name}` } },
      ],
    });
  }
}

/*
 * Post-mortems and decision records — the two kinds a bank's wiki has MOST of,
 * and the two this first missed.
 *
 * Three years of a twelve-team bank is hundreds of incidents, each with a write-up
 * that outlives the incident, plus an architecture decision log nobody deletes.
 * Leaving them out produced a 431-page KB, which is not a bank's KB — it is a
 * bank's service catalogue. They are `topic_page` because the schema has four
 * kinds and neither "post-mortem" nor "ADR" is one of them.
 */
const INCIDENTS = [
  "settlement batch ran twice",
  "duplicate refunds after a retry storm",
  "card authorisations declined at scheme timeout",
  "interest accrued twice on switched products",
  "sanctions screen returned stale list",
  "mobile app pinned to a revoked certificate",
  "ledger lag broke the balance cache",
  "onboarding queue stalled on CDD provider outage",
  "instant payments rejected during a rail upgrade",
  "warehouse backfill deadlocked the hourly ingest",
  "fraud model shadow-scored production traffic",
  "statement generation missed a value date",
];

for (const t of TEAMS) {
  const team = teamByName.get(t.name);
  if (!team) continue;
  // Volume follows the same power law as the memories: payments writes more
  // post-mortems than compliance, because payments has more incidents.
  const count = Math.max(4, Math.round(t.weight * 1.7));
  for (let i = 0; i < count; i++) {
    const inc = pick(INCIDENTS, `${t.id}:pm:${i}`, 91);
    const when = 30 + hInt(`${t.id}:pm:${i}`, 900, 92);
    const day = daysBefore(when).slice(0, 10);
    docs.push({
      key: `pm:${t.name}:${i}`,
      slug: `${nsFor(team)}/post-mortem-${day}-${inc.replace(/[^a-z0-9]+/gi, "-").toLowerCase()}`,
      title: `Post-mortem ${day} — ${inc}`,
      kind: "topic_page",
      team,
      sections: [
        { heading: "What happened", mode: "pinned", pinned: `On ${day}, ${inc}. Detected by alerting; customer impact was contained within the hour.` },
        { heading: "What we learned", mode: "composed", binding: { kinds: ["pitfall"], query: `${inc} ${t.name}` } },
        { heading: "Actions", mode: "composed", binding: { kinds: ["decision"], query: `${inc} remediation` } },
      ],
    });
  }
  const adrs = Math.max(3, Math.round(t.weight * 0.9));
  for (let i = 0; i < adrs; i++) {
    docs.push({
      key: `adr:${t.name}:${i}`,
      slug: `${nsFor(team)}/adr-${String(i + 1).padStart(3, "0")}`,
      title: `ADR-${String(i + 1).padStart(3, "0")} — ${t.name}`,
      kind: "topic_page",
      team,
      sections: [
        { heading: "Decision", mode: "composed", binding: { kinds: ["decision"], query: `${t.name} architecture decision` } },
        { heading: "Consequences", mode: "composed", binding: { query: `${t.name} decision consequences tradeoffs` } },
      ],
    });
  }
}

// runbook — per service entity, one page per operational procedure. A bank runs
// drills, so the DR and capacity procedures are pages too, not folklore.
for (const c of canon.filter((x) => x.kind === "service")) {
  const team = owningTeam(`${c.name}:rb`);
  for (const proc of ["incident", "failover", "rollback", "capacity", "dr-drill"]) {
    docs.push({
      key: `runbook:${c.name}:${proc}`,
      slug: `${nsFor(team)}/${c.name.replace(/\s+/g, "-")}-${proc}`,
      title: `${c.name} — ${proc} runbook`,
      kind: "runbook",
      team,
      sections: [
        { heading: "Symptoms", mode: "composed", binding: { entities: [c.id], kinds: ["pitfall"], query: `${c.name} failure symptoms` } },
        { heading: "Steps", mode: "composed", binding: { entities: [c.id], kinds: ["howto"], query: `${c.name} ${proc} procedure` } },
        { heading: "Escalation", mode: "pinned", pinned: `Escalate to ${team.name} on-call. Do not fail over without a second pair of eyes.` },
      ],
    });
  }
}

/*
 * Documented processes — the "how we do X here" pages.
 *
 * The last population, and the one that separates a bank's wiki from a startup's:
 * a regulated org writes the process down because an auditor will ask to see it,
 * so there is a page for the boring path as well as the interesting one.
 */
const PROCESSES = [
  "change approval",
  "incident severity classification",
  "on-call handover",
  "access request and review",
  "vendor due diligence",
  "data retention and deletion",
  "complaint handling",
  "customer dispute intake",
  "quarterly control attestation",
  "disaster recovery test",
];

for (const t of TEAMS) {
  const team = teamByName.get(t.name);
  if (!team) continue;
  for (const proc of PROCESSES) {
    docs.push({
      key: `proc:${t.name}:${proc}`,
      slug: `${nsFor(team)}/process-${proc.replace(/[^a-z0-9]+/gi, "-").toLowerCase()}`,
      title: `${proc} — ${t.name}`,
      kind: "topic_page",
      team,
      sections: [
        { heading: "The process", mode: "composed", binding: { kinds: ["howto"], query: `${proc} ${t.name}` } },
        { heading: "Evidence", mode: "pinned", pinned: `Every run of this process leaves an audit record. If it is not recorded, it did not happen.` },
      ],
    });
  }
}

// onboarding — one per team.
for (const t of teams) {
  docs.push({
    key: `onboarding:${t.name}`,
    slug: `${t.name}/start-here`,
    title: `${t.name} — start here`,
    kind: "onboarding",
    team: t,
    sections: [
      { heading: "What this team owns", mode: "composed", binding: { query: `${t.name} services ownership decisions` } },
      { heading: "First week", mode: "pinned", pinned: `Read the runbooks for the services above. Pair with your buddy on an incident before you take a shift.` },
    ],
  });
}

// ── the body ─────────────────────────────────────────────────────────────
// Generated prose, honestly labelled. A composed page would carry the claims of
// the memories bound to it; this carries a note saying it did not.
const bodyFor = (d) =>
  [
    `# ${d.title}`,
    "",
    `> Seeded page (docs/BANK-CORPUS.md §4). The section bindings below are real;`,
    `> the prose was written by the seeder, not composed from the memories.`,
    "",
    ...d.sections.flatMap((s) => [
      `## ${s.heading}`,
      "",
      s.mode === "pinned"
        ? s.pinned
        : `Composed from the canonical memories bound to this section${s.binding.entities ? " and anchored to this entity" : ""}. Regenerates whenever one of them is superseded.`,
      "",
    ]),
  ].join("\n");

// ── emit ─────────────────────────────────────────────────────────────────
const lines = ["begin;"];
let published = 0;
let dirty = 0;

for (const d of docs) {
  const id = uuidFor(d.key);
  const revId = uuidFor(`${d.key}:rev`);
  const r = h01(d.key, 81);
  // Not every page is published: a real KB has drafts in flight and pages the
  // composer has already marked dirty because a memory moved under them.
  const status = r < 0.12 ? "draft" : r < 0.16 ? "archived" : "published";
  const isDirty = status === "published" && h01(d.key, 82) < 0.09;
  const vis = h01(d.key, 83) < 0.35 ? "org" : "team";
  const created = daysBefore(30 + hInt(d.key, 600, 84));
  if (status === "published") published++;
  if (isDirty) dirty++;

  lines.push(
    `insert into documents (id, org_id, team_id, slug, title, visibility, doc_kind, status, dirty_at, created_at, updated_at)
     values ('${id}', '${org}', '${d.team.id}', ${sqlStr(d.slug)}, ${sqlStr(d.title)}, '${vis}', '${d.kind}', '${status}',
             ${isDirty ? `'${daysBefore(hInt(d.key, 20, 85))}'` : "null"}, '${created}', '${created}')
     on conflict (id) do update set title = excluded.title, status = excluded.status, dirty_at = excluded.dirty_at;`,
  );

  d.sections.forEach((s, i) => {
    const sid = uuidFor(`${d.key}:sec:${i}`);
    const binding = s.mode === "composed" ? sqlStr(JSON.stringify(s.binding)) : "null";
    const pinned = s.mode === "pinned" ? sqlStr(s.pinned) : "null";
    lines.push(
      `insert into document_sections (id, document_id, org_id, position, heading, mode, binding, pinned_content)
       values ('${sid}', '${id}', '${org}', ${i}, ${sqlStr(s.heading)}, '${s.mode}', ${binding}::jsonb, ${pinned})
       on conflict (id) do update set heading = excluded.heading;`,
    );
  });

  if (status !== "draft") {
    lines.push(
      `insert into document_revisions (id, document_id, org_id, content_md, composed_from, trigger, policy_decision, published_at, created_at)
       values ('${revId}', '${id}', '${org}', ${sqlStr(bodyFor(d))}, '[]'::jsonb, 'memory_change',
               '${h01(d.key, 86) < 0.2 ? "needs_review" : "auto_published"}',
               ${status === "published" ? `'${created}'` : "null"}, '${created}')
       on conflict (id) do update set content_md = excluded.content_md;`,
    );
    lines.push(`update documents set current_revision = '${revId}' where id = '${id}';`);
  }
}

lines.push("commit;");

const out = path.join(ROOT, "target", "seed-bank-docs.sql");
fs.mkdirSync(path.dirname(out), { recursive: true });
fs.writeFileSync(out, lines.join("\n"), "utf8");

execFileSync("docker", ["compose", "exec", "-T", "postgres", "psql", "-U", "brainiac", "-d", "brainiac", "-q", "-v", "ON_ERROR_STOP=1", "-f", "-"], {
  cwd: ROOT,
  input: lines.join("\n"),
  encoding: "utf8",
  maxBuffer: 64 * 1024 * 1024,
});

const total = psql("select count(*) from documents");
const secs = psql("select count(*) from document_sections");
console.log(`knowledge base seeded:
  documents    ${total}   (generated ${docs.length}: ${published} published, ${dirty} dirty, rest draft/archived)
  sections     ${secs}
  by kind      ${psql("select doc_kind || '=' || count(*) from documents group by doc_kind order by 1").split("\n").join("  ")}`);
