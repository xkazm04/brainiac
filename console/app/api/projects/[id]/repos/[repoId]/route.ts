import { NextResponse } from "next/server";

import { ApiError, configFromEnv, removeProjectRepo } from "@/lib/api";

export async function DELETE(
  _req: Request,
  ctx: { params: Promise<{ id: string; repoId: string }> },
) {
  const { id, repoId } = await ctx.params;
  try {
    await removeProjectRepo(configFromEnv(), id, repoId);
    return NextResponse.json({ id: repoId, removed: true });
  } catch (e) {
    const status = e instanceof ApiError ? e.status : 502;
    return NextResponse.json(
      { error: e instanceof Error ? e.message : "upstream unavailable" },
      { status },
    );
  }
}
