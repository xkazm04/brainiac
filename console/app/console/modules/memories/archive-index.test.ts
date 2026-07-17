import { describe, expect, it } from "vitest";

import { memoryLabel, rowView, spanLabel } from "./archive-index";
import type { MemoryRow } from "@/lib/types";

const row = (o: Partial<MemoryRow>): MemoryRow => ({
  id: "m",
  title: null,
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

describe("memoryLabel", () => {
  /*
   * `memories.title` is nullable FOREVER — everything captured before migration
   * 0023 has none, and the extractor does not emit one — so a reader that
   * assumes it will render a column of blanks over the live corpus.
   */
  it("falls back to the content when there is no title", () => {
    const { label, titled } = memoryLabel(row({ title: null, content: "psp timeout is 30s" }));
    expect(label).toBe("psp timeout is 30s");
    expect(titled).toBe(false);
  });

  it("treats an empty or whitespace title as no title", () => {
    expect(memoryLabel(row({ title: "   ", content: "the claim" }))).toEqual({
      label: "the claim",
      titled: false,
    });
  });

  it("reports a real title as one, untouched", () => {
    expect(memoryLabel(row({ title: "psp timeout", content: "long content" }))).toEqual({
      label: "psp timeout",
      titled: true,
    });
  });

  it("clips a long fallback so one row cannot own the column", () => {
    const { label } = memoryLabel(row({ content: "x".repeat(400) }));
    expect(label.length).toBeLessThanOrEqual(96);
    expect(label.endsWith("…")).toBe(true);
  });
});

describe("spanLabel", () => {
  it("renders a closed window as from → to", () => {
    expect(
      spanLabel(row({ valid_from: "2026-01-30T00:00:00Z", valid_to: "2028-01-06T00:00:00Z" })),
    ).toBe("2026-01-30 → 2028-01-06");
  });

  it("renders an open end as → now", () => {
    expect(spanLabel(row({ valid_from: "2026-01-30T00:00:00Z", valid_to: null }))).toBe(
      "2026-01-30 → now",
    );
  });

  it("renders an open start as an em dash", () => {
    expect(spanLabel(row({ valid_from: null, valid_to: null }))).toBe("— → now");
  });
});

describe("rowView", () => {
  it("carries the claim under a title, and never echoes it under a fallback", () => {
    const titled = rowView(row({ title: "psp timeout", content: "psp-gateway timeout is 30s" }));
    const bare = rowView(row({ title: null, content: "psp-gateway timeout is 30s" }));
    expect(titled.sub).toBe("psp-gateway timeout is 30s");
    expect(titled.titled).toBe(true);
    expect(bare.sub).toBe("");
    expect(bare.titled).toBe(false);
    expect(bare.label).toBe("psp-gateway timeout is 30s");
  });

  it("collapses whitespace in the content it renders", () => {
    expect(rowView(row({ title: null, content: "a   b\n\tc" })).label).toBe("a b c");
  });
});
