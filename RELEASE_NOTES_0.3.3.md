# PhaseForge 0.3.3 — From a completed run to the next defensible experiment

Complete incremental source release from 0.3.2; Rust/Next/React/three dependencies and
credential/database locations are retained. No named scientific presets were added.

## New workflow
Completed runs open **Findings**, not a wall of variable deltas. A deterministic
brief separates measured observables, declared constraint outcomes, unknowns,
novelty-not-assessed, and next-step choices. **Explain findings with AI** makes one
metered provider request, saves it against the immutable run/evidence hash and adds
it to chat. Optional automatic explanation is OFF initially, limited to newly
completed runs started in this browser session, and does not retry failed paid
calls. Cached explanations do not incur another model call.

Next-step cards offer a local replay or a proposal for stronger validation,
numerical refinement, or bounded exploration. Proposals use the selected run's
immutable source manifest, have auto-run OFF, and require a separate run approval.
The original revision and evidence remain available. Reports and run/manifest
bundles can be exported; compatible metrics can be compared across runs.

## Corrected scientific evidence
Challenge results now contain baseline values, trial measurements, maximum absolute
change, individual constraints, and explicit expectation/tolerance rules. ALL
replicates must meet a declared rule. Missing rules, missing metrics, truncated
horizons and old score-only challenge flags are inconclusive. Near-zero baselines
never receive a fabricated relative-percent comparison. Legacy histories are read,
not rewritten or silently upgraded. A replay captures new measurements; a revision
is needed to author missing scientific acceptance criteria.

## Visualization
Stable OrbitControls camera, fit and XY view, time-based playback, frame scrubbing,
speeds, entity picking, measured trails, optional velocity vectors and PNG export.
Interpolation affects display only; it does not modify results. Scene resources
are cleaned up, updates reuse instances, and rendering samples are bounded.
Generic ODE manifests support explicit visual entities and velocities. Compatible
old x/y coordinate-family mappings can capture multiple entities on replay. A
one-trace legacy result remains one trace until rerun; omitted z is not a velocity.

## Progress and resources
Read-only CPU/system RAM/backend process metrics with short history, NVIDIA driver
GPU/VRAM counters, Windows WDDM fallback and Linux AMD sysfs fallback. Unknowns stay
unavailable, not zero. Device counters are not per-experiment attribution. GPU
sampling runs separately from CPU/RAM so slow driver utilities cannot freeze them.
Workflow stages show actual provider/validation/simulation/interpretation status.
Only the numerical work plan gets a percentage; no invented overall ETA. Individual
and all-agent cancellation, pause and usage gates remain.

## Validation boundary
See docs/RELEASE_VALIDATION.md for checks actually executed. This packaging
environment has no Rust/Cargo, Windows/MSVC or downloaded Next.js dependency tree.
Native compilation, native regression tests and the full production build require
the included installer or CI. They are NOT claimed as passed here.
