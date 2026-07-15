import { notFound } from "next/navigation";

import DemoBanner from "@/components/DemoBanner";
import DocPage from "@/docs/DocPage";
import { DEMO_DOC, DEMO_DOC_SLUG, DEMO_REVISIONS } from "@/docs/docs-demo";
import { configFromEnv, getDoc, getDocRevisions } from "@/lib/api";
import { withDemoFallback } from "@/lib/demo-fallback";
import type { DocDetail, DocRevisionSummary } from "@/lib/types";

import { approveRevisionAction, editSectionAction } from "./actions";

export const dynamic = "force-dynamic";

interface Payload {
  detail: DocDetail;
  revisions: DocRevisionSummary[];
}

export async function generateMetadata({ params }: { params: Promise<{ slug: string }> }) {
  const { slug } = await params;
  return { title: `Brainiac — ${slug}` };
}

/**
 * The reader. A READ surface, so it takes the demo fallback when the server is
 * unreachable — but the Approve action is passed down ONLY when live: a publish
 * button wired to fixture data would be a lie with consequences.
 *
 * Offline, only the demo page has content; any other slug 404s rather than
 * pretending a page exists.
 */
export default async function DocDetailPage({ params }: { params: Promise<{ slug: string }> }) {
  const { slug } = await params;
  const cfg = configFromEnv();

  const { data, live } = await withDemoFallback<Payload | null>(
    async () => {
      const [detail, revisions] = await Promise.all([
        getDoc(cfg, slug),
        getDocRevisions(cfg, slug).catch(() => [] as DocRevisionSummary[]),
      ]);
      return { detail, revisions };
    },
    slug === DEMO_DOC_SLUG ? { detail: DEMO_DOC, revisions: DEMO_REVISIONS } : null,
  );

  if (!data) notFound();

  return (
    <>
      {!live && <DemoBanner />}
      <DocPage
        detail={data.detail}
        revisions={data.revisions}
        approve={live ? approveRevisionAction.bind(null, slug) : undefined}
        edit={live ? editSectionAction.bind(null, slug) : undefined}
      />
    </>
  );
}
