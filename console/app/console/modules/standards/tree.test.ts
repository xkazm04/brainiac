import { describe, expect, it } from "vitest";

import type { LibraryStandard } from "@/lib/types";

import { buildStandardsTree, dormantOf, proposedOf, rankByPressure } from "./tree";
import { adoptPlan, allowedActions, editablePatch, isEditable } from "./triage";

const rule = (over: Partial<LibraryStandard>): LibraryStandard => ({
  id: over.slug ?? "id",
  origin: "human",
  stack: "rust",
  category: "errors",
  slug: "some-rule",
  statement: "One sentence.",
  rationale: null,
  detail_md: null,
  enforcement: "recommended",
  lifecycle: "adopted",
  adopted_at: null,
  decreed: false,
  uses: 0,
  recent_uses: 0,
  last_used_at: null,
  dormant: false,
  ...over,
});

describe("the standards tree", () => {
  it("groups flat rules into stack ▸ category ▸ rule with counts", () => {
    const tree = buildStandardsTree([
      rule({ stack: "typescript", category: "imports", slug: "no-barrels" }),
      rule({ stack: "rust", category: "errors", slug: "no-unwrap" }),
      rule({ stack: "rust", category: "errors", slug: "typed-errors", lifecycle: "proposed" }),
      rule({ stack: "rust", category: "testing", slug: "pg-tests-serial" }),
    ]);
    // stacks alphabetical; predictable from the name, like the nav
    expect(tree.map((s) => s.stack)).toEqual(["rust", "typescript"]);
    const rust = tree[0];
    expect(rust.count).toBe(3);
    expect(rust.proposed).toBe(1);
    expect(rust.categories.map((c) => c.category)).toEqual(["errors", "testing"]);
  });

  it("floats the triage queue to the top of every branch; rejections sink", () => {
    const tree = buildStandardsTree([
      rule({ slug: "a-adopted", lifecycle: "adopted" }),
      rule({ slug: "b-rejected", lifecycle: "rejected" }),
      rule({ slug: "z-proposed", lifecycle: "proposed" }),
      rule({ slug: "m-deprecated", lifecycle: "deprecated" }),
    ]);
    expect(tree[0].categories[0].rules.map((r) => r.slug)).toEqual([
      "z-proposed", // work first, despite the alphabet
      "a-adopted",
      "m-deprecated",
      "b-rejected", // the dedup memory — visible, never in the way
    ]);
  });

  it("collects the proposed queue across stacks", () => {
    const q = proposedOf([
      rule({ stack: "rust", slug: "b", lifecycle: "proposed" }),
      rule({ stack: "typescript", slug: "a", lifecycle: "proposed" }),
      rule({ stack: "rust", slug: "c", lifecycle: "adopted" }),
    ]);
    expect(q.map((r) => r.slug)).toEqual(["a", "b"]);
  });

  it("handles an empty library without inventing structure", () => {
    expect(buildStandardsTree([])).toEqual([]);
    expect(proposedOf([])).toEqual([]);
  });
});

describe("ordering by the pressure a rule is under", () => {
  it("ranks by pulse, and puts the unratified rule the org already follows on top", () => {
    // Slugs and categories are deliberately in the OPPOSITE order to the
    // pulse: a ranking that quietly fell back to the old category/slug shelf
    // order would produce a-b-c-d and has to fail here.
    const ranked = rankByPressure([
      rule({ slug: "a-quiet-adopted", category: "a", uses: 0, lifecycle: "adopted", dormant: true }),
      rule({ slug: "b-busy-adopted", category: "b", uses: 40, lifecycle: "adopted" }),
      rule({ slug: "c-already-followed", category: "c", uses: 61, lifecycle: "proposed" }),
      rule({ slug: "d-fresh-candidate", category: "d", uses: 0, lifecycle: "proposed" }),
    ]);
    expect(ranked.map((r) => r.slug)).toEqual([
      // The org is already doing this and nobody has ratified it: the
      // cheapest adoption on the board, and invisible in the alphabet.
      "c-already-followed",
      "b-busy-adopted",
      // Zero pulse: the proposal still needs a decision, the adopted rule
      // has gone quiet. Both belong at the bottom, in that order.
      "d-fresh-candidate",
      "a-quiet-adopted",
    ]);
  });

  it("breaks a pulse tie toward the rule that still needs a decision", () => {
    const ranked = rankByPressure([
      rule({ slug: "a-adopted", uses: 5, lifecycle: "adopted" }),
      rule({ slug: "z-proposed", uses: 5, lifecycle: "proposed" }),
    ]);
    expect(ranked[0].slug).toBe("z-proposed");
  });

  it("is a total order — equal rules never reshuffle between renders", () => {
    const rules = [rule({ slug: "b" }), rule({ slug: "a" }), rule({ slug: "c" })];
    expect(rankByPressure(rules).map((r) => r.slug)).toEqual(["a", "b", "c"]);
    expect(rankByPressure([...rules].reverse()).map((r) => r.slug)).toEqual(["a", "b", "c"]);
  });

  it("collects the retirement shortlist without inventing dormancy", () => {
    // Dormancy is the backend's verdict (core's `rule_is_dormant`), never
    // re-derived here from a use count the console cannot date correctly.
    expect(
      dormantOf([
        rule({ slug: "dead", dormant: true }),
        rule({ slug: "unused-but-new", uses: 0, dormant: false }),
      ]).map((r) => r.slug),
    ).toEqual(["dead"]);
  });
});

