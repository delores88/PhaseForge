# First executable molecular laboratory: periodic Lennard-Jones argon

## Scope and frozen acceptance plan

This plan was written before executing the new engine or its acceptance checker.
The first laboratory computes a classical, monatomic, periodic Lennard-Jones fluid
with OpenMM. It does not compute HIV, drug binding, chemical reactions, quantum
electrons, or therapeutic efficacy. The purpose is to establish an inspectable
engine -> trajectory -> instruments -> image-observation path with numerical tests.

The declared illustrative argon-like parameters are mass 39.948 Da, sigma 0.3405 nm,
epsilon 0.997 kJ/mol. They define this model; agreement with experimental argon is
not an acceptance claim. A cubic box follows the requested mass density. Neutral
particles interact using OpenMM NonbondedForce, periodic minimum-image cutoff,
quintic switching from 0.8 cutoff, and no long-range dispersion correction. The
cutoff is min(2.5 sigma, 0.49 box length), so small boxes have an explicitly changed
cutoff. Initial coordinates come from an FCC grid, with seeded thermal velocities.

These criteria are frozen independently of the observed outcomes:

1. **Frozen-coordinate energy and force reference.** Independently sum switched LJ
   pair energies and analytic forces in a standalone checker (no worker helper).
   Include a pair below the switching radius, a pair inside the switch interval,
   a pair across a periodic boundary, and a pair outside the cutoff. OpenMM
   Reference must agree to 1e-8 kJ/mol energy and 1e-7 kJ/(mol nm) max force error.
   CPU must agree to 2e-4 kJ/mol and 3e-3 kJ/(mol nm). Verify the independent
   force's sign and magnitude with central energy finite differences (1e-6 nm,
   1e-5 kJ/(mol nm) tolerance). These are equation/implementation tests, not an
   empirical force-field validation.
2. **NVE conservation/refinement.** Use the same seeded FCC 32-atom system at
   density 0.8 g/cm3 and initial temperature 100 K, Reference platform, no
   thermostat. Evolve 0.5 ps at 2 fs and 1 fs. Maximum total-energy deviation
   divided by max(initial kinetic energy, 1 kJ/mol) must be below 0.005 for both;
   the finer step must not exceed 0.6 times the coarser error (unless both absolute
   errors are below 1e-7 kJ/mol). Check total momentum conservation independently.
3. **Controlled temperature comparison.** Two otherwise identical 108-atom CPU
   Langevin simulations, density 0.8 g/cm3, friction 5/ps, seeds 812/813, 10 ps at
   1 fs, targets 90 K and 180 K. Each mean temperature over the final 5 ps must be
   within 15% of its target; hot mean must exceed cold mean by at least 60 K.
   This tests thermostat response over this small finite sample, not an ensemble
   confidence interval, phase transition, or physical validation of argon.
4. **Artifact and instrument integrity.** Recorded frame count, exact coordinates,
   times, chunk hashes, topology, measurements, checkpoint, and projected PNG
   provenance agree. Independent virial pressure and MSD recomputation at saved
   samples must match. RDF counts must be nonnegative and normalized by the actual
   sampled frame count. JSON must contain no NaN/Infinity. A resumed run must retain
   prior samples and source identity; an incompatible input must be refused.
5. **Invalid inputs.** Negative temperature, nonfinite input, unknown parameter,
   excessive atom/step budgets, and an unavailable platform must produce nonzero
   failure without reporting a completed experiment.

The checker must preserve the criteria and actual numeric results in its report.
A failed check remains failed; do not loosen tolerances to obtain a green label.

## Execution and files

`python tools/scientific_worker.py --input <absolute/input.json> --output <absolute/run-dir>`

Input engine is `openmm_argon`; parameters and bounds are declared in the worker.
The output contains an immutable input/worker hash manifest, topology, chunked
numerical trajectory, scalar measurements with units, RDF and MSD, a checkpoint,
and fixed orthographic PNG projections linked to source-frame hashes. Visual
positions are wrapped into [0,L); MSD uses unwrapped coordinates. Display radius
sigma/2 is presentation geometry, not a hard collision boundary. PNGs use saved
numbers and a declared camera; they are not microscopy images or empirical data.

