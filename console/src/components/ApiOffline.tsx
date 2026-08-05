import Button from "@/components/ui/Button";
import { FONT_MONO, LABEL, MAGENTA } from "@/design/theme";

/**
 * Rendered by feature pages when the brainiac REST server is unreachable, and
 * reused by the route error.tsx boundaries. Pass `onRetry` (the boundary's
 * `reset`) to surface a retry button; server-side callers omit it.
 */
export default function ApiOffline({
  error,
  onRetry,
}: {
  error?: string;
  onRetry?: () => void;
}) {
  return (
    <section className="mx-auto max-w-2xl px-6 py-24 text-center">
      <div className={LABEL} style={{ color: MAGENTA }}>
        signal lost
      </div>
      <h1 className="mt-3 text-3xl font-semibold tracking-tight text-white">
        The brainiac server is not answering.
      </h1>
      <p className={`${FONT_MONO} mt-4 text-sm leading-relaxed text-[var(--ink)]/55`}>
        Start it with <code className="text-[var(--gold)]">brainiac serve</code> and set{" "}
        <code className="text-[var(--gold)]">BRAINIAC_API_URL</code> /{" "}
        <code className="text-[var(--gold)]">BRAINIAC_API_TOKEN</code> for the console
        (console/.env.local), then reload.
      </p>
      {error && (
        <p className={`${FONT_MONO} mt-3 text-xs text-[var(--ink)]/30`}>{error}</p>
      )}
      {onRetry && (
        <Button
          variant="pill"
          size="md"
          accent={MAGENTA}
          onClick={onRetry}
          className="mt-6"
          // Full-strength edge, not the 0.4 wash the accent normally derives:
          // this is the only affordance on an error page and must not read as
          // secondary chrome.
          style={{ borderColor: MAGENTA }}
        >
          ↻ retry
        </Button>
      )}
    </section>
  );
}
