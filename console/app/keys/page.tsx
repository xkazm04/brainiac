import { configFromEnv, getOrgUsers, listTokens } from "@/lib/api";

import { DEMO_KEYS, type KeysData } from "./keys-data";
import KeysLab from "./KeysLab";

export const dynamic = "force-dynamic";

export const metadata = {
  title: "Brainiac — Keys",
};

async function keysData(): Promise<KeysData> {
  try {
    const cfg = configFromEnv();
    const [tokens, users] = await Promise.all([listTokens(cfg), getOrgUsers(cfg)]);
    return { live: true, tokens, users };
  } catch {
    return DEMO_KEYS;
  }
}

export default async function KeysPage() {
  return <KeysLab data={await keysData()} />;
}
