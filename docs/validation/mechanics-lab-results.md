# Mechanics numerical acceptance results

On 2026-09-11 the frozen numerical component acceptance passed all **23 groups**.
The [final combined report](../../.local/validation/mechanics-check-20260911-timeout-retry/report.json)
combines 17 reused successful primary executions, one permitted timeout retry,
three reused auxiliary groups, the pure references, and recomputed comparisons.
This establishes the declared finite numerical tests. Installed ordinary-chat
acceptance and the independent reviewer's fresh variant remain **pending**.

## Model, frozen criteria and evidence identity

The [preregistered proposal](mechanics-lab-proposal.md) fixes interacting point
masses, isolated boundaries, unsoftened inverse-square gravity with G*=1, and
float64 velocity-Verlet. Length, mass and time are L0, M0 and
`T0=sqrt(L0^3/(G_physical*M0))`; no meter, solar-mass or second assignment is
implied. All bodies respond to the supplied masses and coordinates. The binary
and equilateral triangle are initial arrays supplied to the generic worker.

The independent checker evaluates analytic circular/Kepler trajectories, direct
force and invariant formulae, and retained observations without importing worker
force, integration or instrument functions. The operational retry loads the
original checker snapshot by SHA-256 and preserves its numerical functions and
tolerances. The source chain is retained beside both reports.

| Evidence | SHA-256 |
| --- | --- |
| Original report | `067f3fb74651c3d96586748fe6fbd621f7d6158496a2b64d72272431ad3e4b74` |
| Final combined report | `1a5d42bc5af5b82baca70efa8e02afa9cc99ccafdd7839a0a3a7eb7dec67f7de` |
| Worker | `455056ac9bdf12cba80a9b86699970cf92939432e1177841b936838913c1d125` |
| Original independent checker | `e8af97912a93e1bbd00336c6e857546e93e6701cd95801f106c821b023ac56d9` |
| Frozen proposal | `afa5299a2c6bf720d305ff45650d0fa18ec43e185276b9e995ed3b9bd0cd09af` |
| Operational retry plan | `dbb0b1307f776387fa5c5444af7b1988b2f72322fbddcfb3d222b6bc7f9ef331` |
| Retry orchestration | `76d63f37d78e8a6ad09d6f1e1b7b15d978c7ce6833ed901f8aa7c2619e9f1eca` |

The manifests record engine version 1.0.0, Python 3.12.7 and NumPy 2.4.6.
The pinned requirements source hash is
`f9fc292f9975f0cc31ec672548f9e23f5f08ef34d3a29e1fe59194b5f336b2ac`;
runtime descriptor SHA-256 is
`ad003eb5e312b3437fe8eada39bea901d71ef84fc70f289e91665cd0f80618a5`.
This is the declared runtime descriptor, not a complete runtime-file inventory.

## Preserved failure and bounded retry

The [original report](../../.local/validation/mechanics-check-20260911-attempt1/report.json)
finished with **21 of 23 groups passing**, `passed: false`, and
`numerical_acceptance_passed: false`. It took 2,377.766 wall seconds. The
`binary-eccentric-period` process exhausted the original 120-second process
budget; the combined comparison then failed on its missing period prerequisite.
Neither failure was removed or converted into an original-attempt success.

Its [process receipt](../../.local/validation/mechanics-check-20260911-attempt1/binary-eccentric-period/process.receipt.json)
records `timed_out: true`, exit 1 and 120.015 elapsed seconds. The retained
progress/checkpoint reached step 4384 of 4400 at 6.886371096668827 T0.
There is no completed result for that original process. Its files remain in
`.local/validation/mechanics-check-20260911-attempt1/`.

The [operational plan](../../.local/validation/mechanics-timeout-retry-plan-20260911.json)
authorized 300 seconds only for an actual timed-out receipt, with unchanged
worker, inputs, sampling, references and acceptance criteria. The retry used a
fresh `.local/validation/mechanics-check-20260911-timeout-retry/` directory.
Its sole new [process receipt](../../.local/validation/mechanics-check-20260911-timeout-retry/binary-eccentric-period/process.receipt.json)
records exit 0, no timeout and 108.921 wall seconds; step 4400 completed at
6.911503837897545 T0. The original and retry input JSON bytes are identical,
SHA-256 `c94ddd13ffde5709f480bf1f52acafa7af2bcf73655ca54aca5319a682a7a424`.