describe("the triage state machine", () => {
  it("mirrors the backend's lifecycle transitions exactly", () => {
    // proposed → adopt | reject; adopted → deprecate; deprecated and rejected
    // are terminal. The UI must never offer a button the database would refuse.
    expect(allowedActions("proposed")).toEqual(["adopt", "reject"]);
    expect(allowedActions("adopted")).toEqual(["deprecate"]);
    expect(allowedActions("deprecated")).toEqual([]);
    expect(allowedActions("rejected")).toEqual([]);
    expect(allowedActions("nonsense")).toEqual([]);
  });

  it("requires a decree exactly when the rule has no evidence", () => {
    const prov = { kind: "divergence", ref_id: "d1" };
    expect(adoptPlan({ lifecycle: "proposed", provenance: [prov] })).toEqual({ kind: "plain" });
    expect(adoptPlan({ lifecycle: "proposed", provenance: [] })).toEqual({
      kind: "needs_decree",
    });
  });

  it("lets triage edit a live rule and refuses a terminal one", () => {
    // Narrowing is how a mined `general` candidate reaches the stack whose
    // page should carry it; a retired rule is history, not a draft.
    expect(isEditable("proposed")).toBe(true);
    expect(isEditable("adopted")).toBe(true);
    expect(isEditable("deprecated")).toBe(false);
    expect(isEditable("rejected")).toBe(false);
  });

  it("sends only what the maintainer actually changed", () => {
    const filed = { stack: "general", category: "practice", statement: "Retry with backoff." };
    // The narrowing case: the wording is untouched, so no revision is minted.
    expect(editablePatch(filed, { ...filed, stack: "rust", category: "resilience" })).toEqual({
      stack: "rust",
      category: "resilience",
    });
    // A re-word travels alone.
    expect(
      editablePatch(filed, { ...filed, statement: "Retry with full jitter, capped at 30s." }),
    ).toEqual({ statement: "Retry with full jitter, capped at 30s." });
    // Re-submitting the same rule is not a change. It matters: the backend
    // mints a revision for a wording change, so a no-op save would inflate the
    // rule's history with revisions that revised nothing.
    expect(editablePatch(filed, { ...filed })).toBeNull();
    expect(editablePatch(filed, { ...filed, statement: "  Retry with backoff.  " })).toBeNull();
    // An emptied field is a slip of the keyboard, not an instruction to blank
    // the org's rule.
    expect(editablePatch(filed, { ...filed, stack: "   " })).toBeNull();
  });

  it("refuses to plan an adoption for a rule past the gate", () => {
    expect(adoptPlan({ lifecycle: "adopted", provenance: [] }).kind).toBe("not_adoptable");
    const retired = adoptPlan({ lifecycle: "deprecated", provenance: [] });
    expect(retired.kind).toBe("not_adoptable");
    // Retirement is one-way; the way back is a new proposal, and the copy
    // must say so rather than dangling an impossible action.
    if (retired.kind === "not_adoptable") {
      expect(retired.reason).toMatch(/re-propose/);
    }
  });
});
