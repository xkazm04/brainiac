import DemoBanner from "@/components/DemoBanner";
import Observatory from "@/observatory/Observatory";
import {
  DEMO_OBSERVATORY,
  normalizeObservatory,
  type ObservatoryData,
} from "@/observatory/observatory-data";
import { configFromEnv, getObservatory } from "@/lib/api";
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
  return (
    <>
      {!live && <DemoBanner />}
      <Observatory data={data} />
    </>
  );
}
