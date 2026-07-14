> Context: Console: Shell + Nav + Design System
> Total: 5 (Critical: 0, High: 0, Medium: 4, Low: 1)

## 1. MODULE_BAND is a dead, drifted second source of truth for module→band
- **Severity**: Medium
- **Lens**: code-refactor
- **Category**: duplication/dead-source-of-truth
- **File**: console/src/design/theme.ts:38-47 (see console/src/design/routes.ts:28,33-50)
- **Scenario**: `MODULE_BAND` maps module keys to bands (home→gamma, reviews→alpha, graph→gamma, analytics→beta, demo→beta, memories→delta, ingest→theta, disputes→theta). `PRODUCT_ROUTES` in routes.ts carries its OWN `band` field per route, and routes.ts's header comment claims "Bands mirror theme.ts MODULE_BAND." A grep of the whole repo shows nothing imports `MODULE_BAND` — the only references are its definition, the routes.ts comment, and `.claude/skills/prototype/SKILL.md` (which tells authors "Page reads its accent from MODULE_BAND"). The two maps have already drifted: `MODULE_BAND` is missing `health`, `docs`, `divergence`, and `keys`, and still lists `home`/`demo` which are not product routes.
- **Root cause**: When accents were consolidated into `PRODUCT_ROUTES.band` (the map the nav/chrome actually read via `routeAccent`), the older `MODULE_BAND` lookup table was left behind instead of deleted, and the "mirror" comment was never reconciled.
- **Impact**: A maintainer trusting the comment or the SKILL.md will edit `MODULE_BAND` to recolor a module and see no effect, or will read a stale/contradictory band for a module that isn't even in the table. A dead export masquerading as the source of truth is a maintenance trap and invites re-drift.
- **Fix sketch**: Delete `MODULE_BAND` (and its `demo` handling) from theme.ts, or, if a keyed lookup is wanted, derive it from `PRODUCT_ROUTES` (`Object.fromEntries(PRODUCT_ROUTES.map(r => [r.segment, r.band]))`). Remove the "mirror MODULE_BAND" comment in routes.ts and update SKILL.md to point at `routeAccent`/`PRODUCT_ROUTES`.

## 2. Operator caption shows the internal segment, not the nav label ("memories" vs "archive")
- **Severity**: Medium
- **Lens**: bug-hunter
- **Category**: active-state/label-mismatch
- **File**: console/app/chrome.tsx:58-65
- **Scenario**: The persistent top-row caption renders `{active.segment} · {routeBandLabel(active.band)}`. Three routes deliberately give `label` ≠ `segment` (routes.ts:37 memories→"archive", :45 docs→"pages", :48 divergence→"standards"). On `/console/memories` the bottom-row nav link highlights "archive" while the caption above it reads "memories · delta band"; likewise "pages"/"docs" and "standards"/"divergence". `ProductRoute.label` is documented as "may differ from the segment," yet the caption ignores it.
- **Root cause**: `routeForPath` returns the full route object, but the caption grabbed `.segment` (the URL key) instead of `.label` (the user-facing name) — the same field the nav renders one row below.
- **Impact**: On 3 of 10 modules the persistent header identifies the current module by a name the user never sees in the nav, breaking the "you are here" contract the two-row chrome is built around and reading as a governance-console inconsistency.
- **Fix sketch**: Use `active.label` for the caption text (keep the band label as-is). One-word change from `{active.segment}` to `{active.label}`.

