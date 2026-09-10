# PhaseForge 0.6.0 — research programmes and trajectory-aware discovery

Complete incremental **source** release from the supplied **0.5.0** checkpoint.
The Rust/Axum/wgpu backend, Next.js/React frontend, API-key storage, model selection,
usage limits, stopping controls, live telemetry, light/dark themes, molecular
workbench, Discovery and independent verification are retained. No npm or Cargo
dependency versions were changed. Original release notes remain historical records.

## The new workflow

**Research plan → source/data review → informative experiment → Discovery portfolio →
independent verification → evidence-linked report.** Each numerical or paid action
still requires the existing user approval. Saving a programme executes no tasks.

Use the **Research plan** tab for an ambitious question that does not fit one simulator.
The model proposes ordered tasks with required data, dependencies, testable questions,
acceptance criteria and a concrete next action. A missing domain engine blocks that
stage, not all research. Retrieved citations must belong to this research world.

The new public search retrieves Europe PMC metadata and available abstract excerpts,
with explicit per-query consent, query records, failures and a response hash. It is
not an exhaustive or full-text review. Local UTF-8 CSV profiling records missingness,
finite numeric summaries and the submitted file-text hash; raw rows are not retained
or sent to the model. Keep your original file. Column names and aggregate statistics
can enter later metered research context; use deidentified data.

The chat **intent selector** distinguishes a research programme, discovery search,
benchmark/reproduction and automatic selection. Plan intent cannot silently turn into
an immediate simulation; discovery intent requires a real parameter/objective search
or a useful staged programme. Auto-run defaults off. Preparing a programme task proposes
work in chat without starting its simulation or completing dependencies.

## Better numerical questions, not just bigger animations

The native ODE runtime now supports **128 CPU states** plus **64 independent streaming
measurements**. Minima, maxima, range, time-weighted means, integrals, threshold residence,
hysteretic entries and censored first passages are computed at each integration step,
including search screening. They no longer consume spare ODE states. These names can
be used directly in ranking objectives and acceptance constraints.

Use **resolution_ladder** for distinct h/2 and h/4 replays against baseline h. Old
step_halving manifests and results keep their historical repeated-h/2 semantics.
Incomplete or failed challenges remain visible; they do not erase a completed baseline
or count as passes. Old findings flag repeated grids rather than inventing h/4 evidence.

Explicit visual entity **radius expressions** size real sphere geometry in the same
spatial coordinates. Selection changes color, not physical scale. Missing radius uses
the legacy glyph. There is no new collision impulse, merger or material simulation;
geometry and threshold proximity do not imply contact physics.

**Compute preflight** evaluates the actual solver's GPU compatibility and current host
RAM, limits candidate batches, preserves headroom, and records the admitted allocation.
It is checked again after a queued run obtains the numerical lane. It never changes
physical equations, horizon, timestep, approved candidate count or token/time budget.
These are structural estimates, not measured peak memory or an ETA. Per-step reducers
run on CPU/f64; the existing GPU/f32 lowerer remains limited to compatible small models.
A single trajectory is sequential; independent candidates provide parallel work.

The existing **Discovery** workspace already retains diverse candidates, Pareto trade-offs,
failed attempts and finalists. Use its explicit protocol approval rather than expecting
a single Run's optimizer to return a portfolio. New trajectory metrics appear in its
measurement selectors and independent comparison context.

## Install / upgrade on Windows

1. Stop the old backend and frontend, and back up the existing application data.
2. Extract this complete ZIP into a **fresh directory**, not a mixed-version overlay.
3. Open PowerShell in the extracted `PhaseForge` root and paste the **entire**
   `INSTALL_ALL.txt`. It contains both full installers and real build/test gates.
4. The optional independent verifier still uses `INSTALL_VERIFIER.txt`: Python 3.10+
   with no pip packages. It is separate from the Rust application runtime.
5. After installation succeeds, paste the entire `START_ALL.txt`.

The local database/keyring locations have not changed. Existing manifests,
runs, sources, credentials and dossiers are not replaced.
Do not run two source releases against the same database. Keep the old release and
backup until native build/installation checks pass. There is no automatic research
execution on upgrade. New measurements require an explicitly authored new revision;
old results are not retroactively populated with numbers that were never measured.

