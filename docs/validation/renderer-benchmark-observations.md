# Controlled renderer evidence so far

2026-09-11. The original six paired component measurements and two targeted cadence-repair follow-up pairs are complete. Installed application performance remains a separate acceptance item.

## Numerical and image equivalence

The baseline was recovered from the preserved starting Windows installation at commit `7471ff72`, with its actual bundle/source-map identity verified. The candidate is a frozen source copy dated 18:01:09 UTC, not an installed performance claim. The immutable 108-particle, 101-state argon fixture contains 10,908 positions. Every JSON position matched its wrapped NPZ coordinate exactly before copying.

Both renderers produced actual 960 × 600 PNGs at retained frames 0, 50 and 100. They used the same IDs, radii, palette, camera, projection field of view and drawing-buffer dimensions. Every capture passed the independently computed pixel-center projection check. Maximum observed instance-buffer coordinate rounding was 1.155 × 10⁻⁷ nm; maximum projection error was 1.47 × 10⁻⁵ px, well inside the frozen tolerances.

Actual images and measurement receipts are in `.local/viewer-acceptance/renderer-benchmark/`, named `baseline-frame0/50/100` and `candidate-frame0/50/100`, each with PNG and JSON. Matched frame 0 and endpoint frame 100 were opened and visually compared. The newer renderer has glossier highlights and denser sphere surfaces. At the endpoint, small dark speckles visible around some overlaps in the starting image are reduced in the candidate image. This is a source-image observation, not a blinded perceptual score or scientific inference.

The starting renderer submitted 56,160 triangles and one draw call for these particles. The candidate submitted 114,912 triangles and one draw call: approximately 2.046 times as many triangles at the same pixel resolution. Better surface appearance therefore must not be presented as free performance improvement. Neither image adds new scientific states, molecular mechanisms or physical validation.

## Timing qualification failure

The first baseline and candidate attempts each recorded exactly 30 render calls over approximately 30 seconds. The visible-surface qualification also recorded 30 calls. All reported the document visible, so this browser's background behavior prevents using those results as interactive-viewer measurements. A request to show the test tab was queued because the agent's thread was hidden. These attempted timings are retained, excluded from comparative FPS conclusions, and are not used to claim a slowdown or speedup.

No controlled CPU-load solver was started. The browser renderer was disposed, its tab closed, and telemetry stopped before the coordinator resumed ML. Aggregate NVIDIA memory readings are retained in `telemetry.jsonl`; they are not attributable per-viewer VRAM. No RAM or VRAM reduction is claimed.

The hidden Electron qualification also remained at1 Hz. A separately preregistered GPU-accelerated offscreen bitmap host removed that cap. The hidden qualification and its exact source remain retained; no result from it is used as an interactive-performance estimate.

## Completed original comparison

The exact source freeze, host, raw timestamps, PNGs and summaries are under `.local/viewer-acceptance/renderer-benchmark/`. `paired-summary.json` joins the three no-load pairs with the three controlled-load pairs, preserving the startup failure described below. All12 members used the same108 retained particles,960 × 600 drawing buffer, common palette/radii, physical camera, ordered exact-state seeking, and five-second warm-up followed by30 seconds measurement.

| Condition, three paired repetitions | Starting renderer draws/s | Original candidate draws/s | Starting p95 interval | Candidate p95 interval |
| --- | --- | --- | --- | --- |
| No study solver |59.935–59.999|48.033–48.334|17.6–17.9ms|25.7–25.975ms|
| One actual CPU solver thread |59.965–60.001|47.833–48.300|17.6–18.1ms|25.7–26.1ms|

The newer surfaces were visually better in this matched case, but the original candidate drew less often. Its loop discarded fractional frame time by setting its draw clock to the current callback timestamp. Typical renderer submission CPU times were0.1–0.2 ms, far below the observed intervals; these are CPU submission times, not GPU completion times. No smoother-playback claim is supported for that original candidate.

The first CPU-load coordinator incorrectly placed its own logs inside a fresh solver output directory. The scientific worker rejected that directory before producing a state, and the original series stopped. That failed attempt is retained. A fresh coordinator separated its logs from the solver's empty output directory, using identical scientific input and worker bytes. All three loaded pairs then ran successfully; their actual solver PIDs remained alive throughout and were present in every available system sample. Each load ended by cooperative pause with its committed calculated states retained, followed by process exit. No previously measured member was overwritten or silently rerun.

