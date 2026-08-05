import { NextResponse } from "next/server";

import { addProjectRepo, ApiError, configFromEnv } from "@/lib/api";
import { isUuid } from "@/lib/scopes";

// `id` is interpolated into the upstream path and sent with the PRIVILEGED
// server token, so its shape is not cosmetic: a param of `../memories` (which a
// caller spells `%2E%2E%2Fmemories`) survives Next's decoding and then gets
// normalized away by fetch, pointing this proxy at an upstream endpoint the
// console never meant to expose. Project ids are uuids; nothing else is a
// project id.
export async function POST(req: Request, ctx: { params: Promise<{ id: string }> }) {
  const { id } = await ctx.params;
  if (!isUuid(id)) {
    return NextResponse.json({ error: "bad id" }, { status: 400 });
  }
  let body: { remote?: unknown };
  try {
    body = await req.json();
  } catch {
    return NextResponse.json({ error: "bad json" }, { status: 400 });
  }
  if (typeof body.remote !== "string" || !body.remote.trim()) {
    return NextResponse.json({ error: "remote required" }, { status: 400 });
  }
  try {
    const added = await addProjectRepo(configFromEnv(), id, body.remote.trim());
    return NextResponse.json(added, { status: 201 });
  } catch (e) {
    const status = e instanceof ApiError ? e.status : 502;
    return NextResponse.json(
      { error: e instanceof Error ? e.message : "upstream unavailable" },
      { status },
    );
  }
}
