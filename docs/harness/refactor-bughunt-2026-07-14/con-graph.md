> Context: Console: Knowledge Graph Explorer
> Total: 5 (Critical: 0, High: 1, Medium: 3, Low: 1)

## 1. Live drill-down failures silently fabricate evidence as if real
- **Severity**: High
- **Lens**: bug-hunter
- **Category**: silent-failure
- **File**: console/app/console/(modules)/graph/useCanonicalDetail.ts:29-32 (with console/app/console/(modules)/graph/cortex-data.ts:143)
- **Scenario**: In live mode a user clicks a canonical hub. `fetch(/api/graph/canonical/${id})` returns 404/500/502 (RLS denial, deleted node, upstream down) or the network drops. The `.catch(() => finish(demoDetail(id, data.overview)))` swallows the error and renders synthesized `surface_forms`, `edges`, and `memories` in the focus card — no error banner, no "demo" marker (the "· demo data" hint is keyed off `data.live`, which is still `true`).
- **Root cause**: The offline-demo synthesizer was reused as a catch-all fallback for the live path. `demoDetail` also does `overview.canonicals.find(x => x.id === id) ?? overview.canonicals[0]` — when the id is unresolvable it fabricates detail from an *unrelated* entity under the requested name. (Note: the fetch race/unmount itself IS correctly handled by the per-effect `cancelled` flag — that is not the bug here.)
- **Impact**: In a governance/knowledge console whose whole premise is "canonical hubs with evidence pointers," users are shown invented surface forms, invented `depends_on` edges, and invented anchored memories that look identical to real audited data. A transient upstream error becomes fabricated provenance no operator can distinguish from truth.
- **Fix sketch**: Add an `error` state to the hook; on live-fetch rejection set it and render an explicit "couldn't load — retry" panel instead of `demoDetail`. Reserve `demoDetail` strictly for the `!data.live` branch. Also reset `loading` to `false` in the early `if (!id) return` path so a fast deselect never leaves stale loading state.

## 2. `Math.max(1, ...stars.map(...))` spread crashes on a large graph
- **Severity**: Medium
- **Lens**: bug-hunter
- **Category**: degenerate-scale
- **File**: console/app/console/(modules)/graph/views/StarChartView.tsx:52
- **Scenario**: `const maxTeams = Math.max(1, ...stars.map((s) => s.teams));` spreads one function argument per canonical. A live org graph with a very large canonical count (well beyond the 50-node mock the scale toggle ships) blows the JS argument-count limit → `RangeError: Maximum call stack size exceeded`, taking down the whole Star Chart render.
- **Root cause**: Convenience `Math.max(...arr)` written against demo-sized data (12 nodes) and a 50-node stress mock; `canonicals` is an unbounded server array with no cap.
- **Impact**: The relationship lens throws (blank view / error boundary) precisely for the biggest, most valuable orgs — the ones this view exists to make legible. `labelCutoff` already sorts a full copy of `stars` each render, so the same input scale also costs an O(n log n) copy per keystroke.
- **Fix sketch**: Reduce instead of spread: `stars.reduce((m, s) => Math.max(m, s.teams), 1)`. If huge graphs are in scope, also cap/aggregate the rendered node set (top-N by memories) rather than emitting one `<g>` per canonical.

## 3. `teamIndex` returns -1 for an unlisted team, poisoning `teamColor`
- **Severity**: Medium
- **Lens**: bug-hunter
- **Category**: edge-case-color
- **File**: console/app/console/(modules)/graph/views/DepthOfFieldView.tsx:105 (also console/app/console/(modules)/graph/DetailSections.tsx:35-36 via `teamIndex`)
- **Scenario**: A canonical's `team_ids` includes a team id that is absent from `overview.teams` (RLS filters a team out of the teams list while the canonical still references it — `team_ids: string[]` and `teams: number` are independent per the API schema). `teamIndex(tid)` → `findIndex` → `-1`. `teamColor(-1)` computes `TEAM_HUES[-1 % 3]` = `TEAM_HUES[-1]` = `undefined`, yielding `hsla(undefined, 85%, 68%, 0.7)`.
- **Root cause**: `teamColor` indexes `TEAM_HUES[i % length]` assuming `i >= 0`; JS `-1 % 3 === -1`, so a negative index slips through. The card grid guards the *name* (`overview.teams[i]?.name`) but not the *color*, revealing the author knew `i` could be invalid.
- **Impact**: Team stripe pills, and SurfaceForm border/text colors, silently render as an invalid (transparent/inherited) color — the team-attribution signal the view relies on vanishes for exactly the cross-team nodes that matter most, with no error.
- **Fix sketch**: In `teamColor`, guard `i < 0` (return a neutral ink) or `Math.abs`; or have `teamIndex` fall back to a stable neutral bucket. Skip rendering the stripe when `i < 0`.

## 4. Two views duplicate scaffolding and diverge on the "bound" rule
- **Severity**: Medium
- **Lens**: code-refactor
- **Category**: duplication
- **File**: console/app/console/(modules)/graph/views/StarChartView.tsx:31,52,123 & console/app/console/(modules)/graph/views/DepthOfFieldView.tsx:27,85
- **Scenario**: StarChartView and DepthOfFieldView each independently re-declare `const GOLD = band("gamma")` (already exported as `GOLD` from `@/design/theme`, and re-declared a third time in DetailSections.tsx:15), the identical `teamIndex = (id) => overview.teams.findIndex(...)` helper, and the same `useCanonicalDetail`+`selected`+`loading`+focus-card-of-DetailSections scaffold. Worse, the shared business rule "is this a fully-bound hub?" diverges: StarChart uses `s.teams >= Math.max(3, maxTeams)` (StarChartView:123) while Depth uses `c.teams === 3` (DepthOfFieldView:85).
- **Root cause**: The two lenses were built as sibling prototypes (per the file headers) and never had their common selection/detail/color logic hoisted; the gold-highlight threshold was hand-coded in each.
- **Impact**: A 4+-team canonical (possible with live data) is highlighted as bound in Star Chart but NOT in Depth of Field — the same entity reads as "canonical" in one lens and ordinary in the other. Every future change to the focus panel or team-index logic must be made twice, and the redundant `GOLD` re-derivation defeats the design-token export.
- **Fix sketch**: Import `GOLD` from `@/design/theme` in all three files. Extract `teamIndex`, an `isBound(c)` predicate, and a `<CanonicalFocusCard detail loading onHop teamIndex/>` wrapper into a shared module both views consume.

## 5. "esc · defocus" button implies an Escape key that isn't wired
- **Severity**: Low
- **Lens**: bug-hunter
- **Category**: false-affordance
- **File**: console/app/console/(modules)/graph/views/DepthOfFieldView.tsx:135-140
- **Scenario**: The focus-plane close button is labeled `esc · defocus`, but no view registers a `keydown`/Escape listener (StarChart's close button is `✕`, also key-less). Pressing Escape does nothing; only a mouse click on the button dismisses the focused canonical.
- **Root cause**: The label communicates an intended keyboard shortcut that was never implemented (no `useEffect` window listener with cleanup for `key === "Escape"`).
- **Impact**: Keyboard users (and anyone trusting the label) hit Escape and stay trapped in the focus overlay; the affordance is a documented lie. Minor, but it is a correctness gap in the stated interaction contract.
- **Fix sketch**: Add a `useEffect` that binds `keydown` → `if (e.key === "Escape") setSelected(null)`, with a matching removeEventListener cleanup; share it with StarChartView so both honor Escape.
