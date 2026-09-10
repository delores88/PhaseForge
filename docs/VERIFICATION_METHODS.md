# Methods and sources for the 0.5 verification lane

## Implemented choices

The campaign's added search mode is a bounded **k-nearest-neighbor behavioral sparsity heuristic**, not a new scientific algorithm or a calibrated Bayesian method. It averages distances to the five nearest feasible measured neighbors in the recipe's fixed descriptor ranges. Warm-up uses stratified samples; subsequent proposals use a 30% broad-search fraction and mutations around sparse measured behavior. Values reflect at bounds rather than piling up at clipped endpoints. Finalists retain a quality control first and then maximize separation from already retained behaviors. These are project design choices; no state-of-the-art performance is established here.

The new verifier independently codes the Dormand–Prince 5(4) tableau and local error control, not SciPy internals. Adaptive steps are capped by the original observation grid so aggregate measurements remain comparable. Its expression evaluator comes from the separately maintained `reference_ode.py`, so agreement is not independence of the expression language or physical model. The test suite uses analytic systems and, when SciPy is already available to a developer, compares one result with SciPy's DOP853 as an additional check. SciPy is not an application dependency.

Documented reference matching is normalized Euclidean/RMS distance in specified measurements with exact metadata compatibility. It is not nearest-neighbor literature inference. Bibliographic search and human full-text comparison remain separate.

## Primary technical background

- Mouret, J.-B. and Clune, J. (2015), *Illuminating search spaces by mapping elites*, arXiv:1504.04909. Background for the existing quality-diversity archive; not a result obtained by PhaseForge. https://arxiv.org/abs/1504.04909
- SciPy official RK45 documentation identifies the Dormand–Prince pair and its error-control interpretation, with the original Dormand and Prince (1980) reference. PhaseForge implements its own bounded replay and does not provide SciPy's dense-output/interpolation API. https://docs.scipy.org/doc/scipy/reference/generated/scipy.integrate.RK45.html
- Crossref REST API documentation defines member-deposited scholarly metadata; source text must not be inferred from returned titles. https://www.crossref.org/documentation/retrieve-metadata/rest-api/
- Crossref access/rate-limit documentation describes public access, rate and concurrency constraints, and 429 behavior. PhaseForge serializes its searches and retains errors; it does not treat service failures as empty prior-art findings. https://www.crossref.org/documentation/retrieve-metadata/rest-api/access-and-authentication/

None of these sources establishes the novelty, numerical validity, or scientific utility of a user's result. Bibliographic metadata is not independent examination of source texts. Read the relevant literature and record exactly what was compared.
