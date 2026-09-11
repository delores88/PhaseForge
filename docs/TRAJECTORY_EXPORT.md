# Numerical trajectory viewing and export

The laboratory viewer and `tools/trajectory_render.py` read the same retained scientific coordinates. Presentation changes reuse them. Neither renderer imports OpenMM, integrates a force law, modifies an input parameter, or starts a solver. Scientific durations are measured in the trajectory index's units; movie durations are measured in playback seconds.

## Artifact contract

The source job contains `topology.json`, `trajectory/index.json`, and immutable JSON chunks. The topology binds stable entity IDs to element, display radius, mass, coordinate units and periodic cell. Index entries carry chunk paths, frame/time ranges and scientific SHA-256 hashes. JSON frames contain actual step, time and entity coordinates. Authoritative numeric arrays and measurements remain in the solver job.

The Three.js viewer keeps a bounded LRU display cache (normally at most four chunks / approximately 32 MiB). It can seek to any indexed time and refresh a growing live index. It does not truncate the saved trajectory to a fixed number of frames. Malformed coordinates, duplicate IDs, unordered times and invalid chunk paths fail explicitly.

Exact recorded state is the default. Optional smooth display uses linear interpolation with minimum-image periodic displacement and rewraps into the declared cell; without a periodic declaration it uses ordinary interpolation. This is an appearance operation, with source timestamps and interpolation flags retained in captures/exports. It does not count as additional scientific computation. Colors and sphere radii are visual representations; the argon radius is sigma/2, not a hard collision boundary.

## Trusted Blender worker

Run the worker with the existing installed Blender runtime and auto-execution disabled:

```text
blender --background --factory-startup --disable-autoexec --python-exit-code 1 --python tools/trajectory_render.py -- --input export-input.json --output <new-export-job-directory>
```

The backend must supply the source/output directories, use its restricted process supervisor, own cancellation/deadlines and prevent credentials from reaching the worker. The worker accepts data-only JSON; it never imports an arbitrary scene, script, URL or Python expression. A virtual environment alone is not a security boundary. `cancel.request` in the export directory also stops the worker; external process-tree termination is the primary mechanism during a blocked native render. Partial MP4 files are not complete results.

Input:

```json
{
  "source_directory": "<backend-owned source job directory>",
  "width": 1920,
  "height": 1080,
  "fps": 30,
  "playback_duration_seconds": 30,
  "frame_count": 900,
  "start_time": 0,
  "end_time": 10,
  "renderer": "eevee",
  "samples": 64,
  "labels": true,
  "presentation": {
    "color": "#589fff",
    "background": "#080f20",
    "interpolate": false,
    "selectedIds": [],
    "hiddenIds": [],
    "dimOthers": false,
    "exposure": 1.1,
    "opacity": 1,
    "camera": {"position": [4, 3, 5], "target": [1, 1, 1], "up": [0, 1, 0], "fov": 38}
  }
}
```

Camera coordinates use the topology's physical coordinate units. Internal normalization does not alter them. `mode: "png"` renders one requested recorded/interpolated state for an observation; it permits AI monitoring without a mounted UI. The caller controls the requested timestamp with `start_time`. `renderer: "cycles"` enables denoised Cycles and selects a compatible GPU when available. Eevee is the default for movies. Resolution is not adaptively reduced during export. The service should estimate resource use and reject unsupported requests before launching.

The native additional-view instrument (`laboratory/observation.rs`) always forces exact recorded states with interpolation disabled. It accepts a source solver job, scientific time, camera/presentation patch, labels and even 320–2048 × 180–2048 image dimensions. It freezes the merged presentation and source in a separate durable `observation` job under the requesting session's deadline. Owned-process cancellation, the shared render queue, idempotent request IDs and immutable restarted attempts use the same job controls as exports. PNG publication requires a completed job and a matching image digest. Source chunk/field hashes are calculated from the same bytes that are decoded, so the receipt describes the data actually rendered.

