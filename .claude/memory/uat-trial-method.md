# The UAT trial is a three-arm controlled experiment, and C − B is allowed to be negative

`.claude/skills/uat` + the `uat/` overlay exist to answer a question the
Meridian eval harness structurally cannot: **is an org better off with Brainiac
than with a free `CLAUDE.md`, net of the harm a shared governed store uniquely
causes?** NDCG@10 = 0.876 says nothing about whether a PR got better.

Method invariants:

- Every journey runs three arms: **A** cold, **B** the *full free* native
  memory stack (rules globs, auto-memory, symlinked org rules), **C** Brainiac
  on top of B. **The verdict is C − B, and it is allowed to be negative.** A
  run that never returns a negative delta is not being run honestly.
- **The primary endpoint is efficiency** (turns / tokens / exploration reads),
  **not** code quality. The only controlled external evidence found memory
  systems moved cost by 20–30% and code quality by ~zero — and a static context
  file *outscored* a memory system.
- `uat/baseline.md` and `uat/decoys.md` are committed on purpose: they are the
  evidence that the baseline was generous and that the trial actively tried to
  break the product with planted poison.
- Brainiac may only win in five gaps — cross-team, after-the-file, retraction,
  permission, provenance. Two journeys are deliberate `gap: none` controls that
  should show **no** delta; a delta there means the harness is broken.
- Certification is chronological: L1 theoretical over a code-derived surface
  model (cheap, mass-parallel), then L2 empirical against the live server.

Honest standing result: verdict moved `harmful-as-shaped` → **`not-yet`**,
trending adopt. Value clusters entirely in cross-team + retraction; everything
single-team measured as redundant with a static instructions file. Still open:
queue-abandonment economics, a 4th (contractor) visibility tier, no cross-team
principal, and a full three-arm *quality* L2 needs a real BYOM key (runs on the
deterministic embedder only exercise plumbing).
