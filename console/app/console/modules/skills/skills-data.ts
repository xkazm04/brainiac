/*
 * Demo fixtures for the skills catalog — Meridian's shelf, shaped to exercise
 * every state: a published skill with a busy pulse, a published one with a
 * version history, and a draft that is listed but serves nothing. Served only
 * behind <DemoBanner> when the brainiac server is unreachable.
 */

import type { LibrarySkill, SkillDetail } from "@/lib/types";

export const DEMO_SKILLS: LibrarySkill[] = [
  {
    id: "demo-skill-migrations",
    slug: "review-migrations",
    name: "Review migrations",
    description:
      "The schema-change checklist: RLS on every new table, grants for the app role, and the failure modes that only show up under row-level security.",
    domain: "database",
    maturity: "published",
    downloadable: true,
  },
  {
    id: "demo-skill-flaky",
    slug: "triage-flaky-tests",
    name: "Triage flaky tests",
    description:
      "Find, reproduce, and quarantine a flaky test without deleting the signal it carries.",
    domain: "testing",
    maturity: "published",
    downloadable: true,
  },
  {
    id: "demo-skill-retro",
    slug: "incident-retro",
    name: "Run an incident retro",
    description:
      "The retro procedure that feeds what was learned back into the org's memory instead of a slide nobody reopens.",
    domain: "process",
    maturity: "draft",
    downloadable: false,
  },
];

const base = (id: string) => DEMO_SKILLS.find((s) => s.id === id)!;

export const DEMO_SKILL_DETAILS: Record<string, SkillDetail> = {
  "demo-skill-migrations": {
    ...base("demo-skill-migrations"),
    versions: [
      {
        semver: "2.1.0",
        published: true,
        published_at: "2026-07-10T09:00:00Z",
        created_at: "2026-07-09T17:20:00Z",
      },
      {
        semver: "2.0.0",
        published: true,
        published_at: "2026-06-28T08:00:00Z",
        created_at: "2026-06-27T15:00:00Z",
      },
    ],
    usage: [
      { team: "platform", uses: 41 },
      { team: "payments", uses: 22 },
      { team: "data", uses: 8 },
    ],
  },
  "demo-skill-flaky": {
    ...base("demo-skill-flaky"),
    versions: [
      {
        semver: "1.0.0",
        published: true,
        published_at: "2026-07-05T12:00:00Z",
        created_at: "2026-07-05T11:00:00Z",
      },
    ],
    usage: [{ team: "platform", uses: 13 }],
  },
  "demo-skill-retro": {
    ...base("demo-skill-retro"),
    versions: [
      {
        semver: "0.1.0",
        published: false,
        published_at: null,
        created_at: "2026-07-14T10:00:00Z",
      },
    ],
    usage: [],
  },
};
