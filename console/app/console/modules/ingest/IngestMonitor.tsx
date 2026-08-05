"use client";

/*
 * Ingest Monitor — consolidated from the 2026-07-12 prototype round with
 * TWO coexisting lenses (Conduction was cut): Manifest is the triage
 * worklist; Airlock answers "where is everything right now". Both ship
 * until real usage proves one superior. The load button swaps in a
 * deterministic 40-source mock for density checks and demos.
 */

import { useEffect, useMemo, useState } from "react";
import { AnimatePresence, motion } from "framer-motion";

import Button from "@/components/ui/Button";
import { band } from "@/design/theme";

import { makeLargeIngest, type IngestData } from "./ingest-data";
import AirlockView from "./views/AirlockView";
import ManifestView from "./views/ManifestView";

const VIEWS = [
  {
    id: "manifest",
    name: "Manifest",
    blurb: "triage lens · attention floats first",
    Component: ManifestView,
  },
  {
    id: "airlock",
    name: "Airlock",
    blurb: "spatial lens · where is everything right now",
    Component: AirlockView,
  },
] as const;

type ViewId = (typeof VIEWS)[number]["id"];

const STORAGE_KEY = "brainiac-ingest-view";

export default function IngestMonitor({ data }: { data: IngestData }) {
  const [active, setActive] = useState<ViewId>("manifest");
  const [large, setLarge] = useState(false);
  const largeData = useMemo(() => makeLargeIngest(), []);
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
          aria-label="Ingest Monitor lens"
        >
          {VIEWS.map((v) => {
            const selected = v.id === active;
            return (
              // A tab, not a toggle: `aria-selected` (not aria-pressed) is what
              // a tablist announces, so `selected` is deliberately not passed.
              // `quiet` because the "fill" is the shared layoutId pill below,
              // not a border — and the selected label is pinned to black
              // inline, since quiet's `hover:text-white` would otherwise wash
              // it out the moment the pointer landed on the active tab.
              <Button
                key={v.id}
                variant="quiet"
                size="md"
                mono={false}
                role="tab"
                aria-selected={selected}
                onClick={() => pick(v.id)}
                className={`relative rounded-full px-4 py-2 font-medium ${
                  selected ? "text-black" : "text-white/70 hover:text-white"
                }`}
                style={{ color: selected ? "#000" : undefined }}
              >
                {selected && (
                  <motion.span
                    layoutId="ingest-view-pill"
                    className="absolute inset-0 rounded-full bg-white"
                    transition={{ type: "spring", bounce: 0.25, duration: 0.5 }}
                  />
                )}
                <span className="relative flex items-center gap-2">
                  <span className="inline-block h-2 w-2 rounded-full" style={{ background: band("theta") }} />
                  {v.name}
                </span>
              </Button>
            );
          })}
        </div>
        <div className="mt-1.5 flex items-center justify-center gap-3 text-[11px] text-white/60 [text-shadow:0_1px_4px_rgba(0,0,0,0.9)]">
          <span>{current.blurb}</span>
          <span className="text-white/25">·</span>
          {/* `selected` supplies aria-pressed. The colours stay class-side (the
              control brightens on hover, which an inline colour would block),
              and the black wash is inline so pill's `hover:bg-white/5` cannot
              lighten a chip that sits over the view. */}
          <Button
            variant="pill"
            size="xs"
            mono={false}
            selected={large}
            onClick={() => pickScale(!large)}
            className="border-white/20 py-0.5 hover:border-white/50 hover:text-white"
            style={{
              borderColor: undefined,
              color: undefined,
              background: "rgba(0,0,0,0.5)",
            }}
          >
            load: {large ? "40 sources (mock)" : `${data.sources.length} live`}
          </Button>
        </div>
      </div>
    </div>
  );
}
