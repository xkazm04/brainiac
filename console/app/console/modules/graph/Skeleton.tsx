import { Pulse, SkeletonFrame } from "@/components/Skeleton";
import { routeAccent } from "@/design/routes";

// Approximates the Cortex Map: a wide canvas stage with a legend/lens row.
export default function Loading() {
  const accent = routeAccent("gamma");
  return (
    <SkeletonFrame segment="graph" accent={accent}>
      <div className="flex flex-wrap items-center gap-3">
        {Array.from({ length: 4 }).map((_, i) => (
          <Pulse key={i} w="7rem" h={28} rounded="rounded-full" />
        ))}
      </div>
      <div className="mt-4">
        <Pulse w="100%" h="60vh" rounded="rounded-xl" style={{ minHeight: 420 }} />
      </div>
    </SkeletonFrame>
  );
}
