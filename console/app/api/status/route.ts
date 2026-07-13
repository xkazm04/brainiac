// Connection + queue status for the nav (client-poll safe: the bearer
// token never leaves the server; the browser only sees the counts).

import { NextResponse } from "next/server";

import { configFromEnv, getAnalytics } from "@/lib/api";

export const dynamic = "force-dynamic";

export interface NavStatusPayload {
  live: boolean;
  pending: number;
  contradictions: number;
  /** Memories with unresolved wrong/outdated reports from their readers. */
  flagged: number;
  queueDepth: number;
}

const OFFLINE: NavStatusPayload = {
  live: false,
  pending: 0,
  contradictions: 0,
  flagged: 0,
  queueDepth: 0,
};

export async function GET() {
  try {
    // flagged_memories is newer than the hand-written Analytics mirror in
    // types.ts; read it structurally rather than widening that shared type.
    const a = (await getAnalytics(configFromEnv())) as Awaited<
      ReturnType<typeof getAnalytics>
    > & { reviews: { flagged_memories?: number } };
    const payload: NavStatusPayload = {
      live: true,
      pending: a.reviews.pending_promotions,
      contradictions: a.reviews.open_contradictions,
      flagged: a.reviews.flagged_memories ?? 0,
      queueDepth: a.queue.ingest_depth,
    };
    return NextResponse.json(payload);
  } catch {
    return NextResponse.json(OFFLINE);
  }
}
