import { Pulse, SkeletonFrame } from "@/components/Skeleton";
import { routeAccent } from "@/design/routes";

// Approximates the standards board: header prose, then the tree rail beside
// the rule detail card (chips, statement, examples block, pulse bars).
export default function Loading() {
  const accent = routeAccent("theta");
  return (
    <SkeletonFrame segment="standards" accent={accent}>
      <div className="space-y-2">
        <Pulse w="26rem" h={28} />
        <Pulse w="34rem" h={14} />
      </div>
      <div className="mt-8 grid gap-6 lg:grid-cols-[260px_1fr]">
        <div className="space-y-2">
          {Array.from({ length: 8 }).map((_, i) => (
            <Pulse key={i} w={i % 3 === 0 ? "7rem" : "12rem"} h={i % 3 === 0 ? 12 : 18} />
          ))}
        </div>
        <div className="space-y-4 rounded-xl border border-white/5 p-6">
          <div className="flex gap-2">
            <Pulse w="5rem" h={22} rounded="rounded-md" />
            <Pulse w="6rem" h={22} rounded="rounded-md" />
          </div>
          <Pulse w="80%" h={26} />
          <Pulse w="100%" h={90} rounded="rounded-lg" />
          <div className="space-y-2">
            <Pulse w="60%" h={10} />
            <Pulse w="45%" h={10} />
            <Pulse w="30%" h={10} />
          </div>
        </div>
      </div>
    </SkeletonFrame>
  );
}
