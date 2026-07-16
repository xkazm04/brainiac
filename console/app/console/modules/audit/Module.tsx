import DemoBanner from "@/components/DemoBanner";
import { configFromEnv } from "@/lib/api";
import { withDemoFallback } from "@/lib/demo-fallback";
import { auditTrail } from "@/lib/governance-api";

import AuditLedger from "./AuditLedger";
import { DEMO_AUDIT, PAGE, parseKind, parseOffset, type AuditData } from "./audit-data";

export const dynamic = "force-dynamic";

export const metadata = {
  title: "Brainiac — Audit",
};

/*
 * The audit module: "who approved this?" — the first question a corporate
 * auditor asks, and until now the one question the console could not answer.
 * The server has had a reverse-chronological governance feed
 * (crates/brainiac-server/src/console.rs `audit`) for a while; this is its
 * first caller (governance-api.ts `auditTrail`, previously dead code).
 *
 * Read, not write — so this is the ordinary withDemoFallback + DemoBanner
 * shape (unlike reviews, which hard-stops on a live-data requirement because
 * it can mutate). Filtering and paging are both server round trips: `kind`
 * and `offset` are query params the module re-fetches on, so AuditLedger's
 * controls are plain links rather than client state.
 */
export async function AuditModule({
  searchParams,
}: {
  searchParams: Record<string, string | string[] | undefined>;
}) {
  const kind = parseKind(searchParams.kind);
  const offset = parseOffset(searchParams.offset);
  const cfg = configFromEnv();

  const { data, live } = await withDemoFallback<AuditData>(
    async () => {
      const page = await auditTrail(cfg, { limit: PAGE, offset, kind });
      return { live: true, total: page.total, events: page.events };
    },
    DEMO_AUDIT,
  );

  return (
    <>
      {!live && <DemoBanner />}
      <AuditLedger data={data} kind={kind} offset={offset} />
    </>
  );
}

export default AuditModule;
