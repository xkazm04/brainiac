import { NextResponse } from "next/server";

import { ApiError, configFromEnv, getGraphCanonical } from "@/lib/api";
import { isUuid } from "@/lib/scopes";

// Client-side drill-down proxy: the browser never sees the bearer token.
export async function GET(
  _req: Request,
  { params }: { params: Promise<{ id: string }> },
) {
  const { id } = await params;
  if (!isUuid(id)) {
    return NextResponse.json({ error: "bad id" }, { status: 400 });
  }
  try {
    return NextResponse.json(await getGraphCanonical(configFromEnv(), id));
  } catch (e) {
    const status = e instanceof ApiError ? e.status : 502;
    return NextResponse.json(
      { error: e instanceof Error ? e.message : "upstream unavailable" },
      { status },
    );
  }
}
