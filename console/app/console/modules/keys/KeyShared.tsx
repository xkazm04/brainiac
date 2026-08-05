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
import Button from "@/components/ui/Button";
import { Input, Select } from "@/components/ui/Input";
import { band, FONT_DISPLAY, FONT_MONO, GROUND, GROUND_DIM, LABEL, MAGENTA } from "@/design/theme";

import { demoPreview, fetchPreview, mintKey, SCOPES } from "./keys-data";

// Nullable-and-optional — see fmtDate: the generated types allow undefined.
export function fmtAgo(iso: string | null | undefined): string {
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
      {/* The mint form is unlabelled by design — the section header above is the
          only chrome that fits the rail — so each field carries an aria-label.
          `focus:border-white/40` is gone with the rest: the ring is the focus
          signal now, and it survives an `outline-none` further down a class. */}
      <Input
        value={name}
        onChange={(e) => setName(e.target.value)}
        placeholder="key name — who or what will hold it"
        disabled={!live}
        aria-label="Key name"
      />
      <div className="flex flex-wrap items-center gap-2">
        <Select
          value={userId}
          onChange={(e) => setUserId(e.target.value)}
          disabled={!live && users.length === 0}
          fullWidth={false}
          className="px-3"
          aria-label="Acts as user"
        >
          {users.map((u) => (
            <option key={u.id} value={u.id}>
              {u.email} · {u.teams.map((t) => t.name).join("+") || "no team"}
            </option>
          ))}
        </Select>
        {/* Scope toggles. `selected` emits aria-pressed; the pressed *look*
            stays in classes because admin reads magenta and the rest read white
            — a single accent could not say both. controlStyle's inline colours
            are cleared so the class pair (and its hover) still wins. */}
        {SCOPES.map((s) => (
          <Button
            key={s}
            variant="pill"
            size="xs"
            selected={scopes.includes(s)}
            onClick={() => toggleScope(s)}
            className={`px-3 text-xs ${
              scopes.includes(s)
                ? s === "admin"
                  ? "border-[#ff5da2]/70 text-[#ff5da2]"
                  : "border-white/50 text-white"
                : "border-white/12 text-[#e9edff]/40 hover:border-white/30"
            }`}
            style={{ borderColor: undefined, color: undefined, background: undefined }}
          >
            {s}
          </Button>
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

      <Button
        variant="pill"
        size="md"
        accent={state === "error" ? MAGENTA : GROUND}
        onClick={mint}
        disabled={!live || !name.trim() || scopes.length === 0 || state === "minting"}
        className="font-medium"
        style={{ borderColor: state === "error" ? MAGENTA : GROUND }}
      >
        {state === "minting" ? "cutting…" : state === "error" ? "mint failed" : "◈ mint key"}
      </Button>
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
                <Button
                  variant="pill"
                  size="sm"
                  accent={GROUND}
                  onClick={async () => {
                    // Only claim "copied" when the write actually succeeds. This is
                    // a one-time secret (not stored), so a false ✓ on a blocked or
                    // unavailable clipboard would lose it silently. On failure the
                    // secret stays visible in the select-all box above.
                    try {
                      await navigator.clipboard.writeText(minted.token);
                      setCopied(true);
                    } catch {
                      setCopied(false);
                    }
                  }}
                  className="px-4"
                  style={{ borderColor: GROUND }}
                >
                  {copied ? "✓ copied" : "copy secret"}
                </Button>
                {/* Dismiss is the quieter of the pair and brightens on hover, so
                    its colours stay in classes — an inline colour cannot be
                    beaten by a `hover:` utility. */}
                <Button
                  variant="pill"
                  size="sm"
                  onClick={() => {
                    setMinted(null);
                    setCopied(false);
                    onMinted();
                  }}
                  className="border-white/15 px-4 text-[#e9edff]/60 hover:border-white/40 hover:text-white"
                  style={{ borderColor: undefined, color: undefined }}
                >
                  done — I stored it
                </Button>
              </div>
            </motion.div>
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  );
}
