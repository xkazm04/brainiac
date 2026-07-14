> Context: Console: Home + Station Modules
> Total: 5 (Critical: 0, High: 1, Medium: 3, Low: 1)

## 1. Reduced-motion hero field goes blank on any resize
- **Severity**: High
- **Lens**: bug-hunter
- **Category**: broken-render
- **File**: console/src/home/Home.tsx:109-118,189-191
- **Scenario**: A user with `prefers-reduced-motion` loads the page (hero draws once), then the canvas resizes — window resize, sidebar/devtools open, mobile orientation change, or the initial `ResizeObserver` fire after fonts settle. The field turns blank and never repaints.
- **Root cause**: The animation self-schedules only when motion is allowed: `if (!reduce) raf = requestAnimationFrame(draw)` (line 189), so under reduced motion `draw` runs exactly once from the single kick at line 191. But `resize()` (lines 109-115, wired to a `ResizeObserver` at 117-118) assigns `canvas.width`/`canvas.height`, which resets and clears the drawing surface. Nothing redraws afterward because no RAF is queued.
- **Impact**: The primary hero visual — the whole "one wave" pitch — disappears for reduced-motion users after the first layout change, leaving only the DOM emitter labels floating over an empty box. Silent; no error.
- **Fix sketch**: In `resize()`, after re-sizing, call `draw(performance.now())` (or set a `needsRedraw` flag the effect drains) so a static single frame is repainted whenever the surface is cleared, independent of the RAF loop. Same fix covers reduced-motion emitter drags, which also currently don't repaint.

## 2. Station figure is invoked as a plain function, not rendered as an element
- **Severity**: Medium
- **Lens**: bug-hunter
- **Category**: react-antipattern
- **File**: console/src/home/StationModules.tsx:904-905
- **Scenario**: `StationModule` resolves `const Figure = FIGURES[kind]` then returns `Figure({ tone, caption, active })` instead of `<Figure ... />`. Each figure calls hooks (`useStep` → `useState`/`useEffect`/`useReducedMotion`, plus `useReducedMotion` in Health), so those hooks execute inside `StationModule`'s own fiber rather than a child component instance.
- **Root cause**: Treating the component map as a lookup of render functions and calling them directly. It happens to work only because `kind` is fixed per mounted `StationModule` (STATIONS is static), so the same function — and thus the same hook order — is always invoked.
- **Impact**: The figure gets no fiber of its own: no error-boundary isolation, no independent memoization/reconciliation, and it is one step from a "rendered more/fewer hooks" crash the moment `kind` ever becomes dynamic. It also muddies React DevTools (figures don't appear as components).
- **Fix sketch**: Return `<Figure tone={tone} caption={caption} active={active} />`. `FIGURES` already types values as `(p: FigureProps) => ReactNode`, which JSX accepts directly.

## 3. Home.tsx is a god-file: 165-line StationWave + 100-line inline STATIONS config
- **Severity**: Medium
- **Lens**: code-refactor
- **Category**: god-component
- **File**: console/src/home/Home.tsx:263-366,592-765
- **Scenario**: The 765-LOC module bundles three unrelated concerns: the interactive canvas page component, a 100-line `STATIONS` data literal that interleaves copy, live/demo branching, routes, tones, and CTA labels (263-366), and `StationWave` — a 165-line second figure component with four hand-rolled SVG branches (beat/composed/trace/default, 592-765).
- **Root cause**: The in-progress refactor moved the "artifact" figures into StationModules.tsx but left the parallel `StationWave` abstract figures and the station config inline in the page. The two figure systems (right-column `StationWave`, left-column `StationModule`) now render one abstract + one concrete figure per station — visually redundant and split across files by accident of history.
- **Impact**: Editing station copy, routes, or a figure means paging through an 800-line file that mixes canvas physics, data, and SVG; the split makes it easy to update one figure family and forget the other. `StationWave` belongs next to the StationModules figures it mirrors.
- **Fix sketch**: Extract `StationWave`/`WaveKind` into `home/StationWave.tsx` (or fold into StationModules), and lift the `STATIONS` array into a `home/stations.ts` data module (with the live/demo caption builders as pure functions). Home then just maps over it.

## 4. Accent colors hardcoded as raw hsla/rgba/hex literals instead of theme tokens
- **Severity**: Medium
- **Lens**: code-refactor
- **Category**: design-token-duplication
- **File**: console/src/home/Home.tsx:729,739,746 + StationModules.tsx:636-637,661-663
- **Scenario**: `StationWave` hardcodes the gold source wave as `"hsla(46,90%,68%,0.5)"` (729) and the contradiction wave as `"rgba(255,93,162,0.6)"` (739) — even though `Home` already imports `GOLD` and `MAGENTA` from theme.ts. StationModules repeats the same pattern: `"hsla(224,90%,72%,0.5)"` (636-637), `"hsla(262,90%,72%,0.45)"` (661-663), and the `hover:text-[#f3c74f]` class literal recurs ~7× across Home (380, 464, 473, 557, 572, 575, 580).
- **Root cause**: The file has a `soft(tone, a)` helper (StationModules 56-66) and theme tokens (`GOLD`, `MAGENTA`, `band()`) built exactly to derive translucent accents, but the figure SVGs were authored with copy-pasted color strings that duplicate the token values by hand.
- **Impact**: The single source of truth for the band hues is silently forked. A palette change in theme.ts updates the wave canvas and captions but not these figures, so they drift out of tune — the exact failure the theme layer exists to prevent. Also invites the invalid-CSS `${tone}55` bug that `soft()`'s own doc-comment warns about.
- **Fix sketch**: Replace literals with `soft(GOLD, 0.5)`, `soft(MAGENTA, 0.6)`, `soft(band("beta"), 0.5)`, etc., and promote the repeated `#f3c74f` hover into a theme constant or shared Tailwind class.

## 5. StationWave defines motion presets it then bypasses in the 01–03 branch
- **Severity**: Low
- **Lens**: code-refactor
- **Category**: inconsistent-duplication
- **File**: console/src/home/Home.tsx:617-626,726-756
- **Scenario**: `inView` and `sumView` preset objects are defined (617-626) and spread cleanly in the beat/composed/trace branches (`{...inView}`, `{...sumView}`). The default 01–03 branch (726-756) instead re-hand-writes the identical `initial`/`whileInView`/`viewport`/`transition` props inline on all three `<motion.path>` elements.
- **Root cause**: The three primary-figure paths predate the preset extraction and were never migrated to use it, so the abstraction exists but is applied to only half the branches.
- **Impact**: Three near-duplicate motion prop blocks that must be kept in sync by hand; a timing tweak to the presets silently skips the 01–03 figures. Low, purely maintainability.
- **Fix sketch**: Spread `{...inView}` / `{...sumView}` on the three default-branch paths (overriding only per-path `transition.delay`), deleting the inlined prop copies.
