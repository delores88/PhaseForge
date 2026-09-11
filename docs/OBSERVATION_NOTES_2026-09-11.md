# PhaseForge: local app observation notes — September 11, 2026

Observation began at 06:52:34 America/Denver (12:52:34 UTC). This is an observation record, not a change proposal or a claim that the scientific models have been independently validated.

Observation ended at 07:09:37 America/Denver (13:09:37 UTC), after 17 minutes and 3 seconds including verification and note-taking. The final API check still showed 12 numerical runs, 45 provider calls, and no active calls or requests. Git status showed only this new notes document. The app was returned to the viral-envelope project's 3D & fabrication view.

I opened the running installed Windows app, inspected all eight saved projects, played existing viral and electron trajectories, and inspected Results, Agents, 3D & fabrication, Runs, Compute, and Usage & cost. Screenshots and accessibility text were inspected directly. Backend inspection used local read-only GET requests, a read-only SQLite connection including its committed WAL state, and existing artifact files. No new model requests, numerical runs, renders, installs, or code fixes were initiated.

The installed app reports 0.8.0-alpha.1. Its build record declares source commit 7471ff72 and build time 2026-09-11T00:29:13.760Z. The repository is at c3843dc1. The relevant chat, Markdown, session, findings, styling, and agent review files were compared against the declared installed source and have no differences; the chat resizing rules were also confirmed directly in the installed CSS.

**Overall observation**

There is real execution and retained numerical evidence here. The problem is the distance between the requested scientific work and the small models that actually execute, combined with an interface that makes both the work and that distance difficult to understand. The records do not support blaming these failures mainly on user prompting or on a full model context window.

**Saved work inventory**

The active database is `%LOCALAPPDATA%/PhaseForge/PhaseForge/data/phaseforge.sqlite3`. It contains 8 projects, 67 chat messages, 11 manifests, 12 numerical runs, 8 saved run analyses, 45 provider-call records, and 1 durable agent task. Five completed Blender jobs also exist in the artifact filesystem, separately from the numerical-run table.

All 12 numerical runs are recorded as `state_vector_ode`, RK4, `Rayon CPU / f64`. This describes the recorded numerical backend; it does not mean GPU rendering is absent or that the runtime cannot accelerate compatible candidate scoring. The live capability API advertises two numerical families: state-vector ODE and pairwise particles. No stored run in this inventory demonstrates molecular dynamics, docking, ML training, or a domain-specific fluid solver.

| Project | What I saw and what the records establish |
|---|---|
| Viral envelope · 3D encounter study | A detailed white cutaway viral structure in Studio, and a separate colorful virus with three small moving scaffold representations in Experiment. Its one numerical run took about 0.174 seconds: 12 states, 400 integration steps, 201 retained frames. Equations generate helical relaxation and measure proximity. The saved manifest explicitly describes synthetic trajectory generators and excludes molecular force laws, Brownian dynamics, binding, reactions, and membrane deformation. |
| visualize an electron's wave state collapsing | A dark Bloch sphere with small colored markers and trails, 251 frames over five scaled-time units. The four-state calculation took about 0.057 seconds and 500 steps. It represents idealized conditional no-click localization and unconditioned decoherence, not a spatial electron wavefunction or an ensemble of random detector histories. Labels are largely hidden in the object drawer and truncated there. |
| Blackhole Simulation | The broad request for everything within 5,000 light years ultimately produced a comparison between two counterfactual evolutions of a hypothetical massless tracer. Latest saved run: 12 states, 10,000 steps, 501 frames, about 2.25 seconds. The scene initially appears small and low in the viewport; large translucent extent markers dominate its scale. No numerical stellar catalog was retained. |
| HIV Cure | Earlier within-host model and later four-arm hypothetical model. The first run finished computationally but failed both baseline constraints and all three overall challenges. Later runs passed their authored checks. The current legacy visualization is a single generic state marker, which does not visually explain the biological compartments or four treatment arms. |
| Solve the 3 body problem | Four numerical runs, 12–24 states, approximately 12,652–50,000 steps and 64–320 seconds. These contain substantive classical integration and numerical checks. Latest scene displays clusters of small markers, including nominal body/radius markers, rather than three physical colliding solid bodies. Saved chat reports unsupported collision and portfolio requirements and a later concurrent-call-limit error. Historical capability statements in that chat should not be mistaken for current capability discovery. |
| Please build out a simulation and hypothesis to test against solving for | The original planar figure-eight study. Only one state marker appears. Its saved visualization explicitly maps x1/y1/0 and has no explicit entity list, so it displays one body's trajectory despite integrating the three-body system. |
| Find a drug that can cure HIV | No accepted experiment. Two replies report that the available runtime cannot identify or validate the requested drug. The page still prominently offers generic molecular/structural scene examples. |
| CAD / PCB native validation · 8263e8c3 | A software acceptance workspace. Studio displays a saved sample-holder solid and STEP/STL links; the app reports Blender, CAD, and KiCad ready after loading. This is useful artifact execution evidence, separate from a scientific experiment. |

