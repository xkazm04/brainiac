/*
 * The standards board at organizational scale.
 *
 * A caveat this file must carry, because it cuts against the module's own
 * design: the real detector is deliberately conservative, and the shipped demo
 * board says so ("an empty list is a healthy sign, not a broken scan"). Sixty
 * divergences is NOT the expected steady state of a healthy org.
 *
 * It is the state that matters anyway — a first sweep over an org that has never
 * standardized anything, or a quarter's accumulation nobody triaged. That is
 * exactly when a platform lead needs the board most, and exactly when a flat
 * list of tall cards stops being usable. Prototype against the bad day.
 */

import type { PracticeDivergence, PracticeDivergences } from "@/lib/types";
import { hash01, pick } from "@/lib/seeded";

const TEAMS = ["payments", "platform", "data", "growth", "risk"] as const;

/** [practice, what team A does, what team B does, the standard to propose] */
const PRACTICES: [string, string, string, string][] = [
  ["service retry policy", "retry cap of 2 seconds, 3 attempts, for all internal calls", "retry cap of 30 seconds with jitter for the refund worker", "Adopt a 2 s retry cap, 3 attempts, for every internal service call."],
  ["idempotency key TTL", "keys retained 7 days to cover settlement reconciliation", "keys expire after 1 hour to bound Redis memory", "Standardize idempotency-key retention at 24 hours, with a documented exception path."],
  ["deploy approval", "two maintainer approvals via an OPA override PR", "single on-call approval in the deploy CLI", "Require two approvals for production, one for staging, both through OPA."],
  ["structured logging", "JSON lines with a correlation id on every write", "plain text with a request prefix", "Emit JSON lines with a correlation id on every service boundary."],
  ["feature freshness SLO", "p99 of 50ms measured at the serving edge", "p99 of 100ms measured in the client", "Measure p99 at the serving edge and hold it at 50 ms."],
  ["secret rotation", "quarterly rotation through the vault CLI", "annual rotation, manual", "Rotate quarterly via the vault CLI, alerting 14 days before expiry."],
  ["schema migration", "expand-contract over two releases", "in-place ALTER during a maintenance window", "Use expand-contract across two releases; no maintenance windows."],
  ["error budget policy", "freeze features at 100% budget burn", "no formal policy", "Freeze feature work at 100% budget burn until the budget recovers."],
  ["queue consumer scaling", "fixed replica count sized to peak", "KEDA autoscaling on lag", "Autoscale on consumer lag with a floor sized to median load."],
  ["PII handling in logs", "hashed at the call site", "redacted by a log processor downstream", "Hash PII at the call site; never rely on downstream redaction."],
  ["cache invalidation", "TTL-only, 5 minutes", "explicit invalidation on write", "Invalidate explicitly on write, with a 5-minute TTL as a backstop."],
  ["dependency pinning", "exact versions in a lockfile", "caret ranges resolved at build", "Pin exact versions in a committed lockfile; renovate weekly."],
];

const QUALIFIER = [
  "so the same failure is handled two ways depending on who owns the code",
  "so a request that is safe for one service is a duplicate for the other",
  "so the on-call cannot reason about the blast radius without asking",
  "so an incident review has to reconstruct which rule was in force",
  "so a migration between the two teams silently changes behaviour",
];

export const SCALE_DIVERGENCES = 60;

export function makeLargeDivergences(n: number = SCALE_DIVERGENCES): PracticeDivergences {
  const divergences: PracticeDivergence[] = Array.from({ length: n }, (_, i) => {
    const seed = `dv-${i}`;
    const [practice, aWay, bWay, standard] = PRACTICES[i % PRACTICES.length];
    // Impact is heavily skewed low: a board that cries "high" at everything
    // teaches a lead to ignore it, which is the failure the module warns about.
    const r = hash01(seed, 1);
    const impact = r > 0.86 ? "high" : r > 0.55 ? "medium" : "low";
    const ta = pick(TEAMS, seed, 2);
    let tb = pick(TEAMS, seed, 3);
    if (tb === ta) tb = TEAMS[(TEAMS.indexOf(ta) + 1) % TEAMS.length];
    // A third of them pull in a third team — the ones a two-column card cannot
    // show, and a real reason the current layout breaks down.
    const third = hash01(seed, 4) > 0.66;
    let tc = pick(TEAMS, seed, 5);
    if (tc === ta || tc === tb) tc = TEAMS[(TEAMS.indexOf(tc) + 2) % TEAMS.length];
    const suffix = i >= PRACTICES.length ? ` (${["ingest", "serving", "batch", "edge", "mobile"][Math.floor(i / PRACTICES.length) % 5]})` : "";
    return {
      practice: `${practice}${suffix}`,
      summary: `${ta} and ${tb} each solved this their own way, ${pick(QUALIFIER, seed, 6)}.`,
      recommended_standard: standard,
      impact,
      approaches: [
        { team: ta, approach: aWay },
        { team: tb, approach: bWay },
        ...(third ? [{ team: tc, approach: `a third variant, inherited from the ${tc} migration` }] : []),
      ],
      model_ref: "qwen:qwen-max",
      detected_at: `2026-0${1 + (i % 7)}-${String(1 + (i % 27)).padStart(2, "0")}T00:00:00Z`,
    };
  });
  // The server orders by impact; the board assumes it ("highest impact first").
  const rank = { high: 0, medium: 1, low: 2 } as const;
  divergences.sort((a, b) => rank[a.impact as keyof typeof rank] - rank[b.impact as keyof typeof rank]);
  return { divergences };
}
