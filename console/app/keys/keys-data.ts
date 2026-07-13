// Shared substrate for the Keys variants: demo shapes and the client
// mutation helpers. Ground band (0 Hz) — see theme.ts GROUND.

import type { ApiToken, MintedToken, OrgUser, TokenPreview } from "@/lib/types";

export interface KeysData {
  live: boolean;
  tokens: ApiToken[];
  users: OrgUser[];
}

export async function mintKey(
  name: string,
  userId: string | undefined,
  scopes: string[],
): Promise<MintedToken> {
  const r = await fetch("/api/keys", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ name, user_id: userId, scopes }),
  });
  if (!r.ok) throw new Error((await r.json().catch(() => null))?.error ?? String(r.status));
  return r.json();
}

export async function revokeKey(id: string): Promise<void> {
  const r = await fetch(`/api/keys/${id}/revoke`, { method: "POST" });
  if (!r.ok) throw new Error(String(r.status));
}

export async function fetchPreview(userId: string): Promise<TokenPreview> {
  const r = await fetch("/api/keys/preview", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ user_id: userId }),
  });
  if (!r.ok) throw new Error(String(r.status));
  return r.json();
}

export async function refreshTokens(): Promise<ApiToken[]> {
  const r = await fetch("/api/keys");
  if (!r.ok) throw new Error(String(r.status));
  return (await r.json()).tokens;
}

export const SCOPES = ["read", "write", "admin"] as const;

// ── demo shapes ─────────────────────────────────────────────────────────

const DAY = 86400000;

export const DEMO_KEYS: KeysData = {
  live: false,
  tokens: [
    {
      id: "dk-1",
      name: "claude-code · pay-dev1",
      prefix: "brk_4f2a9c1e…",
      scopes: ["read"],
      created_at: new Date(Date.now() - 21 * DAY).toISOString(),
      last_used_at: new Date(Date.now() - 3600000).toISOString(),
      revoked_at: null,
    },
    {
      id: "dk-2",
      name: "ci-ingest · platform",
      prefix: "brk_b81c3d77…",
      scopes: ["read", "write"],
      created_at: new Date(Date.now() - 14 * DAY).toISOString(),
      last_used_at: new Date(Date.now() - 2 * DAY).toISOString(),
      revoked_at: null,
    },
    {
      id: "dk-3",
      name: "mcp-agent · analyst1",
      prefix: "brk_9e01ffa2…",
      scopes: ["read"],
      created_at: new Date(Date.now() - 7 * DAY).toISOString(),
      last_used_at: null,
      revoked_at: null,
    },
    {
      id: "dk-4",
      name: "old laptop",
      prefix: "brk_77aa02c4…",
      scopes: ["read", "write", "admin"],
      created_at: new Date(Date.now() - 60 * DAY).toISOString(),
      last_used_at: new Date(Date.now() - 30 * DAY).toISOString(),
      revoked_at: new Date(Date.now() - 9 * DAY).toISOString(),
    },
  ],
  users: [
    { id: "du-1", email: "dev1@meridian.example", teams: [{ id: "t1", name: "payments", role: "member" }] },
    { id: "du-2", email: "paylead@meridian.example", teams: [{ id: "t1", name: "payments", role: "maintainer" }] },
    { id: "du-3", email: "analyst1@meridian.example", teams: [{ id: "t3", name: "data", role: "member" }] },
    { id: "du-4", email: "platdev@meridian.example", teams: [{ id: "t2", name: "platform", role: "member" }] },
  ],
};

export function demoPreview(userId: string, users: OrgUser[]): TokenPreview {
  const u = users.find((x) => x.id === userId) ?? users[0];
  const teamShare = 14 + (u.email.length % 7);
  return {
    user_id: u.id,
    email: u.email,
    teams: u.teams.map((t) => t.name),
    visible: { total: 27 + teamShare + 1, org: 27, team: teamShare, private: 1, canonical: 24 + (u.email.length % 5) },
  };
}
