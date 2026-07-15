import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { isValidSession, SESSION_MAX_AGE, sessionToken } from "./auth";

const PASSCODE = "correct-horse-battery-staple";

describe("console session cookie", () => {
  const realEnv = process.env;

  beforeEach(() => {
    process.env = { ...realEnv, CONSOLE_PASSCODE: PASSCODE, NODE_ENV: "test" };
  });
  afterEach(() => {
    process.env = realEnv;
    vi.useRealTimers();
  });

  it("round-trips a freshly minted token", async () => {
    expect(await isValidSession(await sessionToken(PASSCODE))).toBe(true);
  });

  it("mints a distinct value per session", async () => {
    // The old cookie was sha256(prefix + passcode): identical for every user and
    // every session, so a single leaked value authenticated forever.
    const a = await sessionToken(PASSCODE);
    const b = await sessionToken(PASSCODE);
    expect(a).not.toEqual(b);
    expect(await isValidSession(a)).toBe(true);
    expect(await isValidSession(b)).toBe(true);
  });

  it("rejects a tampered issued-at (no free lifetime extension)", async () => {
    const [iat, nonce, fp, sig] = (await sessionToken(PASSCODE)).split(".");
    const forged = `${Number(iat) + 999_999}.${nonce}.${fp}.${sig}`;
    expect(await isValidSession(forged)).toBe(false);
  });

  it("expires server-side, not just as a browser attribute", async () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-01-01T00:00:00Z"));
    const t = await sessionToken(PASSCODE);
    expect(await isValidSession(t)).toBe(true);
    // SESSION_MAX_AGE past issue: a copied cookie must stop working.
    vi.setSystemTime(new Date(Date.now() + (SESSION_MAX_AGE + 60) * 1000));
    expect(await isValidSession(t)).toBe(false);
  });

  it("a passcode rotation invalidates outstanding sessions", async () => {
    const t = await sessionToken(PASSCODE);
    expect(await isValidSession(t)).toBe(true);
    process.env.CONSOLE_PASSCODE = "an-entirely-different-passcode";
    expect(await isValidSession(t)).toBe(false);
  });

  it("rejects malformed and legacy bare-digest cookies", async () => {
    expect(await isValidSession(undefined)).toBe(false);
    expect(await isValidSession("")).toBe(false);
    // The pre-HMAC format: a single 64-char sha256 hex, no payload, no signature.
    expect(await isValidSession("a".repeat(64))).toBe(false);
    expect(await isValidSession("1.2.3")).toBe(false);
    expect(await isValidSession("1.2.3.4.5")).toBe(false);
  });

  it("is locked when no passcode is configured", async () => {
    const t = await sessionToken(PASSCODE);
    delete process.env.CONSOLE_PASSCODE;
    expect(await isValidSession(t)).toBe(false);
  });

  it("a session secret keys the signature independently of the passcode", async () => {
    // With CONSOLE_SESSION_SECRET set, a cookie cannot be derived from the
    // passcode alone — so guessing the passcode offline cannot mint a cookie and
    // skip the login throttle.
    process.env.CONSOLE_SESSION_SECRET = "server-side-signing-secret";
    const withSecret = await sessionToken(PASSCODE);
    expect(await isValidSession(withSecret)).toBe(true);
    // The same cookie must not verify once the signing secret changes.
    process.env.CONSOLE_SESSION_SECRET = "rotated-signing-secret";
    expect(await isValidSession(withSecret)).toBe(false);
  });
});
