"use client";

/*
 * Design lab — three full main-page prototypes behind a tab switcher.
 * Selection persists in localStorage + ?variant= so a picked identity
 * survives reloads and can be shared as a link while we iterate.
 */

import { useEffect, useState } from "react";
import { AnimatePresence, motion } from "framer-motion";

import EegVariant from "./variants/EegVariant";
import InterferenceVariant from "./variants/InterferenceVariant";
import SpectrumVariant from "./variants/SpectrumVariant";
import SynapseVariant from "./variants/SynapseVariant";

const VARIANTS = [
  {
    id: "eeg",
    name: "EEG",
    blurb: "the page is the instrument · cursor injects spikes · fire a stimulus",
    Component: EegVariant,
    chip: "#6ef3c5",
  },
  {
    id: "spectrum",
    name: "Spectrum",
    blurb: "drag the dial δ→γ · wave, light and story tune together",
    Component: SpectrumVariant,
    chip: "#f5c451",
  },
  {
    id: "interference",
    name: "Interference",
    blurb: "teams as wave sources · drag emitters · phase-lock the contradiction",
    Component: InterferenceVariant,
    chip: "#ff5da2",
  },
  {
    id: "synapse",
    name: "Synapse (v1)",
    blurb: "round-1 reference · aurora glass",
    Component: SynapseVariant,
    chip: "#22d3ee",
  },
] as const;

type VariantId = (typeof VARIANTS)[number]["id"];

const STORAGE_KEY = "brainiac-design-variant";

export default function DesignLab() {
  const [active, setActive] = useState<VariantId>("eeg");

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
    <div className="relative">
      {/* floating switcher */}
      <div className="fixed bottom-5 left-1/2 z-50 -translate-x-1/2">
        <div
          className="flex items-center gap-1 rounded-full border border-white/15 bg-black/70 p-1.5 shadow-2xl backdrop-blur-xl"
          role="tablist"
          aria-label="Design variants"
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
                    layoutId="lab-pill"
                    className="absolute inset-0 rounded-full bg-white"
                    transition={{ type: "spring", bounce: 0.25, duration: 0.5 }}
                  />
                )}
                <span className="relative flex items-center gap-2">
                  <span
                    className="inline-block h-2 w-2 rounded-full"
                    style={{ background: v.chip }}
                  />
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

      <AnimatePresence mode="wait">
        <motion.div
          key={current.id}
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          exit={{ opacity: 0 }}
          transition={{ duration: 0.25 }}
        >
          <current.Component />
        </motion.div>
      </AnimatePresence>
    </div>
  );
}
