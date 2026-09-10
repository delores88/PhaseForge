# PhaseForge 0.8.0

PhaseForge is a local scientific research workbench: describe an experiment, build
its mathematical model and 3D scene, run supported calculations, inspect the evidence,
and decide what to test next. It combines a Rust runtime, an Electron desktop app,
a Next.js interface and Three.js procedural graphics. The project and its rendering
engine use MIT licenses.

**Development status:** Windows x64 is being built and tested on an RTX 4090 Laptop
GPU machine. Native package workflows are prepared for Windows x64/ARM64 and Linux
x64/ARM64; that does not establish that all four targets have passed acceptance.
No GitHub release has been published for this overhaul. See the
[validation record](docs/RELEASE_VALIDATION.md) for executed checks and remaining work.

## Use the workbench

1. Create a project in **Laboratory** and choose a model from the per-turn picker.
2. Describe the question and measurements needed. **Build experiment** saves a
   validated setup; **Build & run** also authorizes one bounded numerical run.
3. Orbit, pan, zoom and inspect the scene. Open **Results** for measurements,
   constraints and falsification checks, or **Setup** for the exact model.
4. Repeat the setup, refine its time step, or extend its horizon without a model
   call. A changed experiment receives an immutable revision.
5. Open **Agents** for a timed research session with specialist collaborators,
   experiment building, simulation, evidence review and bounded follow-up cycles.

The model picker orders account-visible OpenAI and Anthropic text models from fast
to most capable, recommends stronger models for difficult simulation design, and
offers supported reasoning controls. Changes apply to the next request; session
settings can change on resume. Recommendations are maintained heuristics, not
benchmark guarantees. Provider API access and charges are separate from the app.
OpenAI is used for live development checks in this environment.

**Run saved setup** and the equation editor also work without AI. Existing notes,
sources, molecular imports, batch campaigns, verification dossiers and exports
remain available through the laboratory's additional tools. Older Discovery links
redirect into the workbench.

## Procedural scenes and scientific evidence

Agents can author molecular atoms and bonds, protein ribbons, DNA helices, membranes,
planets and rings, curves, surfaces, supplied streamlines and indexed meshes.
The viewport supports object inspection, camera presets, clipping, image capture,
quality controls and scene/camera export. Scene snapshots belong to their immutable
runs. See [scientific scenes](docs/SCIENTIFIC_SCENES.md).

Rendering and numerical execution are separate. Geometry may illustrate a hypothesis
or use supplied structural coordinates; moving geometry can bind to retained solver
entities. A membrane, molecule or accretion disk does not itself execute molecular
dynamics, drug binding, fluid dynamics or general relativity. Scene provenance and
numerical results identify what was supplied, conceptual or calculated.

The executable solvers are currently:

- **ODE systems:** CPU f64 Euler/RK4 integration with up to 128 states, seeded search,
  observables, constraints and trajectory measurements. Compatible small systems
  can score search candidates using GPU f32, followed by CPU f64 replay.
- **Classical particles:** one to three dimensions and up to 2,048 particles per
  candidate, with authored radial forces, boundaries and external acceleration.
  Finite-range interactions use exact spatial neighbor cells on CPU.

Declared exploratory assumptions are supported when the researcher permits them.
Missing empirical inputs and unavailable solvers remain explicit. A visualization
or completed computational session does not establish clinical efficacy or physical
validity. Molecular intake, diagnostics and QM/MM planning are preserved; external
engine discovery does not constitute an executing docking, MD or quantum adapter.
Read the [scientific scope](docs/SCIENTIFIC_SCOPE.md).

## Long work and recovery

Research sessions support one minute to seven days, bounded cycle counts and up to
three specialists sharing a clock and usage limits. Artifacts, experiments, runs
and reviews are saved locally. Pause preserves completed stages; cancel stops the
session. App restarts pause sessions before their owned simulations can dispatch.
Explicit resume continues the saved workflow.

The runtime samples CPU/RAM, initializes eligible GPUs, checks memory admission
and bounds numerical batches. Standalone searches save completed-generation
checkpoints, including random-generator state, and recover under the original
deadline. Resource failures can reduce batches or fall back to CPU without changing
equations, seeds or the scientific horizon. The desktop makes up to three attempts
to restart its owned engine after an unexpected exit.

These are bounded recovery mechanisms, not OS memory isolation. Interrupted
generations and final replay can restart; arbitrary integrator-step resume is not
implemented. NPU inventory does not imply an available NPU solver. Multi-GPU farms,
DGX/cluster orchestration and local open-source LLM inference remain roadmap work,
not part of the current execution architecture. See
[sessions](docs/research-sessions.md) and [compute/recovery](docs/COMPUTE_AND_RECOVERY.md).

## Run or package locally

The desktop bundles the exported interface and native Rust engine; it does not need
a separate browser or production Next.js server. When a tray icon is available,
closing the window keeps research running; **Quit PhaseForge** exits the app and
its owned engine. Projects remain in local SQLite and provider keys stay in the
operating-system credential store.

Use the [Windows guide](README.windows.md) or [Linux guide](README.linux.md) for
source builds, local packaging, development mode and prerequisites. Packaging
commands explicitly disable publication. Do not run two backends against the same
data directory. Back up existing data before testing a new build; reinstalling the
app is not a request to erase research data or credentials.

The legacy root PowerShell text helpers and shell scripts remain source-development
utilities. They are not the desktop installer workflow. Native build tools are
needed to build from source, not to launch a packaged application.

## Further documentation

- [Architecture](docs/ARCHITECTURE.md), [roadmap](docs/ROADMAP.md),
  [API](docs/API.md), [security](docs/SECURITY.md), [usage](docs/USAGE_AND_COST.md).
- [Direct experiment workflow](docs/EXPERIMENT_WORKFLOW.md),
  [findings](docs/FINDINGS_WORKFLOW.md), [research programmes](docs/RESEARCH_PROGRAMMES.md).
- [Discovery campaigns](docs/DISCOVERY_CAMPAIGNS.md),
  [verification methods](docs/VERIFICATION_METHODS.md),
  [verification dossiers](docs/VERIFICATION_DOSSIERS.md), [exports](docs/RESEARCH_EXPORT.md).
- [Marketplace template](docs/DELORES_MARKETPLACE_TEMPLATE.md): preparation only;
  no marketplace publication is part of this update.

Historical validation records remain under [docs/history](docs/history/). Their
results are not counted as fresh executions for 0.8.0.
