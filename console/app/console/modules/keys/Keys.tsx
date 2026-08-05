"use client";

/*
 * Keys — consolidated from the 2026-07-13 prototype round ("Ground
 * Control" won over Keyring and Blast Radius; the radius rings live on
 * inside the mint panel). Mental model: the operations desk.
 * A dense management table (name, prefix, scopes, usage, status) beside
 * the mint panel. No metaphor at the surface — the admin's daily view.
 */

import { useState } from "react";

import type { KeysData } from "./keys-data";
import { refreshTokens, revokeKey } from "./keys-data";
import { fmtAgo, MintPanel } from "./KeyShared";
import Button from "@/components/ui/Button";
import { FONT_DISPLAY, FONT_MONO, GROUND, LABEL, MAGENTA } from "@/design/theme";
// The onboarding bootstrap used to be a local `OnboardPanel` here. It is now
// shared, because the console's first-run front door needs to show the SAME
// on-ramp — this was the good one, it was simply buried behind `?m=keys`, one
// of fourteen modules with nothing pointing at it.
import OnboardCommand from "@/onboarding/OnboardCommand";

export default function Keys({ data }: { data: KeysData }) {
  const [tokens, setTokens] = useState(data.tokens);
  const [confirming, setConfirming] = useState<string | null>(null);
  const [revokeError, setRevokeError] = useState<string | null>(null);

  const reload = () => {
    if (data.live) void refreshTokens().then(setTokens).catch(() => undefined);
  };
  const revoke = async (id: string) => {
    if (!data.live) return;
    setRevokeError(null);
    try {
      await revokeKey(id);
    } catch {
      // A swallowed revoke is dangerous: the operator believes a compromised key
      // is dead when it is still active. Surface it and keep the confirm open.
      setRevokeError("Revoke failed — the key may still be active. Try again.");
      return;
    }
    setConfirming(null);
    reload();
  };

  const active = tokens.filter((t) => !t.revoked_at);
  const revoked = tokens.filter((t) => t.revoked_at);

  return (
    <div className="mx-auto max-w-6xl px-6 py-6">
      <div className={LABEL} style={{ color: GROUND }}>
        ground · keys · control desk
      </div>
      <h1 className={`${FONT_DISPLAY} mt-1 text-3xl font-semibold tracking-tight text-white`}>
        Who can read the org&apos;s mind.
      </h1>

      <div className="mt-5 grid gap-6 lg:grid-cols-[1.2fr_0.8fr]">
        {/* the ledger */}
        <div>
          <div className={`${FONT_MONO} overflow-hidden rounded-xl border border-white/10`}>
            <div className={`${LABEL} grid grid-cols-[1fr_120px_110px_120px_80px] gap-3 border-b border-white/10 bg-white/[0.02] px-4 py-2.5`} style={{ color: "rgba(233,237,255,0.4)" }}>
              <span>key</span>
              <span>scopes</span>
              <span>last used</span>
              <span>created</span>
              <span></span>
            </div>
            {[...active, ...revoked].map((t) => (
              <div
                key={t.id}
                className={`grid grid-cols-[1fr_120px_110px_120px_80px] items-center gap-3 border-b border-white/[0.05] px-4 py-2.5 text-sm ${t.revoked_at ? "opacity-40" : ""}`}
              >
                <div className="min-w-0">
                  <div className="truncate text-[var(--ink)]/85">{t.name}</div>
                  <div className="truncate text-[10px] tracking-wider text-[var(--ink)]/35">
                    {t.prefix}
                    {t.project_name ? (
                      <span style={{ color: GROUND }}> · {t.project_name}</span>
                    ) : (
                      " · org-wide"
                    )}
                  </div>
                </div>
                <span className="text-xs text-[var(--ink)]/60">
                  {t.scopes.map((s) => (
                    <span key={s} className="mr-1" style={{ color: s === "admin" ? MAGENTA : undefined }}>
                      {s}
                    </span>
                  ))}
                </span>
                <span className="text-xs text-[var(--ink)]/45">{fmtAgo(t.last_used_at)}</span>
                <span className="text-xs text-[var(--ink)]/45">{fmtAgo(t.created_at)}</span>
                {t.revoked_at ? (
                  <span className={`${LABEL} text-right`} style={{ color: MAGENTA }}>
                    revoked
                  </span>
                ) : confirming === t.id ? (
                  <Button
                    variant="pill"
                    size="xs"
                    accent={MAGENTA}
                    onClick={() => revoke(t.id)}
                    className="px-2 py-0.5 text-right text-[10px] uppercase tracking-widest"
                    style={{ borderColor: MAGENTA }}
                  >
                    sure?
                  </Button>
                ) : (
                  // The resting state changes colour on hover, and an inline
                  // style always beats a `hover:` utility — so the accent stays
                  // in classes here and controlStyle's inline colours are
                  // explicitly cleared. (The capsule, focus ring and disabled
                  // treatment still come from the primitive.)
                  <Button
                    variant="pill"
                    size="xs"
                    onClick={() => setConfirming(t.id)}
                    disabled={!data.live}
                    className="border-white/12 px-2 py-0.5 text-right text-[10px] uppercase tracking-widest text-[var(--ink)]/40 hover:border-[var(--magenta)]/60 hover:text-[var(--magenta)]"
                    style={{ borderColor: undefined, color: undefined }}
                  >
                    revoke
                  </Button>
                )}
              </div>
            ))}
            {tokens.length === 0 && (
              <p className={`${FONT_MONO} py-10 text-center text-sm text-[var(--ink)]/35`}>no keys cut yet</p>
            )}
          </div>
          {revokeError && (
            <div className={`${FONT_MONO} mt-2 text-sm text-[var(--amber)]`}>{revokeError}</div>
          )}
          <div className={`${LABEL} mt-2`} style={{ color: "rgba(233,237,255,0.3)" }}>
            {active.length} active · {revoked.length} revoked · secrets never stored, only sha256
            {!data.live && " · demo data"}
          </div>
        </div>

        {/* mint + onboarding bootstrap */}
        <div className="space-y-6 lg:sticky lg:top-4 lg:self-start">
          <div className="rounded-xl border border-white/10 bg-white/[0.015] p-5">
            <MintPanel users={data.users} live={data.live} onMinted={reload} />
          </div>
          <OnboardCommand apiUrl={data.apiUrl} />
        </div>
      </div>
    </div>
  );
}
