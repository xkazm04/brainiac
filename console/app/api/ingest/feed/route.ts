import { NextResponse } from "next/server";

import {
  configFromEnv,
  getPipelineRuns,
  getQueueHealth,
  getSourcesFeed,
} from "@/lib/api";

// Combined monitor feed for client polling — one round trip, token
// server-side.
export async function GET() {
  try {
    const cfg = configFromEnv();
    const [sources, runs, health] = await Promise.all([
      getSourcesFeed(cfg, 30),
      getPipelineRuns(cfg, 40),
      getQueueHealth(cfg),
    ]);
    return NextResponse.json({ sources, runs, health });
  } catch (e) {
    return NextResponse.json(
      { error: e instanceof Error ? e.message : "upstream unavailable" },
      { status: 502 },
    );
  }
}
