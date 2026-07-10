"use client";

/*
 * Archive variant B — "Card Catalog". Mental model: the librarian's desk.
 * No time metaphor at the surface — a dense, filterable ledger where every
 * row shows the facts you'd triage by (status, kind, team, validity,
 * confidence). Filters are drawer tabs; the record card opens inline.
 * Optimized for "find and inspect", not for storytelling.
 */

import { useMemo, useState } from "react";
import { AnimatePresence, motion } from "framer-motion";

import { band, FONT_DISPLAY, FONT_MONO, LABEL } from "@/design/theme";

import type { ArchiveData } from "../archive-data";
import MemoryInspector, { fmtDate, statusTone } from "../MemoryInspector";
import { useMemoryDetail } from "../useMemoryDetail";

const VIOLET = band("delta");

const STATUSES = ["canonical", "candidate", "raw", "deprecated"] as const;

export default function CardCatalogVariant({ data }: { data: ArchiveData }) {
  const [status, setStatus] = useState<string | null>(null);
  const [kind, setKind] = useState<string | null>(null);
  const [team, setTeam] = useState<string | null>(null);
  const [q, setQ] = useState("");
  const [selected, setSelected] = useState<string | null>(null);
  const { detail, loading } = useMemoryDetail(selected, data.live);

  const kinds = useMemo(() => [...new Set(data.rows.map((r) => r.kind))].sort(), [data.rows]);
  const teams = useMemo(() => [...new Set(data.rows.map((r) => r.team))].sort(), [data.rows]);

  const rows = useMemo(() => {
    const needle = q.trim().toLowerCase();
    return data.rows.filter(
      (r) =>
        (!status || r.status === status) &&
        (!kind || r.kind === kind) &&
        (!team || r.team === team) &&
        (!needle || r.content.toLowerCase().includes(needle)),
    );
  }, [data.rows, status, kind, team, q]);

  const chip = (active: boolean) =>
    `${FONT_MONO} rounded-full border px-3 py-1 text-xs transition ${
      active ? "border-[#b9a5f5]/70 text-[#c9b6ff] bg-[#b9a5f5]/[0.08]" : "border-white/12 text-[#e9edff]/50 hover:border-white/30 hover:text-white"
    }`;

  return (
    <div className="mx-auto max-w-6xl px-6 py-6">
      <div className="flex flex-wrap items-baseline justify-between gap-3">
        <div>
          <div className={LABEL} style={{ color: VIOLET }}>
            δ · archive · card catalog
          </div>
          <h1 className={`${FONT_DISPLAY} mt-1 text-3xl font-semibold tracking-tight text-white`}>
            The ledger of everything the org holds.
          </h1>
        </div>
        <input
          value={q}
          onChange={(e) => setQ(e.target.value)}
          placeholder="filter the stacks…"
          className={`${FONT_MONO} w-56 rounded-full border border-white/15 bg-white/[0.03] px-4 py-2 text-sm text-white placeholder:text-[#e9edff]/30 focus:border-[#b9a5f5]/60 focus:outline-none`}
          aria-label="Filter memories"
        />
      </div>

      {/* filter drawers */}
      <div className={`${FONT_MONO} mt-4 flex flex-wrap items-center gap-x-4 gap-y-2 border-y border-white/10 py-3 text-xs`}>
        <span className={LABEL} style={{ color: "rgba(233,237,255,0.35)" }}>status</span>
        {STATUSES.map((s) => (
          <button key={s} onClick={() => setStatus(status === s ? null : s)} className={chip(status === s)}>
            {s}
          </button>
        ))}
        <span className={`${LABEL} ml-2`} style={{ color: "rgba(233,237,255,0.35)" }}>kind</span>
        {kinds.map((k) => (
          <button key={k} onClick={() => setKind(kind === k ? null : k)} className={chip(kind === k)}>
            {k}
          </button>
        ))}
        <span className={`${LABEL} ml-2`} style={{ color: "rgba(233,237,255,0.35)" }}>team</span>
        {teams.map((t) => (
          <button key={t} onClick={() => setTeam(team === t ? null : t)} className={chip(team === t)}>
            {t}
          </button>
        ))}
        <span className="ml-auto text-[#e9edff]/35">
          {rows.length} / {data.total} records{!data.live && " · demo data"}
        </span>
      </div>

      {/* the ledger */}
      <div className="mt-1 divide-y divide-white/[0.06]">
        {rows.slice(0, 60).map((r) => (
          <div key={r.id}>
            <button
              onClick={() => setSelected(r.id === selected ? null : r.id)}
              className="grid w-full grid-cols-[90px_70px_1fr_150px] items-baseline gap-3 py-2.5 text-left transition hover:bg-white/[0.02] max-sm:grid-cols-1"
            >
              <span className={`${FONT_MONO} text-[10px] uppercase tracking-widest`} style={{ color: statusTone(r.status) }}>
                {r.status}
              </span>
              <span className={`${FONT_MONO} text-[10px] uppercase tracking-widest text-[#e9edff]/40`}>
                {r.kind}
              </span>
              <span className={`${FONT_MONO} truncate text-sm text-[#e9edff]/80`} title={r.content}>
                {r.content}
              </span>
              <span className={`${FONT_MONO} text-right text-[10px] uppercase tracking-widest text-[#e9edff]/30`}>
                {r.team} · {fmtDate(r.valid_from ?? r.created_at)}
              </span>
            </button>
            <AnimatePresence>
              {selected === r.id && (
                <motion.div
                  initial={{ opacity: 0, height: 0 }}
                  animate={{ opacity: 1, height: "auto" }}
                  exit={{ opacity: 0, height: 0 }}
                  transition={{ duration: 0.25 }}
                  className="overflow-hidden"
                >
                  <div className="my-2 rounded-xl border p-5" style={{ borderColor: "hsla(262,85%,68%,0.35)", background: "hsla(262,85%,60%,0.04)" }}>
                    {loading && <p className={`${FONT_MONO} text-sm text-[#e9edff]/40`}>pulling the card…</p>}
                    {detail && <MemoryInspector detail={detail} onHop={setSelected} />}
                  </div>
                </motion.div>
              )}
            </AnimatePresence>
          </div>
        ))}
        {rows.length === 0 && (
          <p className={`${FONT_MONO} py-12 text-center text-sm text-[#e9edff]/35`}>
            no records match — loosen a filter
          </p>
        )}
      </div>
    </div>
  );
}
