"use client";

import { useState } from "react";

import { signInWithGoogle } from "@/lib/firebase/client";
import { FONT_DISPLAY, FONT_MONO, GOLD, GOLD_GLOW, LABEL } from "@/design/theme";

import { claimProjectWithGoogle, type SignupResult } from "./actions";

/*
 * The free-tier front door: one button, one project, one key.
 *
 * Deliberately NOT the operator console's login. Signing in here grants no
 * console access — it provisions a workspace in the cloud service and hands back
 * a key for a local device. The two gates are separate on purpose.
 */

export default function SignupClient({ configured }: { configured: boolean }) {
  const [busy, setBusy] = useState(false);
  const [result, setResult] = useState<SignupResult | null>(null);
  const [copied, setCopied] = useState(false);

  const start = async (issueKey: boolean) => {
    setBusy(true);
    setResult(null);
    setCopied(false);
    try {
      const idToken = await signInWithGoogle();
      if (!idToken) {
        setResult({ ok: false, message: "Google sign-in is not configured here." });
        return;
      }
      setResult(await claimProjectWithGoogle(idToken, issueKey));
    } catch (e) {
      // A popup the user closed is the common case and is not an error worth
      // shouting about.
      const msg = e instanceof Error ? e.message : String(e);
      setResult(
        /popup|cancel/i.test(msg)
          ? { ok: false, message: "Sign-in was cancelled." }
          : { ok: false, message: msg },
      );
    } finally {
      setBusy(false);
    }
  };

  if (!configured) {
    return (
      <p className={`${FONT_MONO} text-sm text-[#e9edff]/50`}>
        Google sign-in is not configured on this deployment. Set
        NEXT_PUBLIC_FIREBASE_API_KEY, _AUTH_DOMAIN, _PROJECT_ID and _APP_ID.
      </p>
    );
  }

  return (
    <div className="flex flex-col gap-5">
      <button
        onClick={() => start(false)}
        disabled={busy}
        className={`${FONT_MONO} w-full rounded-full px-6 py-3 text-sm font-semibold transition hover:scale-[1.01] disabled:opacity-50`}
        style={{ background: GOLD, color: "#1a1405", boxShadow: `0 0 32px ${GOLD_GLOW}` }}
      >
        {busy ? "…" : "Continue with Google"}
      </button>

      {result && !result.ok && (
        <p className={`${FONT_MONO} text-xs`} style={{ color: "#ff5da2" }}>
          {result.message}
        </p>
      )}

      {result?.ok && (
        <div className="flex flex-col gap-4 rounded-xl border border-white/10 bg-white/[0.02] p-5">
          <div>
            <div className={LABEL} style={{ color: GOLD }}>
              {result.created ? "project created" : "your project"}
            </div>
            <p className={`${FONT_MONO} mt-1 text-xs text-[#e9edff]/50`}>{result.message}</p>
            <p className={`${FONT_MONO} mt-1 text-xs text-[#e9edff]/35`}>id {result.projectId}</p>
          </div>

          {result.apiKey ? (
            <div className="flex flex-col gap-2">
              <div className={LABEL}>device key — copy it now, it is shown once</div>
              <code className={`${FONT_MONO} select-all break-all rounded-lg border border-white/15 bg-white/[0.03] p-3 text-xs text-white`}>
                {result.apiKey}
              </code>
              <div className="flex items-center gap-3">
                <button
                  onClick={async () => {
                    // Only claim success when the write resolves — this secret is
                    // not stored anywhere and a false ✓ loses it for good.
                    try {
                      await navigator.clipboard.writeText(result.apiKey!);
                      setCopied(true);
                    } catch {
                      setCopied(false);
                    }
                  }}
                  className={`${FONT_MONO} rounded-full border border-white/20 px-4 py-1.5 text-xs text-[#e9edff]/70 transition hover:border-white/50 hover:text-white`}
                >
                  {copied ? "✓ copied" : "copy key"}
                </button>
              </div>
              <p className={`${FONT_MONO} mt-1 text-[11px] leading-relaxed text-[#e9edff]/40`}>
                Point your local agent at this project by setting{" "}
                <span className="text-[#e9edff]/70">BRAINIAC_TOKEN</span> to the key above and{" "}
                <span className="text-[#e9edff]/70">BRAINIAC_API_URL</span> to this service.
              </p>
            </div>
          ) : (
            <button
              onClick={() => start(true)}
              disabled={busy}
              className={`${FONT_MONO} self-start rounded-full border border-white/20 px-4 py-1.5 text-xs text-[#e9edff]/70 transition hover:border-white/50 hover:text-white disabled:opacity-50`}
            >
              issue a new device key
            </button>
          )}
        </div>
      )}
    </div>
  );
}
