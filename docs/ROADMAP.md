# PhaseForge roadmap after 0.8.0

The aim is one workbench for designing, executing and challenging scientific
experiments across scales. New visual representations, solver capabilities and
hardware backends are separate deliverables: progress in one does not establish
the others.

## Implemented in 0.8.0

- Electron desktop packaging around the native Rust engine and static Next.js UI.
  Windows x64 is the current local acceptance target.
- A Three.js procedural scene layer with molecular, biological, orbital, surface,
  curve and mesh primitives; provenance, numerical entity bindings and inspection.
- Default-off public research mode, bounded literature/structure/image retrieval,
  and trusted-host data/GLB/STL intake with original bytes and source hashes.
- Studio scene/CAD/PCB designs with source bindings and immutable revision lineage,
  separate from numerical proposals. Interactive molecular and GLB/STL inspection.
- Optional local Blender Cycles rendering, CadQuery/OpenCascade STEP/STL generation
  and KiCad board/DRC/export workers. The native engines are separately installed;
  current execution checks cover Windows x64, not every desktop package target.
- Per-turn provider/model/reasoning selection and timed, durable research sessions
  with 1–3 collaborating specialists, bounded experiment cycles and evidence review.
- Enforced numerical batches, host-memory admission, exact finite-cutoff particle
  neighbor lookup, GPU allocation retries and completed-generation checkpoints.
- Coordinated pause/restart behavior: standalone numerical recovery preserves its
  deadline; interrupted research sessions remain paused until explicitly resumed.

Existing research programmes, molecular intake, discovery campaigns, verification
dossiers, source records, notebooks and publication exports are retained. Their
scientific limits still apply. See [architecture](ARCHITECTURE.md),
[scenes](SCIENTIFIC_SCENES.md), [sessions](research-sessions.md) and
[compute/recovery](COMPUTE_AND_RECOVERY.md). Studio details are in
[public research](public-research-and-studio.md), [rendering](BLENDER_RENDERING.md)
and [CAD/PCB engines](CAD_PCB_ENGINES.md).

## Acceptance before wider distribution

Native package jobs are prepared for Windows x64, Windows ARM64, Linux x64 and Linux
ARM64. The three targets beyond the current Windows x64 machine have not completed
acceptance for this overhaul. Run their real compiler, packaging, install/uninstall,
credential-store, graphics-driver and application-lifecycle checks. CI definitions
alone do not establish a supported installer.

Validate optional-engine distributions and dependencies independently on each
target, including Cycles devices, CAD native-library compatibility and KiCad's
Python bindings. They are not supplied by the desktop packaging jobs.

Exercise long sessions, large retained scenes, interruption during provider polling,
OS/driver memory pressure, device loss and user cancellation. Expand accessibility,
keyboard navigation, different display scales and Linux tray testing. Record actual
results in [release validation](RELEASE_VALIDATION.md), including limitations.
No public release or marketplace publication is implied by local packaging.

## Deeper numerical and domain execution

The current numerical solvers are explicit ODE and classical particle solvers. Priorities for
real extensions include stiff and event-aware integration, symplectic methods,
uncertainty-aware experiment design, dimensional validation and independently
reproducible adapters. PDE/fluid, quantum and relativistic calculation require
dedicated implementations and benchmarks; an existing scene primitive is not an
adapter for those equations.

Biomolecular preparation, docking, molecular dynamics, free-energy and QM/MM need
real engine workers. Preserve structural provenance, protonation, charge, solvent
and force-field choices. Validate energy/force consistency, convergence and replay
against documented scientific controls. OpenMM, GROMACS, CP2K, xTB and LAMMPS
inventory does not execute these workflows.

Inference about a drug or biological mechanism needs appropriate computational
methods and laboratory evidence. Neither a procedural virus scene nor a simplified
binding model establishes a cure, toxicity profile or clinical benefit.

## More capable local compute

Add measured throughput calibration and device-memory admission for each actual
solver backend, finer checkpoint boundaries for integrators, and process isolation
for the in-process numerical kernels. Native Studio jobs already use separate
supervised processes, with Windows process-tree memory limits. Extend allocation to multiple GPUs only
when each kernel exposes its precision, memory and communication requirements.
Retain CPU paths and responsive desktop operation on smaller machines.

NPU devices can currently be inventoried, but no NPU numerical kernels are installed.
Support must follow a concrete supported workload and verified runtime; do not
count device names or advertised throughput as usable simulation capacity.

## Laboratory and cluster orchestration

Future remote workers should use an authenticated protocol with resource discovery,
reservations, shared artifact transport, cancellation, reconnect/retry semantics and
durable provenance. Separate inference capacity from numerical workers and avoid
duplicate execution when the coordinator or network fails.

DGX installations and other GPU farms are a roadmap use case. The current engine
does not distribute simulations across machines, manage a cluster queue, or pool
all available accelerators. Multi-user collaboration also requires access control,
identity and audit mechanisms beyond today's local single-user application.

## Local language models

Open-source LLM inference is deliberately deferred. The current product uses
configured OpenAI/Anthropic APIs; no model downloader, GPU inference server or
cluster inference pool is part of its architecture. A later adapter should preserve
per-call model attribution, context and cancellation contracts while reserving
inference memory separately from scientific compute.

## Research quality and publication

Extend uncertainty-calibrated surrogates, experimental-design methods, stochastic
replicates and explicit multiple-testing procedures. Add domain-specific equivalence
and symmetry classification, licensed reference corpora and expert-reviewed
benchmarks. Feature distance is not proof of novelty; an AI review is advisory.

Keep the existing independent ODE verifier and frozen source hashes reproducible
when adding new adapters. Publication integrations should preserve source and
method provenance and require the researcher's action to submit or upload.
The [Delores marketplace template](DELORES_MARKETPLACE_TEMPLATE.md) is preparation
for a later integration; no upload or release is part of this overhaul.