Progress is written atomically. `cancel.request` requests a cooperative saved stop.
Re-running the exact input/output pair may resume the last committed checkpoint;
OpenMM binary checkpoints are restricted to compatible engine/platform/hardware.
A checkpoint preserves simulation state and integrator RNG, not an AI memory.
Incomplete chunks are never advertised in the index. Context compaction and app
process lifetime are separate from the physical engine's saved trajectory.

The isolated Python environment runs trusted shipped code. It is not a sandbox
for arbitrary model-generated Python. No credentials are needed by this worker.

## Primary references and license

- [OpenMM standard forces](https://docs.openmm.org/latest/userguide/theory/02_standard_forces.html)
  defines the LJ potential, switching polynomial, and dispersion correction.
- [OpenMM integrators](https://docs.openmm.org/latest/userguide/theory/04_integrators.html)
  describes Verlet and Langevin Middle integration and kinetic-energy conventions.
- [OpenMM Context](https://docs.openmm.org/latest/api-python/generated/openmm.openmm.Context.html)
  documents positions, velocities, energy, and checkpoint portability limits.
- [OpenMM licensing](https://docs.openmm.org/latest/userguide/library/01_introduction.html)
  identifies MIT-licensed core/Reference/CPU and LGPL GPU platforms; the upstream
  [license texts](https://github.com/openmm/openmm/tree/8.5.2/docs-source/licenses)
  must be preserved with distributed engine binaries. This project installs
  unmodified published wheels in a separate runtime rather than relicense them.

Pinned requirements are in `tools/requirements-science.txt`. The local development
runtime is `.local/science/venv/Scripts/python.exe`; desktop runtime provisioning
and process limits belong to the application runtime, not this worker.

## Observed Windows development acceptance (2026-09-11)

The pinned packages installed in an isolated CPython 3.12.7 venv. The published
OpenMM wheel is 8.5.2; its internal runtime version string is
`8.5.2.dev-36a30cb`. Registered platforms were Reference, CPU, and OpenCL. CPU and
Reference were actually executed. Registration alone does not establish a usable
OpenCL device. CUDA was unavailable and its explicit request was rejected.

Frozen criteria document SHA-256 before any run:
`6fa856ca075d145fad215fbfecc07856d315f60270583550b85e79ddf75ac002`.
Raw reports are retained under `.local/science/acceptance-01/report.json`,
`acceptance-02/report.json`, and `acceptance-03/report.json`. The second attempt
found a Windows file-sharing error under aggressive progress polling, before its
checkpoint test completed. The worker now retries atomic replacement for up to
two seconds, preserving the previous complete JSON throughout. No scientific
tolerance was loosened. The third suite passed all checks in approximately 16 s.

| Independent check | Actual result |
| --- | --- |
| Reference frozen LJ energy / max force error | 1.67e-15 kJ/mol / 5.68e-14 kJ/(mol nm) |
| CPU frozen LJ energy / max force error | 8.40e-6 kJ/mol / 4.45e-4 kJ/(mol nm) |
| Analytic pair force versus independent central differences | 6.69e-8 kJ/(mol nm) |
| NVE maximum energy deviation, 2 fs -> 1 fs | 4.6727e-4 -> 1.1681e-4 kJ/mol |
| NVE maximum momentum drift | 2.48e-10 Da nm/ps |
| Late mean temperatures, 90 K / 180 K targets | 87.7615 K / 184.1549 K |
| Independent pressure / MSD errors | 2.61e-12 bar / 0 nm2 |
| Independent RDF pair counts, frame/chunk/image hashes | Exact agreement |
| Actual cooperative pause and resume | Paused after step 60, resumed to 1000; final positions identical to uninterrupted Reference run |
| Invalid and unavailable requests | Nonzero exit, no completed substitute |

A separate `.local/science/supervisor-compat` run executed 100 actual CPU steps
and retained 11 frames using a copied shipped worker, `python -I`, the desktop's
sanitized environment, parent log files, and a distinct `worker-input.json`.
Parent provenance may be stored in `input.json`; it must not be added to the strict
worker input. The final compatibility change only admitted this exact supervisor
filename. No scientific calculations changed after the passing suite.

Projected PNGs were visually inspected; axis labels, frame time, units and depth
encoding are visible. The live app's agent/HTTP/renderer integration, installation,
process supervision and release admission remain separate acceptance stages.
