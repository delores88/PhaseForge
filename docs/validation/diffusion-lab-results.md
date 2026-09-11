# Independent diffusion laboratory results

The acceptance criteria are frozen in
[`diffusion-lab-proposal.md`](diffusion-lab-proposal.md). That proposal is preserved
unchanged. This report distinguishes numerical worker evidence from the separate
installed application and model observation gates.

## Checker and retained evidence

[`tools/check_field_worker.py`](../../tools/check_field_worker.py) launches the
trusted worker as real subprocesses. It never imports the worker's numerical or
instrument implementation. Expected Fourier fields use scalar sine, cosine,
exponential and the independently derived discrete amplification factor, evaluated
at input-defined cell centers. NumPy reads numeric arrays with `allow_pickle=False`;
Pillow reads PNGs. Instruments are recomputed from saved values using scalar
`math.fsum` arithmetic.

The checker requires a new evidence directory for every attempt. It retains input,
stdout, stderr, exit status, timings, worker/checker/proposal hashes, per-frame
numeric errors, pixel samples and checkpoint receipts. A failed attempt remains
available and is never rewritten into a passing attempt.

The executable checks cover:

- 32², 64² and 128² grids at the preregistered timesteps and physical duration,
  with exact discrete field agreement at every retained frame, continuum error,
  refinement, integral conservation and bounds on minima/maxima.
- The 64² doubled-diffusivity intervention, continuum amplitude ratio and conserved
  integral, with identical color scales across benchmark evidence images.
- Point and half-open region instruments independently computed from retained
  arrays; a rectangular 48×32 asymmetric imported array exposes transpose or
  vertical orientation mistakes that the symmetric cosine benchmark could hide.
- Field and PNG SHA-256 provenance, physical time, 1024×1024 evidence image bounds,
  and selected interior pixels evaluated against the declared two-stop color map
  with at most one 8-bit channel level of error.
- Rejection of unstable timesteps, unsupported boundaries, wrong-shaped/object/
  nonfinite arrays, unknown parameters and grid/step/frame budget violations.
- An actual running subprocess stopped by `cancel.request`, exit code 3, retained
  checkpoint and process receipts, resume with unchanged prior field receipts,
  and bit-identical final float64 fields against uninterrupted execution.

## Execution status

The Rust adapter also passed **3 tests, 0 failures, no skips in 0.96 s** after
integration: dimensional/CFL/retention admission, immutable pinned source reuse,
and an actual supervised field-readiness process. The production adapter invokes
the trusted worker on a two-step 16×16 pattern, requires measured decay plus
integral conservation, and saves `field-readiness.json` with its source directory
and worker hash before the requested study. It shares `LabJob` solver identity,
the pinned environment, deadlines, process supervision and cooperative checkpoint
control. This does not replace the independent study checks below.

```powershell
$env:PHASEFORGE_TEST_PYTHON = (Resolve-Path .local/science/venv/Scripts/python.exe).Path
cargo test --locked --manifest-path backend/Cargo.toml --lib laboratory::field::tests -- --nocapture
```

**Numerical acceptance passed on 2026-09-11.** The full second attempt ran from
15:32:51 to 15:33:37 UTC using the existing `.local/science/venv/Scripts/python.exe`
runtime, NumPy 2.4.6 and Pillow 12.3.0. All nine grouped checks passed. The complete
machine-readable report and all subprocess evidence are retained at
`.local/validation/diffusion-check-20260911-attempt2/report.json`.

Reproduction command, with a fresh output directory:

```powershell
.local/science/venv/Scripts/python.exe tools/check_field_worker.py --output .local/validation/diffusion-check-new-attempt
```

| Grid | Timestep (s) | Steps | Final continuum relative L2 error | Maximum discrete field error |
| --- | ---: | ---: | ---: | ---: |
| 32×32 | 0.02 | 128 | 0.0070865376547 | 1.1102230246e-15 |
| 64×64 | 0.005 | 512 | 0.0017647524236 | 1.9984014443e-15 |
| 128×128 | 0.00125 | 2048 | 0.0004407574051 | 2.8865798640e-15 |

