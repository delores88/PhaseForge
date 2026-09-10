# GPU discovery and scheduling — 0.8.0

## Devices and executable capacity

At startup, wgpu enumerates accessible graphics adapters and records their names,
vendor/device IDs, device types, driver information, APIs and compute limits.
Automatic selection excludes CPU/software adapters. Configuration can request an
adapter index, vendor substring or name substring. Windows can use DX12/Vulkan;
Linux GPU execution depends on a working Vulkan loader and driver.

Initialized hardware is separate from solver eligibility. Windows PnP and Linux
accelerator-class inventory are reported separately for neural/AI accelerators.
There is no installed NPU numerical backend. Unavailable utilization and memory
counters remain unavailable; system RAM is not reported as GPU VRAM.

## What runs where

| Work | Current execution |
|---|---|
| Procedural scenes and molecular viewports | Three.js/WebGL rendering |
| Compatible ODE candidate scoring | Generated WGSL on the selected GPU, f32 |
| Winning ODE trajectory and per-step trajectory measurements | CPU f64 |
| Unsupported GPU ODE expressions/constraints/reducers | CPU f64 with route explanation |
| Classical particle integration | CPU f64; exact neighbor cells for finite cutoffs |
| Provider model inference | Configured remote OpenAI/Anthropic service |

The GPU lowerer currently supports at most 24 state variables, 32 constants and
500,000 integration steps per candidate. General CPU ODEs can use up to 128 states.
GPU scoring requires supported Euler/RK4 expressions and an objective; constraints
and per-step trajectory reducers require CPU scoring. The selected candidate is
replayed in CPU f64. Numerical records distinguish GPU f32, CPU f64 and mixed search
execution; a rendered animation alone does not establish GPU numerical work.

## Admission and dispatch

The scheduler retains interactive/research/background priorities, derives worker
defaults from CPU inventory and keeps a numerical slot around the active experiment.
Candidate batches can use multiple CPU workers or the selected GPU. Expensive CPU
work runs outside the async HTTP executor.

Host-memory estimates reserve half of available RAM and respect the requested cap.
Both solvers honor candidate batches and check headroom before each CPU wave.
Allocation never changes equations, initial values, integration grid, seed, physical
horizon or approved search budget. This remains cooperative allocation rather than
an operating-system memory sandbox.

GPU batches respect storage-buffer/workgroup limits and can contain fewer than 64
candidates. Timing can reduce an approved batch; allocation failures trigger at most
four smaller-batch retries before fallback. A readback exceeding 30 seconds disables
further numerical dispatches on that device until restart. Already-submitted GPU
kernels cannot be forcibly cancelled by the application.

## Durable recovery and task ownership

Completed generations save the population, best candidate, history and exact random
state. Standalone runs can recover after interruption under the original deadline,
with at most three scheduler restarts. A partial generation is replayed. Final
trajectory capture and falsification restart from the selected candidate; there is
no arbitrary integrator-step resume.

Timed research sessions have their own shared deadline and pause on app restart.
The scheduler resolves persisted task ownership before dispatch, so their simulations
cannot start while the parent task is paused. Resume is explicit. A cancelled task
simulation restarts as a new run of the same immutable experiment, with that fact
recorded in the session.

The desktop independently supervises its child engine with at most three restart
attempts. These process attempts are distinct from numerical recovery attempts.
Read [compute/recovery](COMPUTE_AND_RECOVERY.md) for retained checkpoint semantics,
resource errors and tests, and [sessions](research-sessions.md) for time controls.

## Current scale boundary

One local numerical runtime and one selected GPU are implemented. Multi-GPU pooling,
DGX/cluster scheduling, remote workers, NPU kernels and local LLM inference are not.
The [roadmap](ROADMAP.md) describes separate adapters and acceptance work for these
capabilities. Native package jobs for four OS/architecture targets are prepared;
only actual executed checks belong in the [validation record](RELEASE_VALIDATION.md).
