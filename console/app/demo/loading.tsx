import { Pulse, SkeletonFrame } from "@/components/Skeleton";
import { routeAccent } from "@/design/routes";

// Generic showcase skeleton — the tour surfaces differ, so this frames a
// headline block over card rows rather than mimicking any one of them.
export default function Loading() {
  const accent = routeAccent("beta");
  return (
    <SkeletonFrame segment="demo" accent={accent}>
      <div className="space-y-2">
        <Pulse w="26rem" h={28} />
        <Pulse w="34rem" h={14} />
      </div>
      <div className="mt-8 grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
        {Array.from({ length: 6 }).map((_, i) => (
          <Pulse key={i} w="100%" h={120} rounded="rounded-lg" />
        ))}
      </div>
    </SkeletonFrame>
  );
}
