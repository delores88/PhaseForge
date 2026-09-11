# Second-domain proposal: a measured spatial diffusion lab

Status: proposed on 2026-09-11; no PDE worker, catalog entry or acceptance run yet.
Implementation and ordinary-chat execution await the installed first-lab reviewer
gate. This proposal follows sections 1 and 2 and the mandatory delivery sequence
in `docs/SCIENTIFIC_WORKBENCH_CODEX_PROMPT.md`.

## Concrete question and supported model

How does doubling a passive scalar's diffusivity change the decay of a spatial
concentration pattern while preserving its spatial integral?

Implement the two-dimensional constant-coefficient diffusion equation
`dc/dt = D (d²c/dx² + d²c/dy²)` on a uniform rectangular periodic grid. Use a
five-point conservative spatial stencil and explicit forward Euler time steps
(FTCS), with float64 fields. Validate `D*dt*(1/dx² + 1/dy²) <= 0.5` before launch;
reject unstable inputs rather than silently adjusting the timestep.

The scalar is a synthetic normalized concentration (unit 1), positions are µm,
time is seconds and diffusivity is µm²/s. The integral has units µm²; it is a
conserved normalized amount, not an experimentally calibrated molecule count.
Support parameterized smooth Fourier patterns, a declared periodic localized
patch, and finite numeric initial arrays imported from retained same-project
data/generated jobs. No advection, reactions, variable diffusivity, biological
binding, irregular geometry or nonperiodic boundaries are claimed in this version.

Default proposal: `nx=ny=64`, `Lx=Ly=10 µm`, `D=0.2 µm²/s`, `dt=0.005 s`,
`steps=512`, output every 8 steps. Initial pattern:
`c(x,y,0)=1+0.2*cos(2πx/10)*cos(4πy/10)` at cell centers. Permit grids 16–512
per axis, at most 100,000 steps, at most 1,001 retained frames, and explicit
memory/output estimates. Reject requested combinations beyond the declared budget.
These are finite execution bounds, not an automatic reduction of scientific fidelity.

## Existing dependencies and execution reuse

Use the already pinned NumPy 2.4.6 and Pillow 12.3.0 in `science-v1` for a trusted
shipped `diffusion_worker.py`. Both have current Windows distributions already
exercised by the first lab. NumPy uses BSD-3-Clause terms; preserve the actual
wheel's bundled notices. Pillow's current permissive license also requires its
copyright/permission notice. No new package family or CUDA stack is needed for
this grid size. CPU performance and memory are to be measured before making a
speed claim. GPU rendering is separate from numerical computation.

Reuse the existing `LabJob`, selected-model parent session, cancellation/deadline,
managed environment, strict source-copy and artifact/observation infrastructure.
The CLI remains `--input absolute/worker-input.json --output absolute/run-dir`.
Proposed adapter id: `diffusion_2d`; no public registration until the gate passes.
The backend chooses the shipped worker; user input never supplies a host command.

The worker writes atomic `progress.json`, `manifest.json`, `measurements.json`,
`result.json`, source/version hashes, and deterministic checkpoints. Save the exact
float64 field, step, input/initial-field hashes, and output-index receipt at each
checkpoint. Cancellation saves a valid checkpoint and exits 3. Resume checks the
same equations, discretization, dependencies and input hash before appending;
otherwise it explicitly requires a fresh immutable attempt.

## Field, instrument and observation artifacts

`fields/index.json` declares shape `[ny,nx]`, cell-center coordinates, axis order,
units, physical timestamps, boundary conditions, chunk ranges and SHA-256 hashes.
Authoritative field chunks use numeric-only `.npy` arrays (`allow_pickle=False`),
at most 16 MiB each. A separate float32 little-endian display buffer or bounded
JSON field slice can support the browser; it is identified as a display derivative.
Do not encode grid cells as fictitious molecule entities or put full arrays into
agent context. Generated NumPy instruments can import selected chunks using the
existing `{job_id,path,destination,sha256}` source contract.

Measure spatial integral, mean, min/max, variance, Fourier-mode amplitude, and
user-specified point/region averages. State the observation model for each.
Record instrument configuration, source field hash, and resulting numeric arrays.

