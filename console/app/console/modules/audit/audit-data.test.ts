import { describe, expect, it } from "vitest";

import {
  actorLabel,
  ageLabel,
  kindLabel,
  outcomeTone,
  parseKind,
  parseOffset,
} from "./audit-data";

describe("parseKind", () => {
  it("accepts a known kind", () => {
    expect(parseKind("contradiction_resolution")).toBe("contradiction_resolution");
  });

  it("treats 'all' as no filter", () => {
    expect(parseKind("all")).toBeUndefined();
  });

  it("falls back to no filter for junk rather than forwarding it to the server", () => {
    expect(parseKind("drop table memories")).toBeUndefined();
    expect(parseKind(undefined)).toBeUndefined();
  });

  it("takes the first value of a repeated query param", () => {
    expect(parseKind(["promotion_review", "feedback_resolution"])).toBe("promotion_review");
  });
});

describe("parseOffset", () => {
  it("parses a positive integer", () => {
    expect(parseOffset("50")).toBe(50);
  });

  it("floors a fractional value", () => {
    expect(parseOffset("12.7")).toBe(12);
  });

  it("rejects negative, zero, and junk back to 0", () => {
    expect(parseOffset("-5")).toBe(0);
    expect(parseOffset("0")).toBe(0);
    expect(parseOffset("not-a-number")).toBe(0);
    expect(parseOffset(undefined)).toBe(0);
  });
});

describe("kindLabel", () => {
  it("labels the three governance actions", () => {
    expect(kindLabel("promotion_review")).toBe("promotion");
    expect(kindLabel("contradiction_resolution")).toBe("contradiction");
    expect(kindLabel("feedback_resolution")).toBe("dispute");
  });

  it("falls back to the raw kind for anything unrecognized", () => {
    expect(kindLabel("something_new")).toBe("something_new");
  });
});

describe("outcomeTone", () => {
  it("reads approvals, reverifications and supersessions as good", () => {
    expect(outcomeTone("approved")).toBe("good");
    expect(outcomeTone("auto_approved")).toBe("good");
    expect(outcomeTone("reverified")).toBe("good");
    expect(outcomeTone("resolved_supersede")).toBe("good");
  });

  it("reads rejections and deprecations as bad", () => {
    expect(outcomeTone("rejected")).toBe("bad");
    expect(outcomeTone("deprecated")).toBe("bad");
  });

  it("reads coexist/dismissed as neutral rather than guessing", () => {
    expect(outcomeTone("resolved_coexist")).toBe("neutral");
    expect(outcomeTone("dismissed")).toBe("neutral");
  });
});

describe("actorLabel", () => {
  it("never renders a null actor as a decision made by no one in particular — names the policy", () => {
    expect(actorLabel(null)).toBe("policy (auto)");
  });

  it("labels a human decision by the org token, not a person — the shared-passcode caveat", () => {
    const label = actorLabel("f00dbabe-0000-0000-0000-000000000001");
    expect(label).toContain("token");
    expect(label).not.toContain("Petra");
  });
});

describe("ageLabel", () => {
  it("renders sub-minute ages in seconds", () => {
    expect(ageLabel(new Date(Date.now() - 30_000).toISOString())).toMatch(/^\d+s ago$/);
  });

  it("renders hour-scale ages in hours", () => {
    expect(ageLabel(new Date(Date.now() - 3 * 3_600_000).toISOString())).toMatch(/^\d+h ago$/);
  });

  it("renders day-scale ages in days", () => {
    expect(ageLabel(new Date(Date.now() - 4 * 86_400_000).toISOString())).toMatch(/^\d+d ago$/);
  });
});
