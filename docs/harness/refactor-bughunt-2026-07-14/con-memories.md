> Context: Console: Memory Archive + Inspector
> Total: 5 (Critical: 0, High: 2, Medium: 2, Low: 1)

## 1. Live detail-fetch failure silently renders an unrelated demo memory as the real one
- **Severity**: High
- **Lens**: bug-hunter
- **Category**: silent-failure-wrong-data
- **File**: console/app/console/(modules)/memories/useMemoryDetail.ts:29-32 (with archive-data.ts:152-153)
- **Scenario**: The page loaded live (`data.live === true`), the user clicks a real memory, and the per-memory request to `/api/memories/<uuid>` returns non-2xx — upstream blip / 502, a 404 for a deleted memory, an RLS-hidden row, or malformed JSON. The hook's `.then(r.ok ? … : reject)` / `.catch(() => finish(demoDetail(id)))` swallows every one of these and calls `demoDetail(id)`. `demoDetail` does `DEMO_ROWS.find(r => r.id === id) ?? DEMO_ROWS[0]`; a real UUID never matches a `dm-…` demo id, so it returns `DEMO_ROWS[0]` — the "psp-gateway client timeout is 10 seconds" fixture.
- **Root cause**: The hook treats "live fetch failed" and "we are offline" as the same case, and `demoDetail` was written to always return *a* memory (the `?? DEMO_ROWS[0]` guard) rather than signal "not found". There is no error state in the hook's return.
- **Impact**: In a governance/provenance/audit console, the inspector shows a completely different memory's content, status, lineage, provenance actor/model, anchored entities and promotion ledger — all fabricated — attributed to the memory the user selected. Because `data.live` stayed true, no `DemoBanner` renders, so there is zero signal that the record is fake. This is silent data fabrication in the one surface whose job is trustworthy provenance.
- **Fix sketch**: Give the hook an `error` state; on live failure set `error`, keep `detail = null`, and do NOT fall back to demo when `live` is true (only synthesize demo when `!live`). Render an explicit "couldn't open this record" state in Archive.tsx's inspector panel. Separately, make `demoDetail` return `null` for an unknown id instead of `?? DEMO_ROWS[0]`.

## 2. Archive silently truncates to 40 rows while the header counts them all; the rest are uninspectable
- **Severity**: High
- **Lens**: bug-hunter
- **Category**: silent-truncation
- **File**: console/app/console/(modules)/memories/Archive.tsx:59,92 (source cap page.tsx:19 `limit: "200"`)
- **Scenario**: The server fetch requests up to 200 rows; after `validAt`/status filtering, `visible.length` can be well over 40. The header prints the full count — `{visible.length} memories true then` (e.g. "180 memories true then") — but the list renders only `visible.slice(0, 40)`. There is no pagination, virtualization, search, or "+N more" affordance, and scrubbing the timeline only changes *which* memories pass `validAt` — the slice always takes the first 40 of the filtered array.
- **Root cause**: The design comment assumes "client-side filtering over one fetched corpus" is enough; the `.slice(0, 40)` was added to bound the animated list but nothing communicates or paginates the remainder, and the header count was taken from the pre-slice array.
- **Impact**: With a realistic archive, 140+ memories that are "true then" can never be selected or inspected — their lineage/provenance is unreachable — and the header actively misleads by claiming all of them are shown. Data is present but silently inaccessible in an audit tool.
- **Fix sketch**: Either paginate/virtualize the full `visible` list, or at minimum render "showing 40 of {visible.length}" plus a load-more control, and add a search/filter so any memory can be reached. Keep the animated window at 40 but drive it from a windowing index, not a hard first-40 slice.

## 3. Duplicated status/validity badge rendering, and shared formatters buried in a component module
- **Severity**: Medium
- **Lens**: code-refactor
- **Category**: duplication
- **File**: console/app/console/(modules)/memories/Archive.tsx:105-114 vs MemoryInspector.tsx:46-62,90-92 (helpers MemoryInspector.tsx:17-27, imported Archive.tsx:19)
- **Scenario**: The "`<status>` · `<valid_from>` → `<valid_to>`" mono badge is hand-built three times: the Archive row (Archive.tsx:105-113), the inspector header (MemoryInspector.tsx:46-62), and each lineage entry (MemoryInspector.tsx:90-92) — each re-applying `statusTone`, `fmtDate`, the "→/– now" fallback, and the same tracking/opacity classes. Separately, the shared formatters `fmtDate`/`statusTone` are defined in the `MemoryInspector` *component* file, forcing `Archive.tsx` to import UI helpers from a sibling component (`import MemoryInspector, { fmtDate, statusTone } from "./MemoryInspector"`).
- **Root cause**: Helpers were co-located with the first component that needed them instead of in `archive-data.ts`, the file whose own header comment declares it the "shared substrate"; the badge markup was copy-adapted per site.
- **Impact**: A change to status colors, the "now" wording, or the date format must be made in 3+ places and will drift; the component→component import couples Archive to Inspector internals. Modest but real maintenance tax across the unit's core rendering.
- **Fix sketch**: Move `fmtDate`/`statusTone` into `archive-data.ts` (or a small `memory-format.ts`) and extract a `<MemoryStatusBadge status validFrom validTo />` (or a `<ValidityRange>`) used by all three sites.

## 4. Dead `ArchiveData.total` field threaded end-to-end but never read
- **Severity**: Medium
- **Lens**: code-refactor
- **Category**: dead-code
- **File**: console/app/console/(modules)/memories/archive-data.ts:8-12,147-150 (populated page.tsx:20)
- **Scenario**: `ArchiveData` declares `total: number`; `page.tsx:20` fetches and assigns `total: out.total`, and `DEMO_ARCHIVE` sets `total: DEMO_ROWS.length`. Nothing in `Archive.tsx` (or anywhere in the memories module) ever reads `data.total` — the header count comes from `visible.length`, not `total`.
- **Root cause**: The field looks like it was intended to back a "N of total" indicator (see finding #2) that was never built, so the plumbing remains without a consumer.
- **Impact**: A misleading contract — `total` implies the UI knows the true corpus size — plus an unused server value carried through the page. It also masks finding #2: the data needed to say "40 of 180" is already fetched but discarded.
- **Fix sketch**: Either wire `total` into the header/pagination affordance (resolves #2), or drop the field from `ArchiveData`, `page.tsx`, and `DEMO_ARCHIVE`.

## 5. `fmtDate` throws (RangeError) on a truthy-but-invalid timestamp, taking down the whole route
- **Severity**: Low
- **Lens**: bug-hunter
- **Category**: crash-edge-case
- **File**: console/app/console/(modules)/memories/MemoryInspector.tsx:17-20
- **Scenario**: `fmtDate` guards only the null/empty case (`if (!iso) return "—"`) then calls `new Date(iso).toISOString()`. If the API ever emits a non-null timestamp that `Date` can't parse (the schema comment even flags that these fields are hand-stringified `Option<String>` columns, unlike the `DateTime` payloads), `new Date("…").toISOString()` throws "Invalid time value". `fmtDate` is the single date formatter used by Archive and Inspector, so one bad value throws during render.
- **Root cause**: The guard checks presence but not validity, and re-serializing through `toISOString()` converts an unparseable string into a thrown exception rather than a fallback.
- **Impact**: A single malformed timestamp crashes the Archive/Inspector render into the `RouteError` boundary instead of degrading one date to "—". Low likelihood given RFC3339 contract, but the blast radius is the entire route.
- **Fix sketch**: Validate before formatting: `const d = new Date(iso); return Number.isNaN(d.getTime()) ? "—" : d.toISOString().slice(0, 10);`