Frozen v0.5.0 verification worker/reference sources are retained and selected by exact
hash, so existing independent protocols keep their original numerical implementation.
A research export includes the correct worker pair next to each frozen verification
input. Unknown source versions are rejected, not silently substituted.

## Verification boundary

See `docs/RELEASE_VALIDATION.md` and the external package report for **executed** tests.
This environment has Python, Node and Chromium, but no Rust/Cargo or Windows tooling;
registry DNS is unavailable. Native compilation, actual Rust HTTP execution and the
full Next.js build are **not claimed to have passed here**. Supplied native tests and
Windows/Linux CI exercise those boundaries, and the TXT installers run real build gates.

No cure, physical theory, chaos result, clinical safety, exhaustive prior-art coverage
or scientific novelty is certified by a successful software test or numerical run.
Use `docs/RESEARCH_PROGRAMMES.md` for scientific and operational limits.

## Retained laboratory architecture


A local-first, open-source computational research lab: ask a question, review a
bounded experiment, simulate, inspect evidence, obtain an optional AI explanation,
and choose the next step. The underlying 0.3.3 laboratory is retained; this is not a replacement framework or patch collection.

## Start on Windows

Stop the old frontend and backend with Ctrl+C. Extract to a **fresh directory**.
Open PowerShell inside `PhaseForge/`, copy the **entire** contents of
`INSTALL_ALL.txt`, paste and run. After the backend build/tests and frontend build
succeed, paste the entire `START_ALL.txt` program. Open http://127.0.0.1:3000.

The root text files are self-contained PowerShell programs. Individual backend/
frontend install/start programs are also supplied. Existing database and OS key
storage paths are unchanged. Preserve any custom environment/configuration;
do not overwrite your database with demonstration data. The start script detects
an old version still occupying the backend port instead of quietly using it.

Requirements: Windows x64, Microsoft C++ Build Tools, a current stable MSVC Rust
toolchain, Git and Node >=20.9. The installers attempt supported dependency setup
using winget when needed. No Python environment is required for the existing core lab; the new independent verifier requires Python 3.10+.
Optional developer tests use Python + jsonschema. Linux scripts/notes are included.
Internet access is needed to resolve/install dependencies. Lockfiles are generated
on first installation in this source distribution; no precompiled backend is
included. Exact frontend dependency versions are retained from 0.3.3.

## Research flow

1. Save an OpenAI or Anthropic API key in Settings, load account-visible models,
   and separately save an active model. There is no forced question modal.
2. Create a research world and discuss a question in the resizable chat. Review
   the complete authored manifest in Experiment. Initial chat auto-run remains
   an explicit composer setting; follow-on cards always request proposals only.
3. Run the approved revision. Progress shows the actual stage; live machine
   resources show CPU, RAM, and available GPU/VRAM counters.
4. Completion opens Findings: the important observables, baseline constraints,
   challenge outcomes, unresolved checks, and next-step options.
5. Use **Explain findings with AI** for one usage-metered plain-language
   interpretation in the main pane and chat. The report is bound to that run's
   evidence hash. Enable the optional automatic-explanation checkbox only when
   you accept one additional paid call for each new run you start. It is OFF by
   default, limited to this browser session's newly started runs, and never loops.
6. Choose a next step. Review its purpose and cost boundary. A generated revision
   does not run until you approve it. A local replay reruns the original manifest
   without a model call. Your original evidence remains intact.

For existing 0.3.2 histories, select the completed run, open Findings, and choose
**Replay with complete evidence**. This captures proper metric comparisons and
new visual mappings. Challenges with no declared acceptance rules will still be
inconclusive; choose **Strengthen the validation plan** to propose those rules.

## What is actually executable

The generic Rust engine supports first-order state-vector ODEs (Euler/RK4),
classical pairwise particle dynamics, bounded random/evolutionary search,
observables, scalar constraints, challenge replays and immutable result records.
Compatible ODE search batches can use the existing wgpu GPU path; authoritative
replay and the pairwise runtime are CPU f64. GPU detection is not a promise that
every workload is GPU-executed. Every run records its execution backend.

