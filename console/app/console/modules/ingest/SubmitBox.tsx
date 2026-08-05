"use client";

/*
 * Shared submit control (hoisted): drops a manual memory into the real
 * pipeline via the token-guarded proxy, then triggers a feed refresh so
 * the new source appears in whatever metaphor is active.
 */

import { useState } from "react";

import Button from "@/components/ui/Button";
import { Input } from "@/components/ui/Input";
import { band, MAGENTA } from "@/design/theme";

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
      {/* Capsule field: `fullWidth={false}` because the caller owns the width
          (w-72) — a baked-in w-full would win or lose by stylesheet order.
          A failed submit reddens the edge; `invalid` also announces it, and the
          MAGENTA override keeps the original solid edge rather than the token's
          softer invalid wash. */}
      <Input
        value={content}
        onChange={(e) => setContent(e.target.value)}
        onKeyDown={(e) => e.key === "Enter" && submit()}
        placeholder={live ? "drop a learning into the pipeline…" : "demo mode — server offline"}
        disabled={!live}
        fullWidth={false}
        invalid={state === "error"}
        className="w-72 rounded-full px-4"
        style={{ borderColor: state === "error" ? MAGENTA : undefined }}
        aria-label="Submit a memory"
      />
      <Button
        variant="pill"
        size="sm"
        accent={THETA}
        onClick={submit}
        disabled={!live || !content.trim() || state === "sending"}
        className="px-4 py-2"
        style={{ borderColor: THETA }}
      >
        {state === "sending" ? "…" : state === "sent" ? "✓ queued" : state === "error" ? "failed" : "⚡ capture"}
      </Button>
    </div>
  );
}
