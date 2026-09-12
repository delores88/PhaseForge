# Required 0.999c black-hole collision: local compute feasibility

Date: 2026-09-12. Status: **the requested equal-mass, head-on collision through merger and spacetime evolution remains unfulfilled**. The current capability-gap message correctly describes missing functionality; it does not deliver the requested experiment. A precontact analytic field, animated spheres, a Blender still, or a lower-speed benchmark cannot satisfy that requirement.

The initial audit made bounded read-only hardware, toolchain and primary-source queries. It started the existing Ubuntu-24.04 WSL2 distribution for those queries. The subsequently authorized CPU foundation work is recorded separately below; no application/profile writes or expensive collision study occurred.

## Measured local capacity

The Windows snapshot was taken at 12:47:50 UTC. Subsequent WSL queries belong to the same audit session. Scalar evidence is retained in `.local/validation/workbench-010/relativistic-hardware-receipt.json`; package plans are adjacent `relativistic-dependency-plan*.json` files. Free capacities can change immediately.

| Resource | Observed |
| --- | --- |
| CPU | Intel Core Ultra 9 185H; 16 physical cores, 22 logical processors |
| Host RAM | 33,736,036,352 bytes installed (31.42 GiB); 10.36 GiB free at the snapshot |
| Discrete GPU | RTX 4090 **Laptop** GPU; driver 596.21; 16,376 MiB total, 12,550 MiB free, 34% utilization |
| Windows | Windows 11 Pro, build 26200, x64 |
| C: free space | 103,064,334,336 bytes (95.99 GiB) |
| D: free space | 2,048,218,374,144 bytes (1,907.55 GiB); observed, not allocated for this work |
| Ubuntu-24.04 WSL2 | Linux 6.6.87.2; 15.33 GiB visible RAM, 14.71 GiB available; 4 GiB unused swap |
| Existing Ubuntu tools | Python 3.12.3, Git 2.43.0, binutils 2.42; no usable C/C++ compiler or CMake |

The GPU figures come from `nvidia-smi`; WMI's `AdapterRAM` field was unsuitable. WSL sees `/dev/dxg` and the same NVIDIA GPU, but `nvcc` is absent. GPU passthrough is not a CUDA compiler installation. WSL RAM is shared with the host, not extra capacity. No sustained solver throughput, disk throughput or exclusive resource reservation was measured.

Ubuntu lacks the queried C++ development, MPI, GSL, HDF5, FFTW and LAPACK packages. Windows has Visual Studio 18 Community, MSVC and bundled CMake/Ninja outside PATH. Docker/Podman and a usable relativity binary were not found in the scoped PATH/registry/named-directory checks; this is not an exhaustive disk search. The separate ResearchTradingApp WSL distribution was not inspected.

## Concrete engine route and outstanding physics