The live `/api/scientific/engines` response reports GROMACS, OpenMM, CP2K, xTB, LAMMPS, and Open Babel unavailable to this running backend. That is a discovery result for this app environment, not a filesystem-wide assertion that these tools cannot exist elsewhere.

**The HIV animation is numerically driven, but the numerical question is very small**

The moving scaffold centers are linked to retained solver positions. Calling the whole thing a fabricated animation would therefore be inaccurate. However, those positions come from authored synthetic transport equations. A visually detailed viral envelope does not participate in binding or molecular mechanics.

Results show near-surface times of about 8.308, 5.260, and 0 scaled-time units, and closest surface gaps of about 0.115, 0.275, and 0.503 scaled-length units. These are legitimate outputs of that specified toy model. They do not measure antiviral effect or binding. The saved objective itself explicitly requested conceptual transport, so this workspace documents a demonstration rather than delivery of the broader scientific ambition discussed with the user.

The three imported molecular records include a 1,686-atom protease and two records of the 22,014-atom 4NCO Env/Fab complex. They were imported hours after the viral numerical run. Their later presence supplies geometry to rendering, not evidence that the earlier dynamics used their atoms or a force field.

**Blender runs, but its integration explicitly produces stills**

The latest saved Blender job, `58d22bdd-1d4f-41a8-808c-97ea28487ad4`, reports Cycles 4.5.9 LTS, OPTIX on the RTX 4090 Laptop GPU, 1024 × 1024 pixels, 64 samples, 514,282 vertices, and about 45.343 seconds. Its visible model viewer reports 1,091,365 triangles. These are different geometry measures, not a discrepancy by themselves.

The output files are PNG, BLEND, GLB, and metadata/input files. The saved request has scene/structure bindings and no simulation run ID or trajectory. The executed worker calls `render(write_still=True)` and exports GLB with `export_animations=False`. One earlier 512-pixel render fell back to CPU after a GPU out-of-memory error. Rendering detail is therefore both real and independently budgeted, while the trajectory-to-animation connection is absent from this job.

Evidence: `%LOCALAPPDATA%/PhaseForge/PhaseForge/data/artifacts/studio/58d22bdd-1d4f-41a8-808c-97ea28487ad4/renderer.json`, `input.json`, and `worker.py` (lines 553–563).

**Chat and report presentation: directly witnessed problems**

- The HIV explanation visibly leaves literal `**` around a bold phrase containing the unit `scaled viral load*day`. Other bold and inline-code spans render, so support is inconsistent rather than entirely absent.
- The renderer is a small handwritten parser. It supports inline backticks, bold, fenced code, headings 1–3, and flat unordered bullets. It does not implement links, tables, math, blockquotes, ordered/nested lists, images, or full nested inline Markdown. A scientific discussion readily exceeds that subset.
- In Agents, opening “4 saved research artifacts” reveals literal `###`, `**`, and mathematical notation in a 10-pixel monospace box with a 240-pixel height limit and its own scrollbar. This bypasses the chat Markdown renderer altogether. A report is difficult to read even after finding it.
- “Explain this in plain language” in viral Results is one long paragraph full of variable names and values such as `11.999999999999936`. There are no digestible sections or first-order conclusion. A session review is stored in the same analysis slot as the plain-language explanation, even though the review prompt asks for exact metric names, values, and units.
- Electron follow-ups visibly contain `Current viewport inspection ... {"inspection":null}`. Internal context is appearing in the user's conversational text even when there was nothing selected.
- The viral task's stored user-role message is an 8,918-character coordinator prompt and serialized packet, with only two actual newlines and 43 literal backslash-n sequences. The interface usefully substitutes a shorter task objective in that case, but the stored conversation and the visible one are different representations.
- The final Builder reply in the viral chat is primarily about repairing an invalid identifier. The useful scientific interpretation and next step live in Results and Agents, so the conversational endpoint does not explain what the work accomplished.
- Many project titles, scene-object names, and lineage entries truncate in narrow regions. The default dark interface gives secondary explanations and controls very small, low-contrast text. The central viewport, chat, artifact boxes, and lineage form several independent navigation regions.

