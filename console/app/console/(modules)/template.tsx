"use client";

import type { ReactNode } from "react";
import { motion, useReducedMotion } from "framer-motion";

// Remounts per navigation (that is what a template is for), giving each module
// swap one short entry — content rises 8px and fades in while the chrome above
// holds perfectly still. Entry-only, ~a quarter second, no infinite loops:
// exactly the utility-page motion budget in design/theme.ts.
export default function ModuleTemplate({ children }: { children: ReactNode }) {
  const reduce = useReducedMotion();
  if (reduce) return <>{children}</>;
  return (
    <motion.div
      initial={{ opacity: 0, y: 8 }}
      animate={{ opacity: 1, y: 0 }}
      transition={{ duration: 0.24, ease: [0.22, 1, 0.36, 1] }}
    >
      {children}
    </motion.div>
  );
}
