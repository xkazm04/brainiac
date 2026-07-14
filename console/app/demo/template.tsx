"use client";

import type { ReactNode } from "react";
import { motion, useReducedMotion } from "framer-motion";

// Same entry as the console modules (app/console/(modules)/template.tsx): the
// tour strip and ribbon hold still while the surface underneath rises in.
export default function DemoTemplate({ children }: { children: ReactNode }) {
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
