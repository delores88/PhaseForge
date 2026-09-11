# Extended molecular trajectory: source component validation

This is an actual **source-level OpenMM run**, performed on 2026-09-11. It is not an installed-app acceptance result, a playback demonstration, or a claim of empirical argon/biological validity. The existing worker and its limits were not modified for this check.

The exact requested case completed: 32 atoms, CPU platform with one thread, Langevin thermostat at 120 K, density 0.8 g/cm³, seed 314159, timestep 1 fs, 500,000 integration steps, sample interval 20 and chunks of 100 frames. Friction was the existing default, 1/ps. Input, tolerances and source snapshots were retained before launch. A Windows Job Object supervised the subprocess tree with cooperative stop at 470 seconds and a hard 480-second bound; neither timeout was reached.

| Retained result | Measured value |
| --- | --- |
| Integrated steps | 500,000 |
| Physical horizon | 499.99999999508844 ps |
| Retained calculated states | **25,001**, beyond the former 20,000 ceiling |
| JSON/NPZ chunk pairs | 251, including the separately committed initial state |
| Hash-linked checkpoint PNGs | 251 |
| Output files hashed | 1,015 |
| Worker time / supervised wall time | 343.750 s / 345.281 s |
| Verification time | 19.234 s |
| Final output size | 146,078,029 bytes, about 139.3 MiB |

The checker streamed every output file through SHA256 and verified all declared chunk, NPZ, observation and final-checkpoint hashes. Every retained frame was checked against the NPZ arrays for entity identity, exact periodic wrapping, step order and time; all 25,001 canonical frame hashes are retained. Chunk endpoints and counts agree with the measurements, result and trajectory indexes. No state arrays were concatenated into an unbounded full-trajectory allocation.

Independent scalar switched-Lennard-Jones calculations used **69 predetermined frames**, spread over the run and including frames immediately around the old 20,000-state boundary. The reference implementation comes from the existing independent checker, not worker helpers. Full pair-force loops were limited to these sampled frames; stored-state integrity and streamed hashes covered every frame/file. Tolerances were frozen in `plan.json` before computation, using the existing CPU numerical limits.

| Sampled independent check | Maximum error | Frozen limit |
| --- | --- | --- |
| Potential energy, kJ/mol | 3.0922e-5 | 2e-4 |
| Force component, kJ/(mol nm) | 9.3480e-4 | 3e-3 |
| Virial pressure, bar | 2.6148e-12 | 1e-8 |
| Mean squared displacement, nm² | 0 | 1e-12 |

All numerical and retention checks passed. The measured second-half temperature was 120.0275 K, reported descriptively; this run did not add an equilibrium or empirical calibration criterion. Langevin total energy is not expected to be conserved.

Actual worker resource counters showed a lifetime peak working set of **141,414,400 bytes** (about 134.9 MiB), peak process commit of **835,588,096 bytes** (about 796.9 MiB), and 195.625 CPU seconds (114.406 user plus 81.219 kernel). It wrote **2,207,341,820 bytes cumulatively**, approximately **15.1 times** final output size. The worker rewrites growing measurement/restart records at each chunk and atomically writes progress at each sample; these measured writes and the substantial kernel time identify storage overhead as a scaling concern. This was not an isolated machine-throughput benchmark: other app acceptance work could run concurrently. Hardware was an Intel Core Ultra 9 185H with about 32 GiB RAM.

One profiler defect was caught and preserved: the initial checker sampled only the Windows virtual-environment launcher, whose memory/CPU/I/O values understate numerical-worker use. The original `report.json` and telemetry remain unchanged; its wall time and process-tree supervision are valid. A separate read-only probe identified the actual numerical child and retained its OS lifetime counters through exit. Those corrected values are in `actual-worker-resources.json` and the combined `summary.json`. The reusable checker now queries every Job Object member and job-level lifetime CPU/I/O; a separate 32-MiB child-process probe passed, including counters after exit. No scientific worker, input or tolerance was changed to address this profiler issue.

Evidence directory: `.local/science/extended-component/`.

- `summary.json`: combined numerical outcome, corrected resources and evidence hashes.
- `plan.json`, `input.json`, `source/`: frozen parameters, tolerances and executed source snapshots.
- `report.json`: original successful calculation/verification report; launcher-only resource caveat above applies.
- `artifact-hashes.json`, `frame-hashes.jsonl`: complete streamed output inventory and frame identities.
- `actual-worker-resources.json[l]`: true worker counters; `tree-metrics-check.json`: corrected reusable profiler check.
- `output/`: all numerical data, images, checkpoints, manifests and instruments.

Executed worker SHA256: `9a7c3167f164a6e4f8ab27988584b66492657ebf2ec87fa1a3ac541fc76e88d5` (also matches the repository worker after the run).

Independent reference checker SHA256: `cd26d5a072589096517263658af947571d4eef74c4503fe6d59ea40a12260d25`.

Executed extended checker SHA256: `fe1bed83f86a804731f24da10e17928e46f395b39c2bd1f9a3fed51ee3dce5d1`. Corrected reusable checker SHA256: `9a53c15c23f2e22de60693954bf6ae6c6d820b08892b21e9e5161cb63d9f9d4a`.

The run used the existing `.local/science/venv/Scripts/python.exe`: Python 3.12.7, NumPy 2.4.6 and OpenMM internal version `8.5.2.dev-36a30cb` (pinned OpenMM 8.5.2 wheel). No application, provider key, model, installer or catalog expansion was used.
