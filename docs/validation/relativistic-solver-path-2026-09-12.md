# A real 0.999c black-hole collision: implementation path

Research and source inspection on 2026-09-12, followed by the separately
authorized CPU engine benchmark recorded below. The reference audit made no
backend change, paid model call or cloud computation. The requested equal-mass head-on collision
remains unfulfilled. Incoming-field approximations, an illustration, an existing
waveform replay, and a lower-speed collision do not fulfil it.

## Scientific target and current evidence

Interpret the requested speed as each hole moving at 0.999c in the asymptotic
centre-of-momentum frame. Direct evaluation gives gamma =
1/sqrt(1-0.999^2) = **22.3662720421**, with P/(m c) = **22.3439057701**.
This is not the coordinate speed of a puncture on an arbitrary curved slice.
The initial separation, boost definition, rest/irreducible mass convention,
spin assumptions and extraction horizon must be retained explicitly. Equal,
nonspinning, uncharged holes in vacuum with zero impact parameter are a useful
proposed specification, not additional user facts. Geometrized units can defer
the overall mass scale; converting to metres/seconds requires that scale.

Healy et al. use constraint-solved, non-conformally-flat Lorentz-boosted initial
data, an extended TwoPunctures implementation, LazEv BSSNOK/CCZ4 evolution,
Carpet refinement and horizon/waveform diagnostics. Although the abstract says
approximately 0.99c, Tables II/III actually tabulate gamma up to **4.1272** and
**4.1231**. Those tables cannot be cited as a gamma22.37 validation. Their
standard and approximate initial-data branches have different constraint
properties and must not be conflated.
[Healy et al., full paper](https://arxiv.org/pdf/1506.06153).

A newer April2026 preprint reports AthenaK Z4c encounters, including head-on
cases, at physical gamma up to **5.1**. It uses Bowen–York data with explicit
binding/junk-radiation calibration, so older apparent Bowen–York boost limits
must not be treated as absolute impossibility results. Crucially, it introduces
a telegrapher lapse driver: conventional gauges fail in its extreme comparison.
The full new gauge formulation is deferred to a paper listed as in preparation.
Its production configuration uses a216^3 root grid and nine refinement levels;
three resolutions test convergence. This is relevant progress, not evidence
that gamma22.37 already works. The reported high radiative efficiency at nonzero
impact parameter is not a prediction for this head-on request.
[Zhu, Pretorius and Stone, methods and supplement](https://arxiv.org/html/2604.26253v1).

## Concrete engine choices on this Windows machine

| Candidate | Actually available upstream | Practical route and remaining gap |
| --- | --- | --- |
| **AthenaK, preferred foundation** | Public Z4c evolution, Kokkos CPU/GPU execution, AMR, constraint and Weyl diagnostics; TwoPunctures and SpECTRE data interfaces. | Build a pinned Linux binary in the existing WSL2 Ubuntu, initially CPU. CUDA is an optional subsequent build, requiring its Linux toolkit. Current public standard/slow-start gauge is not the new extreme-boost method. |
| **Einstein Toolkit** | Public initial-data, evolution, refinement, horizon and waveform infrastructure; a documented real merger example. | Linux/WSL CPU build with C/C++/Fortran, MPI and numerical/I/O dependencies. Closest ecosystem to the older high-boost paper, but its extended initial data and in-house LazEv implementation are not established as a downloadable stock configuration. |
| **GRChombo** | Public C++14 MPI/OpenMP AMR code with a BinaryBH example and optional TwoPunctures initial data. | WSL CPU toolchain plus Chombo, Fortran support, MPI/HDF5, BLAS/LAPACK. Official Docker instructions exist but Docker is not currently established on this host. No verified gamma22.37 setup found. |
| **SpECTRE** | Public XCTS initial-data and generalized-harmonic framework, with Linux container/static tooling. | More dependencies, including Charm++, Boost, GSL and HDF5. Official prebuilt container coverage lists initial data, CCE and Python tooling; it is not a ready merger-evolution appliance. Useful as a later initial-data supplier, not the smallest first adapter. |

Primary implementation references:
[AthenaK requirements](https://ias-astrophysics.github.io/athenak-docs/requirements.html),
[AthenaK numerical relativity](https://ias-astrophysics.github.io/athenak-docs/numerical_relativity.html),
[Einstein Toolkit merger example](https://einsteintoolkit.org/gallery/bbh/index.html),
[LazEv owner description](https://ccrg.rit.edu/content/software/lazev),
[GRChombo prerequisites](https://raw.githubusercontent.com/wiki/GRTLCollaboration/GRChombo/Prerequisites.md),
[GRChombo BinaryBH build](https://github.com/GRTLCollaboration/GRChombo/blob/main/Examples/BinaryBH/README.md),
[SpECTRE installation and container limits](https://spectre-code.org/installation.html).

The parent's separate hardware audit measured Core Ultra9 185H,16cores/22logical
processors,31.42GiB RAM (about10.36GiB free), and an RTX4090 Laptop with16376MiB
VRAM (about12.55GiB free). C: had about96GiB free and D: about1907GiB. WSL2 and
a default Ubuntu24.04 distribution exist. Windows PATH did not expose Docker,
Podman, MPI, GCC or Fortran. Ubuntu's actual compiler/package/CUDA status is a
separate audit; host Windows compiler availability does not establish it.

NVIDIA documents Linux CUDA execution in WSL2 with the Windows GPU driver and a
separate Linux CUDA toolkit for compilation. It also documents memory/interoperability
limits. Do not install a Linux display driver or infer CUDA compilation support
from successful Windows nvidia-smi output.
[NVIDIA WSL guide](https://docs.nvidia.com/cuda/wsl-user-guide/index.html).

## Exact public source checked

AthenaK upstream commit **c5a0d7f9155a70149931bf0be5a4ffb673f2532a** was
resolved through GitHub's tree API. Eight selected source/input/build files were
downloaded without execution. Their byte hashes and immutable URLs are retained
in `.local/validation/nr-source-audit-20260912/receipt.json`, SHA256
`7096860c393b80d4dc4c36c9eb6c46664416565a3f154ea7ce46a8b8f0e2267d`.

The checked constructor, options and RHS expose standard/harmonic and slow-start
lapse controls. No telegrapher option or auxiliary lapse state was found in those
files. This is a bounded source finding, not a claim to have searched every
researcher's branch. The current documentation likewise exposes standard/SSL
controls. The exact initial-data problem path is
`z4c/two_punctures/z4c_two_puncture`; old tutorials using `z4c_two_puncture` alone
are stale relative to this tree.
[Pinned constructor](https://github.com/IAS-Astrophysics/athenak/blob/c5a0d7f9155a70149931bf0be5a4ffb673f2532a/src/z4c/z4c.cpp),
[pinned RHS](https://github.com/IAS-Astrophysics/athenak/blob/c5a0d7f9155a70149931bf0be5a4ffb673f2532a/src/z4c/z4c_calcrhs.cpp),
[pinned build](https://github.com/IAS-Astrophysics/athenak/blob/c5a0d7f9155a70149931bf0be5a4ffb673f2532a/CMakeLists.txt).

A follow-up search on **2026-09-12** checked the exact telegrapher paper title,
Zhu/Truong and Zhu/Pretorius/Stone author combinations, arXiv and public GitHub
results, the official repository's current branch names (two pages), and its
100 most recently updated pull requests. It found no identifiable later paper
or implementation of that specific gauge. The similarly named residual Z4c
outer-damping branch is not evidence of the telegrapher method. This search is
not proof that no newer implementation exists: private forks, unindexed work,
other repositories and unexamined branch contents remain outside its scope.
The April paper's “in preparation” reference alone is insufficient to establish
September availability. API search receipts are retained alongside the source
audit in `followup-search.json` and `followup-branches-page2.json`.

## Smallest authentic integration

1. **Provision one optional trusted WSL engine.** Pin AthenaK, its Kokkos
   submodule, compiler/build options, executable and dependency identities.
   A linear/gauge-wave or single-hole CPU build needs C++17, CMake and Kokkos;
   binary initial data adds independently built TwoPuncturesC and GSL. MPI is
   unnecessary for the first single-process fixture. Do not modify the bundled
   Python runtimes or let model-generated code invoke unrestricted WSL commands.
2. **Add a dedicated execution adapter.** Retain the exact parameter file,
   initial data, binary/source/dependency hashes, physical specification and
   diagnostic/output requests before dispatch. Launch only an allowlisted
   executable via a fixed argument vector. Preserve the current durable job ID,
   parent deadline/Off policy and immutable attempts. Record the Linux process
   identity and enforce its Linux process group/cgroup and resource limits.
   A Windows job containing wsl.exe is not proof that Linux descendants stop.
   Prove cancel, normal shutdown, interrupted launch and checkpoint restart.
3. **Decode real outputs through a trusted data adapter.** Retain original
   AthenaK binary mesh dumps, history and restart files with hashes. Decode
   bounded numeric headers/blocks using the pinned upstream format, never
   expressions, pickle or executable input. Keep refinement levels, cell
   coordinates, time, field definitions, puncture masks and coordinate/gauge
   conventions. Publish a small diagnostic slice and full raw evidence; do not
   pretend that a two-dimensional display is the evolved three-dimensional mesh.
4. **Expose measured fields and an honest status.** Initially show actual lapse,
   conformal factor, metric components and constraint norms. Add computed
   Weyl/extrinsic-curvature channels where retained. A warped sheet is not a
   physical embedding unless a separate defined calculation establishes it.
   Horizon surfaces must come from the horizon finder; coordinate tracks are
   not horizons. A future exact-case result must show the measured/calibrated
   boost, not merely the requested number.

AthenaK's native AMR output is `.bin`; `.hst` stores reductions and `.rst` holds
checkpoints. Legacy VTK supports unigrid only. Its slice output selects actual
cell centres, with possible differing slice locations between refinement levels.
Keep those coordinates rather than labelling an assembled plane as an exact
common slice. Restart parameter overrides are possible upstream; this app's
adapter must forbid unrecorded physics changes on resume.
[Official output formats](https://ias-astrophysics.github.io/athenak-docs/outputs.html).

Local integration points, inspected without editing:

- `backend/src/laboratory/catalog.rs`: executable catalog and capability gaps.
- `backend/src/laboratory/mod.rs:230`: durable solver start; dispatch at267 is
  currently restricted to the existing registered engines and science runtime.
- `backend/src/laboratory/api.rs:32`: explicit normal-API job admission; add an
  NR validator and per-engine execution identity rather than weakening defaults.
- `backend/src/laboratory/process.rs:27`: Windows supervision and a three-second
  cooperative stop window; neither is a sufficient WSL checkpoint protocol.
- `backend/src/laboratory/publication.rs`: bounded particles/scalar fields,
  not AMR/tensor data. Its scalar coordinate contract currently requires um.
  Add explicit geometrized coordinate/time units or a pinned physical conversion;
  never pass M-valued coordinates as micrometres.

This can remain an optional backend provider controlled by the packaged Windows
UI. It would be a new supported runtime boundary requiring its own installation,
provenance and lifecycle acceptance; the current installer cannot be described
as already containing it.

## Validation sequence and the exact-case barrier

First use analytic linear/gauge waves and a single Schwarzschild hole to check
the build, constraints, wave propagation, coordinates, horizon diagnostics and
restart equality. Then run a modest, constraint-solved equal-mass head-on merger
through common-horizon formation and ringdown at three resolutions. Label it
with its actual boost as an **engine benchmark**. Keep the0.999c request open.

For the exact case, choose and pin a reviewed high-boost initial-data/evolution
method: either obtain the actual extreme-gauge implementation and calibration
procedure, or implement and independently validate the non-conformally-flat
constraint-solving route. Code availability/licensing for the research-specific
parts is not established by this audit. An invented gauge based on its name or
abstract is not an acceptable substitute.

Measure the physical initial boost after separating binding/junk effects; do
not equate the input Bowen–York momentum to calibrated0.999c. Check constraints
outside the holes, local strong-field residuals, horizon masses/areas,
momentum symmetry, extraction-radius effects, and energy balance between ADM
input, escaping radiation and remnant. Preserve initial-data and evolution
refinement tests separately. Use at least three resolutions for the exact
gamma22.37 case, plus larger-domain/separation checks; a convergent low-speed
result cannot certify it. Freeze numerical error targets after a pilot and
before production comparisons; retain failures instead of widening tolerances
to accept a preferred picture.

## Resource estimate and uncertainty

No runtime estimate for this exact collision is defensible before a compiled
pilot measures grid allocation, timestep throughput and checkpoint size.
The public code's22 evolved doubles in three arrays, seven constraints and two
Weyl channels already give600bytes per allocated cell. At216^3 cells that is
about5.63GiB before ghost zones, ADM fields, mesh buffers, refinement or extra
gauge state. This is a calculation from inspected allocations, not measured
total memory. A small CPU benchmark is plausible within the current machine;
a resolved gamma22.37 computation is not established to fit its RAM or VRAM.

For scale only, the official Einstein Toolkit GW150914 example reports98GB RAM
and roughly8700core-hours. GRChombo's generic planning example suggests96–128
cores with4–6GB per core for about a day. Neither is a minimum or an estimate for
this symmetric head-on problem.
[ET resource table](https://einsteintoolkit.org/gallery/bbh/index.html),
[GRChombo planning guidance](https://github.com/GRTLCollaboration/GRChombo/wiki/Getting-started).

Planning allowances, not measurements: reserve roughly10–20GB for an isolated
CPU source/build/dependency workspace and more if adding CUDA. Limit the first
numerical pilot to an explicit few-GiB allocation and a short wall budget.
Measure actual outputs before selecting retention cadence. AMR and verified
head-on symmetries may reduce resources substantially, but require demonstrated
convergence. No GPU model name, fast renderer or larger context window supplies
the missing scientific validation. If production exceeds this machine, the
same immutable job/data contract can later target an already-authorized Linux
compute service; no purchase or remote allocation is assumed here.

## Subsequent real CPU foundation: measured and limited

After the reference audit, root authorized a bounded private WSL CPU build and
analytic gauge-wave run. AthenaK commit
`c5a0d7f9155a70149931bf0be5a4ffb673f2532a` and Kokkos gitlink
`6739bc623081648af9e752b616d9671527922cbf` were built with GCC 13.3,
CMake 3.28.3, double precision and serial CPU execution. The resulting Linux
binary SHA256 is
`35436a3c98727aee96b004de28cd63cb7ff1a72522547d98390603654b542a05`.
Build/source/dependency and process receipts are retained in
`.local/validation/nr-foundation-20260912/`. No GPU, MPI, GSL, TwoPunctures or
black-hole initial data was installed for this benchmark.

The frozen input is [tools/athenak_gauge_wave.athinput](../../tools/athenak_gauge_wave.athinput),
SHA256 `38ee535294a3e5df3e2e9d2df09e85c73abfaf40b754275503af7dfd877c3d42`.
It selects harmonic lapse and zero shift. Its exact solution is
`ds² = (1-H)(-dt²+dx²)+dy²+dz²`, with
`H=0.01 sin(2π(x-t))`, a unit periodic domain and `c=1`.
This is flat spacetime in changing coordinates; the moving metric is not
gravitational radiation. It is a useful test of actual Einstein-equation
evolution, coordinate interpretation, retained fields and independent checks.

The first 32×4×4 grid evolved through 13 steps to `t=0.1`, retaining real states
at `0`, `0.0546875` and `0.1`. The output cadence requests `0.05`, but the
intermediate saved time is the actual next solver step; the checker does not
invent evenly spaced timestamps. Native output records float32 values even
though the computation and coordinate locations use doubles.

Two subsequent immutable inputs changed only the mesh and meshblock x counts
to 16 and 64. Their limits and interpretation were preregistered before either
run in `refinement-preregistered/plan.json`, SHA256
`fbf7bb48930d8cb6e1aed5e9db9d3d020d548769ad1f8bf9ee6da75e8fedb9ab`.
Each grid retained its original absolute pass/fail. The refinement criterion
requires 32 and 64 to pass the original absolute thresholds and both doubling
pairs to reduce final L1 errors by at least a factor of two.

| x cells | Native execution seconds | Final lapse Linf error | Physical gxx Linf error | Physical Kxx Linf error | Original absolute checks |
| --- | ---: | ---: | ---: | ---: | --- |
| 16 | 0.0488 | 1.23192e-5 | 7.45984e-5 | 7.09179e-4 | **Fail:** Kxx exceeds 5e-4 |
| 32 | 0.1616 | 3.08935e-6 | 1.91027e-5 | 1.83817e-4 | Pass |
| 64 | 0.4605 | 7.79864e-7 | 4.87405e-6 | 4.62860e-5 | Pass |

The 16→32 L1 error ratios are 3.965, 3.895 and 3.915 for lapse, gxx and Kxx;
32→64 ratios are 3.979, 3.965 and 3.976. The preregistered refinement criterion
passes. The unmodified all-grid absolute report remains **false**, preserving
the coarse-grid failure. This is evidence consistent with second-order error
reduction for this fixture, not general strong-field convergence. Native
Hamiltonian constraint values are reported separately from independently
calculated analytic errors.

The independent [checker](../../tools/check_athenak_gauge.py) uses scalar
`math.fsum` and reconstructs physical spatial metric/extrinsic curvature from
saved conformal fields. The [decoder](../../tools/athenak_decode.py) checks
format version, sizes, indices, coordinates, duplicate blocks, finite values
and bounded allocation. It preserves raw mesh blocks and performs no
interpolation. Eleven focused tests cover analytic sign/phase, wrong gauge,
missing states, static data with changing time headers, malformed/truncated
files, nonfinite data, unsafe dimensions and coordinate preservation.

All 36 retained files across the three runs were independently checked against
their execution receipt hashes and lengths. Six baseline native frames were
also exported as NPZ and every channel/coordinate reloaded for exact equality.
The admission-time decoder was replayed from bytes verified against its original
SHA. A later fix for degenerate one-cell axes was made before inspecting data;
this fixture uses four cells on both transverse axes. Both complete checker
reports are identical. Original and current decoder identities are retained.

Final evidence, relative to `.local/validation/nr-foundation-20260912/`:

- `gauge-check-admitted.json`: exact admission-time analytic check.
- `component-evidence.json`, SHA256
  `ab1ce8366b8c5a145afafdb9e12ea9784de9eb29bde2e5a2a9463b9c19abdba7`:
  baseline file verification, decoder correspondence and NPZ equality.
- `refinement-all-grid-check.json`, SHA256
  `a81a9340a5ca3389f814c26b32377d624118725a5ac12c53c5fff3aff323ceca`:
  all three errors and the retained coarse absolute failure.
- `refinement-evidence.json`, SHA256
  `dadde7fdd7f09f32f8427d8eebfdc3a78208fbab22f2238a41020579a28b806a`:
  exact input/binary/file bindings and the preregistered acceptance decision.

This foundation has no application adapter or installed UI capability yet.
The next scientific requirements remain reviewed high-boost initial data and
gauge evolution, actual horizon/curvature diagnostics, calibrated physical
boost, constraint checks, and convergence for the requested collision. The
**0.999c equal-mass head-on collision is still unfulfilled**. The small CPU
timings above must not be extrapolated into a runtime promise for it.
