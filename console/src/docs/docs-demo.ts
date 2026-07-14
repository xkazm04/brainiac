/*
 * Demo shape for the document layer (KB-PLAN KB2).
 *
 * Rendered only behind <DemoBanner /> when `brainiac serve` is unreachable, so
 * nobody mistakes the Meridian fixture org's pages for their own. The Approve
 * action is hidden offline — a mutating control must never be wired to
 * fabricated data.
 *
 * Deliberately not a flattering corpus: the page carries an `in_flight` claim
 * (idempotency keys, decided but not in production), one page is dirty, and one
 * has a revision waiting on a human. A demo where everything is shipped and
 * published would teach the reader that the lifecycle marks are decoration —
 * they are the whole point.
 */

import type { DocDetail, DocRevisionSummary, DocSummary } from "@/lib/types";

const M = {
  routing: "1a2b3c4d-0001-4a11-8c01-9f0000000001",
  retry: "1a2b3c4d-0002-4a11-8c01-9f0000000002",
  idem: "1a2b3c4d-0003-4a11-8c01-9f0000000003",
  webhook: "1a2b3c4d-0004-4a11-8c01-9f0000000004",
  timeout: "1a2b3c4d-0005-4a11-8c01-9f0000000005",
  ledger: "1a2b3c4d-0006-4a11-8c01-9f0000000006",
} as const;

export const DEMO_DOC_SLUG = "psp-gateway";

export const DEMO_DOCS: DocSummary[] = [
  {
    id: "d0000000-0000-4000-8000-000000000001",
    slug: DEMO_DOC_SLUG,
    title: "psp-gateway",
    doc_kind: "entity_page",
    visibility: "org",
    status: "published",
    dirty: false,
    pending_review: true, // a recomposed revision is held back — see DEMO_DOC.pending
    updated_at: "2026-07-13T09:12:00Z",
  },
  {
    id: "d0000000-0000-4000-8000-000000000002",
    slug: "payments-runbook",
    title: "Payments — on-call runbook",
    doc_kind: "runbook",
    visibility: "team",
    status: "published",
    dirty: true, // a bound memory changed; recomposition is queued
    pending_review: false,
    updated_at: "2026-07-14T06:40:00Z",
  },
  {
    id: "d0000000-0000-4000-8000-000000000003",
    slug: "checkout-service",
    title: "checkout-service",
    doc_kind: "entity_page",
    visibility: "org",
    status: "draft",
    dirty: false,
    pending_review: false,
    updated_at: "2026-07-12T15:02:00Z",
  },
];

const CONTENT_MD = `## What it is

The psp-gateway is Meridian's single egress point to every payment service
provider; no other service may call a PSP directly. [m:${M.routing}]

Card authorizations route to Stripe by default, and to Adyen for merchants
settling in EUR. [m:${M.routing}] The routing decision is taken once, at
authorization time, and recorded on the ledger entry — a capture never
re-routes. [m:${M.ledger}]

## Reliability

Failed PSP calls are retried with exponential backoff and full jitter, capped
at four attempts. [m:${M.retry}] The upstream timeout is 8 seconds; a caller
that sets a shorter deadline will see a gateway timeout before the PSP has
answered. [m:${M.timeout}]

\`\`\`toml
[psp.retry]
max_attempts = 4
initial_backoff_ms = 200
multiplier = 2.0
jitter = "full"
\`\`\`
<sub>[m:${M.retry}]</sub>

Idempotency keys will be required on every authorization call, so that a retry
after a client-side timeout cannot double-charge. [m:${M.idem}]

## Webhooks

PSP webhooks are verified by HMAC signature and replayed into the ledger
consumer; an unverified webhook is dropped, never queued. [m:${M.webhook}]

| Event | Source | Handler |
| --- | --- | --- |
| \`charge.succeeded\` | Stripe | ledger-consumer |
| \`AUTHORISATION\` | Adyen | ledger-consumer |
`;

