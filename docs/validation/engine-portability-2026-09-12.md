# Engine portability: verified scope on 2026-09-12

The engine choices provide a reasonable foundation for a portable application.
They do **not** establish that PhaseForge currently works on all five requested
targets. The current scientific release and its managed runtime acceptance are
deliberately **Windows x64 only**. This audit does not change that release scope.
No new installation, cross-platform build, simulation or marketplace scan was
performed for this audit.

## Upstream availability versus application support

“Wheel” below means a published native CPython 3.13 package for the exact listed
version, not a successful PhaseForge test. “Source route” means a credible build
path that still needs compilation, dependency and numerical validation on that
target. ARM64 and aarch64 refer to the same architecture family here.

| Engine or dependency | Linux x64 | Linux ARM64 | Windows x64 | Windows ARM64 | macOS Apple Silicon |
| --- | --- | --- | --- | --- | --- |
| NumPy 2.4.6 and Pillow 12.3.0 | Wheels | Wheels | Wheels; used locally | Wheels | Wheels |
| PhaseForge heat and 2D fluid worker | CPU source route; untested | CPU source route; untested | Actual numerical benchmarks and retained fields tested | CPU source route; untested | CPU source route; untested |
| OpenMM 8.5.2 | Wheel | Wheel | Wheel; actual CPU/Reference runs tested | No native CPython 3.13 wheel in the checked release; source build unverified | Wheel |
| Blender 4.5.9 | Official binary | Documented source/dependency route; no official binary found in the checked 4.5 archive | Official binary; actual renders tested | Official binary | Official binary |
| AthenaK with Kokkos | Actual serial CPU gauge-wave build/run in Linux x64 under WSL2 | Kokkos ARM source route; AthenaK build untested | Actual Linux CPU component through WSL2; native Windows build untested | WSL2 exists for ARM64; AthenaK route untested | Kokkos CPU/compiler route; AthenaK build untested |

