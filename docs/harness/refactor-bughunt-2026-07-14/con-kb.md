> Context: Console: Knowledge Base Explainer
> Total: 5 (Critical: 0, High: 0, Medium: 3, Low: 2)

## 1. Palette hardcoded as raw hsla/hex literals instead of theme tokens
- **Severity**: Medium
- **Lens**: code-refactor
- **Category**: duplication-palette
- **File**: console/src/kb/illustrations.tsx (68, 109, 139, 144, 153, 195, 230, 266, 272, 278, 284, 381–393, 480, 503); console/src/kb/KnowledgeBase.tsx (98, 277–278, 352, 368, 450, 479–480); console/src/kb/ProjectionDiagram.tsx (67, 129, 153)
- **Scenario**: The gamma/beta band tints are re-typed as string literals dozens of times — `"hsla(46,90%,60%,0.03)"`, `"hsla(46,90%,68%,0.22)"`, `"hsla(158,90%,60%,0.10)"`, the page ground `"#08070c"` (already exported as `BG` in theme, hardcoded ~8× across illustrations + KnowledgeBase:427), and `"rgba(255,255,255,0.02)"`. The theme module already provides `band(key, lightness, alpha)`, `bandGlow()`, and `BG`/`PANEL` that produce exactly these values. The most fragile instance: `Stamp` derives its 8%-alpha fill by string surgery — `tone.replace(", 1)", ", 0.08)")` (KnowledgeBase.tsx:98) — which only works because `band()` happens to emit a trailing `, 1)`; change the default alpha and every filled stamp silently loses its tint.
- **Root cause**: SVG attributes were authored inline as literal color strings during a presentation-heavy build, and the `band()`/`bandGlow()` helpers were never threaded down into the drawings.
- **Impact**: A palette or brand-hue change (e.g. adjusting gamma lightness) has to be found and edited across three files and ~40 sites; the values silently drift from the theme, and the `.replace()` tint trick is a booby trap for the next theme edit.
- **Fix sketch**: Route all band tints through `band("gamma", L, A)` / `bandGlow()` and use the exported `BG`/`PANEL`; replace the `Stamp` `.replace()` with an explicit `band(..., 0.08)` (or a `tint(tone, 0.08)` helper). No behavior change, one source of truth.

