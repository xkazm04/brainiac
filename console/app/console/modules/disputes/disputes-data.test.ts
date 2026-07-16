import { describe, expect, it } from "vitest";

import {
  bandOf,
  claimCount,
  computeFacets,
  daysLeft,
  demoPage,
  matchesFilter,
  pos,
  severity,
  triageOrder,
  type DisputedMemory,
} from "./disputes-data";

// A minimal disputed memory; override only what a test cares about.
function mem(over: Partial<DisputedMemory>): DisputedMemory {
  return {
    memory_id: over.memory_id ?? "m",
    title: null,
    content: "c",
    kind: "fact",
    status: "canonical",
    team_id: null,
    team: null,
    confidence: null,
    valid_to: null,
    provenance: null,
    claims: { wrong: 0, outdated: 0 },
    reporters: 0,
    reports: [],
    oldest_claim_secs: 0,
    ...over,
  };
}

const inDays = (n: number) => new Date(Date.now() + n * 86_400_000).toISOString();

describe("severity", () => {
  it("weights wrong above outdated (defect over decay)", () => {
    expect(severity(mem({ claims: { wrong: 1, outdated: 0 } }))).toBe(2);
    expect(severity(mem({ claims: { wrong: 0, outdated: 1 } }))).toBe(1);
  });
});

describe("triageOrder — mirrors the server, NOT severity", () => {
  it("ranks by raw wrong count, so 2-wrong beats 1-wrong-5-outdated", () => {
    // This is the exact case the old severity sort got backwards:
    // A severity = 1*2+5 = 7, B severity = 2*2+0 = 4, yet the server puts B
    // first because it orders by `wrong DESC`.
    const a = mem({ memory_id: "A", claims: { wrong: 1, outdated: 5 } });
    const b = mem({ memory_id: "B", claims: { wrong: 2, outdated: 0 } });
    expect(triageOrder([a, b]).map((m) => m.memory_id)).toEqual(["B", "A"]);
  });

  it("breaks wrong ties on total claim count, then on age", () => {
    const few = mem({ memory_id: "few", claims: { wrong: 1, outdated: 0 } });
    const many = mem({ memory_id: "many", claims: { wrong: 1, outdated: 3 } });
    expect(triageOrder([few, many]).map((m) => m.memory_id)).toEqual(["many", "few"]);

    const young = mem({ memory_id: "young", claims: { wrong: 1, outdated: 1 }, oldest_claim_secs: 10 });
    const old = mem({ memory_id: "old", claims: { wrong: 1, outdated: 1 }, oldest_claim_secs: 999 });
    expect(triageOrder([young, old]).map((m) => m.memory_id)).toEqual(["old", "young"]);
  });

  it("does not mutate its input", () => {
    const rows = [
      mem({ memory_id: "A", claims: { wrong: 1, outdated: 0 } }),
      mem({ memory_id: "B", claims: { wrong: 2, outdated: 0 } }),
    ];
    triageOrder(rows);
    expect(rows.map((m) => m.memory_id)).toEqual(["A", "B"]);
  });
});

describe("daysLeft / bandOf", () => {
  it("returns null (never negative) when there is no TTL", () => {
    expect(daysLeft(mem({ valid_to: null }))).toBeNull();
    expect(bandOf(mem({ valid_to: null }))).toBe("none");
  });

  it("keeps 'no expiry' distinct from 'far future' — they are different facts", () => {
    expect(bandOf(mem({ valid_to: null }))).toBe("none");
    expect(bandOf(mem({ valid_to: inDays(400) }))).toBe("far");
  });

  it("bands the near term the way the server's BAND_SQL does", () => {
    expect(bandOf(mem({ valid_to: inDays(-3) }))).toBe("past");
    expect(bandOf(mem({ valid_to: inDays(10) }))).toBe("d30");
    expect(bandOf(mem({ valid_to: inDays(60) }))).toBe("d90");
    expect(bandOf(mem({ valid_to: inDays(150) }))).toBe("d180");
  });
});

describe("pos — the decay axis, honest about its endpoints", () => {
  it("returns null for a memory with no TTL (it is off the timeline)", () => {
    expect(pos(null)).toBeNull();
  });
  it("puts 'now' at the -30..180 origin fraction, not at zero", () => {
    // now (0d) sits 30/210 of the way along a [-30, 180] axis.
    expect(pos(0)).toBeCloseTo((30 / 210) * 100, 5);
  });
  it("clamps out-of-range days to the ends", () => {
    expect(pos(-999)).toBe(0);
    expect(pos(999)).toBe(100);
  });
  it("does NOT collapse a far-future TTL onto the null marker", () => {
    // The old bug: both 200d and no-TTL rendered at 100%. Now 200d is 100 and
    // null is null — distinct.
    expect(pos(200)).toBe(100);
    expect(pos(null)).not.toBe(pos(200));
  });
});

