/*
 * The pitch page offers one code artifact as checkable proof: the RLS policy that
 * makes "the scan never sees a row you may not read" structural rather than
 * aspirational. That artifact was a hand-simplified copy of the policy in
 * migration 0001 — which migration 0002 had already dropped and replaced. It was
 * missing `deleted_at IS NULL` and the audited worker escape, so the one thing on
 * the page a skeptic could verify was not the thing that runs.
 *
 * A claim whose proof can silently drift is worse than no proof. This test binds
 * the artifact to the migrations: it finds the LAST `CREATE POLICY memories_read`
 * in migration order (the definition actually in force) and requires the pitch
 * artifact to match it, ignoring only whitespace.
 */

import { readFileSync, readdirSync } from "node:fs";
import path from "node:path";
import { describe, expect, it } from "vitest";
import { MECHANISMS } from "./pitch-data";

const MIGRATIONS = path.resolve(__dirname, "../../../migrations");

/** Collapse whitespace so indentation style is not the thing under test. */
const norm = (s: string) => s.replace(/\s+/g, " ").trim();

/** The definition in force = the last one in migration-number order. */
function policyInForce(name: string): string {
  const files = readdirSync(MIGRATIONS)
    .filter((f) => f.endsWith(".sql"))
    .sort();
  let found: string | null = null;
  for (const f of files) {
    const sql = readFileSync(path.join(MIGRATIONS, f), "utf8");
    const re = new RegExp(`CREATE POLICY ${name}[\\s\\S]*?;`, "g");
    const hits = sql.match(re);
    if (hits && hits.length) found = hits[hits.length - 1];
  }
  if (!found) throw new Error(`no CREATE POLICY ${name} in ${MIGRATIONS}`);
  return found;
}

describe("pitch artifacts are the code that runs", () => {
  it("the RLS mechanism quotes the memories_read policy currently in force", () => {
    const rls = MECHANISMS.find((m) => m.key === "rls");
    expect(rls, "the rls mechanism must exist").toBeDefined();
    expect(rls!.artifact, "the rls claim must carry its artifact").toBeTruthy();

    expect(norm(rls!.artifact!)).toBe(norm(policyInForce("memories_read")));
  });

  it("the quoted policy still contains the two guarantees the prose leans on", () => {
    // Belt and braces: if a future migration removes either, the test above
    // would happily agree with the new policy while the prose went stale.
    const inForce = policyInForce("memories_read");
    expect(inForce, "soft-deleted rows stay invisible").toContain("deleted_at IS NULL");
    expect(inForce, "the worker escape never reaches private memories").toContain(
      "visibility <> 'private'",
    );
  });
});
