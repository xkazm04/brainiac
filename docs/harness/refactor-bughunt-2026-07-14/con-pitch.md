> Context: Console: Pitch / Marketing
> Total: 5 (Critical: 0, High: 1, Medium: 3, Low: 1)

## 1. ResizeObserver spawns a second (and third, and Nth) requestAnimationFrame loop
- **Severity**: High
- **Lens**: bug-hunter
- **Category**: raf-leak
- **File**: console/src/pitch/LedgerField.tsx:272-279
- **Scenario**: The animation `draw()` unconditionally reschedules itself at its tail (`if (!reduce && !disposed) raf = requestAnimationFrame(draw)`, line 272). The `ResizeObserver` callback (lines 275-279) calls `resize()` **then `draw(performance.now())`** — and that draw hits the same tail and schedules its own RAF. ResizeObserver fires one callback immediately on `observe()`, so from mount there are already **two** concurrent RAF chains; every subsequent window/layout resize adds another. Each chain overwrites the shared `raf` handle, so the previous chain's id is leaked and uncancelable.
- **Root cause**: The resize handler reuses the self-scheduling animation function to repaint, instead of a paint-only path. The design assumed `draw` would only ever be driven by the single RAF chain.
- **Impact**: The hero's shared physics state (`claimsRef`/`chainRef`) is mutated N times per frame, so drift, docking lerps (0.09) and chain scroll (0.06) all advance at N× the intended rate — the "ledger" animates visibly too fast even with zero user interaction (initial RO fire alone doubles it), and CPU scales with resize count. Cleanup's single `cancelAnimationFrame(raf)` only stops the newest chain until the others notice `disposed`. This is the first thing every visitor sees.
- **Fix sketch**: In the RO callback, call only `resize()` (the running RAF will repaint next frame); or in reduced-motion draw once without rescheduling. Alternatively cancel the existing `raf` before scheduling, and never let `draw` reschedule when invoked from the observer.

