/*
 * Presentation helpers with no I/O — safe on either side of the boundary.
 *
 * formatAge lived in governance-api.ts, which is `import "server-only"`. That
 * was fine while the only caller was an operator server page, and it stopped
 * being fine the moment the public demo tour became one client-rendered surface:
 * a pure seconds→"3.2h" function is not a reason to drag a server-only module
 * into the browser bundle (it would throw at import).
 */

/** Seconds → the queue's age idiom: "just now", "26m", "1.4h", "3.2d". */
export function formatAge(secs: number): string {
  if (secs <= 0) return "just now";
  if (secs < 3600) return `${Math.round(secs / 60)}m`;
  if (secs < 86400) return `${(secs / 3600).toFixed(1)}h`;
  return `${(secs / 86400).toFixed(1)}d`;
}
