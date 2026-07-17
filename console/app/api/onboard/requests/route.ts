import { NextResponse } from "next/server";

import { ApiError, configFromEnv, listOnboardRequests } from "@/lib/api";

// The approval queue for developer onboarding pairings (admin proxy).
export async function GET() {
  try {
    return NextResponse.json({ requests: await listOnboardRequests(configFromEnv()) });
  } catch (e) {
    const status = e instanceof ApiError ? e.status : 502;
    return NextResponse.json(
      { error: e instanceof Error ? e.message : "upstream unavailable" },
      { status },
    );
  }
}
