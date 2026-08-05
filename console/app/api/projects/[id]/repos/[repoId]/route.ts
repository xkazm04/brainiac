import { NextResponse } from "next/server";

import { ApiError, configFromEnv, removeProjectRepo } from "@/lib/api";
import { isUuid } from "@/lib/scopes";

// Both params land in the upstream path, carried by the PRIVILEGED server
// token — see the sibling route for why an unvalidated segment is a way to aim
// this proxy somewhere else entirely. Both are uuids.
export async function DELETE(
  _req: Request,
  ctx: { params: Promise<{ id: string; repoId: string }> },
) {
  const { id, repoId } = await ctx.params;
  if (!isUuid(id) || !isUuid(repoId)) {
    return NextResponse.json({ error: "bad id" }, { status: 400 });
  }
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
