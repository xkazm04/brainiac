/*
 * Deterministic pseudo-randomness for scale mocks.
 *
 * Every generator that fabricates a large corpus for density checks MUST be
 * deterministic. Two reasons, both load-bearing:
 *
 *  1. Hydration. `Math.random()` produces different values on the server and in
 *     the browser, so a mock built with it renders one DOM on each side and
 *     React tears at hydration.
 *  2. Comparability. A prototype round compares variants against each other. If
 *     the corpus reshuffles per render, "this one feels denser" is measuring the
 *     mock, not the design.
 *
 * The graph module already carries its own copy of this hash (cortex-data.ts,
 * for makeLargeCortex) — this is the shared one for everything since.
 */

/** FNV-ish string hash → stable 0..1. */
export function hash01(s: string, salt = 0): number {
  let h = 2166136261 ^ salt;
  for (let i = 0; i < s.length; i++) {
    h ^= s.charCodeAt(i);
    h = Math.imul(h, 16777619);
  }
  return ((h >>> 0) % 10000) / 10000;
}

/** Stable integer in [0, n). */
export const hashInt = (s: string, n: number, salt = 0): number =>
  Math.floor(hash01(s, salt) * n) % n;

/** Stable pick from a list. */
export const pick = <T>(items: readonly T[], s: string, salt = 0): T =>
  items[hashInt(s, items.length, salt)];

/**
 * A fixed clock for generated corpora.
 *
 * Scale mocks must not call Date.now(): the value differs between the server
 * render and hydration, which is the same tear as Math.random(). Ages are
 * derived from this instant instead, so a generated queue is identical on both
 * sides and across reloads.
 */
export const MOCK_NOW = new Date("2026-07-15T09:00:00Z");

/** ISO timestamp `days` before the fixed clock. */
export const daysBefore = (days: number): string =>
  new Date(MOCK_NOW.getTime() - days * 86_400_000).toISOString();
