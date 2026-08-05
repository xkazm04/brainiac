/*
 * The sitemap — every surface an anonymous visitor may reach, and nothing else.
 *
 * The authority for "public" is `isPublicSurface` in src/design/routes.ts (the
 * middleware's allow-list). This file must never list a path that allow-list
 * would bounce to /login: a sitemap entry that redirects is a crawl error, and
 * a sitemap full of them gets the whole file distrusted. The assertion at the
 * bottom of ROUTES enforces that rather than trusting me to remember.
 *
 * `/login` is public but deliberately absent: it is the operator gate for an
 * existing deployment. There is nothing to index and nobody should arrive there
 * from a search result — `/signup` is the door for someone with no passcode.
 *
 * Routes are filtered by whether their page file actually exists. That is not
 * defensiveness for its own sake: /privacy and /terms are being added in a
 * separate change, and a sitemap that promises a 404 is worse than one that is
 * a page short for a day.
 */

import { existsSync } from "node:fs";
import path from "node:path";
import type { MetadataRoute } from "next";

import { isPublicSurface } from "@/design/routes";
import { siteOrigin } from "@/lib/site-url";

interface Entry {
  path: string;
  changeFrequency: MetadataRoute.Sitemap[number]["changeFrequency"];
  priority: number;
}

const ROUTES: Entry[] = [
  { path: "/", changeFrequency: "weekly", priority: 1 },
  { path: "/pitch", changeFrequency: "weekly", priority: 0.9 },
  { path: "/demo", changeFrequency: "monthly", priority: 0.8 },
  { path: "/signup", changeFrequency: "monthly", priority: 0.8 },
  { path: "/kb", changeFrequency: "weekly", priority: 0.6 },
  { path: "/library", changeFrequency: "weekly", priority: 0.6 },
  { path: "/privacy", changeFrequency: "yearly", priority: 0.3 },
  { path: "/terms", changeFrequency: "yearly", priority: 0.3 },
];

/** `/pitch` → console/app/pitch/page.tsx. `/` → console/app/page.tsx. */
const pageExists = (route: string): boolean => {
  const dir = path.join(process.cwd(), "app", route === "/" ? "" : route.slice(1));
  return ["page.tsx", "page.ts", "page.jsx", "page.js"].some((f) =>
    existsSync(path.join(dir, f)),
  );
};

export default function sitemap(): MetadataRoute.Sitemap {
  const origin = siteOrigin();
  const lastModified = new Date();

  return ROUTES.filter((r) => {
    if (!isPublicSurface(r.path)) {
      throw new Error(
        `sitemap lists ${r.path}, which isPublicSurface() rejects — it would redirect to /login`,
      );
    }
    return pageExists(r.path);
  }).map((r) => ({
    url: `${origin}${r.path}`,
    lastModified,
    changeFrequency: r.changeFrequency,
    priority: r.priority,
  }));
}
