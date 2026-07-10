import ApiOffline from "@/components/ApiOffline";
import { configFromEnv, getGraph } from "@/lib/api";

export const dynamic = "force-dynamic";

// v0: tabular graph view — the data shape the Sigma.js WebGL explorer will
// consume in the visual pass. Canonical hubs listed with their linked
// team-scoped surface forms; edges shown with their evidence memory.
export default async function GraphPage() {
  let graph;
  try {
    graph = await getGraph(configFromEnv());
  } catch (e) {
    return <ApiOffline error={e instanceof Error ? e.message : String(e)} />;
  }
  const byCanonical = new Map<string, string[]>();
  for (const e of graph.entities) {
    if (e.canonical_id) {
      const list = byCanonical.get(e.canonical_id) ?? [];
      list.push(e.name);
      byCanonical.set(e.canonical_id, list);
    }
  }
  const entityName = new Map(graph.entities.map((e) => [e.id, e.name]));

  return (
    <>
      <section aria-labelledby="canonicals-h">
        <h1 id="canonicals-h">Canonical entities ({graph.canonicals.length})</h1>
        <ul>
          {graph.canonicals.map((c) => (
            <li key={c.id}>
              <strong>{c.name}</strong> <small>({c.kind})</small>
              {byCanonical.has(c.id) && (
                <> — known as: {byCanonical.get(c.id)!.join(", ")}</>
              )}
            </li>
          ))}
        </ul>
      </section>

      <section aria-labelledby="edges-h">
        <h1 id="edges-h">Relationships ({graph.edges.length})</h1>
        <ul>
          {graph.edges.map((e, i) => (
            <li key={`${e.src}-${e.dst}-${i}`}>
              {entityName.get(e.src) ?? e.src} <em>{e.relation}</em>{" "}
              {entityName.get(e.dst) ?? e.dst}
              {e.evidence && (
                <>
                  {" "}
                  — <small>&ldquo;{e.evidence}&rdquo;</small>
                </>
              )}
            </li>
          ))}
        </ul>
      </section>
    </>
  );
}