Render actual retained fields into 1024-pixel evidence PNGs with x/y units,
physical time, fixed cross-run color scale and scalar legend. Link each image to
its source frame/hash. Browser playback loads field chunks on demand; its display
resolution may change without changing the saved scientific field. A top-down
field map is the calibrated observation; a height surface, if included, must
label height as scalar value rather than an additional physical dimension.

## Numerical acceptance declared before execution

The independent checker must not import the worker's stencil or instrument code.
For the Fourier benchmark, evaluate ordinary sine/cosine/exponential functions
directly on coordinates reconstructed from the saved input.

1. **Discrete implementation:** compare the field to the independently derived
   discrete Fourier amplification factor
   `g = 1 - 4Ddt[sin²(πmx/nx)/dx² + sin²(πmy/ny)/dy²]` raised to the saved step.
   Require maximum absolute field error below `1e-10` at every retained frame.
2. **Continuum and refinement:** compare to amplitude
   `A(t)=A0*exp(-D[(2πmx/Lx)²+(2πmy/Ly)²]t)`. At `t=2.56 s`, run 32²/64²/128²
   grids with timesteps 0.02/0.005/0.00125 s, using the same physical initial
   condition (`mx=1,my=2`). Relative L2 error of the fluctuation about the mean
   must be below 0.008 on 32² and 0.0005 on 128²; each refinement must reduce it
   by a factor between 3.5 and 4.5. Expected order follows O(dt+dx²+dy²).
3. **Conservation and positivity:** relative drift of the spatial integral below
   `1e-11`; minimum no less than the initial minimum minus `1e-12`, maximum no
   greater than initial maximum plus `1e-12`. Independently remeasure saved arrays.
4. **Intervention:** double D from 0.2 to 0.4 on the 64² case. The measured amplitude
   ratio must agree with the analytic continuum ratio within 1% and show faster
   decay, while both spatial integrals satisfy the same conservation bound.
5. **Recovery:** interrupt mid-integration, retain the checkpoint, resume and
   require bit-identical saved final float64 fields to an uninterrupted run with
   the same input and runtime. Retain process/cancellation receipts separately.
6. **Invalid input:** an explicitly unstable timestep, wrong-shaped/object array,
   non-finite field or unsupported boundary must fail before integration, with no
   substituted model, adjusted timestep or successful result receipt.
7. **Visual/instrument fidelity:** independently check field orientation and
   selected interior pixels against the declared fixed color map, allow at most
   one 8-bit channel level of quantization error, and verify PNG/source hashes.
   Independently computed point/region instruments must agree within `1e-10`.

An installed ordinary-chat run, a control comparison, navigation during execution,
actual model image observation and an independently specified new supported
variant remain separate acceptance gates. Passing the analytic benchmark establishes
numerical agreement for this PDE; predictive validity for a real material is untested.
Any scientifically justified change to these criteria must preserve this proposal
and failed evidence and explain the change before the next acceptance run.

## Later adapters and ML

Reuse the same identities, source snapshots, units, clocks, checkpoints, arrays,
instruments and observation receipts for Newtonian interacting bodies and an
ML-assisted study. A particle adapter can reuse the existing trajectory schema;
field and ML adapters keep their native array representation. Parameter estimation
or a surrogate must train on actual solver output, split by experimental condition,
retain a simple baseline and held-out outcomes, and measure training plus inference
cost. For this linear PDE an analytic/spectral predictor is an unusually strong
baseline; do not claim useful ML acceleration merely by beating a slow implementation.
Select a task where calibration/estimation or repeated expensive solver evaluations
make the learned model useful, and require uncertainty or direct-solver fallback
outside the validated range. Specific mechanics and ML tolerances need separate
preregistration before their implementation and execution.

## Primary references consulted

- [Gilbert Strang, MIT: heat equation discretization and stability](https://math.mit.edu/classes/18.086/2006/am54.pdf)
- [MIT numerical PDE course: periodic Fourier heat evolution and finite differences](https://math.mit.edu/classes/18.336/index.html)
- [NumPy 2.4 FFT conventions](https://numpy.org/doc/2.4/reference/routines.fft.html)
- [NumPy 2.4 numeric array loading and object-array restrictions](https://numpy.org/doc/2.4/reference/generated/numpy.load.html)
- [NumPy 2.4.6 license](https://github.com/numpy/numpy/blob/v2.4.6/LICENSE.txt)
- [Pillow 12.3.0 license](https://pillow.readthedocs.io/en/stable/about.html#license)
