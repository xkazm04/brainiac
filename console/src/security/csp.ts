/*
 * Baseline response security headers for every route.
 *
 * These are applied from `next.config.ts`'s `headers()` rather than the
 * middleware on purpose: the middleware's matcher deliberately skips Next's
 * static output, and a Content-Security-Policy that covers the HTML but not the
 * assets it loads is a policy with a hole in the shape of the thing you were
 * protecting. `headers()` covers every response the server emits.
 *
 * THE CSP IS DELIBERATELY NOT MAXIMAL. The rule this is written against: a
 * policy that breaks Google sign-in is worse than no policy, because the fix
 * under pressure is always to delete the whole header. So it is built from what
 * this console actually loads, and each allowance names why it is there. Tighten
 * it by removing an entry and watching the console, not by pasting a stricter
 * boilerplate.
 *
 * Known softness, stated rather than hidden:
 *
 *   script-src 'unsafe-inline' — the App Router streams hydration payloads as
 *   inline <script>self.__next_f.push(...)</script> tags. The alternative is a
 *   per-request nonce, which requires every page to opt out of static rendering;
 *   that is a real architectural change, not a header tweak. `strict-dynamic`
 *   would neuter the host allow-list without buying anything while inline stays
 *   permitted. So the XSS defence here is `object-src 'none'`, `base-uri
 *   'self'`, `frame-ancestors 'none'` and a closed `connect-src` — which is what
 *   actually bounds where a successful injection could send the org's data.
 *
 *   style-src 'unsafe-inline' — the codebase styles heavily with React `style={}`
 *   attributes (the whole keys/graph colour-band system). CSP3 covers those under
 *   style-src-attr and there is no nonce mechanism for attributes.
 */

/** Origins Firebase Auth's popup flow touches. Fixed Google infrastructure. */
const GOOGLE_AUTH_SCRIPTS = ["https://apis.google.com", "https://www.gstatic.com"];
const GOOGLE_AUTH_CONNECT = [
  "https://identitytoolkit.googleapis.com",
  "https://securetoken.googleapis.com",
  "https://www.googleapis.com",
];
const GOOGLE_AUTH_FRAMES = ["https://accounts.google.com"];

/**
 * The project's own auth domain (`<project>.firebaseapp.com`), which serves the
 * `/__/auth/handler` the popup lands on and the hidden auth-event iframe.
 *
 * The wildcards are the fallback that keeps sign-in working when the config is
 * absent at BUILD time — `headers()` is evaluated once during `next build`, so a
 * container built without NEXT_PUBLIC_FIREBASE_AUTH_DOMAIN would otherwise bake a
 * policy that blocks the very flow the launch depends on. Firebase only ever
 * serves auth handlers from these two apexes.
 */
function firebaseAuthOrigins(): string[] {
  const domain = process.env.NEXT_PUBLIC_FIREBASE_AUTH_DOMAIN?.trim();
  const wildcards = ["https://*.firebaseapp.com", "https://*.web.app"];
  return domain ? [`https://${domain}`, ...wildcards] : wildcards;
}

/**
 * The Sentry ingest origin, parsed out of the DSN so the policy names one host
 * instead of allowing all of sentry.io. Absent DSN = Sentry is off = nothing to
 * allow (see instrumentation-client.ts, which does not init without it).
 */
function sentryOrigins(): string[] {
  const dsn = process.env.NEXT_PUBLIC_SENTRY_DSN?.trim();
  if (!dsn) return [];
  try {
    return [new URL(dsn).origin];
  } catch {
    // A malformed DSN disables Sentry at runtime too; do not widen the policy
    // to compensate for a value that will not be used.
    return [];
  }
}

/** `next dev` compiles and hot-reloads in the browser; production must not. */
const isDev = process.env.NODE_ENV !== "production";

