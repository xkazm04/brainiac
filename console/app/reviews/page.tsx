import ApiOffline from "@/components/ApiOffline";
import { configFromEnv } from "@/lib/api";
import {
  contradictionQueue,
  formatAge,
  promotionQueue,
  type ContradictionStatus,
} from "@/lib/governance-api";
import Link from "next/link";

import { ContradictionButtons, PromotionButtons } from "./review-buttons";

export const dynamic = "force-dynamic";

const STATUS_TABS: { key: ContradictionStatus; label: string }[] = [
  { key: "open", label: "open" },
  { key: "resolved_supersede", label: "superseded" },
  { key: "resolved_coexist", label: "coexist" },
  { key: "dismissed", label: "dismissed" },
  { key: "all", label: "all" },
];

function asStatus(v: string | string[] | undefined): ContradictionStatus {
  const s = Array.isArray(v) ? v[0] : v;
  return STATUS_TABS.some((t) => t.key === s) ? (s as ContradictionStatus) : "open";
}

export default async function ReviewsPage({
  searchParams,
}: {
  searchParams: Promise<Record<string, string | string[] | undefined>>;
}) {
  const params = await searchParams;
  const cstatus = asStatus(params.cstatus);
  const cfg = configFromEnv();
  let promotions, contradictionsPage;
  try {
    [promotions, contradictionsPage] = await Promise.all([
      promotionQueue(cfg),
      contradictionQueue(cfg, { status: cstatus }),
    ]);
  } catch (e) {
    return <ApiOffline error={e instanceof Error ? e.message : String(e)} />;
  }
  const { contradictions, counts } = contradictionsPage;
  const countOf = (key: ContradictionStatus) =>
    key === "all"
      ? counts.reduce((a, c) => a + c.count, 0)
      : (counts.find((c) => c.status === key)?.count ?? 0);

  return (
    <>
      <section aria-labelledby="promotions-h">
        <h1 id="promotions-h">Pending promotions ({promotions.length})</h1>
        {promotions.length === 0 ? (
          <p>Queue is clear.</p>
        ) : (
          <ul>
            {promotions.map((p) => (
              <li key={p.id}>
                {p.memory ? (
                  <blockquote>{p.memory.content}</blockquote>
                ) : (
                  <p>
                    <em>(memory not visible to you)</em> <code>{p.memory_id}</code>
                  </p>
                )}
                <p>
                  <small>
                    {p.memory?.kind && <>{p.memory.kind} · </>}
                    {p.from_status} → <strong>{p.to_status}</strong>
                    {p.memory?.team && <> · team {p.memory.team}</>}
                    {p.memory?.confidence != null && (
                      <> · confidence {p.memory.confidence.toFixed(2)}</>
                    )}
                    {" · waiting "}
                    {formatAge(p.age_secs)}
                    {p.policy_rule && <> · rule: {p.policy_rule}</>}
                  </small>
                </p>
                {p.provenance && (
                  <p>
                    <small>
                      via {p.provenance.actor_kind} {p.provenance.actor_id}
                      {p.provenance.model_ref && <> ({p.provenance.model_ref})</>}
                      {p.provenance.source_kind && (
                        <>
                          {" "}
                          from {p.provenance.source_kind}
                          {p.provenance.source_ref && <>: {p.provenance.source_ref}</>}
                        </>
                      )}
                    </small>
                  </p>
                )}
                <PromotionButtons promotionId={p.id} />
              </li>
            ))}
          </ul>
        )}
      </section>

      <section aria-labelledby="contradictions-h">
        <h1 id="contradictions-h">Contradictions</h1>
        <nav aria-label="Contradiction status filter">
          {STATUS_TABS.map((t) => (
            <span key={t.key}>
              {t.key === cstatus ? (
                <strong>
                  {t.label} ({countOf(t.key)})
                </strong>
              ) : (
                <Link href={`/reviews?cstatus=${t.key}#contradictions-h`}>
                  {t.label} ({countOf(t.key)})
                </Link>
              )}{" "}
            </span>
          ))}
        </nav>
        {contradictions.length === 0 ? (
          <p>Nothing here.</p>
        ) : (
          <ul>
            {contradictions.map((c) => (
              <li key={c.id}>
                <p>
                  <strong>A:</strong> {c.memory_a.content ?? <em>(not visible to you)</em>}
                </p>
                <p>
                  <strong>B:</strong> {c.memory_b.content ?? <em>(not visible to you)</em>}
                </p>
                <p>
                  <small>
                    detected by {c.detected_by} · {formatAge(c.age_secs)} old
                    {c.status !== "open" && <> · {c.status}</>}
                  </small>
                </p>
                {c.suggested_resolution && (
                  <p>
                    <small>Suggested: {c.suggested_resolution}</small>
                  </p>
                )}
                {c.status === "open" && (
                  <ContradictionButtons
                    contradictionId={c.id}
                    memoryAId={c.memory_a.id}
                    memoryBId={c.memory_b.id}
                  />
                )}
              </li>
            ))}
          </ul>
        )}
      </section>
    </>
  );
}
