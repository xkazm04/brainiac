/*
 * The triage state machine — pure, and deliberately the same shape the
 * backend enforces, so the UI never offers a button the database would
 * refuse:
 *
 *   proposed ──adopt──▶ adopted ──deprecate──▶ deprecated (terminal)
 *       └────reject───▶ rejected (terminal — and REMEMBERED: the mining
 *                        sweep dedups against rejections, LB3)
 *
 * plus the attribution rule (L-never #4): adopting an evidence-free rule
 * requires a decree — the maintainer signs for it by name. The backend
 * answers 409 if the UI ever disagrees; these functions exist so it never
 * has to.
 */

import type { StandardDetail } from "@/lib/types";

export type TriageAction = "adopt" | "reject" | "deprecate";

/** What the gate allows from a lifecycle. Deprecated and rejected are
 *  terminal: un-retiring a rule is re-proposing it, not editing history. */
export function allowedActions(lifecycle: string): TriageAction[] {
  switch (lifecycle) {
    case "proposed":
      return ["adopt", "reject"];
    case "adopted":
      return ["deprecate"];
    default:
      return [];
  }
}

export type AdoptPlan =
  | { kind: "plain" }
  | { kind: "needs_decree" }
  | { kind: "not_adoptable"; reason: string };

/**
 * How an adoption of this rule must proceed. A rule with provenance adopts
 * plainly; one without needs the maintainer's explicit signed decree; one
 * that is not proposed cannot adopt at all.
 */
export function adoptPlan(detail: Pick<StandardDetail, "lifecycle" | "provenance">): AdoptPlan {
  if (detail.lifecycle !== "proposed") {
    return {
      kind: "not_adoptable",
      reason:
        detail.lifecycle === "adopted"
          ? "already adopted"
          : detail.lifecycle === "rejected"
            ? "rejected — the sweep remembers; re-propose deliberately if the org changed its mind"
            : "retired — re-propose it instead of editing history",
    };
  }
  return detail.provenance.length > 0 ? { kind: "plain" } : { kind: "needs_decree" };
}
