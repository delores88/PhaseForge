# PhaseForge 0.8.0-alpha.1

An alpha scientific research workbench for anyone with a question, from curious
beginners to experienced researchers. More features are coming. The app is free,
without trials, paid unlocks or required developer credits. Choose OpenAI **or**
Anthropic and supply that provider's API key for AI features; provider API charges
are separate. Both keys are not required, and no additional paid AI service is needed.

## Research, experiments and 3D

- Build and revise bounded experiments, inspect measurements, and run saved setups
  without another model call. Choose models and supported reasoning per turn.
- Give a research team a time limit. Specialist agents share work, retain progress
  and prepare follow-up experiments within the session's limits.
- Retrieve public research and supported scientific assets with source URLs and
  hashes. Inspect molecular coordinates, procedural geometry and saved results
  from different camera angles.
- Render scenes with separately installed Blender Cycles on supported CPU/GPU
  devices, retaining PNG, Blender and GLB files. Build CAD solids through CadQuery
  and PCB artifacts through KiCad, with native validation before manufacturing export.
  These optional engines are free and are not bundled in the desktop installer.
- Detect local resources, admit bounded jobs and preserve checkpoints. Supported
  numerical GPU scoring and CPU replay have distinct roles; hardware inventory
  does not imply every listed accelerator executes every workload.

## Alpha preparation

The interface, application icon, installer, window, tray, shortcut and About view
use the supplied original PhaseForge artwork while preserving the existing themes.
The About view explains alpha status, audience, costs and optional engines.

The packaged backend uses an OS-assigned available loopback port, communicated
over its private launch connection. Desktop startup verifies that owned backend
using a fresh challenge and process-held secret. Development ports 3000 and 7331
are not used by the packaged engine; interface port 7332 remains stable for saved
preferences. An unrelated listener cannot become the desktop's research engine. Direct
API admission validates loopback hosts and configured browser origins. Provider
clients reject redirects, protect credential headers and redact credential-bearing
diagnostics. SQLite is updated to 3.53.2 with runtime identity and preservation checks.

The desktop pins Electron 45.0.0-alpha.6. Its exact upstream source includes the
reviewed ANGLE and V8 fixes that blocked the previous runtime. This is an alpha
runtime; its source review does not replace package security review or native
acceptance. Publication proceeds in order: Windows x64, Apple Silicon macOS ARM64
DMG, then Linux x64 AppImage. Intel and universal Mac packages are excluded.
Platform signing accounts are not required by this project; the Mac app uses
ad-hoc signing without Developer ID or notarization. Marketplace provenance and
security review remain separate checks.

## Scientific scope

A detailed rendering is not an electron micrograph or a completed scientific
experiment. Structural coordinates and declared morphology can inform geometry;
drug binding, therapeutic efficacy, general relativity, fluid dynamics and quantum
mechanics require appropriate validated methods and data. The included solvers
currently execute bounded ODE systems and classical particles. A valid CAD solid
or a PCB that passes DRC still needs physical or electrical engineering validation.

Cluster/DGX orchestration, distributed GPU execution, local open-source LLM
inference and dedicated NPU numerical kernels remain roadmap work. Existing saved
projects and OS-managed provider keys should be preserved when upgrading. See
[the baseline validation record](docs/RELEASE_VALIDATION.md) and the candidate's
native acceptance, runtime inventory and marketplace receipts for the exact scope
of testing. A GitHub asset or successful build alone is not marketplace acceptance.
