# Installed mechanics ordinary-chat preregistration

Status: protocol only, frozen before the installed study on 2026-09-11.
This document launches no solver, model, build or native test. The coordinating
task must authorize the installed checkpoint before execution. The numerical
component already passed its [23-group acceptance](mechanics-lab-results.md);
this installed study does not replace or repeat that entire suite.

The [original proposal](mechanics-lab-proposal.md) and independent checker remain
the scientific authority. Their SHA-256 values are respectively
`afa5299a2c6bf720d305ff45650d0fa18ec43e185276b9e995ed3b9bd0cd09af` and
`e8af97912a93e1bbd00336c6e857546e93e6701cd95801f106c821b023ac56d9`.
The accepted worker is
`455056ac9bdf12cba80a9b86699970cf92939432e1177841b936838913c1d125`.
Capture the installed build and worker hashes before starting; a source change
requires its own preserved review and acceptance decision.

## Scope, names and operational budget

Use the generic `newtonian_nbody` adapter through ordinary installed chat and
the existing LabJob pipeline. Do not add an orbit preset or benchmark-specific
solver behavior. Inputs explicitly use isolated G*=1, L0/M0/T0, where
`T0=sqrt(L0^3/(G_physical*M0))`. Positions are L0, velocities L0/T0,
accelerations L0/T0², energy M0 L0²/T0² and angular momentum M0 L0²/T0.
There is no implicit nm/ps or SI conversion and no periodic box.

Stable study case names are `installed-binary-control`,
`installed-binary-speed-080`, and the later `installed-reviewer-variant`.
Record actual job IDs rather than assuming names identify immutable jobs.
Allow three distinct scientific inputs, sequential execution, and at most four
study solver process invocations: control, resumed control, intervention, and reviewer
variant. Cap each invocation at 300 wall seconds through the existing supervisor
or parent deadline; the aggregate numerical-process allowance is 1,200 seconds.
The surrounding chat and inspection wall times must be reported separately.
Existing installed readiness probes are recorded separately from these study
invocations and are not counted as an additional scientific acceptance case.
No passing case is automatically repeated. Preserve failures and stop for a
separately recorded continuation plan if these operational limits are insufficient.

Each completed case retains 1101 regular states plus any explicit off-grid
cancellation endpoint; an extra endpoint must not replace a regular state.
The worker's 512 MiB retained
output and 128 MiB working-array bounds, together with the backend's 512 MiB
worker process-memory supervision, remain in force. These are separate limits,
not a promised working set or hard filesystem quota. The accepted implementation
writes one-state immutable chunks and checkpoints at every retained state;
`chunk_frames=50` is an upper bound, not functional batching. The earlier small-N
profile was dominated by serialization. Do not shorten a run, reduce sampling,
change equations or introduce batching to make this checkpoint finish faster.

## Stage A: initial ordinary-chat request

Send the following as the initial user request in a fresh installed project,
after recording the selected model/effort and installed build identity:

> Run the first installed mechanics study with the supported generic interacting
> Newtonian-body solver. Compare the circular binary control with a 20% reduction
> of both initial tangential velocities. Use isolated G*=1 and explicitly label
> all scientific time as T0 and positions as L0; do not assign seconds, nm, or a
> periodic box. Use the exact parameters below, retaining step 0, every fourth
> full-step state and step 4400. Both bodies must respond to gravity.
>
> Name the cases `installed-binary-control` and `installed-binary-speed-080` and
> run them sequentially. Show actual progress while I navigate away and back.
> For the control, request a cooperative pause after observing a step from 400
> through 3600, inspect the retained intermediate checkpoint, then explicitly
> resume the same job/input from that checkpoint. Preserve the previous chunk
> hashes and verify continuation after resume. Do not count an already completed
> job as an interruption or substitute a fresh integration for checkpoint resume.
>
> Inspect actual retained images for both cases. Keep the same declared camera
> for the control and intervention. Cross-check positions, body identity, time,
> and instrument values against the exact retained states; screen motion is not
> a physical clock. Request and inspect an additional view of a specified saved
> state, recording its camera and source identity without rerunning integration.
>
> Use an isolated generated instrument, with saved source and source-file hashes,
> to compare the recorded closest pair distance and the first full relative-angle
> revolution. Preserve sample brackets and interpolation method. Compare both
> trajectories with an independent analytic reference and remeasure invariants;
> do not use the solver's own force or integrator as the reference.
>
> Explain which numerical results reject the hypothesis that the reduced-speed
> orbit stays circular at constant separation. Separate numerical measurements,
> analytic expectations, image observations, and computation/playback duration.
> Save a continuation ledger with inputs, units, model/build/source identities,
> job IDs, observations, instrument source, measured values, failures and limits.
> Stop after the two cases and their analysis. The independent reviewer will
> choose a fresh variant afterward; do not choose or execute that variant now.

