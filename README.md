# PhaseForge 0.10.0

PhaseForge is a local Windows scientific workbench. Describe a study in ordinary
chat, run a supported numerical experiment, inspect its saved measurements and
rendered states, and ask the AI to compare the evidence with a hypothesis.
Projects, conversations, job history and numerical artifacts stay on your machine.

The current release target is **Windows x64**. The desktop includes the interface,
Rust backend and pinned Python scientific runtimes. AI requests use your selected
OpenAI or Anthropic API account; provider usage is billed separately. The project
uses the MIT license, with dependency notices in [THIRD_PARTY.md](THIRD_PARTY.md).

## Start a study

1. Open a project and choose the provider, model and supported reasoning effort.
   Set an explicit timer, or choose **Off** for a finite task without a wall-clock
   deadline. Off does not remove API, memory or storage limits.
2. Ask a question in chat. When you want computation, specify the experiment,
   parameters, measurements and comparison. An explanation-only request should
   remain a discussion; a picture or animation alone is not a numerical result.
3. Follow the agent/tool activity and individual jobs. Inspect the saved inputs,
   progress, measurements and artifacts, then play back retained states in the
   laboratory viewer. Ask for an additional camera or numerical instrument when
   it would help test the interpretation.
4. Pause work when needed. Compatible solver checkpoints can resume; generated
   computations use a new immutable attempt. Previous results and failed attempts
   remain available. Source or runtime changes can require a fresh run.

The agent can inspect actual images rendered from saved numerical states and
compare them with measurements. Visual observations supplement the numerical
record; higher image resolution does not improve an experiment's physical model.

## Implemented scientific workflows

| Workflow | Current scope |
|---|---|
| Molecular dynamics | Real OpenMM dynamics for a periodic argon-like Lennard-Jones fluid, with temperature/density controls, energy, pressure, RDF and MSD measurements. |
| Spatial diffusion | A two-dimensional periodic scalar field, retained numerical arrays, point/region instruments and fixed-scale field playback. |
| Heat conduction | Constant-property periodic 2D heat conduction in SI units, retained temperature and heat-flux fields, energy measurements and compatible checkpoints. |
| Incompressible flow | Unforced periodic 2D viscous Newtonian flow, with velocity, vorticity, pressure, divergence, energy and enstrophy measurements. No arbitrary walls, compressible shocks or multiphase flow. |
| Classical mechanics | Interacting Newtonian point masses in an isolated domain, with explicit scaled units, trajectories and conservation instruments. |
| Generated instruments | Python standard library and NumPy calculations inside the Windows LPAC boundary, with project-scoped inputs and retained outputs. Network access is unavailable inside this boundary. |
| Batched studies and ML | Durable solver sweeps and a scoped pressure-surrogate study with frozen data roles, held-out evaluation, uncertainty and direct-solver fallback. Useful acceleration must be demonstrated, not assumed. |

The bundled runtimes contain CPython 3.13.15, NumPy 2.4.6, OpenMM 8.5.2 and
Pillow 12.3.0, with SQLite 3.53.4 and OpenSSL 3.0.22 replacements from pinned
official archives; the generated-code runtime contains Python and NumPy only. They are
verified and copied locally without finding a host Python or downloading packages
at experiment startup. Generated code cannot add packages to the bundled runtime.

These are specific numerical models, not a general-purpose biological laboratory.
The molecular adapter does not simulate HIV infection, drug binding or treatment
efficacy. A saved molecular structure or an attractive render does not establish
that those processes were calculated or that a wet-lab experiment can be replaced.
See [the molecular model and its limits](docs/validation/molecular-lab.md) and
the study evidence below.

## Long work, saved state and rendering

Jobs and solver artifacts have durable identities independent of the model's
context window. Context compaction retains a working summary and references to
saved evidence; the application can read the original artifacts again. A model's
summary is not a substitute for an array, checkpoint or execution receipt.
After an interruption, unfinished work is available for explicit continuation.
Actual solver time, model usage and rendering time are separate costs.

The interactive viewer uses retained trajectories or fields. **Blender is an
optional, separately installed renderer** for additional views and exports;
installing it does not add a scientific solver. Standalone illustrations remain
labeled separately from numerical studies. Existing Studio, molecular imports,
public research and CAD/PCB tools remain available; see
[rendering](docs/BLENDER_RENDERING.md),
[public research and Studio](docs/public-research-and-studio.md), and
[optional CAD/PCB engines](docs/CAD_PCB_ENGINES.md).

Completed outputs are reachable from Results and the horizontal experiment
history. Image and numerical result controls expose each retained output.
Explain with AI, Suggest next steps and Review numerical checks use the current
conversation's model and work limit, read the selected job's pinned evidence,
and reply in chat. These review actions cannot start or modify an experiment.
Visible camera controls and presentation changes reuse saved states; a new
camera angle or higher export resolution does not rerun the scientific solver.

The capability catalog distinguishes executable models from missing domains.
No black-hole merger solver is integrated in this release. A separately built
AthenaK reference benchmark establishes a numerical foundation, but does not
fulfill the requested head-on collision at 0.999c per black hole. See the
[relativistic solver investigation](docs/validation/relativistic-solver-path-2026-09-12.md).

## Current evidence

The evidence ledger distinguishes component checks, observed installed studies
and the acceptance of an exact release artifact:

- [Molecular lab](docs/validation/molecular-lab.md),
  [installed reviewer variant](docs/validation/reviewer-variant.md), and
  [extended installed study](docs/validation/installed-extended-research.md): real
  numerical execution, retained trajectories and documented observation/recovery checks.
- [Installed diffusion study](docs/validation/installed-diffusion-lab.md): completed
  controlled comparison checked against an independent reference, with failed
  operational attempts preserved.
- [Mechanics component results](docs/validation/mechanics-lab-results.md): the
  independent component study and installed two-run numerical comparison passed,
  including checkpoint continuation and rendered-position checks. See the
  [installed mechanics evidence](docs/validation/installed-mechanics-results.md)
  for the completed checks and remaining scope.
- [Installed ML study](docs/validation/installed-ml-study.md): full installed
  99-run evaluation is deferred; implementation and pilot checks do not establish
  predictive usefulness or acceleration.
- [Bundled runtime checks](docs/validation/runtime-v2.md) and
  [Windows isolation](docs/SCIENTIFIC_ISOLATION.md): actual copied-runtime physics,
  NumPy execution, access-denial probes and service recovery checks.
- [OpenSSL runtime correction](docs/validation/runtime-v5.md): official fixed DLL
  provenance and actual copied-runtime TLS compatibility checks for 0.9.1.

These earlier checks do not certify a new installer automatically. Each 0.10.0
installer requires its own installation and marketplace acceptance records. See the
[0.10.0 workbench evidence](docs/validation/workbench-010-results-2026-09-12.md),
[Results checks](docs/validation/workspace-results-2026-09-12.md),
[continuum validation](docs/validation/continuum-results-2026-09-12.md), and
[release coordination](docs/validation/marketplace-release-handoff.md).

## Install or develop

Use the [Windows guide](README.windows.md) for the packaged app, building the
required runtime seeds, local packaging and development mode. Quit the existing
app before switching installations, and keep projects and credentials during an
upgrade. Native build tools are required only when building from source.

Further references: [architecture](docs/ARCHITECTURE.md),
[API](docs/API.md), [security](docs/SECURITY.md), and
[usage and cost](docs/USAGE_AND_COST.md). Older validation and platform guides are
retained in [release history](docs/history/), [Linux notes](README.linux.md) and
[macOS notes](README.macos.md); they are not release targets or acceptance evidence
for this Windows scientific package.
