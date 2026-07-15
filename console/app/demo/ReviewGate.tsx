/*
 * The review gate, read-only — the public twin of app/console/(modules)/reviews.
 *
 * Lifted verbatim out of the old /demo/reviews route when the tour collapsed
 * into one page. It takes its queues as props rather than importing the fixture
 * itself, because the tour's page is the only server component left in the
 * subtree and it does all the fixture loading in one place.
 */

import { band, FONT_MONO, GOLD, LABEL, MAGENTA } from "@/design/theme";
import { formatAge } from "@/lib/format";
import type { ContradictionQueueItem, PromotionQueueItem } from "@/lib/governance-api";

const ALPHA = band("alpha");
const CARD = "rounded-xl border border-white/10 bg-white/[0.02] p-5";
const dim = (a: number) => `rgba(233,237,255,${a})`;

function Chip({ children, tone }: { children: React.ReactNode; tone?: string }) {
  return (
    <span
      className={`${FONT_MONO} rounded-full border px-2.5 py-0.5 text-[11px]`}
      style={{ borderColor: tone ? `${tone}55` : "rgba(233,237,255,0.14)", color: tone ?? dim(0.6) }}
    >
      {children}
    </span>
  );
}

/**
 * The read-only twin of the operator's approve/reject pair.
 *
 * Not disabled real buttons — there is no action wired behind these at all. The
 * public showcase must be structurally incapable of mutating anything, not
 * merely discouraged from it.
 */
function GateStamp() {
  return (
    <span className={`${FONT_MONO} flex items-center gap-2 text-[11px]`} style={{ color: dim(0.35) }}>
      <span
        className="rounded-full border px-3 py-1"
        style={{ borderColor: "hsla(46,90%,68%,0.3)", color: "hsla(46,90%,68%,0.55)" }}
      >
        approve
      </span>
      <span
        className="rounded-full border px-3 py-1"
        style={{ borderColor: "rgba(255,93,162,0.25)", color: "rgba(255,93,162,0.5)" }}
      >
        reject
      </span>
      <span className="ml-1">read-only in the demo</span>
    </span>
  );
}

function PromotionCard({ p }: { p: PromotionQueueItem }) {
  return (
    <article className={CARD}>
      {p.memory ? (
        <p className="text-[15px] leading-relaxed text-[#e9edff]/90">{p.memory.content}</p>
      ) : (
        <p className={`${FONT_MONO} text-sm`} style={{ color: dim(0.45) }}>
          memory not visible to you · <span style={{ color: dim(0.3) }}>{p.memory_id}</span>
          <span className="mt-2 block text-[11px]" style={{ color: ALPHA }}>
            row-level security is doing this, not the UI — the reviewer can see that a
            promotion exists without being shown a claim outside their scope.
          </span>
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
        <p className={`${FONT_MONO} mt-3 text-xs`} style={{ color: dim(0.45) }}>
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
        <span className={LABEL} style={{ color: dim(0.35) }}>
          waiting {formatAge(p.age_secs)}
          {p.policy_rule && <> · {p.policy_rule}</>}
        </span>
        <GateStamp />
      </div>
    </article>
  );
}

function ContradictionCard({ c }: { c: ContradictionQueueItem }) {
  const open = c.status === "open";
  return (
    <article className={CARD}>
      <div className="flex flex-wrap items-center gap-2">
        <Chip tone={open ? MAGENTA : GOLD}>{c.status.replace(/_/g, " ")}</Chip>
        <Chip>detected by {c.detected_by}</Chip>
        {c.suggested_resolution && <Chip tone={ALPHA}>suggests {c.suggested_resolution}</Chip>}
      </div>

      <div className="mt-4 grid gap-3 md:grid-cols-2">
        {[c.memory_a, c.memory_b].map((m, i) => (
          <div
            key={m.id}
            className="rounded-lg border p-4"
            style={{
              borderColor: i === 0 ? "hsla(46,90%,68%,0.25)" : "rgba(255,93,162,0.25)",
              background: i === 0 ? "hsla(46,90%,60%,0.04)" : "rgba(255,93,162,0.04)",
            }}
          >
            <div className={LABEL} style={{ color: i === 0 ? GOLD : MAGENTA }}>
              {m.id}
            </div>
            <p className={`${FONT_MONO} mt-2 text-xs leading-relaxed`} style={{ color: dim(0.75) }}>
              {m.content}
            </p>
          </div>
        ))}
      </div>

      <div className="mt-4 flex flex-wrap items-center justify-between gap-3 border-t border-white/[0.06] pt-4">
        <span className={LABEL} style={{ color: dim(0.35) }}>
          {open ? `open ${formatAge(c.age_secs)}` : `resolved by ${c.resolved_by}`}
        </span>
        {open ? (
          <GateStamp />
        ) : (
          <span className={`${FONT_MONO} text-[11px]`} style={{ color: GOLD }}>
            ✓ a human adjudicated this — the loser was superseded, not deleted
          </span>
        )}
      </div>
    </article>
  );
}

export default function ReviewGate({
  promotions,
  contradictions,
}: {
  promotions: PromotionQueueItem[];
  contradictions: ContradictionQueueItem[];
}) {
  return (
    <div className="mx-auto max-w-5xl px-6 pt-8">
      <div className={LABEL} style={{ color: GOLD }}>
        the gate
      </div>
      <h1 className="mt-2 max-w-3xl text-3xl font-semibold leading-tight tracking-tight text-white md:text-4xl">
        An agent proposes. A named human promotes.
      </h1>
      <p
        className={`${FONT_MONO} mt-4 max-w-2xl text-sm leading-relaxed`}
        style={{ color: dim(0.55) }}
      >
        This is the row that is empty for every other memory product. Nothing below is org
        truth yet — a machine extracted it from a real session, policy routed it here, and
        it stays here until a maintainer signs for it. In the demo the buttons are inert;
        in the console they are the only way anything becomes canonical.
      </p>

      <section className="mt-10">
        <div className="flex items-baseline justify-between">
          <h2 className={LABEL} style={{ color: ALPHA }}>
            pending promotions · {promotions.length}
          </h2>
        </div>
        <div className="mt-4 space-y-4">
          {promotions.map((p) => (
            <PromotionCard key={p.id} p={p} />
          ))}
        </div>
      </section>

      <section className="mt-14">
        <h2 className={LABEL} style={{ color: MAGENTA }}>
          contradictions · when two sources disagree right now
        </h2>
        <p className={`${FONT_MONO} mt-3 max-w-2xl text-xs leading-relaxed`} style={{ color: dim(0.5) }}>
          Every other system in this market resolves a conflict silently, by last-writer-wins,
          or accumulates both and lets the ranker decide. Brainiac stops and asks.
        </p>
        <div className="mt-4 space-y-4">
          {contradictions.map((c) => (
            <ContradictionCard key={c.id} c={c} />
          ))}
        </div>
      </section>
    </div>
  );
}
