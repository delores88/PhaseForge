# PhaseForge 0.8.0-alpha.1 on Linux

PhaseForge combines a native Rust engine, an exported interface and Electron.
The initial alpha targets Linux x64 AppImage. The preceding development revision
passed Linux x64 and ARM64 packaging in the [desktop workflow](.github/workflows/desktop.yml).
That packaging result does not establish native acceptance of the alpha candidate.
ARM64 and Debian packages remain engineering targets outside the initial listing;
the exact alpha AppImage requires native acceptance and marketplace review.

## Build prerequisites

- Current stable Rust/Cargo; the crate declares Rust 1.88 as its minimum.
- A native C/C++ toolchain, `pkg-config`, and development libraries for the Rust
  credential backend. Ubuntu CI includes `libdbus-1-dev` and `libssl-dev`.
- Node.js 22 and npm 10 or newer; Git with a checked-out commit.
- A graphical desktop, a Vulkan loader/vendor driver for GPU compute, and a working
  desktop Secret Service for provider credentials.
- Distribution-specific AppImage/deb dependencies. CI includes `libfuse2` and
  `libopenjp2-tools`; package names vary by distribution.

Use tools matching the intended architecture. Staging labels the runtime using
the native Node process architecture and checks the engine version; it does not
cross-compile the engine.

## Build and run the desktop

From the repository root:

```bash
cargo test --manifest-path backend/Cargo.toml --locked --all-targets
cargo build --manifest-path backend/Cargo.toml --locked --release
npm --prefix frontend ci --no-audit --no-fund
npm --prefix frontend run build
npm --prefix desktop ci --no-audit --no-fund
npm --prefix desktop test
node scripts/stage-desktop.mjs
npm --prefix desktop start
```

Package on a native x64 host with:

```bash
npm --prefix desktop run package:linux:x64
```

Use `package:linux:arm64` on ARM64. Artifacts are written to `desktop/dist/`;
packaging uses `--publish never`. These commands prepare local artifacts, not a
public release. Test installation, launch, graphics, credentials and uninstallation
on each actual target before distributing a package.

The desktop's owned engine binds an OS-assigned available loopback port, announced
through the private parent connection and verified with its launch secret.
The bundled interface retains `127.0.0.1:7332` for persistent preferences and
refuses an occupied interface port. Packaged execution does not use development
port 3000 or the old fixed engine port 7331. A production Next.js server is unnecessary.
Where a tray icon is supported, closing the window keeps research running; use
**Quit PhaseForge** to exit. Tray behavior still needs native Linux testing.

## Browser development mode

Run in separate terminals:

```bash
cargo run --manifest-path backend/Cargo.toml
```

```bash
npm --prefix frontend run dev -- --hostname 127.0.0.1 --port 3000
```

The client is at [127.0.0.1:3000](http://127.0.0.1:3000); inspect
[backend health](http://127.0.0.1:7331/api/health) for the engine version.
Existing `scripts/install-backend.sh`, `install-frontend.sh` and component start
scripts remain source-development helpers, separate from desktop packaging.

The engine accepts `--config PATH`, `--cpu-only`, `--version` and `--help`.
Start from [the example configuration](configs/phaseforge.example.toml).
`PHASEFORGE_CONFIG`, `PHASEFORGE_DATA_DIR` and `PHASEFORGE_DISABLE_GPU=1` are
also available. Stop older processes before launching a new engine; never run
concurrent backends against one data directory.

## Scientific and compute behavior

The runtime executes bounded CPU f64 ODE and classical particle models.
Compatible ODE search scoring can use an initialized Vulkan adapter through wgpu;
the selected candidate is replayed on CPU f64. Three.js renders procedural scenes
independently. A supplied streamline or molecular surface does not add a fluid
solver, force field or docking calculation.

Memory admission, bounded batches and generation checkpoints provide local recovery.
Standalone runs can resume completed generations under their original deadline.
Research sessions pause after an app restart and their owned solvers stop before
dispatch. Explicit resume reuses saved stages; a cancelled simulation restarts
from the immutable experiment's initial conditions.

Linux accelerator-class inventory is separate from initialized numerical engines.
NPU kernels, cluster/DGX orchestration, distributed multi-GPU execution and local
open-source LLM inference remain roadmap work. Model calls currently use configured
remote OpenAI/Anthropic APIs with per-turn selection and saved usage accounting.

## Optional engines and validation

Studio's Blender, CadQuery and KiCad integrations use separately installed native
programs; the desktop package does not contain them. Install an appropriate
[Blender build](https://www.blender.org/download/),
[KiCad package](https://www.kicad.org/download/) and compatible CAD Python environment.
`BLENDER_PATH`, `KICAD_CLI` and `PHASEFORGE_CAD_PYTHON` override executable discovery
when set before starting the app. KiCad export also needs Python with its `pcbnew`
bindings. See [CAD/PCB setup](docs/CAD_PCB_ENGINES.md) and
[Blender setup](docs/BLENDER_RENDERING.md) for discovery paths and job limits.

The pinned CAD requirements and native rendering/engineering checks were exercised
on Windows x64. Wheel availability, KiCad bindings, native process behavior and
Cycles GPU drivers need separate Linux x64/ARM64 validation. A discovered executable
does not establish that a particular job or GPU backend works on that target.

PDB, MOL/SDF V2000 and XYZ intake works without external chemistry engines.
OpenMM, GROMACS, CP2K, xTB, LAMMPS and Open Babel can be detected when their Python
environment or executables are available. Detection is not an executing adapter.
The independent ODE verifier uses Python 3.10+ and the standard library;
`scripts/install-verifier.sh` remains its optional setup helper.

Keep backups of the configured SQLite store and retain access to the credential
service during upgrades. Linux Secret Service, GPU drivers, application sandboxing,
AppImage compatibility and ARM64 dependencies require native checks. Do not disable
platform sandboxing to conceal a packaging failure.

See [validation](docs/RELEASE_VALIDATION.md), [architecture](docs/ARCHITECTURE.md),
[scientific scope](docs/SCIENTIFIC_SCOPE.md), [scenes](docs/SCIENTIFIC_SCENES.md),
[sessions](docs/research-sessions.md), [recovery](docs/COMPUTE_AND_RECOVERY.md), and
[verification methods](docs/VERIFICATION_METHODS.md).