Molecular PDB/SDF/MOL/XYZ intake, normalized structures, diagnostic heuristics,
region planning and engine readiness from 0.3.2 remain. Detection of OpenMM,
GROMACS, CP2K, xTB, LAMMPS or Open Babel does not execute those engines. This release
does not add a docking/MD/QM execution adapter or chemical synthesis protocol.

## Evidence rather than green badges

New result packages record actual constraint-expression values and tolerances.
Challenge checks specify metric, expectation (stable/change/decrease/increase),
and absolute/relative tolerances. All repetitions must pass every declared rule.
No rule, missing metric, incomplete horizon or legacy score-only result means
**inconclusive**. Optimization penalties are never used as a universal scientific
pass/fail rule. One half-step replay does not establish convergence order.

Numerical check success is not proof of a physical hypothesis, long-term stability,
chaos, biological efficacy or novelty. Novelty is explicitly **not assessed**.
AI text is advisory: it cannot change the deterministic numerical verdict.

## 3D and plots

The viewer uses measured entity IDs and simulation timestamps. It supports camera
orbit/pan/zoom, fit, 3D/XY modes, pause/frame stepping, time scrubbing, speed,
trails, velocity arrows, entity inspection, fullscreen and PNG export. It keeps
GPU buffers/camera stable while playing rather than rebuilding the scene on every
frame. Smooth playback interpolates only display positions. Markers and arrow
lengths are display-scaled, not physical radii or a change to velocity evidence.

Explicit `visualization.entities` mappings specify all desired ODE bodies/traces.
A conservative compatible coordinate-family fallback helps old manifests on replay.
The browser caps rendered entities/frames/sample budget; raw exported evidence is
not rewritten. Unrecorded bodies cannot be recovered from an old one-trace animation.
Findings/Evidence add retained-observable plots and compatible run comparisons.

## Resources, cancellation and cost

The expandable monitor distinguishes whole-machine CPU/RAM, backend-process RAM/
CPU and device-wide GPU statistics. It does not pretend system memory is an
experiment's private allocation. NVIDIA counters come from nvidia-smi where
available. Windows fallback uses WDDM counters and may lack reliable capacity;
Linux AMD fallback reads driver sysfs. Unsupported/stale fields remain unknown.
Telemetry is read-only and is not sent to an AI provider. It does not add automatic
resource throttling. The user can stop a request/run or use the existing usage and
budget controls. Remote tokens already processed may remain billable after Stop.

Workflow progress is stage-aware. Numerical percentages reflect evaluated work,
not predicted wall-clock completion. Model generation has no invented percentage.

## Repository and developer checks

- `backend/`: Rust API, numerical runtime, constraints/challenges, findings, usage,
  credential storage, read-only telemetry and persistence.
- `frontend/`: Next.js/React, three.js and the research workspace.
- `tests/`: schema/API/view-model tests plus a real local-backend smoke test.
- `.github/workflows/ci.yml`: Windows/Linux native builds/tests and frontend build.
- `docs/`: API, workflow, telemetry, validation and scientific boundaries.

From the root:

```
python -m pip install jsonschema
python tests/test_proposal_schema.py
node tests/frontend-api.mjs
node tests/lab-model.mjs
node scripts/release-audit.mjs
cd backend
cargo check --all-targets
cargo test --all-targets
cargo build
```

Then run `python tests/runtime_smoke.py --binary backend/target/debug/phaseforge-backend.exe`
from the root (omit `.exe` on Linux). It uses a temporary isolated DB and a local
port, makes no AI calls, and exits its spawned backend after checks. Its fixtures
are software tests only, not selectable scientific presets.

**Build status:** see `docs/RELEASE_VALIDATION.md`. Static checks are not a native
build. The distribution's installer/CI run the native gates not available in the
packaging environment. No paid engine license or cloud account is required for
local numerical execution; provider API usage is optional and user-billed.

License: MIT. Do not expose the unauthenticated development API to an untrusted
network. No shell or arbitrary host-code execution is granted to the AI agents.
