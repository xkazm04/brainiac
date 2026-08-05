import { NextResponse } from "next/server";

import { ApiError, configFromEnv, createToken, listTokens } from "@/lib/api";
import { assertOrgMember, isGuardFailure, parseMintRequest } from "@/lib/key-guard";

// Token management proxies — the operator bearer token stays server-side.
// The minted secret passes through this response exactly once and is never
// logged or stored here.
//
// AUTHORIZATION. The middleware proves the caller holds the console passcode and
// nothing else; this route then acts with the deployment's privileged token, so
// every field it forwards on the browser's say-so is authority the browser gets
// to aim. `user_id` and `scopes` used to be forwarded verbatim — see
// lib/key-guard.ts for what that meant and what is enforced now.
export async function GET() {
  try {
    return NextResponse.json({ tokens: await listTokens(configFromEnv()) });
  } catch (e) {
    const status = e instanceof ApiError ? e.status : 502;
    return NextResponse.json(
      { error: e instanceof Error ? e.message : "upstream unavailable" },
      { status },
    );
  }
}

export async function POST(req: Request) {
  let body: unknown;
  try {
    body = await req.json();
  } catch {
    return NextResponse.json({ error: "bad json" }, { status: 400 });
  }

  const parsed = parseMintRequest(body);
  if (isGuardFailure(parsed)) {
    return NextResponse.json({ error: parsed.error }, { status: parsed.status });
  }

  const cfg = configFromEnv();
  // Only when the request names an identity. Omitting `user_id` mints for the
  // calling principal, which the backend derives from its own token — nothing
  // the browser can influence, so there is nothing to check.
  if (parsed.userId) {
    const denied = await assertOrgMember(cfg, parsed.userId);
    if (denied) return NextResponse.json({ error: denied.error }, { status: denied.status });
  }

  try {
    const minted = await createToken(cfg, parsed.name, parsed.userId, parsed.scopes);
    return NextResponse.json(minted, { status: 201 });
  } catch (e) {
    const status = e instanceof ApiError ? e.status : 502;
    return NextResponse.json(
      { error: e instanceof Error ? e.message : "upstream unavailable" },
      { status },
    );
  }
}
