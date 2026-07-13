# Rubric — the evaluation lens

## The eight dimensions

| # | Dimension | The question | Applies to |
|---|---|---|---|
| 1 | **completion** | Did the developer finish the job? | all arms |
| 2 | **effort** | Turns, tokens, wall-clock, **exploration reads** (files opened before the agent knew what to do). **← PRIMARY ENDPOINT** | all arms |
| 3 | **clarity** | Did they ever not know what to do next? | all arms |
| 4 | **trust** | Would they act on this without verifying? Could they check if they wanted to? | arm C mostly |
| 5 | **missing** | What wasn't there that should have been? | all arms |
| 6 | **decision-delta** | Did retrieved knowledge *change what they typed*, and for the better? | arm C only |
| 7 | **governance-tax** | What did the org pay in reviewer time to make this possible? | arm C only |
| 8 | **harm** | Did the memory system make something *worse*? | arm C only |

**Quality (correctness) is a guardrail, not the endpoint.** The bar is *"arm C is no worse than
arm B."* The published evidence says quality deltas from memory are ~zero and cost deltas are
20–30%; a trial that stakes its verdict on quality is a trial that will measure noise. If arm C
*does* beat arm B on quality, treat it as a surprise that needs a mechanism — name what arm C
knew that arm B structurally could not have known, or suspect the harness.

## Severity

- **blocker** — the developer cannot finish, or Brainiac caused a regression that arm B would
  have prevented. (Every decoy that lands is at least this.)
- **major** — the job is done but the value proposition is broken: no delta over baseline on a
  journey that was supposed to be a win, an unmet trust bar, a governance cost that a real
  maintainer would not keep paying.
- **minor** — friction that annoys but does not decide adoption.
- **polish** — cosmetic.

## Finding types

`missing-feature` · `quality-gap` · `broken-flow` · `confusion` · `trust` · **`harm`**

## Cognitive-walkthrough questions (asked at every step, in-Character)

1. Will I even think to ask the memory system here? *(If the answer is no, the store's quality is
   irrelevant — an uncalled tool has zero value. Check the tool description as an agent would read it.)*
2. Did what came back name **my** service, **my** repo, **my** incident — or is it org wisdom I
   already follow?
3. Would my `CLAUDE.md` have told me this for free? *(If yes → H7 redundancy. Count it.)*
4. Do I believe it? On what basis? Can I check?
5. Did it change what I typed?
6. What did it cost me — tokens, latency, a wrong turn?
7. Would I tell a teammate to turn this on?

## The blind-judge protocol (mandatory — do not shortcut this)

An LLM told *"this output used the memory system"* will find that output better. So:

1. The judge subagent receives the three arms' outputs **shuffled and unlabeled**, plus the task
   statement and the fixture answer key (gold memories, the true state of the supersession chain,
   the decoy's correct rejection).
2. It scores each on correctness, completeness, and **whether the output acted on knowledge that
   could not have been derived from the repo alone** — that last one is the tell for a real
   cross-boundary win, and it is scorable without knowing the arm.
3. It never learns which arm is which. **Unblind only after scoring.**
4. Efficiency metrics (turns/tokens/reads) are collected mechanically by the driver, not by the
   judge — they need no blinding and must not be estimated by a model.

## Multi-sampling (a single delta is not a delta)

Between two agent sessions on the same task, run-to-run variance is large — comfortably larger
than the effect size the literature reports. **Any headline number must survive 3× repetition
with a majority verdict.** A one-shot `C − B` is an anecdote and must be labeled `uncertain`,
never `confirmed`.

## The refuter's standing questions (adversarial pass, applied to every kept finding — especially the flattering ones)

- Would a line in `CLAUDE.md` have done this for free?
- Did the agent actually *use* the retrieved memory, or did it reach the same answer from the code
  and cite the memory as decoration afterwards?
- Is this delta real, or is it model variance between two sessions?
- Is this "slow" a timeout, or just a slow model call?
- If this is a harm finding: is the harm *caused* by Brainiac, or merely *visible* through it?

## Scope honesty

Deliberately deferred work (OIDC/SCIM, Cedar, S3 transcripts, the document layer, the
cross-encoder reranker) → `scope_note`, not a defect.

**But "deferred" is not a defense against a harm finding.** If the shipped shape leaks today, it
leaks today. A roadmap does not protect a secret.
