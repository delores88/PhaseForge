# PhaseForge: simulation fidelity and workbench usability

Requested 2026-09-12. Baseline: installed/accepted 0.9.1, source
`27f5c91ac6482df16c34012c7f56501a1f494f32`.

Work in descending technical difficulty. Local deployment and the Windows
marketplace release are last. An implemented component is not a verified user
workflow. Keep failures and remaining scientific limits in the evidence ledger.

Progress is the sum of completed task weights below, not an estimate of time
spent. Partial task progress is reported separately. Completion updates go to the
existing PhaseForge progress-monitor agent, with completed and remaining percent.

| ID | Weight | Task and acceptance | Status |
| --- | ---: | --- | --- |
| B1 | 25 | Enforce deliverable intent across chat, steering, tools and completion. Simulation requests must produce retained, playable numerical evolution with an appropriate model, or an explicit capability gap. Stills cannot fulfill simulations. Explicit illustrations and presentation-only edits remain supported. Regression includes the saved black-hole case and switching request types in one conversation. | Complete |
| B2 | 20 | Provide real bounded thermal/fluid and relativistic engines, hardware-aware admission, retained measurable results and dependable stopped outcomes. Unsupported models, insufficient resources, numerical instability and persistent tool failures must stop expensive work, preserve evidence and offer concrete choices. Do not claim scientific success from an invalid run. Exact 0.999c collision success is not required for release. | Complete |
| B3 | 15 | Visible, functional pan/zoom/fit controls for stills; pan/orbit/tilt/zoom/reset for interactive 3D and numerical viewers. Playback uses retained data. Test actual control effects and preserved presentation without solver reruns. | Complete |
| B4 | 10 | Restore horizontal iteration cards below the viewer. Chronological progression goes left to right, newest at the far right and initially visible. Selecting prior iterations works, retains lineage, and is not overridden by background refresh. | Complete |
| B5 | 8 | Map visible actions to project/run/result and job ownership. Results, equation experiments and 3D/fabrication must expose applicable saved outputs and actionable unavailable states; historical workflows remain reachable. Show failed/partial result status prominently. Navigation never starts or cancels work by itself. Remove only irrelevant controls. | Reopened |
| B6 | 5 | Inbox-style unread state: opening completed project/job progress clears its applicable blue dot durably; later completion becomes unread again. Preserve truthful running indicators. | Complete |
| B7 | 3 | Use the existing PF symbol with genuine transparency for marketplace discovery, without a rectangular thumbnail background. Preserve two actual app screenshots on the detail/download page only. | In progress |
| B8 | 7 | Integrated regressions and native workflow verification: intent switches, supported simulation versus unsupported merger, playback/camera, lineage, view navigation and read state. Completed legacy equations, solver simulations and numerical analyses must surface their exact results after completion, navigation, reload and history entry. Record failures and exact build identity. No redundant studies or broad scan cycles. | In progress |
| B9 | 7 | Back up and install the verified Windows update locally, preserve user projects/jobs, then package and pass ordinary Delores marketplace admission with the final artwork. Coordinate exact immutable release evidence with the marketplace owner. | Queued |

**Overall: 83% completed / 17% remaining.** Corrected dependable-engine and actual workflow acceptance passed. Remaining work is the exact packaged Windows verification, local installation and marketplace admission/artwork.

**Current user correction, 2026-09-12 17:24 UTC:** deliver dependable research software, not a validated discovery on one extreme collision. Further collision calibration, parameter and horizon research is deferred. Retain the real engine integration and honest scientific boundaries. Complete actual supported success, stopped failure with concrete options, and navigation/reopen/recovery workflows; then back up/install locally and pass ordinary Windows x64 marketplace admission. No further shipping-scope confirmation is required.

**Delivery status: 0.9.1 remains installed. No packaged 0.10 installation or
marketplace admission has occurred.** "Complete" in B1/B3/B4/B6 means the
component and described workflow passed in source/test environments, not that
the user has received the update. B8/B9 contain the remaining installed-build
and delivery checks. At 2026-09-12 13:18 UTC, isolated source UI 7442 used its
own data profile, the temporary native Electron test host was closed, and UI
7443 read original records through a proxy that blocks mutations. Exact receipt:
`.local/validation/workbench-010/current-environments-20260912T131804035Z.json`.

