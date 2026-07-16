import { describe, expect, it } from "vitest";

import {
  buildScope,
  extractTerms,
  indexRows,
  liveAt,
  memoryLabel,
  sortByValid,
  suggest,
} from "./archive-index";
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

describe("indexRows", () => {
  it("carries the claim under a title, and never echoes it under a fallback", () => {
    const [titled, bare] = indexRows([
      row({ id: "a", title: "psp timeout", content: "psp-gateway timeout is 30s" }),
      row({ id: "b", title: null, content: "psp-gateway timeout is 30s" }),
    ]);
    expect(titled.sub).toBe("psp-gateway timeout is 30s");
    expect(bare.sub).toBe("");
  });

  it("searches title, content, team and kind as one haystack", () => {
    const [ix] = indexRows([row({ title: "Ledger cutover", content: "ArgoCD only", kind: "decision" })]);
    for (const q of ["ledger", "argocd", "payments", "decision"]) {
      expect(ix.hay.includes(q)).toBe(true);
    }
  });

  it("parses open bounds to infinities so liveAt is two number compares", () => {
    const [ix] = indexRows([row({ valid_from: null, valid_to: null })]);
    expect(ix.from).toBe(Number.NEGATIVE_INFINITY);
    expect(ix.to).toBe(Number.POSITIVE_INFINITY);
    expect(liveAt(ix, 0)).toBe(true);
  });

  it("matches validAt's half-open window: from <= at < to", () => {
    const [ix] = indexRows([
      row({ valid_from: "2026-01-01T00:00:00Z", valid_to: "2026-06-01T00:00:00Z" }),
    ]);
    expect(liveAt(ix, Date.parse("2026-01-01T00:00:00Z"))).toBe(true);
    expect(liveAt(ix, Date.parse("2026-06-01T00:00:00Z"))).toBe(false);
    expect(liveAt(ix, Date.parse("2025-12-31T23:59:59Z"))).toBe(false);
  });
});

describe("extractTerms", () => {
  it("lifts service, product and acronym names out of a claim", () => {
    const terms = extractTerms(
      "the psp-gateway retry burns PSP quota; ArgoCD deploys it and the backfill DAG locks",
    );
    expect(terms).toContain("psp-gateway");
    expect(terms).toContain("psp");
    expect(terms).toContain("argocd");
    expect(terms).toContain("dag");
  });

  it("does not offer every capitalised sentence opener as an entity", () => {
    expect(extractTerms("Retries are expensive. Deploys are not.")).toEqual([]);
  });
});

describe("sortByValid", () => {
  const rows = indexRows([
    row({ id: "b", valid_from: "2026-02-01T00:00:00Z" }),
    row({ id: "a", valid_from: "2026-01-01T00:00:00Z" }),
    row({ id: "c", valid_from: "2026-03-01T00:00:00Z" }),
  ]);
  const ids = (l: ReturnType<typeof indexRows>) => l.map((ix) => ix.row.id);

  it("is off by default — the server's order survives untouched", () => {
    expect(sortByValid(rows, "off")).toBe(rows);
  });

  it("orders both ways", () => {
    expect(ids(sortByValid(rows, "asc"))).toEqual(["a", "b", "c"]);
    expect(ids(sortByValid(rows, "desc"))).toEqual(["c", "b", "a"]);
  });

  it("is stable: ties keep the order they arrived in", () => {
    const tied = indexRows([
      row({ id: "x", valid_from: "2026-01-01T00:00:00Z" }),
      row({ id: "y", valid_from: "2026-01-01T00:00:00Z" }),
      row({ id: "z", valid_from: "2026-01-01T00:00:00Z" }),
    ]);
    expect(ids(sortByValid(tied, "asc"))).toEqual(["x", "y", "z"]);
    expect(ids(sortByValid(tied, "desc"))).toEqual(["x", "y", "z"]);
  });

  it("does not mutate the list it was handed", () => {
    const before = ids(rows);
    sortByValid(rows, "asc");
    expect(ids(rows)).toEqual(before);
  });
});

describe("suggest", () => {
  const corpus = indexRows([
    row({ id: "1", title: "psp-gateway timeout", content: "psp-gateway client timeout is 30s", team: "payments", kind: "decision" }),
    row({ id: "2", title: "psp-gateway retries", content: "retrying psp-gateway burns quota", team: "payments", kind: "pitfall" }),
    row({ id: "3", title: "ledger cutover", content: "ledger-service owns settlement", team: "platform", kind: "decision" }),
  ]);
  const scope = buildScope(corpus);

  it("says nothing until there is something to say", () => {
    expect(suggest(scope, "")).toEqual([]);
    expect(suggest(scope, "p")).toEqual([]);
  });

  it("offers a facet, and states what picking it will do", () => {
    const s = suggest(scope, "payment").find((x) => x.kind === "team");
    expect(s).toMatchObject({ value: "payments", action: "filter team", count: 2 });
  });

  it("counts a facet within the scope it was handed, not the whole archive", () => {
    // One team's rows only — a suggestion promising 2 here would be a lie.
    const narrowed = buildScope(corpus.filter((ix) => ix.row.team === "platform"));
    expect(suggest(narrowed, "decision").find((x) => x.kind === "kind")?.count).toBe(1);
  });

  it("offers an entity name lifted from the content, with its reach", () => {
    const s = suggest(scope, "gatew").find((x) => x.kind === "term");
    expect(s).toMatchObject({ value: "psp-gateway", action: "search for", count: 2 });
  });

  it("routes to a memory by its label", () => {
    const s = suggest(scope, "cutover").find((x) => x.kind === "memory");
    expect(s).toMatchObject({ value: "3", action: "open record", detail: "platform · decision" });
  });

  it("finds a titleless memory by its content, since the label is the content", () => {
    const bare = buildScope(indexRows([row({ id: "9", title: null, content: "browser autofill duplicates tokenization" })]));
    expect(suggest(bare, "autofill").find((x) => x.kind === "memory")?.value).toBe("9");
  });

  it("stays within its limit, and prefers the decisive options", () => {
    const out = suggest(scope, "psp", 3);
    expect(out).toHaveLength(3);
    expect(out[0].kind).not.toBe("memory");
  });

  it("gives every option a stable id, so the keyboard cursor addresses one thing", () => {
    const out = suggest(scope, "psp");
    expect(new Set(out.map((s) => s.id)).size).toBe(out.length);
    expect(suggest(scope, "psp").map((s) => s.id)).toEqual(out.map((s) => s.id));
  });
});
