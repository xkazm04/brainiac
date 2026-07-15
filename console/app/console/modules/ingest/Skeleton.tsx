import { Pulse, SkeletonFrame } from "@/components/Skeleton";
import { routeAccent } from "@/design/routes";

// Approximates the Ingest Monitor: a submit box, the six-stage rail, and the
// live source feed rows.
export default function Loading() {
  const accent = routeAccent("theta");
  return (
    <SkeletonFrame segment="ingest" accent={accent}>
      <Pulse w="100%" h={64} rounded="rounded-lg" />
      <div className="mt-6 flex flex-wrap gap-2">
        {Array.from({ length: 6 }).map((_, i) => (
          <Pulse key={i} w="8rem" h={44} rounded="rounded-lg" />
        ))}
      </div>
      <div className="mt-6 space-y-2">
        {Array.from({ length: 6 }).map((_, i) => (
          <div
            key={i}
            className="flex items-center gap-4 rounded-lg border border-white/5 p-3"
          >
            <Pulse w="5rem" h={16} />
            <Pulse w={`${60 - (i % 3) * 10}%`} h={12} />
          </div>
        ))}
      </div>
    </SkeletonFrame>
  );
}
