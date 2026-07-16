import DemoBanner from "@/components/DemoBanner";
import { configFromEnv, getSkillDetail, listSkills } from "@/lib/api";
import { withDemoFallback } from "@/lib/demo-fallback";
import type { LibrarySkill, SkillDetail } from "@/lib/types";

import { DEMO_SKILL_DETAILS, DEMO_SKILLS } from "./skills-data";
import SkillsCatalog from "./SkillsCatalog";

export const dynamic = "force-dynamic";

export const metadata = {
  title: "Brainiac — Skills",
};

/** Same prefetch trade as the standards board: a shelf is tens of skills, and
 *  instant expansion is worth one burst of small queries. */
const DETAIL_PREFETCH_CAP = 100;

async function fetchLive(): Promise<{
  skills: LibrarySkill[];
  details: Record<string, SkillDetail>;
}> {
  const cfg = configFromEnv();
  const skills = await listSkills(cfg);
  const details: Record<string, SkillDetail> = {};
  const fetched = await Promise.all(
    skills.slice(0, DETAIL_PREFETCH_CAP).map((s) => getSkillDetail(cfg, s.slug)),
  );
  for (const d of fetched) details[d.id] = d;
  return { skills, details };
}

export default async function SkillsModule() {
  const { data, live } = await withDemoFallback(fetchLive, {
    skills: DEMO_SKILLS,
    details: DEMO_SKILL_DETAILS,
  });
  return (
    <>
      {!live && <DemoBanner />}
      <SkillsCatalog skills={data.skills} details={data.details} />
    </>
  );
}