describe("claimCount", () => {
  it("sums both verdicts", () => {
    expect(claimCount(mem({ claims: { wrong: 2, outdated: 3 } }))).toBe(5);
  });
});

describe("matchesFilter", () => {
  const m = mem({
    kind: "decision",
    team_id: "t1",
    valid_to: inDays(10),
    claims: { wrong: 2, outdated: 1 },
    oldest_claim_secs: 5 * 3600,
  });

  it("passes an empty filter", () => {
    expect(matchesFilter(m, {})).toBe(true);
  });
  it("filters by kind, team and band", () => {
    expect(matchesFilter(m, { kind: "fact" })).toBe(false);
    expect(matchesFilter(m, { kind: "decision" })).toBe(true);
    expect(matchesFilter(m, { teamId: "other" })).toBe(false);
    expect(matchesFilter(m, { band: "d30" })).toBe(true);
    expect(matchesFilter(m, { band: "past" })).toBe(false);
  });
  it("filters by min claims and min age (hours)", () => {
    expect(matchesFilter(m, { minClaims: 3 })).toBe(true);
    expect(matchesFilter(m, { minClaims: 4 })).toBe(false);
    expect(matchesFilter(m, { minAgeHours: 4 })).toBe(true);
    expect(matchesFilter(m, { minAgeHours: 6 })).toBe(false);
  });
});

describe("computeFacets", () => {
  it("counts one per disputed memory and orders bands by decay, not count", () => {
    const rows = [
      mem({ kind: "fact", team_id: "t1", team: "payments", valid_to: inDays(-1) }),
      mem({ kind: "fact", team_id: "t1", team: "payments", valid_to: inDays(400) }),
      mem({ kind: "howto", team_id: null, team: null, valid_to: inDays(400) }),
    ];
    const f = computeFacets(rows);
    expect(f.kinds).toEqual([
      { value: "fact", label: "fact", count: 2 },
      { value: "howto", label: "howto", count: 1 },
    ]);
    // Teamless memories are labelled, not dropped.
    expect(f.teams.find((t) => t.value === "")?.label).toBe("org-wide");
    // Band order follows the axis: past before far, regardless of counts.
    expect(f.bands.map((b) => b.value)).toEqual(["past", "far"]);
  });
});

describe("demoPage — the offline mirror of the server contract", () => {
  const rows = Array.from({ length: 7 }, (_, i) =>
    mem({ memory_id: `m${i}`, claims: { wrong: i, outdated: 0 } }),
  );

  it("reports total over the full match set, not the page", () => {
    const p = demoPage(rows, {}, 0, 3);
    expect(p.total).toBe(7);
    expect(p.flagged).toHaveLength(3);
  });

  it("windows past row N and never re-sorts the page", () => {
    const p0 = demoPage(rows, {}, 0, 3);
    const p1 = demoPage(rows, {}, 1, 3);
    // Ordered wrong DESC: m6,m5,m4 | m3,m2,m1 | m0
    expect(p0.flagged.map((m) => m.memory_id)).toEqual(["m6", "m5", "m4"]);
    expect(p1.flagged.map((m) => m.memory_id)).toEqual(["m3", "m2", "m1"]);
  });

  it("clamps an out-of-range page to the last real one instead of going blank", () => {
    const p = demoPage(rows, {}, 99, 3);
    expect(p.page).toBe(2);
    expect(p.flagged).toEqual([rows[0]]); // m0, the last row
  });

  it("total reflects the filter, but facets reflect the full backlog", () => {
    const mixed = [
      mem({ memory_id: "a", kind: "fact", claims: { wrong: 1, outdated: 0 } }),
      mem({ memory_id: "b", kind: "howto", claims: { wrong: 1, outdated: 0 } }),
    ];
    const p = demoPage(mixed, { kind: "fact" }, 0, 25);
    expect(p.total).toBe(1);
    expect(p.flagged.map((m) => m.memory_id)).toEqual(["a"]);
    // The menu still offers howto, or the operator could never widen back out.
    expect(p.facets.kinds.map((k) => k.value).sort()).toEqual(["fact", "howto"]);
  });
});
