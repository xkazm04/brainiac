"use client";

/*
 * Cortex Map — consolidated from the 2026-07-10 prototype round with TWO
 * surviving lenses (Hemisphere was cut): Star Chart uncovers relationships
 * spatially; Depth of Field is the logical catalog. The toggle is product
 * UI, not prototype scaffolding — both views ship. The scale button swaps
 * in a deterministic 50-node mock for density checks and demos.
 */

import { useEffect, useMemo, useState } from "react";
import { AnimatePresence, motion } from "framer-motion";

import Button from "@/components/ui/Button";
import { band } from "@/design/theme";

import { makeLargeCortex, type CortexData } from "./cortex-data";
import DepthOfFieldView from "./views/DepthOfFieldView";
import StarChartView from "./views/StarChartView";

const VIEWS = [
  {
    id: "starchart",
    name: "Star Chart",
    blurb: "relationship lens · constellations reveal what binds",
    Component: StarChartView,
  },
  {
    id: "depth",
    name: "Depth of Field",
    blurb: "catalog lens · focus a card, the rest recedes",
    Component: DepthOfFieldView,
  },
] as const;

type ViewId = (typeof VIEWS)[number]["id"];

const STORAGE_KEY = "brainiac-cortex-view";

export default function CortexMap({ data }: { data: CortexData }) {
  const [active, setActive] = useState<ViewId>("starchart");
  const [large, setLarge] = useState(false);
  const largeData = useMemo(() => makeLargeCortex(), []);
  const shown = large ? largeData : data;

  useEffect(() => {
    const params = new URLSearchParams(window.location.search);
    const fromUrl = params.get("view");
    const stored = window.localStorage.getItem(STORAGE_KEY);
    const initial = fromUrl ?? stored;
    if (initial && VIEWS.some((v) => v.id === initial)) {
      setActive(initial as ViewId);
    }
    if (params.get("scale") === "large") setLarge(true);
  }, []);

  const pick = (id: ViewId) => {
    setActive(id);
    window.localStorage.setItem(STORAGE_KEY, id);
    const url = new URL(window.location.href);
    url.searchParams.set("view", id);
    window.history.replaceState(null, "", url.toString());
  };

  const pickScale = (next: boolean) => {
    setLarge(next);
    const url = new URL(window.location.href);
    if (next) url.searchParams.set("scale", "large");
    else url.searchParams.delete("scale");
    window.history.replaceState(null, "", url.toString());
  };

  const current = VIEWS.find((v) => v.id === active) ?? VIEWS[0];

  return (
    <div className="relative pb-24">
      <AnimatePresence mode="wait">
        <motion.div
          key={`${current.id}-${large ? "l" : "s"}`}
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          exit={{ opacity: 0 }}
          transition={{ duration: 0.2 }}
        >
          <current.Component data={shown} />
        </motion.div>
      </AnimatePresence>

      <div className="fixed bottom-5 left-1/2 z-50 -translate-x-1/2">
        <div
          className="flex items-center gap-1 rounded-full border border-white/15 bg-black/70 p-1.5 shadow-2xl backdrop-blur-xl"
          role="tablist"
          aria-label="Cortex Map lens"
        >
          {VIEWS.map((v) => {
            const selected = v.id === active;
            return (
              /* `quiet`, not `pill`: the capsule you see is the shared
                 layoutId pill sliding underneath, so the control itself must
                 stay chrome-less — a `pill` border would draw a second, static
                 outline around every tab and fight the animation. `selected` is
                 NOT passed: aria-selected is the correct state for role=tab, and
                 an aria-pressed beside it would announce two conflicting things.
                 `mono={false}` keeps the display face these tabs already used. */
              <Button
                key={v.id}
                variant="quiet"
                size="sm"
                mono={false}
                role="tab"
                aria-selected={selected}
                onClick={() => pick(v.id)}
                style={{ color: undefined }}
                className={`relative rounded-full px-4 py-2 text-sm font-medium ${
                  selected ? "text-black" : "text-white/70 hover:text-white"
                }`}
              >
                {selected && (
                  <motion.span
                    layoutId="cortex-view-pill"
                    className="absolute inset-0 rounded-full bg-white"
                    transition={{ type: "spring", bounce: 0.25, duration: 0.5 }}
                  />
                )}
                <span className="relative flex items-center gap-2">
                  <span className="inline-block h-2 w-2 rounded-full" style={{ background: band("gamma") }} />
                  {v.name}
                </span>
              </Button>
            );
          })}
        </div>
        <div className="mt-1.5 flex items-center justify-center gap-3 text-[11px] text-white/60 [text-shadow:0_1px_4px_rgba(0,0,0,0.9)]">
          <span>{current.blurb}</span>
          <span className="text-white/25">·</span>
          {/* Also `quiet` rather than `pill`, for a legibility reason: this chip
              floats over the map artwork, and its bg-black/50 backing is what
              keeps the label readable there. `pill` brings a shared
              hover:bg-white/5 wash that would replace that backing exactly when
              the pointer is on it. Keeping the capsule as classes costs one line
              and holds the original hover (border + text brighten, backing
              stays). `selected` supplies the aria-pressed that was hand-written;
              `color: undefined` lets the strip's inherited white/60 through. */}
          <Button
            variant="quiet"
            size="xs"
            mono={false}
            onClick={() => pickScale(!large)}
            selected={large}
            style={{ color: undefined }}
            className="rounded-full border border-white/20 bg-black/50 px-2.5 py-0.5 hover:border-white/50"
          >
            scale: {large ? "50 hubs · 7 teams (mock)" : `${data.overview.canonicals.length} hubs · ${data.overview.teams.length} teams`}
          </Button>
        </div>
      </div>
    </div>
  );
}
