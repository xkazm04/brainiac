"use client";

/*
 * The skills catalog — the org's shelf of packaged procedures for coding
 * agents. Cards rank by pulse (a shelf sorted by what actually gets used);
 * the selected card expands into versions, per-team usage, and the exact
 * tool call an agent makes to pull the bundle. A draft is listed but plainly
 * marked unservable — the shelf never pretends a signature exists.
 */

import { useState } from "react";

import {
  band,
  BORDER,
  FONT_DISPLAY,
  FONT_MONO,
  GOLD,
  INK,
  INK_DIM,
  INK_FAINT,
  LABEL,
  PANEL,
  withAlpha,
} from "@/design/theme";
import type { LibrarySkill, SkillDetail } from "@/lib/types";

const BETA = band("beta");

const totalUses = (d: SkillDetail | undefined): number =>
  d?.usage.reduce((n, u) => n + u.uses, 0) ?? 0;

const isoDay = (value: string | null | undefined): string => {
  if (!value) return "—";
  const t = new Date(value);
  return Number.isNaN(t.getTime()) ? "—" : t.toISOString().slice(0, 10);
};

function SkillCard({
  skill,
  detail,
  open,
  onToggle,
}: {
  skill: LibrarySkill;
  detail?: SkillDetail;
  open: boolean;
  onToggle: () => void;
}) {
  const uses = totalUses(detail);
  const maxUses = Math.max(1, ...(detail?.usage.map((u) => u.uses) ?? [1]));
  const current = detail?.versions.find((v) => v.published);
  return (
    <article
      className="flex flex-col overflow-hidden rounded-xl"
      style={{ background: PANEL, border: `1px solid ${open ? withAlpha(BETA, 0.4) : BORDER}` }}
    >
      <button onClick={onToggle} className="flex flex-col gap-2 px-6 py-5 text-left" aria-expanded={open}>
        <div className="flex flex-wrap items-center gap-3">
          <h2 className={`${FONT_DISPLAY} text-xl`} style={{ color: INK }}>
            {skill.name}
          </h2>
          {skill.domain && (
            <span
              className={`${FONT_MONO} rounded-md px-2 py-0.5 text-[10px] uppercase tracking-[0.14em]`}
              style={{ color: BETA, border: `1px solid ${withAlpha(BETA, 0.4)}` }}
            >
              {skill.domain}
            </span>
          )}
          {skill.downloadable ? (
            <span className={`${FONT_MONO} text-[11px]`} style={{ color: INK_FAINT }}>
              {current ? `v${current.semver}` : "published"}
            </span>
          ) : (
            <span
              className={`${FONT_MONO} rounded-md px-2 py-0.5 text-[10px] uppercase tracking-[0.14em]`}
              style={{ color: GOLD, border: `1px dashed ${withAlpha(GOLD, 0.5)}` }}
            >
              draft — nobody signed it, agents get nothing
            </span>
          )}
          <span className={`${FONT_MONO} ml-auto text-[11px]`} style={{ color: uses > 0 ? BETA : INK_FAINT }}>
            {uses > 0 ? `${uses} uses / 30d` : "no pulse yet"}
          </span>
        </div>
        {skill.description && (
          <p className="max-w-2xl text-[14px] leading-snug" style={{ color: INK_DIM }}>
            {skill.description}
          </p>
        )}
      </button>

      {open && detail && (
        <div className="flex flex-col gap-5 border-t px-6 py-5" style={{ borderColor: BORDER }}>
          {/* the pulse */}
          {detail.usage.length > 0 && (
            <div className="flex flex-col gap-1.5">
              <span className={LABEL} style={{ color: INK_FAINT }}>
                the pulse · per team, never per person
              </span>
              {detail.usage.map((u, i) => (
                <div key={`${u.team ?? "org"}-${i}`} className="flex items-center gap-3">
                  <span className={`${FONT_MONO} w-24 shrink-0 truncate text-[11px]`} style={{ color: INK_DIM }}>
                    {u.team ?? "org-scoped"}
                  </span>
                  <span
                    className="h-2 rounded-sm"
                    style={{ width: `${Math.max(3, (u.uses / maxUses) * 100)}%`, background: withAlpha(BETA, 0.6) }}
                  />
                  <span className={`${FONT_MONO} text-[11px]`} style={{ color: INK_FAINT }}>
                    {u.uses}
                  </span>
                </div>
              ))}
            </div>
          )}

          {/* versions — drafts visible, marked, never served */}
          <div className="flex flex-col gap-1.5">
            <span className={LABEL} style={{ color: INK_FAINT }}>
              versions
            </span>
            {detail.versions.map((v) => (
              <div key={v.semver} className={`${FONT_MONO} flex items-baseline gap-3 text-[12px]`}>
                <span style={{ color: v.published ? BETA : GOLD }}>v{v.semver}</span>
                <span style={{ color: INK_FAINT }}>
                  {v.published ? `published ${isoDay(v.published_at)}` : "draft — awaiting a named human"}
                </span>
              </div>
            ))}
          </div>

          {/* how an agent pulls it */}
          <div className="flex flex-col gap-1.5">
            <span className={LABEL} style={{ color: INK_FAINT }}>
              how an agent pulls it
            </span>
            <pre
              className={`${FONT_MONO} overflow-x-auto rounded-lg p-3 text-[12px] leading-relaxed`}
              style={{ background: "rgba(255,255,255,0.02)", border: `1px solid ${BORDER}`, color: INK_DIM }}
            >
              {`skill_fetch { "slug": "${skill.slug}" }        # MCP\nGET /v1/library/skills/${skill.slug}/download  # REST · lib:read`}
            </pre>
          </div>
        </div>
      )}
    </article>
  );
}

export default function SkillsCatalog({
  skills,
  details,
}: {
  skills: LibrarySkill[];
  details: Record<string, SkillDetail>;
}) {
  // Shelf order: what actually gets used first; drafts sink to the bottom.
  const ranked = [...skills].sort(
    (a, b) =>
      Number(b.downloadable) - Number(a.downloadable) ||
      totalUses(details[b.id]) - totalUses(details[a.id]) ||
      a.slug.localeCompare(b.slug),
  );
  const [openId, setOpenId] = useState<string | null>(ranked[0]?.id ?? null);

  return (
    <main className="mx-auto flex max-w-5xl flex-col gap-8 px-6 py-12">
      <header className="flex flex-col gap-3">
        <span className={LABEL} style={{ color: INK_FAINT }}>
          the library · skills
        </span>
        <h1 className={`${FONT_DISPLAY} text-3xl`} style={{ color: INK }}>
          Packaged procedures your agents pull and run
        </h1>
        <p className="max-w-2xl text-[15px] leading-snug" style={{ color: INK_DIM }}>
          A skill is a versioned bundle in the format coding agents already load. Only versions a
          named human published are ever served — a draft is listed here so maintainers can see
          it, and serves nothing. The shelf ranks by pulse: what nobody pulls, the org eventually
          retires, out loud.
        </p>
      </header>

      {skills.length === 0 ? (
        <p
          className="rounded-xl p-6 text-[14px]"
          style={{ background: PANEL, border: `1px solid ${BORDER}`, color: INK_DIM }}
        >
          The shelf is empty. Skills land as drafts over the API and appear here the moment a
          named human publishes one.
        </p>
      ) : (
        <section className="flex flex-col gap-4">
          {ranked.map((s) => (
            <SkillCard
              key={s.id}
              skill={s}
              detail={details[s.id]}
              open={openId === s.id}
              onToggle={() => setOpenId(openId === s.id ? null : s.id)}
            />
          ))}
        </section>
      )}
    </main>
  );
}
