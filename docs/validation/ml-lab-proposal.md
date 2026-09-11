# Preregistered ML study: a surrogate of a measured molecular endpoint

Status: proposal only, written on 2026-09-11 before executing this study. The
installed diffusion gate is still pending. This document authorizes no new engine
catalog entry. Preserve its SHA-256 with the eventual study receipt before any
labels are generated; retain failed candidates and any later, justified protocol
revision as separate studies.

## Question, scope, and three separate outcomes

Can a small numerical model estimate the **10–20 ps mean virial pressure produced
by the existing OpenMM argon-like model**, at an unseen temperature and density,
accurately enough to help screen further solver runs? The alternative hypothesis
is that finite-sample noise or simple baselines make the learned surrogate
unhelpful. A rejected surrogate with an operating direct-solver fallback is a
valid completed evaluation, not evidence of useful acceleration.

Record these outcomes separately:

1. **Integration correctness:** real solver labels, isolated fitting, immutable
   splits/model/receipts, held-out evaluation, displayed uncertainty, and actual
   fallback/recovery work through ordinary chat.
2. **Evaluation completeness:** the frozen checks and comparisons ran, including
   failures and uncertainty. This can pass when the predictive hypothesis fails.
3. **Useful acceleration:** the additional accuracy, uncertainty, coverage, and
   total-cost thresholds below pass. Otherwise keep the candidate unavailable for
   automatic substitution and return solver results.

This predicts a reproducible finite-time computational protocol. It is not an
equation of state established at equilibrium, an empirical argon measurement, or
a biological model. The 10 ps initialization period is a declared exclusion
window, not proof of equilibration. Report drift and seed variation without
discarding difficult conditions or extending only unfavorable runs.

## Existing solver and fixed label protocol

