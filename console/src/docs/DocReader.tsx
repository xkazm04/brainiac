"use client";

/*
 * The composed-page reader (KB-PLAN KB2) — the surface that has to justify the
 * product.
 *
 * A wiki page is easy. What no wiki can do is answer "who says?" for every
 * sentence on it. So provenance is not a footnote here: each `[m:uuid]` the
 * composer emitted becomes an inline marker that opens the governed memory
 * behind that claim — its text, kind, owning team, status, and lifecycle.
 *
 * Two honesty rules the design is built around:
 *
 *  - **Lifecycle is not prose.** A claim backed by an `in_flight` or `proposed`
 *    memory is decided but NOT in production. The marker carries that colour
 *    and the paragraph is rimmed and labelled "not yet shipped" — documenting
 *    unshipped features as though they were live is a top doc-rot failure, and
 *    hiding it in the wording would repeat it.
 *  - **A citation we cannot resolve is a hole**, not a hidden defect: an
 *    unresolvable marker renders as an explicit "unresolved" mark rather than
 *    quietly disappearing.
 *
 * Sanitization: the block tree comes from src/docs/markdown.ts, which has no
 * raw-HTML node kind. Nothing the model writes can reach the DOM as markup.
 */

import { useState } from "react";

import {
  BORDER,
  FONT_DISPLAY,
  FONT_MONO,
  INK,
  INK_DIM,
  INK_FAINT,
  LABEL,
  MAGENTA,
  PANEL,
  band,
} from "@/design/theme";
import type { DocCitation, DocSection, MemoryLifecycle } from "@/lib/types";

import { asLifecycle } from "./facets";
import { parseDoc, type Block, type InlineNode } from "./markdown";
import SectionEditor, { type SectionEditorProps } from "./SectionEditor";

/** A citation whose `lifecycle` has been narrowed at the boundary (facets.ts):
 *  the wire type is a plain string, the UI's colour/caption tables are total
 *  over the union, and an unrecognized value degrades to `shipped`. */
type Cited = Omit<DocCitation, "lifecycle"> & { lifecycle: MemoryLifecycle };
const narrow = (c: DocCitation): Cited => ({ ...c, lifecycle: asLifecycle(c.lifecycle) });

/** Lifecycle → accent. Shipped is calm; anything unshipped is warm and loud. */
const lifecycleAccent = (l: MemoryLifecycle): string =>
  l === "shipped" ? band("beta") : l === "in_flight" ? band("gamma") : MAGENTA;

const LIFECYCLE_COPY: Record<MemoryLifecycle, string> = {
  shipped: "in product",
  in_flight: "decided, not yet shipped",
  proposed: "proposed — not decided",
};

const UNRESOLVED_ACCENT = MAGENTA;

interface Ctx {
  /** memory id → the resolved memory (from the API's `citations`). */
  byId: Map<string, Cited>;
  open: string | null;
  setOpen: (k: string | null) => void;
}

/** Superscript provenance marker. Click reveals the memory it came from. */
function Cite({ node, ctx, k }: { node: Extract<InlineNode, { t: "cite" }>; ctx: Ctx; k: string }) {
  const mem = ctx.byId.get(node.memoryId);
  const accent = mem ? lifecycleAccent(mem.lifecycle) : UNRESOLVED_ACCENT;
  const isOpen = ctx.open === k;
  return (
    <span className="relative inline-block align-baseline">
      <button
        type="button"
        aria-expanded={isOpen}
        aria-label={
          mem ? `Source ${node.n}: ${mem.kind} owned by ${mem.team ?? "org"}` : "Unresolved source"
        }
        onClick={() => ctx.setOpen(isOpen ? null : k)}
        onMouseEnter={() => ctx.setOpen(k)}
        className={`${FONT_MONO} mx-[2px] cursor-pointer rounded-[4px] border px-[5px] align-super text-[10px] leading-[15px] transition hover:brightness-125`}
        style={{
          color: accent,
          borderColor: `${accent}55`,
          background: isOpen ? `${accent}22` : "transparent",
        }}
      >
        {mem ? node.n : "?"}
      </button>
      {isOpen && (
        <span
          role="tooltip"
          onMouseLeave={() => ctx.setOpen(null)}
          className="absolute bottom-[calc(100%+8px)] left-0 z-30 block w-[min(28rem,80vw)] rounded-lg border p-4 shadow-2xl"
          style={{ background: "#0e0d15", borderColor: BORDER }}
        >
          {mem ? <CitationCard c={mem} n={node.n} /> : <Unresolved id={node.memoryId} />}
        </span>
      )}
    </span>
  );
}

