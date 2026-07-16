import { Pulse, SkeletonFrame } from "@/components/Skeleton";
import { routeAccent } from "@/design/routes";

// Approximates the skills catalog: header prose, then skill cards (title row
// with domain chip and pulse, description line).
export default function Loading() {
  const accent = routeAccent("beta");
  return (
    <SkeletonFrame segment="skills" accent={accent}>
      <div className="space-y-2">
        <Pulse w="26rem" h={28} />
        <Pulse w="34rem" h={14} />
      </div>
      <div className="mt-8 space-y-4">
        {Array.from({ length: 3 }).map((_, i) => (
          <div key={i} className="space-y-3 rounded-xl border border-white/5 p-6">
            <div className="flex items-center gap-3">
              <Pulse w="12rem" h={22} />
              <Pulse w="5rem" h={18} rounded="rounded-md" />
              <Pulse w="6rem" h={12} />
            </div>
            <Pulse w="80%" h={14} />
          </div>
        ))}
      </div>
    </SkeletonFrame>
  );
}