## 2. PipelineFigure station coordinates are a fixed 6-slot array decoupled from `stages.length` → undefined/NaN SVG coords
- **Severity**: Medium
- **Lens**: bug-hunter
- **Category**: nan-in-svg
- **File**: console/src/kb/illustrations.tsx:312 (used 345–356 and 389–401)
- **Scenario**: `const XS = [120, 216, 312, 408, 560, 668]` hardcodes exactly six x-positions. `running = stages.slice(0, 4)` maps with `XS[i]`, and `stages.slice(4)` maps with `XS[4 + i]`. Today `COMPOSE_STAGES` has exactly 6 entries so it lines up. Add a seventh compose stage (the honesty test in kb-data.test.ts pins each stage's *status* but never its *count*, so a 7th stage passes CI) and `XS[6]` is `undefined` → `cx={undefined}` / `x={undefined}` on that station's `<circle>`/`<text>`. An undefined/NaN coordinate makes the element silently drop out of the render — no error, just a missing station on a figure whose whole job is completeness.
- **Root cause**: The layout positions were hand-placed for the current data shape rather than derived, so the figure geometry is implicitly coupled to a magic length that nothing enforces.
- **Impact**: A future data edit (very plausible for a pipeline description) produces a silently broken diagram that still passes type-check and the honesty tests. Also: fewer than 5 stages leaves the "dark side" empty with no guard.
- **Fix sketch**: Derive x-positions from `stages.length` (e.g. compute a step, or assert `stages.length === XS.length` and split the running/dark boundary from a named constant), or guard `XS[i] ?? …`. At minimum, add a length assertion so the coupling fails loudly.

## 3. `dim()` helper and `MINT`/`ALPHA` aliases re-declared in every KB file
- **Severity**: Medium
- **Lens**: code-refactor
- **Category**: duplication-helper
- **File**: console/src/kb/KnowledgeBase.tsx:60–63; console/src/kb/illustrations.tsx:25–27; console/src/kb/ProjectionDiagram.tsx:20–22; console/src/kb/RotCurve.tsx:37
- **Scenario**: The identical `const dim = (a: number) => \`rgba(233,237,255,${a})\`;` is copy-pasted into all four component files, and `const MINT = band("beta"); const ALPHA = band("alpha");` into three of them. The `233,237,255` base is already `#e9edff` — the theme's `INK`, and theme already ships `INK_DIM`/`INK_FAINT`/`BORDER` off that same base.
- **Root cause**: Each drawing file was written self-contained; the shared ink helper and the two band aliases were never promoted to `design/theme`.
- **Impact**: Four definitions to keep in sync; if the ink base ever changes it must be edited in four places (and would still diverge from theme's `INK`). Pure duplication with no offsetting benefit.
- **Fix sketch**: Export `dim` (or an `ink(alpha)`), plus `MINT`/`ALPHA` aliases, from `design/theme` and import them; delete the four local copies.

## 4. ProjectionDiagram reimplements the shared `Frame` wrapper and re-types the mono font as a raw string
- **Severity**: Low
- **Lens**: code-refactor
- **Category**: duplication-structure
- **File**: console/src/kb/ProjectionDiagram.tsx:44–50 (and `"var(--font-mono)"` at 30, 33, 61, 64, 84, 87, 100, 103, 108, 111, 116, 121, 125, 130, 133, 137, 140, 154, 157, 160)
- **Scenario**: illustrations.tsx defines a shared `Frame` component (a `overflow-x-auto rounded border` div wrapping a `<svg role="img" aria-label … minWidth>`) that all eight illustrations reuse. ProjectionDiagram hand-rolls the same wrapper (`<div className="w-full overflow-x-auto"><svg … min-w-[640px]>`) instead of using `Frame`, so the framing markup and its a11y contract are maintained twice. Separately it inlines `fontFamily="var(--font-mono)"` ~20 times where illustrations.tsx uses a `MONO` constant.
- **Root cause**: `Frame` is not exported from illustrations.tsx, so the neighboring diagram couldn't reuse it and duplicated the pattern; the mono constant was likewise never shared.
- **Impact**: Two copies of the illustration frame (border, scroll behavior, aria wiring) drift independently; the repeated font literal is noise. Low, but it is the exact "repeated SVG primitive logic across illustrations/ProjectionDiagram" pattern.
- **Fix sketch**: Export `Frame` (and a shared `MONO`) from illustrations.tsx (or a small `kb/svg` module) and have ProjectionDiagram render inside it; replace the raw font strings with the constant.

## 5. `Flow.note` and `Flow.from` are populated but never rendered (dead data)
- **Severity**: Low
- **Lens**: code-refactor
- **Category**: dead-code
- **File**: console/src/kb/kb-data.ts:66–107 (fields `from`, `note`); consumer console/src/kb/KnowledgeBase.tsx:246–256
- **Scenario**: The `Flow` interface carries `from`, `to`, `label`, `gate`, `allowed`, `note`. The ASYMMETRY legend renderer only reads `f.label`, `f.allowed`, and `f.gate`. `f.note` — which holds the richest persuasive copy in this data ("Bidirectional sync would make the wiki a second source of truth again…") — and `f.from` are never shown anywhere in the UI. (`to` survives only because kb-data.test.ts:142 keys on it; `note`/`from` are referenced solely via the test's `JSON.stringify` audience sweep, not by any renderer.)
- **Root cause**: The data model was authored richer than the legend that consumes it — either the `note` prose was meant to render (a tooltip/expander that never got wired) or the fields are leftover scaffolding.
- **Impact**: Authored, audience-checked copy is inert; readers get a terse "does not exist" instead of the reasoning. Maintenance cost of fields nothing displays.
- **Fix sketch**: Decide one way — either render `note` (e.g. as the legend row's second line / a `<title>`), or drop `from`/`note` from the interface and data. Keep `to` (test-referenced).
