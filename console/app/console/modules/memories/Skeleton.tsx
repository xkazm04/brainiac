import { Pulse, SkeletonFrame } from "@/components/Skeleton";
import { routeAccent } from "@/design/routes";

// Approximates the Archive: an as-of scrubber bar over a list of memory rows.
export default function Loading() {
  const accent = routeAccent("delta");
  return (
    <SkeletonFrame segment="memories" accent={accent}>
      <div className="flex flex-wrap items-center justify-between gap-3">
        <Pulse w="14rem" h={26} />
        <Pulse w="20rem" h={32} rounded="rounded-full" />
      </div>
      <div className="mt-6 space-y-2">
        {Array.from({ length: 8 }).map((_, i) => (
          <div
            key={i}
            className="flex items-center gap-4 rounded-lg border border-white/5 p-3"
          >
            <Pulse w="4rem" h={20} rounded="rounded-full" />
            <Pulse w={`${70 - (i % 4) * 8}%`} h={14} />
          </div>
        ))}
      </div>
    </SkeletonFrame>
  );
}
