/*
 * The composed-page parser (KB-PLAN KB2).
 *
 * Two jobs, one pass, no dependency:
 *
 * 1. **Citation extraction.** Composition writes an inline `[m:<uuid>]` after
 *    every factual sentence, and closes verbatim evidence blocks with a
 *    `<sub>[m:<uuid>]</sub>` footer. Both are the SAME thing — a claim pointing
 *    at the governed memory a named human signed — so both parse into one
 *    `cite` node and share the document's citation numbering. The reader turns
 *    those nodes into provenance markers; that traceability is the product.
 *
 * 2. **Markdown → a typed block tree** the reader renders with React elements.
 *    We deliberately do NOT ship a markdown library with `rehype-raw`: the
 *    content is model-authored, and the cheapest sanitizer is a renderer that
 *    has no HTML escape hatch at all. Anything that is not one of the node
 *    kinds below renders as literal text (React escapes it). The single
 *    exception is our own `<sub>[m:uuid]</sub>` evidence footer, which is
 *    normalized to a bare citation BEFORE inline parsing — we emit it, so we
 *    are entitled to understand it; every other tag is inert prose.
 *
 * Pure functions, no React — so the parsing is unit-testable on its own
 * (src/docs/markdown.test.ts) rather than through the DOM.
 */

/** `[m:<uuid>]` — the citation marker composition emits. */
const CITE = /\[m:([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12})\]/;
/** Our own evidence footer: `<sub>[m:uuid]</sub>` under a verbatim block. */
const SUB_CITE = new RegExp(`<sub>\\s*${CITE.source}\\s*</sub>`, "gi");

export type InlineNode =
  | { t: "text"; v: string }
  | { t: "code"; v: string }
  | { t: "strong"; kids: InlineNode[] }
  | { t: "em"; kids: InlineNode[] }
  | { t: "link"; href: string; kids: InlineNode[] }
  /** A claim's provenance. `n` is the document-wide citation number (1-based). */
  | { t: "cite"; memoryId: string; n: number };

export type Block =
  | { t: "heading"; level: number; kids: InlineNode[] }
  | { t: "para"; kids: InlineNode[] }
  | { t: "quote"; kids: InlineNode[] }
  | { t: "list"; ordered: boolean; items: InlineNode[][] }
  | { t: "code"; lang: string | null; v: string }
  | { t: "table"; head: InlineNode[][]; rows: InlineNode[][][] }
  | { t: "rule" };

export interface ParsedDoc {
  blocks: Block[];
  /** Memory ids in order of first appearance; index + 1 is the citation number. */
  order: string[];
}

/** Citation numbering, shared across every inline parse in one document. */
class Numbering {
  readonly order: string[] = [];
  numberFor(id: string): number {
    const at = this.order.indexOf(id);
    if (at >= 0) return at + 1;
    this.order.push(id);
    return this.order.length;
  }
}

/**
 * Split one line of markdown into inline nodes, lifting every `[m:uuid]`
 * (and every `<sub>[m:uuid]</sub>` evidence footer) into a `cite` node.
 */