The retry report took 291.484 wall seconds including independent reinspection of
the original passing primary outputs. It reran exactly one physics process;
17 passing primary processes were not repeated. Recovery, numeric import and
invalid-input group results were carried forward from the original report.
Failed auxiliary groups were ineligible for automatic repetition. References
and refinement/intervention comparisons were recomputed with the frozen checker.
No worker source, equation, numerical input or tolerance changed. The faster
second execution is a measured timing difference, not evidence of a solver
optimization or a guarantee that these workloads finish within 120 seconds.

## Trajectories, invariants and intervention

For each circular/eccentric binary/triangle family, h was `2*pi/1000`, `/2000`
and `/4000` T0, with recording intervals 1, 2 and 4. Each run retains 1001
common times through 2*pi T0. The perturbation multiplies initial tangential
velocities by 0.8 while leaving masses and positions fixed.

| Family | Fine position RMS / L0 | Fine velocity RMS / (L0/T0) | Position reduction factors | Velocity reduction factors |
| --- | ---: | ---: | --- | --- |
| Binary circular | 1.75099523e-6 | 1.63401943e-6 | 3.99995683, 3.99998920 | 3.99992360, 3.99998089 |
| Binary eccentric | 4.68544201e-6 | 7.81035561e-6 | 3.99953776, 3.99988444 | 3.99950171, 3.99987544 |
| Triangle circular | 2.02187512e-6 | 1.88680311e-6 | 3.99995682, 3.99998922 | 3.99992360, 3.99998092 |
| Triangle eccentric | 5.41028245e-6 | 9.01862191e-6 | 3.99953776, 3.99988441 | 3.99950172, 3.99987540 |

Each factor compares successive timestep halvings. All are within the frozen
3.5–4.5 interval. Both coarse and fine position/velocity bounds passed: circular
2e-4 and 1.25e-5; eccentric 3.2e-3 and 2e-4 in their respective normalized units.
The complete coarse/medium/fine error values remain in both reports. The largest
Kepler-equation residual was 4.4408921e-15, below the frozen 1e-13 requirement.

The maxima below cover the completed primary outputs, including the tilted and
translated coordinate fixture. Invariant quantities are independently remeasured
from saved arrays. The instrument figure is the maximum absolute component
disagreement over the configured quantities, each in its declared scaled unit.

| Quantity | Observed maximum | Frozen limit |
| --- | ---: | ---: |
| Relative energy drift, max abs(H-H0)/abs(H0) | 8.41344586e-5 | < 2e-4 |
| Momentum drift / (M0 L0/T0) | 4.04582740e-15 | < 1e-11 |
| Angular momentum drift / (M0 L0²/T0) | 2.20617546e-15 | < 1e-11 |
| COM residual after initial uniform drift / L0 | 1.21041969e-14 | < 1e-11 |
| Instrument component disagreement | 1.66533454e-16 | < 1e-10 |

The maximum saved-acceleration disagreement was 1.11022302e-15 L0/T0².
At the frozen three-mass force fixture, acceleration disagreement was zero and
the reported initial potential was -6.183281572999748 M0 L0²/T0², matching
`-7/2-6/sqrt(5)` to the reported precision. The independent central-difference
force error was 1.90738092e-10, below 1e-8. This checks the potential derivative
separately from the trajectory comparison; it does not use worker forces as truth.

The finest eccentric runs' smallest recorded pair distances were
0.4705922225802632 L0 (binary) and 0.4705922225800175 L0 (triangle).
Each differs from the analytic `8/17 = 0.47058823529411764` by about
3.9873e-6 L0, below the 5e-4 limit and below 0.5 L0. Finest circular distances
departed from 1 L0 by at most 1.23370e-6 L0, below 2e-4.

| Family | Circular period / T0 | Eccentric period / T0 | Measured period ratio |
| --- | ---: | ---: | ---: |
| Binary | 6.283190474885385 | 3.9616261226280702 | 0.6305118615237169 |
| Triangle | 6.2831904748853935 | 3.961626122628104 | 0.6305118615237213 |

The analytic ratio is `(25/34)^(3/2) = 0.6305095042004002`. Relative error is
about 3.73876e-6 in both cases, below 0.001. All period runs used 4400 finest
steps and interval 4, giving 1101 saved states through 1.1*2*pi T0. Periods use
linear interpolation of the first forward 2*pi relative-angle advance. Circular
events are bracketed by [6.283185307179586, 6.289468492486766] T0; eccentric
events by [3.9584067435231396, 3.9646899288303192] T0. These brackets preserve
saved-sample resolution, not statistical confidence intervals. The minimum
distances above are sampled minima, not claimed exact continuous event times.

