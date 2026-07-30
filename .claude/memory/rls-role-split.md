# The admin/`brainiac_app` role split is the RLS mechanism

`crates/brainiac-store/src/lib.rs` (module header) encodes three distinct
connection paths, and they are not interchangeable:

1. `migrate(url)` connects as the **owner/admin** role in the URL and runs DDL
   plus role grants. DDL must never run under the constrained role.
2. `Store::connect(url)` builds the runtime pool with an `after_connect` hook
   that `SET ROLE`s every session down to the **non-owner `brainiac_app`**
   role. This is why RLS applies at all — table owners and superusers bypass
   RLS policies; the app role cannot.
3. `scoped_tx(principal)` opens a transaction with `app.org_id` / `app.user_id`
   (and `app.project_id`) `SET LOCAL` from the verified principal. There is no
   unscoped query path for request or pipeline handling.

**The trap:** `admin_pool(url)` also exists in the same file and deliberately
**bypasses RLS** — it skips the `brainiac_app` demotion. Its only sanctioned
use is the tenant-blind re-embed backfill, which writes *derived* data
(`memory_embeddings`, `canonical_entity_embeddings`) recomputed from content
the operator already stores, so it discloses nothing new. Never route request
handling, or any path that writes knowledge, through `admin_pool`. If you find
yourself reaching for it to "just get the rows", you are about to open a
cross-tenant leak.