export function parseInline(src: string, num: Numbering = new Numbering()): InlineNode[] {
  // Our own footer form collapses to the bare marker — one code path for both.
  const text = src.replace(SUB_CITE, (_m, id: string) => `[m:${id}]`);
  const out: InlineNode[] = [];
  const push = (n: InlineNode) => {
    const last = out[out.length - 1];
    if (n.t === "text" && last?.t === "text") last.v += n.v;
    else if (n.t !== "text" || n.v !== "") out.push(n);
  };

  let i = 0;
  while (i < text.length) {
    const rest = text.slice(i);

    const cite = rest.match(CITE);
    if (cite && cite.index === 0) {
      push({ t: "cite", memoryId: cite[1].toLowerCase(), n: num.numberFor(cite[1].toLowerCase()) });
      i += cite[0].length;
      continue;
    }

    const ch = text[i];
    if (ch === "`") {
      const end = text.indexOf("`", i + 1);
      if (end > i) {
        push({ t: "code", v: text.slice(i + 1, end) });
        i = end + 1;
        continue;
      }
    }
    if (rest.startsWith("**")) {
      const end = text.indexOf("**", i + 2);
      if (end > i + 1) {
        push({ t: "strong", kids: parseInline(text.slice(i + 2, end), num) });
        i = end + 2;
        continue;
      }
    }
    if ((ch === "*" || ch === "_") && text[i + 1] !== ch) {
      const end = text.indexOf(ch, i + 1);
      if (end > i + 1) {
        push({ t: "em", kids: parseInline(text.slice(i + 1, end), num) });
        i = end + 1;
        continue;
      }
    }
    if (ch === "[") {
      const link = rest.match(/^\[([^\]]*)\]\(([^)\s]+)\)/);
      // Only http(s) and in-console links survive; `javascript:` et al. are
      // rendered as plain text rather than becoming an executable href.
      if (link && /^(https?:\/\/|\/|#)/i.test(link[2])) {
        push({ t: "link", href: link[2], kids: parseInline(link[1], num) });
        i += link[0].length;
        continue;
      }
    }

    // Plain run: up to the next character that could start a construct.
    const next = text.slice(i + 1).search(/[`*_[]/);
    const take = next === -1 ? text.length - i : next + 1;
    push({ t: "text", v: text.slice(i, i + take) });
    i += take;
  }
  return out;
}

const isTableDelim = (l: string) => /^\s*\|?[\s:|-]*-[\s:|-]*\|?\s*$/.test(l) && l.includes("-");
const cells = (l: string) =>
  l
    .trim()
    .replace(/^\|/, "")
    .replace(/\|$/, "")
    .split("|")
    .map((c) => c.trim());

/**
 * Parse a composed page into blocks plus its citation order.
 *
 * The returned `order` is authoritative for citation numbering: marker [1] is
 * `order[0]`. The reader joins it against the `citations` the API resolved.
 */
export function parseDoc(md: string): ParsedDoc {
  const num = new Numbering();
  const inline = (s: string) => parseInline(s, num);
  const lines = md.replace(/\r\n/g, "\n").split("\n");
  const blocks: Block[] = [];
  let i = 0;

  while (i < lines.length) {
    const line = lines[i];
    if (line.trim() === "") {
      i++;
      continue;
    }

    // Fenced code — verbatim `detail_md` evidence lives here; never re-parsed.
    const fence = line.match(/^\s*```+\s*(\S+)?\s*$/);
    if (fence) {
      const body: string[] = [];
      i++;
      while (i < lines.length && !/^\s*```+\s*$/.test(lines[i])) body.push(lines[i++]);
      i++; // closing fence
      blocks.push({ t: "code", lang: fence[1] ?? null, v: body.join("\n") });
      continue;
    }

    const heading = line.match(/^(#{1,6})\s+(.*)$/);
    if (heading) {
      blocks.push({ t: "heading", level: heading[1].length, kids: inline(heading[2].trim()) });
      i++;
      continue;
    }

    if (/^\s*(---+|\*\*\*+|___+)\s*$/.test(line)) {
      blocks.push({ t: "rule" });
      i++;
      continue;
    }

    if (/^\s*>\s?/.test(line)) {
      const body: string[] = [];
      while (i < lines.length && /^\s*>\s?/.test(lines[i])) {
        body.push(lines[i].replace(/^\s*>\s?/, ""));
        i++;
      }
      blocks.push({ t: "quote", kids: inline(body.join(" ")) });
      continue;
    }

    // GFM table: a header row followed by a delimiter row.
    if (line.includes("|") && i + 1 < lines.length && isTableDelim(lines[i + 1])) {
      const head = cells(line).map(inline);
      i += 2;
      const rows: InlineNode[][][] = [];
      while (i < lines.length && lines[i].includes("|") && lines[i].trim() !== "") {
        rows.push(cells(lines[i]).map(inline));
        i++;
      }
      blocks.push({ t: "table", head, rows });
      continue;
    }

    const bullet = line.match(/^\s*([-*+]|\d+\.)\s+(.*)$/);
    if (bullet) {
      const ordered = /\d/.test(bullet[1]);
      const items: InlineNode[][] = [];
      while (i < lines.length) {
        const m = lines[i].match(/^\s*([-*+]|\d+\.)\s+(.*)$/);
        if (!m || /\d/.test(m[1]) !== ordered) break;
        items.push(inline(m[2]));
        i++;
      }
      blocks.push({ t: "list", ordered, items });
      continue;
    }

    // Paragraph: to the next blank line or block opener.
    const para: string[] = [];
    while (i < lines.length && lines[i].trim() !== "") {
      const l = lines[i];
      if (/^\s*(#{1,6}\s|>|```|---|\*\*\*|___)/.test(l) || /^\s*([-*+]|\d+\.)\s+/.test(l)) break;
      para.push(l.trim());
      i++;
    }
    if (para.length === 0) {
      i++;
      continue;
    }
    blocks.push({ t: "para", kids: inline(para.join(" ")) });
  }

  return { blocks, order: num.order };
}

/** Every memory id cited in a page, first-appearance order. */
export const citedMemoryIds = (md: string): string[] => parseDoc(md).order;

/** Cited ids that the API could not resolve — a page citing a memory the
 *  reader cannot show is a provenance hole, and the reader says so. */
export function unresolvedCitations(md: string, resolved: Iterable<string>): string[] {
  const have = new Set([...resolved].map((s) => s.toLowerCase()));
  return citedMemoryIds(md).filter((id) => !have.has(id));
}
