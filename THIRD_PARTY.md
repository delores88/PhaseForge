# Third-party software

PhaseForge uses open-source components, including optional separately installed
scientific and rendering engines with their own licenses.
The authoritative license for each dependency is the license shipped by that
project and resolved by Cargo or npm.

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
