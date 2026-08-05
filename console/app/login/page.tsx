import Link from "next/link";

import Button from "@/components/ui/Button";
import { Input } from "@/components/ui/Input";
import { isMisconfigured } from "@/lib/auth";
import {
  AMBER,
  BG,
  FONT_DISPLAY,
  FONT_MONO,
  GOLD,
  LABEL,
  MAGENTA,
  withAlpha,
} from "@/design/theme";

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
          <Link href="/demo" className="underline decoration-dotted underline-offset-4 hover:text-[var(--gold)]">
            public demo
          </Link>{" "}
          runs on the synthetic Meridian org and needs no key.
        </p>

        {unconfigured ? (
          <div
            className={`${FONT_MONO} mt-8 rounded-xl border p-5 text-xs leading-relaxed`}
            style={{
              borderColor: withAlpha(MAGENTA, 0.35),
              background: withAlpha(MAGENTA, 0.05),
              color: dim(0.7),
            }}
          >
            <div className={LABEL} style={{ color: MAGENTA }}>
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
            {/* `outline-none` used to live on this field, which took the
                keyboard focus ring with it — on the one control that is the
                entire front door. The shared Input carries the ring instead,
                and it cannot be switched off from a className. */}
            <Input
              id="passcode"
              name="passcode"
              type="password"
              size="md"
              label="console passcode"
              autoFocus
              autoComplete="current-password"
              invalid={err === "bad"}
            />

            {err === "bad" && (
              <p className={`${FONT_MONO} text-xs`} style={{ color: MAGENTA }}>
                That passcode is not correct.
              </p>
            )}

            {err === "throttled" && (
              <p className={`${FONT_MONO} text-xs`} style={{ color: AMBER }}>
                Too many attempts. Wait a few minutes and try again.
              </p>
            )}

            <Button
              type="submit"
              variant="solid"
              size="md"
              accent={GOLD}
              className="w-full hover:scale-[1.01]"
            >
              unlock the console
            </Button>
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
