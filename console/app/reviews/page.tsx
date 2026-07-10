import { configFromEnv, listContradictions, pendingPromotions } from "@/lib/api";

import { ContradictionButtons, PromotionButtons } from "./review-buttons";

export const dynamic = "force-dynamic";

export default async function ReviewsPage() {
  const cfg = configFromEnv();
  const [promotions, contradictions] = await Promise.all([
    pendingPromotions(cfg),
    listContradictions(cfg),
  ]);

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
                <code>{p.memory_id}</code> → <strong>{p.to_status}</strong>
                {p.policy_rule && <small> ({p.policy_rule})</small>}{" "}
                <PromotionButtons promotionId={p.id} />
              </li>
            ))}
          </ul>
        )}
      </section>

      <section aria-labelledby="contradictions-h">
        <h1 id="contradictions-h">Open contradictions ({contradictions.length})</h1>
        {contradictions.length === 0 ? (
          <p>No open contradictions.</p>
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
                {c.suggested_resolution && <p><small>Suggested: {c.suggested_resolution}</small></p>}
                <ContradictionButtons
                  contradictionId={c.id}
                  memoryAId={c.memory_a.id}
                  memoryBId={c.memory_b.id}
                />
              </li>
            ))}
          </ul>
        )}
      </section>
    </>
  );
}
