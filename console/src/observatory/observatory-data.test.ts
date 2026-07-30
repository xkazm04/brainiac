/*
 * The demand view's job is to stay honest about three states that look alike in
 * a dashboard and mean entirely different things:
 *
 *   - nobody asked            → answeredRate null, render "—", never 0%
 *   - everybody asked, all missed → answeredRate 0, render 0%
 *   - the endpoint is unreachable → no demand at all, render no tile
 *
 * The third is handled by the module (it omits `demand`); the first two live
 * here, in the normalizer, because a `?? 0` anywhere on this path silently turns
 * "no data" into "total failure" on the org's headline number.
 */

import { describe, expect, it } from "vitest";

import { normalizeDemand } from "./observatory-data";
import type { AnalyticsValue } from "@/lib/types";

const base: AnalyticsValue = {
  window_days: 30,
  asks: 0,
  answered: 0,
  answered_rate: null,
  withheld_asks: 0,
  score_floor: 0,
  p50_latency_ms: null,
  p95_latency_ms: null,
  top_unanswered: [],
};

describe("demand view", () => {
  it("keeps 'nobody asked' distinct from 'everything missed'", () => {
    expect(normalizeDemand(base).answeredRate).toBeNull();

    const allMissed = normalizeDemand({ ...base, asks: 10, answered: 0, answered_rate: 0 });
    expect(allMissed.answeredRate).toBe(0);
    // The distinction only survives if null never collapses to 0 on the way in.
    expect(allMissed.answeredRate).not.toBeNull();
  });

  it("carries the score floor through so the tile can say what 'answered' means", () => {
    expect(normalizeDemand(base).scoreFloor).toBe(0);
    expect(normalizeDemand({ ...base, score_floor: 0.35 }).scoreFloor).toBe(0.35);
  });

  it("preserves a null best score — 'nothing returned' is not 'scored zero'", () => {
    const d = normalizeDemand({
      ...base,
      asks: 3,
      top_unanswered: [
        {
          query: "deploy rollback",
          asks: 3,
          projects: 2,
          best_score: null,
          last_asked: "2026-07-29T09:12:00Z",
        },
      ],
    });
    expect(d.topUnanswered[0].bestScore).toBeNull();
  });

  it("tolerates a payload with no miss list", () => {
    const d = normalizeDemand({ ...base, top_unanswered: undefined as never });
    expect(d.topUnanswered).toEqual([]);
  });
});
