import { Pulse, SkeletonFrame } from "@/components/Skeleton";
import { routeAccent } from "@/design/routes";

// Approximates the two-section review queue (pending promotions, then the
// contradictions list with its status tabs).
export default function Loading() {
  const accent = routeAccent("alpha");
  return (
    <SkeletonFrame segment="reviews" accent={accent}>
      <div className="space-y-10">
        <section className="space-y-3">
          <Pulse w="16rem" h={26} />
          {Array.from({ length: 3 }).map((_, i) => (
            <div key={i} className="space-y-2 rounded-lg border border-white/5 p-4">
              <Pulse w="80%" h={16} />
              <Pulse w="45%" h={11} />
              <div className="flex gap-2 pt-1">
                <Pulse w="6rem" h={30} rounded="rounded-full" />
                <Pulse w="6rem" h={30} rounded="rounded-full" />
              </div>
            </div>
          ))}
        </section>
        <section className="space-y-3">
          <Pulse w="12rem" h={26} />
          <div className="flex gap-3">
            {Array.from({ length: 5 }).map((_, i) => (
              <Pulse key={i} w="5rem" h={14} />
            ))}
          </div>
          {Array.from({ length: 2 }).map((_, i) => (
            <div key={i} className="space-y-2 rounded-lg border border-white/5 p-4">
              <Pulse w="70%" h={14} />
              <Pulse w="65%" h={14} />
              <Pulse w="40%" h={11} />
            </div>
          ))}
        </section>
      </div>
    </SkeletonFrame>
  );
}
