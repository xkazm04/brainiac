import Link from "next/link";

import { band, FONT_MONO, GOLD, INK_DIM, INK_FAINT, LABEL, MAGENTA, withAlpha } from "@/design/theme";
import type { AuditEvent, AuditKind } from "@/lib/governance-api";

import {
  ageLabel,
  actorLabel,
  AUDIT_KIND_TABS,
  kindLabel,
  MEMORY_HREF,
  outcomeTone,
  PAGE,
  type AuditData,
} from "./audit-data";

const ALPHA = band("alpha");
// Same convention as everywhere else in the console (theme.ts): gamma/gold
// reads canonical-and-constructive, magenta reads contradiction/destructive.
const TONE_COLOR = { good: GOLD, bad: MAGENTA, neutral: INK_FAINT } as const;

/** Links preserve the current kind filter — paging never resets it. */
function pageHref(kind: AuditKind | undefined, offset: number): string {
  const params = new URLSearchParams({ m: "audit" });
  if (kind) params.set("kind", kind);
  if (offset > 0) params.set("offset", String(offset));
  return `/console?${params.toString()}`;
}

function EventRow({ e }: { e: AuditEvent }) {
  const tone = outcomeTone(e.outcome);
  return (
    <li className="grid grid-cols-[104px_minmax(0,1fr)_160px_96px] items-start gap-x-3 border-b border-white/[0.05] px-3 py-2.5 last:border-b-0">
      <span className={`${LABEL} pt-0.5`} style={{ color: INK_FAINT }}>
        {kindLabel(e.kind)}
      </span>
      <span className="min-w-0">
        <span className={`${FONT_MONO} text-sm`} style={{ color: TONE_COLOR[tone] }}>
          {e.outcome}
        </span>
        {e.detail && (
          <span className={`${FONT_MONO} block truncate text-sm`} style={{ color: INK_DIM }} title={e.detail}>
            {e.detail}
          </span>
        )}
        <span className={`${FONT_MONO} mt-0.5 block text-xs`} style={{ color: INK_FAINT }}>
          <Link href={MEMORY_HREF} className="underline decoration-dotted underline-offset-2 hover:text-white">
            {e.memory_id.slice(0, 8)}
          </Link>
          {e.memory_b && (
            <>
              {" ↔ "}
              <Link href={MEMORY_HREF} className="underline decoration-dotted underline-offset-2 hover:text-white">
                {e.memory_b.slice(0, 8)}
              </Link>
            </>
          )}
        </span>
      </span>
      <span className={`${FONT_MONO} pt-0.5 text-xs`} style={{ color: INK_FAINT }}>
        {actorLabel(e.actor_id)}
      </span>
      <span className={`${FONT_MONO} pt-0.5 text-right text-xs tabular-nums`} style={{ color: INK_FAINT }}>
        {ageLabel(e.at)}
      </span>
    </li>
  );
}

