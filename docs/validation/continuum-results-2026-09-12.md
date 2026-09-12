# Heat and incompressible flow: source component results

The new heat/flow workers passed their fixed analytical, convergence, intervention,
nonlinear, retained-array, recovery and consumer checks on 2026-09-12. These are
small source-level computations with the existing pinned CPython 3.13.15 / NumPy
2.4.6 / Pillow 12.3.0 science interpreter. They are not installed application,
ordinary-chat acceptance, real-material calibration or wet-lab validation.
The scope and criteria were fixed in [continuum-coverage.md](continuum-coverage.md)
before the first run; its SHA256 was
`54c4a698a16e598a30bfbe945901bb18d7953cbb21c588fdd575018749afbf56`.

| Check | Measured result |
| --- | --- |
| Heat second-order refinement | Error reduction 4.01470 and 4.00397 for 16/32/64 grids |
| Heat discrete Fourier solution | Every retained array within 1e-10 K |
| Thermal energy | Independent scalar-sum relative drift <=1e-12 |
| Conductivity control/intervention | Zero leaves exact state unchanged; doubled conductivity follows discrete reference |
| Heat-flux channels | Both retained components match independently derived discrete Fourier flux within 1e-7 W/m² |
| Taylor-Green velocity/vorticity/pressure | Maximum errors 2.08e-16 m/s, 5.66e-15 1/s, 1.60e-17 Pa |
| Flow RK4 refinement | Error reduction 16.9786 and 16.4814 at dt 0.02/0.01/0.005 s |
| Nonlinear advection | Independently differentiated multi-mode RHS relative error 5.9041e-7; nonlinear term is nonzero |
| Inviscid interacting flow | Energy/enstrophy relative drift <=1.32e-14 while numerical state changes |
| Actual subprocess interruption | Both paused at step 10; resumed final channels exactly equal uninterrupted channels |
| Retained-data refusal | Corrupt NPZ and changed physics rejected; stale public index repaired from committed checkpoint |
| Imported temperature array | Exact asymmetric input retained; wrong shape rejected; zero conductivity has zero flux |
| Admission | Eight unsupported/unstable/noninteger requests rejected without a completed experiment |
| Pixels and provenance | All registered arrays, JSON views, channel NPZs, PNG hashes, source pins and sample pixel orientation match |

The first run (`continuum-components-01`) also passed. The final run includes
additional plain-file recovery guards, imported-array and pixel/heat-flux checks:

- `.local/validation/continuum-components-02/report.json`, SHA256
  `c9d3610e892b7daa17c59004e883243b5b28c6db7e796f23b544d8816c2b12d8`.
- 36.45 seconds, 13,665,274 retained bytes at report finalization.
- Worker SHA256 `393ffb7b73422ca1b6338672df16570c2c93872661aaa59e834ee3900dae8fea`;
  shared I/O worker SHA256 `ee88e854487ca98bf124bf41a70fbdc992d62b88c912124dce4ca3523a5e95d7`.
- Independent checker SHA256 `eb8af543b893418f682f02d9f35f9c3c0b70a1cc1e0c837b72194b922b3726f7`.

The existing **unmodified** diffusion checker passed all nine checks, including
its independent reference, convergence, image, intervention and real interruption
cases. Receipt `.local/validation/diffusion-preservation-01/report.json` SHA256
`3455a141a3e73a0cc818fe9b5b81ca50961d90172e0c96c7b77aa1770079c96b`.

## Frontend and Blender consumers

The actual `FieldStore` frontend decoder loaded every retained frame in four
heat/flow cases, verified hashes and cell coordinates, bounded its cache, and
rejected corrupted JSON bytes. This used the decoder directly, not a browser.
Receipt `.local/validation/continuum-components-02/frontend-consumer.json` SHA256
`0a25008a93195ad1c3c1c52e024085a37bbdaee241020af88993dd3bad436eaa`.

Existing Blender 4.5.9 rendered and decoded two two-frame 640×360 H.264 clips,
one consuming heat states at 0/0.4 s and one consuming flow states at 0/0.2 s.
The scalar source hashes and endpoint times matched, every original source file
remained unchanged, and no scientific worker was invoked during rendering.
Actual image inspection showed readable field units, physical time, fixed colour
scale and micrometre coordinate labels; the visible pixel grid is the computed
32×32 discretization. This is not an arbitrary fine-resolution physical field.
The numerical channel NPZs retain velocity/pressure/flux even though the existing
scalar view displays temperature or vorticity.

Receipt `.local/validation/continuum-render-01/report.json` SHA256
`6355b255e5c02bed1680ed2ba36f111d44750a609c3c97d66a3653af1fd078df`.
The videos were decoded using Blender's FFmpeg decoder; a separate player and
interactive/native application were not used for this component check.

## Reproduction

Use the existing pinned science interpreter with `-I -B` and a new output
directory, preserving prior evidence:

```text
python -I -B tools/check_continuum_worker.py --output <new-evidence-directory>
node tools/check_continuum_consumers.mjs <evidence-directory> <new-consumer-report.json>
python -I -B tools/check_continuum_render.py --blender <existing-blender-executable> --sources <evidence-directory> --output <new-render-directory>
python -I -B tools/check_field_worker.py --output <new-diffusion-evidence-directory>
```

Backend dimensional admission tests accompany `thermal.rs` and `fluid.rs`.
The explicit ignored test
`continuum_readiness_runs_actual_temperature_and_velocity_channels` requires
`PHASEFORGE_TEST_PYTHON` and `--ignored`; it must be invoked deliberately so the
default test suite cannot disguise an unavailable interpreter as an actual
scientific execution. Its execution result belongs to the integration receipts,
not the component report above.

Full general-relativistic black-hole merger simulation remains unsupported.
The official Einstein Toolkit merger and current container/hosted tutorial are
referenced in the coverage document; no relativistic engine was installed and no
illustration was substituted for a merger experiment.

## Follow-up: immutable runtime checker launch

A later normal-API integration correctly rejected unexpected `__pycache__`
directories in the development seed. The original diffusion checker (used for
the unchanged-diffusion regression above) launched its child interpreter without
`-I -B`; those flags are not inherited from its parent. The first observed cache
timestamp, 11:41:51.254 UTC, matched that regression's 11:41:51.162 UTC start.
Production field readiness and worker launches, and the new continuum checker's
launch/recovery commands, already explicitly supplied both flags.

The diffusion checker's `Runner.command` now supplies `-I -B`. A new focused
`tools/test_field_checker_launch.py` test executed the actual child command,
verified isolation and `sys.dont_write_bytecode`, imported a retained source
module, and verified no cache directory or source-byte mutation. It passed in
0.070 seconds. The initial test-fixture attempt omitted its dummy input file and
failed before launching a child; that fixture omission was corrected. The solver,
runtime manifest/seed versions and strict runtime inventory validation were not
changed. The prior numerical receipts remain preserved; repairing and verifying
the development seed inventory is a separate integration operation.
