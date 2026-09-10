# PhaseForge 0.8.0 on Linux

PhaseForge's desktop architecture combines a native Rust engine, an exported
Next.js interface and Electron. Linux x64 and ARM64 package jobs are prepared in the
[desktop workflow](.github/workflows/desktop.yml), producing AppImage and Debian
packages. Neither Linux target has completed native acceptance for this overhaul.
Windows x64 is the active local test platform; prepared jobs are not evidence of
successful Linux execution. No release is published.

## Build prerequisites

- Current stable Rust/Cargo; the crate declares Rust 1.85 as its minimum.
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

The desktop starts its owned engine on `127.0.0.1:7331` and serves the bundled
interface on `127.0.0.1:7332`. A production Next.js server is unnecessary.
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
