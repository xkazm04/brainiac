import { Pulse, SkeletonFrame } from "@/components/Skeleton";
import { routeAccent } from "@/design/routes";

// Approximates the Knowledge Health report: sweep strip, headline score +
// trend, four pillar cards, then the attention list.
export default function Loading() {
  const accent = routeAccent("alpha");
  return (
    <SkeletonFrame segment="health" accent={accent}>
      <Pulse w="100%" h={64} rounded="rounded-xl" />
      <div className="mt-8 flex items-end gap-6">
        <Pulse w="9rem" h={72} rounded="rounded-lg" />
        <Pulse w="16rem" h={56} rounded="rounded-lg" />
      </div>
      <div className="mt-8 grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
        {Array.from({ length: 4 }).map((_, i) => (
          <Pulse key={i} w="100%" h={120} rounded="rounded-lg" />
        ))}
      </div>
      <div className="mt-8 space-y-3">
        {Array.from({ length: 3 }).map((_, i) => (
          <Pulse key={i} w="100%" h={72} rounded="rounded-lg" />
        ))}
      </div>
    </SkeletonFrame>
  );
}