## 3. NavStatus.tsx is fully dead — a superseded duplicate of NavDashboard
- **Severity**: Medium
- **Lens**: code-refactor
- **Category**: dead-code/duplication
- **File**: console/src/components/NavStatus.tsx:1-70
- **Scenario**: `NavStatus` is `export default`ed but nothing imports it (repo-wide grep: only self-reference plus an unrelated `NavStatusPayload` type in app/api/status/route.ts). NavDashboard's own header comment names it as the retired ancestor: "Its ancestor, NavStatus, mixed a link into the status badges; splitting the two is the point of the two-row header." The whole ~70-line component is orphaned, and it re-declares the identical `Status` interface, the identical `POLL_MS = 30_000`, and a byte-for-byte copy of the fetch/setInterval/`cancelled`-flag polling effect that lives in NavDashboard.
- **Root cause**: When the single-row status strip was split into the display-only NavDashboard (top row) plus a separate nav row, the old combined component was left in the tree instead of being removed.
- **Impact**: Dead surface that still looks live: a future maintainer may "fix" a status bug in NavStatus and see nothing change, or wire it back in and reintroduce the link-in-status-badge regression the split was meant to kill. It also carries a third copy of the poll logic and `Status` shape to keep in sync.
- **Fix sketch**: Delete NavStatus.tsx. If the `waiting`-badge link ("N to review" → /console/reviews) is still wanted, fold that one affordance into NavDashboard's nav row rather than keeping the whole duplicate component.

## 4. SectionRail only half-honors prefers-reduced-motion
- **Severity**: Medium
- **Lens**: bug-hunter
- **Category**: reduced-motion/motion-policy
- **File**: console/src/components/SectionRail.tsx:50-53,129-156
- **Scenario**: The component's own docblock promises reduced-motion handling ("Honouring prefers-reduced-motion, we jump…"), and `reduce = !!useReducedMotion()` is computed — but `reduce` is consulted in exactly ONE place: the click scroll behavior (`behavior: reduce ? "auto" : "smooth"` at :87). The spring-smoothed progress bar `progress = useSpring(scrollYProgress, …)` (:53) driving `scaleX` on the bottom line (:152-156) still animates continuously, and the active-section underline `motion.span` with `transition={{ type: "spring", … }}` (:129-134) still springs between sections. A reduced-motion visitor scrolling /pitch or /kb gets a bouncing progress bar and a sliding marker anyway.
- **Root cause**: Reduced-motion was retrofitted only onto the explicit click handler; the two framer-driven ambient animations predate the guard and were never gated on `reduce`.
- **Impact**: Violates the component's stated contract and theme.ts's motion policy ("Utility pages: entry animations and hover/click-gated transitions only") for the exact users who opted out — an accessibility/motion-sensitivity regression on the public long-form pages.
- **Fix sketch**: When `reduce` is true, bind the bar to `scrollYProgress` directly (skip `useSpring`) and set the marker's `transition={{ duration: 0 }}` (or render the underline without `layoutId`), so both settle instantly.

## 5. Offline/demo amber #f0b429 is hardcoded across the shell, absent from the theme
- **Severity**: Low
- **Lens**: code-refactor
- **Category**: design-system/hardcoded-color
- **File**: console/src/components/NavDashboard.tsx:89 (also DemoBanner.tsx:25,28 and dead NavStatus.tsx:46)
- **Scenario**: The "API unreachable / demo data" warning color `#f0b429` is written as a literal in NavDashboard (`dot = status.live ? MINT : "#f0b429"`) and in DemoBanner both as `#f0b429` (:28) and as its rgb twin `rgba(240,180,41,…)` for border/background (:25) — 240,180,41 == #f0b429. theme.ts defines band accents, GOLD, MAGENTA and the ink ramp but has no token for this amber warning state, so every offline/demo affordance re-spells the same color (and in two syntaxes, so a grep for one form misses the others).
- **Root cause**: The offline/degraded state was styled ad hoc per component; the theme's palette only anticipated the band spectrum plus contradiction magenta, never a "warning/demo" tone.
- **Impact**: The demo/offline signal cannot be retuned in one place, the hex-vs-rgba split defeats find-and-replace, and there's no guarantee the dot, banner, and any future warning agree. Minor today, drift-prone as more surfaces show the offline state.
- **Fix sketch**: Add a `WARN`/`AMBER` token to theme.ts (e.g. `export const WARN = "#f0b429"` plus a `WARN_SOFT`/glow helper mirroring `bandGlow`) and reference it from NavDashboard and DemoBanner (and drop the copy when NavStatus is deleted per finding #3).
