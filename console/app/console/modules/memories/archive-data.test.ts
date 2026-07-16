import { describe, expect, it } from "vitest";

import { timeBounds, validAt } from "./archive-data";
import type { MemoryRow } from "@/lib/types";

const row = (o: Partial<MemoryRow>): MemoryRow => ({
  id: "m",
  content: "c",
  kind: "fact",
  status: "canonical",
  visibility: "team",
  team: "payments",
  team_id: "t",
  valid_from: null,
  valid_to: null,
  superseded_by: null,
  created_at: null,
  confidence: 1,
  ...o,
});

describe("timeBounds", () => {
  /*
   * The bug this exists to prevent, seen on the live corpus 2026-07-15: every
   * memory carries a TTL, so `valid_to` runs a year or more into the future.
   * Maxing over it put the scrubber's default playhead past every memory's
   * expiry, and the archive opened on an empty corpus — "0 of 5 memories match"
   * — while truthfully reporting a total of 5.
   */
  it("does not let a future valid_to drag the ceiling past the last real record", () => {
    const rows = [
      row({ created_at: "2026-07-15T00:00:00Z", valid_from: "2026-07-15T00:00:00Z", valid_to: "2028-01-06T00:00:00Z" }),
      row({ created_at: "2026-07-14T00:00:00Z", valid_from: "2026-07-14T00:00:00Z", valid_to: "2027-01-11T00:00:00Z" }),
    ];
    const { max } = timeBounds(rows);
    expect(max.toISOString().slice(0, 10)).toBe("2026-07-15");
  });

  it("opens on a playhead where the corpus is actually visible", () => {
    const rows = [
      row({ created_at: "2026-07-15T00:00:00Z", valid_from: "2026-07-15T00:00:00Z", valid_to: "2028-01-06T00:00:00Z" }),
      row({ created_at: "2026-07-14T00:00:00Z", valid_from: "2026-07-14T00:00:00Z", valid_to: "2027-01-11T00:00:00Z" }),
    ];
    const { min, max } = timeBounds(rows);
    // The archive's own default: frac = 1 → the right edge, plus a day.
    const at = new Date(min.getTime() + (max.getTime() - min.getTime() + 86400000) * 1);
    expect(rows.filter((r) => validAt(r, at))).toHaveLength(2);
  });

  it("still lets valid_to widen the floor, for a backfilled window", () => {
    const rows = [
      row({ created_at: "2026-07-15T00:00:00Z", valid_from: null, valid_to: "2024-01-01T00:00:00Z" }),
      row({ created_at: "2026-07-15T00:00:00Z", valid_from: "2026-07-15T00:00:00Z" }),
    ];
    expect(timeBounds(rows).min.toISOString().slice(0, 10)).toBe("2024-01-01");
  });

  it("falls back to a sane span when nothing is dated", () => {
    const { min, max } = timeBounds([row({})]);
    expect(min.getTime()).toBeLessThan(max.getTime());
  });
});
