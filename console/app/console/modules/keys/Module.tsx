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
  // What developers should point their machines at. The console's own
  // BRAINIAC_API_URL may be a private address (docker service name), so a
  // deployment can override the developer-facing one explicitly.
  const apiUrl =
    process.env.BRAINIAC_PUBLIC_API_URL ??
    process.env.BRAINIAC_API_URL ??
    "http://127.0.0.1:8600";
  const { data, live } = await withDemoFallback<KeysData>(async () => {
    const cfg = configFromEnv();
    const [tokens, users] = await Promise.all([listTokens(cfg), getOrgUsers(cfg)]);
    return { live: true, tokens, users, apiUrl };
  }, DEMO_KEYS);
  return (
    <>
      {!live && <DemoBanner />}
      <Keys data={data} />
    </>
  );
}
