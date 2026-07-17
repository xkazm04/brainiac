import { Pulse, SkeletonFrame } from "@/components/Skeleton";
import { routeAccent } from "@/design/routes";

// Approximates the Projects surface: project cards beside the pairing rail.
// Ground band (0 Hz) — desaturated ink accent, per theme.ts GROUND.
export default function Loading() {
  const accent = routeAccent("ground");
  return (
    <SkeletonFrame segment="projects" accent={accent}>
      <div className="space-y-4">
        {Array.from({ length: 3 }).map((_, i) => (
          <div key={i} className="rounded-xl border border-white/5 p-4">
            <Pulse w="10rem" h={14} />
            <div className="mt-3 space-y-2">
              <Pulse w="100%" h={12} />
              <Pulse w="80%" h={12} />
            </div>
          </div>
        ))}
      </div>
    </SkeletonFrame>
  );
}