The preferred development candidate is [AthenaK at commit c5a0d7f9155a70149931bf0be5a4ffb673f2532a](https://github.com/IAS-Astrophysics/athenak/tree/c5a0d7f9155a70149931bf0be5a4ffb673f2532a). Its public Z4c solver evolves the Einstein equations using Kokkos and AMR and has a BSD-3-Clause license. The core CPU build requires C++17, CMake and its pinned Kokkos submodule. CUDA and distributed MPI add dependencies; they are unnecessary for a first serial development test. [Official requirements](https://ias-astrophysics.github.io/athenak-docs/requirements.html), [build instructions](https://ias-astrophysics.github.io/athenak-docs/build.html).

Its public numerical-relativity interface exposes metric variables, Hamiltonian/momentum constraint diagnostics and finite-radius Psi4 extraction. A single Schwarzschild puncture example needs no external initial-data solver. Binary initial data require separately built TwoPuncturesC plus GSL, or precomputed SpECTRE data and its exporter. The public binary tutorial is explicitly marked deprecated, and the documented single-puncture input needs increased resolution; neither should be run uncritically. [Numerical-relativity documentation](https://ias-astrophysics.github.io/athenak-docs/numerical_relativity.html), [pinned build dependencies](https://github.com/IAS-Astrophysics/athenak/blob/c5a0d7f9155a70149931bf0be5a4ffb673f2532a/CMakeLists.txt).

If 0.999c means each hole's asymptotic speed in the center-of-momentum frame, the target Lorentz factor is **22.366272**, calculated from `1/sqrt(1-v²/c²)`. Planning must also explicitly fix spin/charge, rest-mass convention, initial separation and the meaning of the calibrated speed; coordinate velocity or an input puncture momentum is insufficient evidence of that physical boost.

The April 2026 AthenaK preprint demonstrates physical boosts around gamma 5.1 using calibrated Bowen–York data and a new telegrapher lapse gauge. Its production setup uses a 216³ root grid and nine AMR levels, with three-resolution checks. The paper lists the complete new gauge formulation as forthcoming; this audit has not established a public implementation at the pinned commit. It also distinguishes input boost from the physical boost after binding/junk-radiation corrections. Therefore older Bowen–York limitations are **not an absolute impossibility theorem**, while simply increasing the momentum to gamma 22.366 remains unvalidated. The required boost is about 4.39 times the demonstrated physical gamma; that ratio is not a runtime estimate. [Zhu, Pretorius and Stone, arXiv:2604.26253v1, Methods and Appendices A/D](https://arxiv.org/html/2604.26253v1).

The laptop is a plausible development and small-reference-test machine. Exact target memory, GPU viability and completion time cannot be promised without measuring an appropriate implementation. For perspective only, Einstein Toolkit's different six-orbit binary benchmark reports 98 GB RAM, 8,700 core-hours and potentially several TB of outputs. It is not a cost estimate for the requested head-on collision. [Official benchmark](https://www.einsteintoolkit.org/gallery/bbh/index.html).

## Proposed bounded next implementation

The cached Ubuntu plan for `g++ cmake make --no-install-recommends` resolves 39 new packages, **79,130,660 bytes download and 276,362,240 bytes installed**, with no upgrades/removals. The alternative request including GSL currently fails because the cached index cannot find it, although universe is configured. These cached sizes do not establish current archive availability. Refresh signed package metadata and record a new plan before any authorized installation; do not silently expand a 500 MiB download / 2 GiB dependency admission limit.

Use a private WSL source/build directory, the exact AthenaK commit and Kokkos commit, double precision, serial CPU execution, MPI/CUDA disabled. Proposed bounds are four compiler workers, 8 GiB process address space, 45 minutes for compilation, and a separate two-minute initial gauge-wave test. These are stop limits, not completion predictions. Verify the pinned analytic test's actual problem generator and input before launch; retain source hashes, compiler/configuration, stdout, fields, constraint norms, exit status and measured peak resources.

That establishes an engine integration prerequisite only. The exact collision additionally requires: constraint-solved high-boost data and physical-speed calibration; a publicly reproducible stable gauge; apparent-horizon detection through common-horizon formation; wave extraction at several radii; constraint, resolution, boundary and junk-radiation error studies; and continuation into the remnant regime. Resource estimates must come from staged measured meshes before requesting any external compute allocation.

PhaseForge must retain full numerical/checkpoint provenance and expose actual temporal curvature/metric diagnostics, horizons and waveforms with coordinate/units labels. Rendering should read those states. Visual inference can propose features to test against diagnostics; image plausibility is not numerical validation. This path does not waive or replace the user's required 0.999c merger.

## Authorized foundation work after the audit

The fresh signed Ubuntu index invalidated the cached no-upgrade plan. The first attempt stopped before installation and preserved `blocked.json` and both plans under `.local/validation/nr-foundation-20260912`. A subsequent explicit authorization allowed only six dependency upgrades (`gcc-14-base`, `libgcc-s1`, `libstdc++6`, `libc6`, `libc-bin`, `locales`) plus 39 new packages and no removals. The accepted plan totalled 87,897,678 download bytes and 313,235,456 installed bytes summed across all selected packages. Installation succeeded; before/after `dpkg --audit` output was empty. GCC/G++ 13.3.0, CMake 3.28.3 and Make 4.3 are now available in Ubuntu. No unrelated running computation was observed or stopped; the process receipt includes boot services and an idle login shell.

The source checkout is pinned to the commit above and Kokkos `6739bc623081648af9e752b616d9671527922cbf`. The clean checkout inventory covers 2,084 tracked files / 22,566,788 bytes, with inventory SHA256 `a84bbe3f209c1d4b9104b8e9a8178bf5d953645d49ca2c4b6bc4583ec88509f9`. The private directory is `/root/phaseforge-nr-c5a0d7f9155a70149931bf0be5a4ffb673f2532a`. Serial double-precision configuration for `PROBLEM=z4c/z4c_gauge_wave` and compilation succeeded. The build took 167.39 seconds, with 2,080,206,848 bytes peak sampled process-group RSS. Linux enforced 8 GiB address space per process; the additional aggregate RSS guard sampled every 250 ms and conservatively counted shared pages. Neither limit fired. The 12,532,128-byte binary has SHA256 `35436a3c98727aee96b004de28cd63cb7ff1a72522547d98390603654b542a05`.

The frozen harmonic-gauge input `tools/athenak_gauge_wave.athinput` has SHA256 `38ee535294a3e5df3e2e9d2df09e85c73abfaf40b754275503af7dfd877c3d42`: 32 by 4 by 4 cells, amplitude 0.01, zero shift, RK4, final coordinate time 0.1. It ran for 13 cycles and exited zero in 0.162 seconds, retaining 1,458,400 bytes of native outputs/checkpoints in `gauge-native`. Initial checker/source hashes and error limits were recorded before execution in `gauge-admission.json`. Independent analytic checks passed, including the unchanged lapse, metric and extrinsic-curvature limits. The engine-reported final Hamiltonian-constraint maximum was `6.9221e-10`; that diagnostic is separate from the independent analytic comparison. Actual retained times were 0, 0.0546875 and 0.1, rather than assuming the requested output cadence was exact.

Two later authorized variants changed only both longitudinal cell counts to 16 and 64. Before either run, `refinement-preregistered/plan.json` (SHA256 `fbf7bb48930d8cb6e1aed5e9db9d3d020d548769ad1f8bf9ee6da75e8fedb9ab`) required the 32/64 cases to pass the original absolute limits and both successive L1 error ratios to exceed 2 for lapse, `gxx` and `Kxx`. The coarse 16 case was explicitly a diagnostic, with its original absolute pass/fail preserved.

| Longitudinal cells | Execution seconds | Retained bytes | Final lapse maximum error | Final gxx maximum error | Final Kxx maximum error | Original absolute check |
| --- | --- | --- | --- | --- | --- | --- |
| 16 | 0.0488 | 828,572 | 1.232e-5 | 7.460e-5 | 7.092e-4 | **Fails Kxx**, unchanged 5e-4 limit |
| 32 | 0.1616 | 1,458,400 | 3.089e-6 | 1.910e-5 | 1.838e-4 | Pass |
| 64 | 0.4605 | 2,717,986 | 7.799e-7 | 4.874e-6 | 4.629e-5 | Pass |

All six successive L1 error ratios were between 3.895 and 3.980, satisfying the preregistered refinement criterion. The original all-grid checker report remains `passed:false` because the 16-cell absolute failure is retained; it must not be relabeled as an all-grid absolute success. The plan-aware `refinement-evidence.json` records `passed:true`, independently checks all 36 retained files across the three grids, and has SHA256 `dadde7fdd7f09f32f8427d8eebfdc3a78208fbab22f2238a41020579a28b806a`. A decoder correction for inactive transverse axes landed near the first run, while these fixtures use four cells on both transverse axes. `component-evidence.json` (SHA256 `ab1ce8366b8c5a145afafdb9e12ea9784de9eb29bde2e5a2a9463b9c19abdba7`) records the exact admitted decoder bytes and proves that replaying the same saved data with admitted/current decoders produces identical complete reports. No tolerances, solver equations or gauge parameters were adjusted to these results.

The example represents flat spacetime in nontrivial coordinates; it is not physical gravitational radiation. These timings cannot be extrapolated to an AMR black-hole merger. This command-line foundation has not yet been integrated into PhaseForge's ordinary-chat job path. All native arrays, inputs, restart files, execution logs, source inventories and independent reports remain under `.local/validation/nr-foundation-20260912`, with original WSL run trees preserved. No further dependencies or physics studies were launched after these three cases.