References: `frontend/src/components/workspace/ChatMarkdown.jsx:4`, `ResearchSessions.jsx:36`, `FindingsView.jsx:39`, `ResearchWorkbench.jsx:397`, and `frontend/styles/forge.css:9`.

**Chat expansion is broken at the observed window size**

At a 1,529-pixel window width, clicking expand changes its accessible label from “Expand chat to 60%” to “Restore chat width” without moving the divider. The installed CSS imposes `max-width:40%` above 1,500 pixels, so both requested widths, 42% and 60%, are clamped to the same size. Another rule forces 34% below 1,200 pixels. This is a reproduced layout issue, not an opinion about preferred spacing.

Reference: `frontend/styles/forge.css:10`, also present in installed `resources/ui/_next/static/chunks/00jpty0m5-y-7.css`.

**An explanatory question triggered another simulation**

This is the clearest example of the interface working against the user's intent. In HIV Cure on September 8:

| Local time | Saved event |
|---|---|
| 15:34:22 | User: “In plain english was there anything useful in this run or not.” Request records `study_intent=experiment`, `auto_run_requested=true`, and Builder role. |
| 15:34:35 | Initial proposal rejected because experiment mode required a runnable manifest or capability gap. The rejected response itself is not retained here, so its exact wording is unknown. |
| 15:36:08 | Repair creates revision `bda0734b...` and submits run `0fa58650...`. Assistant metadata says `should_run=false`, but execution authority is recorded as an explicit user action. |
| 15:37:59 | The resulting numerical run finishes after approximately 111 seconds. |

The current code preserves mode and Build & run state through messages. Experiment mode rejects an explanation-only action. The execution condition allows the retained auto-run flag to override `should_run=false` in direct experiment mode. The record proves the execution sequence and flags; it does not establish precisely when the user toggled that historical checkbox.

References: `frontend/src/components/workspace/ChatDrawer.jsx:27` and `:89`, `backend/src/experiment/mod.rs:39`, `backend/src/agent/mod.rs:954`.

**The saved failures distinguish output exhaustion from context exhaustion**

The 45 provider calls comprise 32 completed calls, 5 invalid proposals, 7 output-limit errors, and 1 cancellation. All seven output-limit errors used gpt-6-astra and collectively consumed 84,000 output tokens over approximately 34 minutes of recorded call duration. This is a small local sample with different workloads; it is not a model-quality benchmark.

Four failed electron calls report their entire 12,000-token output allocation as reasoning, with inputs of 7,920–8,111 tokens. They leave two failed chat exchanges, each with a proposal and repair. The visible timestamps show roughly nine-minute waits before each final error. The later electron request succeeds with 11,135 output tokens, close to the same ceiling.

The current settings remain 12,000 output tokens, one repair, and one simultaneous provider call. Those settings were confirmed through the app and `/api/usage`. Thus some experiences described as context collapse are conclusively output-budget exhaustion. This does not rule out separate continuity loss in other circumstances.

Actual continuity loss is also evidenced: the retained viral task packet clips a specialist contribution mid-sentence at “Run one candidate, no ”. Chat history is limited to 24 messages; task briefings select six recent artifacts and cut each to 1,800 bytes. Full artifacts remain saved but are not automatically accessible to the model as tools in this path.

