"use client";

/*
 * Keys variant C — "Blast Radius". Mental model: scope-first. You don't
 * start from a form — you start from a PERSON. Pick a principal, see the
 * rings of what their key could read, then mint from inside that view.
 * Access management as informed consent, not bookkeeping.
 */

import { useEffect, useState } from "react";
import { AnimatePresence, motion } from "framer-motion";

import type { TokenPreview } from "@/lib/types";
import { band, FONT_DISPLAY, FONT_MONO, GROUND, LABEL } from "@/design/theme";

import type { KeysData } from "../keys-data";
import { demoPreview, fetchPreview, refreshTokens } from "../keys-data";
import { BlastRings, MintPanel } from "../KeyShared";

export default function BlastRadiusVariant({ data }: { data: KeysData }) {
  const [tokens, setTokens] = useState(data.tokens);
  const [userId, setUserId] = useState(data.users[0]?.id ?? "");
  const [preview, setPreview] = useState<TokenPreview | null>(null);

  useEffect(() => {
    if (!userId) return;
    let cancelled = false;
    const run = data.live ? fetchPreview(userId) : Promise.resolve(demoPreview(userId, data.users));
    run.then((p) => !cancelled && setPreview(p)).catch(() => !cancelled && setPreview(null));
    return () => {
      cancelled = true;
    };
  }, [userId, data]);

  const reload = () => {
    if (data.live) void refreshTokens().then(setTokens).catch(() => undefined);
  };

  const activeCount = tokens.filter((t) => !t.revoked_at).length;

  return (
    <div className="mx-auto max-w-6xl px-6 py-6">
      <div className={LABEL} style={{ color: GROUND }}>
        ground · keys · blast radius
      </div>
      <h1 className={`${FONT_DISPLAY} mt-1 text-3xl font-semibold tracking-tight text-white`}>
        Before you cut a key, see what it opens.
      </h1>

      <div className="mt-5 grid gap-6 lg:grid-cols-[240px_1fr]">
        {/* principal roster */}
        <div className="space-y-1.5">
          <div className={LABEL} style={{ color: "rgba(233,237,255,0.4)" }}>
            principals
          </div>
          {data.users.map((u) => {
            const selected = userId === u.id;
            return (
              <button
                key={u.id}
                onClick={() => setUserId(u.id)}
                className={`${FONT_MONO} block w-full rounded-lg border px-3 py-2 text-left text-sm transition ${
                  selected ? "border-white/50 bg-white/[0.04] text-white" : "border-white/10 text-[#e9edff]/55 hover:border-white/25 hover:text-white"
                }`}
              >
                <div className="truncate">{u.email.split("@")[0]}</div>
                <div className="text-[10px] uppercase tracking-widest" style={{ color: selected ? band("alpha") : "rgba(233,237,255,0.3)" }}>
                  {u.teams.map((t) => `${t.name}${t.role === "maintainer" ? "*" : ""}`).join(" + ") || "no team"}
                </div>
              </button>
            );
          })}
          <p className={`${LABEL} pt-1`} style={{ color: "rgba(233,237,255,0.3)" }}>
            * maintainer · {activeCount} active keys org-wide
            {!data.live && " · demo data"}
          </p>
        </div>

        {/* the radius + mint */}
        <div className="space-y-4">
          <AnimatePresence mode="wait">
            {preview && (
              <motion.div
                key={preview.user_id}
                initial={{ opacity: 0, y: 10 }}
                animate={{ opacity: 1, y: 0 }}
                exit={{ opacity: 0 }}
                transition={{ duration: 0.25 }}
                className="rounded-xl border border-white/10 bg-white/[0.015] p-5"
              >
                <BlastRings preview={preview} />
                <p className={`${FONT_MONO} mt-3 max-w-md text-xs leading-relaxed text-[#e9edff]/40`}>
                  A key minted for this principal can never read more than this —
                  the rings are computed by the same row-level security the runtime
                  enforces on every query.
                </p>
              </motion.div>
            )}
          </AnimatePresence>

          <div className="rounded-xl border border-white/10 bg-white/[0.015] p-5">
            <MintPanel users={data.users} live={data.live} onMinted={reload} />
          </div>
        </div>
      </div>
    </div>
  );
}
