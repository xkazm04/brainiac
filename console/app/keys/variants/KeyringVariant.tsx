"use client";

/*
 * Keys variant B — "Keyring". Mental model: physical keys on a board.
 * Every token is a fob — head (name/principal), shaft with teeth cut per
 * scope, worn tag showing last use. Revoked keys hang grayed with a cut
 * shaft. Minting is literally cutting a new key at the bench below.
 */

import { useState } from "react";
import { motion } from "framer-motion";

import type { ApiToken } from "@/lib/types";
import { FONT_DISPLAY, FONT_MONO, GROUND, GROUND_DIM, LABEL, MAGENTA } from "@/design/theme";

import type { KeysData } from "../keys-data";
import { refreshTokens, revokeKey, SCOPES } from "../keys-data";
import { fmtAgo, MintPanel } from "../KeyShared";

function KeyFob({ t, live, onRevoke }: { t: ApiToken; live: boolean; onRevoke: () => void }) {
  const [confirm, setConfirm] = useState(false);
  const dead = !!t.revoked_at;
  const tone = dead ? "rgba(233,237,255,0.25)" : GROUND;
  return (
    <motion.div
      initial={{ opacity: 0, y: 10 }}
      animate={{ opacity: 1, y: 0 }}
      transition={{ duration: 0.3 }}
      className="rounded-xl border p-4"
      style={{ borderColor: dead ? "rgba(233,237,255,0.08)" : "rgba(233,237,255,0.14)", opacity: dead ? 0.55 : 1 }}
    >
      {/* the key silhouette */}
      <svg viewBox="0 0 220 44" className="w-full" aria-hidden>
        <circle cx="22" cy="22" r="14" fill="none" stroke={tone} strokeWidth="2" />
        <circle cx="22" cy="22" r="5" fill="none" stroke={tone} strokeWidth="1.2" />
        <line x1="36" y1="22" x2={dead ? 150 : 196} y2="22" stroke={tone} strokeWidth="3" strokeLinecap="round" />
        {dead && <line x1="156" y1="12" x2="170" y2="32" stroke={MAGENTA} strokeWidth="2" strokeLinecap="round" />}
        {/* teeth = scopes */}
        {SCOPES.map((s, i) =>
          t.scopes.includes(s) ? (
            <rect
              key={s}
              x={120 + i * 22}
              y={24}
              width="10"
              height={8 + i * 4}
              fill={s === "admin" ? MAGENTA : tone}
              opacity={dead ? 0.4 : 0.9}
              rx="1.5"
            />
          ) : null,
        )}
      </svg>
      <div className="mt-2 flex items-baseline justify-between gap-2">
        <div className="min-w-0">
          <div className={`${FONT_MONO} truncate text-sm text-[#e9edff]/85`}>{t.name}</div>
          <div className={`${FONT_MONO} text-[10px] tracking-wider text-[#e9edff]/35`}>
            {t.prefix} · used {fmtAgo(t.last_used_at)}
          </div>
        </div>
        {dead ? (
          <span className={LABEL} style={{ color: MAGENTA }}>
            revoked
          </span>
        ) : confirm ? (
          <button onClick={onRevoke} className={`${FONT_MONO} shrink-0 rounded-full border px-2.5 py-0.5 text-[10px] uppercase tracking-widest`} style={{ borderColor: MAGENTA, color: MAGENTA }}>
            melt it?
          </button>
        ) : (
          <button
            onClick={() => setConfirm(true)}
            disabled={!live}
            className={`${FONT_MONO} shrink-0 rounded-full border border-white/12 px-2.5 py-0.5 text-[10px] uppercase tracking-widest text-[#e9edff]/40 transition hover:border-[#ff5da2]/60 hover:text-[#ff5da2] disabled:opacity-40`}
          >
            revoke
          </button>
        )}
      </div>
    </motion.div>
  );
}

export default function KeyringVariant({ data }: { data: KeysData }) {
  const [tokens, setTokens] = useState(data.tokens);
  const reload = () => {
    if (data.live) void refreshTokens().then(setTokens).catch(() => undefined);
  };
  const revoke = async (id: string) => {
    if (!data.live) return;
    await revokeKey(id).catch(() => undefined);
    reload();
  };

  return (
    <div className="mx-auto max-w-6xl px-6 py-6">
      <div className={LABEL} style={{ color: GROUND }}>
        ground · keys · the ring
      </div>
      <h1 className={`${FONT_DISPLAY} mt-1 text-3xl font-semibold tracking-tight text-white`}>
        Every key on the board, teeth visible.
      </h1>
      <p className={`${FONT_MONO} mt-2 max-w-lg text-sm text-[#e9edff]/50`}>
        Teeth are scopes — the longer the cut, the more it opens. A magenta tooth is admin.
        Secrets aren&apos;t on the board; only the cut pattern.
      </p>

      <div className="mt-5 grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
        {tokens.map((t) => (
          <KeyFob key={t.id} t={t} live={data.live} onRevoke={() => revoke(t.id)} />
        ))}
        {tokens.length === 0 && (
          <p className={`${FONT_MONO} col-span-full py-10 text-center text-sm text-[#e9edff]/35`}>
            the board is empty — cut the first key below
          </p>
        )}
      </div>

      {/* the cutting bench */}
      <div className="mt-6 rounded-xl border p-5" style={{ borderColor: GROUND_DIM, background: "rgba(223,230,242,0.02)" }}>
        <MintPanel users={data.users} live={data.live} onMinted={reload} />
      </div>
      {!data.live && (
        <div className={`${LABEL} mt-2`} style={{ color: "rgba(233,237,255,0.3)" }}>
          demo data
        </div>
      )}
    </div>
  );
}
