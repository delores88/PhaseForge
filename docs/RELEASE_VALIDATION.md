# PhaseForge 0.8.0 — observed validation

Date: 2026-09-10. Unreleased development build. Historical reports are retained in
`docs/history/`; their unexecuted or static checks are not current runtime evidence.
Application/package source: `44c9026677464bfe1e8096e19243dc6b0db8032c`.
Its saved-source UI fix has passed live rerender validation using the unchanged
backend from `6123022`. The final Windows x64 installation has passed native
acceptance. Scientific CI for this exact application revision passed.

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
| Frontend Node tests | 38 passed | Model intake, molecular surfaces, procedural scenes, lifecycle, conversation presentation and seven saved-source selection tests; scene/presentation counts below are subsets |
| Procedural scene and lifecycle tests | Scene suite: 17 passed; resource suite: 2 passed | Four preset geometries, disposal, budgets, mesh validation, trajectory binding and opacity |
| Research conversation presentation | 3 passed | Team attribution, retained audit data, legacy session display and ordinary user text |
| Desktop tests | 7 passed | Static proxy, host/origin checks, traversal, interrupted upstream connection, scoped fullscreen permissions and policy packaging |
| Combined frontend and desktop Node run | 45 passed | 38 frontend plus 7 desktop; these are the same tests listed above, not additional checks |
| Process cleanup test helpers | 3 passed; isolated native runtime HTTP smoke passed | Test-only cleanup commit `332aea9`; no application changes after the `44c9026` package |
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

Two earlier attempts at the original request to gather nearby-object data and
simulate everything within 5,000 light years of Sagittarius A* reached the
provider's output ceiling without an accepted manifest or numerical run. A first
successful follow-up explicitly narrowed the prompt to one illustrative S2-like
test star around a fixed central point mass. A later installed-app retest of the
unchanged original prompt also produced a completed bounded model, as recorded
below. Neither executable stage constitutes a complete observed-object census.

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

The unchanged original prompt was then retested on installed commit `6123022`
using OpenAI GPT-6 Astra/medium, research mode enabled and the existing 12,000-token
output ceiling. It produced accepted manifest `aa333c09-91d6-44e4-a0c7-58337728fc08`
and completed run `74bd5f2e-0915-4060-85bd-51ae0c67e0f0`. This time the model selected
an executable interpretation: compare one hypothetical tracer under black-hole-only
gravity with the same tracer under black-hole-plus-Hernquist-bulge gravity, inside
a diagnostic 5,000-light-year sphere. The two trajectories are alternative model
evolutions, not catalogued stars or interacting bodies.

The 12-state CPU/f64 RK4 system completed 10,000 steps over 50 simulated Myr in
2.250 seconds, retaining 501 frames and five scene nodes. Both energy-conservation
constraints passed. A resolution ladder at half and quarter time steps passed all
six declared comparisons; ±20% bulge-mass sensitivity trials passed all three
declared comparisons. The hypothetical tracer spent 9.192 Myr inside the sphere
under point-mass gravity and 50 Myr with the assumed bulge. These are conditional
model results, not observations. The recorded research turn completed two catalog
searches and attached one asset, but those materials supplied no usable stellar
phase-space census or observational calibration. The installed workflow now turns
the original broad prompt into a runnable, explicitly bounded experiment; the
historical output-limit failures and missing population data remain disclosed.

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

The saved-source correction was then reviewed and passed the frontend production
build. Rerendering a saved job resolves that job's original scene, structure,
bindings and recorded camera instead of borrowing the current experiment scene.
The UI waits for a matching saved request and blocks rendering when it cannot
recover that source. Imported models, imagery and engineering artifacts no longer
fall through to an unrelated Blender scene. Selection changes cancel pending
saved-input reads, and stale molecular loads cannot replace a newer selection.
Seven source-selection tests cover these source contracts and bounded input reads.
The independent JSX handler check passed all 19 checks, and the source audit
passed all 122 gates. The final UI was then exercised through its actual
“Render in Blender” control: selecting saved job
`fa147773-4cfe-42fd-8e33-0dc174d656c6` created job
`0a365339-207c-409e-b8f6-6820741db6de`. Comparing the two saved requests confirmed
exactly equal scenes, molecular bindings and camera values. The original has two
scene nodes and a null explicit camera; this check preserves its automatic framing
choice rather than claiming a tested custom-camera rerender.