Admission retains a byte-for-byte index snapshot in `source-pin/index.json` and pins its SHA-256, the selected index entry and its scientific artifact hashes; molecular observations also pin the topology hash. Replay, restart and renderer launch check the pin. The live source may append later entries and increase its end time/frame count. It may not replace the selected chunk/field, rewrite its committed digest, or change scientific metadata or topology. The worker uses the original snapshot for selection and verifies the exact decoded bytes. An old observation without an admission-time pin requires a fresh observation request. A restarted attempt reuses the original pin and presentation; it never silently repins changed data. Under an active parent it inherits that parent's exact deadline. Explicit continuation after a stopped, expired or finished parent creates a detached attempt with the original observation ID retained in its restart lineage.

The stricter pin/restart path passed seven Rust tests and fifteen Python renderer contracts. New actual supervised argon and field renders are recorded in `.local/observation-acceptance/b5d2fee6-f2a0-4561-9555-0493e0e1f36b/report.json`. The tests accept appended index entries while rejecting selected-file mutation, rewritten selected digests, topology/unit changes and missing hashes; a failed pin creates no restarted attempt. They also check exact active-parent deadlines and detached continuations. A separate real `labels: false` field render removed title, legend, color bar and backing while keeping the same numerical file hashes, scientific time and central field pixels; `labels-check.json` records five passing checks. Its PNG was visually inspected. Quantitative color metadata remains in the receipt even when visible labels are off.

An actual supervised check on 2026-09-11 rendered both saved OpenMM argon and diffusion fields at 640 × 360 without launching new solvers. Admission/replay and paused-attempt cancellation tests also passed. Evidence is `.local/observation-acceptance/ec236a44-3f3e-432c-99e0-627d6c3fec99/report.json`; both PNGs were visually inspected for readable labels and actual retained content. This is a component check, not an installed ordinary-chat acceptance claim. The first attempt caught JSON float round-trip error selecting a penultimate record. Shared sampling now snaps requests within 64 machine epsilons of a retained time, capped at one quarter of the adjacent record interval; materially earlier requests still hold the previous record. The regression test includes adjacent representable timestamps so snapping cannot erase distinct saved states.

Every successful export records source index/topology hashes, all read chunk hashes, presentation, source endpoint timestamps, interpolation policy, renderer/version/device, actual output dimensions, frame count, duration, size and SHA-256 in `result.json`. The worker supports even resolutions from 320 × 180 to 3840 × 2160 and frame rates up to 60. Eevee accepts 8–128 samples; Cycles accepts 8–512. It exposes its 2–216,000-frame movie limit explicitly; longer scientific trajectories can be exported in selected windows. The backend supplies the validated integer `frame_count`, using positive half-up rounding of duration × fps; the worker verifies it before rendering. Thus 0.15 seconds at 30 fps encodes 5 frames, or 0.166667 playback seconds. The endpoint map includes both start and end. Scientific endpoints within 64 machine epsilons of the retained range are clamped to the exact saved endpoint; materially out-of-range requests fail.

An interrupted export resumes as a separate attempt with `restarted_from_export_id` provenance. Existing partial artifacts stay in the previous attempt; the new renderer starts from the same saved numerical trajectory and presentation. Cancellation and deadlines prevent publication even when an encoder is finishing. The artifact API serves the final MP4 only for a completed export. Preflight estimates include the measured cold-start allowance and describe queue wait as additional.

Outputs are `simulation.mp4`, `first-frame.png`, `last-frame.png`, `scene.blend`, `result.json`, and atomic `progress.json`. A PNG observation produces `first-frame.png` plus provenance. `scene.blend` is the retained editable composition at the last rendered state; the authoritative animation is reproducible from the source chunks and export input, rather than a baked independent physics scene. `progress.json` reports state, fraction, frame count, elapsed time and a throughput-based remaining-time estimate. An estimate made after rendering begins is distinct from the UI's required preflight estimate.

## Rendering choice and verification

The interactive viewer retains Three.js for responsive inspection and uses instanced particle geometry, colored materials, three-point lighting, scale-aware camera controls and independently adaptive preview pixels. New controls include keyboard/on-screen pan, zoom, orbit, tilt, roll, standard views, fit selection, reset, saved views and source camera coordinates. Presentation revisions and undo/redo are separate from scientific state.