## 2. `dim()` and theme color primitives re-declared in every pitch module instead of imported from theme.ts
- **Severity**: Medium
- **Lens**: code-refactor
- **Category**: duplication
- **File**: console/src/pitch/Pitch.tsx:80, sections/EvidenceMatrix.tsx:20-21, sections/LimitsBalanceSheet.tsx:19-20, sections/RetreatAutopsy.tsx:33-34, diagrams.tsx:27-32
- **Scenario**: `const dim = (a) => \`rgba(233,237,255,${a})\`` is copy-pasted verbatim into 4 files of this unit (and 11 files console-wide). `const GOLD = band("gamma")` is re-derived locally in diagrams, EvidenceMatrix, RetreatAutopsy and LimitsBalanceSheet even though `theme.ts` already `export`s `GOLD`. `MINT`/`ALPHA` are likewise re-`band()`-ed in Pitch.tsx (77-78), and raw magenta `rgba(255,93,162,α)` literals are scattered across Pitch, diagrams, LedgerField, RetreatAutopsy and LimitsBalanceSheet with no `magenta(α)` helper.
- **Root cause**: The ink base color `233,237,255` (identical to theme's `INK = "#e9edff"`) and the accent hues live as literals per-file; there is no exported `dim`/`magenta` alpha helper, so each new section reinvents them.
- **Impact**: A palette change (or a future light theme) requires editing the same literal in a dozen places; drift is already visible (some files import `GOLD`, some re-derive it). Pure maintenance tax across a presentation surface that is nothing but color.
- **Fix sketch**: Export `dim`/`ink(α)` and `magenta(α)` helpers plus `GOLD`/`MINT`/`ALPHA` from `theme.ts`; import them everywhere and delete the local copies.

## 3. Pitch.tsx is an 851-line god-component with inconsistent section extraction
- **Severity**: Medium
- **Lens**: code-refactor
- **Category**: god-component
- **File**: console/src/pitch/Pitch.tsx:160-849
- **Scenario**: Three sections (Retreat, Evidence, Limits) were extracted to `sections/*.tsx`, but the equally heavy `Quadrant` (recharts scatter + custom SVG shape renderer, lines 641-777), `Matrix` (table builder, 781-849) and `MechanismCard` (567-630) remain inline in the page file alongside shared furniture (`Section`, `H2`, `Lede`, `Tip`) and the hero. The file is 851 LOC.
- **Root cause**: The section-extraction refactor was applied to some sections and not others, leaving the page as both a composition root and a component library.
- **Impact**: The page mixes layout, data-viz logic (recharts config, the `CAMP_COLOR` map, the SVG shape closure) and reusable primitives in one module, so the diagram code can't be reviewed or reused in isolation and the file is hard to navigate. It is the largest, highest-churn file in the unit.
- **Fix sketch**: Move `Quadrant` → `sections/GapQuadrant.tsx`, `Matrix` → `sections/CapabilityMatrix.tsx`, and `MechanismCard` + `Section`/`H2`/`Lede`/`Tip` into a shared `sections/primitives.tsx`, matching the pattern the other three sections already follow.

## 4. Dead content and an unused type in pitch-data.ts
- **Severity**: Low
- **Lens**: code-refactor
- **Category**: dead-code
- **File**: console/src/pitch/pitch-data.ts:25-28, 428-432
- **Scenario**: `KB_TEASER.points` (three `{label, body}` status objects, lines 428-432) is never referenced — the KB teaser block in Pitch.tsx (491-506) renders only `.status`, `.headline`, `.body` and `.href`, never `.points` (grep for `.points` in the pitch dir returns nothing). The exported `interface Cite` (25-28) is likewise unused: `RETREAT`'s `cite` objects are inferred inline, and the only other `Cite` in the codebase is an unrelated local component in `docs/DocReader.tsx`.
- **Root cause**: `points` looks like content authored for a teaser sub-list that was dropped when the block was simplified; `Cite` predates the inline-typed `RETREAT`.
- **Impact**: ~9 lines of misleading dead data/type — a maintainer editing the KB teaser will reasonably assume `points` renders somewhere. Possibly a latent content-omission (the three shipped/built status lines were meant to show and silently don't).
- **Fix sketch**: Either wire `KB_TEASER.points` into the teaser block or delete it; delete the unused `Cite` interface (or annotate `RETREAT` items with it so it earns its keep).

## 5. Evidence matrix stamps refusals and unverified guesses with the "right answer" glyph
- **Severity**: Medium
- **Lens**: bug-hunter
- **Category**: wrong-render
- **File**: console/src/pitch/sections/EvidenceMatrix.tsx:106-109, 141-143
- **Scenario**: The cold-agent column computes `kind={r.cold === "—" ? "none" : lost ? "none" : "partial"}`, so on both win rows the cold cell always renders `~`. The legend (141-143) defines `~` as "right answer, no better than the baseline." But row 1's cold text is *"Refused — asked to be pointed at the payments repo"* and row 2's is *"Guessed 'too low' — and explicitly marked it unverified."* Neither is a right answer; row 1 is a refusal identical in spirit to the baseline's refusal, which the same table marks `✗` ("wrong, or could not answer").
- **Root cause**: The cold arm was hard-wired to a single "partial" mark rather than derived from its actual per-row outcome, and the legend copy was written for a different meaning of `~`.
- **Impact**: On a page whose entire thesis is *not* misrepresenting results, the table asserts the cold agent gave a "right answer" where it refused or guessed — and marks a refusal as ✗ for the baseline but ~ for the cold agent. It quietly weakens the honesty argument the section is built to make.
- **Fix sketch**: Give each `TRIAL.row` an explicit per-arm outcome kind (like `verdict`) instead of inferring the cold column from `lost`, or relabel the `~` legend to match what the cold cells actually show (e.g. "declined / unverified — no better than baseline").
