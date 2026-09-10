# Findings and continuation

The deterministic report is generated from a completed run and **its own** immutable
manifest, never whichever revision is currently selected for editing. It contains
source IDs, expression definitions/units, criteria, measured results, validation
gaps and a hash of the evidence used. Historical runs remain readable.

Reports use `checks_passed`, `checks_failed` and `inconclusive`; these concern declared
numerical checks only. A failed check means that declaration failed, not that all
physics in the question is false. New challenge data carry `evidence_version: 2`.
Legacy objective-score flags are displayed as inconclusive without rewriting DB data.

## Optional AI layer

A user requests an explanation or explicitly enables automatic explanations for
newly completed runs started in this browser session. The backend serializes
explanations, caches one per run/evidence hash, uses the existing usage admission
and cancellation gate, and limits output to min(user output cap, 3000 tokens).
There is no repair loop for prose explanations. Raw trajectories and bulky trial
arrays are omitted from provider context; included metric summaries stay tagged.
The analyst is instructed not to invent missing values or upgrade an unknown check.
The resulting prose is clearly advisory and appears in Findings and persistent chat.

The next-step workflow is not controlled by model-written executable actions. A
user chooses a bounded action card, sees the source revision and cost boundary,
edits the research direction and approves. A proposal request includes source_run_id
and auto_run=false. The backend binds the context to that run's source manifest.
The resulting new manifest waits in Experiment for a separate run approval.

## Scientific comparison rules

For a baseline b and trial value v, allowed = absolute_tolerance +
relative_tolerance * abs(b). Stable: abs(v-b) <= allowed. Change: abs(v-b) > allowed.
Increase: v-b > allowed. Decrease: b-v > allowed. Every repetition must meet every
check. Min/median/max are summaries; the median cannot hide a failing repetition.
Baseline near zero suppresses relative-change display and relies on explicit
absolute differences. Missing measurements or truncated challenged horizons cannot
pass. Rule thresholds require scientific justification, not a default 25%.

ODE signed perturbations alternate +m,-m,+2m,-2m,... and are recorded per trial.
Particle initial perturbations use increasing recorded noise scales on the same
base seed. They are not a sample from independent random trials or a statistical
confidence interval. Seed-replication semantics remain engine-specific.

A single h/2 comparison is agreement evidence, not measured convergence order.
The next-step refinement prompt asks for a bounded h,h/2,h/4 plan. The program does
not invent an observed order, Lyapunov exponent, stability proof or literature novelty.

## Legacy migration

An existing run can be read immediately. Local replay captures the new evidence
shape and visual mappings without a provider call. Replay does NOT silently add
pass rules to old challenges. A proposed validation revision is the next explicit
step when acceptance rules were absent.
