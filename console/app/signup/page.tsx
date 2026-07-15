import { isFirebaseConfigured } from "@/lib/firebase/config";
import { BG, FONT_DISPLAY, FONT_MONO, LABEL } from "@/design/theme";

import SignupClient from "./SignupClient";

export const metadata = { title: "Get a project · Brainiac" };

/*
 * Free tier: one Google account, one project, one key for your device.
 *
 * A PUBLIC surface (see design/routes.ts): it has to be reachable by someone who
 * has no passcode — that is the entire point. It never reads org data; the only
 * privileged thing behind it is the provisioning call, which happens server-side
 * after the sign-in is verified.
 */
export default function SignupPage() {
  const configured = isFirebaseConfigured();
  return (
    <main
      className="flex min-h-screen items-center justify-center px-6 py-16"
      style={{ background: BG }}
    >
      <div className="w-full max-w-md">
        <div className={LABEL} style={{ color: "rgba(233,237,255,0.4)" }}>
          brainiac cloud · free
        </div>
        <h1
          className={`${FONT_DISPLAY} mt-2 text-3xl font-semibold tracking-tight text-white`}
        >
          Give your agent a memory.
        </h1>
        <p className={`${FONT_MONO} mt-3 mb-7 text-sm leading-relaxed text-[#e9edff]/50`}>
          Sign in with Google and get a project of your own, plus a key your local
          agent uses to reach it. One project per account while this is free.
        </p>

        <SignupClient configured={configured} />

        <p className={`${FONT_MONO} mt-8 text-[11px] leading-relaxed text-[#e9edff]/30`}>
          This is not the operator console. Signing in here creates a workspace in
          the cloud service — it does not unlock an existing deployment&apos;s
          console, which has its own gate.
        </p>
      </div>
    </main>
  );
}
