> Context: Console: Health + Analytics + Observatory + Ops
> Total: 5 (Critical: 0, High: 1, Medium: 3, Low: 1)

## Note on the two named "crown jewels"

Both hypothesized crown jewels were investigated and found ABSENT in this unit, so neither is reported:

- **Unauthorized sweep trigger** — not exploitable. `console/middleware.ts` gates every non-public path (including Server Action POSTs, which land on `/console/health` and `/console/divergence`) behind a valid session cookie. The app has a single shared passcode (`app/login/actions.ts`) — there is no viewer/maintainer split, so "non-maintainer" does not exist as a role. The bearer token stays server-side (`configFromEnv`), and the sweep actions are imported ONLY by the two protected pages (grep: `SweepControl`/`sweep-actions` appear in no `/demo`, `/pitch`, or `/kb` bundle), so their action IDs are unreachable from any public surface. No bypass.
- **Division-by-zero / NaN in health scores** — not present. `score`, `grade`, and all four `pillars` are computed server-side and only rendered/clamped here (`Math.max(0, Math.min(100, value))`). Every client divisor is guarded (`Trend` early-returns when `points.length < 2`; `Observatory` uses `Math.max(1, ...)` for the heat denominator). `age.ts` guards `secs <= 0`. Its `age.test.ts` is correct.

The 5 below are the highest-value defects that genuinely exist.

## 1. Divergence report crashes on a malformed `detected_at`, with no error boundary to catch it
- **Severity**: High
- **Lens**: bug-hunter
- **Category**: crash-render
- **File**: console/src/divergence/PracticeDivergence.tsx:123 (route: console/app/console/(modules)/divergence/ has no error.tsx)
- **Scenario**: `<span>{new Date(d.detected_at).toISOString().slice(0, 10)}</span>` runs for every card. If the server emits a `detected_at` that `Date` cannot parse (empty string, null, a non-ISO value), `new Date(...).toISOString()` throws `RangeError: Invalid time value` during render.
- **Root cause**: `readApproaches` (lines 38–46) already treats sweep output as untrusted — it guards `approaches` with `Array.isArray`, `typeof === "object"`, and `String(... ?? "")` coercion — but `detected_at` is fed straight into `new Date().toISOString()` with no such guard, even though it comes from the same scan-divergence payload. `PracticeDivergence.detected_at` is a `date-time` string in the schema; a single out-of-contract row is enough.
- **Impact**: One bad row throws, and because `/divergence` is one of only two module routes lacking an `error.tsx` (7 of 9 siblings — analytics, disputes, graph, ingest, keys, memories, reviews — have one; health and divergence do not), the throw escapes to Next's root error and white-screens the entire standardization board with no retry affordance. `withDemoFallback` does not help: it only catches the *fetch*, not the render.
- **Fix sketch**: Parse defensively, e.g. `const d0 = new Date(d.detected_at); const day = Number.isNaN(d0.getTime()) ? "—" : d0.toISOString().slice(0, 10);`. Additionally add `console/app/console/(modules)/divergence/error.tsx` (and health/error.tsx) re-exporting `@/components/RouteError`, matching the seven siblings, so any future render throw degrades instead of crashing the app.

## 2. "run now" can re-queue a sweep that is already running — double-submit protection is only local
- **Severity**: Medium
- **Lens**: bug-hunter
- **Category**: double-submit
- **File**: console/src/ops/SweepControl.tsx:83-90,120-128 · console/src/ops/sweep-actions.ts:24-27
- **Scenario**: `run now` is `disabled={pending}` where `pending` comes from `useTransition`. The action `runSweepAction` only *queues* a multi-minute LLM sweep and returns immediately; `pending` then flips back to false while `schedule.last_status` is still `"running"`. The button is now clickable again, so a user (or an impatient double-click after the transition resolves) can queue a second, third… run of the same sweep. `runSweepAction` does no idempotency check — it calls `runSweep` unconditionally.
- **Root cause**: The transition guards only the in-flight request, not the sweep's server-side lifecycle. The component already knows the sweep is mid-flight (`schedule.last_status === "running"`) but does not use that to disable the trigger.
- **Impact**: Redundant multi-minute LLM scans pile up on the worker — wasted compute and cost, and confusing status flicker — for exactly the operation the panel exists to make safe.
- **Fix sketch**: Disable `run now` when `pending || schedule.last_status === "running"`, and give the button running-state microcopy ("running…"). Ideally the `/run` server action should also be a no-op (or return the existing job) when a run is already in flight.

