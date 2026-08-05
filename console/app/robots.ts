/*
 * robots.txt.
 *
 * The disallow list is the inverse of src/design/routes.ts's public allow-list.
 * It is a crawl-budget hint, not a security control — the middleware is what
 * actually refuses these paths — but stating it keeps crawlers out of a wall of
 * /login redirects, which is how a small site earns a "soft 404" reputation.
 *
 * /login is disallowed on purpose even though it IS public: nobody should reach
 * an operator gate from a search result. /signup is the indexable front door.
 */

import type { MetadataRoute } from "next";

import { siteOrigin } from "@/lib/site-url";

export default function robots(): MetadataRoute.Robots {
  const origin = siteOrigin();
  return {
    rules: [
      {
        userAgent: "*",
        allow: "/",
        disallow: ["/console", "/api/", "/login"],
      },
    ],
    sitemap: `${origin}/sitemap.xml`,
    host: origin,
  };
}
