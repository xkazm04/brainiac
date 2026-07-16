import type { ReactNode } from "react";

import Chrome from "../chrome";

/*
 * The operator shell. The console is ONE route now (page.tsx, switching on ?m=)
 * plus the single document sub-route under docs/[slug], so the chrome mounts
 * here once and persists across every module swap — the nav and its live
 * mini-dashboard never remount or refetch while the pane below changes.
 *
 * It used to live one level down, in a (modules) route group, because the
 * console home was a landing page at /console that had to stay outside it. That
 * landing was a duplicate of "/" and is gone, so the group had nothing left to
 * exclude and the layout came up here.
 */
export default function ConsoleLayout({ children }: { children: ReactNode }) {
  return (
    <>
      <Chrome />
      {children}
    </>
  );
}
