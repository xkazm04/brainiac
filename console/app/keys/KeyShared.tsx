"use client";

/*
 * Shared Keys components (hoisted per the /prototype skill): the
 * blast-radius rings, the mint flow (picker → preview → mint → one-time
 * secret reveal), and small formatting helpers. Variants frame these
 * differently; the mechanics are identical.
 */

import { useEffect, useState } from "react";
import { AnimatePresence, motion } from "framer-motion";

import type { MintedToken, OrgUser, TokenPreview } from "@/lib/types";
import { band, FONT_DISPLAY, FONT_MONO, GROUND, GROUND_DIM, LABEL, MAGENTA } from "@/design/theme";

import { demoPreview, fetchPreview, mintKey, SCOPES } from "./keys-data";

export function fmtAgo(iso: string | null): string {
  if (!iso) return "never";
  const secs = Math.max(0, (Date.now() - new Date(iso).getTime()) / 1000);
  if (secs < 3600) return `${Math.round(secs / 60)}m ago`;
  if (secs < 86400) return `${Math.round(secs / 3600)}h ago`;
  return `${Math.round(secs / 86400)}d ago`;
}

/** Concentric rings: what this principal's key would see. */
export function BlastRings({ preview }: { preview: TokenPreview }) {
  const { org, team, private: priv, total } = preview.visible;
  const max = Math.max(1, total);
  const rings = [
    { label: "org", n: org, r: 86, tone: GROUND_DIM },
    { label: "team", n: team, r: 60, tone: band("alpha", 68, 0.6) },
    { label: "private", n: priv, r: 34, tone: band("delta", 68, 0.7) },
  ];
  return (
    <div className="flex items-center gap-5">
      <svg viewBox="0 0 200 200" className="h-44 w-44 shrink-0" role="img" aria-label={`Blast radius: ${total} memories visible`}>
        {rings.map((ring) => (
          <motion.circle
            key={ring.label}
            cx={100}
            cy={100}
            r={ring.r}
            fill="none"
            stroke={ring.tone}
            strokeWidth={2 + (ring.n / max) * 10}
            initial={{ pathLength: 0, opacity: 0 }}
            animate={{ pathLength: 1, opacity: 1 }}
            transition={{ duration: 0.7 }}
          />
        ))}
        <text x={100} y={96} textAnchor="middle" fontSize="26" fill={GROUND} fontWeight={600}>
          {total}
        </text>
        <text x={100} y={114} textAnchor="middle" fontSize="9" fill="rgba(233,237,255,0.45)" style={{ textTransform: "uppercase", letterSpacing: "0.16em" }}>
          readable
        </text>
      </svg>
      <div className={`${FONT_MONO} space-y-1.5 text-sm`}>
        <div className={LABEL} style={{ color: "rgba(233,237,255,0.4)" }}>
          {preview.email} · {preview.teams.join(" + ") || "no team"}
        </div>
        <div className="text-[#e9edff]/70">
          <span style={{ color: GROUND }}>{org}</span> org-wide
        </div>
        <div className="text-[#e9edff]/70">
          <span style={{ color: band("alpha") }}>{team}</span> team-tier
        </div>
        <div className="text-[#e9edff]/70">
          <span style={{ color: band("delta") }}>{priv}</span> private (their own)
        </div>
        <div className="pt-1 text-xs text-[#e9edff]/40">{preview.visible.canonical} canonical among them</div>
      </div>
    </div>
  );
}

