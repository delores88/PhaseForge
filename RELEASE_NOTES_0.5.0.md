# PhaseForge 0.5.0 — verify, compare, and make claims accountable

Complete incremental release from 0.4.0. Rust/Axum/wgpu and Next.js/React remain in place. No dependency-version changes, replacement framework, canned scientific problem, or automatic publication.

## Implemented

- A `novelty_search` campaign option uses measured behavioral sparsity (five nearest neighbors in fixed descriptor scales), broad sampling and reflected mutations. Feasible quality-control and behaviorally diverse finalists are retained. This is novelty within the explored dataset, not novelty to science.
- **Discovery → Verify & compare** freezes a selected candidate, exact run/manifest, control runs, tolerances, new perturbation seed, numerical reference corpus and verifier source hashes in a durable dossier.
- A separately approved local Python worker executes an independent Euler/RK4 implementation and a new adaptive Dormand–Prince 5(4) implementation; the original observation grid defines summary measurements.
- Explicit control intervals, 2–16 new perturbations, total right-hand-side evaluation limits, wall limits, progress, cancellation and completed checkpoints. Missing or failed checks cannot become a pass. Restarting never resumes verification.
- Documented numerical references can come from completed local runs or author-supplied published measurements. Compatibility checks, fixed scales, near-match flags and excluded-reference counts distinguish a comparison from an absent corpus.
- Consented Crossref metadata searches retain queries, responses, failures and scope. Named human reviews record full-text reading declarations, limitations, dispositions and an evidence hash. Changed evidence makes an older review stale.
- Bounded recorded-signal windows, deterministic signal diagnostics and read-only, metered Falsifier interpretation. The new advisory path cannot write manifests or submit runs.
- Research bundles include exact verifier inputs, outputs, controls, references, searches, reviews and source. Publication readiness lists unresolved verification requirements rather than certifying novelty.

## Installation

Stop both old services, back up application data, extract a fresh full ZIP, then paste **all** of `INSTALL_ALL.txt`. Existing data/key locations are unchanged. Paste `INSTALL_VERIFIER.txt` to enable the new independent execution lane; it creates an isolated Python 3.10+ standard-library environment. Then paste `START_ALL.txt`.

Python is optional for existing laboratory functionality but required for actual independent verification. There are no new pip packages, API keys or paid-license requirements for that worker. Existing Node and Rust dependency constraints are unchanged.

## Limits

The new independent worker is an ODE verifier, not a production adaptive solver replacing the main runtime, a quantum engine or an MD runner. It shares the companion reference tool's expression semantics but implements a different integration method. It does not establish independence of the physical assumptions.

No symmetry-aware orbit classification, exhaustive known-solution database, automated full-text literature review, multiple-testing-corrected significance claim, local-model hosting, multi-GPU campaign orchestration or clinical discovery is claimed. Numerical reference measurements remain author-verified. A bounded search can find a candidate; neither the software nor passing its checks guarantees a new discovery.

Native compiler/build limitations and executed test results are recorded in `docs/RELEASE_VALIDATION.md`.
