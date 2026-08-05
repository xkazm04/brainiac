"use client";

import { useState } from "react";

import { FUNNEL } from "@/analytics/config";
import { track } from "@/analytics/track";
import Button from "@/components/ui/Button";
import { signInWithGoogle } from "@/lib/firebase/client";
import { FONT_MONO, GOLD, INK_DIM, LABEL, MAGENTA } from "@/design/theme";

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
      const outcome = await claimProjectWithGoogle(idToken, issueKey);
      setResult(outcome);
      // Funnel step 2 of 3. Fired on the SERVER-VERIFIED outcome, not on the
      // button press: a Google popup that succeeded but whose token failed
      // verification, or whose provisioning call errored, is not a signup. The
      // `created` prop separates a first project from a returning account
      // re-issuing a key — the funnel needs the former, and counting the two
      // together would silently inflate conversion. No identifiers ride along.
      if (outcome.ok) {
        track(FUNNEL.signupComplete, { created: outcome.created === true });
      }
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
      <Button
        variant="solid"
        size="md"
        accent={GOLD}
        onClick={() => start(false)}
        disabled={busy}
        className="w-full hover:scale-[1.01]"
      >
        {busy ? "…" : "Continue with Google"}
      </Button>

      {result && !result.ok && (
        <p className={`${FONT_MONO} text-xs`} style={{ color: MAGENTA }}>
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
                <Button
                  variant="pill"
                  size="sm"
                  accent={copied ? GOLD : undefined}
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
                  className="px-4"
                  // Type size through style, not `text-xs`: Tailwind resolves
                  // competing font-size utilities by stylesheet order, so the
                  // primitive's text-sm would beat it about as often as not.
                  style={{ fontSize: "0.75rem", color: copied ? GOLD : INK_DIM }}
                >
                  {copied ? "✓ copied" : "copy key"}
                </Button>
              </div>
              <p className={`${FONT_MONO} mt-1 text-[11px] leading-relaxed text-[#e9edff]/40`}>
                Point your local agent at this project by setting{" "}
                <span className="text-[#e9edff]/70">BRAINIAC_TOKEN</span> to the key above and{" "}
                <span className="text-[#e9edff]/70">BRAINIAC_API_URL</span> to this service.
              </p>
            </div>
          ) : (
            <Button
              variant="pill"
              size="sm"
              onClick={() => start(true)}
              disabled={busy}
              className="self-start px-4"
              style={{ fontSize: "0.75rem" }}
            >
              issue a new device key
            </Button>
          )}
        </div>
      )}
    </div>
  );
}
