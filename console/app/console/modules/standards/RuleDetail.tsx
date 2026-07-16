"use client";

/*
 * One rule, in full: the statement a team is asked to follow, the chips that
 * say how strongly and since when, the evidence behind it (or the named
 * decree standing in for evidence), the per-team pulse, and the version
 * history. The triage controls mount below only on a live board.
 */

import {
  band,
  BORDER,
  FONT_DISPLAY,
  FONT_MONO,
  GOLD,
  INK,
  INK_DIM,
  INK_FAINT,
  LABEL,
  MAGENTA,
  PANEL,
  withAlpha,
} from "@/design/theme";
import type { StandardDetail } from "@/lib/types";

export const lifecycleTone = (lifecycle: string): string =>
  lifecycle === "adopted"
    ? band("beta")
    : lifecycle === "proposed"
      ? GOLD
      : lifecycle === "rejected"
        ? withAlpha(MAGENTA, 0.6)
        : INK_FAINT;

function Chip({ text, tone }: { text: string; tone: string }) {
  return (
    <span
      className={`${FONT_MONO} rounded-md px-2.5 py-1 text-[10px] uppercase tracking-[0.14em]`}
      style={{ color: tone, border: `1px solid ${tone}`, background: withAlpha(tone, 0.08) }}
    >
      {text}
    </span>
  );
}

/** Strip a single outer code fence; examples render mono either way. */
const unfence = (md: string): string => {
  const m = md.match(/^```[^\n]*\n([\s\S]*?)\n?```\s*$/);
  return m ? m[1] : md;
};

const isoDay = (value: string | null | undefined): string => {
  if (!value) return "—";
  const t = new Date(value);
  return Number.isNaN(t.getTime()) ? "—" : t.toISOString().slice(0, 10);
};

export default function RuleDetail({
  detail,
  gate,
}: {
  detail: StandardDetail;
  /** The triage controls, mounted by the live board only — a demo board must
   *  never offer a working-looking gate over fabricated rules. */
  gate?: React.ReactNode;
}) {
  const tone = lifecycleTone(detail.lifecycle);
  const maxUses = Math.max(1, ...detail.usage.map((u) => u.uses));
  return (
    <article className="flex flex-col gap-6 rounded-xl p-6" style={{ background: PANEL, border: `1px solid ${BORDER}` }}>
      <header className="flex flex-col gap-3">
        <div className="flex flex-wrap items-center gap-2">
          <Chip text={detail.lifecycle} tone={tone} />
          <Chip text={detail.enforcement} tone={detail.enforcement === "mandatory" ? MAGENTA : INK_FAINT} />
          {/* Who is asking — a maintainer weighing trust must see the source. */}
          {detail.origin === "sweep" && <Chip text="mined by the sweep" tone={band("theta")} />}
          {detail.origin === "agent" && <Chip text="proposed by an agent" tone={band("alpha")} />}
          {detail.decreed && <Chip text="decreed — signed, no evidence" tone={GOLD} />}
          <span className={`${FONT_MONO} ml-auto text-[11px]`} style={{ color: INK_FAINT }}>
            {detail.stack} / {detail.category} / {detail.slug}
          </span>
        </div>
        <h2 className={`${FONT_DISPLAY} text-2xl leading-snug`} style={{ color: INK }}>
          {detail.statement}
        </h2>
        {detail.rationale && (
          <p className="max-w-2xl text-[14px] leading-snug" style={{ color: INK_DIM }}>
            {detail.rationale}
          </p>
        )}
      </header>

      {detail.detail_md && (
        <section className="flex flex-col gap-2">
          <span className={LABEL} style={{ color: INK_FAINT }}>
            the examples — verbatim, never re-typed
          </span>
          <pre
            className={`${FONT_MONO} overflow-x-auto rounded-lg p-4 text-[12.5px] leading-relaxed`}
            style={{ background: "rgba(255,255,255,0.02)", border: `1px solid ${BORDER}`, color: INK }}
          >
            {unfence(detail.detail_md)}
          </pre>
        </section>
      )}

      {/* Why this rule exists — or the named decree standing in for evidence. */}
      <section className="flex flex-col gap-2">
        <span className={LABEL} style={{ color: INK_FAINT }}>
          why it exists
        </span>
        {detail.provenance.length === 0 ? (
          <p className={`${FONT_MONO} text-[12px]`} style={{ color: detail.decreed ? GOLD : MAGENTA }}>
            {detail.decreed
              ? "no evidence — a named human signed for this rule (a decree)."
              : "no evidence yet — this rule cannot be adopted until it has provenance or a signed decree."}
          </p>
        ) : (
          <div className="flex flex-wrap gap-2">
            {detail.provenance.map((p) => (
              <span
                key={`${p.kind}-${p.ref_id}`}
                className={`${FONT_MONO} rounded-md px-2.5 py-1.5 text-[11px]`}
                style={{
                  border: `1px solid ${withAlpha(p.kind === "divergence" ? band("theta") : band("delta", 72), 0.5)}`,
                  color: p.kind === "divergence" ? band("theta") : band("delta", 76),
                }}
                title={p.ref_id}
              >
                {p.kind === "divergence" ? "drift" : "memory"} · {p.ref_id.slice(0, 8)}
              </span>
            ))}
          </div>
        )}
      </section>

      {/* The pulse: usage per team. Teams, never names — by construction. */}
      <section className="flex flex-col gap-2">
        <span className={LABEL} style={{ color: INK_FAINT }}>
          the pulse · fetches + checks per team
        </span>
        {detail.usage.length === 0 ? (
          <p className={`${FONT_MONO} text-[12px]`} style={{ color: INK_FAINT }}>
            no signal yet — nothing has fetched or checked against this rule.
          </p>
        ) : (
          <div className="flex flex-col gap-1.5">
            {detail.usage.map((u, i) => (
              <div key={`${u.team ?? "org"}-${i}`} className="flex items-center gap-3">
                <span className={`${FONT_MONO} w-24 shrink-0 truncate text-[11px]`} style={{ color: INK_DIM }}>
                  {u.team ?? "org-scoped"}
                </span>
                <span
                  className="h-2 rounded-sm"
                  style={{
                    width: `${Math.max(3, (u.uses / maxUses) * 100)}%`,
                    background: withAlpha(band("theta"), 0.65),
                  }}
                />
                <span className={`${FONT_MONO} text-[11px]`} style={{ color: INK_FAINT }}>
                  {u.uses}
                </span>
              </div>
            ))}
          </div>
        )}
      </section>

      {/* Version history — every change to a rule is a numbered revision. */}
      <section className="flex flex-col gap-2">
        <span className={LABEL} style={{ color: INK_FAINT }}>
          versions
        </span>
        <ol className="flex flex-col gap-1.5">
          {detail.versions.map((v) => (
            <li key={v.rev} className={`${FONT_MONO} flex items-baseline gap-3 text-[12px]`}>
              <span style={{ color: INK_FAINT }}>r{v.rev}</span>
              <span className="min-w-0 flex-1 truncate" style={{ color: INK_DIM }}>
                {v.statement}
              </span>
              <span style={{ color: INK_FAINT }}>{v.enforcement}</span>
              <span style={{ color: INK_FAINT }}>{isoDay(v.created_at)}</span>
            </li>
          ))}
        </ol>
      </section>

      {gate && (
        <footer className="border-t pt-4" style={{ borderColor: BORDER }}>
          {gate}
        </footer>
      )}
    </article>
  );
}