Historical scope correction (superseded by the current correction above): the original interpretation required an actual relativistic collision.
The earlier 83% report incorrectly closed B2 after bounded thermal/fluid work and
honest reporting of the missing relativistic engine. B2 is reopened in full under
the same all-or-nothing task-weight method; the verified thermal/fluid subwork is
preserved. That exact-collision requirement no longer gates release. Results, lifecycle, local installation and ordinary marketplace acceptance still do.

## Evidence and decisions

- User-workflow acceptance now requires the ordinary chat path to honor the
  requested output type, execute the appropriate supported calculation, expose
  its result in an obvious location, provide useful inspection and a grounded
  explanation with next steps, preserve state through navigation/recovery, and
  handle a fresh unseen variation within its claimed scope. Component builds,
  selected examples and test counts alone do not establish product reliability.
  Apply this to the specific changed workflows, without an open-ended optional
  testing cycle. Existing completion weights are not reliability percentages.

- Engine-selection constraint: prefer portable scientific engines and data
  contracts across Linux x64/ARM64, Windows x64/ARM64, and macOS Apple Silicon.
  Upstream source support, available binaries, GPU backends, app integration and
  tested packaging are separate facts. Preserve replaceable execution adapters;
  the initial AthenaK WSL prototype must not define the universal app runtime.
  The current release remains Windows x64 only. Additional platforms need their
  own native dependencies, isolation/resource enforcement and acceptance; no
  multi-platform release is implied, and future-platform acceptance must not
  delay the Windows release. See `docs/validation/engine-portability-2026-09-12.md`.

- Initial source inspection confirms camera controls currently hide inside a
  collapsed disclosure. The illustration viewer initially selects its still
  image, and modern lab outputs are collected under Scientific lab.
- The existing project sidebar computes unread state across completed jobs but
  selecting a project does not itself acknowledge them. Verify the full update
  path before changing the behavior.
- Saved black-hole project `3a8b52bc-5669-417c-a837-f4cf5d12a1d2`
  includes generated incoming-field calculations. Job
  `925a7791-0009-4026-a4eb-d9c9d894392c` requests an HTML replay;
  `78ec44da-cdcd-4a7a-ac3b-83e0849c03da` explicitly excludes solving
  the merger. Neither job title establishes delivery of the user's requested
  collision simulation. Inspect retained tool and artifact evidence.
- Full numerical relativity, arbitrary thermodynamics, turbulent/multiphase
  flows and universal wet-lab replacement are not implied by a bounded adapter.
  Capability limits must be visible before and after execution.

## Completion ledger

- Final source checks: 408 backend tests passed (11 optional tests ignored,
  with relevant rendering/readiness checks recorded separately); 145 frontend
  tests and production build passed. Final full-page Results and timer checks
  are recorded in `docs/validation/workspace-results-2026-09-12.md`.
  Release helpers passed 73 JavaScript tests (6 platform-inapplicable skips)
  and 63 Python tests. The first Python runner lacked the release-only psutil
  dependency; the development interpreter passed without changing app runtimes.
  New hosted acceptance requires numerical heat/flow arrays, playback and
  restored hashes, but has not yet run on an exact 0.10 installer. Source
  verification does not close B8/B9 or the integrated software workflow criteria.

- Current release decision: complete dependable ordinary-user workflows and
  their packaged Windows acceptance, then install and publish. A scientific
  capability gap is acceptable when the app stops cleanly, preserves evidence
  and offers concrete alternatives without silently substituting the requested
  physics. Coarser grids never imply validity merely because they fit hardware.
  The app must discover current CPU, RAM, GPU/VRAM and scratch capacity, propose
  an explicit experiment budget, enforce it during execution, and use measured
  pilots to establish attainable resolution. Hardware inventory alone is not
  a throughput estimate or proof that a numerical method is valid.

- Scientific limit retained: an equal-mass head-on 0.999c collision remains
  unvalidated. Further calibration research is deferred. Neither a precontact
  approximation nor a still is labeled as a solved collision.