Use the accepted `openmm_argon` adapter, OpenMM wheel 8.5.2, CPU platform with one
thread, `DeterministicForces=true`, and the exact shipped worker hash recorded by
each job. The initial FCC coordinates, sigma 0.3405 nm, epsilon 0.997 kJ/mol, mass
39.948 Da, minimum-image cutoff, quintic switching, and absent dispersion
correction remain those in [molecular-lab.md](molecular-lab.md). The upstream
[force documentation](https://docs.openmm.org/latest/userguide/theory/02_standard_forces.html)
defines the switched LJ model; an unswitched or infinite-cutoff fluid is a different
label source. Actual version receipts take precedence over the moving `latest`
documentation version.

Every label run uses N=108, Langevin Middle thermostat, friction 5/ps, timestep
1 fs, 20,000 steps, sampling every 100 steps, chunks of 50 frames, and no changed
fidelity following a failure. Retain all 201 sampled states and usual instruments,
checkpoints, images, and source hashes. Select pressure samples by integer steps
`10000 < step <= 20000`, exactly 100 observations at 0.1 ps spacing; do not select
floating-point timestamps using exact equality.

For a condition c and seed r, the target is the arithmetic mean P[c,r] in bar of
these 100 recorded pressures. The condition label P[c] is the mean of its three
seed means. The fitted dimensionless target is Z[c]=P[c]/P0[c], where
`P0=(N-1)*R*T/V * (1e25/NA)` in bar, R=0.00831446261815324 kJ/(mol K),
NA=6.02214076e23/mol, and V is the retained box volume in nm^3. The N-1 term follows
this worker's zero-center-of-mass temperature/kinetic convention; do not silently
substitute a bulk N-particle ideal pressure. Infer from T and density only. Do not
include a seed, measured temperature, intermediate pressure, or final-state
observable as a feature available only after running the solver.

Report each seed mean, standard deviation of the three seed means, standard
error `s/sqrt(3)`, and the descriptive Student-t interval with df=2 and multiplier
4.302653 for a nominal 95% interval, following the
[NIST critical-value convention](https://www.itl.nist.gov/div898/handbook/eda/section3/eda3672.htm).
Its assumptions and very small replicate count must remain visible.
It is not an equilibrium confidence interval. Also report the five consecutive
2 ps pressure-block means within each seed's label window, their serial
correlation, and the 10–15 versus 15–20 ps difference; these correlated blocks do
not become independent training rows or replace seed-level uncertainty.

## Fixed condition split and bounded work

The 32 conditions are the Cartesian product of temperatures
`[220,250,280,310,325,370,400,430]` K and densities
`[0.25,0.45,0.65,0.85]` g/cm^3. Number conditions in ascending temperature, then
density order, starting at zero. Three distinct seeds are
`110000 + 1000*condition_index + replicate_index`, with replicate_index 0,1,2.
All replicas and frames of one condition have the same role.

Hold out the entire 325 K temperature slice (four conditions) as a regime test.
For the remaining 28 conditions, sort by ascending SHA-256 hexadecimal digest of
the ASCII string `phaseforge-ml-argon-v1|T=<integer>|rho=<two decimals>`; break a
theoretical tie by temperature then density. The first 12 are training, the next
3 validation, the next 9 calibration, and the final 4 ordinary test conditions.
Write the exact resulting membership and hashes before launching the first run.
Neither test group nor calibration labels may enter fitting or model selection.

This is **96 label runs**, not an open-ended search for a successful model. Add
one actual OOD fallback condition, T=180 K and density=0.55 g/cm^3, with condition
index 32 and three seeds: **99 solver runs in total**. The first scheduled
training run is also the timing/storage pilot and is reused by identity; do not
generate an extra, selectively favorable pilot. Unsuccessful runs remain in the
ledger. An incomplete label blocks evaluation; never impute it with an LLM answer
or drop its condition. A restart from an identical, validated checkpoint retains
attempt lineage and does not count as another independent replicate.

The existing development acceptance report
`../../.local/science/acceptance-03/report.json` took 15.59 s for its full suite,
which included two 108-atom, 10 ps CPU runs plus reference/recovery checks. Each
of those temperature artifacts occupied approximately 1.86 MB at 101 frames.
A simple storage extrapolation is roughly 0.4 GB for 99 runs at 201 frames;
reserve at least 1 GiB for solver artifacts and 256 MiB for analysis. A planning
envelope of 5–30 minutes is deliberately broad: the existing evidence does not
measure these new conditions or installed orchestration overhead. Profile the
first training job, report actual wall time, steps/s, bytes/frame, and peak memory,
then replace that estimate before scheduling the remaining jobs. Keep concurrency
at one initially. Larger concurrency requires a measured resource estimate.

Use one durable sweep record with per-case solver job IDs and an immutable
manifest, not a provider call for every case. Pause cancels active children and
stops scheduling; resume reuses completed cases only after input/artifact hashes
match, and creates new attempts for incomplete cases. Off has no hidden total
deadline. An explicit parent budget can stop the sweep; preserve pending cases and
report incompleteness without reducing samples, cases, or numerical resolution.

## Fitting, baselines, calibration, and OOD policy

Use the already pinned LPAC runtime: CPython 3.13.15 and NumPy 2.4.6, CPU only.
There is no need to install scikit-learn, PyTorch, or a GPU training stack for this
small matrix problem. NumPy supplies Cholesky/SVD/linear solves; preserve its
[BSD license](https://github.com/numpy/numpy/blob/v2.4.6/LICENSE.txt) and bundled
dependency notices. The solver uses the existing unmodified OpenMM distribution;
its [licensing documentation](https://docs.openmm.org/latest/userguide/library/01_introduction.html)
distinguishes MIT core/CPU from LGPL GPU components.

Normalize input coordinates using the fixed declared bounds, T in [220,430] and
density in [0.25,0.85]. Fit residual Z-1. Center and scale targets using fitting
rows only, with a scale floor of 1e-6. Compare all of the following:

- P0 alone (Z=1), training-mean Z, and nearest training condition in the fixed
  normalized Euclidean coordinates; ties resolve by condition index.
- Linear and quadratic ridge regression, features respectively [1,x,y] and
  [1,x,y,x^2,xy,y^2], unpenalized intercept, ridge values
  `[1e-6,1e-4,1e-2,1]` on the averaged squared-error objective.
- Gaussian-kernel ridge regression, kernel
  `exp(-0.5*sum(((x-x')/ell)^2))`, independent length scales from
  `[0.15,0.3,0.6,1.2]`, and diagonal regularizers
  `[1e-6,1e-4,1e-2,0.1]` added to K. Use a Cholesky solve, not an explicit matrix
  inverse. Failed factorizations remain failed candidates; no unrecorded jitter.

These are actual fitted numerical models; their predictive means follow the
standard kernel regression construction discussed in
[Rasmussen and Williams, chapter 2](https://gaussianprocess.org/gpml/chapters/RW2.pdf).
The kernel candidate's uncertainty is not treated as a validated Gaussian-process
posterior merely because the algebra resembles one.

Select the lowest validation RMSE among the fitted candidates, including linear
and quadratic models. Tie within 1e-12 favors fewer fitted coefficients, then
lexical configuration order. Preserve every score. Refit the chosen configuration
on training plus validation rows (15 conditions), freeze its model and source
hash, and only then open calibration labels. Do not refit after calibration or
test evaluation. Baselines use the same 15 fitting conditions for final evaluation.

Set a constant nominal 90% interval half-width q from the nine absolute
calibration residuals in Z units: their order statistic at rank
`ceil((9+1)*0.9)=9`, hence the maximum. This follows the split-calibration
construction in [Angelopoulos and Bates](https://arxiv.org/abs/2107.07511).
The small designed grid and deliberately withheld temperature regime do not
establish an exchangeability or real-world 90% coverage guarantee. Report actual
coverage separately on both test groups, interval widths, and seed uncertainty.
The interval concerns a fresh three-seed finite-window endpoint, not equilibrium
pressure. Keep negative pressure predictions/intervals visible where they occur;
clipping would change the evaluation.

Every inference checks the complete protocol/model identity and support bounds.
Missing/nonfinite values, wrong units, and parameters outside the solver's own
supported limits return `invalid_input` without scheduling a job. For otherwise
valid solver requests, return `requires_solver` for T or density outside the
surrogate rectangle, normalized distance to the nearest fitted condition greater than
0.40, an unknown/corrupt model, any changed N/potential/cutoff/thermostat/time
window/platform/worker version, or a rejected predictive candidate. An interval
half-width above 0.15 Z also forces fallback. A scientific request with fallback
launches the actual solver protocol and returns its lineage; an explanation-only
question explains this status without scheduling anything. Never fabricate an
OOD prediction, silently extrapolate, or return a reused training label as a new
solver result.

## Frozen checks and rejection rules

The checker must not import fitting-worker helpers. Its independent small-matrix
fixtures check linear/ridge predictions against direct elimination, kernel solves
against a separate dense solver, and the calibration order statistic. Maximum
prediction disagreement on these well-conditioned fixtures is 1e-10 Z. Fixtures
also exercise constant labels, duplicated inputs, a singular unregularized
matrix, nonfinite values, incorrect units, changed hashes, and leakage attempts.

Integration/evaluation acceptance requires all 99 requested solver protocols or
explicit retained failures, all 96 dataset seeds bound to immutable jobs, disjoint
condition splits, and no absent label presented as a passed comparison. Full
evaluation requires the 96 seed runs to complete; the actual OOD fallback must
also finish all three solver runs and return their measured endpoint to pass its
integration check. Independently recompute every
seed mean and condition label from registered scalar samples, within 1e-10 bar.
Check sample indices/counts, time increments to 1e-8 ps, units, seed uniqueness,
source hashes, and absence of nonfinite numbers. On the first and last label-window
states of one seed per test condition, independently recalculate switched pair
virial pressure from retained numeric arrays within 1e-8 bar; do not import the
worker's pressure helper. This establishes data/instrument consistency, building
on the earlier independent LJ force/integrator acceptance rather than claiming a
new empirical force-field validation.

Fitting receives only training/validation role files inside LPAC. The frozen model
must precede calibration and test source access in receipts. A malicious or
mistaken request to import withheld labels into fitting must be rejected by the
study coordinator. Inspect retained source as well as receipts; timestamps alone
do not prove non-leakage. Rerunning inference from retained model arrays must match
within 1e-10 Z. A second checker independently recomputes all test/baseline errors,
coverage, OOD decisions, and break-even arithmetic.

The **useful-acceleration hypothesis** passes only when all are true:

1. Combined held-out RMSE <=0.05 Z, each test group's RMSE <=0.07 Z, and maximum
   absolute error <=0.15 Z. Also report errors in bar and seed uncertainty; these
   normalized thresholds are application requirements, not literature constants.
2. Combined RMSE is at most 0.8 times the best of P0, training-mean, and
   nearest-condition baselines. Report fitted linear/quadratic/kernel comparisons
   even if a simple fitted model wins. Improvement smaller than three times the
   RMS held-out seed standard error in Z units is inconclusive, not a demonstrated
   win: compare `best_baseline_RMSE - selected_RMSE` to
   `3*sqrt(mean((SE_pressure/P0)^2))` over the eight held-out conditions.
3. At least seven of eight test labels are inside the frozen interval, including
   at least three of four in each test group; q<=0.15 Z. At least six of eight
   test conditions pass the distance/support policy. Report small denominators;
   passing this check does not prove population coverage.
4. Warm inference, with all validation/OOD logic, is at least 10 times faster than
   one measured 20 ps solver run for an admitted query. Report single-query and
   batch timings, cold model load, model fitting/selection, environment startup,
   dataset generation, calibration/test runs, and fallback costs separately.
5. For a declared 1,000-query reuse workload with 20% direct-solver fallback,
   total measured setup plus amortized inference/fallback cost is below the same
   1,000 queries using direct three-seed endpoints. Use actual per-condition
   solver timing distributions and show break-even query count. The reference
   cost is a three-seed endpoint, not a free label lookup. Charge all 99 study
   solver runs and all model-selection work to setup; do not exclude validation
   to inflate acceleration. This is a stated workload scenario, not a prediction
   of user demand.

Failure of any predictive criterion disables automatic substitution and records
the failed hypothesis. Do not add conditions, tune on test errors, change target
windows, or loosen thresholds to obtain a pass. A later scientifically motivated
study needs a new preregistration and an untouched test set. Operational defects
may be fixed with failed receipts preserved; rerun the affected integrity checks.

## Reusable integration and retained artifacts

Use existing `LabJob` solver children and `python_numpy` generated jobs. The
current service already enforces same-project, immutable source copies with
optional expected SHA-256; its import limits are 32 files, 64 MiB/file, and
128 MiB total. It permits only NumPy/stdlib inside LPAC, finite `result.json`,
relative registered output paths, and explicit time/storage/memory budgets.
See [SCIENTIFIC_ISOLATION.md](../SCIENTIFIC_ISOLATION.md). No host shell, generated
pip install, arbitrary host OpenMM import, network access, or pickle is needed.

The missing reusable piece is a bounded **solver-sweep and dataset instrument**:
an immutable list of adapter parameter sets, role/replicate/group metadata,
declared scalar-window reductions, child-job reservations, and hash-checked
completion/restart receipts. This should accept other supported solver outputs;
the argon protocol belongs to this study input, not a special UI button. A generic
ML descriptor identifies the target protocol, feature schema/units, selected
model arrays, applicable range, uncertainty method, and rejection/fallback policy.
Do not register an untested family of ML packages as executable capabilities.

Respect the 32-import limit by reducing at most 24 source measurements per
isolated reduction job plus one bundled manifest containing the 24 pinned per-run
records (25 imports total, not 24 additional manifest files). Produce role-separated
shards, then import only the appropriate small shards into fitting, calibration,
and evaluation. Source arrays remain available by original solver IDs for the
independent checker. Hash-provenance chains must retain the original scalar input
files, not only the final table's hash. Generated NumPy code may extend instruments
within this boundary; source, inputs, and resulting artifacts remain immutable.

Retain `study.json`, `split.json`, `runs.json`, numeric `dataset.npz`, role-specific
arrays, per-seed/block summaries, model-selection scores, `model.npz`,
`model-card.json`, calibration residuals, all per-case predictions and baseline
errors, uncertainty/fallback decisions, timing samples, failed attempts, and
independent checker receipts. Use numeric arrays with `allow_pickle=False`; model
metadata is finite JSON. Register source code and environment hashes. A plotting
instrument renders measured-versus-predicted pressure with error bars, condition
coverage, and OOD/fallback markers from these files. PNGs link exact plotted data
hashes; real molecular frames keep their existing observation provenance.
The generated runtime does not contain Pillow. A trusted, data-only plotting
worker may use the existing pinned science environment, or generated code can
write bounded SVG using the standard library. Do not run generated plotting code
unrestricted in the trusted worker environment merely to obtain image libraries.

Ordinary-chat acceptance must request this finite-window study, launch its durable
batch, navigate away during work, pause/recover partway through label generation,
and complete isolated fitting/evaluation. The AI must inspect an actual plotted
image and a corresponding numerical discrepancy or interval, distinguish ML
inference from solver results, and explain a rejected candidate without inventing
a success. Demonstrate the actual 180 K direct fallback. An independent reviewer
then chooses one additional supported condition/protocol change after model
freezing; it must execute fresh held-out computation or correctly route to the
solver through normal tools without application edits. Do not refit the model on
that challenge. Export source, datasets, splits, model, solver receipts, numerical
checks, and measured visual evidence in the normal artifact path.

The additional reviewer challenge is explicitly outside the fixed 99-run initial
evaluation budget; disclose its own solver/replicate count and cost before launch.
It provides further evidence or rejects the model without changing the frozen
initial test scores.

No ML fitting, solver sweep, predictive comparison, or installed acceptance has
been executed for this proposal yet. The first-lab evidence informs feasibility
only; it does not constitute this study's acceptance.
