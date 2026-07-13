import { Pulse, SkeletonFrame } from "@/components/Skeleton";
import { routeAccent } from "@/design/routes";

// Approximates the Observatory: a row of stat tiles over two chart panels.
export default function Loading() {
  const accent = routeAccent("beta");
  return (
    <SkeletonFrame segment="analytics" accent={accent}>
      <div className="grid grid-cols-2 gap-4 md:grid-cols-4">
        {Array.from({ length: 4 }).map((_, i) => (
          <Pulse key={i} w="100%" h={92} rounded="rounded-lg" />
        ))}
      </div>
      <div className="mt-6 grid gap-4 md:grid-cols-2">
        <Pulse w="100%" h={260} rounded="rounded-xl" />
        <Pulse w="100%" h={260} rounded="rounded-xl" />
      </div>
    </SkeletonFrame>
  );
}
