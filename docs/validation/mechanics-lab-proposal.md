# Preregistered mechanics lab: interacting Newtonian bodies

Status: proposed on 2026-09-11; no mechanics engine, catalog entry or acceptance
experiment has been implemented or executed under this proposal. Implementation
starts only after the installed diffusion gate passes and the coordinating task
advances the sequence. This proposal addresses sections 1/2 and minimum study 1
in `docs/SCIENTIFIC_WORKBENCH_CODEX_PROMPT.md`; it does not complete that assignment.

## Question, model and units

How does reducing orbital tangential speed by 20% change closest separation and
orbital period while preserving energy, momentum and angular momentum?
Use interacting point masses in three-dimensional Euclidean space, with isolated
boundaries, constant masses and the unsoftened Newtonian inverse-square law:
`a_i = sum(j != i) G*m_j*(q_j-q_i)/|q_j-q_i|^3`.
The Hamiltonian is `H = sum_i m_i*|v_i|^2/2 - sum(i<j) G*m_i*m_j/r_ij`.
No central body is fixed; each massive body responds to every other body.

Use dimensionless `G*=1`, length unit L0, mass unit M0, and time unit
`T0=sqrt(L0^3/(G_physical*M0))`; velocity is L0/T0, energy M0*L0²/T0².
The executable inputs and acceptance data use these scaled units, with no implied
meter, solar mass or second assignment. A later physical conversion requires
explicit scales and a sourced gravitational constant. Display time as `t/T0`;
separately report elapsed computation seconds and movie/playback seconds.

