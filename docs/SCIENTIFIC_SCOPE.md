# Scientific scope and limitations

## What PhaseForge is

PhaseForge is an experimental computational workbench for creating, executing, inspecting, and challenging bounded scientific models. It is not a source of experimental truth and does not turn model output into proof about nature or medicine.

Version 0.8 adds a local Electron desktop, procedural 3D scenes and timed research
sessions. These improve construction, inspection and iteration around implemented
models; they do not expand the native solver into all scientific domains.

## Procedural representation

Three.js scene primitives represent atoms, explicit bonds, protein backbones, DNA,
membranes, planets, surfaces, curves, supplied streamlines and bounded triangle
meshes. Nodes can bind to numerical entities, use supplied coordinates or remain
conceptual. Scene provenance is saved with each run. See [scenes](SCIENTIFIC_SCENES.md).

Visual geometry is not a force field. Procedural DNA is not an experimentally
determined conformation; a viral envelope and nearby molecules do not establish
binding or efficacy. The analytic potential-flow example is not a viscous fluid solver.

## Executable numerical capabilities

### State-vector dynamics

First-order ordinary differential equations with explicit variables, constants, initial conditions, Euler or RK4 integration, observables, constraints, search, visualization mappings, and falsification passes.

### Pairwise particles

One- to three-dimensional particles governed by a radial interaction expression, damping, external acceleration, configurable boundaries, search variables, observables, and particle-cloud visualization.

## Molecular capabilities

PhaseForge imports PDB, MOL/SDF V2000, and XYZ structures into normalized records. It can compute bounded deterministic and heuristic diagnostics, visualize the structure, define candidate QM/MM regions, estimate electron-count consistency, and plan staged computational campaigns.

It does not presently provide a general docking engine, force-field parameterization pipeline, production molecular-dynamics executor, free-energy implementation, electronic-structure solver, reaction-path solver, pharmacokinetic predictor, toxicity system, or synthesis robot.

## Evidence labels

The interface must distinguish:

- parsed source facts;
- conceptual/procedural illustration and supplied structural coordinates;
- inferred connectivity or screening heuristics;
- results produced by an implemented numerical kernel;
- plans requiring an external engine;
- independent reproduction;
- wet-lab or observational validation.

## Capability gaps

Agents must return a capability gap when a question requires unsupported PDEs, fields, quantum amplitudes, stochastic wavefunctions, relativity, electronic structure, molecular dynamics, docking, reaction chemistry, or another absent representation. They must not coerce such questions into an unrelated classical model merely to produce output.

## Biomolecular claims

Structure diagnostics and simulations cannot establish safety, efficacy, selectivity, bioavailability, synthesizability, novelty, or clinical benefit. Those claims require appropriate computational methods, independent validation, and controlled laboratory evidence.

## Numerical checks are not hypothesis or novelty adjudication

Version 0.3.3 records each declared constraint's value, tolerance, and verdict.
Challenges retain trial measurements, actual perturbations, endpoint completion,
and baseline comparisons. A challenge without explicit metric comparison rules
is inconclusive, even when it has no optimization objective and a zero score.
Each trial must satisfy its declared checks; a median cannot hide a failed trial.

A stable timestep-halving result does not establish convergence order, a small
perturbation does not prove chaos, endpoint conservation does not bound all
intermediate error, and a visually recurrent orbit does not establish novelty.
AI explanations remain advisory and cannot change these deterministic verdicts.
Missing drivers, fields, comparison rules, or historical evidence remain missing.

Timed sessions can execute supported experiments within the researcher's time,
cycle and usage limits. Specialist agreement is not independent replication.
Completion records bounded work, not validation of the overarching hypothesis.

## 0.5.0 independent ODE scrutiny

An optional approved Python worker now executes independent Euler/RK4 and adaptive
Dormand–Prince 5(4) replay, calibration intervals and new parameter perturbations.
The original observation grid defines its summary measurements. This verifies only
the represented ODE and protocol; it adds no MD/QM/PDE capability or general stiff
solver. Reference distances are feature comparisons, not equivalence or novelty
proof. No p-value or corrected significance is inferred from a selected candidate.
