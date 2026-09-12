# Optional numerical-relativity execution and resource planning

The user revised release acceptance to dependable scientific software: real
bounded execution, usable results, clean stopped outcomes and actionable next
options. A validated 0.999c collision is not a release gate and remains unsolved
here. The work described here is an execution and validation foundation. A
harmonic gauge wave is flat spacetime in changing coordinates;
it is neither a black-hole collision nor physical gravitational radiation.

## App resource planning

The native agent's former `lab_catalog` implementation returned a static catalog
despite advertising hardware information. It now shares a live catalog with the
API and records a durable `resource_plan` event. The plan samples host physical
and logical CPU counts, current available/total RAM, actual workspace free space,
and timestamped device-wide GPU/VRAM telemetry. Stale or unknown counters remain
unknown. GPU aliases are not summed, and a graphics API does not establish
compatibility with a particular scientific solver.

`phaseforge.resource-plan.v1` separates observations, proposed budgets, actual
reservations and solver feasibility. It leaves at least half available RAM and
a 2 GiB reserve, two logical threads where possible, and 10 GiB workspace reserve.
The exact inherited UTC deadline or Off is retained. A measured pilot must name
its source/configuration receipts; it does not automatically produce an ETA or
certify another grid. Nine focused Rust planning checks passed.

The optional NR dispatcher acquires both laboratory solver slots, recomputes the
same live plan, obeys a hold decision, then caps the small benchmark at four CPU
cores, 4 GiB RAM and 64 MiB output. These slots serialize laboratory solvers;
they are not OS-wide reservations against other applications. Linux independently
checks its own available memory and scratch capacity before execution.

## Owned worker lifetime

The optional trusted registration pins the exact AthenaK manifest and source/build
identity. `nr_stage.py` creates one private immutable UUID attempt and verifies the
input hash; it cannot accept a generated shell command. `nr_supervisor.py` executes
only the pinned engine with the retained parameter file. Sealed executable/input
descriptors prevent verified bytes changing underneath execution.

The Linux supervisor owns a process group, a parent-loss guardian and an explicit
control pipe. Pause, cancel, deadline, stdin loss and Windows-to-WSL transport loss
stop its workers. Affinity, per-process address space and per-file size are hard
limits. Aggregate RSS/output and the deadline are sampled guards with possible
overshoot. This is trusted-worker supervision, not arbitrary-code isolation or a
guarantee of checkpoint continuation.

Twelve real WSL lifecycle tests passed, including supervisor death and descendants.
A separate actual `wsl.exe` transport-loss check found no live supervisor, guardian,
engine or grandchild afterward. Fourteen staging tests verified identity, paths,
permissions, partial staging and immutable repeats without launching a solver.
Receipts are under `.local/validation/nr-supervisor-20260912` and
`.local/validation/nr-stage-tests-20260912-01` (`wsl-tests.log` and `report.json`).

## Actual app execution, before retention hardening

The isolated source app on port 7442 completed job
`c62783dd-9264-4e32-bab0-a3f78ebea1de` in project
`eecf12a6-be7e-4c17-b352-9c230d1d2f42` through its ordinary standalone job API.
There was no model call. The existing installed 0.9.1 profile was unchanged.

The exact 32×4×4 gauge input ran for about 0.249 seconds with timer Off, four allowed
CPU cores, 3,327,131,648 bytes of RAM admission and 67,108,864 bytes of output
admission. Twelve native files were copied and independently hash/size checked.
An identical API request returned the same completed record without rerunning.
The original analytic checker passed on these actual copied native files.

Evidence: `.local/validation/workbench-010/nr-api-acceptance.json` and
`nr-api-independent-check.json`. This first attempt predates exact raw admission
retention and the in-app postprocessor; it must not be retroactively represented
as validation of those later changes.

## Actual integration and recovery verification

The retention path now saves exact staged request and manifest bytes, imports
immutable hash-checked output, and provides explicit recovery after a stopped
attempt without restarting its solver. Reparse/link/path protections and atomic
file creation prevent replacing previous retained files. Recovery must preserve
failed numerical checks; process exit zero alone cannot complete validated science.

The trusted gauge postprocessor reconstructs the physical metric and extrinsic
curvature from retained fields, invokes the unchanged analytic checker, and saves
exact XY cell-centre slices with source binary/block hashes and dimensionless
units L=1, c=1. Its seven tests pass. Actual 32-grid data pass; the original 16-grid
Kxx failure remains a numerical failure under the same threshold. No additional
solver or rendered interpolation is involved. These are diagnostic slices of 3D
evolution, not a visual embedding of curved spacetime.

The stricter source app then ran job `027406c3-355b-4e75-b060-d83d5bbffe05`
through its ordinary job API with timer Off. It retained three exact native times
0, 0.0546875 and 0.1; the trusted analytic postprocessor passed. The 16-grid job
`293a446d-f4b9-42ab-820b-f6acee1f8f2d` retained times 0, 0.0625 and 0.1 and
correctly remained failed under the unchanged numerical tolerance. Both cases
recovered their files twice without execution, changed input/deadline, duplicate
recovery events or changed completion time. All 22 diagnostic artifact pins per
case were reread. Supplying a new timer to recovery was rejected with HTTP 422.
The first harness expected 400; its failed evidence remains separate from the
corrected continuation, which reused the completed 32-grid job without rerunning.
Evidence: `.local/validation/workbench-010/nr-api-integrated-02-resumed.json`.
This is direct API evidence; normal model routing and NR viewer integration have
not yet passed complete user-workflow acceptance.

