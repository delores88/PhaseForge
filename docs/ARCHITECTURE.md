# Architecture — 0.8.0

## Design objective

PhaseForge separates four concerns:

1. **Research intent:** persistent questions, conversations, attachments, structures, and plans.
2. **Trusted execution:** validated declarative manifests, bounded numerical kernels, resource policies, persistence, and evidence.
3. **Representation:** bounded procedural scenes, supplied structures and camera/inspection context, with provenance and optional bindings to numerical entities.
4. **Scientific integrations:** normalized molecular records and explicit adapters or plans for external engines.

An AI provider proposes changes. It does not receive arbitrary shell, filesystem, process, credential, or host-code authority.

## Durable research objects

SQLite stores research projects, immutable manifest revisions, conversation messages, attachment records, run records, molecular structures, QM/MM-region plans, computational campaigns, durable research sessions and numerical search checkpoints. Provider secrets are stored separately through the operating-system credential service.

## Provider workflow

Credential storage and model selection are independent. The backend stores a key first, queries the live provider model endpoint, returns model metadata, and stores the chosen model ID separately. A provider is chat-ready only when both pieces exist.

Per-turn provider, model and supported reasoning overrides are recorded with the
request. Sessions retain these settings until an explicit resume changes them.
Model ranking is an account-catalog heuristic, not a measured performance claim.
Calls retain metering, cancellation and bounded repair. Long OpenAI requests can
use background-response polling rather than reissuing a slow generation. See
[session/provider details](research-sessions.md).

## Manifest boundary

Provider-authored experiments are complete `ExperimentManifestDraft` documents. The Rust sandbox validates model kind, expressions, search targets, numerical limits, visualization mapping, constraints, and falsification before persistence or execution.

The optional `visualization.scene` passes a bounded scene schema. Agents author
data, not arbitrary shaders, JavaScript or engine plugins. Each run saves its scene
snapshot with the numerical output. Conceptual surface detail remains distinct from
integration; a membrane or black-hole visual does not install MD or relativistic physics.

## Compute path

The scheduler uses CPU or an eligible `wgpu` adapter according to kernel compatibility,
manifest policy and inventory. ODE scoring can use GPU f32; final replay and trajectory
measurements use CPU f64. Classical particles use CPU and exact neighbor cells for
finite-cutoff interactions. One numerical run owns the slot at a time; its candidate
batches can use multiple workers.

Admission reserves host-memory headroom. Resource retries reduce allocation without
changing the scientific model. Generation checkpoints persist population, history
and RNG state. Standalone interrupted runs can recover within their original deadline;
task-owned runs stop before startup dispatch because their parent returns paused.
Resuming a cancelled task simulation starts from its declared initial conditions.
This is not arbitrary integrator-step checkpointing or OS memory isolation.

NPU inventory is separate from initialized numerical backends. No NPU kernels,
distributed scheduler or local open-source LLM inference service is installed.
See [compute/recovery](COMPUTE_AND_RECOVERY.md) and [GPU scheduling](GPU_AND_SCHEDULING.md).

## Molecular path

The backend parses supported text structure formats into one normalized record. Deterministic diagnostics, the browser viewport, the agent context, QM/MM-region planner, and campaign planner consume that same record.

External engines are discovered, not impersonated. Campaign stages remain blocked when the required engine is unavailable.

## Desktop and browser architecture

Electron packages the static Next.js export and native Rust executable. The renderer
has Node integration disabled, context isolation enabled and sandboxing enabled.
A local server serves the UI at `127.0.0.1:7332` and proxies API/WebSocket requests to
the engine at `127.0.0.1:7331`. The stable UI origin preserves interface preferences.
A production Next.js server is unnecessary. Browser development remains on port 3000.

The desktop uses a single-instance lock, checks engine versions and makes at most
three restart attempts after its engine exits unexpectedly. With a tray icon,
closing the window preserves background work; Quit exits the app and owned engine.
The renderer cannot directly read credentials or launch processes.

