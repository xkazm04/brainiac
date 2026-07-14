> Context: Publish: Render + Confluence + Git
> Total: 5 (Critical: 0, High: 2, Medium: 3, Low: 0)

## 1. Confluence HTTP client has no timeout — a hung wiki freezes publishing and holds a DB transaction open
- **Severity**: High
- **Lens**: bug-hunter
- **Category**: no-timeout-hang
- **File**: crates/brainiac-publish/src/confluence.rs:43 (and every `.send().await` at 89, 109, 130)
- **Scenario**: Confluence (or an intercepting proxy / load balancer) accepts the TCP connection but never responds — a stalled `pages/{id}` GET, PUT, or POST. `reqwest::Client::new()` is built with the default configuration, which sets **no** request, connect, or read timeout, so the `.send().await` never returns.
- **Root cause**: `from_config` uses `reqwest::Client::new()` and never calls `.timeout(...)`/`.connect_timeout(...)`. The design assumed Confluence always answers.
- **Impact**: `publisher.publish()` hangs forever. Crucially, in `lib.rs::publish_org` the call sits inside a live `store.scoped_tx` (opened at lib.rs:149, committed only at lib.rs:208), so one unresponsive sink pins a scoped DB transaction open indefinitely — starving the connection pool and blocking other org work — while `publish_org` never returns to the caller. No error, no `stats.failed`; just a wedge.
- **Fix sketch**: Build the client once with `reqwest::Client::builder().timeout(Duration::from_secs(30)).connect_timeout(Duration::from_secs(10)).build()`. Consider also not holding `scoped_tx` across the network call (render + fetch state, drop tx, publish, reopen tx to record).

## 2. Create-page path discards a successfully-created page on any post-2xx failure, duplicating pages on the next run
- **Severity**: High
- **Lens**: bug-hunter
- **Category**: partial-publish-inconsistency
- **File**: crates/brainiac-publish/src/confluence.rs:120-142 (POST create), esp. the `.json()` at 138-141
- **Scenario**: First-ever publish of a doc (`external_ref == None`). The POST to `pages` succeeds server-side (HTTP 200/201, page really created), but the client then fails *after* the write: `res.json::<PageResponse>()` errors (schema drift, truncated body, a `.send()` connection reset while reading the body, or the timeout from Finding 1). `publish()` returns `Err`.
- **Root cause**: The publication handle (`created.id`) is only obtained and returned *after* body parsing; there is no idempotency key and no "find existing page by title/space" fallback. In `lib.rs`, `record_publication` runs only on `Ok` (lib.rs:189-198), and a POST is issued only when `external_ref` is `None` (lib.rs:156-186). So a page that was created but whose id was lost is never recorded.
- **Impact**: The exact failure the module comment forbids (confluence.rs:79-81): on the *next* `publish_org`, `external_ref` is still `None`, so it POSTs again, yielding **two pages with the same title** ("psp-gateway") — a team with no way to tell which is authoritative, plus every subsequent run creating yet another duplicate. Success theater in reverse: a real publish reported as a failure, then compounded.
- **Fix sketch**: Send an idempotency/`X-Atlassian-Request-Id` key, or before POST do a title+space lookup and adopt an existing page id, or extract the id from the `Location`/status line defensively. At minimum, on create-success-but-parse-failure, retry the GET-by-title so the id can still be recorded.

## 3. Banner backlink (and any `[text](url)` link) renders as literal text in Confluence storage format
- **Severity**: Medium
- **Lens**: bug-hunter
- **Category**: rendering-omission
- **File**: crates/brainiac-publish/src/render.rs:158-210 (inline pass); triggered via banner_md at render.rs:34-43 and confluence.rs:76
- **Scenario**: Every published page is `banner_md(...) + content` (lib.rs:169-173). The banner contains a standard markdown link `[open in Brainiac](<page_url>)`. When the Confluence target renders it, `inline_to_storage` only re-introduces `**bold**` and `[m:<uuid>]` citations — it has no case for `[text](url)`. So the link is escaped and emitted verbatim.
- **Root cause**: The inline pass is deliberately minimal ("re-introduce ONLY the constructs we generate ourselves") but the crate itself generates a markdown link in the banner, which that pass does not recognize.
- **Impact**: On *every* Confluence page, the "open in Brainiac" backlink appears as raw `[open in Brainiac](https://…)` text rather than a clickable link — directly undercutting the "one click back to the console / do not edit here" promise the banner exists to deliver. Any composer-emitted inline link is similarly dead on the Confluence target (it still works on the Git/markdown target).
- **Fix sketch**: Add a bounded `[text](url)` case to `inline_to_storage` that emits `<a href="{esc_attr(url)}">{text}</a>` (escaping the href for attribute context, i.e. also `"`), or have the banner emit a citation-style/pre-rendered anchor the pass already understands.

## 4. Git publisher does blocking filesystem I/O inside an async fn and writes non-atomically
- **Severity**: Medium
- **Lens**: bug-hunter
- **Category**: async-blocking / non-atomic-write
- **File**: crates/brainiac-publish/src/git.rs:62-66
- **Scenario**: `publish()` is `async`, but calls synchronous `std::fs::create_dir_all` and `std::fs::write` with no `spawn_blocking`. On a slow/networked volume these block the Tokio worker thread, stalling unrelated tasks. Separately, `std::fs::write` truncates-then-writes in place: a crash, disk-full, or a concurrent `publish_org` for the same doc mid-write leaves a truncated `.md`.
- **Root cause**: Convenience — `std::fs` is simpler than `tokio::fs`/`spawn_blocking`, and a straight `write` is simpler than write-temp-then-rename. The path-traversal guard (git.rs:57-60) shows the author was security-minded but the durability/runtime seam was missed.
- **Impact**: Runtime-thread starvation under load; and because the operator's pipeline commits whatever is on disk, an interrupted write can commit a half-written or empty knowledge page (silent partial data loss, no error surfaced).
- **Fix sketch**: Wrap the fs work in `tokio::task::spawn_blocking` (or use `tokio::fs`), and write to `path.tmp` then `std::fs::rename` for atomic replace on the same filesystem.

## 5. Duplicated and inconsistent Confluence request/error handling across GET/PUT/POST
- **Severity**: Medium
- **Lens**: code-refactor
- **Category**: duplication / inconsistent-error-handling
- **File**: crates/brainiac-publish/src/confluence.rs:82-142
- **Scenario**: `.basic_auth(&self.cfg.user_email, Some(&self.token))` is repeated three times (lines 87, 101, 123). The `if !res.status().is_success() { let status …; let body = res.text()…; bail!(…) }` block is copy-pasted for PUT (112-116) and POST (133-137). Meanwhile the GET (89-95) uses `.error_for_status()`, which throws away the response *body* — so a rejected version-fetch reports only a status code while update/create failures include Confluence's explanatory JSON.
- **Root cause**: Each HTTP call was written inline; no shared request builder or `check_status`/`send_json` helper was extracted.
- **Impact**: Three-way maintenance hazard (a fix to auth or error formatting must be made in three places), and asymmetric diagnostics — the one call most likely to fail on a permissions/space-id misconfiguration (the initial GET) yields the least actionable error. Not a runtime bug, but it obscures the failures the other findings depend on diagnosing.
- **Fix sketch**: Extract `fn request(&self, method, path) -> RequestBuilder` that attaches `basic_auth`, and a `async fn ensure_ok(res, ctx) -> Result<Response>` that formats `status + body` uniformly; route the GET through it too so its body is preserved.