Common parameters, copied into each solver input without inferred defaults:

```json
{
  "body_ids": ["body-0", "body-1"],
  "masses": [0.5, 0.5],
  "positions": [[-0.5, 0.0, 0.0], [0.5, 0.0, 0.0]],
  "timestep": 0.0015707963267948967,
  "steps": 4400,
  "sample_interval": 4,
  "chunk_frames": 50,
  "min_separation": 0.05,
  "boundary": "isolated",
  "unit_system": "scaled_G1"
}
```

Control velocities are `[[-0.0,-0.5,0.0],[-0.0,0.5,0.0]]`; intervention
velocities are `[[-0.0,-0.4,0.0],[-0.0,0.4,0.0]]`. Put the selected array in
`parameters.velocities` and use outer `engine: "newtonian_nbody"`.
Only the velocity intervention and case metadata differ. Record the exact
accepted input JSON and hash for each job before it advances.

The control pause must leave a checkpoint with `0 < step < 4400` and actual
worker exit 3. Same-input continuation must preserve all already published
chunk bytes, retain full-step velocities, and finish with no duplicated or
missing regular samples. A pause at a nonregular step may add that one recorded
state; scientific comparison selects the original step-multiple-of-four grid,
and the extra checkpoint sample is preserved and audited separately. The first
1001 regular states end at nominal 2*pi T0; all 1101 end at nominal 1.1*2*pi T0.
Use the actual saved `step*h` timestamps, including normal floating-point rounding.

## Frozen numerical and observation decisions

Apply the original criteria, without loosening thresholds or choosing favorable
windows after seeing results:

1. On the first 1001 regular samples, position and velocity RMS errors use the
   squared Euclidean error summed over bodies/times, divided by N*1001, then
   square-rooted. Circular fine errors must be below 1.25e-5 in L0 and L0/T0;
   eccentric fine errors must be below 2e-4 in those units. The prior three-grid
   factors 3.5–4.5 remain established by the separate frozen suite; do not claim
   this single-grid installed run measures a new convergence rate.
2. Across the period records, relative energy drift must be below 2e-4; momentum,
   angular-momentum and COM residual after initial uniform drift must each be
   below 1e-11 in the original scaled units. Independently measured instrument
   components must agree within 1e-10. No energy adjustment is allowed.
3. On the first 1001 regular samples, the eccentric recorded minimum must agree
   with 8/17 L0 within 5e-4 and be below 0.5; the circular pair distance must
   remain within 2e-4 of 1 L0. The first forward 2*pi relative-angle crossing
   uses saved bracketing samples and linear interpolation. The eccentric/control
   period ratio must agree with `(25/34)^(3/2)` within 0.1% relative error.
   Do not relabel a sampled minimum or interpolated event as an exact observation.
4. The reference Kepler solve must have residual below 1e-13. The original
   force fixture, finite-difference test, invalid-input cases and full recovery
   comparison remain separate component evidence; no fresh pass is claimed for
   them merely because these installed jobs complete.
5. Validate original 1024-square orthographic images at the five regular
   milestones (steps 0, 1100, 2200, 3300 and 4400), exact saved-frame/image
   hashes, IDs, units, timestamps, and
   matching instrument snapshots. Independently projected marker centers must
   differ by at most one pixel. The extra view must reference the same selected
   numerical state with an explicitly retained camera, not a new solver run.
6. Refusal, a guard activation, missing source pins, failed numerical checks,
   stale image reuse, or failure to perform actual installed pause/resume leaves
   the relevant gate unpassed. Scientific failure cannot be relabeled as an
   operational timeout. Preserve the report and source evidence before changes.

## Stage B: independent reviewer chooses a fresh variant

