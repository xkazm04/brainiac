import type { Metadata } from "next";

import KnowledgeBase from "@/kb/KnowledgeBase";

// Public, static, example-data-only — like the pitch at "/", it makes no API
// call and holds no token. The real pages, once KB1 lands, live behind the
// console gate; this surface only explains the layer.
export const metadata: Metadata = {
  title: "Brainiac — the wiki that cannot rot",
  description:
    "A knowledge base whose pages are projections over canonical memories, not a second source of truth. Truth flows one way: memories compose into pages; a human edit to a page re-enters through extraction and the same review gate. Every capability is stamped shipped, in progress, or roadmap.",
};

export default function KnowledgeBasePage() {
  return <KnowledgeBase />;
}
