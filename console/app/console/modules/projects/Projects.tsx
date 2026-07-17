"use client";

/*
 * Projects — the registry API keys scope to and the onboarding allow-list.
 * Two jobs on one desk, deliberately: the left ledger says which repos may
 * join which project; the right rail is the doorbell — pairing requests from
 * `bx`-less developers running the brainiac-onboard skill, waiting for a
 * human to sign. Approval derives the project from the whitelist match; an
 * unmatched request cannot be approved, only denied (or fixed by registering
 * the repo first). Ground band (0 Hz), same family as Keys: this is access.
 */

import { useCallback, useEffect, useState } from "react";

import type { ProjectsData } from "./projects-data";
import {
  addRepo,
  createProject,
  decideRequest,
  refreshProjects,
  refreshRequests,
  removeRepo,
} from "./projects-data";
import { fmtAgo } from "../keys/KeyShared";
import { FONT_DISPLAY, FONT_MONO, GROUND, LABEL, MAGENTA } from "@/design/theme";

export default function Projects({ data }: { data: ProjectsData }) {
  const [projects, setProjects] = useState(data.projects);
  const [requests, setRequests] = useState(data.requests);
  const [error, setError] = useState<string | null>(null);
  const [newProject, setNewProject] = useState("");
  const [repoDrafts, setRepoDrafts] = useState<Record<string, string>>({});
  const [busy, setBusy] = useState<string | null>(null);

  const reloadProjects = useCallback(() => {
    if (data.live) void refreshProjects().then(setProjects).catch(() => undefined);
  }, [data.live]);

  // The approval rail polls: a pairing request appears while the operator is
  // already on this page (the skill just told the developer to come here), so
  // waiting for a manual refresh is exactly the wrong default.
  useEffect(() => {
    if (!data.live) return;
    const tick = () => void refreshRequests().then(setRequests).catch(() => undefined);
    const t = setInterval(tick, 10000);
    return () => clearInterval(t);
  }, [data.live]);

  const run = async (key: string, fn: () => Promise<void>) => {
    if (!data.live || busy) return;
    setBusy(key);
    setError(null);
    try {
      await fn();
    } catch (e) {
      setError(e instanceof Error ? e.message : "request failed");
    } finally {
      setBusy(null);
    }
  };

  const onCreateProject = () =>
    run("create", async () => {
      await createProject(newProject.trim());
      setNewProject("");
      reloadProjects();
    });

  const onAddRepo = (projectId: string) =>
    run(`repo-${projectId}`, async () => {
      await addRepo(projectId, (repoDrafts[projectId] ?? "").trim());
      setRepoDrafts((d) => ({ ...d, [projectId]: "" }));
      reloadProjects();
    });

  const onRemoveRepo = (projectId: string, repoId: string) =>
    run(`rm-${repoId}`, async () => {
      await removeRepo(projectId, repoId);
      reloadProjects();
    });

  const onDecide = (id: string, decision: "approve" | "deny") =>
    run(`${decision}-${id}`, async () => {
      await decideRequest(id, decision);
      setRequests((rs) => rs.filter((r) => r.id !== id));
      reloadProjects();
    });

  return (
    <div className="mx-auto max-w-6xl px-6 py-6">
      <div className={LABEL} style={{ color: GROUND }}>
        ground · projects · onboarding desk
      </div>
      <h1 className={`${FONT_DISPLAY} mt-1 text-3xl font-semibold tracking-tight text-white`}>
        Which repos may join the org&apos;s mind.
      </h1>

      {error && <div className={`${FONT_MONO} mt-3 text-sm text-[#f0b429]`}>{error}</div>}

      <div className="mt-5 grid gap-6 lg:grid-cols-[1.2fr_0.8fr]">
        {/* the registry */}
        <div className="space-y-4">
          {projects.map((p) => (
            <div key={p.id} className="overflow-hidden rounded-xl border border-white/10">
              <div className="flex items-baseline justify-between border-b border-white/10 bg-white/[0.02] px-4 py-2.5">
                <span className={`${FONT_MONO} text-sm text-[#e9edff]/85`}>{p.name}</span>
                <span className={LABEL} style={{ color: "rgba(233,237,255,0.35)" }}>
                  {p.repos.length} repo{p.repos.length === 1 ? "" : "s"} · created {fmtAgo(p.created_at)}
                </span>
              </div>
              {p.repos.map((r) => (
                <div
                  key={r.id}
                  className={`${FONT_MONO} flex items-center justify-between gap-3 border-b border-white/[0.05] px-4 py-2 text-sm`}
                >
                  <span className="truncate text-[#e9edff]/70">{r.remote}</span>
                  <button
                    onClick={() => onRemoveRepo(p.id, r.id)}
                    disabled={!data.live || busy !== null}
                    className="rounded-full border border-white/12 px-2 py-0.5 text-[10px] uppercase tracking-widest text-[#e9edff]/40 transition hover:border-[#ff5da2]/60 hover:text-[#ff5da2] disabled:opacity-40"
                  >
                    remove
                  </button>
                </div>
              ))}
              {p.repos.length === 0 && (
                <p className={`${FONT_MONO} px-4 py-3 text-xs text-[#e9edff]/35`}>
                  no repos yet — onboarding cannot match this project until one is registered
                </p>
              )}
              <div className="flex gap-2 px-4 py-2.5">
                <input
                  value={repoDrafts[p.id] ?? ""}
                  onChange={(e) => setRepoDrafts((d) => ({ ...d, [p.id]: e.target.value }))}
                  placeholder="https://github.com/owner/name or git@…"
                  disabled={!data.live}
                  className={`${FONT_MONO} w-full rounded-lg border border-white/15 bg-white/[0.03] px-3 py-1.5 text-xs text-white placeholder:text-[#e9edff]/30 focus:border-white/40 focus:outline-none disabled:opacity-50`}
                />
                <button
                  onClick={() => onAddRepo(p.id)}
                  disabled={!data.live || !(repoDrafts[p.id] ?? "").trim() || busy !== null}
                  className={`${FONT_MONO} shrink-0 rounded-full border px-3 py-1 text-xs transition disabled:opacity-40`}
                  style={{ borderColor: GROUND, color: GROUND }}
                >
                  {busy === `repo-${p.id}` ? "adding…" : "+ whitelist repo"}
                </button>
              </div>
            </div>
          ))}

          <div className="flex gap-2 rounded-xl border border-white/10 bg-white/[0.015] p-4">
            <input
              value={newProject}
              onChange={(e) => setNewProject(e.target.value)}
              placeholder="new project — an application or domain"
              disabled={!data.live}
              className={`${FONT_MONO} w-full rounded-lg border border-white/15 bg-white/[0.03] px-3.5 py-2 text-sm text-white placeholder:text-[#e9edff]/30 focus:border-white/40 focus:outline-none disabled:opacity-50`}
            />
            <button
              onClick={onCreateProject}
              disabled={!data.live || !newProject.trim() || busy !== null}
              className={`${FONT_MONO} shrink-0 rounded-full border px-5 py-2 text-sm font-medium transition disabled:opacity-40`}
              style={{ borderColor: GROUND, color: GROUND }}
            >
              {busy === "create" ? "creating…" : "◈ create project"}
            </button>
          </div>
          <div className={LABEL} style={{ color: "rgba(233,237,255,0.3)" }}>
            a repo maps to exactly one project · keys minted by onboarding are scoped to it
            {!data.live && " · demo data"}
          </div>
        </div>

        {/* the doorbell */}
        <div className="rounded-xl border border-white/10 bg-white/[0.015] p-5 lg:sticky lg:top-4 lg:self-start">
          <div className={LABEL} style={{ color: "rgba(233,237,255,0.4)" }}>
            pairing requests — approve only codes you recognize
          </div>
          <div className="mt-3 space-y-3">
            {requests.map((r) => (
              <div key={r.id} className="rounded-lg border border-white/10 p-3">
                <div className="flex items-baseline justify-between gap-2">
                  <span className={`${FONT_MONO} text-lg tracking-[0.2em] text-white`}>{r.user_code}</span>
                  <span className={`${FONT_MONO} text-[10px] text-[#e9edff]/40`}>{fmtAgo(r.created_at)}</span>
                </div>
                <div className={`${FONT_MONO} mt-1 truncate text-xs text-[#e9edff]/70`}>{r.remote}</div>
                <div className={`${FONT_MONO} text-[11px] text-[#e9edff]/45`}>from {r.label}</div>
                <div className={`${FONT_MONO} mt-1.5 text-[11px]`}>
                  {r.project_name ? (
                    <span style={{ color: GROUND }}>→ project “{r.project_name}” · key: read+write</span>
                  ) : (
                    <span style={{ color: MAGENTA }}>repo not whitelisted — register it above to approve</span>
                  )}
                </div>
                <div className="mt-2.5 flex gap-2">
                  <button
                    onClick={() => onDecide(r.id, "approve")}
                    disabled={!data.live || !r.project_name || busy !== null}
                    className={`${FONT_MONO} rounded-full border px-3.5 py-1 text-xs transition disabled:opacity-40`}
                    style={{ borderColor: GROUND, color: GROUND }}
                  >
                    {busy === `approve-${r.id}` ? "approving…" : "approve"}
                  </button>
                  <button
                    onClick={() => onDecide(r.id, "deny")}
                    disabled={!data.live || busy !== null}
                    className={`${FONT_MONO} rounded-full border border-white/12 px-3.5 py-1 text-xs text-[#e9edff]/50 transition hover:border-[#ff5da2]/60 hover:text-[#ff5da2] disabled:opacity-40`}
                  >
                    {busy === `deny-${r.id}` ? "denying…" : "deny"}
                  </button>
                </div>
              </div>
            ))}
            {requests.length === 0 && (
              <p className={`${FONT_MONO} py-6 text-center text-sm text-[#e9edff]/35`}>
                no pending pairings — developers start one with the brainiac-onboard skill
              </p>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
