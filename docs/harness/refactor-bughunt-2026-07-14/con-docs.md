> Context: Console: Docs Reader + Editor
> Total: 5 (Critical: 0, High: 2, Medium: 3, Low: 0)

Note on the crown jewels: both are largely defended in this slice, and I will not
fabricate a Critical. The markdown path has no `dangerouslySetInnerHTML`, no
raw-HTML node kind, and a scheme-allowlist on links (markdown.ts + markdown.test.ts
assert it) — script-injection XSS is not reachable. Anonymous mutation is closed
by `console/middleware.ts`, which gates every `/console/*` POST (server actions
included) behind the session. The residual, real gaps are the two Highs below.

## 1. Docs mutations authorize via a single shared token, defeating the backend's per-team maintainer gate
- **Severity**: High
- **Lens**: bug-hunter
- **Category**: authz-boundary
- **File**: console/app/console/(modules)/docs/[slug]/actions.ts:32,57 (with configFromEnv, src/lib/api.ts:57-62)
- **Scenario**: A console operator who is NOT a maintainer of the owning team clicks "approve & publish" or "propose/save this prose". `approveRevisionAction` / `editSectionAction` call `approveDocRevision(configFromEnv(), …)` / `editDocSection(configFromEnv(), …)`. `configFromEnv()` returns ONE static `process.env.BRAINIAC_API_TOKEN` for every request. `describe()` even has a branch for the API's `403 "You need to be a maintainer of the owning team."` — proving the backend intends a per-principal maintainer check.
- **Root cause**: The console is a shared-passcode/shared-token BFF. `src/lib/auth.ts` explicitly warns "this gate … is NOT per-user identity … the API's own per-principal tokens already carry RLS … do not build per-user features on it" — yet approve/edit ARE per-maintainer features, and they run on the ambient shared token, so all authenticated operators collapse to one backend identity.
- **Impact**: Every passcode holder gets the union of that token's rights across all teams: any operator can publish revisions and pin/propose edits to any team's governed pages, regardless of team maintainership. The backend's per-team 403 can never fire for a non-maintainer because there is no non-maintainer principal on the wire. Anonymous callers are still blocked by middleware, which is why this is High rather than a clean Critical.
- **Fix sketch**: Mint/attach a per-principal token for the acting operator (the console already exposes `createToken(cfg, name, userId, scopes)` / `previewToken`) and pass THAT config into the two actions instead of `configFromEnv()`, so the backend's RLS/maintainer check evaluates the real user. Until per-user identity lands, at minimum scope the shared token narrowly and document that approve/edit are org-wide-privileged.

## 2. Pinned-section editor opens on a blank textarea but submits a full-section replacement — silent content loss
- **Severity**: High
- **Lens**: bug-hunter
- **Category**: data-loss
- **File**: console/src/docs/SectionEditor.tsx:45,116-128,145-149
- **Scenario**: A maintainer opens "edit this prose" on a **pinned** section to fix one sentence. `content` is initialised to `""` (line 45) and the API's `DocSectionView` carries only `{id, heading, mode}` — no current prose — so there is nothing to prefill; the textarea is empty with placeholder "Write the section as it should read. It goes onto the page as typed." On submit, `edit(section.id, content.trim(), note.trim())` sends `EditSectionBody.content` = "the section as the human now wants it to read", i.e. a full-section replace.
- **Root cause**: The edit contract is whole-section replacement, but the UI gives the maintainer no baseline to edit from and no signal that submitting replaces everything, not just what changed.
- **Impact**: On the human-owned ("save this prose", verbatim, never regenerated) path, a maintainer who types a short correction silently truncates the pinned section to only what they retyped — the rest of the prose is overwritten and lost, with the UI reporting "Saved to the page". This is exactly the "the product lost your work" failure the editor's own doc-comment says it exists to prevent.
- **Fix sketch**: Have the detail payload return each section's current source text (extend `DocSectionView`), prefill the textarea with it for pinned sections, and/or relabel the affordance as "rewrite this section (replaces current prose)". Disable submit until content differs from the loaded baseline.

