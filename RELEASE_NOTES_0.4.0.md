# PhaseForge 0.4.0 — Discovery campaigns and research records

An incremental source release from 0.3.3. Existing numerical, molecular, chat,
Findings, usage/cost, resource-monitoring, theme, and Windows-install components
remain in place. No npm or Cargo dependency versions were changed.

## New execution, not just new labels

* Latin-hypercube and differential-evolution algorithms in the existing batch
  optimizer. DE/rand/1/bin uses F=0.7 and CR=0.9; each evaluated proposal competes
  against its actual parent before population sorting. Both existing numerical
  model families use the new selection step.
* Durable discovery studies with frozen parameter bounds, metric goals,
  behavioral descriptors, seeds, time allocations and comparison tolerances.
* Stratified exploration or MAP-Elites-style adaptive behavioral illumination.
  A cell retains its best feasible candidate; descriptive Pareto analysis tracks
  competing objectives. Raw failed, infeasible and out-of-range trials remain.
* Explicitly approved campaigns automatically evaluate their bounded trial plan,
  then freeze finalists and test each at h/2 and h/4. This is numerical agreement
  testing with one engine and common random numbers, not independent replication.
* Pause, end, restart reconciliation, and remaining-budget resume. At most one
  campaign executes at a time; no automatic restart after backend interruption.
* Measured-series diagnostics (range, variability, turning points, lag statistics,
  and a bounded uniform-sampling frequency hint). The hints are not periodicity,
  chaos, significance, or novelty certificates.

## Natural-language research workflow

A scientist can select an existing experiment and request **AI study design**.
The metered agent prepares a new complete manifest with suggested bounds,
observables, objectives, and challenges. The user reviews the form, freezes the
protocol, and separately approves compute. The local protocol editor also works
without any API key.

After a campaign, **Request AI review** produces a skeptical advisory linked to
the full evidence report, including rejects. **Propose next experiment** may
create a new validated revision, but does not execute it. One optional automatic
review is off by default. Existing token/cost admission and Stop all agents apply;
repair attempts are separately recorded and billed under the existing policy.

## Research organization and export

Versioned notebooks record human authors, ORCIDs, contributions, hypotheses,
protocols, observations, interpretations, counter-evidence, limitations,
literature comparisons and disclosures. Claim links must refer to completed runs
in the same research world. Notebook revisions are immutable and transactional;
stale concurrent writes are rejected. Manifest revision reservations are now
atomic across chat and campaign workers.

An explicit Crossref search returns bibliographic metadata, not full papers.
Only the entered search query leaves the machine; no model key is sent. References
are human-editable research records. No-match results never establish novelty.

Research ZIP export includes exact study recipes, all run records and manifests,
CSV measurements/series, data-derived SVG figures, an author-review manuscript
scaffold, claim ledger, BibTeX/CSL, author disclosures, RO-Crate-style metadata,
and per-file hashes. Keys, endpoint settings, prompts, chat history and price cards
are excluded. The latest saved notebook revision is exported.

A separately implemented standard-library Python ODE integrator supports
Euler/RK4 replay from exported inputs and comparison with recorded measurements.
It does not require PhaseForge, a model key, a GPU, NumPy, or SciPy. It must actually
be run by the scientist before recording an independent-verification claim.

## Deliberate boundaries

Campaigns currently execute sequential CPU/f64 trials through the existing local
scheduler (4–512 exploration trials; 0–6 finalists). The native batch optimizer
still retains its supported GPU path. No distributed/multi-GPU campaign runtime,
new molecular dynamics or quantum solver, Bayesian surrogate, automatic novelty
adjudication, formal proof, journal submission, or cross-user access-control system
is claimed in this release. Those are future capabilities, not hidden stubs.

See `docs/RELEASE_VALIDATION.md` for executed tests and unexecuted native gates.
