import { describe, expect, it } from "vitest";

import { INTENT, OUTCOME, asMode, asOutcome } from "./edit-copy";

describe("outcome narrowing", () => {
  it("recognizes the two outcomes the API returns", () => {
    expect(asOutcome("saved")).toBe("saved");
    expect(asOutcome("captured")).toBe("captured");
  });

  it("degrades an unknown outcome to `captured` — the reading that claims less", () => {
    expect(asOutcome("")).toBe("captured");
    expect(asOutcome("SAVED")).toBe("captured");
    expect(asOutcome("something-new")).toBe("captured");
  });
});

describe("mode narrowing", () => {
  it("only the exact string `pinned` gets the human-owns-it treatment", () => {
    expect(asMode("pinned")).toBe("pinned");
    expect(asMode("composed")).toBe("composed");
    expect(asMode("Pinned")).toBe("composed");
    expect(asMode("")).toBe("composed");
  });
});

describe("a captured edit never claims to have been saved", () => {
  const captured = OUTCOME[asOutcome("captured")];

  it("does not use the word `saved` anywhere in its copy", () => {
    const words = `${captured.status} ${captured.next}`.toLowerCase();
    expect(words).not.toMatch(/\bsaved\b/);
    expect(words).not.toMatch(/\bsave\b/);
  });

  it("says what actually happens: proposed knowledge, review, recompose", () => {
    const words = `${captured.status} ${captured.next}`.toLowerCase();
    expect(words).toContain("proposed knowledge");
    expect(words).toContain("review");
    expect(words).toContain("recompose");
    // and is explicit that the page was NOT changed
    expect(words).toContain("not written into the page");
  });

  it("is toned as queued work, not as a finished action", () => {
    expect(captured.tone).toBe("queued");
    expect(OUTCOME.saved.tone).toBe("done");
  });
});

describe("a pinned edit is the one that may say saved", () => {
  it("says saved, and promises regeneration will not overwrite it", () => {
    expect(OUTCOME.saved.status).toMatch(/saved/i);
    expect(OUTCOME.saved.next.toLowerCase()).toContain("regenerates over it");
  });
});

describe("the warning arrives before the keystrokes", () => {
  it("a composed section warns that the edit becomes a proposal, not the page", () => {
    const w = INTENT.composed.warning.toLowerCase();
    expect(w).toContain("proposed knowledge");
    expect(w).toContain("review");
    expect(w).not.toMatch(/\bsave\b/);
    // The CTA must not promise a save either.
    expect(INTENT.composed.cta.toLowerCase()).not.toMatch(/sav/);
    expect(INTENT.composed.cta_pending.toLowerCase()).not.toMatch(/sav/);
  });

  it("a pinned section promises the prose is the human's and survives regeneration", () => {
    const w = INTENT.pinned.warning.toLowerCase();
    expect(w).toContain("regeneration never touches it");
    expect(INTENT.pinned.cta.toLowerCase()).toContain("save");
  });
});
