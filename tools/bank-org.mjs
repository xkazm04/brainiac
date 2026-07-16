/*
 * The bank's shape, as data. See docs/BANK-CORPUS.md for the reasoning.
 *
 * Split from the generator so the *org design* is readable on its own: the
 * weights below are the argument, and gen-bank-corpus.mjs is only the machinery
 * that turns them into YAML.
 */

/** FNV-ish hash → 0..1. Deterministic: same seed, same corpus, forever. */
export function h01(s, salt = 0) {
  let h = 2166136261 ^ salt;
  for (let i = 0; i < s.length; i++) {
    h ^= s.charCodeAt(i);
    h = Math.imul(h, 16777619);
  }
  return ((h >>> 0) % 100000) / 100000;
}
export const hInt = (s, n, salt = 0) => Math.floor(h01(s, salt) * n) % n;
export const pick = (xs, s, salt = 0) => xs[hInt(s, xs.length, salt)];

/**
 * `weight` is the team's share of the corpus (BANK-CORPUS §2.1: volume follows
 * incidents, not headcount). Money movement dominates; the control functions
 * know less but what they know is org-visible and long-lived.
 */
export const TEAMS = [
  { id: "team-payments", name: "payments", domain: "money", weight: 16 },
  { id: "team-core-banking", name: "core-banking", domain: "core", weight: 15 },
  { id: "team-cards", name: "cards", domain: "money", weight: 11 },
  { id: "team-channels", name: "channels", domain: "front", weight: 10 },
  { id: "team-data", name: "data", domain: "platform", weight: 9 },
  { id: "team-platform", name: "platform", domain: "platform", weight: 9 },
  { id: "team-lending", name: "lending", domain: "products", weight: 8 },
  { id: "team-fincrime", name: "fincrime", domain: "control", weight: 7 },
  { id: "team-risk", name: "risk", domain: "control", weight: 6 },
  { id: "team-deposits", name: "deposits", domain: "products", weight: 5 },
  { id: "team-compliance", name: "compliance", domain: "control", weight: 2.5 },
  { id: "team-security", name: "security", domain: "platform", weight: 2.5 },
];

/**
 * The five hubs (§2.2): entities many teams touch and each names differently.
 * `aliases` are the dialects — this is what canonical binding has to survive.
 */
export const HUBS = [
  {
    id: "hub-ledger",
    canonical: "ledger",
    kind: "service",
    owner: "team-core-banking",
    dialects: {
      "team-core-banking": ["ledger-core", "the ledger"],
      "team-payments": ["posting engine", "ledger-svc"],
      "team-cards": ["the book"],
      "team-data": ["ledger stream"],
      "team-risk": ["exposure ledger"],
    },
  },
  {
    id: "hub-customer",
    canonical: "customer record",
    kind: "concept",
    owner: "team-core-banking",
    dialects: {
      "team-core-banking": ["party record", "CIF"],
      "team-channels": ["the profile"],
      "team-fincrime": ["KYC subject"],
      "team-lending": ["the applicant"],
      "team-compliance": ["data subject"],
    },
  },
  {
    id: "hub-rail",
    canonical: "payment rail",
    kind: "system",
    owner: "team-payments",
    dialects: {
      "team-payments": ["SEPA rail", "instant rail"],
      "team-core-banking": ["the clearing feed"],
      "team-channels": ["send money"],
      "team-fincrime": ["the monitored flow"],
    },
  },
  {
    id: "hub-scheme",
    canonical: "card scheme",
    kind: "system",
    owner: "team-cards",
    dialects: {
      "team-cards": ["the scheme", "Visa/MC gateway"],
      "team-payments": ["scheme rail"],
      "team-fincrime": ["scheme fraud feed"],
      "team-risk": ["interchange source"],
    },
  },
  {
    id: "hub-kyc",
    canonical: "KYC decision",
    kind: "concept",
    owner: "team-fincrime",
    dialects: {
      "team-fincrime": ["CDD outcome", "the KYC call"],
      "team-channels": ["onboarding check"],
      "team-lending": ["identity gate"],
      "team-compliance": ["the CDD record"],
    },
  },
];

