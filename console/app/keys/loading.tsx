import { Pulse, SkeletonFrame } from "@/components/Skeleton";
import { routeAccent } from "@/design/routes";

// Approximates the Keys surface: a mint panel over the token table. Ground
// band (0 Hz) — desaturated ink accent, per theme.ts GROUND.
export default function Loading() {
  const accent = routeAccent("ground");
  return (
    <SkeletonFrame segment="keys" accent={accent}>
      <Pulse w="100%" h={120} rounded="rounded-lg" />
      <div className="mt-6 space-y-2">
        {Array.from({ length: 6 }).map((_, i) => (
          <div
            key={i}
            className="flex items-center justify-between gap-4 rounded-lg border border-white/5 p-3"
          >
            <Pulse w="12rem" h={14} />
            <Pulse w="6rem" h={12} />
            <Pulse w="5rem" h={28} rounded="rounded-full" />
          </div>
        ))}
      </div>
    </SkeletonFrame>
  );
}
