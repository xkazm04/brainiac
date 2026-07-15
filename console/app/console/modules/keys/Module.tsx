import DemoBanner from "@/components/DemoBanner";
import { configFromEnv, getOrgUsers, listTokens } from "@/lib/api";
import { withDemoFallback } from "@/lib/demo-fallback";

import { DEMO_KEYS, type KeysData } from "./keys-data";
import Keys from "./Keys";

export const dynamic = "force-dynamic";

export const metadata = {
  title: "Brainiac — Keys",
};

// Live tokens + org users when reachable; demo shape (behind an unconditional
// DemoBanner — these are fabricated tokens) when not.
export default async function KeysPage() {
  const { data, live } = await withDemoFallback<KeysData>(async () => {
    const cfg = configFromEnv();
    const [tokens, users] = await Promise.all([listTokens(cfg), getOrgUsers(cfg)]);
    return { live: true, tokens, users };
  }, DEMO_KEYS);
  return (
    <>
      {!live && <DemoBanner />}
      <Keys data={data} />
    </>
  );
}