Windows x64 is being tested locally. Native package jobs exist for Windows and Linux,
each on x64 and ARM64. Prepared jobs are not completed acceptance. Packaging commands
do not publish releases; consult the platform READMEs and validation record.

## Durable timed research sessions

`agent::tasks` coordinates Scientific designer, Skeptical reviewer and Simulation
architect briefs, a validated builder call, numerical execution and evidence review.
Sessions share a deadline across 1–3 specialists and bounded cycles. Calls run
concurrently only within the usage allowance. Specialists consume supplied context;
they do not receive arbitrary browsing or shell tools.

Stages, artifacts, selected models, manifests and runs are persisted. Pause stops
provider work and owned simulations; cancel is terminal. Restarts pause sessions
instead of silently reissuing potentially billed calls. Solver submission uses the
task-state gate and actual time remaining after model work. Saved builder receipts
are reconciled across interruption boundaries. Build-only sessions can review a
matching manual run on resume without rebuilding. See [sessions](research-sessions.md).

## Findings and user-controlled continuation (0.3.3)

Each new numerical result carries evidence_version=2. Constraint adjudication
and challenge comparisons are independent of the optimizer score. A deterministic
findings report is assembled from the selected run and its immutable manifest,
not from whichever revision happens to be active in chat. Old score-only survival
flags are treated as inconclusive.

An explicit explanation request (or the user's default-off opt-in for newly
completed runs they start) makes one metered model call. Explanations are cached
by evidence hash and shown in Findings and chat. They cannot overwrite numeric
verdicts or submit simulations. Suggested continuations pass source_run_id and
force auto_run=false; the proposed revision then waits for review and approval.
A local replay requires its own confirmation and does not call an AI provider.

Read-only telemetry has separate CPU/RAM and GPU sampling tasks. A small workflow
endpoint exposes active phases and run progress without repeatedly downloading
trajectories. The UI never invents provider completion percentages or device
utilization from detected hardware. Displays are separate from the scheduler's
admission/recovery controls and are not evidence of distributed execution.

The viewer captures explicit entity mappings, preserves identities between
frames, and uses simulation timestamps for playback. Bounded display sampling,
position interpolation, coordinate recentering, and velocity-arrow scaling do
not modify retained scientific data.

## Discovery and publication layer (0.4.0)

A `DiscoveryService` above the existing scheduler owns frozen study recipes, sequential
CPU/f64 trial admission, resumable budgeted campaigns, candidate archives, and
resolution challenges. Each trial materializes an immutable ordinary manifest and
run; it does not bypass the existing validator or create a second solver. Source
manifest snapshots are retained without being overwritten. Manifest revision
numbers are now reserved atomically across interactive and campaign authors.

Study execution makes no LLM calls per candidate. Explicit study design, review
and next-proposal actions use the existing metered agent interface and require
existing provider settings. Optional post-campaign review is off by default.

Versioned per-world notebooks link authored claims to completed supporting and
contradicting runs. Updates use revision conflict detection and one transaction for
the current record plus immutable revision. Research exports package the exact
study, inputs, outcomes, references, evidence-derived figures, and independently
implemented ODE replay code. No submission is performed.

This remains a local single-user application, not a signed provenance authority or
distributed laboratory. See `DISCOVERY_CAMPAIGNS.md` and `RESEARCH_EXPORT.md` for
limits, scientific caveats, storage and workflow semantics.

## Incremental 0.5.0 modules

`assurance::{types,comparison,service,api}` provides frozen verification objects,
reference matching, approved independent-process scheduling, auditable review and
selective signal inspection. Additive SQLite object kinds retain snapshots.
`tools/verification_worker.py` is the static independent ODE implementation;
`frontend/.../assurance/VerificationLab.jsx` is within advanced laboratory tools, not a new
navigation shell. `AgentService::review_verification` is a bounded read-only
call path through the existing token ledger, separate from proposal execution.
