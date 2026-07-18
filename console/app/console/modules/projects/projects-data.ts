// Shared substrate for the Projects module: demo shapes and the client
// mutation helpers. Ground band (0 Hz) — access surfaces, like Keys.

import type { AddedRepo, CreatedProject, OnboardDecision, OnboardRequest, Project } from "@/lib/types";

export interface ProjectsData {
  live: boolean;
  projects: Project[];
  requests: OnboardRequest[];
}

async function jsonOrThrow<T>(r: Response): Promise<T> {
  if (!r.ok) throw new Error((await r.json().catch(() => null))?.error ?? String(r.status));
  return r.json() as Promise<T>;
}

export async function refreshProjects(): Promise<Project[]> {
  const r = await fetch("/api/projects");
  return (await jsonOrThrow<{ projects: Project[] }>(r)).projects;
}

export async function createProject(name: string): Promise<CreatedProject> {
  const r = await fetch("/api/projects", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ name }),
  });
  return jsonOrThrow(r);
}

export async function addRepo(projectId: string, remote: string): Promise<AddedRepo> {
  const r = await fetch(`/api/projects/${projectId}/repos`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ remote }),
  });
  return jsonOrThrow(r);
}

export async function removeRepo(projectId: string, repoId: string): Promise<void> {
  const r = await fetch(`/api/projects/${projectId}/repos/${repoId}`, { method: "DELETE" });
  await jsonOrThrow(r);
}

export async function refreshRequests(): Promise<OnboardRequest[]> {
  const r = await fetch("/api/onboard/requests");
  return (await jsonOrThrow<{ requests: OnboardRequest[] }>(r)).requests;
}

export async function decideRequest(
  id: string,
  decision: "approve" | "deny",
): Promise<OnboardDecision> {
  const r = await fetch(`/api/onboard/requests/${id}/${decision}`, { method: "POST" });
  return jsonOrThrow(r);
}

// ── demo shapes ─────────────────────────────────────────────────────────

const DAY = 86400000;

// Projects are APPLICATIONS, deliberately NOT team names (the org's teams are
// payments/platform/data): a project is what code is ABOUT, a team is who
// works on it, and two teams routinely feed one app. The demo must teach that
// distinction, not blur it.
export const DEMO_PROJECTS: ProjectsData = {
  live: false,
  projects: [
    {
      id: "dp-1",
      name: "payments-api",
      created_at: new Date(Date.now() - 40 * DAY).toISOString(),
      repos: [
        { id: "dr-1", remote: "github.com/meridian/payments-api", path_prefix: "", created_at: new Date(Date.now() - 40 * DAY).toISOString() },
        { id: "dr-2", remote: "github.com/meridian/payments-ledger", path_prefix: "", created_at: new Date(Date.now() - 12 * DAY).toISOString() },
      ],
    },
    {
      id: "dp-2",
      name: "checkout-web",
      created_at: new Date(Date.now() - 33 * DAY).toISOString(),
      // A monorepo split (migration 0039): checkout-web and feature-store share
      // one remote, each claiming its own subtree by path_prefix.
      repos: [
        { id: "dr-3", remote: "github.com/meridian/platform-monorepo", path_prefix: "apps/checkout", created_at: new Date(Date.now() - 33 * DAY).toISOString() },
      ],
    },
    {
      id: "dp-3",
      name: "feature-store",
      created_at: new Date(Date.now() - 9 * DAY).toISOString(),
      repos: [
        { id: "dr-4", remote: "github.com/meridian/platform-monorepo", path_prefix: "apps/features", created_at: new Date(Date.now() - 9 * DAY).toISOString() },
      ],
    },
  ],
  requests: [
    {
      id: "dq-1",
      user_code: "MKQ4TX7C",
      remote: "github.com/meridian/payments-api",
      label: "dev1@pay-laptop",
      created_at: new Date(Date.now() - 120000).toISOString(),
      expires_at: new Date(Date.now() + 780000).toISOString(),
      project_id: "dp-1",
      project_name: "payments-api",
    },
    {
      id: "dq-2",
      user_code: "W3NGA9YD",
      remote: "github.com/meridian/fraud-models",
      label: "analyst1@data-ws",
      created_at: new Date(Date.now() - 300000).toISOString(),
      expires_at: new Date(Date.now() + 600000).toISOString(),
      project_id: null,
      project_name: null,
    },
  ],
};
