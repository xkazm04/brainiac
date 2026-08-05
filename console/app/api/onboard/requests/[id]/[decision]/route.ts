import { NextResponse } from "next/server";

import { ApiError, configFromEnv, decideOnboardRequest } from "@/lib/api";
import { isUuid } from "@/lib/scopes";

// Approve/deny a pairing. The decision segment is validated here so the proxy
// can never be steered to an arbitrary upstream path — and so is `id`, which
// the comment above used to imply was covered and was not. Both segments are
// interpolated into the upstream URL and sent with the PRIVILEGED server token;
// pinning only the last one still leaves the middle of the path caller-chosen.
export async function POST(
  _req: Request,
  ctx: { params: Promise<{ id: string; decision: string }> },
) {
  const { id, decision } = await ctx.params;
  if (decision !== "approve" && decision !== "deny") {
    return NextResponse.json({ error: "unknown decision" }, { status: 404 });
  }
  if (!isUuid(id)) {
    return NextResponse.json({ error: "bad id" }, { status: 400 });
  }
  try {
    const out = await decideOnboardRequest(configFromEnv(), id, decision);
    return NextResponse.json(out);
  } catch (e) {
    const status = e instanceof ApiError ? e.status : 502;
    return NextResponse.json(
      { error: e instanceof Error ? e.message : "upstream unavailable" },
      { status },
    );
  }
}
