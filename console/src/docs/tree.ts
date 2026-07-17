/*
 * The page tree (KB-PLAN KB2).
 *
 * The corpus already HAS a folder structure and the old index threw it away.
 * Composition namespaces every slug as `<team>/<page-name>` — `payments/psp-gateway`
 * — so the hierarchy is not something the console has to invent, guess, or store:
 * it is sitting in the slug, and deriving it is a string split. What made the flat
 * list unusable at 436 pages was never missing data. It was refusing to read it.
 *
 * Pure functions, no React — the shape of the tree is unit-testable on its own
 * (tree.test.ts) rather than through the DOM, same rule as markdown.ts.
 */

import type { DocSummary } from "@/lib/types";

/** Pages whose slug carries no namespace (the old flat `retry-policy` doc).
 *  They are a REAL space, not an error: a page nobody filed is still a page,
 *  and dropping it on the floor would be the flat list's sin inverted. */
export const UNFILED = "unfiled";

export interface WikiPage {
  doc: DocSummary;
  /** First slug segment, or UNFILED. */
  space: string;
  /** Everything after the namespace — the leaf name, as filed. */
  leaf: string;
  /** Lowercased title + slug, precomputed: search re-tests every page per
   *  keystroke, and lowercasing 436 strings per stroke is pure waste. */
  hay: string;
}

export interface WikiSpace {
  name: string;
  pages: WikiPage[];
  /** Pages in this space whose current revision is waiting on a human. */
  review: number;
  /** Pages knowingly behind their sources — a recompose is queued. */
  dirty: number;
}

const split = (slug: string): { space: string; leaf: string } => {
  const at = slug.indexOf("/");
  if (at <= 0) return { space: UNFILED, leaf: slug };
  return { space: slug.slice(0, at), leaf: slug.slice(at + 1) };
};

/**
 * The server's own space key for a slug: `split_part(slug, '/', 1)` — the first
 * segment, or the WHOLE slug when it has no `/`, or `""` for a leading slash.
 *
 * This is deliberately NOT `toPage().space` (which buckets every un-namespaced
 * slug under one `unfiled` label). The wiki's space directory now comes from the
 * server's `facets.spaces`, so the client must group by the SAME key the server
 * does — otherwise the demo mirror and the live view would disagree on what a
 * space is. `spaceLabel` handles the `""` case for DISPLAY.
 */
export const spaceKey = (slug: string): string => {
  const at = slug.indexOf("/");
  return at < 0 ? slug : slug.slice(0, at);
};

/** The leaf name to show for a slug inside its space — everything after the
 *  first `/`, or the whole slug when there is none. */
export const leafName = (slug: string): string => {
  const at = slug.indexOf("/");
  return at < 0 ? slug : slug.slice(at + 1);
};

/** How a space FACET value reads on screen: the empty key (a leading-slash slug
 *  the server groups under `""`) is the un-namespaced bucket, shown as `unfiled`. */
export const spaceLabel = (value: string): string => (value === "" ? UNFILED : value);

export const toPage = (doc: DocSummary): WikiPage => {
  const { space, leaf } = split(doc.slug);
  return { doc, space, leaf, hay: `${doc.title} ${doc.slug}`.toLowerCase() };
};

/**
 * Group pages into spaces, biggest first.
 *
 * `unfiled` is pinned last however big it gets: it is a bucket, not a team, and
 * sorting it into the middle of the rail would read as one.
 */
export function buildSpaces(pages: WikiPage[]): WikiSpace[] {
  const by = new Map<string, WikiPage[]>();
  for (const p of pages) {
    const list = by.get(p.space);
    if (list) list.push(p);
    else by.set(p.space, [p]);
  }
  return [...by.entries()]
    .map(([name, list]) => ({
      name,
      pages: [...list].sort((a, b) => a.doc.title.localeCompare(b.doc.title)),
      review: list.filter((p) => p.doc.pending_review).length,
      dirty: list.filter((p) => p.doc.dirty).length,
    }))
    .sort((a, b) => {
      if (a.name === UNFILED) return 1;
      if (b.name === UNFILED) return -1;
      return b.pages.length - a.pages.length || a.name.localeCompare(b.name);
    });
}
