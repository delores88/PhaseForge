# Research programmes, measurements and compute — 0.6.0

## Using the programme pane

Create/select a research world and configure an OpenAI or Anthropic model using the existing
Settings flow. Select **Plan the research** in chat or **Design research programme** in the
Research plan tab. A plan is a saved proposal, not a hidden autonomous job. It separates an
ultimate goal from the next tractable question, describes inputs and methods, and records
success criteria. Task dependencies must refer to earlier tasks. A plan claiming to cure a
disease or prove physics without evidence is not a software-validated outcome.

Prepare a task in chat to request a metered next proposal. Inputs still missing remain visible;
preparing it neither runs an experiment nor marks prerequisites complete. Human progress notes
are append-only and attributed as human assessments, not automatic scientific verdicts. Use
Discovery's claim ledger for publication-grade links to computed evidence.

For sources, enter or prefill a focused public query, check consent, and explicitly retrieve.
Only a fixed Europe PMC endpoint is contacted; no provider key is sent. Up to15 metadata/abstract
records are retained. A source ID in a new plan must have been retrieved in this world. A valid
ID does not prove the model interpreted that source correctly. Abstracts may be truncated and
are not full-text reading. Empty and failed searches do not establish absence or novelty.

The newer default-off [research mode](public-research-and-studio.md) also retrieves
general scholarly metadata through Crossref. Its saved sources can appear beside
programme searches, identified by their source IDs and scope. The programme pane's
explicit Europe PMC request remains separate; it does not silently expand that
specific consent to other catalogs.

CSV input is limited to1MiB,20,000 rows,128 unique-header columns and bounded fields. It must be
UTF-8; BOM bytes are retained in the submission and checksum while ignored as a header marker.
Quoted fields/newlines are parsed. Ragged rows, malformed quotes and duplicate headers are
rejected. Missing markers: blank, NA, N/A, null and missing. NaN/Infinity are counted separately.
Finite numeric columns use running mean/sample standard deviation. Overflow is disclosed with
null summaries, not fake zeros. The saved record includes the exact submitted UTF-8 SHA-256,
provenance and aggregate stats, not raw records. Keep your original file. Deidentify data; column
names and summaries can be included in later metered model context. No causal, therapeutic,
safety or efficacy conclusion follows from these descriptive statistics.

## Numerical intent and representation

**Discovery** intent refuses a single-run/no-objective numerical proposal as a substitute for
an exploration. It may still produce a staged research programme when meaningful execution
needs inputs or another engine. **Benchmark** is appropriate for known-answer calibration.
This is generic workflow behavior: no hard-coded scientific question or initial condition was
added to the application. The included analytic fixtures test software only.

The new `trajectory` list contains instantaneous expressions and reduction rules. Unlike extra
ODE integrals, these do not consume dynamical state slots. They are measured at each fixed-step
endpoint even when display frames are thinned or capture is off during search. Names enter the
analysis context and may appear in objectives/constraints/ordinary observables. First-passage
`NAME_observed` is1 only when a threshold event was observed (including initial occupancy);
otherwise the elapsed horizon is right-censored. It is not a guaranteed survival duration.
Entry counts exclude initial occupancy and use hysteresis. Durations interpolate the scalar
linearly between samples. Multiple within-step events and extrema may be missed. Temporal
resolution, model validity and objective relevance remain scientific design decisions.

Use `resolution_ladder` with2 levels to preserve baseline h and independently replay h/2,h/4.
Up to4 levels are allowed within per-model step budgets. Historical step_halving continues to
repeat h/2, and old findings explain its limits. Creating a new revision is required to change
a historical experiment. A failed challenge is incomplete evidence; its baseline is retained.
Stopping still cancels work. Passing numerical comparisons does not establish convergence order.

Explicit visual radius expressions use model coordinates and generate true sphere sizes, not
extra point markers. A renderer does not enforce non-overlap, contact impulses, merging,
deformation or fragmentation. Bodies can pass through nominal envelopes unless governing
physics separately models an interaction. The current release adds no such contact solver.

## Compute and provenance

Preflight uses actual GPU lowering compatibility, not just adapter detection. Per-step
trajectory reductions and larger ODEs use CPU/f64; GPU screening remains f32 with supported
expressions/dimensions and f64 replay. The UI explains the route. CPU candidate waves honor
bounded batch width and policy; a single trajectory cannot be parallelized simply by choosing
more workers. RAM admission reserves half currently available host memory and obeys the declared
budget, then checks again after the numerical gate opens. Estimates include state/population
and capture overhead, but are not measured peak RSS, hard memory isolation or an ETA. Existing
wall-time, cancellation, key storage, paid-usage and public-query controls remain independent.

Existing Discovery retains diverse trials/Pareto candidates. Start from a measured manifest,
then define justified descriptor ranges, objective scales and bounded protocol approval.
Internal diversity is not novelty to science. Verification still needs controls, comparisons,
held-out perturbations, source review and independent expertise. Frozen v0.5.0 workers are kept
verbatim under `tools/legacy/0.5.0` and chosen by exact saved hashes. Unknown versions require
their matching release or a new dossier; previous failures/results are never overwritten.

Publication bundles include research plans (without task prompt text), source-query records,
CSV profiles and human task progress in addition to existing evidence. Original raw CSV data
must be archived by its owner. Review confidentiality, rights and interpretations before sharing.
