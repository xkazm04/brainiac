import ApiOffline from "@/components/ApiOffline";
import { band, FONT_DISPLAY, FONT_MONO, GOLD, LABEL, MAGENTA } from "@/design/theme";
import { configFromEnv } from "@/lib/api";
import {
  contradictionQueue,
  formatAge,
  promotionQueue,
  type ContradictionStatus,
  type PromotionQueueItem,
} from "@/lib/governance-api";
import Link from "next/link";

import { ContradictionButtons, PromotionButtons } from "./review-buttons";

export const dynamic = "force-dynamic";

// Reviews rides the alpha band (calm governance). Approve/canonical is gamma
// gold (constructive); the contradiction seam is magenta.
const ALPHA = band("alpha");
const CARD = "rounded-xl border border-white/10 bg-white/[0.02] p-5";

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

/** A mono micro-chip; `tone` colors the border + text, else a neutral chip. */
function Chip({ children, tone }: { children: React.ReactNode; tone?: string }) {
  return (
    <span
      className={`${FONT_MONO} rounded-full border px-2.5 py-0.5 text-[11px]`}
      style={{
        borderColor: tone ? `${tone}55` : "rgba(233,237,255,0.14)",
        color: tone ?? "rgba(233,237,255,0.6)",
      }}
    >
      {children}
    </span>
  );
}

