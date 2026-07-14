/*
 * The boundary between the public pitch and the real org's knowledge base.
 *
 * PUBLIC (example data only, no API token ever used):
 *   /        the pitch — static competitive/evidence content
 *   /demo    the Observatory on the synthetic "Meridian" fixture org
 *   /login   the gate itself
 *
 * PROTECTED (reads/writes the live org via the privileged server token):
 *   /console /reviews /disputes /graph /memories /ingest /analytics /keys
 *   /api/*   — the route handlers the client components call
 *
 * API routes get a 401, not a redirect: a fetch() should fail, not silently
 * receive an HTML login page and try to parse it as JSON.
 */

import { NextResponse, type NextRequest } from "next/server";

import { isPublicSurface } from "@/design/routes";
import { isUnlockedByDefault, isValidSession, SESSION_COOKIE } from "@/lib/auth";

// The allow-list itself lives in design/routes.ts (isPublicSurface) — ONE copy,
// shared with the operator chrome. The two used to hold separate lists and
// drifted twice: /kb was unreachable (exact-match here), then the operator
// chrome stacked on top of /demo (trailing-slash prefix there). Public surfaces
// render fixture data through the same components the operator sees, with
// `live: false` — every component degrades to synthesized detail and disables
// its write controls. No API token is ever used on those paths.

export async function middleware(req: NextRequest) {
  const { pathname } = req.nextUrl;

  if (isPublicSurface(pathname)) return NextResponse.next();

  // Development convenience only — never in production (see lib/auth.ts).
  if (isUnlockedByDefault()) return NextResponse.next();

  if (await isValidSession(req.cookies.get(SESSION_COOKIE)?.value)) {
    return NextResponse.next();
  }

  if (pathname.startsWith("/api/")) {
    return NextResponse.json(
      { error: "console session required", code: "unauthorized" },
      { status: 401 },
    );
  }

  const url = req.nextUrl.clone();
  url.pathname = "/login";
  url.search = "";
  url.searchParams.set("next", pathname);
  return NextResponse.redirect(url);
}

export const config = {
  // Everything except Next's own static output and image assets.
  matcher: [
    "/((?!_next/static|_next/image|favicon.ico|.*\\.(?:svg|png|jpg|jpeg|gif|webp|ico|txt)$).*)",
  ],
};