## 3. Section editor is matched to a heading by plain-text equality — drift hides it, duplicate headings target the wrong section_id
- **Severity**: Medium
- **Lens**: bug-hunter
- **Category**: edit-routing
- **File**: console/src/docs/DocReader.tsx:391-395
- **Scenario**: `editable` is a `Map` keyed by `section.heading.trim().toLowerCase()`, and `sectionAt(b)` looks a rendered `##` heading up by `plainText(b.kids).toLowerCase()`. (a) If the composed markdown's heading text differs at all from the API's `section.heading` (e.g. markdown renders `## Reliability & retries` while the section is named `Reliability`, or the heading renders at level 3), `get` misses and the section's editor silently never appears. (b) If two sections share a heading, the Map collapses them — both rendered headings resolve to the SAME (last-wins) section, so editing one submits the other's `section.id`.
- **Root cause**: Sections are joined to rendered blocks by human-readable text instead of by a structural handle (index/anchor/id emitted into the markdown).
- **Impact**: (a) A maintainer loses the ability to edit a section with no error — a silent capability gap. (b) In the duplicate-heading case a save is routed to the wrong `section_id`, so the wrong section is overwritten — a wrong-target mutation on top of finding #2's full-replace semantics.
- **Fix sketch**: Emit a stable section id/anchor into the composed markdown (e.g. the composer tags each heading with its `section_id`) and match on that, or match by heading occurrence order rather than text; when a heading has no unique section match, render no editor deterministically rather than a wrong one.

## 4. Inline emphasis ignores flanking rules — intraword and cross-identifier underscores/asterisks render as italics
- **Severity**: Medium
- **Lens**: bug-hunter
- **Category**: markdown-render
- **File**: console/src/docs/markdown.ts:108-115
- **Scenario**: `parseInline` treats any `*`/`_` (whose next char differs) as an emphasis opener and pairs it with the next same char via `indexOf`, with no CommonMark left/right-flanking check. In a technical governance doc, a line like `set max_attempts and initial_backoff to defaults` italicises "attempts and initial" (the two identifier underscores pair across words). Any line with an even count of `_` (or `*`) across identifiers mis-renders.
- **Root cause**: A deliberately minimal inline parser omits the flanking/intraword rule that CommonMark uses precisely to keep `snake_case` and `a * b` literal.
- **Impact**: Wrong rendering of exactly the content this product is full of — config keys, snake_case identifiers, table cells — degrading the credibility of a "compiled from canonical memory" page. It is a rendering bug, not a data bug, hence Medium.
- **Fix sketch**: Require emphasis delimiters to be whitespace/punctuation-flanked (do not open when the previous char is alphanumeric for `_`; do not open when followed by whitespace), or only treat `_` as emphasis at word boundaries. Add cases to markdown.test.ts for `max_attempts` and `a * b`.

## 5. DocReader re-parses the whole document and rebuilds its citation maps on every render (every citation hover/open)
- **Severity**: Medium
- **Lens**: code-refactor
- **Category**: perf/memoization
- **File**: console/src/docs/DocReader.tsx:384-397
- **Scenario**: DocReader holds `useState open`. On every render — which includes every `setOpen` fired by `onMouseEnter`/`onClick`/`onMouseLeave` on any of the inline citation markers — the component recomputes `new Map(citations.map(...))`, `parseDoc(contentMd)` (a full O(content) markdown parse), the `editable` map, and the `used`/`unshipped`/`unresolved` derivations in the render body. None are memoized.
- **Root cause**: Derived, input-only data is computed inline in the render function of a client component whose state changes on pure hover interactions.
- **Impact**: Hovering citations on a large governed page re-parses the entire markdown on each pointer event, producing avoidable jank — directly amplified by the "huge content" edge case. It also muddies the 502-LOC component's structure.
- **Fix sketch**: Wrap `byId`, `parseDoc(contentMd)`, `editable`, and the `used`/`unshipped`/`unresolved` derivations in `useMemo` keyed on `contentMd`/`citations`/`sections`/`draft`/`edit`, so hover-driven re-renders only touch the popover state.
