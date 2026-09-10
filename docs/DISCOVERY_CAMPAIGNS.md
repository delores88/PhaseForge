# Discovery studies: from a working run to a testable research program

## Start from evidence, not a canned problem

Keep the existing research world. Open **Discovery** in the single sidebar, or
use **Build a discovery campaign from this run** in Findings. Select the authored
manifest revision you intend to extend. Study-generated trial revisions remain
in the Laboratory lineage but are filtered from this source selector.

**AI study design** is an optional paid call. It requests a complete revised
manifest and suggested ranges, observables and checks from the selected source.
It does not run the proposed experiment. Without a key, use **New campaign** to
edit the protocol yourself; numerical execution never requires an LLM.

The author must review: which parameters can change, meaningful physical units,
plausible parameter bounds, what metric should improve, what trivial solutions
must be excluded, and what would count as a numerical failure. Automatic initial
bounds are suggestions only. The first selected metric may be an inappropriate
objective; review both the metric and minimize/maximize direction.

## Algorithms and their actual roles

**Latin hypercube** samples every planned stratum in every declared parameter
once. Each dimension uses a seeded permutation and independent jitter. This gives
broad initial coverage; it is not a guarantee of uniform coverage of combinations,
a posterior distribution, or a significance test.

**MAP-Elites-style illumination** first performs bounded warm-up sampling, then
selects parents uniformly among occupied behavioral cells. It applies bounded
parameter mutations, with occasional broad exploration. Users define one or two
observable descriptor axes and bin boundaries. The first objective determines
which feasible candidate occupies a cell; other declared objectives contribute
to the displayed, descriptive non-dominated frontier. This is not NSGA-II.

Out-of-range descriptor observations remain in the trial table but are not
clamped into edge cells. Occupied-cell coverage is coverage of the author's grid,
not coverage of all phenomena or prior literature. Changing descriptor semantics
requires a new study; it does not rewrite old evidence.

**Differential evolution** is a new option in ordinary manifest search. It is
DE/rand/1/bin with F=0.7, CR=0.9, stratified initialization, bounded trials and parent-
trial selection before sorting. The native execution allocation must have at
least four candidates. This supports the existing ODE and pairwise-particle
scoring paths; supported ODE GPU evaluation is retained. Discovery campaigns use
the separate sequential/f64 path so each candidate retains complete evidence.

## Frozen protocol and bounded autonomy

Creating the study saves a draft containing the original immutable manifest,
recipe, seed, parameter ranges, metric/descriptor definitions, absolute and
relative tolerances and SHA-256 recipe hash. This is a local pre-execution record;
it is not externally witnessed preregistration.

Only **Approve & start budget** authorizes execution. Limits are 4–512 exploratory
trials, 0–6 finalists, a 10–86,400-second campaign wall allocation, and a
1–3,600-second per-trial allocation. These caps are deliberately finite.
Numerical workers are cooperative, not OS hard resource-limit containers.

A finalist receives h/2 and h/4 trials in addition to its original h run.
Common parameters and the same seed are maintained. Compare all declared ranking
metrics using `absolute_tolerance + relative_tolerance * max(abs(h), abs(h/2),
abs(h/4))`. Missing/ineligible runs are inconclusive; disagreement remains visible.
No convergence order is inferred from that rule. Replication over stochastic
seeds, stability, physical validity and independent engine checks are separate.

Pause/end cancels the active campaign trial. Completed evidence is retained;
resume does not reset elapsed budget. Cancelling an individual run in the Runs
page marks that trial unsuccessful and may allow the campaign to continue.
Use campaign Pause/End or stop the backend to stop the whole sequence. A backend
restart leaves studies paused, never automatically running or spending tokens.
An interrupted trial is reconciled on explicit resume; cancelled attempts remain
in the finite attempted-trial count rather than being hidden and retried forever.
The service is durable, but does not claim distributed exactly-once execution.

## Interpret, challenge, and continue

The results page distinguishes feasibility, behavioral coverage, ranking,
non-dominated candidates, and finalist numerical agreement. All are about the
specified numerical model. None is a scientific novelty certificate.

An explicit AI review is metered through the existing provider/usage service.
It receives a bounded report with strong candidates, failed examples, numerical
checks and saved bibliography notes. At most three review/proposal attempts per
study are admitted by this page; each can use existing bounded repair calls.
The ordinary chat remains available for additional explicitly requested work.
An off-by-default auto-review option permits one such review when compute finishes.

**Propose next experiment** requests a revised manifest for a human-reviewed
follow-up. No model proposal launches another campaign automatically. The author
chooses the next direction, reviews the suggested recipe, then approves its budget.

## Signal inspection

Click a trial or occupied cell for its recorded metrics and sample diagnostics.
The optional frequency hint uses at most 1,024 evenly strided saved samples and
128 frequency bins, only when timestamps are uniform and variance is nonzero.
Decimation can alias. Turning points and lag correlation are descriptive, not
Lyapunov exponents or tests of chaos. The renderer and diagnostics use retained
samples; they do not silently recover unsaved high-resolution trajectories.

## 0.5.0: behavioral sparsity and independent evidence

`novelty_search` is now a third campaign strategy. It measures five-neighbor sparsity
in frozen descriptor scales, keeps a 30% broad-search fraction after warm-up and
selects a quality-control finalist plus separated behavioral candidates. It is not
novelty against published science. Rejected trials stay in the ledger.

Use **Verify & compare** after pausing/completing the study. The frozen verification
dossier and independent process are separate from same-engine h/2/h/4 refinement.
Read `VERIFICATION_DOSSIERS.md` before interpreting its checks.
