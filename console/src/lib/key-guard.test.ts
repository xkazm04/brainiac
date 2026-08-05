import { describe, expect, it } from "vitest";

import { assertOrgMember, isGuardFailure, parseMintRequest } from "./key-guard";
import { SCOPES } from "./scopes";

const UUID_A = "11111111-2222-4333-8444-555555555555";
const UUID_B = "99999999-8888-4777-8666-555555555555";

function cfgWithUsers(users: unknown, ok = true) {
  return {
    baseUrl: "http://x",
    token: "t",
    fetchImpl: (async () =>
      new Response(JSON.stringify({ users }), {
        status: ok ? 200 : 500,
        headers: { "content-type": "application/json" },
      })) as unknown as typeof fetch,
  };
}

describe("parseMintRequest", () => {
  it("accepts a well-formed request and de-duplicates scopes", () => {
    const out = parseMintRequest({
      name: "  ci-ingest  ",
      user_id: UUID_A,
      scopes: ["read", "write", "read"],
    });
    expect(isGuardFailure(out)).toBe(false);
    if (isGuardFailure(out)) return;
    expect(out.name).toBe("ci-ingest");
    expect(out.userId).toBe(UUID_A);
    expect(out.scopes).toEqual(["read", "write"]);
  });

  it("defaults to read rather than leaving the grant implicit", () => {
    const out = parseMintRequest({ name: "k" });
    if (isGuardFailure(out)) throw new Error("expected success");
    expect(out.scopes).toEqual(["read"]);
    expect(out.userId).toBeUndefined();
  });

  it("rejects a scope outside the vocabulary", () => {
    // The whole point: "superuser" is not in auth::SCOPES, and the console must
    // not relay a request for authority it never offered.
    const out = parseMintRequest({ name: "k", scopes: ["read", "superuser"] });
    expect(isGuardFailure(out) && out.status).toBe(400);
  });

  it("rejects an empty scope list", () => {
    expect(isGuardFailure(parseMintRequest({ name: "k", scopes: [] }))).toBe(true);
  });

  it("accepts every scope the chip row offers", () => {
    const out = parseMintRequest({ name: "k", scopes: [...SCOPES] });
    expect(isGuardFailure(out)).toBe(false);
  });

  it("rejects a user_id that is not a uuid", () => {
    // The old regex was /^[0-9a-f-]{36}$/i, which "------------------------------------"
    // satisfies. A non-uuid can never match an org member, but it should be
    // refused at the shape check rather than one round trip later.
    const out = parseMintRequest({ name: "k", user_id: "-".repeat(36) });
    expect(isGuardFailure(out) && out.status).toBe(400);
  });

  it("rejects a missing or blank name", () => {
    expect(isGuardFailure(parseMintRequest({ name: "   " }))).toBe(true);
    expect(isGuardFailure(parseMintRequest({}))).toBe(true);
    expect(isGuardFailure(parseMintRequest(null))).toBe(true);
  });
});

describe("assertOrgMember", () => {
  it("passes a real member", async () => {
    const cfg = cfgWithUsers([{ id: UUID_A, email: "a@x", teams: [] }]);
    expect(await assertOrgMember(cfg, UUID_A)).toBeNull();
  });

  it("denies an identity nobody in the org holds", async () => {
    const cfg = cfgWithUsers([{ id: UUID_A, email: "a@x", teams: [] }]);
    const denied = await assertOrgMember(cfg, UUID_B);
    expect(denied?.status).toBe(403);
  });

  it("fails closed when the roster cannot be read", async () => {
    // An upstream outage must not degrade into "skip the check".
    const cfg = cfgWithUsers([], false);
    const denied = await assertOrgMember(cfg, UUID_A);
    expect(denied?.status).toBe(502);
  });
});
