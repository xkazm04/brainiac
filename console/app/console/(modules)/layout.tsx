import type { ReactNode } from "react";

import Chrome from "../../chrome";

// The operator shell. Every console module lives in this route group, so the
// chrome is mounted ONCE here and persists across module navigation — the nav
// and its live mini-dashboard never remount or refetch while the content pane
// swaps underneath (each module is its own code-split chunk, streamed behind
// its loading.tsx skeleton). The group also draws the boundary that used to be
// path-matching in chrome.tsx: the console home ("/console", the wave field)
// sits OUTSIDE it and keeps its own full-bleed header, as do the public shells.
export default function ConsoleModulesLayout({ children }: { children: ReactNode }) {
  return (
    <>
      <Chrome />
      {children}
    </>
  );
}
