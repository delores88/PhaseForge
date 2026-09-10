# Numerical compute and recovery

PhaseForge runs immutable experiment manifests. Compute adaptation changes allocation,
not equations, initial conditions, integration grids, physical horizons, candidate
counts or random seeds.

## Local allocation

- CPU cores and available host RAM are sampled from the operating system. Admission
  reserves half of currently available RAM and respects the experiment's memory cap.
  This is a conservative allocation estimate, not an OS memory sandbox.
- Both ODE and particle candidate searches honor bounded batches. Host-memory headroom
  is checked before each CPU batch. One numerical run owns the numerical slot at a
  time; other research tasks can remain active while their simulations queue.
- Compatible ODE candidate scoring uses a generated WGSL compute program on the
  selected hardware adapter through wgpu. Integration replay and trajectory
  measurements use CPU f64. Final results identify CPU, GPU f32, or mixed scoring.
- GPU batches respect storage-buffer and workgroup limits, including allocations
  smaller than 64 candidates. Dispatch timings may reduce a batch; they never enlarge
  an approved allocation. GPU staging allocation failure is fallible, and GPU buffer
  allocation failures trigger up to four smaller-batch retries before CPU fallback.
- A 30-second GPU readback watchdog disables further numerical dispatches on a stalled
  device until the backend restarts. A submitted driver kernel cannot be forcibly
  cancelled by this application; CPU cancellation is cooperative as well.
- The shader cache retains at most 32 compiled programs. GPU VRAM is not inferred from
  system memory. Telemetry displays only available driver counters.
- Windows PnP and Linux accelerator-class inventories are exposed separately under
  compute capacity. No NPU numerical backend is currently installed, so detected
  neural accelerators are not counted as executable simulation capacity.

## Search checkpoints

Each completed search generation commits a checkpoint into the local SQLite objects
table. It records the immutable manifest hash, population, best candidate, history,
exact ChaCha random-generator position and differential-evolution target population.
Checkpoint format version 1 rejects mismatched manifests and candidate allocations.

On process restart, standalone queued/running simulations are recovered automatically
up to three times. Simulations owned by interrupted or paused research tasks are
stopped before dispatch; resume the owning task to continue its budgeted workflow.
The existing deadline remains fixed and includes time while the backend
is stopped. Cancelled, completed and failed simulations are not automatically
restarted. A run that has already exhausted its original deadline stays stopped.

Typed memory-pressure failures retry with smaller CPU batches and the same completed
generation checkpoint. Numerical divergence, invalid equations, and arbitrary errors
are not classified as resource pressure. Recovery history is retained in the result's
`numerical.recovery` object; progress events include `run_recovering`.

A partially evaluated generation restarts. Final integration replay and falsification
restart from the selected candidate. There is no claim of arbitrary integrator-step
resume. Completed result persistence precedes checkpoint cleanup. Failed/cancelled
checkpoints remain available for diagnosis; they are not silently attached to a new
experiment with a different run ID.

## Particle acceleration

Finite-cutoff particle interactions with at least 64 particles use spatial neighbor
cells. Open and periodic domains retain the same exact cutoff, pair ordering, and
equal-and-opposite forces as the original all-pairs calculation. Small populations,
infinite-range forces and unrepresentable cell coordinates use all pairs. This is CPU
numerical work; graphics rendering can independently use a GPU. The existing solver
limit is 2,048 particles per candidate. It is not a molecular force-field engine or a
general-purpose fluid, relativistic, or quantum solver.

## Verification

Automated tests compare resumed ODE and particle searches to uninterrupted runs,
including smaller batch allocation, random-generator state and DE selection targets.
They reject checkpoint provenance mismatch and expired/repeated recovery. Particle
neighbor tests compare exact accelerations to all pairs in open and periodic domains
and check momentum symmetry and final-frame retention.

The ignored hardware smoke test `gpu_one_candidate_batches_match_analytic_decay`
must be explicitly run on a machine with a hardware GPU. It evaluates several
single-candidate GPU batches against analytic exponential decay and checks cancellation.

Distributed scheduling, remote cluster adapters, NPU numerical kernels, exact
integrator-state checkpoints, and process isolation for these in-process numerical
kernels remain future work.

## Separate Studio workers

Blender rendering and CAD/PCB exports use separate supervised processes and queues.
Their time limits include queueing, and interrupted jobs retain their inputs and
files for an explicit new job. Blender can retry a recoverable GPU render failure
once on CPU within the original deadline. This is separate from generation
checkpointing; native jobs do not share the numerical solver's checkpoint format
or claim partial render continuation. Windows native workers enforce process-tree
memory budgets, but cannot reserve GPU VRAM against other applications. See
[render resource controls](BLENDER_RENDERING.md) and [CAD/PCB engines](CAD_PCB_ENGINES.md).