function Unresolved({ id }: { id: string }) {
  return (
    <span className="block">
      <span className={LABEL} style={{ color: UNRESOLVED_ACCENT }}>
        unresolved source
      </span>
      <span className={`${FONT_MONO} mt-2 block text-[12px]`} style={{ color: INK_DIM }}>
        This claim cites <code>{id}</code>, which the API did not return — the memory may have been
        superseded or is outside your visibility. Treat the claim as unverified.
      </span>
    </span>
  );
}

/** The memory behind a claim: content, kind, lifecycle, team, status. */
function CitationCard({ c, n }: { c: Cited; n: number }) {
  const accent = lifecycleAccent(c.lifecycle);
  return (
    <span className="block">
      <span className="flex items-center gap-2">
        <span className={`${FONT_MONO} text-[11px]`} style={{ color: INK_FAINT }}>
          [{n}]
        </span>
        <span
          className={`${FONT_MONO} rounded-full border px-2 py-[1px] text-[10px] uppercase tracking-[0.14em]`}
          style={{ color: accent, borderColor: `${accent}55` }}
        >
          {c.lifecycle.replace("_", " ")}
        </span>
        <span className={`${FONT_MONO} text-[11px]`} style={{ color: INK_FAINT }}>
          {LIFECYCLE_COPY[c.lifecycle]}
        </span>
      </span>
      <span className="mt-2 block text-[13px] leading-relaxed" style={{ color: INK }}>
        {c.content}
      </span>
      <span
        className={`${FONT_MONO} mt-3 flex flex-wrap gap-x-4 gap-y-1 text-[11px]`}
        style={{ color: INK_FAINT }}
      >
        <span>kind · {c.kind}</span>
        <span>team · {c.team ?? "org-wide"}</span>
        <span>status · {c.status}</span>
      </span>
      <a
        href={`/memories/${c.memory_id}`}
        className={`${FONT_MONO} mt-3 inline-block text-[11px] underline underline-offset-4`}
        style={{ color: band("gamma") }}
      >
        open the memory →
      </a>
    </span>
  );
}

function Inline({ nodes, ctx, path }: { nodes: InlineNode[]; ctx: Ctx; path: string }) {
  return (
    <>
      {nodes.map((n, i) => {
        const k = `${path}.${i}`;
        switch (n.t) {
          case "text":
            return <span key={k}>{n.v}</span>;
          case "code":
            return (
              <code
                key={k}
                className={`${FONT_MONO} rounded px-1.5 py-[1px] text-[0.85em]`}
                style={{ background: "rgba(255,255,255,0.06)", color: band("gamma") }}
              >
                {n.v}
              </code>
            );
          case "strong":
            return (
              <strong key={k} style={{ color: INK }}>
                <Inline nodes={n.kids} ctx={ctx} path={k} />
              </strong>
            );
          case "em":
            return (
              <em key={k}>
                <Inline nodes={n.kids} ctx={ctx} path={k} />
              </em>
            );
          case "link":
            return (
              <a
                key={k}
                href={n.href}
                className="underline underline-offset-4"
                style={{ color: band("gamma") }}
              >
                <Inline nodes={n.kids} ctx={ctx} path={k} />
              </a>
            );
          case "cite":
            return <Cite key={k} node={n} ctx={ctx} k={k} />;
        }
      })}
    </>
  );
}

/** Every lifecycle a block's citations carry — drives the "not yet shipped" rim. */
function blockLifecycles(b: Block, byId: Map<string, Cited>): MemoryLifecycle[] {
  const walk = (nodes: InlineNode[]): MemoryLifecycle[] =>
    nodes.flatMap((n) => {
      if (n.t === "cite") {
        const m = byId.get(n.memoryId);
        return m ? [m.lifecycle] : [];
      }
      if (n.t === "strong" || n.t === "em" || n.t === "link") return walk(n.kids);
      return [];
    });
  switch (b.t) {
    case "para":
    case "quote":
    case "heading":
      return walk(b.kids);
    case "list":
      return b.items.flatMap(walk);
    case "table":
      return [...b.head, ...b.rows.flat()].flatMap(walk);
    default:
      return [];
  }
}

