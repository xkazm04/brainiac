import { NextResponse } from "next/server";

import { ApiError, configFromEnv, createToken, listTokens } from "@/lib/api";

// Token management proxies — the operator bearer token stays server-side.
// The minted secret passes through this response exactly once and is never
// logged or stored here.
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
  let body: { name?: unknown; user_id?: unknown; scopes?: unknown };
  try {
    body = await req.json();
  } catch {
    return NextResponse.json({ error: "bad json" }, { status: 400 });
  }
  if (typeof body.name !== "string" || !body.name.trim()) {
    return NextResponse.json({ error: "name required" }, { status: 400 });
  }
  try {
    const minted = await createToken(
      configFromEnv(),
      body.name.trim(),
      typeof body.user_id === "string" ? body.user_id : undefined,
      Array.isArray(body.scopes) ? (body.scopes as string[]) : undefined,
    );
    return NextResponse.json(minted, { status: 201 });
  } catch (e) {
    const status = e instanceof ApiError ? e.status : 502;
    return NextResponse.json(
      { error: e instanceof Error ? e.message : "upstream unavailable" },
      { status },
    );
  }
}