Blender is preserved for higher-quality exports and requested scientific illustrations. Its built-in FFmpeg encoder creates H.264 in MP4; no additional Python encoder package is required. A basic worker check reopens the output through Blender's movie decoder and checks dimensions/frame count. This is not an independent player check. Acceptance must also inspect real images, probe/decode with a separate tool, and play the resulting files independently. A valid MP4 can still contain an incorrectly rendered scene.

An early local test illustrated this distinction: the MP4 was structurally correct while an extreme near/far clipping ratio made Eevee particles black. Image inspection caught the problem. Distance-aware clipping corrected it; comparable protections are used in the interactive controls. Keep this regression in visual acceptance, rather than accepting codec metadata alone.

Local worker tests and renderer timings live under `.local/render-acceptance/`. These are component-level results until the exact packaged Windows application exercises export, navigation, cancellation and download. They do not by themselves satisfy the installed-app acceptance gate or certify a scientific model.

Verified locally on 2026-09-11 using the real OpenMM argon acceptance trajectory (108 atoms, 0–5 ps window):

| Export | Independent FFprobe result | Worker elapsed time |
| --- | --- | --- |
| 720p fast | H.264, 1280 × 720, 24 fps, 12 frames, 0.5 s | 4.17 s |
| 720p slow | H.264, 1280 × 720, 24 fps, 24 frames, 1.0 s | 6.67 s |
| 1080p fast | H.264, 1920 × 1080, 24 fps, 12 frames, 0.5 s | 5.56 s |
| 1080p slow | H.264, 1920 × 1080, 24 fps, 24 frames, 1.0 s | 9.14 s |

All four source-index hashes match and an independent FFmpeg process decoded every frame without errors. A later 3-second/72-frame export exercised improved whole-cell camera fitting and higher contrast; its first and last PNGs were visually inspected. FFplay played that video with exit 0 and zero dropped frames. A cancellation test stopped an in-flight worker in 0.203 seconds, exit 130, with no completed `result.json`. Evidence is in `verification.json`, `final-camera/ffplay.log` and `cancellation/cancellation-evidence.json` under that local directory. These short component tests check encoding behavior; they are not demonstrations of a longer newly computed scientific horizon. A cold Eevee shader initialization added approximately 16 seconds in the first test; estimates must account for cold starts and changing scene complexity.

The current interactive contrast control adjusts displayed particle contrast continuously. Blender exports use AgX's Medium High/Medium Low Contrast looks when contrast differs from 1; the chosen look is recorded in the output. This is a declared rendering difference, not a change in the recorded physical state.

## Upstream references and distribution

Scalar diffusion jobs use the same durable export endpoints, with `fields/index.json` and the trusted `tools/field_render.py` worker. It verifies authoritative NumPy arrays against hashed JSON views and renders exact recorded cells using a fixed quantitative palette and legend. Field exports use Standard color management and emission colors; their numeric values are unaffected by presentation contrast. See [field viewer and illustration validation](validation/field-viewer-and-illustration.md) for independent pixel and MP4 checks. The common trajectory worker is copied alongside it solely for trusted camera, input-validation and encoding helpers; no solver runs during export.

- [Blender supported media formats](https://docs.blender.org/manual/en/latest/files/media/video_formats.html): Blender uses FFmpeg for video encoding/decoding.
- [Blender 4.5 output settings](https://docs.blender.org/manual/id/4.5/render/output/properties/output.html): renderer output/encoding settings.
- [Blender licensing](https://www.blender.org/about/license/): Blender is GPL software. Its licensing obligations apply when distributing the runtime; this worker does not change the user's ownership of rendered artwork.
- [FFprobe documentation](https://ffmpeg.org/ffprobe.html): independent stream metadata and frame-count checks.
- [Official FFmpeg download page](https://ffmpeg.org/download.html) links the [Gyan Windows builds](https://www.gyan.dev/ffmpeg/builds/). For local independent verification only, FFmpeg/FFprobe 9.0.1 essentials was downloaded with the publisher's SHA-256 check and stored under `.local/render-tools/`. Its archive hash is `fec81ae03971d9dd4be3ebe02e263bd2ec1d789483f931bdba5f5715e65da2e9`. That GPLv3 build is not included in the PhaseForge installer by these changes. Any future bundling needs its actual license/source and exact binary provenance in release materials.
