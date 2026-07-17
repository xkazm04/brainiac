/*
 * Practice divergence — the standardization surface.
 *
 * Where the disputes bench catches two facts that cannot both be true, this
 * catches the subtler, costlier thing: several teams solving the SAME problem
 * DIFFERENT ways, each locally reasonable, invisible to anyone inside one team.
 * The server's LLM sweep (scan-divergence) adjudicates cross-team clusters into
 * a named practice, each team's approach, and ONE recommended standard.
 *
 * Design stance: this is a decision surface for a platform lead, so each card
 * ends in an argument, not a verdict — the recommended standard is a starting
 * position a human ratifies with the provenance in front of them, which is why
 * the adjudicating model is named on every card.
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
import type { PracticeDivergence, PracticeDivergences } from "@/lib/types";

/**
 * `YYYY-MM-DD`, or an em dash when the timestamp is unusable.
 *
 * `new Date(x).toISOString()` THROWS a RangeError on an unparseable value, and
 * this renders inside a card in a mapped list — so ONE malformed `detected_at`
 * from the sweep took down the whole board. The route had no error.tsx either, so
 * the crash reached the root boundary and white-screened it. A date we can't read
 * is worth an em dash, not an outage.
 */
function isoDay(value: string | null | undefined): string {
  if (!value) return "—";
  const t = new Date(value);
  return Number.isNaN(t.getTime()) ? "—" : t.toISOString().slice(0, 10);
}

/** One group's way: `team` on team-axis rows, `project` on project-axis ones
 *  (PROJECT-PLAN PR3). `group` is whichever was present. */
type Approach = { group: string; approach: string };

/** impact → accent. High is the alarm colour; the sweep is conservative, so a
 *  "high" is rare and earns the strongest signal. */
const impactAccent = (impact: string): string =>
  impact === "high" ? MAGENTA : impact === "medium" ? GOLD : band("beta");

const readApproaches = (raw: unknown): Approach[] => {
  if (!Array.isArray(raw)) return [];
  return raw
    .filter((a): a is Record<string, unknown> => !!a && typeof a === "object")
    .map((a) => ({
      group: String(a.project ?? a.team ?? "—"),
      approach: String(a.approach ?? ""),
    }));
};

const teamAccent = (i: number): string =>
  [band("theta"), band("gamma"), band("beta"), band("delta"), band("alpha")][i % 5];

function DivergenceCard({ d }: { d: PracticeDivergence }) {
  const accent = impactAccent(d.impact);
  const approaches = readApproaches(d.approaches);
  return (
    <article
      className="flex flex-col overflow-hidden rounded-xl"
      style={{ background: PANEL, border: `1px solid ${BORDER}` }}
    >
      <div
        className="flex items-center gap-3 px-6 py-5"
        style={{ borderBottom: `1px solid ${BORDER}` }}
      >
        <span
          className={`${FONT_MONO} rounded-md px-2.5 py-1 text-[10px] uppercase tracking-[0.14em]`}
          style={{ color: accent, border: `1px solid ${accent}`, background: withAlpha(accent, 0.08) }}
        >
          {d.impact} impact
        </span>
        {/* The divergence class: who is diverging — teams, or applications.
            A cross-project drift usually resolves into a per-stack Library
            rule; a cross-team one into a conversation (PR3). */}
        <span
          className={`${FONT_MONO} rounded-md px-2.5 py-1 text-[10px] uppercase tracking-[0.14em]`}
          style={{
            color: INK_FAINT,
            border: `1px solid ${BORDER}`,
          }}
        >
          {d.axis === "project" ? "across projects" : "across teams"}
        </span>
        <h2 className={`${FONT_DISPLAY} text-2xl`} style={{ color: INK }}>
          {d.practice}
        </h2>
      </div>

      {d.summary && (
        <p className="px-6 pt-5 text-[15px] leading-snug" style={{ color: INK_DIM }}>
          {d.summary}
        </p>
      )}

      {/* Each team's approach, side by side — the divergence made concrete. */}
      {approaches.length > 0 && (
        <div className="grid gap-px px-6 pt-5 sm:grid-cols-2" style={{ background: "transparent" }}>
          {approaches.map((a, i) => {
            const ta = teamAccent(i);
            return (
              <div
                key={`${a.group}-${i}`}
                className="flex flex-col gap-2 p-4"
                style={{ background: "rgba(255,255,255,0.02)", border: `1px solid ${BORDER}` }}
              >
                <div className="flex items-center gap-2">
                  <span className="h-2 w-2 rounded-sm" style={{ background: ta }} />
                  <span className={LABEL} style={{ color: ta }}>
                    {a.group}
                  </span>
                </div>
                <p className={`${FONT_MONO} text-[13px] leading-relaxed`} style={{ color: INK }}>
                  {a.approach}
                </p>
              </div>
            );
          })}
        </div>
      )}

      {/* The recommended standard — the one line a platform lead acts on. */}
      {d.recommended_standard && (
        <div className="flex items-start gap-3 px-6 py-5">
          <span className={LABEL} style={{ color: band("beta"), paddingTop: "2px" }}>
            recommend
          </span>
          <p className={`${FONT_DISPLAY} text-[16px] leading-snug`} style={{ color: INK }}>
            {d.recommended_standard}
          </p>
        </div>
      )}

      <div
        className={`${FONT_MONO} flex flex-wrap justify-between gap-2 px-6 py-3 text-[11px]`}
        style={{ borderTop: `1px solid ${BORDER}`, color: INK_FAINT }}
      >
        <span>adjudicated by {d.model_ref ?? "—"}</span>
        <span>{isoDay(d.detected_at)}</span>
      </div>
    </article>
  );
}

export default function PracticeDivergenceReport({ data }: { data: PracticeDivergences }) {
  const divergences = data.divergences ?? [];
  return (
    <main className="mx-auto flex max-w-5xl flex-col gap-10 px-6 py-12">
      <header className="flex flex-col gap-3">
        <span className={LABEL} style={{ color: INK_FAINT }}>
          standardization
        </span>
        <h1 className={`${FONT_DISPLAY} text-3xl`} style={{ color: INK }}>
          Where teams solved the same problem different ways
        </h1>
        <p className="max-w-2xl text-[15px] leading-snug" style={{ color: INK_DIM }}>
          A contradiction is two facts that can&rsquo;t both be true. This is subtler: teams each
          solving the <em>same</em> problem their own way, every choice locally reasonable, the
          drift invisible from inside a single team. The sweep only surfaces the genuine ones — an
          empty list is a healthy sign, not a broken scan.
        </p>
      </header>

      {divergences.length === 0 ? (
        <p
          className="rounded-xl p-6 text-[14px]"
          style={{ background: PANEL, border: `1px solid ${BORDER}`, color: INK_DIM }}
        >
          No divergences on the board. Either the last sweep found every cross-team cluster
          consistent, or no sweep has run yet — trigger one from the scan control above, or enable
          the schedule so it runs on a cadence.
        </p>
      ) : (
        <section className="flex flex-col gap-5">
          <div className="flex items-baseline gap-3">
            <h2 className={`${FONT_DISPLAY} text-xl`} style={{ color: INK }}>
              What to standardize
            </h2>
            <span className={`${FONT_MONO} text-[12px]`} style={{ color: INK_FAINT }}>
              {divergences.length} {divergences.length === 1 ? "divergence" : "divergences"} —
              highest impact first
            </span>
          </div>
          {divergences.map((d, i) => (
            <DivergenceCard key={`${d.practice}-${i}`} d={d} />
          ))}
        </section>
      )}
    </main>
  );
}