Only after reviewing Stage A's actual evidence does the independent reviewer
choose a new generic two-body binary or three-body Lagrange homographic input.
The developer/model that executed Stage A must not preselect the variant here.
Choose fresh positive masses summing to 1, orientation, COM translation and/or
small COM velocity. Keep initial pair separation 1, and choose tangential speed
multiplier s from the original values 0.8 or 1. Keep
the same h/4400 steps/interval 4/minimum-separation guard, and scaled units.
This retains the original reference families and their applicable thresholds while
testing new supplied arrays. Three-body masses may be unequal.

Before any variant execution, save a reviewer-authored addendum containing:

- Exact body IDs, mass/position/velocity arrays, full input JSON and SHA-256;
  the selected rotation Q, initial COM c and constant COM velocity u.
- Independent analytic reference values and initial H, P, J and COM, with units;
  the reference implementation/source hash and expected sample/time sequence.
- The original fine circular or eccentric RMS, invariant, minimum-distance, event and image
  limits above, plus declared plane/basis/pair for angle unwrapping and camera.
- Pure-reference checks of the supplied initial state and numerical guards;
  no reference may be produced by running the worker. Freeze the addendum hash
  before submitting `installed-reviewer-variant` through ordinary chat.

For a binary, form barycentric planar positions
`r0=(-m1,0,0)`, `r1=(m0,0,0)`. For a triangle, form equilateral vertices
`b_k=(cos(2*pi*k/3),sin(2*pi*k/3),0)/sqrt(3)` and subtract their mass-weighted
COM: `r_k=b_k-sum_j(m_j*b_j)`. Then choose an orthogonal proper rotation Q and
set `q_i(0)=Q*r_i+c`, `v_i(0)=s*Q*J*r_i+u`, where `J(x,y,0)=(-y,x,0)`.
Record actual decimal arrays and their reference residuals before execution.

For either body family at s=0.8, `a=25/34`, `e=0.36`, pair minimum `8/17`, period
`2*pi*(25/34)^(3/2) = 3.9616080528290403 T0`, and the control-period ratio
is `0.6305095042004002`. At s=1, `a=1`, `e=0`, pair separation remains 1,
period is 2*pi T0, and the control-period ratio is 1. Use the original circular
fine RMS limit 1.25e-5 and separation limit 2e-4 for s=1, or the eccentric
fine RMS limit 2e-4 and minimum-distance limit 5e-4 for s=0.8. The 0.1% relative
period-ratio limit and the invariant/image limits remain unchanged.
With `E-e*sin(E)=pi+t/a^(3/2)`, use
`X=a*(e-cos(E))`, `Y=-a*sqrt(1-e²)*sin(E)` and their analytic time derivatives.
Reference states are `q_i(t)=Q*(X*r_i+Y*J*r_i)+c+t*u` and
`v_i(t)=Q*(Xdot*r_i+Ydot*J*r_i)+u`. These formulas respond to the reviewer's
actual masses and transform; they are not benchmark-specific engine logic.

Equivalently, using actual submitted COM-relative positions and velocities,
let `r'_i=q_i(0)-c` and `w'_i=(v_i(0)-u)/s`. Then the same reference is
`q_i(t)=X*r'_i+Y*w'_i+c+t*u`, `v_i(t)=Xdot*r'_i+Ydot*w'_i+u`.
Validate the common plane and right-angle tangents before execution.

For the pre-execution invariant values, let `I=sum_i m_i*|r_i|²` and
`n=Q*(0,0,1)`. Then `H0=(s²/2-1)*I+|u|²/2`, `P0=u`,
`J0=s*I*n+c cross u`, and `COM(t)=c+t*u`. Avoid a choice with vanishing
H0, for which the existing relative-energy criterion would be undefined.
Verify body visibility under the fixed declared camera before execution without
using computed worker output to tune the test. Keep camera identity fixed within
this variant; its declared transform may differ from the Stage A camera.

The reviewer's follow-up ordinary-chat request must supply the frozen addendum,
ask the installed tools to execute its exact generic arrays and independent
instrument, inspect a new retained image, and compare the results with that
predeclared reference. The variant may also compare its measured period against
the Stage A circular period because total mass and initial separation are still
1. Do not submit it until the addendum exists, and do not select another variant
after a failure without preserving that failure and a new preregistration.

