import { describe, expect, it } from "vitest";

import {
  CONTRA_HEADING_ID,
  resolveFocusIndex,
  stepFocus,
  STATUS_TABS,
} from "./review-surface";

/*
 * The rail's cursor, tested as the safety property it is.
 *
 * These run in vitest's node environment (no DOM, no React), which is exactly
 * enough: the bug being pinned is not a rendering bug. It is that the cursor was
 * an INDEX into a list that the server revalidates underneath it — so the
 * arithmetic below IS the bug surface, and it is pure.
 */

const rail = (...ids: string[]) => ids.map((id) => ({ id }));

describe("resolveFocusIndex", () => {
  it("defaults to the front of the rail", () => {
    expect(resolveFocusIndex(rail("a", "b", "c"), null)).toBe(0);
  });

  it("reports -1 when there is nothing to focus", () => {
    expect(resolveFocusIndex([], null)).toBe(-1);
    expect(resolveFocusIndex([], "a")).toBe(-1);
  });

  it("finds the focused row wherever it sits", () => {
    expect(resolveFocusIndex(rail("a", "b", "c"), "c")).toBe(2);
  });

  /*
   * THE REGRESSION. An index cursor pointed at slot 2 and stayed pointed at
   * slot 2 while the list shifted under it — so the pane silently swapped to a
   * claim the operator had never read, and `a` would have signed it.
   *
   * Against the pre-fix code this is the whole bug: focus === 2 both before and
   * after, and matched[2] is "c" before the refresh and "d" after it.
   */
  it("follows the focused claim when a row above it is decided away", () => {
    const before = rail("a", "b", "c", "d");
    const focusId = before[2].id; // "c" — the claim the operator is reading

    expect(resolveFocusIndex(before, focusId)).toBe(2);

    // "b" gets approved elsewhere and leaves the queue; everything after shifts up.
    const after = rail("a", "c", "d");

    // The cursor moved with the claim...
    expect(resolveFocusIndex(after, focusId)).toBe(1);
    // ...and the pane still shows the SAME claim, not the stranger now at slot 2.
    expect(after[resolveFocusIndex(after, focusId)].id).toBe("c");
    expect(after[2].id).toBe("d"); // what an index cursor would have re-pointed at
  });

  it("goes to the front — never to a neighbour — when the focused claim is gone", () => {
    const after = rail("a", "b", "d");
    // "c" was decided. The cursor must not silently inherit slot 2 ("d").
    expect(resolveFocusIndex(after, "c")).toBe(0);
  });

  it("survives a filter that shrinks the rail under the cursor", () => {
    const all = rail("a", "b", "c", "d", "e");
    const focusId = "e";
    expect(resolveFocusIndex(all, focusId)).toBe(4);
    // A team filter leaves two rows, one of them still the focused claim.
    expect(resolveFocusIndex(rail("b", "e"), focusId)).toBe(1);
  });
});

describe("stepFocus", () => {
  it("walks the rail by id", () => {
    const r = rail("a", "b", "c");
    expect(stepFocus(r, "a", 1)).toBe("b");
    expect(stepFocus(r, "c", -1)).toBe("b");
  });

  it("clamps at both ends rather than wrapping or falling off", () => {
    const r = rail("a", "b", "c");
    expect(stepFocus(r, "c", 1)).toBe("c");
    expect(stepFocus(r, "a", -1)).toBe("a");
  });

  it("steps from the front when nothing is focused yet", () => {
    expect(stepFocus(rail("a", "b"), null, 1)).toBe("b");
  });

  it("has nowhere to go on an empty rail", () => {
    expect(stepFocus([], null, 1)).toBeNull();
  });

  /*
   * The keyboard-approve flow: `a` signs the focused claim, then steps the
   * cursor to the NEXT id — computed against the list as it is NOW, before the
   * decided row is revalidated away. That is what keeps the operator's next
   * keystroke aimed where they are looking.
   */
  it("advances to the next id before the decided row disappears", () => {
    const before = rail("a", "b", "c");
    const next = stepFocus(before, "a", 1);
    expect(next).toBe("b");
    // "a" is approved and leaves the queue.
    const after = rail("b", "c");
    expect(after[resolveFocusIndex(after, next)].id).toBe("b");
  });
});

describe("CONTRA_HEADING_ID", () => {
  /*
   * The status tabs are links carrying a fragment. If that fragment stops
   * matching the heading the rail renders, every tab click round-trips and then
   * scrolls nowhere — silently, because a dead fragment is not an error. Both
   * sides import this constant; this pins the value the markup was written for.
   */
  it("is the id the contradictions heading renders", () => {
    expect(CONTRA_HEADING_ID).toBe("wl-contra-h");
  });
});

describe("STATUS_TABS", () => {
  it("is the allow-list the route validates ?cstatus= against", () => {
    expect(STATUS_TABS.map((t) => t.key)).toEqual([
      "open",
      "resolved_supersede",
      "resolved_coexist",
      "dismissed",
      "all",
    ]);
  });
});
