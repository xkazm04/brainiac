import DemoBanner from "@/components/DemoBanner";
import { configFromEnv, listOnboardRequests, listProjects } from "@/lib/api";
import { withDemoFallback } from "@/lib/demo-fallback";

import { DEMO_PROJECTS, type ProjectsData } from "./projects-data";
import Projects from "./Projects";

export const dynamic = "force-dynamic";

export const metadata = {
  title: "Brainiac — Projects",
};

// Live registry + pairing queue when reachable; demo shape (behind an
// unconditional DemoBanner — fabricated repos and codes) when not.
export default async function ProjectsPage() {
  const { data, live } = await withDemoFallback<ProjectsData>(async () => {
    const cfg = configFromEnv();
    const [projects, requests] = await Promise.all([
      listProjects(cfg),
      listOnboardRequests(cfg),
    ]);
    return { live: true, projects, requests };
  }, DEMO_PROJECTS);
  return (
    <>
      {!live && <DemoBanner />}
      <Projects data={data} />
    </>
  );
}
