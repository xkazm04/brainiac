"use client";

/*
 * Disputes — prototype round 1 (2026-07-13). Three directional lenses on the
 * same triage queue, behind the standard switcher:
 *
 *   Interference — physics lens: claims as waves cancelling the org's signal
 *   Half-life    — decay bench: what is dying before anyone re-verified it
 *   Testimony    — annotated recording: keyboard docket for a maintainer
 *                  who has forty of these
 *
 * Pick one (or fuse) and the round consolidates to a single view.
 */

import { useEffect, useState } from "react";
import { AnimatePresence, motion } from "framer-motion";

import { band } from "@/design/theme";

import type { DisputeData } from "./disputes-data";
import HalfLifeView from "./variants/HalfLifeView";
import InterferenceView from "./variants/InterferenceView";
import TestimonyView from "./variants/TestimonyView";

const VIEWS = [
  {
    id: "interference",
    name: "Interference",
    blurb: "physics lens · who is transmitting against us",
    Component: InterferenceView,
  },
  {
    id: "halflife",
    name: "Half-life",
    blurb: "decay bench · what dies before it was re-verified",
    Component: HalfLifeView,
  },
  {
    id: "testimony",
    name: "Testimony",
    blurb: "annotated take · keyboard docket, j/k + r/d/x",
    Component: TestimonyView,
  },
] as const;

type ViewId = (typeof VIEWS)[number]["id"];

const STORAGE_KEY = "brainiac-disputes-view";

export default function DisputeLab({ data }: { data: DisputeData }) {
  const [active, setActive] = useState<ViewId>("interference");

  useEffect(() => {
    const params = new URLSearchParams(window.location.search);
    const initial = params.get("view") ?? window.localStorage.getItem(STORAGE_KEY);
    if (initial && VIEWS.some((v) => v.id === initial)) setActive(initial as ViewId);
  }, []);

  const pick = (id: ViewId) => {
    setActive(id);
    window.localStorage.setItem(STORAGE_KEY, id);
    const url = new URL(window.location.href);
    url.searchParams.set("view", id);
    window.history.replaceState(null, "", url.toString());
  };

  const current = VIEWS.find((v) => v.id === active) ?? VIEWS[0];

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
          aria-label="Disputes lens"
        >
          {VIEWS.map((v) => {
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
                    layoutId="disputes-view-pill"
                    className="absolute inset-0 rounded-full bg-white"
                    transition={{ type: "spring", bounce: 0.25, duration: 0.5 }}
                  />
                )}
                <span className="relative flex items-center gap-2">
                  <span
                    className="inline-block h-2 w-2 rounded-full"
                    style={{ background: band("theta") }}
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
    </div>
  );
}
