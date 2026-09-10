# Verification and prior-work dossiers

## A workflow, not a novelty badge

The question is not “did the optimizer return a good score?” It is whether the selected candidate represents a reproducible phenomenon under the authored model, whether the interpretation survives new challenges, and what existing work already explains it.

Start in **Discovery** with an authored executable baseline. Approve a small exploratory campaign and inspect every attempted trial. `novelty_search` favors sparsely occupied behavior; the existing LHS and MAP-Elites-style strategies remain available. Define descriptors that distinguish meaningful behavior rather than a cosmetic variation or arbitrary numerical scale.

After exploration, pause or finish the study. In **Verify & compare**, choose a feasible exploration candidate. You can use the study's frozen finalists, but an unrefined candidate will carry an explicit missing-refinement gap.

### 1. Freeze the question and assumptions

Choose 1–8 metrics already measured in the exact selected run. Set a physically meaningful comparison scale and separate tolerances for implementation agreement and perturbation robustness. No model key is needed. Defaults are editable suggestions, not scientific validation.

Select 2–16 perturbations with a new seed, a fraction of each parameter's full allowed range, an adaptive solver tolerance, a total RHS evaluation ceiling and a wall budget. Declare known controls: a completed ODE run, a metric, its independently justified expected interval and a rationale. The candidate cannot serve as its own control. No control is an explicit gap, not an automatic pass.

Optionally freeze numerical references. Each entry needs a source and license/permission note. Local references must be completed runs in the same world and comparison namespace. External measurements are entered by the researcher and are **not automatically verified against their stated source**. Check the same units, normalization, measurement definition and numerical domain before importing them.

Freeze creates a dossier; it does not start a worker. The frozen protocol is post-selection verification—not external preregistration and not a way to erase exploratory selection bias.

### 2. Approve independent computation

Paste `INSTALL_VERIFIER.txt` once in the project root; restart the backend and check the worker. Approve a frozen dossier's separate budget. This executes code shipped with PhaseForge, never arbitrary code authored by an agent.

The worker performs:

1. Independent Euler/RK4 replay of the exact nominal candidate.
2. Adaptive Dormand–Prince 5(4) replay of the same ODE.
3. Declared calibration/control interval checks through the adaptive implementation.
4. New bounded parameter perturbations, recorded with actual values, against the independent nominal result.

The adaptive solver adjusts internal steps while sampling aggregate metrics on the original integration grid. Otherwise differences in means or extrema could reflect a changed measurement convention rather than a model discrepancy. This design does not provide dense output or rigorous continuous-time error bounds. Smooth nonstiff ODEs are its intended scope; discontinuities, stiff systems and event-sensitive problems require a more suitable method.

Agreement uses `absolute tolerance + relative tolerance × max(abs(recorded), abs(independent))`. Perturbation checks use the separately declared absolute robustness tolerance. Every configured comparison and endpoint/constraint check matters. Failed/missing values stay failed or inconclusive.

A sensitive candidate may be interesting. “Sensitive or invalid” does not mean the observation is useless; it means a broad stability claim is not supported by this protocol. Freeze a new, justified follow-up rather than editing away the failure.

The worker runs once per approved dossier. It has total RHS and wall caps, one independent process at a time and an explicit Stop button. Completed checkpoints survive a hard stop when available. Their counters describe the last completed checkpoint, not incomplete work inside the interrupted calculation. A backend restart marks work interrupted and never auto-resumes it. Create a new dossier for a further attempt; old attempts remain.

### 3. Compare, including the uncomfortable matches

Local k-neighbor rarity is reported separately from documented-reference distance. The comparison namespace hashes the model, observables, constraints, integration settings, aliases, selected features/scales and declared parameter targets; only the searched scalar values are masked. This deliberately conservative rule can exclude physically equivalent models encoded differently.

Distance is RMS across explicit measurement scales. Close signatures flag possible rediscovery; different signatures justify only “distinct in the selected features.” No rotation, time shift, dimensional rescaling, symmetry, topology, dynamical equivalence or exhaustive catalog coverage is inferred. Two physically different systems can share a feature vector, and equivalent systems can have different vectors.

A search sends only the query the user consents to send to public Crossref. It retains the first 12 bibliographic matches, query/time/scope and errors. It does not download papers or claim to have read full text. Empty results and failures cannot certify absence of prior work. Searches across PhaseForge are serialized, with a short spacing delay and no runaway retry loop; provider rate-limit errors remain visible. Researchers should broaden/adjust sources manually as necessary.

### 4. Human review and publishable evidence

The reviewer records identity by name (not cryptographic authentication), sources and pages actually examined, comparison, limitations and a provisional disposition: inconclusive, artifact/failed, rediscovery, or potentially distinct. A full-text declaration is a human attestation, not automated proof of reading. Every review is stamped against the current evidence hash. New searches or changed evidence make older reviews stale.

The maximum automated status is **ready for external review**, not discovered, proven, safe, effective, clinically validated or accepted for publication. It requires the numerical and comparison requirements plus a current, substantive potentially-distinct human review; human notes cannot override missing calculations. “Inconclusive” and “rediscovery” remain valid scientific outcomes.

Optional **Request metered AI review** makes one read-only call through the existing token/cost ledger and cancellation controls. It receives bounded verification, prior-work and recorded-signal diagnostics. The call cannot execute a simulation or write a manifest. Its text is stored in project chat and shown beside the dossier. Subsequent experiments continue through the existing explicit proposal → review → approve flow.

## Actual limits

- Independent execution: first-order ODE manifests only, 1–128 states, supported safe expressions. CPU/Python standard library; no GPU acceleration in this lane.
- Dossier: 1–8 metrics, 2–16 perturbations, 0–4 controls, 0–100 selected references.
- Approved work: 5–3,600 wall seconds; 100–20,000,000 RHS evaluations across all phases. Bounds are caps, not timing promises.
- Input snapshot cap: 64 MiB. Worker output cap: 16 MiB. Research export cap remains 128 MiB. Oversize records fail explicitly instead of silently dropping evidence.
- One worker per backend; this is not a multi-process/distributed coordinator. Do not point concurrent backend versions at the same database.
- Up to 20 stored searches and 100 human review revisions per dossier. Catalog/dossier capacity is 10,000 objects each; archive before exceeding it.
- Optional interpreter: explicit absolute `PHASEFORGE_VERIFIER_PYTHON`, local `.phaseforge-verifier`, or compatible Python on PATH. Code is embedded at Rust compile time; changed verifier source requires rebuilding and a new dossier.
- Native application integration has not been executed in the packaging environment. Consult the release validation record and run CI/native acceptance before serious research use.