## Cadence repair and targeted follow-up

The repaired source advances an anchored draw deadline, including a0.1 ms allowance for rounded timestamps. Physical playback time, background behavior, the active 60 / idle 15 caps and all rendering features remain unchanged. Eight focused timing/trajectory tests passed before measurement. The first remainder-only implementation failed the rounded 60 Hz trace and was corrected before the new source was frozen.

`freeze-repaired.json` pins that new candidate and its metadata-only post-context GPU diagnostics. The original copies remain unchanged. The two preregistered fresh pairs produced:

| Condition | Fresh starting renderer | Repaired candidate | Starting p95 interval | Repaired p95 interval |
| --- | --- | --- | --- | --- |
| No study solver |60.014draws/s|59.968draws/s|17.7ms|17.8ms|
| One actual CPU solver thread |59.968draws/s|59.999draws/s|17.6ms|18.1ms|

Both repaired candidates passed the declared≥55draws/s and within5% of their fresh baseline criteria. Candidate p99intervals were still about25 ms versus18.2–18.6 ms in these fresh baselines, so this is a repaired average cadence, not proof that every tail-latency measure improved. The candidate still renders114,912 triangles against56,160 starting triangles. It retains its denser surfaces and different material/lighting; matching the drawing cap does not prove that extra graphics work is free.

All16 measured members passed numerical-position, buffer, projection, context-loss and disposal checks; no render errors or requested/displayed frame lag were recorded. An additional independent check reconstructed the nominal camera from the frozen box and direction. Every endpoint was frame 100, and the maximum physical-camera difference was8.89 ×10⁻¹⁶, below1 ×10⁻⁸. Both source inventories were rehashed after measurement and remained intact. Evidence:`independent-camera-checks.json`, `paired-summary.json`, and `paired-repaired-summary.json`.

## Resource and interpretation limits

The machine exposed an Intel Core Ultra 9 185H with16 cores / 22 logical processors and approximately31.4 GiB RAM. Fresh follow-up canvas contexts identify the NVIDIA GeForce RTX 4090 Laptop GPU through ANGLE/D3D11, with driver 596.21. Their post-context Electron receipts confirm hardware acceleration. The original receipts queried GPU state before context creation and therefore do not independently identify that adapter; their uninitialized data are retained without inventing an active device.

Dedicated Electron process RAM sampled at approximately 1 Hz. Original renderer-private medians were about64.9–66.1 MiB for the baseline and58.5–62.3 MiB for the slower original candidate. Fresh repaired renderer-private medians were61.1 / 61.8 MiB; fresh baseline medians were69.6 / 65.0 MiB. These short process samples include the harness, cached data and garbage collection. The complete offscreen host had much greater and variable private memory, roughly611–739 MiB across the fresh run medians. No application memory reduction is claimed.

Whole-machine CPU/NVIDIA telemetry achieved roughly 5-second spacing, slower than its intended 1 Hz cadence; raw timestamps are authoritative. NVIDIA memory was approximately 3.1 GiB aggregate across the machine and cannot be assigned to a viewer. No attributable VRAM measurement or VRAM win is claimed. No Cargo, rustc or Blender process appeared in the recorded comparison samples; the coordinator held other study/ML/render work. Ordinary desktop background activity remained.

These are controlled source-component results in Electron 45.0.0-alpha.6 with offscreen GPU-to-CPU bitmap copying. They are not native installed-app FPS, a graphics stress limit, a large-particle scaling benchmark, or additional scientific validation. The case contains108 particles at 960 × 600 only. The copied numerical states are real and fixed; visual appearance does not add uncomputed physical detail.

The same tested timing helper was subsequently applied to field/model viewer loops while preserving their hidden-document and dirty-frame policies. Thirty-four focused viewer/camera/field/model/resource tests passed, and the frontend 0.9.0 production export built all eight routes successfully. This source build awaits the next installed native acceptance. All test hosts, load workers and telemetry processes exited after their measurements; compute was released to the coordinator.
