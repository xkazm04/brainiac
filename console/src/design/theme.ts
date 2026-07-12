/*
 * Brainiac console theme — the fused identity (2026-07-10 design lab).
 *
 * Baseline: "Interference" (wave-physics hero, sine-spine story, emitter
 * interactivity). Theme + type strategy: "Spectrum" (Space Grotesk display,
 * JetBrains Mono microcopy, hsl band hues with glow).
 *
 * The soul of the theme is the EEG band system: every module of the console
 * is tuned to a band, and its accent hue follows. Gamma — the binding band —
 * is the brand color: in neuroscience gamma oscillations bind distributed
 * representations into one percept; Brainiac binds three teams' dialects
 * into one canonical graph.
 *
 * Fixed art direction → literal hexes/hsl on purpose (no light theme).
 */

export const FONT_DISPLAY = "font-[family-name:var(--font-display)]";
export const FONT_MONO = "font-[family-name:var(--font-mono)]";

export const BG = "#08070c";
export const PANEL = "rgba(255,255,255,0.03)";
export const INK = "#e9edff";
export const INK_DIM = "rgba(233,237,255,0.55)";
export const INK_FAINT = "rgba(233,237,255,0.35)";
export const BORDER = "rgba(233,237,255,0.10)";

/** EEG bands — hue per band; the module → band map below assigns accents. */
export const BAND_HUES = {
  delta: 262, // deep archive
  theta: 224, // reflection / contradiction work
  alpha: 190, // calm governance
  beta: 158, // active recall
  gamma: 46, // binding — the brand band
} as const;

export type BandKey = keyof typeof BAND_HUES;

export const MODULE_BAND: Record<string, BandKey> = {
  home: "gamma",
  reviews: "alpha",
  graph: "gamma",
  analytics: "beta",
  demo: "beta",
  memories: "delta",
  ingest: "theta",
};

export const band = (key: BandKey, lightness = 68, alpha = 1) =>
  `hsla(${BAND_HUES[key]}, 90%, ${lightness}%, ${alpha})`;

export const bandGlow = (key: BandKey, alpha = 0.35) =>
  `hsla(${BAND_HUES[key]}, 90%, 60%, ${alpha})`;

/** Brand accents (gamma = canonical/constructive; magenta = contradiction). */
export const GOLD = band("gamma");
export const GOLD_GLOW = bandGlow("gamma");
export const MAGENTA = "#ff5da2";
export const MAGENTA_GLOW = "rgba(255,93,162,0.35)";

/** Shared micro-typography: uppercase tracked mono label. */
export const LABEL = `${FONT_MONO} text-[11px] uppercase tracking-[0.2em]`;

/**
 * Motion policy:
 * - Hero/brand surfaces (home field) may run ambient canvas motion — the
 *   wave IS the brand — always behind a prefers-reduced-motion static frame.
 * - Utility pages (reviews, graph, analytics): entry animations and
 *   hover/click-gated transitions only. No infinite loops.
 */