/** The long tail: one team, nobody else cares. Names are per-domain. */
export const SERVICES = {
  money: ["psp-gateway", "refund-worker", "sepa-adapter", "swift-bridge", "instant-router", "settlement-batcher", "chargeback-svc", "3ds-broker", "tokenizer", "clearing-poller", "mandate-store", "direct-debit-runner", "fx-quoter", "payout-scheduler"],
  core: ["posting-engine", "interest-accrual", "eod-batch", "account-svc", "statement-gen", "balance-cache", "product-catalog", "fee-engine", "standing-order-svc", "overdraft-engine", "gl-reconciler", "calendar-svc"],
  front: ["mobile-bff", "web-bff", "public-api", "session-svc", "notification-hub", "feature-flags", "consent-ui", "device-registry", "push-relay", "webview-shell"],
  products: ["origination-svc", "underwriting-engine", "servicing-svc", "collections-worker", "rate-engine", "term-deposit-svc", "offer-store", "affordability-calc", "arrears-tracker", "product-switch-svc"],
  control: ["sanctions-screen", "txn-monitor", "case-manager", "scoring-svc", "limits-engine", "regreport-gen", "audit-trail", "pep-screen", "alert-router", "sar-filer", "model-registry", "exposure-calc"],
  platform: ["feature-store", "warehouse-etl", "event-bus", "mesh-gateway", "deploy-ctl", "secret-broker", "iam-svc", "dr-orchestrator", "schema-registry", "cost-reporter", "trace-collector", "config-svc"],
};

/**
 * Regional and channel deployments.
 *
 * A bank does not run one `psp-gateway`; it runs one per market, per rail, and
 * per legacy migration it never finished. This is the single biggest reason a
 * real graph has hundreds of nodes where a fixture has thirty — and it is not
 * padding: an operator genuinely has to tell `psp-gateway-eu` from
 * `psp-gateway-uk-legacy` at 2am, which is exactly what the cortex map is for.
 */
export const VARIANTS = ["", "-eu", "-uk", "-legacy", "-v2"];

/** Non-service entities. A corpus of only services is a corpus of only nouns. */
export const CONCEPTS = {
  money: ["settlement window", "scheme fee", "end-to-end id", "value date"],
  core: ["posting rule", "accrual basis", "product calendar", "book balance"],
  front: ["SCA exemption", "session token", "consent scope", "app release train"],
  products: ["bureau score", "affordability rule", "offer expiry", "arrears stage"],
  control: ["sanctions list", "alert threshold", "case SLA", "risk appetite"],
  platform: ["blast radius", "rollout ring", "retention window", "cost centre"],
};

/**
 * Cross-cutting practices (§2.7) — the divergence substrate. Every team solved
 * each of these, none of them talked. `variants` are the ways they solved it.
 */
export const PRACTICES = [
  {
    id: "retry",
    practice: "service retry policy",
    variants: [
      "retry cap of 2 seconds over 3 attempts for all internal calls",
      "retry cap of 30 seconds with jitter, tuned for an external provider",
      "no retries at all — fail fast and let the caller decide",
      "exponential backoff to 60 seconds, unbounded attempts",
    ],
  },
  {
    id: "idem",
    practice: "idempotency key TTL",
    variants: [
      "keys retained 24 hours",
      "keys retained 7 days to cover settlement reconciliation",
      "keys expire after 1 hour to bound Redis memory",
      "keys are never expired, and the table is swept quarterly",
    ],
  },
  {
    id: "pii",
    practice: "PII handling in logs",
    variants: [
      "hashed at the call site before any log line is written",
      "redacted downstream by the log processor",
      "PAN truncated to first6/last4, everything else passes through",
      "full payloads logged at DEBUG, disabled in production by config",
    ],
  },
  {
    id: "deploy",
    practice: "deploy approval",
    variants: [
      "two maintainer approvals through an OPA override PR",
      "single on-call approval in the deploy CLI",
      "change-advisory board sign-off for anything touching the ledger",
      "automatic on green CI, with a 30-minute bake",
    ],
  },
  {
    id: "secrets",
    practice: "secret rotation",
    variants: [
      "quarterly rotation through the vault CLI",
      "annual rotation, tracked in a spreadsheet",
      "rotation on personnel change only",
      "90-day automated rotation with a 14-day expiry alert",
    ],
  },
];

/**
 * Boundary pairs (§2.6): where two teams own two halves of one truth. Every
 * contradiction in the corpus is generated on one of these seams — scattering
 * them randomly would teach the disputes bench a lie about where conflict lives.
 */
export const SEAMS = [
  { a: "team-payments", b: "team-core-banking", topic: "when a payment is posted versus settled", hub: "hub-ledger" },
  { a: "team-cards", b: "team-fincrime", topic: "whether a chargeback hold outranks a fraud hold", hub: "hub-scheme" },
  { a: "team-lending", b: "team-risk", topic: "which credit score is authoritative at decision time", hub: "hub-customer" },
  { a: "team-channels", b: "team-fincrime", topic: "whether onboarding may proceed before CDD clears", hub: "hub-kyc" },
  { a: "team-payments", b: "team-compliance", topic: "how long payment instructions must be retained", hub: "hub-rail" },
  { a: "team-data", b: "team-compliance", topic: "whether hashed PAN in the warehouse is still personal data", hub: "hub-customer" },
  { a: "team-core-banking", b: "team-deposits", topic: "which clock interest accrual uses at period end", hub: "hub-ledger" },
];

