import { NextResponse } from "next/server";

import { ApiError, configFromEnv, previewToken } from "@/lib/api";

export async function POST(req: Request) {
  let userId: unknown;
  try {
    ({ user_id: userId } = await req.json());
  } catch {
    return NextResponse.json({ error: "bad json" }, { status: 400 });
  }
  if (typeof userId !== "string" || !/^[0-9a-f-]{36}$/i.test(userId)) {
    return NextResponse.json({ error: "user_id required" }, { status: 400 });
  }
  try {
    return NextResponse.json(await previewToken(configFromEnv(), userId));
  } catch (e) {
    const status = e instanceof ApiError ? e.status : 502;
    return NextResponse.json(
      { error: e instanceof Error ? e.message : "upstream unavailable" },
      { status },
    );
  }
}