Use float64 velocity-Verlet: half kick, full drift, half kick. Retained positions
and velocities describe the same integer-step time. The algorithm is second order,
time reversible and symplectic for this fixed-step separable Hamiltonian; none of
these properties makes numerical energy exactly constant. See the
[Hairer–Lubich–Wanner treatment](https://www.unige.ch/~hairer/preprints/gniverlet.html).
No adaptive timestep, force cutoff, damping, collision merge, relativistic term,
external field, softening or corrective energy projection is included.

## Dependency choice and finite execution envelope

Reuse pinned NumPy 2.4.6 and Pillow 12.3.0 in the existing `science-v1` environment.
NumPy provides arrays and pair arithmetic; no new package family or CUDA runtime
is required. CPU calculation and GPU presentation are separate claims. Preserve
NumPy's BSD-3-Clause notice plus actual wheel-bundled dependency notices, and
Pillow's copyright/permission notice; primary license links appear below.
Readiness must execute a tiny interacting case and inspect its arrays/invariants,
rather than treating a successful import or version probe as a capability test.

Declare 2–128 bodies, positive masses from 1e-9 to 1e9 M0, finite coordinates and
velocities with absolute components at most 1e6 in scaled units, at most 200,000
steps, and at most 10,001 retained frames. Reject combinations exceeding 2e8 total
pair evaluations, counted as `(steps+1)*N*(N-1)/2`, 512 MiB retained
output, or a conservative 128 MiB working-memory estimate before launching.
Estimate arrays as at least `48*N*frames` bytes for position/velocity alone, then
include indexes, instruments, checkpoint copies, display derivatives and images.
Monitor actual output and process-tree memory; save failure evidence on overflow.
Measure launch, integration, serialization and rendering costs before speed claims.

Set explicit `min_separation=0.05 L0` for all preregistered studies. At every force
evaluation reject any `r_ij <= min_separation`. Before accepting the drift, also
check the minimum pair distance along that straight numerical drift segment.
At both endpoints require, for every pair, `h*sqrt((m_i+m_j)/r_ij^3) <= 0.03`
and `h*|v_i-v_j|/r_ij <= 0.03`. These are declared resolution guards, not a general
stability theorem or an exact continuous-trajectory collision detector. Refuse
violations with the last valid checkpoint and offending pair/time; do not soften
the force or reduce h. A changed h or guard is a new immutable attempt.

## Reusable inputs, worker and observation contracts

Proposed adapter id: `newtonian_nbody`; trusted shipped worker uses the existing
`--input absolute/input.json --output absolute/run-dir` CLI and LabJob lifecycle.
Inputs contain body IDs, masses `[N]`, positions/velocities `[N,3]`, h, steps,
record interval, isolated boundary, explicit guard and instrument configuration.
Accept either bounded numeric input arrays or a retained numeric-only `.npz`
through `{job_id,path,destination,sha256}` same-project source copying. Validate
shapes, finite values, units, hashes, IDs and resource bounds before integration.
Convenience initial-condition generators may emit ordinary arrays, but the solver
must integrate supplied arrays without recognizing a benchmark name or orbit type.

Preserve source/generated-code snapshots, environment/dependency hashes, normalized
input, initial-state hash, equations, units and parent research/model identity.
Generated initial conditions and NumPy instruments use the existing restricted
generated-job execution boundary; a virtual environment alone is not that boundary.
Never accept a user-supplied host command through the mechanics adapter.

Reuse the trajectory index, stable body identity, instruments and observation
registration used by molecular jobs; do not duplicate orchestration or create a
special demo UI. Add representation metadata where scaled units differ from nm/ps.
Authoritative chunks are numeric-only `.npz` or `.npy`, at most 16 MiB each,
with float64 positions and synchronized velocities, exact step/time, axis order
`[frame,body,xyz]`, IDs, masses, units, chunk ranges and SHA-256 receipts.
Save step 0, each interval and the final step; identify display derivatives explicitly.
Playback must use recorded scientific timestamps with an explicit seconds-per-T0
presentation setting, independent of retained duration, body count and frame count.

Measure kinetic/potential/total energy, total momentum, center of mass, angular
momentum about the declared inertial origin, all pair distances for small N,
minimum separation, and configured pair radial velocity/relative angle. Angular
unwrapping requires a declared plane and pair. Events use sign-change brackets in
saved samples with the interpolation method and uncertainty interval retained.
Every measurement binds its configuration and source-array hash. Generated
instruments may select retained chunks for new measurements without rerunning force
integration. Do not claim an event occurring between samples was directly observed.

Write atomic progress, manifest, trajectory index, measurements and result receipts.
Checkpoint at each retained state and on cancellation, retaining integer step,
positions, full-step velocities, acceleration, input/initial/worker/runtime hashes,
and hashes of all committed output indexes. Checkpoint publication follows the
artifacts it references; recover the last fully committed generation after a crash.
Cancellation during integration saves a valid checkpoint and exits 3. Same-input
resume verifies identities and prior output hashes, appends without duplication,
and refuses incompatible source/runtime/input or corrupted state as a fresh-attempt
requirement. Preserve process, cancellation and scientific result receipts separately.

## Analytic references fixed before execution

All cases use G*=1, total mass 1, initial pair separation 1, zero center-of-mass
position/velocity, and z=0. Circular frequency is 1/T0 and period is `2*pi T0`.
The two-body control has masses `(1/2,1/2)`, positions `(-1/2,0,0),(1/2,0,0)`
and velocities `(0,-1/2,0),(0,1/2,0)`. Exact states rotate these vectors by t/T0;
initial energy is -1/8 and angular momentum is `(0,0,1/4)` in scaled units.

The equal-mass Lagrange case has three masses 1/3 at
`q_k=(cos(2*pi*k/3),sin(2*pi*k/3),0)/sqrt(3)`, `k=0,1,2`, and
`v_k=(-q_ky,q_kx,0)`. Exact states rotate rigidly; every pair distance is 1,
initial energy -1/6 and angular momentum `(0,0,1/3)`. Direct substitution gives
`a_i=-q_i` because the total mass is 1 and every pair separation is 1.
This is a special equilateral solution; equal masses are dynamically unstable to
general perturbations. Restrict analytic comparisons to the declared finite window.
See [Princeton's Lagrange discussion](https://vanderbei.princeton.edu/nBody_animations/LagrangeL4L5.html)
and the [Hu–Long–Sun stability study](https://arxiv.org/abs/1206.6162).

For the controlled perturbation multiply every initial tangential velocity by 0.8,
in both the binary and the triangle, leaving positions and masses fixed. The
triangle remains an exact homographic equilateral solution: size changes while
shape is preserved. Kepler reduction independently predicts `a=25/34`, `e=0.36`,
closest pair separation `8/17`, and period ratio `(25/34)^(3/2)` relative to control.
These follow from relative specific energy `0.8²/2-1` and angular momentum 0.8;
the [orbital energy/period relations](https://openstax.org/books/university-physics-volume-1/pages/13-4-satellite-orbits-and-energy)
provide the independent mechanics reference. The quoted a is for pair separation;
individual barycentric semimajor axes scale with their initial distances from COM.

Independent reference: solve `E-e*sin(E)=pi+t/a^(3/2)` with residual below 1e-13;
set `X=a*(e-cos(E))`, `Y=-a*sqrt(1-e²)*sin(E)`, and
`Edot=a^(-3/2)/(1-e*cos(E))`. Compute Xdot/Ydot by direct differentiation.
For each initial planar q, the exact position is `X*q + Y*J*q`, where
`J(x,y,0)=(-y,x,0)`; velocity is `Xdot*q + Ydot*J*q`.
No reference routine may import worker force, stepping or instrument functions.

## Frozen quantitative acceptance and failure criteria

1. **Force/potential contract.** Use masses `(1,2,3)` at `(0,0,0),(1,0,0),(0,2,0)`.
   Independently expect acceleration vectors `(2,3/4,0)`,
   `(-1-3/(5*sqrt(5)),6/(5*sqrt(5)),0)`, and
   `(2/(5*sqrt(5)),-1/4-4/(5*sqrt(5)),0)`; potential is `-7/2-6/sqrt(5)`.
   Maximum acceleration/potential absolute error must be below 1e-12. Independently
   central-difference potential with displacement 1e-5; force error below 1e-8.
2. **Trajectory/refinement.** Execute each of the four cases above to `t=2*pi T0`
   at `h=2*pi/1000`, `/2000`, `/4000`, with intervals 1,2,4 respectively, yielding
   1001 common sample times. Define separate position/velocity RMS errors over
   all saved bodies/times, using squared Euclidean errors divided by N*1001,
   normalized by L0 and L0/T0. For both circular cases require errors below 2e-4
   at coarse h and 1.25e-5 at fine h. For both eccentric cases require below
   3.2e-3 and 2e-4 respectively. Every successive halving must reduce each error
   by a factor 3.5–4.5. These are preregistered requirements, not measured outcomes
   or rigorous a priori bounds. Save every error, including failures.
3. **Invariants/instruments.** Independently remeasure every saved array. Require
   `max|H-H0|/|H0| < 2e-4`, `max|P-P0| < 1e-11`, `max|J-J_initial| < 1e-11`, and
   `max|COM(t)-COM(0)-P0*t/M| < 1e-11` in their declared scaled units, for all
   twelve refinement runs and the four period runs below; J denotes angular
   momentum. Instrument scalar/vector components must match independent
   measurements within 1e-10. No energy renormalization is permitted.
4. **Intervention.** On the finest binary and triangle runs, the smallest recorded
   pair distance must agree with `8/17` within 5e-4 L0 and be below 0.5 L0;
   circular counterparts must remain within 2e-4 L0 of 1. For period measurement,
   execute each of the four finest cases separately to 4400 steps at interval 4
   (`t=1.1*2*pi T0`), leaving room to bracket numerical phase delay in the circle.
   Measure the first full 2*pi relative-angle advance by bracketed interpolation;
   perturbed/control period ratio must agree with `(25/34)^(3/2)` within 0.1%
   relative error. Reject
   the predeclared false hypothesis that the 20% speed reduction preserves a
   circular, constant-separation orbit. Numerical guard activation fails these runs.
5. **Recovery and invalid input.** Touch `cancel.request` after observing strictly
   intermediate numerical progress in an actual subprocess; require exit 3,
   a valid retained checkpoint, unchanged prior sample bytes after resume, and
   bit-identical final float64 positions/velocities to uninterrupted execution
   with the same runtime. Reject changed input/source, corrupted checkpoints,
   nonpositive masses, NaN/Infinity, object/wrong-shaped arrays, duplicate IDs,
   coincident/subguard bodies, oversized h, unsupported softening/boundaries,
   and resource overflow without substituted equations or a successful receipt.
6. **Visual/time fidelity.** Retain 1024-pixel orthographic PNGs at initial,
   quarter, half, three-quarter and final times, with fixed cross-run camera,
   body-ID colors, axis units and physical time. Independently project saved
   positions and require identifiable marker centers within 1 pixel; verify
   image/source hashes and matching instrument captions. Test a translated,
   tilted asymmetric fixture to expose orientation/identity errors. Marker size
   is presentation, not a gravitational radius. Any trails use retained samples.

## Installed acceptance, reviewer variant and integration needs

After the diffusion gate passes, add the worker/readiness hook, versioned adapter,
scaled-unit trajectory support, pair/angle instruments and particle observations
through existing environment discovery, LabJob, source-copy and result contracts.
Numerical acceptance precedes public capability claims; retain failed attempts and
source snapshots. Changes to these criteria require a preserved proposal/failure
and a scientific justification written before the next acceptance attempt.

The installed Windows build must execute the question and controlled intervention
from ordinary chat, retain selected model/effort and project lineage, permit
navigation during real work, show fresh numeric playback, and recover interruption.
The model must inspect actual retained images, request an additional instrument
or view, and explain the changed closest separation/period with evidence links.
Record build/worker hashes, tool calls, inputs, timings, arrays, images, observations
and checker results. Terminal-only success does not pass the installed gate.

Then an independent reviewer specifies fresh masses, orientation, initial speed
or COM drift within the declared model, without developer edits or a prepared
answer. An unequal-mass homographic triangle or an eccentric binary in a tilted
plane is an eligible family; exact values must be chosen later by that reviewer.
Preregister its reference and tolerances before execution and run it through the
ordinary tool path, retaining any generated initial-condition/instrument source.
The adapter must also accept other valid numeric N-body initial states, while
reporting the finite accuracy evidence available for each requested regime.

No general closed-form three-body solution, long-term chaotic prediction, collision
physics, ephemeris accuracy or empirical astrophysical validity is established.
The separate molecular, diffusion, ML and remaining release criteria stay on the
acceptance ledger. This proposal launches no experiment and installs no dependency.

Primary dependency references: [NumPy 2.4.6 license](https://github.com/numpy/numpy/blob/v2.4.6/LICENSE.txt),
[numeric archive format and pickle controls](https://numpy.org/doc/2.4/reference/generated/numpy.savez.html),
and [Pillow 12.3.0 license](https://github.com/python-pillow/Pillow/blob/12.3.0/LICENSE).
