# PhaseForge 0.8.0 — observed validation

Date: 2026-09-10. Unreleased development build. Historical reports are retained in
`docs/history/`; their unexecuted or static checks are not current runtime evidence.
Application source commit: `5a26384`. Final package-build provenance remains pending.

## Executed on Windows x64

Machine: Windows 11, RTX 4090 Laptop GPU (16 GiB VRAM), approximately 32 GiB RAM.

| Check | Observed result | Scope |
|---|---|---|
| Rust native all-target tests | 192 passed, 5 opt-in tests excluded | Real numerical, scheduler, checkpoint, validator, usage and mocked-provider integration tests |
| Opt-in GPU numerical test | Passed on RTX 4090 / DX12 | Real GPU decay scoring at batch size 1, compared against CPU |
| CPU-only Rust build/tests | Passed | No GPU feature dependency; final task regressions also tested on CPU |
| Native release build | Passed | Optimized 0.8.0 executable with working CLI configuration and CPU override |
| Next.js production build | Passed | Eight exported routes and actual hydration against the Rust API |
| Native HTTP smoke programs | 5 passed | Runtime, experiment, research, discovery, verification; isolated databases, actual solvers, no paid calls |
| Strict provider schema | 22 passed | Closed composite scene/experiment schema, LF reproducibility |
| Independent Python reference/verification | 23 + 34 passed | Actual replay, numerical controls and integrity handling |
| Research upgrade and equation editor | 30 + 5 passed | Measurements and emitted equation setups |
| Existing JavaScript helper suites | Passed; research helpers: 25 checks | API, lab, experiment, research, discovery, verification; no unverified combined total |
| JSX event-handler checks | 19 passed | Actual callbacks with deterministic hook doubles |
| Frontend Node tests | 30 passed | Model intake, molecular surfaces, procedural scenes, lifecycle and conversation presentation; the following scene/presentation counts are subsets |
| Procedural scene and lifecycle tests | Latest targeted scene suite: 17 passed; resource suite: 2 passed | Four preset geometries, disposal, budgets, mesh validation, trajectory binding and opacity; includes a correction after the 30-test frontend run |
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

## Scoped Galactic-center experiment

The original request to gather nearby-object data and simulate everything within
5,000 light years of Sagittarius A* remains incomplete. Two inspected broad
attempts reached the provider's output ceiling without an accepted manifest or
submitted numerical run. The successful follow-up explicitly narrowed the prompt
to one illustrative S2-like test star around a fixed central point mass; this was
not autonomous completion of the original request.

GPT-6 Astra/medium generated and submitted that six-state Newtonian baseline.
CPU/f64 RK4 completed 64,000 steps over 32 simulated years in 2.168 seconds,
retaining 641 frames and three scene nodes. The star is bound to its computed
trajectory; the black-hole glyph and disk remain conceptual. Maximum relative
energy error was 8.31168e-11 and angular-momentum error was 1.81251e-13, both below
the declared 1e-6 tolerance. All three step-halving comparisons passed; halving
the time step reduced energy error to 4.97843e-12. These are checks of the authored
numerical model, not empirical validation of Sagittarius A* or S2.

The turn recorded two bounded public-catalog searches, but no astronomical dataset
was imported or used to fit the orbit. Its initial conditions and central mass
were declared illustrative. Relativity, extended stellar mass, gas, radiation,
spin and a 5,000-light-year object census were outside this executable stage.

A subsequent project-scoped Crossref metadata search for “Sagittarius A* S2 orbit”
completed in 1.17 seconds and exposed five DOI-bearing literature results in the UI.
It included an S2-orbit paper alongside less relevant Sagittarius matches. This
verifies public metadata retrieval and visible citations; it does not retroactively
source the earlier illustrative orbital parameters or establish a stellar census.

## Scientific studio follow-up

The actual public RCSB 4NCO entry was imported through the app API and rendered
through its durable native queue. All 22,014 atoms were retained. Blender 4.5.9 LTS
used OptiX on the RTX 4090, producing a 1024 × 1024, 48-sample image in 41.516 seconds,
a 1,484,492-triangle GLB and an editable Blender project. The GLB was opened in the
production UI and visually checked in fullscreen with orbiting. It depicts the
Env–antibody structural complex; it is not a complete measured virus or a microscope
acquisition. The separate whole-virion cutaway is explicitly morphological.

GPT-6 Astra/medium also generated a declarative microscope sample-holder plate:
60 × 40 × 5 mm, with a central 22 mm opening and four 3.2 mm mounting holes.
The actual HTTP fabrication job completed with CadQuery 2.6.1 in 3.325 seconds,
producing one valid solid. Its 9938.486900714375 mm³ volume matches the analytic
volume of the plate minus the five through-holes. Both STEP and STL downloaded
successfully; the 2,552-triangle CAD model was visually checked. This establishes
geometric construction and exports, not physical fit, strength or print tolerances.

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

A second render submitted through the actual app API bound the full 4NCO complex
to illustrative virion envelope placements and 1HSG coordinates to a separate
protein node. Blender completed a 1024 × 1024, 48-sample image in 56.234 seconds
using OptiX, within a 4 GiB RAM grant and 300-second deadline. It retained 22,014
4NCO atoms and 1,686 1HSG atoms in the saved input; 127 solvent atoms were hidden
from the 1HSG surface. Nine Env–Fab template instances remained after the display
budget and cutaway. Placement, membrane and capsid are morphological reconstruction.

All five advertised artifacts downloaded over HTTP with matching byte counts:
PNG, BLEND, GLB, input JSON and renderer JSON. The 13,600,384-byte GLB was visually
checked fullscreen with 1,091,365 expanded triangles. Source fingerprints, hidden
solvent counts and coordinate mappings remain in the per-job evidence. The earlier
62.594-second, 768-pixel test was a separate direct-worker validation.

## Packaging and remaining acceptance

The Windows x64 NSIS installer built and installed successfully with the existing
projects and OS-stored credential preserved. The installed app opened the retained
3D experiment with playback. An idle-engine crash test recovered a healthy native
backend automatically in 4.95 seconds. The installer is unsigned.

An earlier scientific CI run, `34530065047`, completed all three jobs successfully.
At the recorded interim packaging check, Linux x64 and ARM64 packages succeeded
while both Windows package jobs were pending. Those results apply to earlier
source snapshots. The subsequent scientific run `34531057689` and desktop run
`34531093553` for commit `44237f2` were still running when recorded. Application
commit `5a26384` includes a subsequent opacity correction with its targeted scene
suite passing. These earlier CI observations are not acceptance of that revision.

The final follow-up installation and four native package workflow results remain
pending and will be recorded after those checks finish.
The workflow uploads private CI artifacts only; it has no release publication step.
Windows ARM64 and Linux x64/ARM64 runtime acceptance are not inferred from a
Windows x64 build. NPU computation, distributed clusters, external MD/CFD/QM engines
and local open-source LLM execution are roadmap items, not verified capabilities.

Recovery tests establish bounded generation checkpoints, memory-pressure batch
reduction, retry ceilings and ownership-safe startup. They do not claim mid-step
integrator recovery, arbitrary process survival, or multi-node fault tolerance.
