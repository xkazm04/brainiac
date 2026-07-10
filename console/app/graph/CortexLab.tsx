"use client";

/*
 * Cortex Map prototype switcher (/prototype round 1). Three mental models
 * over the same multi-level graph data. Removed at consolidation.
 */

import { useEffect, useState } from "react";
import { AnimatePresence, motion } from "framer-motion";

import { band } from "@/design/theme";

import type { CortexData } from "./cortex-data";
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

  useEffect(() => {
    const fromUrl = new URLSearchParams(window.location.search).get("variant");
    const stored = window.localStorage.getItem(STORAGE_KEY);
    const initial = fromUrl ?? stored;
    if (initial && VARIANTS.some((v) => v.id === initial)) {
      setActive(initial as VariantId);
    }
  }, []);

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
          key={current.id}
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          exit={{ opacity: 0 }}
          transition={{ duration: 0.2 }}
        >
          <current.Component data={data} />
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
        <div className="mt-1.5 text-center text-[11px] text-white/60 [text-shadow:0_1px_4px_rgba(0,0,0,0.9)]">
          {current.blurb}
        </div>
      </div>
    </div>
  );
}