The optional runtime now also has a separately qualified v2 physical-RAM boundary.
Each job receives its own systemd scope, with an exact verified memory maximum,
zero swap, task cap and owned-child membership before the launch gate opens.
The supervisor and guardian remain outside that scope. Six real WSL checks
covered a 16 GiB untouched virtual mapping under 128 MiB charged RAM, one actual
OOM, escaped descendants, parent loss and survival of an independent sibling.
The original 12 CPU lifecycle checks also passed. When systemd removes an empty
scope, unavailable final counters stay unknown; earlier samples keep timestamps.
The OOM case retained the exact unit's `oom-kill` result and complete drainage.
Evidence: `.local/validation/nr-cgroup-20260912-01/report.json`. CUDA execution
remains unadmitted until its separate VRAM policy and runtime are verified.

The actual low-momentum black-hole pilot, including its incomplete deadline stop,
constraint growth and unsuccessful common-horizon searches, is recorded separately
in `relativistic-black-hole-pilot-plan-2026-09-12.md`. Neither the gauge foundation
nor that pilot fulfills the requested collision. Local installation and marketplace
admission remain pending the user's scientific gate.
# Actual CUDA execution and backend agreement

Job `5ebd8820-43ef-4e91-8213-9d158a439809` completed through the isolated
application with the fixed candidate02 input SHA256
`66dff1103d4f521b601efc79e57c1e3ac353222f937e4d97dc07fbff922d07df`.
The original 15-minute deadline was `2026-09-12T16:39:36.354900500Z`;
the application completed at `16:34:21.045094900Z`. No solver was rerun.

| Measured fact | Result |
| --- | --- |
| Solver wall time | 469.792443084 seconds |
| Saved paired native times / cycles | 0 / 0; 0.5126953125 / 35; 1 / 69 |
| Peak sampled host-process RSS | 446,844,928 bytes |
| Peak sampled device-wide GPU memory growth | 2,651,848,704 bytes; not per-process attribution |
| GPU observations | 442 |
| Native files / bytes | 33 / 499,251,217 |
| Live numerical monitor | Three complete pairs accepted; no accuracy claim |
| Process / owned RAM scope | Exit 0; completed; drained |
| Independently verified artifact inventories | 33 native and 124 derived files |

All CPU/CUDA native-field and derived-constraint comparisons passed at the
two exact shared times, 0 and 0.5126953125. The criteria and comparator source
were frozen before any GPU data. The comparison ran with the original retained
reduction helper, even though a later additive time-label clarification was
already prepared in source. The final CUDA state at time 1 has no CPU counterpart;
full recorded-trajectory agreement remains false. This is computational agreement,
not convergence, physical accuracy, boost calibration or a validated merger.

The original API harness **failed**: one GET timed out after 30 seconds during
the large native-data import. The solver and application nevertheless completed.
A separate read-only receipt verified the same completed job and all artifact
pins; it does not turn the original workflow failure into a pass. Moving large
retention/recovery/intent verification to blocking workers and measuring API
responsiveness remain required follow-up work. The exact 0.999c release gate
remains open; neither installation nor publication is authorized by this result.

Evidence:
- `.local/validation/workbench-010/nr-cuda-api-pilot-01.json` — original failed harness.
- `.local/validation/workbench-010/nr-cuda-api-pilot-01-completion.json` — same-job read-only completion evidence.
- `.local/validation/nr-backend-comparison-20260912/actual-cuda-01/comparison.json`, SHA256 `46f89a1b4471ab7862995c9b02053f2eb8d019944684eaff2880fcf58533e6a2`.

## Earlier CUDA runtime and live-observation source checkpoint

The CUDA build completed with pinned AthenaK/Kokkos/TwoPunctures and a private
CUDA 12.9 runtime. Its executable has not yet been run at this checkpoint.
The fixed candidate02 input remains byte-identical to the original CPU pilot;
neither coordinate velocity nor Bowen–York momentum is reported as physical speed.

The application now budgets 4 GiB for this solver plus a separate 512 MiB for
the read-only Windows monitor. CUDA execution uses an owned systemd cgroup v2
RAM boundary, no inherited finite address-space limit, an exact physical GPU
UUID and sampled device-wide growth/reserve guards. These GPU observations are
not a hard VRAM quota or per-process kernel attribution. The monitor checks
complete native metric/constraint pairs for finite values, positive physical
metric/proper volume and the original 100× RMS-growth stopping condition. It
becomes ready before the solver starts; cancellation is rechecked after readiness.
Failure stops the solver through its existing control pipe. After solver exit,
a bounded 20-second monitor-only final scan does not extend numerical evolution.

Both retained CPU states passed actual local and WSL-share monitor replays,
with approximately 81 MB peak memory; the cold WSL replay took 8.44 seconds.
The focused backend integration checks passed 49 tests, after preserving and
correcting a rejected-source empty-directory regression and an incorrectly
admitted optional-engine test fixture. Six NR frontend data checks and its
production build passed. Actual CUDA execution and browser interaction remain
separate required acceptance steps. The exact 0.999c collision release gate
is unchanged and unfulfilled.

Evidence: `.local/validation/nr-backend-integration-20260912-01/`,
`.local/validation/nr-cuda-20260912/continuation-02/`,
`.local/validation/nr-live-guard-20260912/`, and
`.local/validation/nr-cuda-supervisor-20260912-03/`.