That UI-created job completed at 1024 × 1024 and 64 samples in 57.328 seconds on
the RTX 4090 through OptiX, with no fallback. All five artifacts downloaded with
exactly the advertised byte counts and retained SHA-256 checksums. The GLB was
13,600,384 bytes; PNG was 1,140,698 bytes. This verifies the final UI's saved-source
handoff to the native engine. The replacement installed app subsequently loaded
the completed job and recovered its original source, with the render button enabled.

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

Installed commit `6123022` also ran fresh native jobs through the desktop proxy.
The saved AI-designed plate completed again in 2.972 seconds with one valid solid
and the same volume and STEP/STL outputs. A 512 × 512, 32-sample 1HSG render first
attempted OptiX, which reported GPU memory exhaustion. The worker retried on CPU
and completed in 5.109 seconds within the original 180-second deadline and 2 GiB
RAM grant. All five render artifacts downloaded through the desktop proxy. This
is an observed bounded GPU-to-CPU recovery; it does not establish recovery from
every graphics-driver crash or arbitrary memory failure.

## Packaging and native acceptance

The Windows x64 NSIS installer built and installed successfully with the existing
projects and OS-stored credential preserved. The installed app opened the retained
3D experiment with playback. An idle-engine crash test recovered a healthy native
backend automatically in 4.95 seconds. The installer is unsigned.

The `6123022` installer received a further local acceptance check: seven saved
projects and the existing credential remained available, the OpenAI model list
contained 71 entries, and the installed 3D viewport entered and exited native
fullscreen after the scoped permission fix. The recommended Astra/max setting
was selected without an additional model-generation call. This interim installer
was 117,740,097 bytes with SHA-256
`E95F37B0678757A5A7E86F635BD1BB15BF1EBE3231796C2247478F74489260BF`.
That interim package predates the saved-source correction; its checksum is retained
as historical installation evidence.

The replacement Windows x64 package was built from clean commit
`44c9026677464bfe1e8096e19243dc6b0db8032c`. It is 117,741,558 bytes with SHA-256
`2884703579B9E921DB7B5E451A5CDA3944D15BA8C603B0CC75FA8B377DA9ACBE`.
The visible installer completed successfully. Installed `build.json` identifies
`44c9026` with a clean source tree, and the bundled backend hash matches the checked
binary. The installed application owns the local API on port 7331 and serves its
desktop proxy on port 7332. Seven saved projects, the existing credential and 71
OpenAI model entries remained available. Studio automatically opened the latest
rerender with 1,091,365 expanded triangles; its original input was recovered and
the render control became available. Fullscreen entry/exit, orbit and zoom were
visually checked in the native application.

Scientific CI run `34532211210` for `6123022` completed successfully, including
browser checks and Windows/Ubuntu backend tests with native HTTP smoke programs.
Desktop workflow `34532259964` for the same source also completed successfully:
Linux x64, Linux ARM64, Windows x64 and Windows ARM64 each built on their native
runner and uploaded an artifact archive. Archive IDs, sizes and checksums are
retained in the JSON record; they describe CI download archives, not the local
installer checksum above. These four package results apply to `6123022`, while
the final `44c9026` Windows x64 package has the separate local acceptance above.

Exact-revision scientific run `34533754223` for `44c9026` completed successfully
at 21:54:42 UTC: browser, Windows backend and Ubuntu backend jobs all passed,
including the Windows native HTTP smoke programs. There are no backend source
changes from `6123022` to `44c9026`. Subsequent commit `332aea9` changes test cleanup
and its CI coverage only; it does not change the installed application.
The workflow uploads private CI artifacts only; it has no release publication step.
Windows ARM64 and Linux x64/ARM64 runtime acceptance are not inferred from a
Windows x64 build. NPU computation, distributed clusters, external MD/CFD/QM engines
and local open-source LLM execution are roadmap items, not verified capabilities.

Recovery tests establish bounded generation checkpoints, memory-pressure batch
reduction, retry ceilings and ownership-safe startup. They do not claim mid-step
integrator recovery, arbitrary process survival, or multi-node fault tolerance.