- Current software blockers found by actual workflow and code inspection:
  recovery kept141 navigation GETs responsive but its POST timed out at30s;
  a selected failed gauge result lacked a prominent failure warning; ordinary
  timer-Off sessions lacked a deterministic repeated-tool-failure stop; parent
  resume blindly attempted unsupported NR checkpoint continuation; unchanged
  capability gaps exposed a Resume action that could buy another model turn.
  These are the active fixes. Ordinary saved-result explanation (including
  compaction) and quantitative Analysis already passed on actual CUDA evidence;
  see `docs/validation/nr-ui-results-2026-09-12.md`.

- Relativity foundation: pinned AthenaK was built in the existing WSL Ubuntu
  and ran three bounded harmonic gauge-wave benchmarks. The preregistered
  refinement check passed with roughly fourfold error reduction per doubling;
  the coarse-grid absolute-check failure stays recorded. This is an actual
  Einstein-equation implementation check, not a black-hole collision or a
  physical gravitational wave. No new collision capability is advertised.
  See `docs/validation/relativistic-compute-feasibility-2026-09-12.md` and
  `docs/validation/relativistic-solver-path-2026-09-12.md`.

- B4 complete: actual React/Electron interaction checks verify newest-at-right initial scroll, selection, append and polling preservation. Evidence: `.local/viewer-acceptance/controls-20260912/interaction-report.json`; frontend production build and 116 tests passed. Full-workbench layout is checked again under B8.

- B3 complete: 68 actual interaction assertions cover visible controls, native field/particle/still components, real camera changes and layout at 320/520/1000px. Fixed narrow-pane footer overflow. Immutable evidence for both B3/B4: `.local/viewer-acceptance/controls-20260912/runs/2026-09-12T11-43-16-113Z/interaction-report.json`. Full-workbench verification remains B8.

- B2 thermal/fluid subwork complete (the original full-task completion report was corrected above): actual heat and flow jobs completed through the normal API;21 retained states loaded and played in the browser. Independent numerical, decoder, Blender export and explicit Rust readiness checks passed. See `docs/validation/continuum-results-2026-09-12.md` and `.local/validation/workbench-010/actual-fields-status.json`. Development seed cache contamination was correctly refused; the isolated profile was restored from unchanged verified installed runtime bytes. The responsible test subprocess now disables bytecode.
- Full model integration exposed a repeated contract-resolution loop despite passing focused tests. Its repair is undergoing actual saved-session recovery checks; B1 remains in progress.

- B5/B6 complete: distinct useful tools and notes open while agents/solvers run; static numerical JSON is readable with verified original bytes and no inert image controls. Project click clears unread completion, stored read timestamps survive backend restart, and a later session completion restores unread until opened. Receipt: `.local/validation/workbench-010/navigation-read-verification.json`.

- B1 complete: actual provider requests verify illustration, status steering, numerical analysis, native publication and an explicit unsupported-merger gap. The same UI conversation switched its selected Blender still to a new heat simulation with 21 retained states. Initial application-instruction contract looping was found and repaired; interrupted provider receipts remain preserved without automatic replacement calls. Evidence: docs/validation/workbench-010-results-2026-09-12.md. Full backend library suite:400 passed,0 failed,11 ignored; optional published rendering and continuum readiness were separately run.

- Additional B8 report: completed Results did not consistently appear. Source inspection found legacy equation outputs omitted from default lab/lineage, a chat findings link tied to stale selectedRun, and run deep links that loaded records without opening measurements. A functional exact-run results navigator and routing/selection regressions are required before source freeze. Original retained data stays untouched.

- Final B2 acceptance: the combined backend passed508 tests (12 explicitly ignored). The real saved capability-gap UI offers capabilities, exact evidence and draft-preserving revision with no unchanged Resume. One same-job recovery returned202 in16ms, completed in46.04s, and kept212 concurrentGETs at63ms maximum/47ms p95. Original scientific output and deadline remained unchanged. See `docs/validation/nr-background-recovery-2026-09-12.md`.

- Final B5 acceptance: actual legacyResults opens all3 Blackholeoutputs by exactidentity; historicalHIVreplay keeps its own title; failedpartialoutputs show their status; saved-gap nextactions work. DedicatedResultsreview b434de80 completed without another solver, while navigation remained usable and the completion dot cleared durably on opening its project. The explanation was lengthy; that is recorded as polish rather than another release-blocking scientific study.
