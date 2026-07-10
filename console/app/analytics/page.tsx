import { configFromEnv, getObservatory } from "@/lib/api";

import ObservatoryLab from "./ObservatoryLab";
import {
  DEMO_OBSERVATORY,
  normalizeObservatory,
  type ObservatoryData,
} from "./observatory-data";

export const dynamic = "force-dynamic";

export const metadata = {
  title: "Brainiac — Observatory",
};

// Prototype round: live data when brainiac serve is up, demo shape when not,
// so all three variants stay comparable either way.
async function observatoryData(): Promise<ObservatoryData> {
  try {
    return normalizeObservatory(await getObservatory(configFromEnv()));
  } catch {
    return DEMO_OBSERVATORY;
  }
}

export default async function AnalyticsPage() {
  return <ObservatoryLab data={await observatoryData()} />;
}
