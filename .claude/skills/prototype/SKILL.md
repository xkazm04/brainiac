---
name: prototype
description: Iteratively prototype a console page/component through directional variants behind a tab switcher, then consolidate the winner. Use when the user wants to explore multiple design approaches for a Brainiac console surface (visual appeal, creativity, UX clarity).
allowed-tools: Read, Write, Edit, Bash, Glob, Grep, Agent
---

# Prototype — Directional Variant Workflow (Brainiac console)

Adapted from the Personas `/prototype` skill for this repo. A disciplined
A/B loop: produce radically different directional variants behind a tab
switcher, let the user prune/fuse across rounds until one wins, then
consolidate. The console's identity was itself picked this way (two rounds
of the design lab → "Interference" structure fused with "Spectrum" theme).

## When to use

The user wants to explore a page or pillar component with an **open
direction** and a **visual quality bar** — "prototype three approaches for
the reviews page", "iterate until this is amazing". Not for fixed-scope
edits, bug fixes, or non-visual code.

## Ground rules — the Brainiac identity (calibrate BEFORE writing variants)

1. **Read `console/src/design/theme.ts` first.** It is the single source of
   truth: Space Grotesk display (`FONT_DISPLAY`) + JetBrains Mono microcopy
   (`FONT_MONO`, `LABEL`), dark-only `BG #08070c`, and the **EEG band
   system** — every module is tuned to a band (`MODULE_BAND`) and takes its
   accent from `band(key)`. Gold gamma = canonical/constructive (the brand
   color); `MAGENTA` = contradiction/destructive. New variants must express
   the page's band, not invent a palette.
2. **Quality reference: `console/src/home/Home.tsx`.** Mine it for the
   layout shape (full-bleed instrument + mono stats strip + spine story),
   motion language, and typography rhythm. Variants should feel like
   siblings of the home page, differing in *mental model*, not in brand.
3. **The wave is the brand.** Each variant should carry one wave-physics or
   instrument metaphor through layout, motion, and copy — annotations on a
   recording, phase alignment, band tuning, constructive/destructive light.
   Decorative dashboards with no metaphor are a round-1 failure mode.
4. **Motion policy** (from theme.ts): ambient canvas motion is allowed ONLY
   on hero/brand surfaces, always behind a `useReducedMotion` static
   fallback. Utility pages get entry animations and hover/click-gated
   transitions — no infinite loops, no `hover:-translate-y-*`.
5. **Data must be real.** Pages wire to the REST layer via
   `console/src/lib/api.ts` (server components + server actions; the bearer
   token never reaches the browser). Variants may take mock props while
   iterating, but the winning variant is consolidated against live data
   before the round closes. Surface *meaningful* fields (memory kind, team,
   policy rule, age, provenance) — name-only chips are a failure mode.

## Workflow

### Phase 0 — starting point
Ask which page/component to prototype if not already named. Verify the file
actually renders (grep for imports/usages) before touching it — don't trust
the filename.

### Phase 1 — scaffold a variant switcher
Local to the page: a small client component with a floating pill switcher
(see git history of `console/src/design/DesignLab.tsx` for the pattern —
localStorage + `?variant=` persistence). Variants live as siblings:
`<dir>/variants/<Name>Variant{A,B,C}.tsx`, each self-contained with a header
comment naming its metaphor. Baseline (current page, if any) stays the
default tab.

### Phase 2 — 3 directional variants
Three per round for a new page (two when refining an existing one). Each is
a different **mental model** of the same data, not a reskin. Every variant
takes identical props. Degrade gracefully when the API is down (the home
page's `live: null` pattern).

### Phase 3 — iterate by subtraction and fusion
- Rejection → delete the file, import, and tab immediately.
- Fusion → extract the praised element into the survivor, delete the donor.
- Specific feedback → refine in place; never spawn a new variant unasked.
- Hoist shared pieces the moment two variants render the same structure.
End each round with an explicit menu of changes; don't auto-advance.

### Phase 4 — consolidate the winner
On "this is the one" / "make X the baseline": remove the switcher, delete
non-winners from disk and imports, wire live data, run
`npm run typecheck && npm test && npm run build` in `console/`, commit.
Refactor into subcomponents only when explicitly asked.

## Guardrails

- **Typography**: body copy ≥ `text-sm`; `text-[10px]`/`text-[11px]` only
  inside the `LABEL` token for uppercase-tracked instrument labels.
- **Framer-motion on SVG**: never animate raw `cx`/`cy`/`r` — wrap in
  `motion.g` and animate transforms (`x`/`y`/`scale`) or `pathLength`.
- **Canvas loops**: rAF with cleanup, `ResizeObserver` for sizing, DPR clamp
  at 2, and a reduced-motion single-frame branch.
- **`useMemo` is not `useEffect`** — grep your output for `useMemo(.*set[A-Z]`
  before finishing a round.
- **One-shot typecheck per round**, not per file.
- **Atomic commit per round** (variants generated / pruning / consolidation
  each get their own commit).
- Don't touch files outside the prototype scope; if `git status` shows other
  modified files, leave them alone.

## Exit checklist

- [ ] Winner is the only rendered component; switcher and losers deleted.
- [ ] Page reads its accent from `MODULE_BAND`, fonts from theme tokens.
- [ ] Live data wired through `src/lib/api.ts`; graceful when API is down.
- [ ] `npm run typecheck` + `npm test` + `npm run build` green in `console/`.
- [ ] Summarize in 1-2 sentences which metaphor won and why.