The recorded contraction and period reduction reject the preregistered false
hypothesis that this speed reduction preserves a circular constant-separation
orbit. No guard activated in the accepted study runs. This result applies to
the specified binary and special homographic triangle over the tested window.

## Recovery, imported state and visual fidelity

The actual recovery subprocess received `cancel.request` after observed step 4,
retained checkpoint step 8 and exited 3. The original group verified that resume
left published samples unchanged and produced final float64 positions/velocities
bit-identical to an uninterrupted comparison. Changed input and a corrupted
checkpoint each exited 2; the same-input resume exited 0. This is actual process
interruption and continuation, distinct from retrying the timed-out period run.

The positive numeric NPZ import completed with maximum position/velocity errors
1.19352879e-8 L0 and 9.62664504e-10 L0/T0; changing its source under the old
identity was refused with exit 2. All 14 invalid-input cases exited 2: zero or
negative mass, wrong shape, duplicate ID, coincident or subguard bodies,
unresolved timestep, unsupported softening/boundary, excessive steps/frames/pair
work, nonfinite JSON and an object-array archive. These are the exercised
fixtures, not an exhaustive test of every resource or collision configuration.

The independent image checks cover **25 PNGs and 60 body-marker comparisons**:
the four period runs and the tilted/translated fixture, each at five retained
milestones. Maximum marker-center error was **0.645000043 pixels**, below one
pixel. Checks bind image hashes to registered numerical chunks and exact saved
frames, body identities/colors, scientific step/time, and instrument snapshots;
they also check 1024-square PNGs and unchanged control/intervention cameras.
Instrument snapshot equality is a metadata check, not OCR of every caption glyph.
The coordinate fixture passed the same finest eccentric trajectory tolerance.

## Retained data, computation costs and limits

The 18 completed primary outputs contain **17,419 frames**, **35,213 files**
and **99,150,729 logical bytes**: twelve refinement runs, four period runs,
the force fixture and the coordinate fixture. Recovery/import support outputs
are excluded from that primary count. At this audit, the entire original
evidence tree contains 39,568 files / 113,299,650 bytes and the retry tree
2,234 files / 5,799,802 bytes. Those broader totals include source/report
snapshots, the failed attempt, process receipts and support/corruption copies.
There are 39 original subprocess receipts and exactly one retry subprocess receipt.

These are logical retained-file lengths, not allocated disk blocks or total
bytes written through the filesystem. Replacing indexes and checkpoints writes
additional bytes that do not remain as distinct files. The documentation audit
read reports, fine measurements, inputs and file metadata; it did not repeat
the original checker's full NPZ inspection or launch additional workers.

The [binary circular 1000 result](../../.local/validation/mechanics-check-20260911-attempt1/binary-circular-1000/output/result.json)
provides a concrete worker profile: 111.419049 wall seconds, including 0.510167
integration, 100.759444 serialization and 0.150621 rendering seconds. The
remaining 9.998817 seconds are outside those timed scopes. Its enclosing process
took 111.906 seconds. Simulated duration is 2*pi T0, independent of these wall
times and any later playback duration. Across all 18 accepted primary executions,
worker wall time sums to 1800.996487 seconds; integration, serialization and
rendering sum to 13.829585, 1618.649193 and 2.170123 seconds respectively.
These are worker-reported timing scopes, not an isolated hardware benchmark.

Maximum reported primary-worker peak working set was 61,177,856 bytes. This
includes the Python process and loaded libraries, not only numerical arrays;
it is separate from disk totals and the 128 MiB declared working-array estimate.
The current recorder publishes one-state immutable chunks and checkpoints at
every retained state. `chunk_frames` is an upper bound, not functional batching.
Repeated durable publication dominates these small-N runs. No batching change
or performance improvement was made during the frozen acceptance sequence.

## Remaining acceptance

The installed ordinary-chat path still must demonstrate model/project lineage,
real work with navigation, numeric playback, retained-image inspection, an
additional requested instrument/view, and interruption recovery in the installed
build. A fresh independent-reviewer variant must then preregister its reference
and tolerances and run through the ordinary tools. Neither gate is certified by
these standalone worker/checker receipts; `installed_acceptance` is `not evaluated`.

The equal-mass triangle is a special, generally unstable solution. These results
do not establish a general three-body closed-form solution, long-term chaotic
prediction, collision physics, ephemeris accuracy or empirical astrophysical
validity. Broader application and release criteria remain separate.
