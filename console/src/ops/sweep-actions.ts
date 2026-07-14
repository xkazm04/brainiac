"use server";

/*
 * Server actions for the sweep-control panel. The bearer token lives only in
 * the server process (configFromEnv), so a client component can drive the
 * admin-scoped /v1/ops/sweeps endpoints without the token ever reaching the
 * browser. Each action revalidates the page it was invoked from so the panel's
 * status reflects the change on the next paint.
 */

import { revalidatePath } from "next/cache";

import { configFromEnv, runSweep, updateSweep } from "@/lib/api";

export async function updateSweepAction(
  kind: string,
  patch: { enabled?: boolean; cadence_secs?: number },
  revalidate: string,
): Promise<void> {
  await updateSweep(configFromEnv(), kind, patch);
  revalidatePath(revalidate);
}

export async function runSweepAction(kind: string, revalidate: string): Promise<void> {
  await runSweep(configFromEnv(), kind);
  revalidatePath(revalidate);
}
