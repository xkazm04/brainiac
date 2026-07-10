import { FONT_MONO, LABEL, MAGENTA } from "@/design/theme";

/** Rendered by feature pages when the brainiac REST server is unreachable. */
export default function ApiOffline({ error }: { error?: string }) {
  return (
    <section className="mx-auto max-w-2xl px-6 py-24 text-center">
      <div className={LABEL} style={{ color: MAGENTA }}>
        signal lost
      </div>
      <h1 className="mt-3 text-3xl font-semibold tracking-tight text-white">
        The brainiac server is not answering.
      </h1>
      <p className={`${FONT_MONO} mt-4 text-sm leading-relaxed text-[#e9edff]/55`}>
        Start it with <code className="text-[#f3c74f]">brainiac serve</code> and set{" "}
        <code className="text-[#f3c74f]">BRAINIAC_API_URL</code> /{" "}
        <code className="text-[#f3c74f]">BRAINIAC_API_TOKEN</code> for the console
        (console/.env.local), then reload.
      </p>
      {error && (
        <p className={`${FONT_MONO} mt-3 text-xs text-[#e9edff]/30`}>{error}</p>
      )}
    </section>
  );
}