export function contentSecurityPolicy(): string {
  const firebase = firebaseAuthOrigins();
  const directives: Record<string, string[]> = {
    "default-src": ["'self'"],
    "script-src": [
      "'self'",
      "'unsafe-inline'",
      // React Refresh and the dev overlay evaluate compiled chunks.
      ...(isDev ? ["'unsafe-eval'"] : []),
      ...GOOGLE_AUTH_SCRIPTS,
    ],
    "style-src": ["'self'", "'unsafe-inline'"],
    "img-src": [
      "'self'",
      // data: for inline SVG/canvas exports; blob: for mermaid's rendered
      // diagrams (src/docs/MermaidBlock.tsx).
      "data:",
      "blob:",
      // Google account avatars come back on the sign-in profile.
      "https://lh3.googleusercontent.com",
    ],
    "font-src": ["'self'", "data:"],
    "connect-src": [
      "'self'",
      ...GOOGLE_AUTH_CONNECT,
      ...firebase,
      ...sentryOrigins(),
      // The dev server's HMR socket.
      ...(isDev ? ["ws:", "wss:"] : []),
    ],
    // signInWithPopup opens a window, but the SDK also mounts a hidden
    // auth-event iframe on the project's auth domain.
    "frame-src": ["'self'", ...firebase, ...GOOGLE_AUTH_FRAMES],
    "worker-src": ["'self'", "blob:"],
    "object-src": ["'none'"],
    "base-uri": ["'self'"],
    // The console has no third-party form targets; login/signup post to itself.
    "form-action": ["'self'"],
    // The modern half of X-Frame-Options. Both are sent: the header is still
    // what older proxies and scanners look for.
    "frame-ancestors": ["'none'"],
  };

  const serialized = Object.entries(directives)
    .map(([name, values]) => `${name} ${values.join(" ")}`)
    .join("; ");
  // Only meaningful over https and only in production, where every URL the
  // console emits should already be https.
  return isDev ? serialized : `${serialized}; upgrade-insecure-requests`;
}

export interface HeaderPair {
  key: string;
  value: string;
}

/**
 * The full baseline set.
 *
 * `CSP_REPORT_ONLY=1` swaps the policy onto the report-only header. It exists so
 * that the first person to hit a CSP break in production has a lever other than
 * ripping the whole thing out — flip it, read the violations, fix the directive,
 * flip it back. Everything else is unconditional.
 */
export function securityHeaders(): HeaderPair[] {
  const reportOnly = process.env.CSP_REPORT_ONLY === "1";
  const headers: HeaderPair[] = [
    {
      key: reportOnly
        ? "Content-Security-Policy-Report-Only"
        : "Content-Security-Policy",
      value: contentSecurityPolicy(),
    },
    // No MIME sniffing: a text/plain memory body must never be executed as
    // script because a byte pattern looked like one.
    { key: "X-Content-Type-Options", value: "nosniff" },
    // Belt to frame-ancestors' braces.
    { key: "X-Frame-Options", value: "DENY" },
    // Send the full URL only same-origin; an org's memory ids and search terms
    // live in these paths and must not ride along to a third party.
    { key: "Referrer-Policy", value: "strict-origin-when-cross-origin" },
    // Nothing here uses a camera, a microphone or a location.
    {
      key: "Permissions-Policy",
      value: "camera=(), microphone=(), geolocation=(), payment=(), usb=()",
    },
    // Isolate this origin's browsing-context group from anything it opens.
    { key: "Cross-Origin-Opener-Policy", value: "same-origin-allow-popups" },
  ];

  // HSTS is ignored over plain http, so sending it in dev is noise at best and a
  // foot-gun the day someone runs a local https proxy.
  if (!isDev) {
    headers.push({
      key: "Strict-Transport-Security",
      // Two years, subdomains included. No `preload`: that is a one-way door
      // onto a browser-vendor list and belongs to whoever owns the apex domain,
      // not to this config.
      value: "max-age=63072000; includeSubDomains",
    });
  }

  return headers;
}
