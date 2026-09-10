# Third-party software

PhaseForge is designed around permissively licensed open-source components.
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
