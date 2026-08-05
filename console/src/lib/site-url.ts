/*
 * The site's own absolute origin.
 *
 * Every absolute-URL surface Next generates — `metadataBase` (which is what
 * turns a relative og:image into the absolute URL a crawler requires), the
 * sitemap entries, and the `Sitemap:` line in robots.txt — needs one origin,
 * and they must all agree. Disagreement is the classic way a share card
 * silently 404s on one social platform and works on another.
 *
 * There is no production domain yet, so this is deliberately NOT hardcoded:
 *
 *   1. NEXT_PUBLIC_SITE_URL   — set this when a real domain exists. It wins.
 *   2. VERCEL_PROJECT_PRODUCTION_URL / VERCEL_URL — the preview/prod host Vercel
 *      injects, so a deploy is self-describing before anyone configures step 1.
 *   3. http://localhost:3100  — the dev port (see package.json `dev`).
 *
 * Returning a URL rather than a string is intentional: `new URL()` throws on a
 * malformed value at build time, which is where a typo'd env var should die —
 * not later, as a half-broken og:image nobody notices for a month.
 */

const FALLBACK = "http://localhost:3100";

/** Accept "example.com" as well as "https://example.com" — env vars get pasted. */
const withScheme = (raw: string): string =>
  /^https?:\/\//i.test(raw) ? raw : `https://${raw}`;

export function siteUrl(): URL {
  const raw =
    process.env.NEXT_PUBLIC_SITE_URL ||
    process.env.VERCEL_PROJECT_PRODUCTION_URL ||
    process.env.VERCEL_URL ||
    FALLBACK;
  return new URL(withScheme(raw.trim()));
}

/** `siteUrl()` as a string with no trailing slash — for concatenation. */
export const siteOrigin = (): string => siteUrl().origin;
