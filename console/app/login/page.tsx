import Link from "next/link";

import { isMisconfigured } from "@/lib/auth";
import { BG, FONT_DISPLAY, FONT_MONO, GOLD, GOLD_GLOW, LABEL } from "@/design/theme";

import { login } from "./actions";

export const metadata = { title: "Brainiac — Console access" };

const dim = (a: number) => `rgba(233,237,255,${a})`;

export default async function LoginPage({
  searchParams,
}: {
  searchParams: Promise<{ err?: string; next?: string }>;
}) {
  const { err, next } = await searchParams;
  const unconfigured = err === "unconfigured" || isMisconfigured();

  return (
    <div
      className={`${FONT_DISPLAY} flex min-h-screen items-center justify-center px-6`}
      style={{ background: BG }}
    >
      <div className="w-full max-w-md">
        <Link href="/" className="flex items-center gap-3">
          <span className="text-xl font-semibold tracking-tight text-white">Brainiac</span>
          <span className={LABEL} style={{ color: GOLD }}>
            γ · binding console
          </span>
        </Link>

        <h1 className="mt-8 text-3xl font-semibold leading-tight tracking-tight text-white">
          This side holds real knowledge.
        </h1>
        <p className={`${FONT_MONO} mt-3 text-sm leading-relaxed`} style={{ color: dim(0.55) }}>
          The console reads and writes your organization&rsquo;s live memory — canonical
          decisions, provenance, the review queue. The{" "}
          <Link href="/demo" className="underline decoration-dotted underline-offset-4 hover:text-[#f3c74f]">
            public demo
          </Link>{" "}
          runs on the synthetic Meridian org and needs no key.
        </p>

        {unconfigured ? (
          <div
            className={`${FONT_MONO} mt-8 rounded-xl border p-5 text-xs leading-relaxed`}
            style={{ borderColor: "rgba(255,93,162,0.35)", background: "rgba(255,93,162,0.05)", color: dim(0.7) }}
          >
            <div className={LABEL} style={{ color: "#ff5da2" }}>
              console not configured
            </div>
            <p className="mt-3">
              No <code>CONSOLE_PASSCODE</code> is set, so the console is locked rather than
              served to the public. Set it in the deployment environment and restart:
            </p>
            <pre className="mt-3 overflow-x-auto rounded-lg bg-black/40 p-3" style={{ color: GOLD }}>
              CONSOLE_PASSCODE=&lt;a long random string&gt;
            </pre>
          </div>
        ) : (
          <form action={login} className="mt-8 space-y-4">
            <input type="hidden" name="next" value={next ?? "/console"} />
            <div>
              <label className={LABEL} style={{ color: dim(0.45) }} htmlFor="passcode">
                console passcode
              </label>
              <input
                id="passcode"
                name="passcode"
                type="password"
                autoFocus
                autoComplete="current-password"
                className={`${FONT_MONO} mt-2 w-full rounded-lg border bg-white/[0.03] px-4 py-3 text-sm text-white outline-none transition focus:border-[#f3c74f]`}
                style={{ borderColor: "rgba(233,237,255,0.14)" }}
              />
            </div>

            {err === "bad" && (
              <p className={`${FONT_MONO} text-xs`} style={{ color: "#ff5da2" }}>
                That passcode is not correct.
              </p>
            )}

            <button
              type="submit"
              className={`${FONT_MONO} w-full rounded-full px-6 py-3 text-sm font-semibold transition hover:scale-[1.01]`}
              style={{ background: GOLD, color: "#1a1405", boxShadow: `0 0 32px ${GOLD_GLOW}` }}
            >
              unlock the console
            </button>
          </form>
        )}

        <p className={`${FONT_MONO} mt-8 text-[11px] leading-relaxed`} style={{ color: dim(0.3) }}>
          A shared passcode, not an identity. It gates the console surface; the API&rsquo;s
          own per-principal tokens still carry row-level security on every query.
        </p>
      </div>
    </div>
  );
}
