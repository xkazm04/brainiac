import { NextResponse } from "next/server";

import { addProjectRepo, ApiError, configFromEnv } from "@/lib/api";

export async function POST(req: Request, ctx: { params: Promise<{ id: string }> }) {
  const { id } = await ctx.params;
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
