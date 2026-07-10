import DemoBanner from "@/components/DemoBanner";
import Observatory from "@/observatory/Observatory";
import {
  DEMO_OBSERVATORY,
  normalizeObservatory,
  type ObservatoryData,
} from "@/observatory/observatory-data";
import { configFromEnv, getObservatory } from "@/lib/api";

export const dynamic = "force-dynamic";

export const metadata = {
  title: "Brainiac — Observatory",
};

// Live data when brainiac serve is reachable; the demo shape (clearly
// labeled in the UI) when not, so the page never 500s.
async function observatoryData(): Promise<ObservatoryData> {
  try {
    return normalizeObservatory(await getObservatory(configFromEnv()));
  } catch {
    return DEMO_OBSERVATORY;
  }
}

export default async function AnalyticsPage() {
  const data = await observatoryData();
  return (
    <>
      {!data.live && <DemoBanner />}
      <Observatory data={data} />
    </>
  );
}
