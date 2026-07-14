/**
 * Humanize an age in seconds. Leaders read "2d 16h", not "232000".
 *
 * Lives on its own because KB4 gave it a second, heavier job: it renders
 * `oldest_dirty_secs`, the propagation SLA. The product's promise is that a
 * resolved contradiction reaches every page automatically; this number is what
 * says whether "automatically" means minutes or means never. A number that
 * load-bearing gets a test.
 */
export const age = (secs: number): string => {
  if (secs <= 0) return "—";
  const d = Math.floor(secs / 86_400);
  const h = Math.floor((secs % 86_400) / 3_600);
  if (d > 0) return `${d}d ${h}h`;
  const m = Math.floor((secs % 3_600) / 60);
  return h > 0 ? `${h}h ${m}m` : `${m}m`;
};

/**
 * The verdict on propagation, in the product's own terms. Zero dirty pages is
 * the only state where the promise is being kept outright; everything else is
 * graded by how long the oldest page has been behind the corpus.
 */
export const propagationVerdict = (
  pagesDirty: number,
  oldestDirtySecs: number,
): { verdict: string; tone: "good" | "watch" | "bad" } => {
  if (pagesDirty === 0)
    return { verdict: "every page is current with the corpus", tone: "good" };
  if (oldestDirtySecs < 3_600)
    return { verdict: `recomposing — oldest is ${age(oldestDirtySecs)} behind`, tone: "good" };
  if (oldestDirtySecs < 86_400)
    return { verdict: `lagging — oldest is ${age(oldestDirtySecs)} behind`, tone: "watch" };
  return {
    verdict: `stalled — a page has been ${age(oldestDirtySecs)} behind the corpus`,
    tone: "bad",
  };
};