The exact NumPy/OpenMM wheel names, hashes and URLs were read from official
release metadata and retained in
`.local/validation/engine-portability-20260912/pypi-cp313-wheel-matrix.json`;
Pillow has a companion `pillow-cp313-wheel-matrix.json`. Linux wheels also impose
their tagged libc baseline: the checked OpenMM manylinux wheels specify 2.34.
These are availability receipts, not executed compatibility checks.
[NumPy release files](https://pypi.org/project/numpy/2.4.6/#files),
[OpenMM release files](https://pypi.org/project/OpenMM/8.5.2/#files),
[Pillow release metadata](https://pypi.org/pypi/pillow/12.3.0/json).

Blender's official archive contains the listed 4.5.9 packages. Its Linux build
documentation explicitly describes `linux_arm64` dependency builds, while
warning that building all dependencies is a platform-maintainer workflow.
This is not evidence that PhaseForge has a releasable Linux ARM64 renderer.
[Official 4.5 archive](https://download.blender.org/release/Blender4.5/),
[Linux source build](https://developer.blender.org/docs/handbook/building_blender/linux/).

Kokkos documents ARM CPU architectures and Apple/MSVC compiler paths; that is
dependency portability, not certification of every AthenaK problem generator
or dependency. Microsoft documents WSL2 on both Windows x64 and ARM64. Our
actual AthenaK result used only Ubuntu 24.04 x64, serial CPU and double-precision
evolution, with a harmonic gauge-wave refinement check. It is a separately
retained engine component, with **no PhaseForge job adapter or packaged AthenaK
runtime yet**. It does not fulfil the requested 0.999c black-hole collision.
[Kokkos configuration](https://kokkos.org/kokkos-core-wiki/get-started/configuration-guide.html),
[compiler requirements](https://kokkos.org/kokkos-core-wiki/get-started/requirements.html),
[AthenaK requirements](https://ias-astrophysics.github.io/athenak-docs/requirements.html),
[WSL architecture support](https://learn.microsoft.com/en-us/windows/wsl/install-manual),
[actual component evidence and scientific limits](relativistic-solver-path-2026-09-12.md).

## GPU support is a separate capability

The present heat/fluid implementation in `tools/continuum_worker.py` uses NumPy
CPU arrays and FFTs. An NVIDIA or Apple GPU does not accelerate it automatically.
OpenMM's CPU, Reference, OpenCL and optional CUDA/HIP platform availability
depends on its build, dependencies and drivers. PhaseForge currently accepts
Reference/CPU/CUDA/OpenCL platform requests in `tools/scientific_worker.py`;
this does not prove each is available or validated on every target. The current
upstream guide discusses OpenCL on macOS, not an OpenMM Metal platform. Its
latest-version installation guidance is not a claim that our pinned 8.5.2
package contains all latest optional extras.
[OpenMM installation/platform guide](https://docs.openmm.org/latest/userguide/application/01_getting_started.html).

Blender Cycles has vendor-specific GPU backends including Metal on macOS.
Kokkos lists CUDA, HIP and SYCL execution spaces along with CPU spaces; Metal
is not listed there. Blender's Apple GPU support therefore cannot be carried
over as an AthenaK capability. WSL CUDA also requires an appropriate Windows
driver and Linux build environment; our AthenaK CPU proof did not install or
test that path.
[Cycles GPU rendering](https://docs.blender.org/manual/en/4.5/render/cycles/gpu_rendering.html),
[Kokkos execution spaces](https://kokkos.org/kokkos-core-wiki/API/core/execution_spaces.html),
[NVIDIA WSL guide](https://docs.nvidia.com/cuda/wsl-user-guide/index.html).

## Concrete PhaseForge portability work still required

The repository already has cross-platform desktop scaffolding and Blender
discovery paths, but the scientific runtime has narrower admission rules:

- `.github/workflows/marketplace-candidates.yml:28` selects Windows x64 only.
  `scripts/stage-desktop.mjs:15` rejects Windows ARM64 for this scientific
  release. Other desktop build script names are not platform test evidence.
- `scripts/release/build_seeds.py:22` pins an amd64 embedded interpreter,
  Windows amd64 wheels, and Windows x64 SQLite/OpenSSL libraries. Every new
  OS/architecture needs its own compatible runtime artifacts and provenance;
  those DLLs cannot be relabelled as Linux, ARM64 or macOS resources.
- `backend/src/laboratory/runtime.rs:340` rejects non-Windows managed runtimes.
  The heat/fluid numerical source is portable in principle, but its current
  application launch still goes through that environment loader.
- `backend/src/laboratory/isolation.rs:36` requires the validated Windows LPAC
  boundary for generated code and disables an unrestricted fallback. Linux
  and macOS require their own tested filesystem, network and process boundary.
- `backend/src/laboratory/process.rs:48` currently ignores the memory limit on
  the non-Windows branch and lacks the Windows Job Object equivalent there.
  Process-tree cancellation, quit/restart and resource enforcement need actual
  target-specific work. The separate Unix Blender process-group helper does
  not establish these guarantees for every laboratory worker.
- `backend/src/studio/render.rs` already searches Linux and macOS Blender
  locations as well as Windows locations. Discovery is only one part of
  renderer packaging and execution acceptance.

The appropriate design is to keep scientific inputs, units, numerical outputs
and engine identity independent of the runtime launcher. Runtime capabilities
must separately record OS, CPU architecture, GPU backend, source/binary pins
and tested limits. Retain a portable CPU baseline where feasible, admit optional
acceleration only after its numerical and lifecycle checks, and show an
explicit unsupported capability when a target lacks a validated route.

The most concrete upstream gaps are native Windows ARM64 OpenMM packaging,
Linux ARM64 Blender packaging, and unverified AthenaK target/backend combinations.
The larger application gap is implementing and testing non-Windows runtime
provisioning, isolation and lifecycle management. These are engineering tasks
to plan explicitly; they are not reasons to replace real simulation with an
illustrative animation.