/** kind → how long it stays true (§2.3), in days. Drives valid_to. */
export const HALF_LIFE = {
  fact: 540,
  decision: 720,
  policy: 1100,
  pitfall: 180,
  howto: 300,
};

export const KINDS = ["fact", "decision", "pitfall", "howto"];

/**
 * Domain-flavoured claims. `X` is the entity the memory anchors to.
 *
 * Each entry is [kind, title, content]. The TITLE is how an operator would refer
 * to the claim in a list — it names the thing and what about it. The CONTENT is
 * the claim, and it still stands alone: an agent is served the content without
 * the row it was listed in, so the title can never carry load-bearing meaning.
 * That is the whole reason they are two fields and not one truncation.
 */
export const CLAIMS = {
  money: [
    ["pitfall", "X retry storms in the settlement window", "retrying against X during a settlement window piles requests up faster than the provider recovers"],
    ["decision", "X moved behind the instant-payment router", "X was moved behind the instant-payment router so the SEPA path and the instant path stop sharing a timeout budget"],
    ["fact", "X rejects reused end-to-end ids", "X rejects any instruction whose end-to-end id is reused inside the idempotency window"],
    ["howto", "replay a stuck batch through X", "replay a stuck batch through X with the settlement CLI, and never by re-submitting the file"],
    ["fact", "X latency spike at 14:00 UTC", "X publishes settlement batches at 14:00 UTC, and its latency triples for roughly twenty minutes"],
    ["decision", "X representment capped at one attempt", "chargeback representment through X is capped at one attempt per scheme rules"],
  ],
  core: [
    ["decision", "X posts value-dated entries", "X posts value-dated entries, so a payment can be authorised today and posted with yesterday's date"],
    ["pitfall", "X double-counts against the EOD batch", "running X concurrently with the end-of-day batch double-counts accrual on any account touched in both"],
    ["fact", "X holds the authoritative balance", "X holds the authoritative balance; every other balance in the bank is a cache and may be stale"],
    ["howto", "reverse a mis-posted entry in X", "reverse a mis-posted entry in X with a compensating posting — the ledger is append-only and rows are never updated"],
    ["fact", "X accrues on the product calendar", "interest accrual in X uses the account's product calendar, not the banking calendar"],
  ],
  front: [
    ["decision", "X talks to the BFF, never to core", "X calls the BFF and never a core service directly, so a core migration cannot break the app"],
    ["pitfall", "X caches the profile past a KYC change", "X caches the customer profile for 15 minutes, so a KYC status change is invisible to the app until it expires"],
    ["howto", "roll a change through X behind a flag", "roll a change through X behind a feature flag scoped to internal staff before any customer sees it"],
    ["fact", "X enforces SCA above the PSD2 threshold", "X enforces SCA on every payment initiation above the PSD2 exemption threshold"],
  ],
  products: [
    ["decision", "X freezes the bureau score at application", "X takes the bureau score at application time and freezes it for the life of the decision"],
    ["pitfall", "re-scoring in X after an offer is issued", "re-scoring in X after an offer is issued produces an offer the applicant was never shown"],
    ["fact", "X audits every decision, declines included", "X writes an audit record for every underwriting decision, including the declines"],
    ["howto", "reprice a term deposit in X", "reprice a term deposit in X by issuing a new rate version — existing holdings keep the rate they were sold"],
  ],
  control: [
    ["policy", "X retains screening decisions for five years", "X must retain every screening decision for five years, including the ones that cleared"],
    ["decision", "X holds rather than declines", "X holds a payment pending review rather than declining it, so a false positive is recoverable"],
    ["fact", "X screens at initiation and settlement", "X screens against the sanctions list at both initiation and settlement, because the list can move in between"],
    ["pitfall", "X threshold tuning floods the case queue", "tuning X's threshold down floods the case queue faster than the team can work it, and the backlog itself becomes the risk"],
    ["policy", "X data exports complete within thirty days", "a personal data export from X must complete within thirty days of the request"],
  ],
  platform: [
    ["decision", "X is the only path to production", "X is the only supported path to production; the previous pipeline is frozen"],
    ["pitfall", "X autoscaling is off on the stateful tier", "X's autoscaling is disabled on the stateful tier, so a traffic spike there is a manual intervention"],
    ["howto", "rotate a credential in X without an outage", "rotate a credential in X and roll the consumers before revoking the old one — revoke-first causes an outage"],
    ["fact", "X replicates cross-region asynchronously", "X replicates cross-region asynchronously, so a regional failover can lose the last few seconds of writes"],
  ],
};
