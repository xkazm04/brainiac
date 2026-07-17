import { NextResponse } from "next/server";

import { ApiError, configFromEnv, createProject, listProjects } from "@/lib/api";

// Project registry proxies — the operator bearer token stays server-side.
export async function GET() {
  try {
    return NextResponse.json({ projects: await listProjects(configFromEnv()) });
  } catch (e) {
    const status = e instanceof ApiError ? e.status : 502;
    return NextResponse.json(
      { error: e instanceof Error ? e.message : "upstream unavailable" },
      { status },
    );
  }
}

export async function POST(req: Request) {
  let body: { name?: unknown };
  try {
    body = await req.json();
  } catch {
    return NextResponse.json({ error: "bad json" }, { status: 400 });
  }
  if (typeof body.name !== "string" || !body.name.trim()) {
    return NextResponse.json({ error: "name required" }, { status: 400 });
  }
  try {
    const created = await createProject(configFromEnv(), body.name.trim());
    return NextResponse.json(created, { status: 201 });
  } catch (e) {
    const status = e instanceof ApiError ? e.status : 502;
    return NextResponse.json(
      { error: e instanceof Error ? e.message : "upstream unavailable" },
      { status },
    );
  }
}
