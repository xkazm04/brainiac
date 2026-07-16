"use client";

/*
 * Per-module error isolation, kept by hand now that the modules are not routes.
 *
 * Next gave this away for free while every module was its own route: an
 * error.tsx per segment meant a divergence crash took down the standards pane
 * and nothing else. Collapsing to one route would have collapsed that too —
 * every module sharing a single boundary, so one bad `detected_at` white-screens
 * the whole console. That regression is not worth the URL, so the boundary is
 * explicit here instead. `key={module}` remounts it on every tab change, which
 * is what lets a reader walk away from a broken module rather than being stuck
 * on its error screen.
 *
 * A class component because that is still the only way to catch a render error
 * in React; the UI it falls back to is the same RouteError the segments used.
 */

import { Component, type ReactNode } from "react";
import { motion, useReducedMotion } from "framer-motion";

import RouteError from "@/components/RouteError";

class Boundary extends Component<
  { children: ReactNode },
  { error: (Error & { digest?: string }) | null }
> {
  state: { error: (Error & { digest?: string }) | null } = { error: null };

  static getDerivedStateFromError(error: Error) {
    return { error };
  }

  render() {
    if (this.state.error) {
      return <RouteError error={this.state.error} reset={() => this.setState({ error: null })} />;
    }
    return this.props.children;
  }
}

/**
 * The module pane: one short entry per swap while the chrome above holds still.
 * Entry-only, ~a quarter second, no loops — the utility-page motion budget from
 * design/theme.ts. (This is what the old route `template.tsx` did.)
 */
export default function ModuleBoundary({ children }: { children: ReactNode }) {
  const reduce = useReducedMotion();
  const inner = <Boundary>{children}</Boundary>;
  if (reduce) return inner;
  return (
    <motion.div
      initial={{ opacity: 0, y: 8 }}
      animate={{ opacity: 1, y: 0 }}
      transition={{ duration: 0.24, ease: [0.22, 1, 0.36, 1] }}
    >
      {inner}
    </motion.div>
  );
}
