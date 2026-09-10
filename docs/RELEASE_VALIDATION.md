# PhaseForge 0.8.0 — observed validation

Date: 2026-09-10. Unreleased development build. Historical reports are retained in
`docs/history/`; their unexecuted or static checks are not current runtime evidence.

## Executed on Windows x64

Machine: Windows 11, RTX 4090 Laptop GPU (16 GiB VRAM), approximately 32 GiB RAM.

| Check | Observed result | Scope |
|---|---|---|
| Rust native all-target tests | 188 passed, 4 opt-in GPU/network/Blender tests excluded | Real numerical, scheduler, checkpoint, validator, usage and mocked-provider integration tests |
| Opt-in GPU numerical test | Passed on RTX 4090 / DX12 | Real GPU decay scoring at batch size 1, compared against CPU |
| CPU-only Rust build/tests | Passed | No GPU feature dependency; final task regressions also tested on CPU |
| Native release build | Passed | Optimized 0.8.0 executable with working CLI configuration and CPU override |
| Next.js production build | Passed | Eight exported routes and actual hydration against the Rust API |
| Native HTTP smoke programs | 5 passed | Runtime, experiment, research, discovery, verification; isolated databases, actual solvers, no paid calls |
| Strict provider schema | 22 passed | Closed composite scene/experiment schema, LF reproducibility |
| Independent Python reference/verification | 23 + 34 passed | Actual replay, numerical controls and integrity handling |
| Research upgrade and equation editor | 30 + 5 passed | Measurements and emitted equation setups |
| Existing JavaScript helper suites | 165 checks passed | API, lab, experiment, research, discovery, verification |
| JSX event-handler checks | 19 passed | Actual callbacks with deterministic hook doubles |
| Procedural scene and lifecycle tests | 18 passed | Four preset geometries, disposal, budgets, mesh validation, trajectory binding |
| Research conversation presentation | 3 passed | Team attribution, retained audit data, legacy session display and ordinary user text |
| Desktop proxy tests | 3 passed | Static export, host/origin checks, traversal, interrupted upstream connection |
| Source audit | 122 gates passed | Integration contracts and shipping file/credential checks; not a native build |

## Live OpenAI build–run–review session

Executed through the production UI with the existing OpenAI key, GPT 5.6 Sol/high,
one specialist, one experiment cycle, and a ten-minute parent budget. The session
finished in approximately 5 minutes 34 seconds with about 4 minutes 26 seconds unused.
It retained specialist output, one accepted manifest, a real numerical run, and a
structured review with a proposed parameter-permutation control. The initial
proposal used an invalid falsification identifier; the bounded repair fixed it
before any numerical execution. All provider calls were metered. Anthropic was
not called.

The experiment contains 12 procedural nodes (viral envelope, proteins/interior,
generic molecular scaffolds and references), 201 recorded frames, and three
curved scaffold-center ODE trajectories. Geometry and parameters are explicitly
conceptual. Neither the renderer nor the transport calculation establishes drug
binding, efficacy, a treatment, or a cure. The run is an application integration
check, not biomedical validation.

## Scientific studio follow-up

The actual public RCSB 4NCO entry was imported through the app API and rendered
through its durable native queue. All 22,014 atoms were retained. Blender 4.5.9 LTS
used OptiX on the RTX 4090, producing a 1024 × 1024, 48-sample image in 41.516 seconds,
a 1,484,492-triangle GLB and an editable Blender project. The GLB was opened in the
production UI and visually checked in fullscreen with orbiting. It depicts the
Env–antibody structural complex; it is not a complete measured virus or a microscope
acquisition. The separate whole-virion cutaway is explicitly morphological.

Real HTTP CAD jobs produced a valid Boolean solid and downloadable STEP/STL.
A connected KiCad board passed native DRC and produced Gerber/drill manufacturing
files; a deliberately disconnected board retained errors and withheld manufacturing.
Every file advertised by the final positive PCB job downloaded successfully. The
editable project archive contains the board settings and generated footprint library.

A Windows native dependency conflict was reproduced outside PhaseForge and fixed
with an isolated CPython runtime and pinned CasADi version. The seven worker tests
and an independent CAD subprocess now exit successfully; no crash status is ignored.

The Studio camera feedback loop and repeated WebGL context allocation were fixed.
The production UI now displays the large surface, collapses the design composer for
saved work, removes the unrelated run footer, and keeps the model picker within the
window. Public research mode and per-turn provider settings remain explicit controls.

A second native scene render bound the full 4NCO complex to illustrative virion
envelope placements and full 1HSG coordinates to a separate protein node. It
finished in 62.594 seconds on OptiX; the 13.6 MB GLB passed the actual viewer parser.
Source fingerprints distinguish measured input coordinates from authored assembly.

## Packaging and remaining acceptance

The Windows x64 NSIS installer built and installed successfully with the existing
projects and OS-stored credential preserved. The installed app opened the retained
3D experiment with playback. An idle-engine crash test recovered a healthy native
backend automatically in 4.95 seconds. The installer is unsigned.

The final follow-up installation and four native package workflow results will be
recorded after those checks finish.
The workflow uploads private CI artifacts only; it has no release publication step.
Windows ARM64 and Linux x64/ARM64 runtime acceptance are not inferred from a
Windows x64 build. NPU computation, distributed clusters, external MD/CFD/QM engines
and local open-source LLM execution are roadmap items, not verified capabilities.

Recovery tests establish bounded generation checkpoints, memory-pressure batch
reduction, retry ceilings and ownership-safe startup. They do not claim mid-step
integrator recovery, arbitrary process survival, or multi-node fault tolerance.
