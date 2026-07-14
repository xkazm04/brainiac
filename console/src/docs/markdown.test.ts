import { describe, expect, it } from "vitest";

import { DEMO_DOC } from "./docs-demo";
import {
  citedMemoryIds,
  parseDoc,
  parseInline,
  unresolvedCitations,
  type InlineNode,
} from "./markdown";

const A = "1a2b3c4d-0001-4a11-8c01-9f0000000001";
const B = "1a2b3c4d-0002-4a11-8c01-9f0000000002";

const texts = (kids: InlineNode[]) =>
  kids.filter((k): k is { t: "text"; v: string } => k.t === "text").map((k) => k.v);
const cites = (kids: InlineNode[]) =>
  kids.filter((k): k is { t: "cite"; memoryId: string; n: number } => k.t === "cite");

describe("citation parsing", () => {
  it("splits a sentence into text + a citation node", () => {
    const kids = parseInline(`Retries are capped at four. [m:${A}] Then we give up.`);
    expect(kids).toEqual([
      { t: "text", v: "Retries are capped at four. " },
      { t: "cite", memoryId: A, n: 1 },
      { t: "text", v: " Then we give up." },
    ]);
  });

  it("numbers citations by first appearance and reuses the number", () => {
    const { blocks, order } = parseDoc(
      `One. [m:${A}]\n\nTwo. [m:${B}]\n\nThree, same source. [m:${A}]`,
    );
    expect(order).toEqual([A, B]);
    const all = blocks.flatMap((b) => (b.t === "para" ? cites(b.kids) : []));
    expect(all.map((c) => c.n)).toEqual([1, 2, 1]);
    expect(all.map((c) => c.memoryId)).toEqual([A, B, A]);
  });

  it("treats our own <sub>[m:uuid]</sub> evidence footer as a citation, not HTML", () => {
    const kids = parseInline(`<sub>[m:${A}]</sub>`);
    expect(kids).toEqual([{ t: "cite", memoryId: A, n: 1 }]);
  });

  it("does not mistake other bracketed text for a citation", () => {
    const kids = parseInline("[not-a-citation] and [m:nope] stay literal");
    expect(cites(kids)).toEqual([]);
    expect(texts(kids).join("")).toBe("[not-a-citation] and [m:nope] stay literal");
  });

  it("lifts citations out of list items and headings too", () => {
    const { order } = parseDoc(`# Title [m:${A}]\n\n- item [m:${B}]\n`);
    expect(order).toEqual([A, B]);
  });

  it("citedMemoryIds returns first-appearance order across the whole page", () => {
    expect(citedMemoryIds(DEMO_DOC.revision!.content_md)).toEqual([
      "1a2b3c4d-0001-4a11-8c01-9f0000000001",
      "1a2b3c4d-0006-4a11-8c01-9f0000000006",
      "1a2b3c4d-0002-4a11-8c01-9f0000000002",
      "1a2b3c4d-0005-4a11-8c01-9f0000000005",
      "1a2b3c4d-0003-4a11-8c01-9f0000000003",
      "1a2b3c4d-0004-4a11-8c01-9f0000000004",
    ]);
  });

  it("flags a cited memory the API did not resolve (a provenance hole)", () => {
    expect(unresolvedCitations(`Claim. [m:${A}] Other. [m:${B}]`, [A.toUpperCase()])).toEqual([B]);
  });
});

describe("block parsing", () => {
  it("parses the demo page's structure", () => {
    const { blocks } = parseDoc(DEMO_DOC.revision!.content_md);
    const kinds = blocks.map((b) => b.t);
    expect(kinds).toContain("heading");
    expect(kinds).toContain("para");
    expect(kinds).toContain("code");
    expect(kinds).toContain("table");
  });

  it("keeps a fenced evidence block verbatim and unparsed", () => {
    const { blocks } = parseDoc("```toml\nmax_attempts = 4\n# **not bold**\n```");
    expect(blocks).toEqual([
      { t: "code", lang: "toml", v: "max_attempts = 4\n# **not bold**" },
    ]);
  });

  it("renders unknown HTML as literal text (no raw-HTML escape hatch)", () => {
    const { blocks } = parseDoc('<img src=x onerror="alert(1)">');
    expect(blocks[0]).toEqual({
      t: "para",
      kids: [{ t: "text", v: '<img src=x onerror="alert(1)">' }],
    });
  });

  it("drops a javascript: link back to plain text", () => {
    const kids = parseInline("[click](javascript:alert(1))");
    expect(kids.some((k) => k.t === "link")).toBe(false);
  });

  it("parses inline emphasis, code and links", () => {
    expect(parseInline("a **b** `c` [d](/docs)")).toEqual([
      { t: "text", v: "a " },
      { t: "strong", kids: [{ t: "text", v: "b" }] },
      { t: "text", v: " " },
      { t: "code", v: "c" },
      { t: "text", v: " " },
      { t: "link", href: "/docs", kids: [{ t: "text", v: "d" }] },
    ]);
  });

  it("parses a gfm table", () => {
    const { blocks } = parseDoc("| a | b |\n| --- | --- |\n| 1 | 2 |");
    const t = blocks[0];
    expect(t.t).toBe("table");
    if (t.t !== "table") return;
    expect(t.head.length).toBe(2);
    expect(t.rows.length).toBe(1);
  });
});
