import { afterEach, describe, expect, it, vi } from "vitest";

import { contentSecurityPolicy, securityHeaders } from "./csp";

/**
 * These tests exist for one reason: the failure mode of a CSP is silent and
 * remote. Nothing in the build breaks when a directive is too tight — sign-in
 * just stops working for users, on a page nobody re-tested. So the directives
 * the auth flow depends on are asserted by name.
 */

function directive(policy: string, name: string): string[] {
  const found = policy
    .split(";")
    .map((d) => d.trim())
    .find((d) => d === name || d.startsWith(`${name} `));
  return found ? found.split(/\s+/).slice(1) : [];
}

afterEach(() => {
  vi.unstubAllEnvs();
});

describe("contentSecurityPolicy", () => {
  it("permits the origins Google sign-in needs", () => {
    const p = contentSecurityPolicy();
    expect(directive(p, "script-src")).toContain("https://apis.google.com");
    expect(directive(p, "connect-src")).toContain("https://identitytoolkit.googleapis.com");
    expect(directive(p, "connect-src")).toContain("https://securetoken.googleapis.com");
    expect(directive(p, "frame-src")).toContain("https://accounts.google.com");
  });

  it("falls back to the Firebase apex wildcards when the auth domain is unset at build", () => {
    // A container built without NEXT_PUBLIC_FIREBASE_AUTH_DOMAIN must still be
    // able to complete a sign-in; headers() is evaluated once, at build.
    vi.stubEnv("NEXT_PUBLIC_FIREBASE_AUTH_DOMAIN", "");
    const p = contentSecurityPolicy();
    expect(directive(p, "frame-src")).toContain("https://*.firebaseapp.com");
  });

  it("names the configured auth domain when it is present", () => {
    vi.stubEnv("NEXT_PUBLIC_FIREBASE_AUTH_DOMAIN", "brainiac-demo.firebaseapp.com");
    const p = contentSecurityPolicy();
    expect(directive(p, "frame-src")).toContain("https://brainiac-demo.firebaseapp.com");
    expect(directive(p, "connect-src")).toContain("https://brainiac-demo.firebaseapp.com");
  });

  it("allows exactly the Sentry ingest origin, not all of sentry.io", () => {
    vi.stubEnv("NEXT_PUBLIC_SENTRY_DSN", "https://abc123@o42.ingest.sentry.io/7");
    expect(directive(contentSecurityPolicy(), "connect-src")).toContain(
      "https://o42.ingest.sentry.io",
    );
  });

  it("does not widen connect-src for a malformed DSN", () => {
    vi.stubEnv("NEXT_PUBLIC_SENTRY_DSN", "not-a-url");
    expect(
      directive(contentSecurityPolicy(), "connect-src").some((s) => s.includes("sentry")),
    ).toBe(false);
  });

  it("does not emit upgrade-insecure-requests", () => {
    // Not an oversight — see the block comment in csp.ts. It has nothing to fix
    // here (no http:// subresources anywhere) and it was observed killing Next's
    // same-origin /login prefetch with ERR_SSL_PROTOCOL_ERROR on the plain-HTTP
    // deployment posture the stack still supports. HSTS carries the intent.
    expect(contentSecurityPolicy()).not.toContain("upgrade-insecure-requests");
  });

  it("keeps the directives that actually bound an injection", () => {
    const p = contentSecurityPolicy();
    expect(directive(p, "object-src")).toEqual(["'none'"]);
    expect(directive(p, "base-uri")).toEqual(["'self'"]);
    expect(directive(p, "frame-ancestors")).toEqual(["'none'"]);
    expect(directive(p, "form-action")).toEqual(["'self'"]);
  });
});

describe("securityHeaders", () => {
  it("ships the baseline set", () => {
    const keys = securityHeaders().map((h) => h.key);
    expect(keys).toContain("Content-Security-Policy");
    expect(keys).toContain("X-Content-Type-Options");
    expect(keys).toContain("X-Frame-Options");
    expect(keys).toContain("Referrer-Policy");
    expect(keys).toContain("Permissions-Policy");
  });

  it("keeps the popup channel open for signInWithPopup", () => {
    // COOP: same-origin would sever window.opener and break the Google popup.
    const coop = securityHeaders().find((h) => h.key === "Cross-Origin-Opener-Policy");
    expect(coop?.value).toBe("same-origin-allow-popups");
  });

  it("CSP_REPORT_ONLY moves the policy to the report-only header", () => {
    vi.stubEnv("CSP_REPORT_ONLY", "1");
    const keys = securityHeaders().map((h) => h.key);
    expect(keys).toContain("Content-Security-Policy-Report-Only");
    expect(keys).not.toContain("Content-Security-Policy");
  });
});
