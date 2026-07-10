// Connection + queue status for the nav (client-poll safe: the bearer
// token never leaves the server; the browser only sees the counts).

import { NextResponse } from "next/server";

import { configFromEnv, getAnalytics } from "@/lib/api";

export const dynamic = "force-dynamic";

export interface NavStatusPayload {
  live: boolean;
  pending: number;
  contradictions: number;
  queueDepth: number;
}

export async function GET() {
  try {
    const a = await getAnalytics(configFromEnv());
    const payload: NavStatusPayload = {
      live: true,
      pending: a.reviews.pending_promotions,
      contradictions: a.reviews.open_contradictions,
      queueDepth: a.queue.ingest_depth,
    };
    return NextResponse.json(payload);
  } catch {
    const payload: NavStatusPayload = {
      live: false,
      pending: 0,
      contradictions: 0,
      queueDepth: 0,
    };
    return NextResponse.json(payload);
  }
}
