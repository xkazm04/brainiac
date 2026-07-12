"use client";

/*
 * Shared submit control (hoisted): drops a manual memory into the real
 * pipeline via the token-guarded proxy, then triggers a feed refresh so
 * the new source appears in whatever metaphor is active.
 */

import { useState } from "react";

import { band, FONT_MONO } from "@/design/theme";

const THETA = band("theta");

export default function SubmitBox({
  live,
  onSubmitted,
}: {
  live: boolean;
  onSubmitted: () => void;
}) {
  const [content, setContent] = useState("");
  const [state, setState] = useState<"idle" | "sending" | "sent" | "error">("idle");

  const submit = async () => {
    if (!content.trim() || state === "sending") return;
    setState("sending");
    try {
      const r = await fetch("/api/ingest", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ content: content.trim() }),
      });
      if (!r.ok) throw new Error(String(r.status));
      setContent("");
      setState("sent");
      onSubmitted();
      setTimeout(() => setState("idle"), 2500);
    } catch {
      setState("error");
      setTimeout(() => setState("idle"), 3000);
    }
  };

  return (
    <div className="flex items-center gap-2">
      <input
        value={content}
        onChange={(e) => setContent(e.target.value)}
        onKeyDown={(e) => e.key === "Enter" && submit()}
        placeholder={live ? "drop a learning into the pipeline…" : "demo mode — server offline"}
        disabled={!live}
        className={`${FONT_MONO} w-72 rounded-full border border-white/15 bg-white/[0.03] px-4 py-2 text-sm text-white placeholder:text-[#e9edff]/30 focus:outline-none disabled:opacity-50`}
        style={{ borderColor: state === "error" ? "#ff5da2" : undefined }}
        aria-label="Submit a memory"
      />
      <button
        onClick={submit}
        disabled={!live || !content.trim() || state === "sending"}
        className={`${FONT_MONO} rounded-full border px-4 py-2 text-sm transition disabled:opacity-40`}
        style={{ borderColor: THETA, color: THETA }}
      >
        {state === "sending" ? "…" : state === "sent" ? "✓ queued" : state === "error" ? "failed" : "⚡ capture"}
      </button>
    </div>
  );
}