Every benchmark ends at 2.56 s. The continuum error decreases by factors
4.0155987661 and 4.0039087334, satisfying the frozen 3.5–4.5 interval. Coarse and
fine errors meet the respective 0.008 and 0.0005 bounds. Every one of the 260
retained frames across the three grids and doubled-D control meets the discrete
field error bound of `1e-10`.

The measured final amplitude ratio for D=0.4 versus D=0.2 is 0.363899989694;
the continuum ratio is 0.363983227512. Their relative discrepancy is
0.0002286858602 (0.02287%), below 1%, with faster decay for the larger D.
The maximum independent relative integral drift across all four runs is
1.4210854715e-16, below `1e-11`. Every retained field also satisfies the frozen
initial min/max bounds with `1e-12` slack.

The largest scalar-instrument discrepancy in the refinement runs is
1.4210854715e-14; the largest point/region discrepancy is 2.2204460493e-16,
both below `1e-10`. The asymmetric array's instruments also pass. All 23 verified
PNG images have matching source and image hashes, and all 575 sampled interior
pixels match the declared color map exactly (maximum channel error 0). The
Fourier grids and D intervention share the fixed range [0.8, 1.2]. The asymmetric
array fixture's initial image makes both axis orientation and transpose errors
observable. Selected pixels establish the tested mapping; they do not constitute
human or model assessment of image readability.

All ten invalid-input cases exit nonzero before saving an advanced field, with no
successful result receipt. The recovery test observes an actively running process
at step 1024/32768, touches `cancel.request`, and obtains exit code 3 with a valid
checkpoint at step 1120. Removing the cancellation request and rerunning the same
input/output pair completes. All three prior field receipts and their bytes are
unchanged. Altering D to 0.21 in a separate copied cancelled attempt is refused
with exit code 2, preserving the checkpoint.

The resumed and uninterrupted final arrays are bit-identical float64 fields;
both final `.npy` files have SHA-256
`78665b2916f16b710d454d0edd2c02915eaf527b534e30c17acc26339833f18e`.
Cancellation, refusal and resume stdout/stderr/process receipts remain separate
from scientific result receipts. The cancellation checkpoint JSON/NPZ and its
then-current field index and measurements are preserved under
`recovery-interrupted/cancelled-checkpoint/` in the attempt directory.

Observed worker subprocess times for 32², 64² and 128² are 4.923, 6.068 and
6.564 seconds respectively, including retained arrays, JSON, instruments and PNG
generation. These measurements do not isolate solver performance or peak memory.
The full second attempt retained approximately 78.8 MB of evidence before any
source snapshots; no broader speed or memory claim is established.

## Preserved failed attempt and source identity

The first full attempt is retained at
`.local/validation/diffusion-check-20260911-attempt1/report.json` with `passed:false`.
Its asymmetric-array metadata check failed because the checker compared the entire
requested initial dictionary against the normalized manifest dictionary, which
adds the imported file's `source_sha256`. The worker preserved the requested
scientific values. The checker was corrected to require every requested nested
value to remain exact and to independently verify the additional imported-file
hash. No numerical, image, conservation, refinement or recovery tolerance changed.
All other first-attempt grouped checks passed, including actual cancellation and
bit-identical recovery. The failure remains recorded rather than being relabeled.
Both attempt directories now include `sources/` snapshots verified against their
recorded execution hashes. The first checker's snapshot was reconstructed by
reversing the single documented metadata patch and accepted only after its bytes
matched the first execution's SHA-256; the snapshot receipt records that history.

| Source | SHA-256 |
| --- | --- |
| Frozen proposal, both attempts | `a8602554696071d66b0cea67dfa4c4b530c9c8faf516099af22c0900fe126f61` |
| Worker, both attempts | `492df5739012fc99b7ce738c16c93a2f28e8b003c27245295ca5f52ed16f2bd1` |
| Checker, failed first attempt | `cb44bb5fe8a45cfea28020565bb7905844c984471af1fe3156a748463a52f38e` |
| Checker, passing second attempt | `633ac4d2922dc7008c0fade5eca143f0feb4a9af16e3ef22808d6be9684ae9cf` |

## Remaining gates and scientific limits

An installed ordinary-chat run, controlled comparison in the installed app,
navigation during execution, actual model image observation and an independently
specified supported variant are separate acceptance gates. Passing this checker
alone does not establish those gates. Agreement with the diffusion equation does
not establish predictive validity for a real material.
