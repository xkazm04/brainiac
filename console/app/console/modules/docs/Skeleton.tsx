import { Pulse, SkeletonFrame } from "@/components/Skeleton";
import { routeAccent } from "@/design/routes";

// Approximates the wiki: header, view tabs, the space rail, the space directory.
// A skeleton that draws the wrong shape is its own small lie — this one used to
// promise the flat list of cards the module no longer renders.
export default function Loading() {
  const accent = routeAccent("gamma");
  return (
    <SkeletonFrame segment="docs" accent={accent}>
      <div className="space-y-2">
        <Pulse w="22rem" h={28} />
        <Pulse w="32rem" h={14} />
      </div>
      <div className="mt-5 flex gap-2">
        {Array.from({ length: 3 }).map((_, i) => (
          <Pulse key={i} w={`${6 + i}rem`} h={24} />
        ))}
      </div>
      <div className="mt-5 grid gap-5 lg:grid-cols-[264px_minmax(0,1fr)]">
        <div className="space-y-2">
          <Pulse w="100%" h={32} />
          {Array.from({ length: 8 }).map((_, i) => (
            <Pulse key={i} w={`${58 + ((i * 7) % 34)}%`} h={20} />
          ))}
        </div>
        <div className="grid gap-3 sm:grid-cols-2 xl:grid-cols-3">
          {Array.from({ length: 6 }).map((_, i) => (
            <div key={i} className="space-y-2 rounded-xl border border-white/5 p-4">
              <Pulse w={`${40 + (i % 3) * 12}%`} h={18} />
              <Pulse w="60%" h={12} />
            </div>
          ))}
        </div>
      </div>
    </SkeletonFrame>
  );
}
