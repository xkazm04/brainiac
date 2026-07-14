import { describe, expect, it } from "vitest";

import { age, propagationVerdict } from "./age";

describe("age", () => {
  it("renders nothing rather than a zero", () => {
    expect(age(0)).toBe("—");
    expect(age(-5)).toBe("—");
  });

  it("reads in minutes under an hour", () => {
    expect(age(59)).toBe("0m");
    expect(age(90)).toBe("1m");
    expect(age(3_599)).toBe("59m");
  });

  it("reads in hours and minutes under a day", () => {
    expect(age(3_600)).toBe("1h 0m");
    expect(age(69_000)).toBe("19h 10m");
    expect(age(86_399)).toBe("23h 59m");
  });

  it("reads in days and hours beyond that", () => {
    expect(age(86_400)).toBe("1d 0h");
    expect(age(232_000)).toBe("2d 16h");
  });
});

describe("propagationVerdict — does 'automatically' mean minutes or never?", () => {
  it("kept: no page is behind the corpus", () => {
    expect(propagationVerdict(0, 0)).toEqual({
      verdict: "every page is current with the corpus",
      tone: "good",
    });
    // dirty count is the authority: a stale age with zero dirty pages is still healthy.
    expect(propagationVerdict(0, 900_000).tone).toBe("good");
  });

  it("healthy: dirty, but recomposing within the hour", () => {
    const p = propagationVerdict(3, 1_500);
    expect(p.tone).toBe("good");
    expect(p.verdict).toContain("25m");
  });

  it("watch: hours behind", () => {
    const p = propagationVerdict(2, 69_000);
    expect(p.tone).toBe("watch");
    expect(p.verdict).toContain("lagging");
  });

  it("bad: a page has been behind for more than a day — this is 'never'", () => {
    const p = propagationVerdict(1, 400_000);
    expect(p.tone).toBe("bad");
    expect(p.verdict).toContain("stalled");
    expect(p.verdict).toContain("4d 15h");
  });
});