## Completion evidence and limits

Capture the installed model/effort, build and worker/runtime hashes, project and
parent/child IDs, ordinary tool calls, progress across navigation, pause/resume
receipts, exact inputs, stored arrays/indexes, measurements, generated source,
observations and source pins, independent reports and final continuation ledger.
Keep scientific T0, process wall seconds and playback seconds separate.

Stage A completion alone does not pass the fresh reviewer stage. Only actual
installed ordinary-chat execution plus the independent artifact checks can close
this installed checkpoint. The 23-group standalone pass remains its own evidence.
No general three-body closed-form solution, long-term chaotic forecast, collision
physics, empirical astrophysical validation or broader release completion follows.

## Read-only checker and pause evidence

[check_installed_mechanics.py](../../tools/check_installed_mechanics.py) never
launches a solver or provider. `prepare --output <fresh-directory>` freezes the
baseline input JSONs, this protocol, the original reference/proposal, worker and
checker source hashes. Its `verify` command requires a plan, a JSON run mapping
and a fresh report directory. The actual running checker must match the frozen
copy. The default scope is `installed`; `--scope component-fixture` is reserved
for re-reading existing component outputs and never establishes installed success.

The baseline mapping has keys `installed-binary-control` and
`installed-binary-speed-080`. Each contains absolute `directory`, `job_snapshot`
and `session_snapshot` paths; the control also requires `pause_receipt`. Solver
creation must follow plan creation, and both cases must share their recorded
project/session and exact parameters. The checker validates all retained arrays,
instruments, invariants and image provenance; RMS and closest-separation criteria
use the original 1001 regular samples through step 4000. Period measurement uses
the full step-4400 run. Individual period errors are reported descriptively; the
unchanged acceptance gate is the measured perturbed/control ratio.

The current supervisor discards the exit status observed during cooperative
cancellation. Therefore a `paused` API receipt alone cannot establish exit 3.
The acceptance observer will open a read-only Windows process handle to the
`solver_started` PID while that process runs, request pause in the declared step
window, and record `GetExitCodeProcess` from the same retained handle after exit.
This adds no scientific process, does not change OS security settings, and is
recorded separately from the model's ordinary-chat tool use.

After the worker has stopped, run `snapshot-paused` with `--plan`, `--case`,
`--directory`, `--job-snapshot`, `--process-receipt` and a fresh `--output`.
The native process receipt records `job_id`, `pid`, `exit_code`,
`observed_running_step`, `opened_unix_s`, `pause_requested_unix_s`, `exited_unix_s`
and method `Windows GetExitCodeProcess on original retained process handle`.
The snapshot verifies the canceled state and archives its numerical checkpoint,
raw indexes/instruments, stopped-job/process receipts, logs and immutable prefix
hashes before resume. The final audit checks those prefix bytes still match.
Same-runtime bit equality to an independently executed uninterrupted copy remains
the component suite's evidence; this installed study does not invent that rerun.

When the independent reviewer later selects the variant, its exact specification
contains `parameters`, `speed`, `selection_provenance`, and `control_evidence` with
the completed installed baseline `acceptance_report` path. Use `prepare
--reviewer-spec <file> --output <fresh-directory>` before submitting the variant.
This pins the original control's accepted period, project and complete source-file
inventory. The final reviewer audit checks the original control remains unchanged
and compares the new measured period against that real control with the original
0.1% ratio tolerance. No reviewer parameters are selected by this preparation tool.

### Initial-state hash zero-sign amendment

Before any replacement installed numerical attempt, the coordinating task
authorized [a separate amendment](../../.local/science/installed-mechanics/initial-zero-sign-amendment.json)
to the initial-state hash comparison only. The model may spell a planned `-0.0`
as `0.0`; both represent the same numerical initial condition but hash differently
as JSON. The checker now requires identical IDs, shapes, finite values and exact
numeric array equality against the frozen inputs, then uses the actual retained
source arrays' zero signs to check the manifest's initial-state hash. It reports
every changed zero sign. Raw source/input/chunk hashes remain exact. Nonzero
magnitudes, timestep, equations, sample counts and every scientific error threshold
are unchanged. The v2 plan and failed fourth-checkpoint evidence remain preserved;
a new plan snapshot records the amended checker before further execution.
