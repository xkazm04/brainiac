import { Pulse, SkeletonFrame } from "@/components/Skeleton";
import { routeAccent } from "@/design/routes";

// Approximates the standardization board: sweep strip, header prose, then
// divergence cards (title row, two approach panels, the recommendation line).
export default function Loading() {
  const accent = routeAccent("theta");
  return (
    <SkeletonFrame segment="divergence" accent={accent}>
      <Pulse w="100%" h={64} rounded="rounded-xl" />
      <div className="mt-8 space-y-2">
        <Pulse w="24rem" h={28} />
        <Pulse w="34rem" h={14} />
      </div>
      <div className="mt-8 space-y-5">
        {Array.from({ length: 2 }).map((_, i) => (
          <div key={i} className="space-y-3 rounded-xl border border-white/5 p-6">
            <div className="flex items-center gap-3">
              <Pulse w="6rem" h={22} rounded="rounded-md" />
              <Pulse w="14rem" h={22} />
            </div>
            <div className="grid gap-3 sm:grid-cols-2">
              <Pulse w="100%" h={72} rounded="rounded-lg" />
              <Pulse w="100%" h={72} rounded="rounded-lg" />
            </div>
            <Pulse w="80%" h={16} />
          </div>
        ))}
      </div>
    </SkeletonFrame>
  );
}
