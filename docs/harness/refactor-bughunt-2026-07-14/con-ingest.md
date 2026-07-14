> Context: Console: Ingest Monitor
> Total: 5 (Critical: 0, High: 2, Medium: 2, Low: 1)

## 1. SubmitBox guards double-submit with stale React state, not a synchronous lock
- **Severity**: High
- **Lens**: bug-hunter
- **Category**: double-submit
- **File**: console/app/console/(modules)/ingest/SubmitBox.tsx:25-43 (guard line 26, Enter handler line 50)
- **Scenario**: Operator holds Enter (keydown auto-repeat) or hits Enter then clicks "⚡ capture" before the first POST resolves. Each event runs `submit()` closed over the render where `state === "idle"`, so both pass the `if (... || state === "sending") return` gate and both `fetch("/api/ingest", { method: "POST", ... })`.
- **Root cause**: The only re-entrancy guard is the `state` value captured in the render's closure plus the button's `disabled` attribute. Both only flip after `setState("sending")` commits a new render; two invocations dispatched against the same committed render both observe `"idle"`. There is no synchronous ref/lock.
- **Impact**: Duplicate source captures are written into the real ingest pipeline — the same transcript/memory ingested 2+ times, polluting the knowledge base and wasting worker/token budget. This is a write-path data-integrity bug, not just extra load.
- **Fix sketch**: Add a `const sendingRef = useRef(false)`; at the top of `submit` do `if (sendingRef.current || !content.trim()) return; sendingRef.current = true;` and clear it in `finally`. Keep the `state` machine for UI only. (Server-side dedupe/idempotency key is the belt-and-suspenders complement.)

## 2. useIngestFeed fires a fixed 6s poll with no in-flight guard and no error backoff
- **Severity**: High
- **Lens**: bug-hunter
- **Category**: request-storm
- **File**: console/app/console/(modules)/ingest/useIngestFeed.ts:30-37 (interval) + 14-28 (refresh)
- **Scenario**: `/api/ingest/feed` gets slow or starts erroring (backend degraded). `setInterval(tick, 6000)` keeps calling `refresh()` every 6s regardless of whether the previous fetch resolved. If a fetch takes >6s, requests overlap and pile up; if the endpoint returns errors, the `catch {}` swallows them and the next tick hammers again in 6s — indefinitely, with zero backoff.
- **Root cause**: The poll is a naive fixed-interval timer. `refreshing` state exists but is never checked to skip an overlapping call, and the empty `catch` treats a persistent failure identically to a transient one. There is also no request timeout.
- **Impact**: A degraded feed endpoint gets a steady, un-throttled request stream from every open Ingest Monitor tab, and slow responses accumulate concurrent in-flight fetches — amplifying load exactly when the backend is already unhealthy. Failures are invisible (no error surfaced to the operator).
- **Fix sketch**: Gate on an in-flight ref (skip tick if a fetch is pending); on error, apply exponential backoff (e.g. 6s→12s→24s, cap ~60s) and reset on success; add an AbortController with a timeout. Consider a self-scheduling `setTimeout` loop instead of `setInterval` so the next poll only arms after the prior one settles.

## 3. In-flight feed fetch is never aborted: stale response can clobber fresher data and setState fires after unmount
- **Severity**: Medium
- **Lens**: bug-hunter
- **Category**: response-race
- **File**: console/app/console/(modules)/ingest/useIngestFeed.ts:14-28 (fetch 18, setData 21)
- **Scenario**: Two overlapping polls (see #1/#2) — poll A dispatched at t=0, poll B at t=6s. A is slow and resolves *after* B; `setData` from A overwrites B's newer snapshot, so the UI flips back to stale rows. Separately, switching lens (Manifest↔Airlock triggers `AnimatePresence mode="wait"` to unmount the old view) or navigating away clears the interval but leaves an already-dispatched `refresh()` running; it still calls `setData`/`setRefreshing` on the unmounted component.
- **Root cause**: `fetch` is issued without an `AbortController`, and responses aren't sequence-checked, so ordering isn't guaranteed and there's no cancellation on teardown. (In React 19 the post-unmount `setState` is a benign no-op, but the un-cancelled request and the out-of-order clobber are real.)
- **Impact**: Occasional UI regressions to stale data on flaky networks, plus wasted requests that continue after the view is gone.
- **Fix sketch**: Create an `AbortController` per `refresh`, pass `signal` to `fetch`, and abort it in the effect cleanup and before each new poll. Optionally stamp each request with a monotonically increasing id and drop a response whose id is not the latest.

## 4. ManifestView and AirlockView duplicate the header/SubmitBox/feed wiring and the queue-health readout
- **Severity**: Medium
- **Lens**: code-refactor
- **Category**: duplication
- **File**: console/app/console/(modules)/ingest/views/ManifestView.tsx:25-64,158-167 vs views/AirlockView.tsx:36-57,119-141
- **Scenario**: Both views open with the identical block: `const { data, refresh } = useIngestFeed(initial)`, the `θ · ingest monitor · …` label, a `FONT_DISPLAY` `<h1>`, and `<SubmitBox live={data.live} onSubmitted={refresh} />`. Both also render the same `data.health` fields (`ready`, `in_flight`, `archived.ok`, `dead_letters`, and the `!data.live && "demo data"` marker) with slightly divergent labels ("waiting at dock" vs "queue ready").
- **Root cause**: The two lenses were forked from the same prototype (per the IngestMonitor header comment) and each hand-rolled the shared chrome instead of sharing a component. The feed-wiring boilerplate and the health panel were copy-pasted.
- **Impact**: Any change to the header, SubmitBox wiring, feed-hook contract, or health readout must be made in two places and can drift (already visible in the divergent health labels/threshold colors). Doubles the surface for bugs like #1–#3 to be fixed inconsistently.
- **Fix sketch**: Extract an `<IngestHeader lens="manifest"|"airlock" data live onSubmitted />` wrapper that owns `useIngestFeed` + the label/h1/SubmitBox, and a `<QueueHealthStats health live />` component; have both views consume them.

## 5. useIngestFeed computes and returns `refreshing`, but no caller reads it
- **Severity**: Low
- **Lens**: code-refactor
- **Category**: dead-code
- **File**: console/app/console/(modules)/ingest/useIngestFeed.ts:12,26,39
- **Scenario**: The hook maintains `const [refreshing, setRefreshing] = useState(false)`, toggles it around every fetch, and returns it. Both consumers destructure only `{ data, refresh }` (ManifestView.tsx:26, AirlockView.tsx:37); SubmitBox doesn't use the hook. `refreshing` is never rendered anywhere.
- **Root cause**: Left-over from an earlier design that presumably showed a live "refreshing…" indicator; the indicator was dropped but the state plumbing stayed.
- **Impact**: Dead state that adds an extra re-render per poll (two `setState`s each 6s) and misleads readers into thinking a spinner exists. Ironically it is exactly the flag that should have been reused as the in-flight guard in #2.
- **Fix sketch**: Either delete `refreshing`/`setRefreshing` and its return, or repurpose it: surface it in the views as a subtle poll indicator and reuse a ref-backed version to gate overlapping polls.
