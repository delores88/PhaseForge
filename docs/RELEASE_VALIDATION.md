# PhaseForge 0.8.0 — observed validation

Date: 2026-09-10. Unreleased development build. Historical reports are retained in
`docs/history/`; their unexecuted or static checks are not current runtime evidence.

## Executed on Windows x64

Machine: Windows 11, RTX 4090 Laptop GPU (16 GiB VRAM), approximately 32 GiB RAM.

| Check | Observed result | Scope |
|---|---|---|
| Rust native all-target tests | 156 passed, 1 opt-in GPU test excluded | Real numerical, scheduler, checkpoint, validator, usage and mocked-provider integration tests |
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
| Procedural scene tests | 14 passed | Four preset geometries, disposal, budgets, mesh validation, trajectory binding |
| Desktop proxy tests | 3 passed | Static export, host/origin checks, traversal, interrupted upstream connection |
| Source audit | 121 gates passed | Integration contracts and shipping file/credential checks; not a native build |

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
generic molecular scaffolds and references),201 recorded frames, and three
curved scaffold-center ODE trajectories. Geometry and parameters are explicitly
conceptual. Neither the renderer nor the transport calculation establishes drug
binding, efficacy, a treatment, or a cure. The run is an application integration
check, not biomedical validation.

## Packaging and remaining acceptance

The Windows x64 NSIS installer builds successfully. Final installation and the
four native package workflow results will be recorded after those checks finish.
The workflow uploads private CI artifacts only; it has no release publication step.
Windows ARM64 and Linux x64/ARM64 runtime acceptance are not inferred from a
Windows x64 build. NPU computation, distributed clusters, external MD/CFD/QM engines
and local open-source LLM execution are roadmap items, not verified capabilities.

Recovery tests establish bounded generation checkpoints, memory-pressure batch
reduction, retry ceilings and ownership-safe startup. They do not claim mid-step
integrator recovery, arbitrary process survival, or multi-node fault tolerance.
