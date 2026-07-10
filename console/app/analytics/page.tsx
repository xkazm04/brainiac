import { configFromEnv, getAnalytics } from "@/lib/api";

export const dynamic = "force-dynamic";

function age(secs: number): string {
  if (secs <= 0) return "—";
  if (secs < 3600) return `${Math.round(secs / 60)} min`;
  if (secs < 86400) return `${Math.round(secs / 3600)} h`;
  return `${Math.round(secs / 86400)} d`;
}

export default async function AnalyticsPage() {
  const a = await getAnalytics(configFromEnv());
  return (
    <>
      <section aria-labelledby="reviews-h">
        <h1 id="reviews-h">Governance</h1>
        <ul>
          <li>Pending promotions: {a.reviews.pending_promotions}</li>
          <li>Oldest pending: {age(a.reviews.oldest_pending_secs)}</li>
          <li>Open contradictions: {a.reviews.open_contradictions}</li>
          <li>Ingest queue depth: {a.queue.ingest_depth}</li>
        </ul>
      </section>

      <section aria-labelledby="corpus-h">
        <h1 id="corpus-h">Corpus (your visibility)</h1>
        <table>
          <thead>
            <tr>
              <th>Status</th>
              <th>Memories</th>
            </tr>
          </thead>
          <tbody>
            {a.memories_by_status.map((r) => (
              <tr key={r.status}>
                <td>{r.status}</td>
                <td>{r.count}</td>
              </tr>
            ))}
          </tbody>
        </table>
        <p>
          Entities: {a.graph.entities} · Canonical: {a.graph.canonicals} · Embedding model:{" "}
          <code>{a.embedding_model}</code>
        </p>
      </section>
    </>
  );
}
