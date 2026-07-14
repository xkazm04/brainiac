/*
 * One honest offline/demo mechanism for the console's read surfaces.
 *
 * Every page that swaps to fixture data when the brainiac server is
 * unreachable routes that swap through withDemoFallback, and renders a
 * page-level <DemoBanner /> whenever it comes back live:false — so a
 * maintainer never sees fabricated tokens / memories / graph nodes without a
 * prominent warning. (The inner components' own "· demo data" microcopy is
 * kept, but no page relies on it as the only signal.)
 *
 * Deliberate exception: reviews. It is a write surface (approve / reject /
 * resolve), so a fabricated queue wired to real actions would be dangerous.
 * It does NOT use this helper — it hard-stops with <ApiOffline /> instead.
 */

export interface DemoResult<T> {
  data: T;
  /** True only when the live fetch succeeded. */
  live: boolean;
}

/**
 * Run the live fetch; on any throw, fall back to the demo fixture and report
 * live:false. The returned `live` is authoritative for the page-level banner.
 */
export async function withDemoFallback<T>(
  fetchLive: () => Promise<T>,
  demo: T,
): Promise<DemoResult<T>> {
  try {
    return { data: await fetchLive(), live: true };
  } catch (e) {
    // Log the underlying failure so a real error — a bad token (401), an RLS
    // misconfig (403), or a server 500 — is diagnosable in the server logs instead
    // of vanishing silently behind the demo banner. The page still degrades to
    // fixtures + <DemoBanner>; distinguishing "offline" from "misconfigured" in the
    // banner itself is a per-page follow-up.
    console.error("[demo-fallback] live fetch failed, serving fixtures:", e);
    return { data: demo, live: false };
  }
}
