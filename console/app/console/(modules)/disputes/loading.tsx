import { Pulse, SkeletonFrame } from "@/components/Skeleton";
import { routeAccent } from "@/design/routes";

// Approximates the Disputes bench: a decision bar over a list of flagged
// (contested) memory cards.
export default function Loading() {
  const accent = routeAccent("theta");
  return (
    <SkeletonFrame segment="disputes" accent={accent}>
      <Pulse w="100%" h={52} rounded="rounded-lg" />
      <div className="mt-6 space-y-3">
        {Array.from({ length: 5 }).map((_, i) => (
          <div key={i} className="space-y-2 rounded-lg border border-white/5 p-4">
            <div className="flex items-center gap-3">
              <Pulse w="5rem" h={20} rounded="rounded-full" />
              <Pulse w="8rem" h={12} />
            </div>
            <Pulse w={`${75 - (i % 3) * 10}%`} h={14} />
          </div>
        ))}
      </div>
    </SkeletonFrame>
  );
}