All 45 call records are unpriced; no rate cards are configured. Usage correctly labels its main cost subtotal “Unpriced,” although model rows still show a $0 priced subtotal alongside the unpriced count. No reliable dollar total can be inferred. One cancelled call lacks final reported usage.

**Agent activity is stored and exposed incompletely**

The only saved task has one specialist, one cycle, one numerical run, and four artifacts. It took approximately 5m33s overall while its numerical calculation took 0.174 seconds. The task card says Completed while its horizontal bar is only partly filled, because the bar measures elapsed time budget, not completed work. The clock shows remaining time, 00:04:26, rather than elapsed work.

The child card shows “Scientific designer — completed”; substantive output is under the artifact disclosure. The UI attempts to display child summary/output/message fields that the backend child type does not supply. It does not display the available child timestamps or cycle next to that card.

Source inspection confirms the chat awaits a whole response, rather than streaming assistant text. Its progress indicator polls one current phase string every 1.5 seconds; Agents polls task snapshots every three seconds. Existing WebSocket events concern numerical runs. Provider requests do not request a visible reasoning summary, and this path does not expose model-directed tool calls. There is no durable, unified log of tool actions, intermediate findings, or agent decisions.

User-facing work summaries and tool receipts would address the desire to see what agents are doing. These observations concern those useful signals, not access to private chain-of-thought.

No agent call was active during this walkthrough. I did not start a paid request to test live generation, so streaming/progress behavior here is established by code inspection and stored outcomes, not a freshly observed live call.

References: `ChatDrawer.jsx:70`, `ResearchWorkbench.jsx:432`, `ResearchSessions.jsx:30–36`, `backend/src/agent/tasks.rs:59`, `backend/src/agent/providers.rs:260`, `backend/src/api/mod.rs:635`.

**Selecting an old run produces mixed context**

The Runs table labels every numerical run Completed, including the HIV run whose scientific constraints failed. Its status means execution finished; no scientific verdict column is visible there. Opening that old run correctly shows the selected run and a warning that the active revision is different. However, the viewport heading still uses the newer active experiment's title, and the adjacent chat remains at the newer run's interpretation. The user is simultaneously looking at an old run, a new experiment title, and a later conversational conclusion.

Results does correctly show the old run's red “At least one declared check failed,” with zero passing and two failing constraints and three failing challenges. The underlying evidence was not replaced. The viewport title fallback selects the active manifest title even when its numerical frames come from another run.

References: `GenericViewport.jsx:55`, `ResearchWorkbench.jsx:713`, `:738–739`, and `:748–758`.

**What is working and what remains unestablished**

The black-hole Research tab exposes a further mismatch: a source section labeled Europe PMC, a query labeled `Sagittarius A* orbit`, and retained hits including the Sagittarius dwarf galaxy, `Sagittarius, n.`, and `Sagittarius, Kaspar`. Some entries are relevant to astronomy, but the collection is not a calibrated Galactic-center dataset. Stored searches include three initial zero-result attempts; the only imported astronomy asset identified is a Hubble Sagittarius JPEG. The latest model reply correctly acknowledges that no usable stellar phase-space catalog was supplied. Retrieving source metadata and gathering simulation-ready inputs are not the same completed action.

The app reconnects to a healthy local backend and retains enough evidence to reconstruct specific errors and actions. Recorded replay, metric cards, plots, failed-check reports, run lineage, real Blender output, and CAD artifact inspection work. Compute explicitly distinguishes browser graphics, compatible GPU scoring, and CPU final trajectories, and shows one numerical run slot. These are useful foundations.

The records do not demonstrate AI looking at rendered images: current viewport context is structured selection, coordinates, camera, and provenance appended as text, not a frame image sent to the model. They also do not demonstrate independent empirical validation, domain-engine molecular simulation, sustained multi-cycle research, or recovery of a long active agent task through a restart. None of those were tested or repaired during this observation.

Read-only API surfaces inspected include `/api/health`, `/api/projects`, `/api/runs`, selected manifests, `/api/usage`, `/api/capabilities`, `/api/hardware`, and `/api/scientific/engines`, through the installed app's loopback service. Counts and findings cover this active local database and inspected artifact directories, not backups or other installations.
