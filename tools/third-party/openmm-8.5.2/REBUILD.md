# OpenMM source and PhaseForge recombination materials

This is the source/rebuild plan for owner review before public redistribution.
The upstream archive has been verified; this modified-library rebuild procedure
has not been executed. It is not a claim of a bit-for-bit wheel reproduction or
completed redistribution acceptance.

## Distribution set

Provide these materials with the exact installer, without a paywall or a
request-only process. The OpenMM source ZIP is a required installer payload;
a separately hosted source asset does not replace it:

1. The verified OpenMM source archive named and hashed in `README.md`, including
   all its upstream build scripts and component sources, installed at
   `resources/third-party-sources/openmm/` outside ASAR. This directory also
   includes the notices, this rebuild document, provenance, and `delivery.json`.
2. The exact corresponding PhaseForge source snapshot, including the actual
   dirty-tree changes used for the binary until a clean release commit exists.
   Include Cargo/npm lockfiles, workers, runtime manifests, the seed builder,
   native staging scripts, and Windows build instructions. A link to an earlier
   commit does not establish correspondence with this checkpoint.
3. This notice directory and the original CPython/NumPy/Pillow distribution
   notices. Keep all notices inside a modified OpenMM distribution too.
4. A release receipt tying the installer, both source archives, runtime
   manifests, and build instructions to their SHA-256 hashes.

The staging helper verifies the cached source archive under
`.local/redistribution-sources/`, or fetches only the fixed upstream archive if
the cache is absent. It publishes the checked resource tree under
`desktop/runtime/third-party-sources/openmm`. Packaging verifies its exact bytes
before and after copying it into application resources; installed-payload
verification checks it again against the matching source materials receipt.
These checks do not execute source, establish a rebuild, or perform a recursive
inner-archive secret scan. Native installed acceptance and the exact application
source snapshot remain separate requirements before final release.

## Why the source build route matters

The supplied OpenCL plugins and XTC/XDR extension include LGPL-3.0-or-later
code. [LGPLv3 section 4](LGPL.txt) includes both a corresponding-source/application
code route and a suitable shared-library route. The stock PhaseForge binary
checks exact runtime files before use, so a user cannot simply replace a DLL
and assume that the stock executable will accept it. The distribution plan
therefore provides source and the ability to build a corresponding application
with a modified, interface-compatible library. It does not describe the stock
hash-pinned executable as an unrestricted drop-in-library mechanism.

No license permission is conditioned on retaining the original hashes. The
published source is editable. A user's source build can generate and compile its
own pins, while keeping generated experiment code behind the Windows isolation
boundary. This does not require accepting arbitrary libraries into the existing
shipped runtime or changing that runtime in place.

## Rebuilding and recombining in a separate source checkout

The following describes the actual source touchpoints; it is an unexecuted
procedure. Keep the released application and its evidence unchanged.

1. Verify and unpack the provided OpenMM source archive into a new build
   directory. Use the included Windows compilation instructions at
   `docs-source/usersguide/library/02_compiling.rst`. They identify the required
   Visual Studio/CMake, CPython, SWIG, Cython, NumPy and OpenCL development tools.
   Build tools are separate from the shipped isolated runtime.
2. Make the desired library changes in that source tree. Retain notices and
   identify the modifications. Configure the x64 shared-library build for
   CPython 3.13 using CMake; select the needed CPU/Reference/OpenCL and plugin
   targets. `CMAKE_INSTALL_PREFIX` and `PYTHON_EXECUTABLE` choose the independent
   installation prefix and build interpreter. Build and run the upstream tests.
3. Build the Python wheel using the upstream `PythonBdistWheel` target in
   `wrappers/python/CMakeLists.txt`. The build tree's Python setup script includes
   the native library distribution. Confirm that the result includes the
   intended `OpenMM.libs/lib` and plugin DLLs, wrappers, XTC extension, and notices.
   The modified library must expose the APIs used by the trusted workers.
4. In the separate PhaseForge source checkout, edit only its OpenMM source
   entry in `scripts/release/build_seeds.py` to identify the locally built wheel
   and its new SHA-256. Put that wheel in a separate offline cache at the name
   in the entry. Retain the other pinned archives. The `--offline` path reads
   those cached bytes; it does not need to fetch a new binary.
5. Generate new manifests with that source checkout's `build_seeds.build(...)`
   function using `freeze=True` and a new `manifest_root`. The function accepts
   explicit output, cache, offline, source_commit and manifest_root arguments.
   Retain the released manifests, then copy the new science manifest into that
   checkout's `tools/runtime-seeds/science-v5.manifest.json`. The generated
   Python/NumPy `python-numpy-v4` runtime is independent of OpenMM and should remain identical
   unless it was deliberately modified too.
6. In the same source checkout, update the OpenMM expected source SHA-256 in
   `backend/src/laboratory/runtime.rs::parse` to the new wheel hash. The exact
   file inventory is embedded from the new manifest at compile time. Keep path,
   link, byte-length, finite-number, cancellation, and process-boundary checks.
   No private signing key or external approval service controls these source
   constants.
7. Follow `README.windows.md` to build the backend/frontend and stage the new
   seed root using `PHASEFORGE_RUNTIME_SEED_ROOT`. Use a separate application
   data directory so the released managed environment remains unchanged. Run
   runtime and worker acceptance against this source build; record its new
   source, wheel, manifest and binary hashes. A modified result has its own
   scientific validation status.

Before public release, the owner must review the accompanying source set and
verify that these materials allow a modified compatible library to be used by
the corresponding application. The upstream source was not built during the
notice audit, and that verification is currently pending.