export const DEMO_DOC: DocDetail = {
  document: DEMO_DOCS[0],
  // The page's sections, as the API names them (KB4). Two composed projections
  // and one pinned human-owned section — the asymmetry the editor exists to
  // make legible. Offline these are read-only: the editor is never wired to
  // demo data (see app/docs/[slug]/page.tsx).
  sections: [
    { id: "5e000000-0000-4000-8000-000000000001", heading: "What it is", mode: "composed" },
    { id: "5e000000-0000-4000-8000-000000000002", heading: "Reliability", mode: "composed" },
    { id: "5e000000-0000-4000-8000-000000000003", heading: "Webhooks", mode: "pinned" },
  ],
  revision: {
    id: "r0000000-0000-4000-8000-000000000009",
    content_md: CONTENT_MD,
    composed_from: Object.values(M),
    policy_decision: "auto_published",
    published_at: "2026-07-13T09:12:00Z",
    created_at: "2026-07-13T09:11:41Z",
  },
  // A recompose triggered by the idempotency-key decision landing as canonical:
  // it is held back because the new draft drops a previously published claim.
  pending: {
    id: "r0000000-0000-4000-8000-000000000011",
    content_md: CONTENT_MD.replace(
      `Idempotency keys will be required on every authorization call, so that a retry
after a client-side timeout cannot double-charge. [m:${M.idem}]`,
      `Every authorization call carries an idempotency key, so a retry after a
client-side timeout cannot double-charge. [m:${M.idem}]`,
    ),
    composed_from: Object.values(M),
    policy_decision: "needs_review",
    created_at: "2026-07-14T06:41:20Z",
  },
  citations: [
    {
      memory_id: M.routing,
      content:
        "All PSP traffic egresses through psp-gateway; card authorizations route to Stripe by default and to Adyen for EUR-settling merchants.",
      kind: "decision",
      lifecycle: "shipped",
      status: "canonical",
      team: "payments",
    },
    {
      memory_id: M.retry,
      content:
        "PSP calls retry with exponential backoff and full jitter, capped at four attempts.",
      kind: "config",
      lifecycle: "shipped",
      status: "canonical",
      team: "payments",
    },
    {
      memory_id: M.idem,
      content:
        "Authorization calls will require an idempotency key so a client-side timeout retry cannot double-charge.",
      kind: "decision",
      lifecycle: "in_flight", // decided, signed, NOT in production
      status: "canonical",
      team: "payments",
    },
    {
      memory_id: M.webhook,
      content:
        "PSP webhooks are HMAC-verified before entering the ledger consumer; unverified webhooks are dropped, not queued.",
      kind: "practice",
      lifecycle: "shipped",
      status: "canonical",
      team: "platform",
    },
    {
      memory_id: M.timeout,
      content: "The psp-gateway upstream timeout is 8 seconds.",
      kind: "config",
      lifecycle: "shipped",
      status: "canonical",
      team: "platform",
    },
    {
      memory_id: M.ledger,
      content:
        "The PSP routing decision is recorded on the ledger entry at authorization time; captures never re-route.",
      kind: "decision",
      lifecycle: "shipped",
      status: "canonical",
      team: "ledger",
    },
  ],
};

export const DEMO_REVISIONS: DocRevisionSummary[] = [
  {
    id: "r0000000-0000-4000-8000-000000000009",
    content_md: CONTENT_MD,
    policy_decision: "auto_published",
    published_at: "2026-07-13T09:12:00Z",
    created_at: "2026-07-13T09:11:41Z",
    composed_from: Object.values(M),
  },
  {
    id: "r0000000-0000-4000-8000-000000000005",
    content_md: CONTENT_MD.split("Idempotency keys")[0].trimEnd(),
    policy_decision: "needs_review",
    published_at: "2026-06-29T11:03:00Z",
    created_at: "2026-06-29T10:47:12Z",
    composed_from: [M.routing, M.retry, M.webhook, M.timeout],
  },
  {
    id: "r0000000-0000-4000-8000-000000000001",
    content_md: CONTENT_MD.split("## Reliability")[0].trimEnd(),
    policy_decision: "needs_review", // a page's first revision always needs a human
    published_at: "2026-06-02T08:20:00Z",
    created_at: "2026-06-02T08:19:55Z",
    composed_from: [M.routing, M.retry],
  },
];
