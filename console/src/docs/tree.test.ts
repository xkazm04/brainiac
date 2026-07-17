import { describe, expect, it } from "vitest";

import type { DocSummary } from "@/lib/types";

import { buildSpaces, leafName, spaceKey, spaceLabel, toPage, UNFILED } from "./tree";

const doc = (slug: string, over: Partial<DocSummary> = {}): DocSummary => ({
  id: slug,
  slug,
  title: slug,
  doc_kind: "entity_page",
  status: "published",
  visibility: "org",
  updated_at: "2026-07-01T00:00:00Z",
  pending_review: false,
  dirty: false,
  ...over,
});

const spacesOf = (slugs: string[]) => buildSpaces(slugs.map((s) => toPage(doc(s))));

describe("toPage", () => {
  it("reads the namespace out of the slug", () => {
    expect(toPage(doc("payments/psp-gateway"))).toMatchObject({
      space: "payments",
      leaf: "psp-gateway",
    });
  });

  // The old flat corpus is still in the database; a page nobody filed is a page.
  it("files an un-namespaced slug under `unfiled` rather than dropping it", () => {
    expect(toPage(doc("retry-policy"))).toMatchObject({ space: UNFILED, leaf: "retry-policy" });
  });

  it("keeps deeper paths whole in the leaf — the first segment is the space", () => {
    expect(toPage(doc("payments/psp/timeouts"))).toMatchObject({
      space: "payments",
      leaf: "psp/timeouts",
    });
  });

  // A leading slash would otherwise mint an empty-named space.
  it("treats a leading slash as unfiled, not as a nameless space", () => {
    expect(toPage(doc("/orphan")).space).toBe(UNFILED);
  });

  it("indexes title and slug together for search", () => {
    expect(toPage(doc("payments/psp-gateway", { title: "PSP Gateway" })).hay).toBe(
      "psp gateway payments/psp-gateway",
    );
  });
});

describe("buildSpaces", () => {
  it("groups by namespace, biggest space first", () => {
    const s = spacesOf(["a/1", "b/1", "a/2", "a/3", "b/2"]);
    expect(s.map((x) => [x.name, x.pages.length])).toEqual([
      ["a", 3],
      ["b", 2],
    ]);
  });

  it("breaks a size tie by name so the rail never reorders itself", () => {
    expect(spacesOf(["z/1", "a/1"]).map((x) => x.name)).toEqual(["a", "z"]);
  });

  // `unfiled` is a bucket, not a team — it sorts last however big it gets.
  it("pins unfiled last even when it is the biggest", () => {
    expect(spacesOf(["x", "y", "z", "a/1"]).map((x) => x.name)).toEqual(["a", UNFILED]);
  });

  it("counts the work per space: awaiting review and recomposing", () => {
    const [s] = buildSpaces([
      toPage(doc("a/1", { pending_review: true })),
      toPage(doc("a/2", { dirty: true })),
      toPage(doc("a/3", { pending_review: true, dirty: true })),
      toPage(doc("a/4")),
    ]);
    expect(s).toMatchObject({ review: 2, dirty: 2 });
    expect(s.pages).toHaveLength(4);
  });

  it("sorts pages within a space by title", () => {
    const s = buildSpaces([
      toPage(doc("a/2", { title: "Zebra" })),
      toPage(doc("a/1", { title: "Alpha" })),
    ]);
    expect(s[0].pages.map((p) => p.doc.title)).toEqual(["Alpha", "Zebra"]);
  });

  it("is empty for an empty corpus", () => {
    expect(buildSpaces([])).toEqual([]);
  });
});

// These mirror the SERVER's `split_part(slug, '/', 1)` — the wiki's space
// directory now comes from the server facet, so the demo mirror must group by
// the same key. Deliberately NOT toPage().space (which buckets under `unfiled`).
describe("spaceKey", () => {
  it("takes the first slug segment as the space", () => {
    expect(spaceKey("payments/psp-gateway")).toBe("payments");
  });

  it("treats a whole un-namespaced slug as its own space (as split_part does)", () => {
    expect(spaceKey("retry-policy")).toBe("retry-policy");
  });

  it("yields the empty space for a leading slash, matching split_part", () => {
    expect(spaceKey("/orphan")).toBe("");
  });
});

describe("leafName", () => {
  it("is everything after the first slash", () => {
    expect(leafName("payments/psp/timeouts")).toBe("psp/timeouts");
  });

  it("is the whole slug when there is no namespace", () => {
    expect(leafName("retry-policy")).toBe("retry-policy");
  });
});

describe("spaceLabel", () => {
  it("shows the empty (leading-slash) space as unfiled", () => {
    expect(spaceLabel("")).toBe(UNFILED);
  });

  it("shows a named space as itself", () => {
    expect(spaceLabel("payments")).toBe("payments");
  });
});
