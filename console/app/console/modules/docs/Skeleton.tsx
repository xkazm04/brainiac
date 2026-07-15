import { Pulse, SkeletonFrame } from "@/components/Skeleton";
import { routeAccent } from "@/design/routes";

// Approximates the pages index: header prose over a list of page cards.
export default function Loading() {
  const accent = routeAccent("gamma");
  return (
    <SkeletonFrame segment="docs" accent={accent}>
      <div className="space-y-2">
        <Pulse w="22rem" h={28} />
        <Pulse w="32rem" h={14} />
      </div>
      <div className="mt-8 space-y-3">
        {Array.from({ length: 6 }).map((_, i) => (
          <div key={i} className="space-y-2 rounded-lg border border-white/5 p-5">
            <Pulse w={`${40 + (i % 3) * 12}%`} h={18} />
            <Pulse w="70%" h={12} />
          </div>
        ))}
      </div>
    </SkeletonFrame>
  );
}