## 3. Observatory "total memories" headline sums rejected and deprecated buckets, overstating the corpus
- **Severity**: Medium
- **Lens**: bug-hunter
- **Category**: wrong-metric
- **File**: console/src/observatory/Observatory.tsx:76,85
- **Scenario**: `const totalMemories = Object.values(data.totals).reduce((a, b) => a + b, 0);` sums *every* status bucket, then renders it as the "total memories" gauge — the largest, whitest number on the ops wall. `data.totals` (built by `normalizeObservatory` → `Object.fromEntries(p.totals.map(...))`) includes `deprecated` and `rejected`. In the demo shape that is canonical 81 + candidate 7 + raw 12 + deprecated 6 + rejected 3 = 109, shown beside "canonical 81".
- **Root cause**: The reduce blindly totals all statuses instead of the ones that represent knowledge the org actually holds. `rejected` are candidates that were explicitly adjudicated NOT to be memories; `deprecated` are retired — neither belongs in a "total memories" headline a leader is judged on.
- **Impact**: The top-line corpus size is inflated by exactly the memories governance threw away, so the dashboard's most prominent number over-reports the knowledge base and moves in the wrong direction (rejecting bad memories makes "total" go *up*).
- **Fix sketch**: Sum only the live statuses (e.g. exclude `rejected` and `deprecated`, or whitelist `canonical`/`candidate`/`raw`), and label precisely — or rename the gauge to "all rows" if the inclusive count is genuinely intended.

## 4. Four divergent elapsed-time humanizers for the same values; the tested one is shadowed
- **Severity**: Medium
- **Lens**: code-refactor
- **Category**: duplication
- **File**: console/src/health/age.ts:10-17 · console/src/observatory/Observatory.tsx:41-46 · console/src/ops/SweepControl.tsx:50-59,62-70
- **Scenario**: The cluster carries four hand-rolled seconds/ms → human-string functions: `age.ts::age` (exported, unit-tested, renders `"2d 16h"` / `"3h 12m"`), `Observatory.tsx::age` (a private copy that renders `"2.6d"` / `"3.2h"`), and `SweepControl`'s `ago` and `until`. Observatory reimplements `age` with a *different output format* rather than importing the tested one two directories over. The concrete tell: the demo `oldestSecs = 11520` renders as `"3h 12m"` on `/console/health` but `"3.2h"` on `/console/analytics` — the same quantity, two formats, from two functions.
- **Root cause**: Each surface grew its own formatter; no shared duration utility, so the tested implementation gets duplicated-and-diverged instead of reused.
- **Impact**: User-visible inconsistency for identical metrics across sibling pages, and three of the four copies are untested — the load-bearing "propagation age" logic is only covered in `age.ts`, and the divergent copies can drift further without any test noticing.
- **Fix sketch**: Promote one humanizer (extend `age.ts` with the coarse `"3.2h"` variant as an option, or add `agoFrom(iso)`/`untilFrom(iso)` beside it), delete Observatory's local `age`, and route `SweepControl` through the shared module.

## 5. `ago()` / `until()` render "NaNm ago" / "in NaNm" on an unparseable timestamp
- **Severity**: Low
- **Lens**: bug-hunter
- **Category**: nan-guard
- **File**: console/src/ops/SweepControl.tsx:50-59,62-70
- **Scenario**: `ago(iso)` computes `secs = Math.max(0, (Date.now() - new Date(iso).getTime()) / 1000)`. If `iso` is a present-but-unparseable string, `getTime()` is `NaN`; `Math.max(0, NaN)` is `NaN`, every `secs < …` comparison is false, and it falls through to `${Math.floor(secs / 60)}m ago` → `"NaNm ago"`. `until(iso)` has the same flaw (`"in NaNm"`). `last_run_at` / `next_run_at` are `date-time` fields, so this only bites on out-of-contract data — but it degrades to gibberish rather than a dash.
- **Root cause**: `ago`/`until` guard only the *null* case (`if (!iso) return "never"`), never an *invalid-but-non-null* date, unlike `age.ts` which explicitly guards `secs <= 0`.
- **Impact**: The sweep status strip can display "NaNm ago" / "in NaNm" — a minor but sloppy failure on the ops panel; harmless beyond the confusing label.
- **Fix sketch**: After parsing, guard finiteness: `const t = new Date(iso).getTime(); if (!Number.isFinite(t)) return "—";` before computing `secs`.