/** Full mint flow. Calls onMinted after the secret dialog is dismissed. */
export function MintPanel({
  users,
  live,
  onMinted,
}: {
  users: OrgUser[];
  live: boolean;
  onMinted: () => void;
}) {
  const [name, setName] = useState("");
  const [userId, setUserId] = useState<string>(users[0]?.id ?? "");
  const [scopes, setScopes] = useState<string[]>(["read"]);
  const [preview, setPreview] = useState<TokenPreview | null>(null);
  const [minted, setMinted] = useState<MintedToken | null>(null);
  const [state, setState] = useState<"idle" | "minting" | "error">("idle");
  const [copied, setCopied] = useState(false);

  useEffect(() => {
    if (!userId) return;
    let cancelled = false;
    const run = live ? fetchPreview(userId) : Promise.resolve(demoPreview(userId, users));
    run.then((p) => !cancelled && setPreview(p)).catch(() => !cancelled && setPreview(null));
    return () => {
      cancelled = true;
    };
  }, [userId, live, users]);

  const toggleScope = (s: string) =>
    setScopes((cur) => (cur.includes(s) ? cur.filter((x) => x !== s) : [...cur, s]));

  const mint = async () => {
    if (!name.trim() || !live || state === "minting" || scopes.length === 0) return;
    setState("minting");
    try {
      setMinted(await mintKey(name.trim(), userId, scopes));
      setName("");
      setState("idle");
    } catch {
      setState("error");
      setTimeout(() => setState("idle"), 3000);
    }
  };

  return (
    <div className="space-y-4">
      <div className={LABEL} style={{ color: "rgba(233,237,255,0.4)" }}>
        cut a new key
      </div>
      <input
        value={name}
        onChange={(e) => setName(e.target.value)}
        placeholder="key name — who or what will hold it"
        disabled={!live}
        className={`${FONT_MONO} w-full rounded-lg border border-white/15 bg-white/[0.03] px-3.5 py-2 text-sm text-white placeholder:text-[#e9edff]/30 focus:border-white/40 focus:outline-none disabled:opacity-50`}
      />
      <div className="flex flex-wrap items-center gap-2">
        <select
          value={userId}
          onChange={(e) => setUserId(e.target.value)}
          disabled={!live && users.length === 0}
          className={`${FONT_MONO} rounded-lg border border-white/15 bg-[#0a090f] px-3 py-2 text-sm text-white focus:border-white/40 focus:outline-none`}
          aria-label="Acts as user"
        >
          {users.map((u) => (
            <option key={u.id} value={u.id}>
              {u.email} · {u.teams.map((t) => t.name).join("+") || "no team"}
            </option>
          ))}
        </select>
        {SCOPES.map((s) => (
          <button
            key={s}
            onClick={() => toggleScope(s)}
            className={`${FONT_MONO} rounded-full border px-3 py-1 text-xs transition ${
              scopes.includes(s)
                ? s === "admin"
                  ? "border-[#ff5da2]/70 text-[#ff5da2]"
                  : "border-white/50 text-white"
                : "border-white/12 text-[#e9edff]/40 hover:border-white/30"
            }`}
            aria-pressed={scopes.includes(s)}
          >
            {s}
          </button>
        ))}
      </div>

      {preview && (
        <div className="rounded-xl border border-white/10 bg-white/[0.015] p-4">
          <div className={LABEL} style={{ color: GROUND }}>
            blast radius — computed by the same RLS the runtime enforces
          </div>
          <div className="mt-3">
            <BlastRings preview={preview} />
          </div>
        </div>
      )}

      <button
        onClick={mint}
        disabled={!live || !name.trim() || scopes.length === 0 || state === "minting"}
        className={`${FONT_MONO} rounded-full border px-5 py-2 text-sm font-medium transition disabled:opacity-40`}
        style={{ borderColor: state === "error" ? MAGENTA : GROUND, color: state === "error" ? MAGENTA : GROUND }}
      >
        {state === "minting" ? "cutting…" : state === "error" ? "mint failed" : "◈ mint key"}
      </button>
      {!live && (
        <p className={`${FONT_MONO} text-xs text-[#e9edff]/35`}>demo mode — minting needs the live server</p>
      )}

      {/* one-time secret reveal */}
      <AnimatePresence>
        {minted && (
          <motion.div
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
            className="fixed inset-0 z-[60] grid place-items-center bg-black/70 p-6 backdrop-blur-sm"
            role="dialog"
            aria-modal="true"
            aria-label="New key secret"
          >
            <motion.div
              initial={{ scale: 0.95, y: 10 }}
              animate={{ scale: 1, y: 0 }}
              className="w-full max-w-lg rounded-xl border p-6"
              style={{ borderColor: GROUND_DIM, background: "#0a090f" }}
            >
              <div className={LABEL} style={{ color: GROUND }}>
                key cut · shown exactly once
              </div>
              <h2 className={`${FONT_DISPLAY} mt-2 text-2xl font-semibold text-white`}>{minted.name}</h2>
              <p className={`${FONT_MONO} mt-1 text-xs text-[#e9edff]/45`}>
                scopes: {minted.scopes.join(", ")} · this secret is not stored — copy it now.
              </p>
              <div className={`${FONT_MONO} mt-4 select-all break-all rounded-lg border border-white/15 bg-white/[0.03] p-3 text-sm text-white`}>
                {minted.token}
              </div>
              <div className="mt-4 flex items-center gap-3">
                <button
                  onClick={() => {
                    void navigator.clipboard?.writeText(minted.token);
                    setCopied(true);
                  }}
                  className={`${FONT_MONO} rounded-full border px-4 py-1.5 text-sm transition`}
                  style={{ borderColor: GROUND, color: GROUND }}
                >
                  {copied ? "✓ copied" : "copy secret"}
                </button>
                <button
                  onClick={() => {
                    setMinted(null);
                    setCopied(false);
                    onMinted();
                  }}
                  className={`${FONT_MONO} rounded-full border border-white/15 px-4 py-1.5 text-sm text-[#e9edff]/60 transition hover:border-white/40 hover:text-white`}
                >
                  done — I stored it
                </button>
              </div>
            </motion.div>
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  );
}
