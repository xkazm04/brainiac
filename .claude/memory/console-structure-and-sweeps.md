# Console SPA layout and UI-managed sweeps — where new surfaces and jobs go

Two structural decisions that retired recurring bugs; new work should follow
them rather than re-invent:

**Operator console is an SPA under `console/app/console/modules/`.** Every
operator module lives there (analytics, audit, disputes, divergence, docs,
graph, health, ingest, keys, memories, projects, reviews, skills, standards) and
`app/console/layout.tsx` owns the chrome — there is **no path-matching gating
anywhere**. `isPublicSurface` in `console/src/design/routes.ts` is the
middleware's single allow-list, and `PRODUCT_ROUTES` in the same file is the one
list nav/home render from (`CONSOLE_MODULES` is just an alias of it). Old paths
307 in `next.config`. Two separate route-gating drift bugs were retired by this
structure, so don't reintroduce per-page gating. A new operator surface = a
directory under `app/console/modules/<name>/` plus a `PRODUCT_ROUTES` entry.

**Periodic jobs are UI-managed sweeps, not cron or a CLI habit.** The
`sweep_schedules` table (migration `0018`) is global operator config —
deliberately no `org_id` and no RLS, just the grant. The tick runs inside the
worker on the admin pool; admin endpoints live at `/v1/ops/sweeps` and the
SweepControl UI sits on the health and divergence pages. A new periodic job =
a new `sweep_schedules` kind + an arm in `sweeps.rs::execute`.

Related: the practice-divergence detector (pipeline `divergence.rs`,
`scan-divergence` CLI, `/v1/analytics/practice-divergence`, console
`/console/divergence`) is the *detector*; the Library owns the resulting
artifact via `ratify_divergence` — do not build a second divergence pipeline.
