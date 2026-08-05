import { NextResponse } from "next/server";

import { ApiError, configFromEnv, previewToken } from "@/lib/api";
import { assertOrgMember } from "@/lib/key-guard";
import { isUuid } from "@/lib/scopes";

// "Blast radius" preview for the mint form: what a key acting as this user would
// be able to see. Same authorization shape as the mint itself — the browser
// names a principal and this route asks the backend with the operator's token,
// so the id must be an actual member of the org. The preview echoes back an
// email and team names, which is precisely the kind of roster an unchecked uuid
// parameter invites a caller to enumerate.
export async function POST(req: Request) {
  let userId: unknown;
  try {
    ({ user_id: userId } = await req.json());
  } catch {
    return NextResponse.json({ error: "bad json" }, { status: 400 });
  }
  if (!isUuid(userId)) {
    return NextResponse.json({ error: "user_id required" }, { status: 400 });
  }
  const cfg = configFromEnv();
  const denied = await assertOrgMember(cfg, userId);
  if (denied) return NextResponse.json({ error: denied.error }, { status: denied.status });
  try {
    return NextResponse.json(await previewToken(cfg, userId));
  } catch (e) {
    const status = e instanceof ApiError ? e.status : 502;
    return NextResponse.json(
      { error: e instanceof Error ? e.message : "upstream unavailable" },
      { status },
    );
  }
}
