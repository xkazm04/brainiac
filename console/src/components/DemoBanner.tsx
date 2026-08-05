"use client";

import { useEffect, useState } from "react";

import Button from "@/components/ui/Button";
import { AMBER, band, FONT_MONO, LABEL } from "@/design/theme";

const DISMISS_KEY = "brainiac-demo-banner-dismissed";

// Formalizes the silent demo fallback: when a page renders fixture-shaped
// data because the brainiac server is unreachable, say so explicitly and
// tell the operator how to go live. Dismissal sticks for the tab session.
export default function DemoBanner() {
  const [dismissed, setDismissed] = useState(true);

  useEffect(() => {
    setDismissed(sessionStorage.getItem(DISMISS_KEY) === "1");
  }, []);

  if (dismissed) return null;
  return (
    <div className="mx-auto max-w-7xl px-6 pt-6">
      <div
        role="status"
        className="flex flex-wrap items-center justify-between gap-3 rounded-lg border p-4"
        style={{ borderColor: "rgba(240,180,41,0.4)", background: "rgba(240,180,41,0.06)" }}
      >
        <div>
          <span className={LABEL} style={{ color: AMBER }}>
            demo data
          </span>
          <p className={`${FONT_MONO} mt-1 text-sm text-[var(--ink)]/70`}>
            The brainiac server is unreachable — these numbers are the Meridian fixture
            org. Start it (<code>brainiac-server serve</code>) and set{" "}
            <code>BRAINIAC_API_URL</code> + <code>BRAINIAC_API_TOKEN</code> to go live.
          </p>
        </div>
        <Button
          variant="pill"
          size="sm"
          accent={band("gamma")}
          onClick={() => {
            sessionStorage.setItem(DISMISS_KEY, "1");
            setDismissed(true);
          }}
        >
          dismiss
        </Button>
      </div>
    </div>
  );
}
