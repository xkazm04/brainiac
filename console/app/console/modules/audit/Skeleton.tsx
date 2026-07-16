import { Pulse, SkeletonFrame } from "@/components/Skeleton";
import { routeAccent } from "@/design/routes";

// Approximates the Audit ledger: headline + caveat banner, kind filter tabs,
// then the table (header row + N event rows) and the pager line. Matches
// AuditLedger's real layout so the page does not jump when the fetch lands.
export default function Loading() {
  const accent = routeAccent("alpha");
  return (
    <SkeletonFrame segment="audit" accent={accent}>
      <Pulse w="60%" h={30} rounded="rounded-lg" />
      <Pulse w="90%" h={16} rounded="rounded" style={{ marginTop: 10 }} />
      <Pulse w="100%" h={52} rounded="rounded-lg" style={{ marginTop: 16 }} />

      <div className="mt-5 flex gap-1.5">
        {Array.from({ length: 4 }).map((_, i) => (
          <Pulse key={i} w="5.5rem" h={26} rounded="rounded-full" />
        ))}
      </div>

      <div className="mt-4 rounded-xl border border-white/10">
        <Pulse w="100%" h={30} rounded="rounded-t-xl" />
        <div className="space-y-0 p-0">
          {Array.from({ length: 6 }).map((_, i) => (
            <div key={i} className="border-b border-white/5 px-3 py-2.5">
              <Pulse w={`${70 - (i % 3) * 12}%`} h={14} />
              <Pulse w="40%" h={11} style={{ marginTop: 6 }} />
            </div>
          ))}
        </div>
        <Pulse w="100%" h={38} rounded="rounded-b-xl" />
      </div>
    </SkeletonFrame>
  );
}