export default function AuditLedger({
  data,
  kind,
  offset,
}: {
  data: AuditData;
  kind: AuditKind | undefined;
  offset: number;
}) {
  const { events, total } = data;
  const hasPrev = offset > 0;
  const hasNext = offset + events.length < total;

  return (
    <div className="mx-auto max-w-5xl px-6 py-8">
      <div className={LABEL} style={{ color: ALPHA }}>
        α · audit · the ledger
      </div>
      <h1 className="mt-1 text-3xl font-semibold tracking-tight text-white">
        Who approved this?
      </h1>
      <p className={`${FONT_MONO} mt-2 max-w-2xl text-sm leading-relaxed`} style={{ color: INK_DIM }}>
        {total.toLocaleString()} governance decision{total === 1 ? "" : "s"} — every promotion review,
        contradiction resolution and dispute answer, reverse-chronological.
      </p>

      {/*
       * The honest caveat, stated where an auditor will actually read it — not
       * a footnote. src/lib/auth.ts is explicit that the console gate is ONE
       * shared passcode: the server stamps every human decision with the same
       * principal, so "actor" below names the token that decided, never a
       * person. Building a per-user identity read on top of a shared secret
       * would be the fake this module exists not to build.
       */}
      <div
        className="mt-4 rounded-lg border px-4 py-2.5"
        style={{ borderColor: withAlpha(ALPHA, 0.25), background: withAlpha(ALPHA, 0.05) }}
      >
        <p className={`${FONT_MONO} text-xs leading-relaxed`} style={{ color: INK_DIM }}>
          This console is gated by one shared org passcode, not per-user login. Every row below
          records <em>which token</em> decided — not which person was at the keyboard. Treat
          &ldquo;org token&rdquo; as the org&rsquo;s signature, not an individual&rsquo;s.
        </p>
      </div>

      {/* kind filter — a server round trip, so tabs are links */}
      <div className="mt-5 flex flex-wrap items-center gap-1.5">
        {AUDIT_KIND_TABS.map((t) => {
          const active = (t.key === "all" && !kind) || t.key === kind;
          return (
            <Link
              key={t.key}
              href={pageHref(t.key === "all" ? undefined : t.key, 0)}
              className={`${FONT_MONO} rounded-full border px-3 py-1 text-xs uppercase tracking-[0.12em] transition ${
                active ? "" : "border-white/15 text-[#e9edff]/50 hover:border-white/40 hover:text-white"
              }`}
              style={active ? { borderColor: ALPHA, color: ALPHA, background: withAlpha(ALPHA, 0.1) } : undefined}
            >
              {t.label}
            </Link>
          );
        })}
      </div>

      <div className="mt-4 rounded-xl border border-white/10 bg-white/[0.015]">
        <div
          className={`${LABEL} grid grid-cols-[104px_minmax(0,1fr)_160px_96px] gap-x-3 border-b border-white/10 px-3 py-2`}
          style={{ color: INK_FAINT }}
        >
          <span>kind</span>
          <span>outcome · memory</span>
          <span>actor</span>
          <span className="text-right">when</span>
        </div>

        {events.length === 0 ? (
          <p className={`${FONT_MONO} px-3 py-12 text-center text-sm`} style={{ color: INK_FAINT }}>
            no decisions {kind ? `of this kind` : "yet"} — the ledger fills as reviews resolve
          </p>
        ) : (
          <ul>
            {events.map((e) => (
              <EventRow key={`${e.kind}:${e.id}`} e={e} />
            ))}
          </ul>
        )}

        <div className="flex flex-wrap items-center justify-between gap-x-4 gap-y-2 border-t border-white/10 px-3 py-2">
          <span className={`${FONT_MONO} text-xs`} style={{ color: INK_DIM }}>
            {events.length === 0
              ? `showing 0 of ${total}`
              : `showing ${offset + 1}–${offset + events.length} of ${total.toLocaleString()}`}
          </span>
          <div className="flex items-center gap-2">
            <Link
              href={pageHref(kind, Math.max(0, offset - PAGE))}
              aria-disabled={!hasPrev}
              className={`${FONT_MONO} rounded-full border px-3 py-1 text-[11px] uppercase tracking-[0.14em] transition ${
                hasPrev
                  ? "border-white/25 text-white hover:border-white/60"
                  : "pointer-events-none border-transparent"
              }`}
              style={!hasPrev ? { color: INK_FAINT } : undefined}
            >
              ← prev
            </Link>
            <Link
              href={pageHref(kind, offset + PAGE)}
              aria-disabled={!hasNext}
              className={`${FONT_MONO} rounded-full border px-3 py-1 text-[11px] uppercase tracking-[0.14em] transition ${
                hasNext
                  ? "border-white/25 text-white hover:border-white/60"
                  : "pointer-events-none border-transparent"
              }`}
              style={!hasNext ? { color: INK_FAINT } : undefined}
            >
              next →
            </Link>
          </div>
        </div>
      </div>

      {!data.live && (
        <div className={`${LABEL} mt-3`} style={{ color: INK_FAINT }}>
          demo data
        </div>
      )}
    </div>
  );
}
