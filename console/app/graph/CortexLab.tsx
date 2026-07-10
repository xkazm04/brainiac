"use client";

/*
 * Cortex Map prototype switcher (/prototype round 1). Three mental models
 * over the same multi-level graph data. Removed at consolidation.
 */

import { useEffect, useMemo, useState } from "react";
import { AnimatePresence, motion } from "framer-motion";

import { band } from "@/design/theme";

import { makeLargeCortex, type CortexData } from "./cortex-data";
import DepthOfFieldVariant from "./variants/DepthOfFieldVariant";
import HemisphereVariant from "./variants/HemisphereVariant";
import StarChartVariant from "./variants/StarChartVariant";

const VARIANTS = [
  {
    id: "hemisphere",
    name: "Hemisphere",
    blurb: "anatomy · team lobes · binding pulls hubs to center",
    Component: HemisphereVariant,
  },
  {
    id: "starchart",
    name: "Star Chart",
    blurb: "astronomy · magnitude = memories · constellations on focus",
    Component: StarChartVariant,
  },
  {
    id: "depth",
    name: "Depth of Field",
    blurb: "focal planes · grid recedes, neighborhood comes forward",
    Component: DepthOfFieldVariant,
  },
] as const;

type VariantId = (typeof VARIANTS)[number]["id"];

const STORAGE_KEY = "brainiac-cortex-variant";

export default function CortexLab({ data }: { data: CortexData }) {
  const [active, setActive] = useState<VariantId>("hemisphere");
  // Scale stress-toggle: swap the real org for a deterministic 50-node,
  // 7-team mock to evaluate how each metaphor degrades with density.
  const [large, setLarge] = useState(false);
  const largeData = useMemo(() => makeLargeCortex(), []);
  const shown = large ? largeData : data;

  useEffect(() => {
    const params = new URLSearchParams(window.location.search);
    const fromUrl = params.get("variant");
    const stored = window.localStorage.getItem(STORAGE_KEY);
    const initial = fromUrl ?? stored;
    if (initial && VARIANTS.some((v) => v.id === initial)) {
      setActive(initial as VariantId);
    }
    if (params.get("scale") === "large") setLarge(true);
  }, []);

  const pickScale = (next: boolean) => {
    setLarge(next);
    const url = new URL(window.location.href);
    if (next) url.searchParams.set("scale", "large");
    else url.searchParams.delete("scale");
    window.history.replaceState(null, "", url.toString());
  };

  const pick = (id: VariantId) => {
    setActive(id);
    window.localStorage.setItem(STORAGE_KEY, id);
    const url = new URL(window.location.href);
    url.searchParams.set("variant", id);
    window.history.replaceState(null, "", url.toString());
  };

  const current = VARIANTS.find((v) => v.id === active) ?? VARIANTS[0];

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
          aria-label="Cortex Map variants"
        >
          {VARIANTS.map((v) => {
            const selected = v.id === active;
            return (
              <button
                key={v.id}
                role="tab"
                aria-selected={selected}
                onClick={() => pick(v.id)}
                className={`relative rounded-full px-4 py-2 text-sm font-medium transition ${
                  selected ? "text-black" : "text-white/70 hover:text-white"
                }`}
              >
                {selected && (
                  <motion.span
                    layoutId="cortex-pill"
                    className="absolute inset-0 rounded-full bg-white"
                    transition={{ type: "spring", bounce: 0.25, duration: 0.5 }}
                  />
                )}
                <span className="relative flex items-center gap-2">
                  <span className="inline-block h-2 w-2 rounded-full" style={{ background: band("gamma") }} />
                  {v.name}
                </span>
              </button>
            );
          })}
        </div>
        <div className="mt-1.5 flex items-center justify-center gap-3 text-[11px] text-white/60 [text-shadow:0_1px_4px_rgba(0,0,0,0.9)]">
          <span>{current.blurb}</span>
          <span className="text-white/25">·</span>
          <button
            onClick={() => pickScale(!large)}
            className="rounded-full border border-white/20 bg-black/50 px-2.5 py-0.5 transition hover:border-white/50 hover:text-white"
            aria-pressed={large}
          >
            scale: {large ? "50 hubs · 7 teams (mock)" : `${data.overview.canonicals.length} hubs · ${data.overview.teams.length} teams`}
          </button>
        </div>
      </div>
    </div>
  );
}