function BlockView({ b, ctx, path }: { b: Block; ctx: Ctx; path: string }) {
  const inner = (nodes: InlineNode[]) => <Inline nodes={nodes} ctx={ctx} path={path} />;
  switch (b.t) {
    case "heading": {
      const size =
        b.level <= 2 ? "text-2xl mt-12" : b.level === 3 ? "text-lg mt-8" : "text-base mt-6";
      return (
        <h2 className={`${FONT_DISPLAY} ${size} mb-3 font-medium`} style={{ color: INK }}>
          {inner(b.kids)}
        </h2>
      );
    }
    case "para":
      return (
        <p className="my-4 text-[15px] leading-[1.75]" style={{ color: "rgba(233,237,255,0.82)" }}>
          {inner(b.kids)}
        </p>
      );
    case "quote":
      return (
        <blockquote
          className="my-5 border-l-2 pl-4 text-[15px] leading-relaxed italic"
          style={{ borderColor: BORDER, color: INK_DIM }}
        >
          {inner(b.kids)}
        </blockquote>
      );
    case "list": {
      const Tag = b.ordered ? "ol" : "ul";
      return (
        <Tag
          className={`my-4 ml-5 space-y-2 text-[15px] leading-[1.7] ${b.ordered ? "list-decimal" : "list-disc"}`}
          style={{ color: "rgba(233,237,255,0.82)" }}
        >
          {b.items.map((it, i) => (
            <li key={i}>
              <Inline nodes={it} ctx={ctx} path={`${path}.i${i}`} />
            </li>
          ))}
        </Tag>
      );
    }
    case "code":
      // Verbatim evidence: `detail_md` copied from the memory, never re-typed
      // by the model. It is quoted, so it is displayed as a quote.
      return (
        <pre
          className={`${FONT_MONO} my-5 overflow-x-auto rounded-lg border p-4 text-[12.5px] leading-relaxed`}
          style={{ background: "rgba(255,255,255,0.02)", borderColor: BORDER, color: INK_DIM }}
        >
          {b.lang && (
            <span className={`${LABEL} mb-2 block`} style={{ color: INK_FAINT }}>
              {b.lang} · verbatim
            </span>
          )}
          <code>{b.v}</code>
        </pre>
      );
    case "table":
      return (
        <div className="my-5 overflow-x-auto">
          <table className="w-full border-collapse text-left text-[13px]">
            <thead>
              <tr>
                {b.head.map((h, i) => (
                  <th
                    key={i}
                    className={`${LABEL} border-b px-3 py-2`}
                    style={{ borderColor: BORDER, color: INK_FAINT }}
                  >
                    <Inline nodes={h} ctx={ctx} path={`${path}.h${i}`} />
                  </th>
                ))}
              </tr>
            </thead>
            <tbody>
              {b.rows.map((r, ri) => (
                <tr key={ri}>
                  {r.map((c, ci) => (
                    <td
                      key={ci}
                      className="border-b px-3 py-2"
                      style={{ borderColor: BORDER, color: INK_DIM }}
                    >
                      <Inline nodes={c} ctx={ctx} path={`${path}.r${ri}.${ci}`} />
                    </td>
                  ))}
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      );
    case "rule":
      return <hr className="my-8" style={{ borderColor: BORDER }} />;
  }
}

/** The plain text of an inline run — used to match a rendered `## heading`
 *  back to the section the API named, which is the only handle the editor has
 *  on a `section_id`. */
function plainText(nodes: InlineNode[]): string {
  return nodes
    .map((n) => {
      switch (n.t) {
        case "text":
        case "code":
          return n.v;
        case "strong":
        case "em":
        case "link":
          return plainText(n.kids);
        default:
          return "";
      }
    })
    .join("")
    .trim();
}

export interface DocReaderProps {
  contentMd: string;
  citations: DocCitation[];
  /** Rendered greyed as "the version awaiting review", not as the page. */
  draft?: boolean;
  /** The page's sections — the reader matches each `## heading` back to one so
   *  the editor knows which `section_id` it is editing, and in which mode. */
  sections: DocSection[];
  /** The edit server action — passed ONLY when the console is live. */
  edit?: SectionEditorProps["edit"];
}

/**
 * Render a composed page with per-claim provenance.
 *
 * The right-hand rail is the same data as the markers — the page's whole
 * source list, numbered — so provenance is legible even before anyone hovers.
 */
export default function DocReader({
  contentMd,
  citations,
  draft = false,
  sections,
  edit,
}: DocReaderProps) {
  const [open, setOpen] = useState<string | null>(null);
  const byId = new Map(citations.map((c) => [c.memory_id.toLowerCase(), narrow(c)]));
  const { blocks, order } = parseDoc(contentMd);
  const ctx: Ctx = { byId, open, setOpen };

  // heading → section. Editing is offered only on the published page (never on
  // a draft) and only when the console is live enough to carry the action.
  const editable = new Map<string, DocSection>(
    !draft && edit ? sections.map((s) => [s.heading.trim().toLowerCase(), s]) : [],
  );
  const sectionAt = (b: Block): DocSection | undefined =>
    b.t === "heading" && b.level <= 2 ? editable.get(plainText(b.kids).toLowerCase()) : undefined;

  const used = order.map((id, i) => ({ n: i + 1, id, mem: byId.get(id) }));
  const unshipped = used.filter((u) => u.mem && u.mem.lifecycle !== "shipped");
  const unresolved = used.filter((u) => !u.mem);

  return (
    <div className="grid gap-10 lg:grid-cols-[minmax(0,1fr)_20rem]">
      <article className={draft ? "opacity-80" : undefined}>
        {(unshipped.length > 0 || unresolved.length > 0) && (
          <div
            className="mb-6 rounded-lg border p-4"
            style={{ borderColor: `${band("gamma")}40`, background: `${band("gamma")}0d` }}
          >
            <span className={LABEL} style={{ color: band("gamma") }}>
              read with care
            </span>
            <p className={`${FONT_MONO} mt-1 text-[12.5px]`} style={{ color: INK_DIM }}>
              {unshipped.length > 0 && (
                <>
                  {unshipped.length} claim{unshipped.length === 1 ? "" : "s"} on this page
                  {unresolved.length > 0 ? ", " : " "}
                  {unshipped.length === 1 ? "is" : "are"} backed by a memory that is not shipped —
                  decided, but not in production.{" "}
                </>
              )}
              {unresolved.length > 0 && (
                <>
                  {unresolved.length} citation{unresolved.length === 1 ? "" : "s"} could not be
                  resolved to a memory you can see.
                </>
              )}
            </p>
          </div>
        )}
        {blocks.map((b, i) => {
          const sec = sectionAt(b);
          const lc = blockLifecycles(b, byId);
          const hot = lc.find((l) => l !== "shipped");
          if (!hot)
            return (
              <div key={i}>
                <BlockView b={b} ctx={ctx} path={`b${i}`} />
                {sec && edit && <SectionEditor section={sec} edit={edit} />}
              </div>
            );
          const accent = lifecycleAccent(hot);
          return (
            <div
              key={i}
              className="relative my-1 border-l-2 pl-4"
              style={{ borderColor: `${accent}66` }}
            >
              <BlockView b={b} ctx={ctx} path={`b${i}`} />
              <span
                className={`${FONT_MONO} mb-3 block text-[10px] uppercase tracking-[0.18em]`}
                style={{ color: accent }}
              >
                {hot === "in_flight" ? "not yet shipped" : "proposed — not decided"}
              </span>
            </div>
          );
        })}
      </article>

      <aside className="lg:sticky lg:top-8 lg:self-start">
        <span className={LABEL} style={{ color: INK_FAINT }}>
          sources · {used.length}
        </span>
        <p className={`${FONT_MONO} mt-2 text-[11px] leading-relaxed`} style={{ color: INK_FAINT }}>
          Every sentence above is compiled from a canonical memory a named human signed. Nothing on
          this page was written by hand.
        </p>
        <ol className="mt-4 space-y-2">
          {used.map((u) => {
            const accent = u.mem ? lifecycleAccent(u.mem.lifecycle) : UNRESOLVED_ACCENT;
            return (
              <li
                key={u.id}
                className="rounded-lg p-3"
                style={{ background: PANEL, border: `1px solid ${BORDER}` }}
              >
                <div className="flex items-start gap-2">
                  <span className={`${FONT_MONO} text-[11px]`} style={{ color: accent }}>
                    [{u.n}]
                  </span>
                  <div className="min-w-0">
                    <p className="text-[12.5px] leading-snug" style={{ color: INK_DIM }}>
                      {u.mem ? u.mem.content : "Unresolved — this memory was not returned."}
                    </p>
                    <p
                      className={`${FONT_MONO} mt-1.5 text-[10px] uppercase tracking-[0.12em]`}
                      style={{ color: accent }}
                    >
                      {u.mem
                        ? `${u.mem.lifecycle.replace("_", " ")} · ${u.mem.team ?? "org"} · ${u.mem.kind}`
                        : "unresolved"}
                    </p>
                  </div>
                </div>
              </li>
            );
          })}
        </ol>
      </aside>
    </div>
  );
}
