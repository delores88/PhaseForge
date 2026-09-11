# OpenMM 8.5.2: notices and source provenance

OpenMM's published Windows CPython 3.13 wheel is included unchanged in the
PhaseForge science runtime. Its SHA-256 is
`289c870e28c946b03992215f1f01f0b7d870f84a6f4c0a57e103bd8d62712b46`.
The installed `openmm/version.py` reports commit
`36a30cbca54e727b216b606f3c011b67201eb8b4`, which is also the upstream `8.5.2`
tag. Its short `METADATA` license field is incomplete for the bundled components.

The unmodified `Licenses.txt`, `LGPL.txt`, and `GPL.txt` here were fetched from
[`docs-source/licenses` at that exact commit](https://github.com/openmm/openmm/tree/36a30cbca54e727b216b606f3c011b67201eb8b4/docs-source/licenses).
Their complete byte contents match the upstream Git blob identifiers.
`XTC-NOTICES.txt` retains complete copyright/license comment blocks from the
same commit's XTC sources, including their embedded Sun RPC notice.
`provenance.json` records URLs, byte lengths, hashes, and the native file mapping.
These accompanying notices do not change the frozen runtime inventory.

## Included components

All paths below are relative to `science-v2`.

| Files | Upstream terms |
| --- | --- |
| `OpenMM.libs/lib/OpenMM.dll`, `OpenMMAmoeba.dll`, `OpenMMDrude.dll`, `OpenMMRPMD.dll` | MIT core/API, plus the component notices in `Licenses.txt` |
| `OpenMM.libs/lib/plugins/OpenMMCPU.dll`, `OpenMMPME.dll`, and `OpenMMAmoebaReference.dll`, `OpenMMDrudeReference.dll`, `OpenMMRPMDReference.dll` | MIT CPU/Reference implementations, plus the component notices in `Licenses.txt` |
| `OpenMM.libs/lib/plugins/OpenMMOpenCL.dll`, `OpenMMAmoebaOpenCL.dll`, `OpenMMDrudeOpenCL.dll`, `OpenMMRPMDOpenCL.dll` | LGPL-3.0-or-later OpenCL implementations, plus applicable component notices |
| `openmm/_openmm.cp313-win_amd64.pyd`, `openmm/app/internal/compiled.cp313-win_amd64.pyd` | MIT Python/API bindings, plus applicable component notices |
| `openmm/app/internal/xtc_utils.cp313-win_amd64.pyd` | LGPL-3.0-or-later XDR/XTC code; MIT Accellera wrapper; embedded Sun RPC terms, all retained in `XTC-NOTICES.txt` with the full LGPL/GPL texts |

The OpenMM aggregate notice also credits SFMT, Rice University's Hilbert Curve,
AsmJit, John Westbrook's PDBx/mmCIF reader (CC BY 3.0), irrXML, L-BFGS, and VkFFT.
No CUDA or HIP plugin wheel is included in these seeds. Host graphics drivers
and separate CUDA/HIP development kits are not distributed by this notice bundle.

## Source archive prepared for accompanying distribution

The complete upstream source archive is named
`openmm-8.5.2-36a30cbca54e727b216b606f3c011b67201eb8b4.zip`:

- Source: [immutable upstream archive](https://codeload.github.com/openmm/openmm/zip/36a30cbca54e727b216b606f3c011b67201eb8b4).
- Size: 23,152,521 bytes.
- SHA-256: `c6c604a769d6ceced546cb2d0d8af36a784a0506d51495023170d9fe3c6ccf1f`.
- All 2,255 Git blobs were checked against the immutable upstream tree; no
  Git submodules are omitted. The archive includes CMake files, Python wrapper
  sources, third-party source, license texts, and upstream compilation instructions.

The packaging pipeline requires this archive inside the installed application
at `resources/third-party-sources/openmm/`, outside ASAR, together with these
notices, `provenance.json`, `REBUILD.md`, and `delivery.json`. Staging and package
hooks verify the exact archive hash; installed-payload verification records the
same paths and hashes. A convenience source download does not replace this
required installer resource. These checks do not extract or recursively scan
the source ZIP for secrets and do not assert that an installer has already been
accepted or installed. Corresponding PhaseForge source/rebuild materials still
have to accompany the final release; the current dirty source tree must be
captured exactly before claiming that correspondence.
The concrete source delivery and recombination procedure is in
[REBUILD.md](REBUILD.md); its modified-library execution remains pending.

OpenMM's upstream Windows instructions are in
[`docs-source/usersguide/library/02_compiling.rst`](https://github.com/openmm/openmm/blob/36a30cbca54e727b216b606f3c011b67201eb8b4/docs-source/usersguide/library/02_compiling.rst#compiling-on-windows).
They describe Visual Studio, CMake, Python, SWIG/Cython, shared library and
Python-wrapper builds, and OpenCL prerequisites. Source verification here did
not compile or execute that source and does not establish a byte-identical
reproduction of the published wheel.
