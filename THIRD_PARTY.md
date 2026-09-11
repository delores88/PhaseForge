# Third-party software

PhaseForge uses open-source components, including bundled scientific runtimes and
optional separately installed rendering and engineering engines. Dependencies
retain their upstream licenses, including the notices shipped in Python wheels,
CPython distributions, Cargo dependencies, and npm packages.

Core components include Rust, Cargo, Tokio, Axum, Tower HTTP, wgpu, WGSL,
SQLite through rusqlite, React, Next.js, Three.js, and Lucide.

No Unreal Engine, MATLAB, proprietary solver, CUDA toolkit, or ROCm SDK is
required for the initial application. GPU execution uses native graphics and
compute drivers exposed through DirectX 12 or Vulkan via wgpu.

The desktop shell uses Electron and electron-builder (MIT). The procedural scene
engine uses Three.js (MIT); it does not impose engine royalties or per-seat
license fees. These packages include other dependencies and notices. The Windows
package carries Electron's bundled license and Chromium notices. Source lockfiles
pin the resolved versions. Model-provider API use is billed separately by the
chosen provider and is not an engine license.

The local high-quality renderer invokes a separately installed **Blender**
executable. Blender binary distributions are GPL-3.0-or-later; individual source
components have their own compatible licenses, including Cycles (Apache-2.0)
and Python (Python Software Foundation license). The installed Blender archive's
`copyright.txt` and `license/` directory contain its authoritative notices.
Blender and its Python modules are not copied into the PhaseForge desktop
package by this integration. Its original fixed worker,
[`tools/blender_render.py`](tools/blender_render.py), is dual-licensed under
`MIT OR GPL-3.0-or-later`; see [MIT](LICENSE) and the
[GPL license text](tools/BLENDER_WORKER_LICENSE.txt). Other PhaseForge source
retains the repository license unless a file says otherwise.

The interactive model viewer, mesh handling and effects use Three.js and its
official add-ons under the [MIT license](https://github.com/mrdoob/three.js/blob/dev/LICENSE).
Blender's license applies to Blender software and does not impose engine royalties
or that software license on rendered artwork and `.blend` output, as explained
in [Blender's license statement](https://www.blender.org/about/license/). Source
datasets and imported assets retain their respective provenance and licenses.

## Bundled Windows scientific runtimes

The Windows 0.9 package includes CPython 3.13.15 and NumPy 2.4.6 in
`resources/runtime/runtime-seeds/science-v2` and `python-numpy-v2`. The science
seed also contains OpenMM 8.5.2 and Pillow 12.3.0. These components are copied
into managed environments without changing their binary or license files.
The matching CPython source standard library replaces embedded bytecode, as
recorded in each seed's manifest. Generated-code isolation is a separate Windows
process boundary and does not change these licenses.

| Component | License and included notice location, relative to its seed |
| --- | --- |
| CPython 3.13.15 | PSF/Python license and bundled component terms: `LICENSE.txt`; matching source notices: `licenses/CPython-source-LICENSE.txt` |
| NumPy 2.4.6 | BSD-3-Clause and bundled component terms: `numpy-2.4.6.dist-info/licenses/LICENSE.txt`, plus all nested notices in that directory |
| OpenMM 8.5.2 | MIT for core, Reference/CPU and application layers; LGPL-3.0-or-later for bundled OpenCL platforms and XTC/XDR code, with additional component terms. Exact upstream notices accompany the application under `resources/tools/third-party/openmm-8.5.2/`, outside the frozen seed. |
| Pillow 12.3.0 | MIT-CMU and bundled image/font/codec terms: `pillow-12.3.0.dist-info/licenses/LICENSE` |

NumPy's aggregate notice includes OpenBLAS, LAPACK, and the GCC runtime's GPLv3
license with GCC Runtime Library Exception 3.1. CPython's aggregate notices
cover its bundled native dependencies. Pillow's aggregate notice retains its
image and font dependency notices. This software is based in part on the work
of the [FreeType Team](https://freetype.org/) and the
[Independent JPEG Group](https://ijg.org/).

OpenMM: portions copyright 2008–2026 Stanford University and the Authors.
Hilbert Curve implementation copyright 1998, Rice University.
OpenMM incorporates John Westbrook's PDBx/mmCIF reader under
[CC BY 3.0](https://creativecommons.org/licenses/by/3.0/), relocated by OpenMM
into `openmm.app.internal`. XTC/XDR code includes work by Erik Lindahl,
David van der Spoel, Frans van Hoesel, Accellera, and Sun Microsystems; its
copyright and license notices are retained in
[XTC-NOTICES.txt](tools/third-party/openmm-8.5.2/XTC-NOTICES.txt).

The [OpenMM notice and source record](tools/third-party/openmm-8.5.2/README.md)
maps the included native files to their upstream terms and identifies the
exact corresponding source commit. The matching source ZIP, notices and rebuild
instructions are required installer resources under
`resources/third-party-sources/openmm/`, outside ASAR. Full [OpenMM component notices](tools/third-party/openmm-8.5.2/Licenses.txt),
[LGPLv3](tools/third-party/openmm-8.5.2/LGPL.txt), and
[GPLv3](tools/third-party/openmm-8.5.2/GPL.txt) accompany the application.
The wheel's short metadata license field does not replace these component
licenses. OpenMM's LGPL components and their use remain covered by those terms;
PhaseForge does not restrict modification or reverse engineering for debugging
such modifications. A source rebuild may establish different runtime pins;
the shipped integrity checks are not a restriction on rights granted by a
dependency's license.

## Optional native CAD and PCB dependencies

These engines and their Python environments are installed separately; the desktop
package carries the trusted worker, pinned requirements and setup documentation,
not the engine binaries. The tested Windows x64 environment contains:

| Component | Tested version | License / authoritative notice |
| --- | --- | --- |
| CadQuery | 2.6.1 | Apache-2.0; installed distribution metadata and [project license](https://github.com/CadQuery/cadquery/blob/2.6.1/LICENSE) |
| OCP Python bindings | 7.8.1.1.post1 | Apache-2.0 for the bindings; [OCP license](https://github.com/CadQuery/OCP/blob/7.8.1.1/LICENSE). The bundled OpenCascade kernel retains its own terms below. |
| OpenCascade Technology | 7.8.1 | LGPL-2.1 with the [OCCT additional exception](https://github.com/Open-Cascade-SAS/OCCT/blob/V7_8_1/OCCT_LGPL_EXCEPTION.txt) |
| VTK | 9.3.1 | BSD-3-Clause; installed wheel `LICENSE` and bundled third-party notices |
| CasADi | 3.6.7 | LGPL-3.0-or-later; installed distribution metadata |
| NLopt | 2.11.0 | MIT; installed distribution metadata |
| Python jsonschema | 4.25.1 | MIT; installed distribution's `licenses/COPYING` |
| KiCad | 10.0.6 | GPL-3.0-or-later for the distribution, with separately licensed components listed in its `COPYRIGHT.txt`; [KiCad licenses](https://www.kicad.org/about/licenses/) |

The OCP wheel's metadata does not declare a license string, so its binding and
kernel entries above are taken from the upstream license files rather than
inferred from the wheel name. Transitive dependencies retain their own bundled
notices. KiCad library assets have separate terms; PhaseForge's test boards use
its generated local footprint library. See [native engine setup and
validation](docs/CAD_PCB_ENGINES.md) and [pinned requirements](tools/requirements-cad.txt).