function PromotionCard({ p }: { p: PromotionQueueItem }) {
  return (
    <article className={CARD}>
      {p.memory ? (
        <p className="text-[15px] leading-relaxed text-[#e9edff]/90">{p.memory.content}</p>
      ) : (
        <p className={`${FONT_MONO} text-sm text-[#e9edff]/45`}>
          memory not visible to you ·{" "}
          <span className="text-[#e9edff]/30">{p.memory_id}</span>
        </p>
      )}

      <div className="mt-3 flex flex-wrap items-center gap-2">
        {p.memory?.kind && <Chip>{p.memory.kind}</Chip>}
        {p.memory?.confidence != null && (
          <Chip tone={GOLD}>{(p.memory.confidence * 100).toFixed(0)}% confidence</Chip>
        )}
        <Chip tone={ALPHA}>
          {p.from_status} → {p.to_status}
        </Chip>
        {p.memory?.team && <Chip>team {p.memory.team}</Chip>}
      </div>

      {p.provenance && (
        <p className={`${FONT_MONO} mt-3 text-xs text-[#e9edff]/45`}>
          via {p.provenance.actor_kind} {p.provenance.actor_id}
          {p.provenance.model_ref && <> · {p.provenance.model_ref}</>}
          {p.provenance.source_kind && (
            <>
              {" · from "}
              {p.provenance.source_kind}
              {p.provenance.source_ref && <>: {p.provenance.source_ref}</>}
            </>
          )}
        </p>
      )}

      <div className="mt-4 flex flex-wrap items-center justify-between gap-3 border-t border-white/[0.06] pt-4">
        <span className={`${LABEL}`} style={{ color: "rgba(233,237,255,0.35)" }}>
          waiting {formatAge(p.age_secs)}
          {p.policy_rule && <> · {p.policy_rule}</>}
        </span>
        <PromotionButtons promotionId={p.id} />
      </div>
    </article>
  );
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
  // Deliberate exception to withDemoFallback (see src/lib/demo-fallback.ts):
  // reviews is a write surface (approve / reject / resolve), so it hard-stops
  // rather than showing a fabricated queue wired to real actions.
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
    <div className={`${FONT_DISPLAY} mx-auto max-w-5xl px-6 py-8`}>
      <div className={LABEL} style={{ color: ALPHA }}>
        α · reviews · maintainer queue
      </div>
      <h1 className="mt-1 text-3xl font-semibold tracking-tight text-white">
        Sign what the org will remember.
      </h1>
      <p className={`${FONT_MONO} mt-2 max-w-2xl text-sm leading-relaxed text-[#e9edff]/55`}>
        Promotions raise a memory&apos;s visibility; contradictions are two claims out of phase.
        Every decision here is ledgered and signed.
      </p>

      {/* ── promotions ─────────────────────────────────────────────────── */}
      <section aria-labelledby="promotions-h" className="mt-8">
        <div className="flex items-baseline justify-between">
          <h2
            id="promotions-h"
            className="scroll-mt-6 text-lg font-semibold text-white"
          >
            Pending promotions
          </h2>
          <span className={LABEL} style={{ color: "rgba(233,237,255,0.35)" }}>
            {promotions.length} in queue
          </span>
        </div>

        {promotions.length === 0 ? (
          <div
            className={`${CARD} mt-3 flex flex-col items-center gap-1.5 py-10 text-center`}
          >
            <span className={FONT_MONO} style={{ color: GOLD }}>
              ◉ in phase
            </span>
            <p className={`${FONT_MONO} text-sm text-[#e9edff]/55`}>
              Promotion queue clear — nothing waiting on a maintainer.
            </p>
          </div>
        ) : (
          <div className="mt-3 space-y-3">
            {promotions.map((p) => (
              <PromotionCard key={p.id} p={p} />
            ))}
          </div>
        )}
      </section>

      {/* ── contradictions ─────────────────────────────────────────────── */}
      <section aria-labelledby="contradictions-h" className="mt-10">
        <div className="flex items-baseline justify-between">
          <h2
            id="contradictions-h"
            className="scroll-mt-6 text-lg font-semibold text-white"
          >
            Contradictions
          </h2>
          <span className={LABEL} style={{ color: "rgba(233,237,255,0.35)" }}>
            the dark seams
          </span>
        </div>

        <nav
          aria-label="Contradiction status filter"
          className={`${FONT_MONO} mt-3 flex flex-wrap items-center gap-2 text-xs`}
        >
          {STATUS_TABS.map((t) => {
            const active = t.key === cstatus;
            return (
              <Link
                key={t.key}
                href={`/reviews?cstatus=${t.key}#contradictions-h`}
                aria-current={active ? "true" : undefined}
                className="rounded-full border px-3 py-1 transition"
                style={
                  active
                    ? { borderColor: `${ALPHA}b3`, background: `${ALPHA}14`, color: ALPHA }
                    : { borderColor: "rgba(233,237,255,0.12)", color: "rgba(233,237,255,0.5)" }
                }
              >
                {t.label} · {countOf(t.key)}
              </Link>
            );
          })}
        </nav>

        {contradictions.length === 0 ? (
          <div
            className={`${CARD} mt-3 flex flex-col items-center gap-1.5 py-10 text-center`}
          >
            <span className={FONT_MONO} style={{ color: GOLD }}>
              ◉ constructive
            </span>
            <p className={`${FONT_MONO} text-sm text-[#e9edff]/55`}>
              {cstatus === "open"
                ? "No open contradictions — every source in phase."
                : "Nothing under this filter."}
            </p>
          </div>
        ) : (
          <div className="mt-3 space-y-3">
            {contradictions.map((c) => {
              const open = c.status === "open";
              return (
                <article
                  key={c.id}
                  className="rounded-xl border p-5"
                  style={{
                    borderColor: open ? `${MAGENTA}33` : "rgba(233,237,255,0.10)",
                    background: "rgba(255,255,255,0.02)",
                  }}
                >
                  <div className="grid gap-4 md:grid-cols-2 md:gap-0">
                    <div className="md:pr-5">
                      <div className={LABEL} style={{ color: "rgba(233,237,255,0.4)" }}>
                        A
                      </div>
                      <p className="mt-1.5 text-sm leading-relaxed text-[#e9edff]/85">
                        {c.memory_a.content ?? (
                          <span className="text-[#e9edff]/40">(not visible to you)</span>
                        )}
                      </p>
                    </div>
                    {/* the destructive seam */}
                    <div
                      className="md:border-l md:pl-5"
                      style={{ borderColor: `${MAGENTA}2e` }}
                    >
                      <div className={LABEL} style={{ color: "rgba(233,237,255,0.4)" }}>
                        B
                      </div>
                      <p className="mt-1.5 text-sm leading-relaxed text-[#e9edff]/85">
                        {c.memory_b.content ?? (
                          <span className="text-[#e9edff]/40">(not visible to you)</span>
                        )}
                      </p>
                    </div>
                  </div>

                  <p className={`${FONT_MONO} mt-4 text-xs text-[#e9edff]/40`}>
                    detected by {c.detected_by} · {formatAge(c.age_secs)} old
                    {!open && <> · {c.status}</>}
                  </p>

                  {c.suggested_resolution && (
                    <p
                      className={`${FONT_MONO} mt-3 rounded-lg border px-3 py-2 text-xs leading-relaxed`}
                      style={{
                        borderColor: `${ALPHA}33`,
                        background: `${ALPHA}0d`,
                        color: "rgba(233,237,255,0.75)",
                      }}
                    >
                      <span style={{ color: ALPHA }}>suggested · </span>
                      {c.suggested_resolution}
                    </p>
                  )}

                  {open && (
                    <div className="mt-4 border-t border-white/[0.06] pt-4">
                      <ContradictionButtons
                        contradictionId={c.id}
                        memoryAId={c.memory_a.id}
                        memoryBId={c.memory_b.id}
                      />
                    </div>
                  )}
                </article>
              );
            })}
          </div>
        )}
      </section>
    </div>
  );
}
