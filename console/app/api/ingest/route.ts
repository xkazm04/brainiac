import { NextResponse } from "next/server";

import { ApiError, configFromEnv, submitMemory } from "@/lib/api";

// Submit a manual memory into the pipeline (token server-side).
export async function POST(req: Request) {
  let content: unknown;
  try {
    ({ content } = await req.json());
  } catch {
    return NextResponse.json({ error: "bad json" }, { status: 400 });
  }
  if (typeof content !== "string" || !content.trim() || content.length > 4000) {
    return NextResponse.json({ error: "content must be 1–4000 chars" }, { status: 400 });
  }
  try {
    return NextResponse.json(await submitMemory(configFromEnv(), content.trim()), {
      status: 202,
    });
  } catch (e) {
    const status = e instanceof ApiError ? e.status : 502;
    return NextResponse.json(
      { error: e instanceof Error ? e.message : "upstream unavailable" },
      { status },
    );
  }
}
