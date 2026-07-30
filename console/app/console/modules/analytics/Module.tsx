import DemoBanner from "@/components/DemoBanner";
import Observatory from "@/observatory/Observatory";
import {
  DEMO_OBSERVATORY,
  normalizeDemand,
  normalizeObservatory,
  type ObservatoryData,
} from "@/observatory/observatory-data";
import { configFromEnv, getAnalyticsValue, getObservatory } from "@/lib/api";
import { withDemoFallback } from "@/lib/demo-fallback";

export const dynamic = "force-dynamic";

export const metadata = {
  title: "Brainiac — Observatory",
};

// Live data when brainiac serve is reachable; the demo shape (behind an
// unconditional DemoBanner) when not, so the page never 500s.
export default async function AnalyticsPage() {
  const { data, live } = await withDemoFallback<ObservatoryData>(
    async () => normalizeObservatory(await getObservatory(configFromEnv())),
    DEMO_OBSERVATORY,
  );

  // The demand side (migration 0043) is fetched separately and allowed to fail
  // on its own: it reads a table an older deployment may not have migrated yet,
  // and a failure there must not take down an Observatory that worked before
  // this feature existed. On failure the tile is omitted rather than
  // zero-filled — inventing "0% answered" is worse than saying nothing.
  let demand: ObservatoryData["demand"];
  if (live) {
    try {
      demand = normalizeDemand(await getAnalyticsValue(configFromEnv()));
    } catch (e) {
      console.error("demand analytics unavailable", e);
    }
  }

  return (
    <>
      {!live && <DemoBanner />}
      <Observatory data={live ? { ...data, demand } : data} />
    </>
  );
}
