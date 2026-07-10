import { NextResponse } from "next/server";

import { ApiError, configFromEnv, getMemoryDetail } from "@/lib/api";

// Client-side detail proxy: the browser never sees the bearer token.
export async function GET(
  _req: Request,
  { params }: { params: Promise<{ id: string }> },
) {
  const { id } = await params;
  if (!/^[0-9a-f-]{36}$/i.test(id)) {
    return NextResponse.json({ error: "bad id" }, { status: 400 });
  }
  try {
    return NextResponse.json(await getMemoryDetail(configFromEnv(), id));
  } catch (e) {
    const status = e instanceof ApiError ? e.status : 502;
    return NextResponse.json(
      { error: e instanceof Error ? e.message : "upstream unavailable" },
      { status },
    );
  }
}
